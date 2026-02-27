#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
UDL="$ROOT_DIR/ffi/src/openclipboard.udl"
OUT_KOTLIN="$ROOT_DIR/ffi/bindings/kotlin"
OUT_SWIFT="$ROOT_DIR/ffi/bindings/swift"

# Build the cdylib so uniffi can pick the correct cdylib name.
source "$HOME/.cargo/env"
cd "$ROOT_DIR"

cargo build -p openclipboard_ffi

# Locate the built shared library (linux .so). If not found, continue without it.
LIB=""
if [[ -f "$ROOT_DIR/target/debug/libopenclipboard_ffi.so" ]]; then
  LIB="$ROOT_DIR/target/debug/libopenclipboard_ffi.so"
elif [[ -f "$ROOT_DIR/target/release/libopenclipboard_ffi.so" ]]; then
  LIB="$ROOT_DIR/target/release/libopenclipboard_ffi.so"
fi

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
