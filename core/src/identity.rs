//! Peer identity abstraction.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

/// Identity provider trait.
pub trait IdentityProvider: Send + Sync {
    /// Get this peer's unique ID.
    fn peer_id(&self) -> &str;
    /// Sign data, return signature bytes.
    fn sign(&self, data: &[u8]) -> Vec<u8>;
    /// Verify a signature from a given peer_id.
    fn verify(&self, peer_id: &str, data: &[u8], signature: &[u8]) -> bool;
    /// Return the public key bytes (for serialization/pairing).
    fn public_key_bytes(&self) -> Vec<u8>;
}

/// Ed25519-based identity using ed25519-dalek.
///
/// Peer ID is `blake3(public_key)` in hex.
#[derive(Clone)]
pub struct Ed25519Identity {
    signing_key: SigningKey,
    id: String,
}

impl Ed25519Identity {
    pub fn generate() -> Self {
        // ed25519-dalek requires a CSPRNG; use OsRng.
        let mut rng = rand_core::OsRng;
        let signing_key = SigningKey::generate(&mut rng);
        let id = Self::peer_id_from_public_key(signing_key.verifying_key().as_bytes());
        Self { signing_key, id }
    }

    pub fn from_signing_key(signing_key: SigningKey) -> Self {
        let id = Self::peer_id_from_public_key(signing_key.verifying_key().as_bytes());
        Self { signing_key, id }
    }

    /// Export the signing key seed bytes (32 bytes). Useful for local persistence.
    pub fn signing_key_seed_bytes(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }

    /// Verify signature against the provided raw public key bytes.
    pub fn verify_with_public_key(data: &[u8], signature: &[u8], public_key: &[u8]) -> bool {
        let Ok(pk_bytes): Result<[u8; 32], _> = public_key.try_into() else {
            return false;
        };
        let Ok(pk) = VerifyingKey::from_bytes(&pk_bytes) else {
            return false;
        };
        let Ok(sig_bytes): Result<[u8; 64], _> = signature.try_into() else {
            return false;
        };
        let sig = Signature::from_bytes(&sig_bytes);
        pk.verify(data, &sig).is_ok()
    }

    pub fn peer_id_from_public_key(public_key: &[u8]) -> String {
        blake3::hash(public_key).to_hex().to_string()
    }
}

impl IdentityProvider for Ed25519Identity {
    fn peer_id(&self) -> &str {
        &self.id
    }

    fn sign(&self, data: &[u8]) -> Vec<u8> {
        self.signing_key.sign(data).to_bytes().to_vec()
    }

    fn verify(&self, peer_id: &str, data: &[u8], signature: &[u8]) -> bool {
        if peer_id != self.id {
            return false;
        }
        Self::verify_with_public_key(data, signature, self.signing_key.verifying_key().as_bytes())
    }

    fn public_key_bytes(&self) -> Vec<u8> {
        self.signing_key.verifying_key().as_bytes().to_vec()
    }
}

/// Blake3-based identity (stub — uses blake3 hash as "signing").
pub struct Blake3Identity {
    secret: Vec<u8>,
    id: String,
}

impl Blake3Identity {
    pub fn generate() -> Self {
        let secret: Vec<u8> = (0..32).map(|_| rand_byte()).collect();
        let id = blake3::hash(&secret).to_hex().to_string();
        Self { secret, id }
    }

    pub fn from_secret(secret: Vec<u8>) -> Self {
        let id = blake3::hash(&secret).to_hex().to_string();
        Self { secret, id }
    }
}

fn rand_byte() -> u8 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let s = RandomState::new();
    let mut h = s.build_hasher();
    h.write_u8(0);
    h.finish() as u8
}

impl IdentityProvider for Blake3Identity {
    fn peer_id(&self) -> &str {
        &self.id
    }

    fn sign(&self, data: &[u8]) -> Vec<u8> {
        let mut keyed = blake3::Hasher::new_keyed(&padded_key(&self.secret));
        keyed.update(data);
        keyed.finalize().as_bytes().to_vec()
    }

    fn verify(&self, peer_id: &str, data: &[u8], signature: &[u8]) -> bool {
        if peer_id != self.id {
            return false;
        }
        let expected = self.sign(data);
        expected == signature
    }

    fn public_key_bytes(&self) -> Vec<u8> {
        // Not a real public key; used only for tests.
        self.secret.clone()
    }
}

fn padded_key(secret: &[u8]) -> [u8; 32] {
    let mut key = [0u8; 32];
    let len = secret.len().min(32);
    key[..len].copy_from_slice(&secret[..len]);
    key
}

/// Mock identity for testing.
pub struct MockIdentity {
    id: String,
}

impl MockIdentity {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

impl IdentityProvider for MockIdentity {
    fn peer_id(&self) -> &str {
        &self.id
    }

    fn sign(&self, data: &[u8]) -> Vec<u8> {
        let mut out = self.id.as_bytes().to_vec();
        out.extend_from_slice(data);
        blake3::hash(&out).as_bytes().to_vec()
    }

    fn verify(&self, peer_id: &str, data: &[u8], signature: &[u8]) -> bool {
        let mut out = peer_id.as_bytes().to_vec();
        out.extend_from_slice(data);
        let expected = blake3::hash(&out).as_bytes().to_vec();
        expected == signature
    }

    fn public_key_bytes(&self) -> Vec<u8> {
        self.id.as_bytes().to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blake3_identity_generate() {
        let id = Blake3Identity::generate();
        assert!(!id.peer_id().is_empty());
    }

    #[test]
    fn blake3_sign_verify() {
        let id = Blake3Identity::generate();
        let data = b"hello world";
        let sig = id.sign(data);
        assert!(id.verify(id.peer_id(), data, &sig));
        assert!(!id.verify(id.peer_id(), b"wrong", &sig));
    }

    #[test]
    fn mock_identity_sign_verify() {
        let id = MockIdentity::new("peer-a");
        let sig = id.sign(b"test");
        assert!(id.verify("peer-a", b"test", &sig));
        assert!(!id.verify("peer-b", b"test", &sig));
    }

    #[test]
    fn deterministic_peer_id() {
        let a = Blake3Identity::from_secret(vec![1, 2, 3]);
        let b = Blake3Identity::from_secret(vec![1, 2, 3]);
        assert_eq!(a.peer_id(), b.peer_id());
    }

    #[test]
    fn ed25519_generate_sign_verify() {
        let id = Ed25519Identity::generate();
        assert!(!id.peer_id().is_empty());

        let data = b"hello ed25519";
        let sig = id.sign(data);
        assert!(id.verify(id.peer_id(), data, &sig));
        assert!(!id.verify(id.peer_id(), b"wrong", &sig));
        assert!(!id.verify("other-peer", data, &sig));
    }

    #[test]
    fn ed25519_peer_id_from_public_key_matches() {
        let id = Ed25519Identity::generate();
        let pk = id.public_key_bytes();
        let derived = Ed25519Identity::peer_id_from_public_key(&pk);
        assert_eq!(derived, id.peer_id());
    }

    #[test]
    fn ed25519_verify_with_public_key() {
        let id = Ed25519Identity::generate();
        let data = b"cross verify";
        let sig = id.sign(data);
        let pk = id.public_key_bytes();

        assert!(Ed25519Identity::verify_with_public_key(data, &sig, &pk));
        assert!(!Ed25519Identity::verify_with_public_key(b"wrong", &sig, &pk));
    }
}
