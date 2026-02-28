use openclipboard_ffi::{
    clipboard_node_new_with_sync_discovery, identity_generate, trust_store_open, EventHandler,
};
use openclipboard_core::MockDiscovery;
use std::sync::{mpsc, Arc, Mutex};
use tempfile::TempDir;

#[derive(Clone)]
struct TestHandler {
    got_text_tx: Arc<Mutex<Option<mpsc::Sender<(String, String)>>>>,
    connected_tx: Arc<Mutex<Option<mpsc::Sender<()>>>>,
    errors: Arc<Mutex<Vec<String>>>,
}

impl TestHandler {
    fn with_senders(tx: mpsc::Sender<(String, String)>, connected_tx: mpsc::Sender<()>) -> Self {
        Self {
            got_text_tx: Arc::new(Mutex::new(Some(tx))),
            connected_tx: Arc::new(Mutex::new(Some(connected_tx))),
            errors: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl Default for TestHandler {
    fn default() -> Self {
        Self {
            got_text_tx: Arc::new(Mutex::new(None)),
            connected_tx: Arc::new(Mutex::new(None)),
            errors: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl EventHandler for TestHandler {
    fn on_clipboard_text(&self, peer_id: String, text: String, _ts_ms: u64) {
        if let Some(tx) = self.got_text_tx.lock().unwrap().as_ref() {
            let _ = tx.send((peer_id, text));
        }
    }

    fn on_file_received(&self, _peer_id: String, _name: String, _data_path: String) {}

    fn on_peer_connected(&self, _peer_id: String) {
        if let Some(tx) = self.connected_tx.lock().unwrap().as_ref() {
            let _ = tx.send(());
        }
    }

    fn on_peer_disconnected(&self, _peer_id: String) {}

    fn on_error(&self, message: String) {
        self.errors.lock().unwrap().push(message);
    }
}

#[test]
fn ffi_phase3_start_sync_and_cliptext_roundtrip_with_mock_discovery() {
    let td = TempDir::new().unwrap();

    let a_id_path = td.path().join("a_identity.json").to_string_lossy().to_string();
    let b_id_path = td.path().join("b_identity.json").to_string_lossy().to_string();

    let a_trust_path = td.path().join("a_trust.json").to_string_lossy().to_string();
    let b_trust_path = td.path().join("b_trust.json").to_string_lossy().to_string();

    // Pre-create identities so we can trust-pin by pubkey.
    let id_a = identity_generate();
    let id_b = identity_generate();
    id_a.save(a_id_path.clone()).unwrap();
    id_b.save(b_id_path.clone()).unwrap();

    let trust_a = trust_store_open(a_trust_path.clone()).unwrap();
    let trust_b = trust_store_open(b_trust_path.clone()).unwrap();

    trust_a
        .add(id_b.peer_id(), id_b.pubkey_b64(), "b".into())
        .unwrap();
    trust_b
        .add(id_a.peer_id(), id_a.pubkey_b64(), "a".into())
        .unwrap();

    // Deterministic discovery via mock injection.
    let shared = Arc::new(MockDiscovery::new_shared());
    let disc_a = Arc::new(shared.clone_shared());
    let disc_b = Arc::new(shared.clone_shared());

    let node_a = clipboard_node_new_with_sync_discovery(
        a_id_path,
        a_trust_path,
        disc_a,
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
    )
    .unwrap();
    let node_b = clipboard_node_new_with_sync_discovery(
        b_id_path,
        b_trust_path,
        disc_b,
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
    )
    .unwrap();

    let (tx, rx) = mpsc::channel::<(String, String)>();
    let (connected_tx, connected_rx) = mpsc::channel::<()>();
    let handler_b = TestHandler::with_senders(tx, connected_tx);

    // Track connection events on both sides so we don't broadcast before the peer handle exists.
    let (tx_a_unused, _rx_a_unused) = mpsc::channel::<(String, String)>();
    let (connected_tx_a, connected_rx_a) = mpsc::channel::<()>();
    let handler_a = TestHandler::with_senders(tx_a_unused, connected_tx_a);

    node_b
        .start_sync(0, "b".into(), Box::new(handler_b.clone()))
        .unwrap();
    node_a
        .start_sync(0, "a".into(), Box::new(handler_a.clone()))
        .unwrap();

    // Wait for both sides to observe the connection before broadcasting.
    let _ = connected_rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap_or_else(|_| {
        panic!(
            "node_b did not observe peer connect within timeout; errors={:?}",
            handler_b.errors.lock().unwrap()
        )
    });
    let _ = connected_rx_a.recv_timeout(std::time::Duration::from_secs(2)).unwrap_or_else(|_| {
        panic!(
            "node_a did not observe peer connect within timeout; errors={:?}",
            handler_a.errors.lock().unwrap()
        )
    });

    node_a.send_clipboard_text("hello".into()).unwrap();

    let (from_peer, text) = rx
        .recv_timeout(std::time::Duration::from_secs(3))
        .unwrap_or_else(|_| {
            panic!(
                "did not receive cliptext via handler within timeout; errors={:?}",
                handler_b.errors.lock().unwrap()
            )
        });

    assert_eq!(text, "hello");
    assert_eq!(from_peer, node_a.peer_id());

    node_a.stop_sync();
    node_b.stop_sync();
}
