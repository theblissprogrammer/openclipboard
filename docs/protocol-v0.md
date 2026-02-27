# openclipboard — Protocol v0 (Draft)

This document defines the first implementable version of the openclipboard protocol.

## Goals (v0)
- Pair two devices securely (QR/code)
- Discover devices via Bluetooth LE (advertise/scan) *(discovery only)*
- Transfer clipboard (text + PNG images) and files over **LAN QUIC**
- Support background-first operation (within OS limits)
- Leave room for later: relay fallback, transport switching, multi-path

## Non-goals (v0)
- NAT traversal / internet relay
- Full mid-transfer handoff between transports
- Audio/video streaming

---

## Terminology
- **Peer**: a device running openclipboard
- **PeerId**: stable identity key fingerprint (derived from identity public key)
- **Session**: a secure connection between two peers over a transport
- **Transport**: QUIC over LAN (v0)

---

## Identity & Pairing
### Identity keys
Each peer generates at install time:
- `identity_sk`: Ed25519 private key
- `identity_pk`: Ed25519 public key

`PeerId = trunc16(sha256(identity_pk))` (displayed as hex/base32)

### Pairing
Pairing is required once per peer pair.

**Preferred flow (v0):** Mac shows QR, Android scans.

Pairing payload embedded in QR:
```json
{
  "v": 0,
  "peerId": "...",
  "name": "MacBook Pro",
  "identityPk": "base64...",
  "lan": {"port": 18455},
  "ts": 0,
  "nonce": "base64..."
}
```

Android responds by displaying a 6-digit code derived from:
`code = trunc6digits(sha256(nonce || androidPeerId || macPeerId))`

User confirms the same code on both devices.

After pairing, each peer stores a **TrustRecord**:
- trusted peerId
- trusted identity public key
- display name
- createdAt

---

## Discovery (BLE) — v0
BLE is used only to show “nearby devices” and initiate pairing/connection.

Advertisement payload (conceptual):
- service UUID: `OPENCLIPBOARD_UUID`
- peerIdHash: 8 bytes
- flags: 1 byte
- optional: lanPort (2 bytes)

Notes:
- Privacy later: rotate ephemeral IDs

---

## Transport & Security
### Transport
- QUIC over LAN
- One peer listens on `0.0.0.0:18455` by default

### Authenticated session
Use a Noise-style handshake over QUIC stream 0:
- `IK` pattern when peers are paired (both know identity keys)
- derive session keys, then all frames are encrypted/authenticated

(Implementation may use an existing Noise library or libsodium-based equivalent.
Exact crypto library is an implementation detail as long as it provides mutual auth and forward secrecy.)

---

## Multiplexing & Frames
All application data is sent as frames on a single QUIC stream initially (simple v0),
with a future path to multi-stream QUIC.

Frame header (binary):
- `u8  version` (0)
- `u8  msgType`
- `u32 streamId` (logical stream)
- `u64 seq`
- `u32 len`
- `bytes[len] payload`

Logical streamIds:
- `1` control
- `2` clipboard
- `3` file

---

## Message types (v0)
### Control
- `HELLO` — announce peer info, capabilities
- `PING` / `PONG`

### Clipboard
- `CLIP_TEXT`
  - payload: `{ mime: "text/plain", text: "...", ts }`
- `CLIP_IMAGE`
  - payload: `{ mime: "image/png", width, height, bytes(base64), ts }`

### File transfer
- `FILE_OFFER`
  - payload: `{ fileId, name, size, mime, sha256 }`
- `FILE_ACCEPT` / `FILE_REJECT`
- `FILE_CHUNK`
  - payload: `{ fileId, offset, bytes }`
- `FILE_DONE`

---

## Reliability & Ordering
- `seq` is monotonically increasing per session.
- Clipboard messages: keep last-write-wins semantics.
- File chunks: ordered by offset; sender can stream sequentially.

---

## OS behavior notes (v0)
- Android clipboard monitoring: prefer Accessibility; fallback to foreground sync mode; fallback to share sheet.
- iOS not in v0.

---

## Next drafts
- v0.1: multi-QUIC-stream usage (clipboard vs file)
- v0.2: relay fallback transport
- v0.3: resumable sessions + handoff
