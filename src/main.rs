use wgpu::util::DeviceExt;

fn main() {
    // Enable detailed logging for wgpu/naga
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,wgpu=debug,naga=trace"),
    )
    .init();
    pollster::block_on(run());
}

async fn run() {
    // Enable wgpu trace if set
    if let Ok(trace_path) = std::env::var("WGPU_TRACE") {
        println!(
            "WGPU trace enabled, output will be written to: {}",
            trace_path
        );
    }

    // Force DX12 on Windows to trigger the bug
    let backends = if cfg!(windows) {
        wgpu::Backends::DX12
    } else {
        wgpu::Backends::all()
    };

    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends,
        backend_options: wgpu::BackendOptions {
            #[cfg(target_os = "windows")]
            dx12: wgpu::Dx12BackendOptions {
                shader_compiler: wgpu::Dx12Compiler::StaticDxc,
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    });

    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await
        .expect("Failed to find adapter");

    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: None,
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: Default::default(),
            trace: Default::default(),
        })
        .await
        .expect("Failed to create device");

    println!("=== Workgroup Memory Race Condition Test (rust-gpu SPIR-V) ===");
    println!("Adapter: {}", adapter.get_info().name);
    println!("Backend: {:?}", adapter.get_info().backend);
    println!("Driver: {}", adapter.get_info().driver);
    println!();

    // Load the SPIR-V shader compiled by rust-gpu
    let shader_spirv = include_bytes!("../shader.spv");

    // Manually convert SPIR-V to see the intermediate HLSL
    if cfg!(windows) {
        println!("=== Attempting to dump HLSL output ===");

        // Parse SPIR-V using Naga
        let options = naga::front::spv::Options::default();
        let module = match naga::front::spv::parse_u8_slice(shader_spirv, &options) {
            Ok(module) => {
                println!("Successfully parsed SPIR-V module");
                module
            }
            Err(e) => {
                eprintln!("Failed to parse SPIR-V: {:?}", e);
                panic!("SPIR-V parsing failed");
            }
        };

        // Validate the module
        let mut validator = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        );
        let module_info = match validator.validate(&module) {
            Ok(info) => info,
            Err(e) => {
                eprintln!("Module validation failed: {:?}", e);
                panic!("Validation failed");
            }
        };

        // Generate HLSL
        let mut hlsl_string = String::new();
        let options = naga::back::hlsl::Options {
            shader_model: naga::back::hlsl::ShaderModel::V5_1,
            binding_map: Default::default(),
            fake_missing_bindings: false,
            special_constants_binding: None,
            push_constants_target: None,
            zero_initialize_workgroup_memory: true,
            dynamic_storage_buffer_offsets_targets: Default::default(),
            force_loop_bounding: false,
            restrict_indexing: false,
            sampler_heap_target: Default::default(),
            sampler_buffer_binding_map: Default::default(),
        };

        let pipeline_options = naga::back::hlsl::PipelineOptions {
            entry_point: Some((naga::ShaderStage::Compute, "main_cs".to_string())),
        };

        match naga::back::hlsl::Writer::new(&mut hlsl_string, &options, &pipeline_options).write(
            &module,
            &module_info,
            None,
        ) {
            Ok(_) => {
                println!("=== Generated HLSL ===");
                println!("{}", hlsl_string);
                println!("=== End HLSL ===");

                // Save to file for easier inspection
                std::fs::write("generated.hlsl", &hlsl_string).ok();
                println!("(Also saved to generated.hlsl)");
            }
            Err(e) => {
                eprintln!("Failed to generate HLSL: {:?}", e);
            }
        }

        println!();
    }

    // Convert bytes to u32 array for SPIR-V
    let spirv_data = wgpu::util::make_spirv(shader_spirv);
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("rust-gpu workgroup memory test"),
        source: spirv_data,
    });

    // Create input buffer with values 1..=64
    let input_data: Vec<u32> = (1..=64).collect();
    let input_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Input Buffer"),
        contents: bytemuck::cast_slice(&input_data),
        usage: wgpu::BufferUsages::STORAGE,
    });

    // Output buffer for the sum
    let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Output Buffer"),
        size: 4, // 1 u32
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Staging Buffer"),
        size: 4,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    // Create bind group
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Bind Group Layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Bind Group"),
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: input_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: output_buffer.as_entire_binding(),
            },
        ],
    });

    // Create compute pipeline
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Pipeline Layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("Compute Pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("main_cs"),
        compilation_options: Default::default(),
        cache: None,
    });

    // Execute compute pass
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Command Encoder"),
    });

    {
        let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Compute Pass"),
            timestamp_writes: None,
        });
        compute_pass.set_pipeline(&pipeline);
        compute_pass.set_bind_group(0, &bind_group, &[]);
        compute_pass.dispatch_workgroups(1, 1, 1);
    }

    // Copy result to staging buffer
    encoder.copy_buffer_to_buffer(&output_buffer, 0, &staging_buffer, 0, 4);

    queue.submit(std::iter::once(encoder.finish()));

    // Read result
    let buffer_slice = staging_buffer.slice(..);
    let (sender, receiver) = std::sync::mpsc::channel();
    buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
        sender.send(result).unwrap();
    });
    let _ = device.poll(wgpu::PollType::Wait);
    receiver.recv().unwrap().unwrap();

    let data = buffer_slice.get_mapped_range();
    let result = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);

    println!("Test: Parallel reduction sum of 1..=64");
    println!(
        "Using: rust-gpu compiled SPIR-V → wgpu → Naga → {}",
        if cfg!(windows) {
            "HLSL/DXC"
        } else {
            "Metal/Vulkan"
        }
    );
    println!();
    println!("Result: {}", result);
    println!("Expected: 2080 (sum of 1..64)");
    println!();

    if result != 2080 {
        println!("❌ FAIL: Got {} instead of 2080", result);
        println!();
        println!("This confirms the workgroup memory race condition bug in");
        println!("Naga's SPIR-V → HLSL translation on Windows/DX12.");
        std::process::exit(1);
    } else {
        println!("✅ PASS");
    }
}
