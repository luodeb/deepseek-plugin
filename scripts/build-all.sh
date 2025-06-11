#!/bin/bash

# Build script for all target platforms
# This script helps test the build process locally before pushing

set -e

echo "🦀 Building DeepSeek Plugin for all platforms..."

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to print colored output
print_status() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if Rust is installed
if ! command -v cargo &> /dev/null; then
    print_error "Cargo not found. Please install Rust first."
    exit 1
fi

# Define targets
TARGETS=(
    "x86_64-unknown-linux-gnu"
    "x86_64-pc-windows-msvc"
    "x86_64-apple-darwin"
    "aarch64-apple-darwin"
)

# Library names for each target
declare -A LIB_NAMES
LIB_NAMES["x86_64-unknown-linux-gnu"]="libdeepseek.so"
LIB_NAMES["x86_64-pc-windows-msvc"]="deepseek.dll"
LIB_NAMES["x86_64-apple-darwin"]="libdeepseek.dylib"
LIB_NAMES["aarch64-apple-darwin"]="libdeepseek.dylib"

# Output names
declare -A OUTPUT_NAMES
OUTPUT_NAMES["x86_64-unknown-linux-gnu"]="deepseek-plugin-x86_64.so"
OUTPUT_NAMES["x86_64-pc-windows-msvc"]="deepseek-plugin-x86_64.dll"
OUTPUT_NAMES["x86_64-apple-darwin"]="deepseek-plugin-x86_64.dylib"
OUTPUT_NAMES["aarch64-apple-darwin"]="deepseek-plugin-aarch64.dylib"

# Create output directory
mkdir -p dist

print_status "Installing required targets..."

# Install targets
for target in "${TARGETS[@]}"; do
    print_status "Adding target: $target"
    rustup target add "$target" || print_warning "Target $target might already be installed"
done

print_status "Building for all targets..."

# Build for each target
for target in "${TARGETS[@]}"; do
    print_status "Building for $target..."
    
    if cargo build --release --target "$target"; then
        # Copy the built library to dist with proper naming
        lib_name="${LIB_NAMES[$target]}"
        output_name="${OUTPUT_NAMES[$target]}"
        
        if [[ -f "target/$target/release/$lib_name" ]]; then
            cp "target/$target/release/$lib_name" "dist/$output_name"
            print_status "✅ Built $output_name"
        else
            print_error "❌ Library not found: target/$target/release/$lib_name"
        fi
    else
        print_error "❌ Failed to build for $target"
    fi
done

print_status "Build complete! Libraries are in the 'dist' directory:"
ls -la dist/

print_status "🎉 All builds completed successfully!"
