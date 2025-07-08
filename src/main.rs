fn main() {
    env_logger::init();
    pollster::block_on(run());
}

async fn run() {
    // Force DX12 on Windows to trigger the bug
    let backends = if cfg!(windows) {
        wgpu::Backends::DX12
    } else {
        wgpu::Backends::all()
    };

    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends,
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

    println!("=== Minimal Workgroup Memory Race Condition Test ===");
    println!("Adapter: {}", adapter.get_info().name);
    println!("Backend: {:?}", adapter.get_info().backend);
    println!();

    // Minimal shader that demonstrates the race condition
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Minimal Repro"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
    });

    // Output buffer to read results
    let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Output"),
        size: 32, // 8 u32 values
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Staging"),
        size: 32,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    // Bind group
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Bind Group Layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Bind Group"),
        layout: &bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: output_buffer.as_entire_binding(),
        }],
    });

    // Pipeline
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Pipeline Layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("Pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });

    // Run the shader
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(1, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output_buffer, 0, &staging_buffer, 0, 32);
    queue.submit(std::iter::once(encoder.finish()));

    // Read results
    let slice = staging_buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    let _ = device.poll(wgpu::MaintainBase::Wait);
    rx.recv().unwrap().unwrap();

    let data = slice.get_mapped_range();
    let mut values = Vec::new();
    for i in 0..8 {
        let offset = i * 4;
        values.push(u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]));
    }

    println!("Test: Each thread writes (thread_id + 1000) to workgroup memory");
    println!("Expected: Non-zero values (1000, 1001, 1031, 1032, 1047, 1048, 1062, 1063)");
    println!();
    println!("Results:");

    let expected = [1000, 1001, 1031, 1032, 1047, 1048, 1062, 1063];
    let indices = [0, 1, 31, 32, 47, 48, 62, 63];
    let mut corrupted = false;

    for i in 0..8 {
        let is_correct = values[i] == expected[i];
        let status = if is_correct { "✓" } else { "✗ CORRUPTED" };
        println!(
            "  shared[{}] = {} (expected: {}) {}",
            indices[i], values[i], expected[i], status
        );
        if !is_correct {
            corrupted = true;
        }
    }

    println!();

    if corrupted {
        println!("❌ BUG DETECTED: Workgroup memory was corrupted!");
        println!();
        println!("Explanation: The race condition in Naga's HLSL backend caused");
        println!("thread 0's zero-initialization to overwrite values that other");
        println!("threads had already written to workgroup memory.");
        println!();
        println!("Notice how some or all values are 0 instead of 1000+");
        std::process::exit(1);
    } else {
        println!("✅ PASS: All values are correct");
    }
}
