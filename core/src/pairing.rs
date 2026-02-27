//! Pairing protocol: QR payload generation and confirmation code derivation.

use anyhow::Result;
use base64::Engine as _;
use serde::{Deserialize, Serialize};

/// Payload exchanged during pairing (e.g. encoded as QR).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PairingPayload {
    pub version: u8,
    pub peer_id: String,
    pub name: String,
    pub identity_pk: Vec<u8>,
    pub lan_port: u16,
    pub nonce: Vec<u8>,
}

impl PairingPayload {
    /// Serialize to JSON then base64 (URL-safe, no padding) for QR embedding.
    pub fn to_qr_string(&self) -> String {
        let json = serde_json::to_vec(self).expect("PairingPayload JSON serialize");
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json)
    }

    /// Parse from base64(JSON).
    pub fn from_qr_string(s: &str) -> Result<Self> {
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(s)?;
        Ok(serde_json::from_slice(&bytes)?)
    }
}

/// Derive a 6-digit confirmation code from the nonce and both peer IDs.
///
/// `code = blake3(nonce || peer_a_id || peer_b_id) % 1_000_000`.
pub fn derive_confirmation_code(nonce: &[u8], peer_a_id: &str, peer_b_id: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(nonce);
    hasher.update(peer_a_id.as_bytes());
    hasher.update(peer_b_id.as_bytes());
    let hash = hasher.finalize();

    let b = hash.as_bytes();
    let n = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    format!("{:06}", n % 1_000_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairing_payload_qr_roundtrip() {
        let payload = PairingPayload {
            version: 1,
            peer_id: "peer-a".into(),
            name: "Alice's Mac".into(),
            identity_pk: vec![1, 2, 3, 4],
            lan_port: 18455,
            nonce: vec![9; 32],
        };

        let s = payload.to_qr_string();
        let decoded = PairingPayload::from_qr_string(&s).unwrap();
        assert_eq!(payload, decoded);
    }

    #[test]
    fn derive_code_is_deterministic_and_6_digits() {
        let nonce = vec![42u8; 32];
        let code1 = derive_confirmation_code(&nonce, "peer-a", "peer-b");
        let code2 = derive_confirmation_code(&nonce, "peer-a", "peer-b");
        assert_eq!(code1, code2);
        assert_eq!(code1.len(), 6);
        assert!(code1.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn derive_code_changes_with_inputs() {
        let nonce = vec![42u8; 32];
        let c1 = derive_confirmation_code(&nonce, "peer-a", "peer-b");
        let c2 = derive_confirmation_code(&nonce, "peer-a", "peer-c");
        assert_ne!(c1, c2);
    }
}
