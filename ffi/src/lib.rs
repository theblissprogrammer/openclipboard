use anyhow::Context as _;
use base64::Engine as _;
use std::sync::Arc;
use std::sync::Mutex;
use std::collections::HashMap;

use openclipboard_core::{
    derive_confirmation_code as core_derive_confirmation_code,
    Ed25519Identity,
    IdentityProvider,
    TrustStore as CoreTrustStore,
    Session,
    MemoryReplayProtector,
    FileTrustStore,
    ClipboardProvider,
    clipboard::MockClipboard,
    quic_transport::{make_server_endpoint, make_insecure_client_endpoint, QuicListener, QuicTransport},
    Listener,
    Transport,
    Message,
};

// NOTE: This crate uses the UDL-based UniFFI flow.
// - Types are defined in `src/openclipboard.udl`
// - `build.rs` generates Rust scaffolding from the UDL
// - `uniffi::include_scaffolding!` includes the generated glue
// Therefore we DO NOT use proc-macro derives like `uniffi::Object` / `uniffi::Record`
// or `#[uniffi::export]` here — doing so would create duplicate symbols.

#[derive(Debug, Clone)]
pub enum OpenClipboardError {
    Other,
}

impl std::fmt::Display for OpenClipboardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenClipboardError::Other => write!(f, "OpenClipboardError::Other"),
        }
    }
}

impl std::error::Error for OpenClipboardError {}

impl From<anyhow::Error> for OpenClipboardError {
    fn from(_: anyhow::Error) -> Self {
        Self::Other
    }
}

pub type Result<T> = std::result::Result<T, OpenClipboardError>;

// ─────────────────────────────────────────────────────────────────────────────
// Dictionaries (UDL `dictionary`)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct IdentityInfo {
    pub peer_id: String,
    pub pubkey_b64: String,
}

#[derive(Clone, Debug)]
pub struct TrustRecord {
    pub peer_id: String,
    pub identity_pk_b64: String,
    pub display_name: String,
    pub created_at_ms: u64,
}

// ─────────────────────────────────────────────────────────────────────────────
// Interfaces (UDL `interface`)
// ─────────────────────────────────────────────────────────────────────────────

pub struct Identity {
    inner: Ed25519Identity,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct IdentityFile {
    /// base64 secret key bytes (ed25519 signing key seed)
    signing_key_b64: String,
}

impl Identity {
    pub fn peer_id(&self) -> String {
        self.inner.peer_id().to_string()
    }

    pub fn pubkey_b64(&self) -> String {
        base64::engine::general_purpose::STANDARD.encode(self.inner.public_key_bytes())
    }

    pub fn info(&self) -> IdentityInfo {
        IdentityInfo {
            peer_id: self.peer_id(),
            pubkey_b64: self.pubkey_b64(),
        }
    }

    pub fn save(&self, path: String) -> Result<()> {
        let sk_bytes = self.inner.signing_key_seed_bytes();
        let file = IdentityFile {
            signing_key_b64: base64::engine::general_purpose::STANDARD.encode(sk_bytes),
        };

        let path = std::path::PathBuf::from(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create parent dir {}", parent.display()))?;
        }
        let json = serde_json::to_string_pretty(&file).context("serialize identity")?;
        std::fs::write(&path, json).with_context(|| format!("write {}", path.display()))?;
        Ok(())
    }
}

pub fn identity_generate() -> Arc<Identity> {
    Arc::new(Identity {
        inner: Ed25519Identity::generate(),
    })
}

pub fn identity_load(path: String) -> Result<Arc<Identity>> {
    let path = std::path::PathBuf::from(path);
    let s = std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let file: IdentityFile = serde_json::from_str(&s).context("parse identity json")?;
    let sk_bytes = base64::engine::general_purpose::STANDARD
        .decode(file.signing_key_b64)
        .context("decode signing_key_b64")?;
    let sk_arr: [u8; 32] = sk_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("expected 32 bytes signing key seed"))?;
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&sk_arr);
    Ok(Arc::new(Identity {
        inner: Ed25519Identity::from_signing_key(signing_key),
    }))
}

pub struct PairingPayload {
    inner: openclipboard_core::PairingPayload,
}

impl PairingPayload {
    pub fn version(&self) -> u8 {
        self.inner.version
    }

    pub fn peer_id(&self) -> String {
        self.inner.peer_id.clone()
    }

    pub fn name(&self) -> String {
        self.inner.name.clone()
    }

    pub fn identity_pk(&self) -> Vec<u8> {
        self.inner.identity_pk.clone()
    }

    pub fn lan_port(&self) -> u16 {
        self.inner.lan_port
    }

    pub fn nonce(&self) -> Vec<u8> {
        self.inner.nonce.clone()
    }

    pub fn to_qr_string(&self) -> Result<String> {
        Ok(self.inner.to_qr_string())
    }
}

pub fn pairing_payload_create(
    version: u8,
    peer_id: String,
    name: String,
    identity_pk: Vec<u8>,
    lan_port: u16,
    nonce: Vec<u8>,
) -> Arc<PairingPayload> {
    Arc::new(PairingPayload {
        inner: openclipboard_core::PairingPayload {
            version,
            peer_id,
            name,
            identity_pk,
            lan_port,
            nonce,
        },
    })
}

pub fn pairing_payload_from_qr_string(s: String) -> Result<Arc<PairingPayload>> {
    let inner = openclipboard_core::PairingPayload::from_qr_string(&s)?;
    Ok(Arc::new(PairingPayload { inner }))
}

pub fn derive_confirmation_code(nonce: Vec<u8>, peer_a_id: String, peer_b_id: String) -> String {
    core_derive_confirmation_code(&nonce, &peer_a_id, &peer_b_id)
}

pub fn default_identity_path() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    std::path::PathBuf::from(home)
        .join(".openclipboard")
        .join("identity.json")
        .to_string_lossy()
        .to_string()
}

fn trust_record_from_core(r: openclipboard_core::TrustRecord) -> TrustRecord {
    TrustRecord {
        peer_id: r.peer_id,
        identity_pk_b64: base64::engine::general_purpose::STANDARD.encode(r.identity_pk),
        display_name: r.display_name,
        created_at_ms: r.created_at.timestamp_millis().max(0) as u64,
    }
}

pub struct TrustStore {
    inner: openclipboard_core::FileTrustStore,
}

pub fn trust_store_open(path: String) -> Result<Arc<TrustStore>> {
    let path = std::path::PathBuf::from(path);
    let inner = openclipboard_core::FileTrustStore::new(path)?;
    Ok(Arc::new(TrustStore { inner }))
}

pub fn trust_store_default_path() -> String {
    openclipboard_core::default_trust_store_path()
        .to_string_lossy()
        .to_string()
}

impl TrustStore {
    pub fn add(&self, peer_id: String, identity_pk_b64: String, display_name: String) -> Result<()> {
        let pk = base64::engine::general_purpose::STANDARD
            .decode(identity_pk_b64)
            .context("decode identity_pk_b64")?;

        let record = openclipboard_core::TrustRecord {
            peer_id,
            identity_pk: pk,
            display_name,
            created_at: chrono::Utc::now(),
        };
        self.inner.save(record)?;
        Ok(())
    }

    pub fn get(&self, peer_id: String) -> Result<Option<TrustRecord>> {
        let got = self.inner.get(&peer_id)?;
        Ok(got.map(trust_record_from_core))
    }

    pub fn list(&self) -> Result<Vec<TrustRecord>> {
        Ok(self
            .inner
            .list()?
            .into_iter()
            .map(trust_record_from_core)
            .collect())
    }

    pub fn remove(&self, peer_id: String) -> Result<bool> {
        Ok(self.inner.remove(&peer_id)?)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ClipboardNode & EventHandler
// ─────────────────────────────────────────────────────────────────────────────

pub trait EventHandler: Send + Sync {
    fn on_clipboard_text(&self, peer_id: String, text: String, ts_ms: u64);
    fn on_file_received(&self, peer_id: String, name: String, data_path: String);
    fn on_peer_connected(&self, peer_id: String);
    fn on_peer_disconnected(&self, peer_id: String);
    fn on_error(&self, message: String);
}

struct IncomingFile {
    name: String,
    expected: u64,
    buf: Vec<u8>,
}

pub struct ClipboardNode {
    identity: Ed25519Identity,
    trust_store: Arc<FileTrustStore>,
    replay_protector: Arc<MemoryReplayProtector>,
    runtime: tokio::runtime::Runtime,
    listener_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl ClipboardNode {
    fn new_internal(identity_path: String, trust_path: String) -> Result<Self> {
        let identity_path = std::path::PathBuf::from(identity_path);
        let trust_path = std::path::PathBuf::from(trust_path);

        // Load or create identity
        let identity = if identity_path.exists() {
            let s = std::fs::read_to_string(&identity_path)
                .with_context(|| format!("read identity from {}", identity_path.display()))?;
            let file: IdentityFile = serde_json::from_str(&s).context("parse identity json")?;
            let sk_bytes = base64::engine::general_purpose::STANDARD
                .decode(file.signing_key_b64)
                .context("decode signing_key_b64")?;
            let sk_arr: [u8; 32] = sk_bytes
                .try_into()
                .map_err(|_| anyhow::anyhow!("expected 32 bytes signing key seed"))?;
            let signing_key = ed25519_dalek::SigningKey::from_bytes(&sk_arr);
            Ed25519Identity::from_signing_key(signing_key)
        } else {
            let identity = Ed25519Identity::generate();
            let sk_bytes = identity.signing_key_seed_bytes();
            let file = IdentityFile {
                signing_key_b64: base64::engine::general_purpose::STANDARD.encode(sk_bytes),
            };
            if let Some(parent) = identity_path.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("create parent dir {}", parent.display()))?;
            }
            let json = serde_json::to_string_pretty(&file).context("serialize identity")?;
            std::fs::write(&identity_path, json)
                .with_context(|| format!("write {}", identity_path.display()))?;
            identity
        };

        let trust_store = Arc::new(FileTrustStore::new(trust_path)?);
        let replay_protector = Arc::new(MemoryReplayProtector::new(1024));
        let runtime = tokio::runtime::Runtime::new()
            .context("create tokio runtime")?;

        Ok(Self {
            identity,
            trust_store,
            replay_protector,
            runtime,
            listener_handle: Mutex::new(None),
        })
    }
}

impl ClipboardNode {
    pub fn peer_id(&self) -> String {
        self.identity.peer_id().to_string()
    }

    pub fn start_listener(&self, port: u16, handler: Box<dyn EventHandler>) -> Result<()> {
        let identity = self.identity.clone();
        let trust_store = self.trust_store.clone();
        let replay_protector = self.replay_protector.clone();

        // Bind synchronously so callers can connect immediately after this returns.
        // (The previous implementation raced: connect could happen before the endpoint was bound.)
        // For unit tests and local loopback, bind to localhost.
        // (Binding to 0.0.0.0 can fail in some CI sandboxes / restricted environments.)
        let bind = format!("127.0.0.1:{}", port).parse().unwrap();
        let (endpoint, _cert) = match make_server_endpoint(bind) {
            Ok(ep) => ep,
            Err(e) => {
                let msg = format!("Failed to create server endpoint: {}", e);
                handler.on_error(msg.clone());
                eprintln!("{msg}");
                return Err(OpenClipboardError::Other);
            }
        };

        let handle = self.runtime.spawn(async move {
            let listener = QuicListener::new(endpoint);

            loop {
                let conn = match listener.accept().await {
                    Ok(conn) => conn,
                    Err(e) => {
                        handler.on_error(format!("Failed to accept connection: {}", e));
                        continue;
                    }
                };

                let session = Session::with_trust_and_replay(
                    conn,
                    identity.clone(),
                    MockClipboard::new(),
                    trust_store.clone(),
                    replay_protector.clone(),
                );

                let peer_id = match session.handshake().await {
                    Ok(peer_id) => {
                        handler.on_peer_connected(peer_id.clone());
                        peer_id
                    }
                    Err(e) => {
                        handler.on_error(format!("Handshake failed: {}", e));
                        continue;
                    }
                };

                let mut files: HashMap<String, IncomingFile> = HashMap::new();

                loop {
                    let msg = match session.recv_message().await {
                        Ok(m) => m,
                        Err(_) => {
                            handler.on_peer_disconnected(peer_id.clone());
                            break;
                        }
                    };

                    match msg {
                        Message::ClipText { text, ts_ms, .. } => {
                            handler.on_clipboard_text(peer_id.clone(), text, ts_ms);
                        }
                        Message::FileOffer { file_id, name, size, .. } => {
                            files.insert(
                                file_id.clone(),
                                IncomingFile { name: name.clone(), expected: size, buf: Vec::new() },
                            );
                            if session.send_file_accept(&file_id).await.is_err() {
                                handler.on_error("Failed to send file accept".to_string());
                            }
                        }
                        Message::FileChunk { file_id, data_b64, .. } => {
                            if let Ok(data) = base64::engine::general_purpose::STANDARD.decode(data_b64) {
                                if let Some(f) = files.get_mut(&file_id) {
                                    f.buf.extend_from_slice(&data);
                                }
                            }
                        }
                        Message::FileDone { file_id, .. } => {
                            if let Some(f) = files.remove(&file_id) {
                                // Save file to temp directory
                                let temp_dir = std::env::temp_dir().join("openclipboard");
                                let _ = std::fs::create_dir_all(&temp_dir);
                                let safe_name = f.name.chars()
                                    .map(|c| if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
                                    .collect::<String>();
                                let temp_path = temp_dir.join(safe_name);
                                if std::fs::write(&temp_path, &f.buf).is_ok() {
                                    handler.on_file_received(
                                        peer_id.clone(),
                                        f.name,
                                        temp_path.to_string_lossy().to_string(),
                                    );
                                }
                            }
                        }
                        _ => {} // Ignore other message types
                    }
                }
            }
        });

        *self.listener_handle.lock().unwrap() = Some(handle);
        Ok(())
    }

    pub fn connect_and_send_text(&self, addr: String, text: String) -> Result<()> {
        let identity = self.identity.clone();
        let trust_store = self.trust_store.clone();
        let replay_protector = self.replay_protector.clone();

        self.runtime.block_on(async move {
            let endpoint = make_insecure_client_endpoint()?;
            let transport = QuicTransport::new(endpoint);
            let conn = transport.connect(&addr).await?;

            let session = Session::with_trust_and_replay(
                conn,
                identity,
                MockClipboard::new(),
                trust_store,
                replay_protector,
            );

            session.handshake().await?;

            use openclipboard_core::ClipboardContent;
            session.clipboard.write(ClipboardContent::Text(text))?;
            session.send_clipboard().await?;

            Ok::<_, anyhow::Error>(())
        })?;

        Ok(())
    }

    pub fn connect_and_send_file(&self, addr: String, file_path: String) -> Result<()> {
        let identity = self.identity.clone();
        let trust_store = self.trust_store.clone();
        let replay_protector = self.replay_protector.clone();
        let file_path = std::path::PathBuf::from(file_path);

        self.runtime.block_on(async move {
            let endpoint = make_insecure_client_endpoint()?;
            let transport = QuicTransport::new(endpoint);
            let conn = transport.connect(&addr).await?;

            let session = Session::with_trust_and_replay(
                conn,
                identity,
                MockClipboard::new(),
                trust_store,
                replay_protector,
            );

            session.handshake().await?;

            // Use the send_file helper from cli
            Self::send_file_internal(&session, &file_path).await?;

            Ok::<_, anyhow::Error>(())
        })?;

        Ok(())
    }

    pub fn stop(&self) {
        if let Some(handle) = self.listener_handle.lock().unwrap().take() {
            handle.abort();
        }
    }

    // Helper function to send a file (similar to cli/src/lib.rs)
    async fn send_file_internal(
        session: &Session<impl openclipboard_core::transport::Connection, Ed25519Identity, MockClipboard>,
        path: &std::path::Path,
    ) -> anyhow::Result<()> {
        use sha2::{Digest, Sha256};

        let data = tokio::fs::read(path).await
            .with_context(|| format!("read file {}", path.display()))?;
        
        let name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let file_id = format!("file-{}", rand_core::RngCore::next_u32(&mut rand_core::OsRng));
        let size = data.len() as u64;
        let mime = "application/octet-stream".to_string();

        session.send_file_offer(&file_id, &name, size, &mime).await?;

        // Wait for accept
        let msg = session.recv_message().await?;
        if !matches!(msg, Message::FileAccept { .. }) {
            anyhow::bail!("expected file accept");
        }

        // Send chunks
        const CHUNK_SIZE: usize = 64 * 1024;
        for (i, chunk) in data.chunks(CHUNK_SIZE).enumerate() {
            let offset = (i * CHUNK_SIZE) as u64;
            session.send_file_chunk(&file_id, offset, chunk).await?;
        }

        // Send done with hash
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let hash = format!("{:x}", hasher.finalize());
        session.send_file_done(&file_id, &hash).await?;

        Ok(())
    }
}

pub fn clipboard_node_new(identity_path: String, trust_path: String) -> Result<Arc<ClipboardNode>> {
    Ok(Arc::new(ClipboardNode::new_internal(identity_path, trust_path)?))
}

uniffi::include_scaffolding!("openclipboard");
