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

## Pairing devices (MVP flow)

Pairing is currently done by exchanging short strings ("init" / "response") and confirming a **6‑digit code**.

> You can pair **more than 2 devices**. Once multiple devices are trusted, OpenClipboard runs in **mesh mode**: clipboard changes from any device fan out to all other trusted peers on the LAN.

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

## Clipboard History (cross-device)

Both apps keep a **clipboard history** that records clipboard text received from (and sent by) all paired devices.

- In the Android/macOS UI, you can open **Clipboard History**, tap an entry to **recall** it.
- **Recall is local-only**: selecting an entry copies it to your current clipboard **without broadcasting** it to other peers.

---

## Android: enable the OpenClipboard keyboard (history-only IME)

OpenClipboard ships with an optional **history-only keyboard** so you can paste synced clipboard history into any app.

1. Android **Settings → System → Keyboard → On-screen keyboard → Manage on-screen keyboards**
2. Enable **OpenClipboard (History)**
3. Switch keyboards in any text field and choose **OpenClipboard (History)**
4. Tap an entry to paste; long-press copies the entry back to the system clipboard.

## Android: enable Background Sync

To keep the Android device listening for nearby peers and incoming clipboard updates:

1. Open the Android app.
2. Go to **Settings → Background Sync → Start**.
3. On Android 13+ (SDK 33+), the app may request **Notification** permission.
   - Denying it may hide the ongoing foreground notification, but can also make background behavior less reliable on some devices.

---

## Troubleshooting

- **Same Wi‑Fi / same network:** discovery and connections are LAN-first; verify devices are on the same Wi‑Fi (and not isolated guest networks).
- **Discovery can take a moment:** allow ~10–30 seconds for devices to appear, especially after toggling Wi‑Fi.
- **Permissions:**
  - Android 13+: allow notification permission for the foreground-service notification.
  - Ensure the app is allowed to run in the background.
- **Battery optimization (Android):** disable battery optimization for OpenClipboard if the foreground service stops or discovery is flaky.
- **If pairing strings fail:** ensure you copied the full init/response string (no truncation/extra whitespace).
- **"No identity/trust store found" (Android):** those files live inside the app sandbox (`filesDir`). If you reinstall the APK or clear app storage, you will need to re-pair.

---

## Repo layout

- `core/` — Rust core (identity, crypto, framing, transport)
- `ffi/` — UniFFI component and bindings generation
- `android/` — Android app (Kotlin + Compose)
- `macos/` — macOS app (Swift)
- `docs/` — protocol and architecture docs

## License

MIT
