@group(0) @binding(0) var<storage, read> input_buffer: array<u32>;
@group(0) @binding(1) var<storage, read_write> output_buffer: array<u32>;

var<workgroup> shared_data: array<u32, 64>;

@compute @workgroup_size(64, 1, 1)
fn main(@builtin(local_invocation_id) local_id: vec3<u32>) {
    let tid = local_id.x;
    
    // Each thread writes its value to shared memory
    shared_data[tid] = input_buffer[tid];
    
    // Synchronize all threads in the workgroup
    workgroupBarrier();
    
    // Thread 0 sums all values
    if (tid == 0u) {
        var sum = 0u;
        for (var i = 0u; i < 64u; i = i + 1u) {
            sum = sum + shared_data[i];
        }
        output_buffer[0] = sum;
    }
}