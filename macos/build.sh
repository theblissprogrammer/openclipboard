#!/bin/bash
set -e

echo "Building OpenClipboard macOS app..."

# Step 1: Build the Rust FFI library
echo "Step 1: Building Rust FFI library..."
cd ..
cargo build -p openclipboard_ffi --release --target x86_64-apple-darwin

# Step 2: Generate Swift bindings
echo "Step 2: Generating Swift bindings..."
cd ffi
cargo run --bin openclipboard_bindgen --release -- --language swift --out-dir bindings/swift

# Step 3: Build the Swift app
echo "Step 3: Building Swift app..."
cd ../macos
swift build -c release

echo "Build complete!"
echo "FFI library: ../target/x86_64-apple-darwin/release/libopenclipboard_ffi.dylib"
echo "Swift bindings: ../ffi/bindings/swift/"
echo "Swift app: .build/release/OpenClipboard"

# TODO: Package as .app bundle
echo ""
echo "TODO: Package as proper .app bundle with Info.plist, Resources, etc."
echo "TODO: Copy FFI library to app bundle"
echo "TODO: Sign and notarize for distribution"