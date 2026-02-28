//! Mesh clipboard sync: PeerRegistry + clipboard watcher + fanout orchestration.

use crate::clipboard::{ClipboardContent, ClipboardProvider};
use crate::sync::EchoSuppressor;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

// ─────────────────────────────────────────────────────────────────────────────
// PeerRegistry
// ─────────────────────────────────────────────────────────────────────────────

/// Online/offline status of a peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerStatus {
    Online,
    Offline,
}

/// Runtime info for a known peer.
#[derive(Debug, Clone)]
pub struct PeerEntry {
    pub peer_id: String,
    pub display_name: String,
    pub last_addr: Option<String>,
    pub status: PeerStatus,
}

/// Thread-safe runtime registry of known peers.
#[derive(Clone)]
pub struct PeerRegistry {
    peers: Arc<RwLock<HashMap<String, PeerEntry>>>,
}

impl PeerRegistry {
    pub fn new() -> Self {
        Self {
            peers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Seed from trust store records.
    pub async fn load_from_trust<T: crate::trust::TrustStore + ?Sized>(&self, store: &T) -> anyhow::Result<()> {
        let records = store.list()?;
        let mut map = self.peers.write().await;
        for rec in records {
            map.entry(rec.peer_id.clone()).or_insert_with(|| PeerEntry {
                peer_id: rec.peer_id,
                display_name: rec.display_name,
                last_addr: None,
                status: PeerStatus::Offline,
            });
        }
        Ok(())
    }

    pub async fn set_online(&self, peer_id: &str, addr: Option<String>) {
        let mut map = self.peers.write().await;
        if let Some(entry) = map.get_mut(peer_id) {
            entry.status = PeerStatus::Online;
            if let Some(a) = addr {
                entry.last_addr = Some(a);
            }
        }
    }

    pub async fn set_offline(&self, peer_id: &str) {
        let mut map = self.peers.write().await;
        if let Some(entry) = map.get_mut(peer_id) {
            entry.status = PeerStatus::Offline;
        }
    }

    pub async fn list_online(&self) -> Vec<PeerEntry> {
        let map = self.peers.read().await;
        map.values()
            .filter(|e| e.status == PeerStatus::Online)
            .cloned()
            .collect()
    }

    pub async fn list_all(&self) -> Vec<PeerEntry> {
        let map = self.peers.read().await;
        map.values().cloned().collect()
    }

    pub async fn get(&self, peer_id: &str) -> Option<PeerEntry> {
        let map = self.peers.read().await;
        map.get(peer_id).cloned()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Clipboard watcher
// ─────────────────────────────────────────────────────────────────────────────

/// Result of a fanout broadcast.
#[derive(Debug, Clone)]
pub struct FanoutResult {
    pub peer_id: String,
    pub success: bool,
    pub error: Option<String>,
}

/// Watch a clipboard provider for changes and invoke a callback.
///
/// Polls every `poll_interval` and compares with last known content.
/// Uses the `EchoSuppressor` to skip content we just received from a peer.
/// Returns a `JoinHandle` that runs until the `stop` receiver fires.
pub fn start_clipboard_watcher<F>(
    provider: Arc<dyn ClipboardProvider>,
    echo_suppressor: Arc<Mutex<EchoSuppressor>>,
    poll_interval: std::time::Duration,
    mut stop_rx: tokio::sync::watch::Receiver<bool>,
    on_change: F,
) -> tokio::task::JoinHandle<()>
where
    F: Fn(ClipboardContent) + Send + Sync + 'static,
{
    tokio::spawn(async move {
        let mut last: Option<ClipboardContent> = None;

        loop {
            tokio::select! {
                _ = stop_rx.changed() => { break; }
                _ = tokio::time::sleep(poll_interval) => {}
            }

            let current = match provider.read() {
                Ok(c) => c,
                Err(_) => continue,
            };

            if current == ClipboardContent::Empty {
                continue;
            }

            if last.as_ref() == Some(&current) {
                continue;
            }

            // Check echo suppression for text content.
            if let ClipboardContent::Text(ref t) = current {
                if echo_suppressor.lock().await.should_ignore_local_change(t) {
                    last = Some(current);
                    continue;
                }
            }

            last = Some(current.clone());
            on_change(current);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard::MockClipboard;
    use crate::trust::TrustStore;

    #[test]
    fn peer_registry_basic() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let reg = PeerRegistry::new();
            let store = crate::trust::MemoryTrustStore::new();
            store.save(crate::trust::TrustRecord {
                peer_id: "p1".into(),
                identity_pk: vec![1],
                display_name: "Peer1".into(),
                created_at: chrono::Utc::now(),
            }).unwrap();
            store.save(crate::trust::TrustRecord {
                peer_id: "p2".into(),
                identity_pk: vec![2],
                display_name: "Peer2".into(),
                created_at: chrono::Utc::now(),
            }).unwrap();

            reg.load_from_trust(&store).await.unwrap();
            assert_eq!(reg.list_all().await.len(), 2);
            assert_eq!(reg.list_online().await.len(), 0);

            reg.set_online("p1", Some("1.2.3.4:5000".into())).await;
            assert_eq!(reg.list_online().await.len(), 1);

            reg.set_offline("p1").await;
            assert_eq!(reg.list_online().await.len(), 0);
        });
    }

    #[tokio::test]
    async fn clipboard_watcher_detects_change() {
        let cb = Arc::new(MockClipboard::new());
        let suppressor = Arc::new(Mutex::new(EchoSuppressor::new(8)));
        let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let handle = start_clipboard_watcher(
            cb.clone(),
            suppressor,
            std::time::Duration::from_millis(50),
            stop_rx,
            move |content| {
                let _ = tx.send(content);
            },
        );

        // Change clipboard
        cb.write(ClipboardContent::Text("hello".into())).unwrap();

        let got = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got, ClipboardContent::Text("hello".into()));

        let _ = stop_tx.send(true);
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn watcher_suppresses_echoed_content() {
        let cb = Arc::new(MockClipboard::new());
        let suppressor = Arc::new(Mutex::new(EchoSuppressor::new(8)));

        // Pre-note as remote write
        suppressor.lock().await.note_remote_write("remote-text");

        let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let _handle = start_clipboard_watcher(
            cb.clone(),
            suppressor,
            std::time::Duration::from_millis(50),
            stop_rx,
            move |content| {
                let _ = tx.send(content);
            },
        );

        // Write content that was just received from remote — should be suppressed.
        cb.write(ClipboardContent::Text("remote-text".into())).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Write new content — should NOT be suppressed.
        cb.write(ClipboardContent::Text("local-text".into())).unwrap();

        let got = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got, ClipboardContent::Text("local-text".into()));

        let _ = stop_tx.send(true);
    }
}
