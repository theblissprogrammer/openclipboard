# openclipboard

Cross-platform (macOS, Windows, Android, iOS) clipboard + file transfer with proximity discovery.

**Goal:** copy on one device, paste on another — fast, minimal setup, secure.

## Status
Early MVP scaffolding.

## Repo layout
- `core/` — Rust core (identity, crypto, framing, QUIC transport)
- `android/` — Android app (Kotlin)
- `macos/` — macOS app (Swift)
- `docs/` — protocol and architecture docs

## License
MIT
