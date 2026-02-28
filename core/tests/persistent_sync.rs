use openclipboard_core::{Ed25519Identity, IdentityProvider, MemoryReplayProtector, MemoryTrustStore, SyncHandler, SyncService, TrustRecord, TrustStore, MockDiscovery};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct TestHandler {
    texts: Mutex<Vec<(String, String)>>,
    connected: Mutex<Vec<String>>,
    disconnected: Mutex<Vec<String>>,
    errors: Mutex<Vec<String>>,
}

impl SyncHandler for TestHandler {
    fn on_clipboard_text(&self, peer_id: String, text: String, _ts_ms: u64) {
        self.texts.lock().unwrap().push((peer_id, text));
    }

    fn on_peer_connected(&self, peer_id: String) {
        self.connected.lock().unwrap().push(peer_id);
    }

    fn on_peer_disconnected(&self, peer_id: String) {
        self.disconnected.lock().unwrap().push(peer_id);
    }

    fn on_error(&self, message: String) {
        self.errors.lock().unwrap().push(message);
    }
}

fn trust_each_other(a: &Ed25519Identity, b: &Ed25519Identity, store: &MemoryTrustStore, name: &str) {
    store.save(TrustRecord {
        peer_id: b.peer_id().to_string(),
        identity_pk: b.public_key_bytes(),
        display_name: name.to_string(),
        created_at: chrono::Utc::now(),
    }).unwrap();
}

#[tokio::test]
async fn quic_persistent_cliptext_sync_under_1s_loopback() {
    let disc1 = MockDiscovery::new_shared();
    let disc2 = disc1.clone_shared();

    let id1 = Ed25519Identity::generate();
    let id2 = Ed25519Identity::generate();

    let trust1 = Arc::new(MemoryTrustStore::new());
    let trust2 = Arc::new(MemoryTrustStore::new());
    trust_each_other(&id1, &id2, &trust1, "peer2");
    trust_each_other(&id2, &id1, &trust2, "peer1");

    let h1 = Arc::new(TestHandler::default());
    let h2 = Arc::new(TestHandler::default());

    let s1 = SyncService::new(
        id1.clone(),
        trust1,
        Arc::new(MemoryReplayProtector::new(1024)),
        Arc::new(disc1),
        SocketAddr::from(([127, 0, 0, 1], 0)),
        "dev1".into(),
        h1.clone(),
    ).unwrap();

    let s2 = SyncService::new(
        id2.clone(),
        trust2,
        Arc::new(MemoryReplayProtector::new(1024)),
        Arc::new(disc2),
        SocketAddr::from(([127, 0, 0, 1], 0)),
        "dev2".into(),
        h2.clone(),
    ).unwrap();

    s1.start().await.unwrap();
    s2.start().await.unwrap();

    // Wait for connection establishment on both sides.
    let start = std::time::Instant::now();
    while start.elapsed() < std::time::Duration::from_secs(2) {
        let c1 = !h1.connected.lock().unwrap().is_empty();
        let c2 = !h2.connected.lock().unwrap().is_empty();
        if c1 && c2 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    let t0 = std::time::Instant::now();
    s1.broadcast_clip_text("hello".to_string()).await;

    // should arrive under 1s (loopback should be fast)
    let mut got = false;
    while t0.elapsed() < std::time::Duration::from_secs(1) {
        let texts = h2.texts.lock().unwrap().clone();
        if texts.iter().any(|(_, t)| t == "hello") {
            got = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(got, "did not receive cliptext within 1s; errors={:?}", h2.errors.lock().unwrap());

    // send again to ensure the connection is still alive.
    s1.broadcast_clip_text("hello2".to_string()).await;
    let start2 = std::time::Instant::now();
    while start2.elapsed() < std::time::Duration::from_secs(1) {
        let texts = h2.texts.lock().unwrap().clone();
        if texts.iter().any(|(_, t)| t == "hello2") {
            got = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    s1.stop().await;
    s2.stop().await;

    assert!(got);
}
