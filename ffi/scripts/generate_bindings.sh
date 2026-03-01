#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
UDL="$ROOT_DIR/ffi/src/openclipboard.udl"
OUT_KOTLIN="$ROOT_DIR/ffi/bindings/kotlin"
OUT_SWIFT="$ROOT_DIR/ffi/bindings/swift"

# Build the cdylib so uniffi can pick the correct cdylib name.
# Set OPENCLIPBOARD_BINDINGS_PROFILE=release to generate against release builds.
PROFILE="${OPENCLIPBOARD_BINDINGS_PROFILE:-debug}"
source "$HOME/.cargo/env"
cd "$ROOT_DIR"

CARGO_FLAGS=()
if [[ "$PROFILE" == "release" ]]; then
  CARGO_FLAGS+=(--release)
fi

cargo build -p openclipboard_ffi ${CARGO_FLAGS[@]+"${CARGO_FLAGS[@]}"}

# Locate the built shared library (platform-specific). If not found, continue without it.
LIB=""
for candidate in \
  "$ROOT_DIR/target/$PROFILE/libopenclipboard_ffi.so" \
  "$ROOT_DIR/target/$PROFILE/libopenclipboard_ffi.dylib" \
  "$ROOT_DIR/target/$PROFILE/openclipboard_ffi.dll"; do
  if [[ -f "$candidate" ]]; then
    LIB="$candidate"
    break
  fi
done

rm -rf "$ROOT_DIR/ffi/bindings"
mkdir -p "$OUT_KOTLIN" "$OUT_SWIFT"

ARGS=()
if [[ -n "$LIB" ]]; then
  ARGS+=(--library "$LIB")
else
  echo "Warning: could not locate libopenclipboard_ffi.so; generating bindings without library metadata" >&2
fi

# Kotlin bindings
cargo run -p openclipboard_ffi --bin openclipboard_bindgen -- \
  --language kotlin \
  --udl "$UDL" \
  --out "$OUT_KOTLIN" \
  "${ARGS[@]}"

# Swift bindings
cargo run -p openclipboard_ffi --bin openclipboard_bindgen -- \
  --language swift \
  --udl "$UDL" \
  --out "$OUT_SWIFT" \
  "${ARGS[@]}"

echo "Bindings generated under ffi/bindings/"
