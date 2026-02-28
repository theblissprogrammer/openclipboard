# OpenClipboard

OpenClipboard is a **local-first, cross-device clipboard sync** MVP.

- **Goal:** copy on one device, paste on another.
- **Transport:** LAN-first (same Wi‑Fi), with proximity-style discovery.
- **Security:** explicit pairing + trust store; pairing shows a human-verifiable 6‑digit code.

> Supported in this repo today: **Android** and **macOS** MVP apps (plus shared Rust core + UniFFI bindings).

---

## Install (from GitHub Actions artifacts)

OpenClipboard is distributed as CI artifacts via the **Build Artifacts** workflow.

1. Open **Actions → Build Artifacts** in this repo.
2. Click the latest successful run (or run it via **Run workflow**).
3. Download the artifacts:
   - **`OpenClipboard-Android`** (contains one or more `*.apk`)
   - **`OpenClipboard-macOS`** (contains `OpenClipboard-macos.zip`)

### Android (APK)

1. Unzip the **`OpenClipboard-Android`** artifact.
2. Copy the APK to your Android device.
3. Install it:
   - If prompted, allow installing from this source ("Unknown apps").
4. Launch **OpenClipboard**.

### macOS (ZIP)

1. Unzip the **`OpenClipboard-macOS`** artifact.
2. Unzip `OpenClipboard-macos.zip`.
3. Move `OpenClipboard.app` into `/Applications` (optional, but recommended).
4. Open the app.
   - If macOS Gatekeeper blocks it, use **System Settings → Privacy & Security → Open Anyway**.

---

## Pairing two devices (MVP flow)

Pairing is currently done by exchanging short strings ("init" / "response") and confirming a **6‑digit code**.

On **Device A** (Initiator):
1. Open **Pair Device**.
2. Choose **Initiate**.
3. Copy the generated **Init string** and send it to Device B.

On **Device B** (Responder):
1. Open **Pair Device**.
2. Choose **Respond**.
3. Paste Device A’s **Init string**.
4. Tap **Generate**.
5. Copy the generated **Response string** and send it back to Device A.
6. Verify the **6‑digit confirmation code** matches Device A.
7. Tap **Confirm**.

Back on **Device A**:
1. Paste Device B’s **Response string**.
2. Tap **Derive Code**.
3. Verify the **6‑digit confirmation code** matches Device B.
4. Tap **Confirm**.

After confirmation, both devices add each other to their local trust store.

---

## Android: enable Background Sync

To keep the Android device listening for nearby peers and incoming clipboard updates:

1. Open the Android app.
2. Go to **Settings → Background Sync → Start**.
3. On Android 13+ (SDK 33+), the app may request **Notification** permission.
   - Denying it may hide the ongoing foreground notification, but can also make background behavior less reliable on some devices.

---

## Troubleshooting

- **Same Wi‑Fi / same network:** discovery and connections are LAN-first; verify both devices are on the same Wi‑Fi (and not isolated guest networks).
- **Discovery can take a moment:** allow ~10–30 seconds for devices to appear, especially after toggling Wi‑Fi.
- **Permissions:**
  - Android 13+: allow notification permission for the foreground-service notification.
  - Ensure the app is allowed to run in the background.
- **Battery optimization (Android):** disable battery optimization for OpenClipboard if the foreground service stops or discovery is flaky.
- **If pairing strings fail:** ensure you copied the full init/response string (no truncation/extra whitespace).

---

## Repo layout

- `core/` — Rust core (identity, crypto, framing, transport)
- `ffi/` — UniFFI component and bindings generation
- `android/` — Android app (Kotlin + Compose)
- `macos/` — macOS app (Swift)
- `docs/` — protocol and architecture docs

## License

MIT
