//! Replay protection for authenticated handshakes.

use anyhow::Result;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

/// Protects against replayed handshakes (e.g. re-sent `Hello` messages).
///
/// Implementations should be thread-safe and may keep bounded state.
pub trait ReplayProtector: Send + Sync {
    /// Check whether `nonce` has already been seen for `peer_id`.
    /// If it is new, store it and return Ok(()). If it is a replay, return Err.
    fn check_and_store(&self, peer_id: &str, nonce: &[u8]) -> Result<()>;
}

/// In-memory replay protector.
///
/// Stores a bounded FIFO list of recently seen nonces per peer.
pub struct MemoryReplayProtector {
    pub per_peer_capacity: usize,
    pub map: Mutex<HashMap<String, VecDeque<[u8; 32]>>>,
}

impl MemoryReplayProtector {
    pub fn new(per_peer_capacity: usize) -> Self {
        Self { per_peer_capacity: per_peer_capacity.max(1), map: Mutex::new(HashMap::new()) }
    }
}

impl ReplayProtector for MemoryReplayProtector {
    fn check_and_store(&self, peer_id: &str, nonce: &[u8]) -> Result<()> {
        let nonce: [u8; 32] = nonce
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid nonce length for replay check"))?;

        let mut map = self.map.lock().expect("replay protector mutex poisoned");
        let q = map.entry(peer_id.to_string()).or_default();

        if q.iter().any(|n| n == &nonce) {
            anyhow::bail!("replayed hello nonce for peer_id={peer_id}");
        }

        q.push_back(nonce);
        while q.len() > self.per_peer_capacity {
            q.pop_front();
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_replay_per_peer() {
        let rp = MemoryReplayProtector::new(8);
        let nonce = [7u8; 32];
        rp.check_and_store("peer1", &nonce).unwrap();
        assert!(rp.check_and_store("peer1", &nonce).is_err());

        // Different peer_id should not collide.
        rp.check_and_store("peer2", &nonce).unwrap();
    }

    #[test]
    fn respects_capacity() {
        let rp = MemoryReplayProtector::new(1);
        let n1 = [1u8; 32];
        let n2 = [2u8; 32];
        rp.check_and_store("peer", &n1).unwrap();
        rp.check_and_store("peer", &n2).unwrap();

        // n1 was evicted, so it can be seen again without being flagged.
        rp.check_and_store("peer", &n1).unwrap();
    }
}
