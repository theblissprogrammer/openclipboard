#!/usr/bin/env bash
set -euo pipefail

# Rebuild everything (Rust core + FFI, regenerate bindings, Android APK, macOS app)
# Usage:
#   ./scripts/rebuild_all.sh                # debug builds
#   OPENCLIPBOARD_PROFILE=release ./scripts/rebuild_all.sh
#   SKIP_ANDROID=1 ./scripts/rebuild_all.sh
#   SKIP_MACOS=1 ./scripts/rebuild_all.sh
#
# Notes:
# - Android build requires ANDROID SDK/NDK configured (local.properties or ANDROID_HOME).
# - macOS build requires running on macOS with Swift toolchain.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROFILE="${OPENCLIPBOARD_PROFILE:-debug}"

SKIP_ANDROID="${SKIP_ANDROID:-0}"
SKIP_MACOS="${SKIP_MACOS:-0}"

cd "$ROOT_DIR"

echo "==> Rebuild Rust (profile=$PROFILE)"
CARGO_FLAGS=()
if [[ "$PROFILE" == "release" ]]; then
  CARGO_FLAGS+=(--release)
fi

cargo build "${CARGO_FLAGS[@]+"${CARGO_FLAGS[@]}"}"
cargo build -p openclipboard_ffi "${CARGO_FLAGS[@]+"${CARGO_FLAGS[@]}"}"

echo "==> Regenerate UniFFI bindings (Swift + Kotlin)"
OPENCLIPBOARD_BINDINGS_PROFILE="$PROFILE" ./ffi/scripts/generate_bindings.sh

echo "==> Android build"
if [[ "$SKIP_ANDROID" == "1" ]]; then
  echo "    SKIP_ANDROID=1; skipping"
else
  (cd android && ./gradlew assembleDebug)
  echo "    APK: android/app/build/outputs/apk/debug/app-debug.apk"
fi

echo "==> macOS build"
if [[ "$SKIP_MACOS" == "1" ]]; then
  echo "    SKIP_MACOS=1; skipping"
else
  if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "    Not on macOS (uname=$(uname -s)); skipping macOS build. Set SKIP_MACOS=1 to silence." >&2
  else
    (cd macos && swift build)
  fi
fi

echo "✅ Done"