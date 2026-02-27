# OpenClipboard Android App

Android client app for OpenClipboard built with Kotlin and Jetpack Compose.

## Prerequisites

- Android SDK 34
- Java 17
- Android NDK (for building the Rust FFI library)
- Gradle 8.7+

## Project Structure

```
android/
├── app/
│   ├── src/main/
│   │   ├── AndroidManifest.xml
│   │   ├── kotlin/com/openclipboard/
│   │   │   ├── MainActivity.kt           # Main UI with Compose
│   │   │   ├── service/
│   │   │   │   └── ClipboardService.kt   # Background service
│   │   │   └── ui/theme/                 # Material3 theme
│   │   ├── res/                          # Android resources
│   │   └── jniLibs/                      # Native libraries (JNI)
│   └── build.gradle.kts                  # App module configuration
├── gradle/
│   └── libs.versions.toml                # Version catalog
├── build.gradle.kts                      # Root build configuration
└── settings.gradle.kts                   # Project settings
```

## Features

- **Home Screen**: Shows peer ID, connection status, and recent activity
- **Peers Screen**: Manage trusted peers (add/remove)
- **Settings Screen**: Configuration options
- **Background Service**: Keeps OpenClipboard running to receive connections
- **Material3 UI**: Modern Android design with dynamic colors

## Building

### Method 1: Local Build

1. **Build Rust FFI libraries:**
   ```bash
   # Install cargo-ndk for cross-compilation
   cargo install cargo-ndk
   
   # Add Android targets
   rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
   
   # Cross-compile the FFI library
   cd ../
   cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -o android/app/src/main/jniLibs build -p openclipboard_ffi --release
   ```

2. **Generate Kotlin bindings:**
   ```bash
   cd ffi
   cargo run --bin openclipboard_bindgen --release -- --language kotlin --out-dir bindings/kotlin
   
   # Copy bindings to Android project
   cp -r bindings/kotlin/* ../android/app/src/main/kotlin/
   ```

3. **Build APK:**
   ```bash
   cd ../android
   ./gradlew assembleDebug
   ```

   The APK will be generated at `app/build/outputs/apk/debug/app-debug.apk`.

### Method 2: Using CI

The GitHub Actions workflow in `.github/workflows/build-artifacts.yml` automates the above steps.

## Integration Status

The app currently contains scaffold code with TODO comments for FFI integration:

- [ ] **ClipboardNode Integration**: Initialize and manage the FFI ClipboardNode
- [ ] **Event Handling**: Implement EventHandler callbacks for incoming data
- [ ] **Clipboard Access**: Read/write system clipboard
- [ ] **File Handling**: Send/receive files through the app
- [ ] **Trust Management**: UI for managing trusted peers
- [ ] **Background Service**: Full implementation with FFI integration

## Dependencies

- **AndroidX**: Core, Lifecycle, Activity Compose
- **Jetpack Compose**: UI framework with Material3
- **Navigation Compose**: Screen navigation
- **JNA**: Java Native Access for FFI integration

## Permissions

- `INTERNET`: Network communication
- `ACCESS_NETWORK_STATE`: Network state monitoring
- `FOREGROUND_SERVICE`: Background clipboard service

## TODO

1. Complete FFI integration with generated Kotlin bindings
2. Implement EventHandler callbacks in ClipboardService
3. Add proper launcher icon and app branding
4. Implement clipboard read/write functionality
5. Add peer discovery and pairing UI
6. File picker and file sharing integration
7. Notification management for incoming data
8. Settings persistence
9. Error handling and user feedback
10. Security: prevent backup of sensitive files