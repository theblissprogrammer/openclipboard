//! openclipboard_core — trait-based architecture for cross-device clipboard sync.

pub mod protocol;
pub mod identity;
pub mod transport;
pub mod discovery;
pub mod clipboard;
pub mod session;
pub mod quic_transport;
pub mod pairing;
pub mod trust;
pub mod replay;
pub mod sync;
pub mod mesh;

pub use protocol::{Frame, MsgType, StreamId, Message, encode_frame, decode_frame, encode_message, decode_message, PROTOCOL_VERSION};
pub use identity::{IdentityProvider, Blake3Identity, MockIdentity, Ed25519Identity};
pub use transport::{Connection, Transport, Listener, MemoryConnection, memory_connection_pair, MemoryListener};
pub use discovery::{Discovery, PeerInfo, MockDiscovery, MdnsDiscovery, DiscoveryEvent, DiscoveryListener, BoxDiscovery};
pub use clipboard::{ClipboardContent, ClipboardProvider, MockClipboard};
pub use session::Session;
pub use trust::{TrustRecord, TrustStore, MemoryTrustStore, FileTrustStore, default_trust_store_path};
pub use replay::{ReplayProtector, MemoryReplayProtector};
pub use pairing::{PairingPayload, derive_confirmation_code};
pub use sync::{SyncService, SyncHandler, EchoSuppressor};
pub use mesh::{PeerRegistry, PeerEntry, PeerStatus, FanoutResult, start_clipboard_watcher};
