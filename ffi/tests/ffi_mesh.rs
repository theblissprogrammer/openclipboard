//! E2E test: 3-node mesh clipboard sync.
//!
//! Starts 3 nodes (A, B, C) with mutual trust, starts mesh mode with MockClipboard,
//! simulates a clipboard copy on A, and verifies that both B and C receive it.

use openclipboard_ffi::{
    clipboard_node_new_with_sync_discovery, identity_generate, trust_store_open, EventHandler,
};
use openclipboard_core::clipboard::MockClipboard;
use openclipboard_core::{ClipboardContent, ClipboardProvider, MockDiscovery};
use std::sync::{mpsc, Arc, Mutex};
use tempfile::TempDir;

#[derive(Clone)]
struct TestHandler {
    label: String,
    got_text_tx: Arc<Mutex<Option<mpsc::Sender<(String, String)>>>>,
    connected_tx: Arc<Mutex<Option<mpsc::Sender<String>>>>,
    errors: Arc<Mutex<Vec<String>>>,
}

impl TestHandler {
    fn new(label: &str, text_tx: mpsc::Sender<(String, String)>, conn_tx: mpsc::Sender<String>) -> Self {
        Self {
            label: label.into(),
            got_text_tx: Arc::new(Mutex::new(Some(text_tx))),
            connected_tx: Arc::new(Mutex::new(Some(conn_tx))),
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
    fn on_peer_connected(&self, peer_id: String) {
        if let Some(tx) = self.connected_tx.lock().unwrap().as_ref() {
            let _ = tx.send(peer_id);
        }
    }
    fn on_peer_disconnected(&self, _peer_id: String) {}
    fn on_error(&self, message: String) {
        self.errors.lock().unwrap().push(format!("[{}] {}", self.label, message));
    }
}

#[test]
fn mesh_three_node_clipboard_fanout() {
    let td = TempDir::new().unwrap();

    // Generate 3 identities.
    let ids: Vec<_> = (0..3)
        .map(|i| {
            let id = identity_generate();
            let path = td.path().join(format!("id_{i}.json")).to_string_lossy().to_string();
            id.save(path).unwrap();
            id
        })
        .collect();

    // Set up full-mesh trust (everyone trusts everyone else).
    let trust_paths: Vec<String> = (0..3)
        .map(|i| td.path().join(format!("trust_{i}.json")).to_string_lossy().to_string())
        .collect();

    for i in 0..3 {
        let store = trust_store_open(trust_paths[i].clone()).unwrap();
        for j in 0..3 {
            if i == j { continue; }
            store.add(ids[j].peer_id(), ids[j].pubkey_b64(), format!("node_{j}")).unwrap();
        }
    }

    // Shared mock discovery.
    let shared_disc = Arc::new(MockDiscovery::new_shared());

    // Create 3 nodes.
    let nodes: Vec<_> = (0..3)
        .map(|i| {
            let id_path = td.path().join(format!("id_{i}.json")).to_string_lossy().to_string();
            let disc = Arc::new(shared_disc.clone_shared());
            clipboard_node_new_with_sync_discovery(
                id_path,
                trust_paths[i].clone(),
                disc,
                std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            )
            .unwrap()
        })
        .collect();

    // Set up channels for receiving clipboard text on nodes B and C.
    let (tx_b, rx_b) = mpsc::channel::<(String, String)>();
    let (conn_tx_b, conn_rx_b) = mpsc::channel::<String>();
    let handler_b = TestHandler::new("B", tx_b, conn_tx_b);

    let (tx_c, rx_c) = mpsc::channel::<(String, String)>();
    let (conn_tx_c, conn_rx_c) = mpsc::channel::<String>();
    let handler_c = TestHandler::new("C", tx_c, conn_tx_c);

    // Node A handler (we don't need its text events, but need connection events).
    let (tx_a_unused, _rx_a_unused) = mpsc::channel::<(String, String)>();
    let (conn_tx_a, conn_rx_a) = mpsc::channel::<String>();
    let handler_a = TestHandler::new("A", tx_a_unused, conn_tx_a);

    // Start sync on all 3 nodes.
    nodes[0].start_sync(0, "A".into(), Box::new(handler_a.clone())).unwrap();
    nodes[1].start_sync(0, "B".into(), Box::new(handler_b.clone())).unwrap();
    nodes[2].start_sync(0, "C".into(), Box::new(handler_c.clone())).unwrap();

    // Wait for connections to establish (each node should connect to 2 peers).
    let timeout = std::time::Duration::from_secs(5);

    // Wait for A to see at least 2 connections.
    let mut a_conns = 0;
    while a_conns < 2 {
        conn_rx_a.recv_timeout(timeout).unwrap_or_else(|_| {
            panic!("Node A didn't get enough connections (got {}); errors_a={:?}, errors_b={:?}, errors_c={:?}",
                a_conns,
                handler_a.errors.lock().unwrap(),
                handler_b.errors.lock().unwrap(),
                handler_c.errors.lock().unwrap(),
            )
        });
        a_conns += 1;
    }

    // Wait for B and C to also be connected.
    let mut b_conns = 0;
    while b_conns < 2 {
        conn_rx_b.recv_timeout(timeout).unwrap_or_else(|_| {
            panic!("Node B didn't get enough connections (got {b_conns})")
        });
        b_conns += 1;
    }
    let mut c_conns = 0;
    while c_conns < 2 {
        conn_rx_c.recv_timeout(timeout).unwrap_or_else(|_| {
            panic!("Node C didn't get enough connections (got {c_conns})")
        });
        c_conns += 1;
    }

    // Node A broadcasts clipboard text.
    nodes[0].send_clipboard_text("mesh-hello".into()).unwrap();

    // Both B and C should receive it.
    let (from_b, text_b) = rx_b.recv_timeout(std::time::Duration::from_secs(3))
        .unwrap_or_else(|_| panic!("B didn't receive text; errors={:?}", handler_b.errors.lock().unwrap()));
    assert_eq!(text_b, "mesh-hello");
    assert_eq!(from_b, nodes[0].peer_id());

    let (from_c, text_c) = rx_c.recv_timeout(std::time::Duration::from_secs(3))
        .unwrap_or_else(|_| panic!("C didn't receive text; errors={:?}", handler_c.errors.lock().unwrap()));
    assert_eq!(text_c, "mesh-hello");
    assert_eq!(from_c, nodes[0].peer_id());

    // Cleanup.
    for n in &nodes {
        n.stop_sync();
    }
}

#[test]
fn mesh_three_node_no_echo_loop() {
    // Verify that when A sends, B receives it, but B doesn't echo it back to A or C.
    // This test is the same setup but we verify A does NOT get its own text back.
    let td = TempDir::new().unwrap();

    let ids: Vec<_> = (0..3)
        .map(|i| {
            let id = identity_generate();
            let path = td.path().join(format!("id_{i}.json")).to_string_lossy().to_string();
            id.save(path).unwrap();
            id
        })
        .collect();

    let trust_paths: Vec<String> = (0..3)
        .map(|i| td.path().join(format!("trust_{i}.json")).to_string_lossy().to_string())
        .collect();

    for i in 0..3 {
        let store = trust_store_open(trust_paths[i].clone()).unwrap();
        for j in 0..3 {
            if i == j { continue; }
            store.add(ids[j].peer_id(), ids[j].pubkey_b64(), format!("node_{j}")).unwrap();
        }
    }

    let shared_disc = Arc::new(MockDiscovery::new_shared());

    let nodes: Vec<_> = (0..3)
        .map(|i| {
            let id_path = td.path().join(format!("id_{i}.json")).to_string_lossy().to_string();
            let disc = Arc::new(shared_disc.clone_shared());
            clipboard_node_new_with_sync_discovery(
                id_path,
                trust_paths[i].clone(),
                disc,
                std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            )
            .unwrap()
        })
        .collect();

    let (tx_a, rx_a) = mpsc::channel::<(String, String)>();
    let (conn_tx_a, conn_rx_a) = mpsc::channel::<String>();
    let handler_a = TestHandler::new("A", tx_a, conn_tx_a);

    let (tx_b, rx_b) = mpsc::channel::<(String, String)>();
    let (conn_tx_b, conn_rx_b) = mpsc::channel::<String>();
    let handler_b = TestHandler::new("B", tx_b, conn_tx_b);

    let (tx_c, _rx_c) = mpsc::channel::<(String, String)>();
    let (conn_tx_c, conn_rx_c) = mpsc::channel::<String>();
    let handler_c = TestHandler::new("C", tx_c, conn_tx_c);

    nodes[0].start_sync(0, "A".into(), Box::new(handler_a.clone())).unwrap();
    nodes[1].start_sync(0, "B".into(), Box::new(handler_b.clone())).unwrap();
    nodes[2].start_sync(0, "C".into(), Box::new(handler_c.clone())).unwrap();

    let timeout = std::time::Duration::from_secs(5);
    // Wait for full mesh.
    for _ in 0..2 { conn_rx_a.recv_timeout(timeout).unwrap(); }
    for _ in 0..2 { conn_rx_b.recv_timeout(timeout).unwrap(); }
    for _ in 0..2 { conn_rx_c.recv_timeout(timeout).unwrap(); }

    // A broadcasts.
    nodes[0].send_clipboard_text("no-echo".into()).unwrap();

    // B should receive.
    let (_, text) = rx_b.recv_timeout(std::time::Duration::from_secs(3)).unwrap();
    assert_eq!(text, "no-echo");

    // A should NOT receive its own text back (wait briefly to confirm no echo).
    let result = rx_a.recv_timeout(std::time::Duration::from_millis(500));
    assert!(result.is_err(), "A should not receive its own broadcast back, but got: {:?}", result.unwrap());

    for n in &nodes {
        n.stop_sync();
    }
}
