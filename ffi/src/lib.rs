use anyhow::Context as _;
use base64::Engine as _;
use std::sync::Arc;

use openclipboard_core::{
    derive_confirmation_code as core_derive_confirmation_code,
    Ed25519Identity,
    IdentityProvider,
    TrustStore as CoreTrustStore,
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

uniffi::include_scaffolding!("openclipboard");
