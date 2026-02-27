#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FFI_DIR="$ROOT_DIR/ffi"
UDL="$FFI_DIR/src/openclipboard.udl"

if ! command -v uniffi-bindgen >/dev/null 2>&1; then
  echo "uniffi-bindgen not found; installing via cargo..." >&2
  cargo install uniffi_bindgen --version 0.29.5 --locked
fi

rm -rf "$FFI_DIR/bindings"
mkdir -p "$FFI_DIR/bindings/kotlin" "$FFI_DIR/bindings/swift"

uniffi-bindgen generate "$UDL" --language kotlin --out-dir "$FFI_DIR/bindings/kotlin"
uniffi-bindgen generate "$UDL" --language swift --out-dir "$FFI_DIR/bindings/swift"

echo "Bindings generated under ffi/bindings/"
