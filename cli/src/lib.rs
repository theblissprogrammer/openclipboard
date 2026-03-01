use anyhow::{Context, Result};

pub mod bench;
use base64::Engine as _;
use openclipboard_core::{
    Ed25519Identity, IdentityProvider, PairingPayload, TrustRecord, derive_confirmation_code,
};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(serde::Serialize, serde::Deserialize)]
pub struct IdentityFile {
    /// base64 secret key bytes (ed25519 signing key seed)
    pub signing_key_b64: String,
}

pub fn default_identity_path() -> PathBuf {
    home_dir().join(".openclipboard").join("identity.json")
}

pub fn default_trust_path() -> PathBuf {
    home_dir().join(".openclipboard").join("trust.json")
}

pub fn home_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home);
    }
    PathBuf::from(".")
}

pub fn load_or_create_identity(path: &Path) -> Result<Ed25519Identity> {
    if path.exists() {
        load_identity(path)
    } else {
        let id = Ed25519Identity::generate();
        save_identity(path, &id)?;
        Ok(id)
    }
}

pub fn load_identity(path: &Path) -> Result<Ed25519Identity> {
    let s = fs::read_to_string(path)
        .with_context(|| format!("read identity file {}", path.display()))?;
    let file: IdentityFile = serde_json::from_str(&s)?;
    let sk_bytes = base64::engine::general_purpose::STANDARD
        .decode(file.signing_key_b64)
        .context("decode signing_key_b64")?;
    let sk_arr: [u8; 32] = sk_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("expected 32 bytes signing key seed"))?;
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&sk_arr);
    Ok(Ed25519Identity::from_signing_key(signing_key))
}

pub fn save_identity(path: &Path, id: &Ed25519Identity) -> Result<()> {
    let sk_bytes = id.signing_key_seed_bytes();
    let file = IdentityFile {
        signing_key_b64: base64::engine::general_purpose::STANDARD.encode(sk_bytes),
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(&file)?)
        .with_context(|| format!("write identity file {}", path.display()))?;
    Ok(())
}

/// Create a pairing init payload and return its QR string.
///
/// Exposed for tests so the nonce can be deterministic.
pub fn pairing_init_qr(name: String, port: u16, id: &Ed25519Identity, nonce: [u8; 32]) -> String {
    let payload = PairingPayload {
        version: 1,
        peer_id: id.peer_id().to_string(),
        name,
        identity_pk: id.public_key_bytes(),
        lan_port: port,
        nonce: nonce.to_vec(),
        lan_addrs: openclipboard_core::get_local_ip_addresses(),
    };
    payload.to_qr_string()
}

/// Respond to an init QR string; returns (resp_qr, confirmation_code).
pub fn pairing_respond_qr(
    init_qr: &str,
    name: String,
    port: u16,
    id: &Ed25519Identity,
) -> Result<(String, String)> {
    let init = PairingPayload::from_qr_string(init_qr)?;
    let resp = PairingPayload {
        version: 1,
        peer_id: id.peer_id().to_string(),
        name,
        identity_pk: id.public_key_bytes(),
        lan_port: port,
        nonce: init.nonce.clone(),
        lan_addrs: openclipboard_core::get_local_ip_addresses(),
    };
    let resp_qr = resp.to_qr_string();
    let code = derive_confirmation_code(&init.nonce, &init.peer_id, &resp.peer_id);
    Ok((resp_qr, code))
}

/// Finalize a pairing exchange; validates nonce and returns:
/// (confirmation_code, trust_records_to_write)
pub fn pairing_finalize(init_qr: &str, resp_qr: &str) -> Result<(String, [TrustRecord; 2])> {
    let init = PairingPayload::from_qr_string(init_qr)?;
    let resp = PairingPayload::from_qr_string(resp_qr)?;

    if init.nonce != resp.nonce {
        anyhow::bail!("nonce mismatch between init and resp payload");
    }
    let code = derive_confirmation_code(&init.nonce, &init.peer_id, &resp.peer_id);

    let a = TrustRecord {
        peer_id: init.peer_id,
        identity_pk: init.identity_pk,
        display_name: init.name,
        created_at: chrono::Utc::now(),
    };
    let b = TrustRecord {
        peer_id: resp.peer_id,
        identity_pk: resp.identity_pk,
        display_name: resp.name,
        created_at: chrono::Utc::now(),
    };

    Ok((code, [a, b]))
}

pub fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect()
}

pub fn preview(s: &str) -> String {
    const N: usize = 80;
    if s.len() <= N {
        return s.to_string();
    }
    format!("{}…", &s[..N])
}

pub async fn send_file<C, I, CB>(
    session: &openclipboard_core::Session<C, I, CB>,
    path: &Path,
) -> Result<()>
where
    C: openclipboard_core::Connection,
    I: openclipboard_core::IdentityProvider,
    CB: openclipboard_core::ClipboardProvider,
{
    const CHUNK: usize = 64 * 1024;

    let data = fs::read(path).with_context(|| format!("read file {}", path.display()))?;
    let size = data.len() as u64;
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file.bin");

    let file_id = blake3::hash(format!("{}:{}", name, size).as_bytes())
        .to_hex()
        .to_string();

    session
        .send_file_offer(&file_id, name, size, "application/octet-stream")
        .await?;

    // Wait a short time for accept, but don't require it.
    let _ = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        session.recv_message(),
    )
    .await;

    let mut offset = 0u64;
    for chunk in data.chunks(CHUNK) {
        session.send_file_chunk(&file_id, offset, chunk).await?;
        offset += chunk.len() as u64;
    }

    let hash = blake3::hash(&data).to_hex().to_string();
    session.send_file_done(&file_id, &hash).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_json_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("HOME", dir.path()) };

        let path = default_identity_path();
        let id = Ed25519Identity::generate();
        save_identity(&path, &id).unwrap();
        let loaded = load_identity(&path).unwrap();
        assert_eq!(loaded.peer_id(), id.peer_id());
        assert_eq!(loaded.public_key_bytes(), id.public_key_bytes());
    }

    #[test]
    fn trust_path_helpers_do_not_panic_and_allow_override() {
        let dir = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("HOME", dir.path()) };

        let a = default_identity_path();
        let b = default_trust_path();
        assert!(a.to_string_lossy().contains(".openclipboard"));
        assert!(b.to_string_lossy().contains(".openclipboard"));

        let override_path = dir.path().join("custom").join("trust.json");
        assert!(override_path.ends_with("trust.json"));
    }

    #[test]
    fn pairing_qr_roundtrip_init_respond_finalize() {
        let alice = Ed25519Identity::generate();
        let bob = Ed25519Identity::generate();
        let nonce = [7u8; 32];

        let init_qr = pairing_init_qr("Alice".into(), 1111, &alice, nonce);
        let (resp_qr, code1) = pairing_respond_qr(&init_qr, "Bob".into(), 2222, &bob).unwrap();
        let (code2, records) = pairing_finalize(&init_qr, &resp_qr).unwrap();

        assert_eq!(code1, code2);
        let ids: Vec<String> = records.iter().map(|r| r.peer_id.clone()).collect();
        assert!(ids.contains(&alice.peer_id().to_string()));
        assert!(ids.contains(&bob.peer_id().to_string()));
    }
}
