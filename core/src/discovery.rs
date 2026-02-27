//! Discovery abstraction.

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerInfo {
    pub peer_id: String,
    pub name: String,
    pub addr: String,
}

#[async_trait]
pub trait Discovery: Send + Sync {
    async fn advertise(&self, info: PeerInfo) -> Result<()>;
    async fn scan(&self) -> Result<Vec<PeerInfo>>;
}

/// Mock discovery backed by a shared list.
#[derive(Clone)]
pub struct MockDiscovery {
    peers: Arc<Mutex<Vec<PeerInfo>>>,
}

impl MockDiscovery {
    pub fn new_shared() -> Self {
        Self { peers: Arc::new(Mutex::new(Vec::new())) }
    }

    /// Create a second handle to the same shared state.
    pub fn clone_shared(&self) -> Self {
        Self { peers: Arc::clone(&self.peers) }
    }
}

#[async_trait]
impl Discovery for MockDiscovery {
    async fn advertise(&self, info: PeerInfo) -> Result<()> {
        let mut peers = self.peers.lock().await;
        peers.retain(|p| p.peer_id != info.peer_id);
        peers.push(info);
        Ok(())
    }

    async fn scan(&self) -> Result<Vec<PeerInfo>> {
        Ok(self.peers.lock().await.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_advertise_and_scan() {
        let disc = MockDiscovery::new_shared();
        disc.advertise(PeerInfo { peer_id: "a".into(), name: "Alice".into(), addr: "mem://a".into() }).await.unwrap();
        disc.advertise(PeerInfo { peer_id: "b".into(), name: "Bob".into(), addr: "mem://b".into() }).await.unwrap();
        let peers = disc.scan().await.unwrap();
        assert_eq!(peers.len(), 2);
    }

    #[tokio::test]
    async fn shared_discovery() {
        let d1 = MockDiscovery::new_shared();
        let d2 = d1.clone_shared();
        d1.advertise(PeerInfo { peer_id: "a".into(), name: "A".into(), addr: "x".into() }).await.unwrap();
        let peers = d2.scan().await.unwrap();
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].peer_id, "a");
    }

    #[tokio::test]
    async fn advertise_replaces_existing() {
        let disc = MockDiscovery::new_shared();
        disc.advertise(PeerInfo { peer_id: "a".into(), name: "Old".into(), addr: "x".into() }).await.unwrap();
        disc.advertise(PeerInfo { peer_id: "a".into(), name: "New".into(), addr: "y".into() }).await.unwrap();
        let peers = disc.scan().await.unwrap();
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].name, "New");
    }
}
