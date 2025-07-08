#!/bin/bash
# Script to set up the repository

echo "Initializing git repository..."
git init

echo "Adding files..."
git add .

echo "Creating initial commit..."
git commit -m "Initial commit: Demonstrate wgpu workgroup memory bug on Windows"

echo ""
echo "Repository created successfully!"
echo ""
echo "To push to GitHub:"
echo "1. Create a new repository on GitHub (e.g., 'wgpu-workgroup-memory-bug')"
echo "2. Run: git remote add origin https://github.com/YOUR_USERNAME/wgpu-workgroup-memory-bug.git"
echo "3. Run: git push -u origin main"
echo ""
echo "The CI will automatically run and show the test failing on Windows but passing on Linux/macOS."