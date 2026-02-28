# OpenClipboard Android

Android client app for OpenClipboard, built with **Kotlin** + **Jetpack Compose**, backed by the shared Rust core via **UniFFI**.

## What’s included (MVP)

- **Rust FFI integration (UniFFI):** uses the generated Kotlin bindings from `ffi/`.
- **LAN discovery:** shows nearby peers on the local network.
- **Pairing UI (v1):** initiator/responder flow using init/response strings + a 6‑digit confirmation code.
- **Trust store:** paired devices are stored locally and shown under paired devices.
- **Mesh clipboard sync:** clipboard changes can fan out to **all trusted peers** on the LAN.
- **Clipboard history (cross-device):** browse recent clipboard entries across all devices; tap to recall locally (no broadcast).
- **Foreground service (“Background Sync”):** keeps a listener running so the device can receive connections/updates while the app is not in the foreground.
- **Optional keyboard (IME):** "OpenClipboard (History)" keyboard for quick paste while typing.

## Prerequisites (local development)

- Android SDK (API 35 recommended)
- Java 17
- Android NDK (CI uses `26.1.10909125`)
- Rust toolchain

## Project structure

```
android/
├── app/
│   ├── src/main/
│   │   ├── kotlin/com/openclipboard/
│   │   └── jniLibs/
├── build.gradle.kts
└── settings.gradle.kts
```

## Build (local)

From the repo root:

1. **Build the Rust FFI libraries into Android `jniLibs/`:**

   ```bash
   cargo install cargo-ndk
   rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android

   # Ensure NDK_HOME points at your installed NDK
   # (CI uses $ANDROID_SDK_ROOT/ndk/26.1.10909125)
   cargo ndk \
     -t arm64-v8a -t armeabi-v7a -t x86_64 \
     -o android/app/src/main/jniLibs \
     build -p openclipboard_ffi --release
   ```

2. **Generate Kotlin bindings (UniFFI) and sync them into the Android project:**

   ```bash
   export OPENCLIPBOARD_BINDINGS_PROFILE=debug
   ./ffi/scripts/generate_bindings.sh
   ```

3. **Build the APK:**

   ```bash
   cd android
   ./gradlew assembleDebug
   ```

The debug APK is produced under `android/app/build/outputs/`.

## CI builds (APK artifact)

The workflow at `.github/workflows/build-artifacts.yml` produces an installable APK as an Actions artifact:

- Artifact name: **`OpenClipboard-Android`**
- Contains: `android/app/build/outputs/**/*.apk`

## Enable the OpenClipboard keyboard (IME)

OpenClipboard includes an optional **history-only keyboard** that can paste from your synced clipboard history in any app.

1. Open **Android Settings → System → Keyboard → On-screen keyboard → Manage on-screen keyboards**
2. Enable **OpenClipboard (History)**
3. In any app, switch keyboards and select **OpenClipboard (History)**
4. Tap an entry to paste. Long-press copies the entry back to the system clipboard.

> Note: for the MVP, the IME starts the core if needed when the keyboard is shown.

## Permissions / runtime notes

- The app uses a **foreground service** for Background Sync.
- On Android 13+ (SDK 33+), starting Background Sync may prompt for **Notification** permission (for the ongoing notification).
- If background behavior is unreliable, disable battery optimization for OpenClipboard.

### Identity + trust store files

Android stores `identity.json` and `trust.json` in the app’s private storage (`context.filesDir`). If you **reinstall** the APK or **clear app storage**, those files are wiped and you’ll need to generate a new identity and re-pair.
