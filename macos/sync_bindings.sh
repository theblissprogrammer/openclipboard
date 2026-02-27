#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

SWIFT_IN="$ROOT_DIR/ffi/bindings/swift"
SWIFT_OUT_BINDINGS="$ROOT_DIR/macos/OpenClipboardBindings"
SWIFT_OUT_FFI="$ROOT_DIR/macos/openclipboardFFI/include"

mkdir -p "$SWIFT_OUT_BINDINGS" "$SWIFT_OUT_FFI"

cp "$SWIFT_IN/openclipboard.swift" "$SWIFT_OUT_BINDINGS/openclipboard.swift"
cp "$SWIFT_IN/openclipboardFFI.h" "$SWIFT_OUT_FFI/openclipboardFFI.h"

echo "Synced Swift bindings into macos/"