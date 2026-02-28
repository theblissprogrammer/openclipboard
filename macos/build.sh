#!/usr/bin/env bash
set -euo pipefail

# macos/build.sh
#
# Local packaging helper for macOS.
#
# This script mirrors the macOS portion of the GitHub Actions workflow:
#   .github/workflows/build-artifacts.yml
#
# Notes:
# - Code signing + notarization are intentionally out of scope here.
# - For official distributable artifacts, prefer the GitHub Actions build
#   artifacts produced by the workflow above.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MACOS_DIR="$ROOT_DIR/macos"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "Error: macos/build.sh must be run on macOS (Darwin)." >&2
  exit 1
fi

cd "$ROOT_DIR"

echo "Building OpenClipboard macOS app bundle (unsigned)…"

echo "Step 1: Build Rust FFI library (release)…"
cargo build -p openclipboard_ffi --release

echo "Step 2: Generate Swift bindings and sync into macos/…"
export OPENCLIPBOARD_BINDINGS_PROFILE=release
"$ROOT_DIR/ffi/scripts/generate_bindings.sh"
"$MACOS_DIR/sync_bindings.sh"

echo "Step 3: Build Swift package (release)…"
cd "$MACOS_DIR"
export OPENCLIPBOARD_FFI_LIB_DIR="../target/release"
export DYLD_LIBRARY_PATH="$PWD/../target/release"
swift build -c release

echo "Step 4: Create .app bundle structure…"
APP_DIR="$MACOS_DIR/OpenClipboard.app"
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Frameworks" "$APP_DIR/Contents/Resources"

cp .build/release/OpenClipboard "$APP_DIR/Contents/MacOS/"
cp ../target/release/libopenclipboard_ffi.dylib "$APP_DIR/Contents/Frameworks/"

cat > "$APP_DIR/Contents/Info.plist" << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDisplayName</key>
    <string>OpenClipboard</string>
    <key>CFBundleExecutable</key>
    <string>OpenClipboard</string>
    <key>CFBundleIdentifier</key>
    <string>com.openclipboard.macos</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>OpenClipboard</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>1.0.0</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>LSMinimumSystemVersion</key>
    <string>13.0</string>
    <key>LSUIElement</key>
    <true/>
</dict>
</plist>
EOF

echo "Step 5: Package ZIP…"
ZIP_NAME="OpenClipboard-macos.zip"
rm -f "$MACOS_DIR/$ZIP_NAME"
cd "$MACOS_DIR"
zip -r "$ZIP_NAME" "OpenClipboard.app" >/dev/null

echo "Done."
echo "  App bundle: $APP_DIR"
echo "  ZIP:        $MACOS_DIR/$ZIP_NAME"
