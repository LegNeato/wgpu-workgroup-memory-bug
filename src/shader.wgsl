// Minimal reproduction of the workgroup memory race condition bug
// Expected: Each thread writes its ID + 1000, then we verify all values
// Actual on Windows/DX12: Values get overwritten with zeros during initialization

@group(0) @binding(0)
var<storage, read_write> output: array<u32>;

var<workgroup> shared_mem: array<u32, 64>;

@compute @workgroup_size(64)
fn main(@builtin(local_invocation_id) local_id: vec3<u32>) {
    let tid = local_id.x;
    
    // CRITICAL: Each thread immediately writes a non-zero value
    // Using tid + 1000 to make corruption more obvious (zeros vs 1000+)
    shared_mem[tid] = tid + 1000u;
    
    // Barrier to ensure all writes complete
    workgroupBarrier();
    
    // Check multiple values to increase chance of detecting corruption
    if (tid == 0u) {
        // Check 8 different thread values
        output[0] = shared_mem[0];   // Should be 1000
        output[1] = shared_mem[1];   // Should be 1001
        output[2] = shared_mem[31];  // Should be 1031
        output[3] = shared_mem[32];  // Should be 1032
        output[4] = shared_mem[47];  // Should be 1047
        output[5] = shared_mem[48];  // Should be 1048
        output[6] = shared_mem[62];  // Should be 1062
        output[7] = shared_mem[63];  // Should be 1063
    }
}