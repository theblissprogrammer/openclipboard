//! Peer identity abstraction.

/// Identity provider trait.
pub trait IdentityProvider: Send + Sync {
    /// Get this peer's unique ID.
    fn peer_id(&self) -> &str;
    /// Sign data, return signature bytes.
    fn sign(&self, data: &[u8]) -> Vec<u8>;
    /// Verify a signature from a given peer_id.
    fn verify(&self, peer_id: &str, data: &[u8], signature: &[u8]) -> bool;
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
}
