#!/bin/bash

set -e

echo "=== Workgroup Memory Bug Test Script ==="
echo

# Check if cargo-gpu is installed
if ! command -v cargo-gpu &> /dev/null; then
    echo "cargo-gpu not found. Installing..."
    cargo install --git https://github.com/rust-gpu/cargo-gpu cargo-gpu
fi

# Compile the shader
echo "Compiling shader to SPIR-V..."
cd shader
cargo gpu build --output-dir .. --auto-install-rust-toolchain --force-overwrite-lockfiles-v4-to-v3
cd ..

# Check if shader was compiled
if [ ! -f "shader.spv" ]; then
    echo "Error: shader.spv not found after compilation"
    exit 1
fi

echo "Shader compiled successfully!"
echo

# Run the test
echo "Running workgroup memory test..."
# Set trace environment to dump shader
RUST_LOG=trace WGPU_TRACE=trace.txt cargo run --release

# Check if HLSL was generated on Windows
if [ -f "generated.hlsl" ]; then
    echo
    echo "=== Generated HLSL saved to generated.hlsl ==="
    echo "First 50 lines:"
    head -n 50 generated.hlsl
fi

# Clean up if requested
if [ "$1" = "--clean" ]; then
    echo
    echo "Cleaning up compiled shader..."
    rm -f shader.spv
fi