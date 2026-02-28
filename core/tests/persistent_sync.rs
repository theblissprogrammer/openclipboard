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

    // Loopback should be fast, but CI runners can be noisy.
    // Keep a strict target locally; allow a slightly larger window under CI to avoid flakes.
    let max_wait = if std::env::var("CI").is_ok() { std::time::Duration::from_secs(3) } else { std::time::Duration::from_secs(1) };

    let mut got = false;
    while t0.elapsed() < max_wait {
        let texts = h2.texts.lock().unwrap().clone();
        if texts.iter().any(|(_, t)| t == "hello") {
            got = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        got,
        "did not receive cliptext within {:?}; errors={:?}",
        max_wait,
        h2.errors.lock().unwrap()
    );

    // send again to ensure the connection is still alive.
    got = false;
    s1.broadcast_clip_text("hello2".to_string()).await;
    let start2 = std::time::Instant::now();
    while start2.elapsed() < max_wait {
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

#[tokio::test]
async fn quic_persistent_dedup_only_one_side_dials() {
    let disc1 = MockDiscovery::new_shared();
    let disc2 = disc1.clone_shared();

    // Choose peer IDs that make the dial rule deterministic.
    // Dial rule: only the lexicographically smaller peer_id dials.
    let id_small = Ed25519Identity::generate();
    let id_big = Ed25519Identity::generate();
    let (id1, id2) = if id_small.peer_id().to_string() <= id_big.peer_id().to_string() {
        (id_small, id_big)
    } else {
        (id_big, id_small)
    };

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
    )
    .unwrap();

    let s2 = SyncService::new(
        id2.clone(),
        trust2,
        Arc::new(MemoryReplayProtector::new(1024)),
        Arc::new(disc2),
        SocketAddr::from(([127, 0, 0, 1], 0)),
        "dev2".into(),
        h2.clone(),
    )
    .unwrap();

    s1.start().await.unwrap();
    s2.start().await.unwrap();

    // Let discovery + dial settle and guard against duplicate connections.
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Each side should report at most one connection event for the peer.
    // If both sides dial (or inbound isn't rejected), we can get multiple connects.
    assert!(
        h1.connected.lock().unwrap().len() <= 1,
        "unexpected duplicate connects on node1: {:?}",
        h1.connected.lock().unwrap()
    );
    assert!(
        h2.connected.lock().unwrap().len() <= 1,
        "unexpected duplicate connects on node2: {:?}",
        h2.connected.lock().unwrap()
    );

    s1.stop().await;
    s2.stop().await;
}

#[tokio::test]
async fn quic_persistent_reconnects_after_remote_restart() {
    let shared = MockDiscovery::new_shared();

    // Ensure the "local" node is the dialer (lexicographically smaller peer_id),
    // otherwise it will never attempt outbound reconnects.
    let id_a = Ed25519Identity::generate();
    let id_b = Ed25519Identity::generate();
    let (dialer_id, acceptor_id) = if id_a.peer_id().to_string() <= id_b.peer_id().to_string() {
        (id_a, id_b)
    } else {
        (id_b, id_a)
    };

    let dialer_disc = shared.clone_shared();
    let acceptor_disc = shared.clone_shared();

    let dialer_trust = Arc::new(MemoryTrustStore::new());
    let acceptor_trust = Arc::new(MemoryTrustStore::new());
    trust_each_other(&dialer_id, &acceptor_id, &dialer_trust, "acceptor");
    trust_each_other(&acceptor_id, &dialer_id, &acceptor_trust, "dialer");

    let dialer_h = Arc::new(TestHandler::default());
    let acceptor_h = Arc::new(TestHandler::default());

    let dialer = SyncService::new(
        dialer_id.clone(),
        dialer_trust,
        Arc::new(MemoryReplayProtector::new(1024)),
        Arc::new(dialer_disc),
        SocketAddr::from(([127, 0, 0, 1], 0)),
        "dialer".into(),
        dialer_h.clone(),
    )
    .unwrap();

    let acceptor = SyncService::new(
        acceptor_id.clone(),
        acceptor_trust.clone(),
        Arc::new(MemoryReplayProtector::new(1024)),
        Arc::new(acceptor_disc),
        SocketAddr::from(([127, 0, 0, 1], 0)),
        "acceptor".into(),
        acceptor_h.clone(),
    )
    .unwrap();

    dialer.start().await.unwrap();
    acceptor.start().await.unwrap();

    // Wait for initial connection.
    let start = std::time::Instant::now();
    while start.elapsed() < std::time::Duration::from_secs(2) {
        if !dialer_h.connected.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(!dialer_h.connected.lock().unwrap().is_empty(), "expected initial connect");

    // Stop remote.
    acceptor.stop().await;

    // Give the dialer a moment to observe disconnect and begin retrying.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Restart remote with same identity/trust and a fresh handler.
    let acceptor_h2 = Arc::new(TestHandler::default());
    let acceptor_disc2 = shared.clone_shared();
    let acceptor2 = SyncService::new(
        acceptor_id.clone(),
        acceptor_trust,
        Arc::new(MemoryReplayProtector::new(1024)),
        Arc::new(acceptor_disc2),
        SocketAddr::from(([127, 0, 0, 1], 0)),
        "acceptor".into(),
        acceptor_h2.clone(),
    )
    .unwrap();
    acceptor2.start().await.unwrap();

    // Expect reconnection within a reasonable window (backoff starts at 200ms).
    let start2 = std::time::Instant::now();
    while start2.elapsed() < std::time::Duration::from_secs(6) {
        if !acceptor_h2.connected.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    assert!(
        !acceptor_h2.connected.lock().unwrap().is_empty(),
        "expected reconnect on restarted node; dialer_errors={:?}",
        dialer_h.errors.lock().unwrap()
    );

    // Functional check after reconnect.
    dialer.broadcast_clip_text("after-restart".into()).await;
    let start3 = std::time::Instant::now();
    let mut got = false;
    while start3.elapsed() < std::time::Duration::from_secs(3) {
        if acceptor_h2
            .texts
            .lock()
            .unwrap()
            .iter()
            .any(|(_, t)| t == "after-restart")
        {
            got = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    dialer.stop().await;
    acceptor2.stop().await;

    assert!(got, "expected cliptext after reconnect");
}
