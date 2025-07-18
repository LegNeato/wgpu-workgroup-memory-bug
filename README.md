# wgpu Workgroup Memory Bug on Windows

This repository demonstrates a bug in wgpu/Naga's DX12/HLSL backend on windows.

## The Problem

When running a simple compute shader that:

1. Has 64 threads in a workgroup
2. Each thread writes its index+1 to shared memory
3. Synchronizes with `workgroupBarrier()`
4. Thread 0 sums all values

The expected result is 2080 (sum of 1 through 64), but on Windows with DX12 backend, the
result is 3.

## Test Results

- ✅ **Linux** (Vulkan): 2080 (correct)
- ✅ **macOS** (Metal): 2080 (correct)
- ❌ **Windows** (DX12): 3 (incorrect)

## The Bug

The value 3 (which is 1 + 2) suggests that only threads 0 and 1 are properly
synchronized or that the workgroup size is incorrectly set to 2 instead of 64.

## Running the Test

### Prerequisites

The test requires `cargo-gpu` to compile the Rust shader to SPIR-V. The script will
install it automatically if not present.

### Run Test

```bash
./run_test.sh
```

This script will:

1. Install cargo-gpu if needed
2. Compile the rust-gpu shader to SPIR-V
3. Run the wgpu test program
4. Report whether the test passes or fails
5. Output logs

### Clean Build Artifacts

```bash
./run_test.sh --clean
```

## CI Status

Check the GitHub Actions results to see the test failing on Windows but passing on Linux
and macOS. The HLSL should be uploaded as an artifact and it is also echoed in the logs.

When the same shader runs through Vulkan on Windows (e.g., using SwiftShader), it
produces the correct result, confirming the issue is in the DX12/HLSL path.
