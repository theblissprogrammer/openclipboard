//! Extended FFI-level tests for clipboard history and recall.

use openclipboard_ffi::*;
use openclipboard_core::clipboard::MockClipboard;
use openclipboard_core::{ClipboardContent, ClipboardProvider, MockDiscovery};
use std::sync::{mpsc, Arc, Mutex};
use tempfile::TempDir;

struct TestClipboard {
    content: Mutex<Option<String>>,
}

impl TestClipboard {
    fn new() -> Self {
        Self { content: Mutex::new(None) }
    }
    fn get(&self) -> Option<String> {
        self.content.lock().unwrap().clone()
    }
}

impl ClipboardCallback for TestClipboard {
    fn read_text(&self) -> Option<String> {
        self.content.lock().unwrap().clone()
    }
    fn write_text(&self, text: String) {
        *self.content.lock().unwrap() = Some(text);
    }
}

struct NoopHandler;
impl EventHandler for NoopHandler {
    fn on_clipboard_text(&self, _: String, _: String, _: u64) {}
    fn on_file_received(&self, _: String, _: String, _: String) {}
    fn on_peer_connected(&self, _: String) {}
    fn on_peer_disconnected(&self, _: String) {}
    fn on_error(&self, _: String) {}
}

fn make_mesh_node(dir: &std::path::Path) -> (Arc<ClipboardNode>, Arc<TestClipboard>) {
    let id_path = dir.join("id.json").to_string_lossy().to_string();
    let trust_path = dir.join("trust").to_string_lossy().to_string();
    let node = clipboard_node_new(id_path, trust_path).unwrap();
    let cb = Arc::new(TestClipboard::new());

    // We need to use a struct that implements ClipboardCallback, not the Arc
    let cb_for_node = TestClipboard::new();

    node.start_mesh(0, "test".into(), Box::new(NoopHandler), Box::new(cb_for_node), 50).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    (node, cb)
}

fn make_node(dir: &std::path::Path) -> Arc<ClipboardNode> {
    let id_path = dir.join("id.json").to_string_lossy().to_string();
    let trust_path = dir.join("trust").to_string_lossy().to_string();
    clipboard_node_new(id_path, trust_path).unwrap()
}

#[test]
fn history_empty_without_mesh() {
    let dir = tempfile::tempdir().unwrap();
    let node = make_node(dir.path());
    assert!(node.get_clipboard_history(10).is_empty());
    assert!(node.get_clipboard_history(0).is_empty());
    assert!(node.get_clipboard_history_for_peer("any".into(), 10).is_empty());
}

#[test]
fn history_limit_zero_returns_empty() {
    let dir = tempfile::tempdir().unwrap();
    let (node, _cb) = make_mesh_node(dir.path());
    assert!(node.get_clipboard_history(0).is_empty());
    node.stop();
}

#[test]
fn recall_without_mesh_returns_error() {
    let dir = tempfile::tempdir().unwrap();
    let node = make_node(dir.path());
    let result = node.recall_from_history("some-id".into());
    assert!(result.is_err());
}

#[test]
fn recall_invalid_id_returns_error() {
    let dir = tempfile::tempdir().unwrap();
    let (node, _cb) = make_mesh_node(dir.path());
    let result = node.recall_from_history("nonexistent-id".into());
    assert!(result.is_err());
    node.stop();
}

#[test]
fn history_for_unknown_peer_returns_empty() {
    let dir = tempfile::tempdir().unwrap();
    let (node, _cb) = make_mesh_node(dir.path());
    let entries = node.get_clipboard_history_for_peer("unknown-peer".into(), 10);
    assert!(entries.is_empty());
    node.stop();
}

// ─── Mesh integration: history recorded on receive ───────────────────────────

#[derive(Clone)]
struct RecordingHandler {
    texts: Arc<Mutex<Vec<(String, String)>>>,
    connected: Arc<Mutex<Vec<String>>>,
}

impl RecordingHandler {
    fn new() -> Self {
        Self {
            texts: Arc::new(Mutex::new(vec![])),
            connected: Arc::new(Mutex::new(vec![])),
        }
    }
}

impl EventHandler for RecordingHandler {
    fn on_clipboard_text(&self, peer_id: String, text: String, _: u64) {
        self.texts.lock().unwrap().push((peer_id, text));
    }
    fn on_file_received(&self, _: String, _: String, _: String) {}
    fn on_peer_connected(&self, peer_id: String) {
        self.connected.lock().unwrap().push(peer_id);
    }
    fn on_peer_disconnected(&self, _: String) {}
    fn on_error(&self, _msg: String) {}
}

#[test]
fn mesh_history_recorded_on_receive() {
    let td = TempDir::new().unwrap();

    // Create 2 nodes with mutual trust
    let ids: Vec<_> = (0..2).map(|i| {
        let id = identity_generate();
        id.save(td.path().join(format!("id_{i}.json")).to_string_lossy().to_string()).unwrap();
        id
    }).collect();

    let trust_paths: Vec<String> = (0..2)
        .map(|i| td.path().join(format!("trust_{i}.json")).to_string_lossy().to_string())
        .collect();

    for i in 0..2 {
        let store = trust_store_open(trust_paths[i].clone()).unwrap();
        for j in 0..2 {
            if i == j { continue; }
            store.add(ids[j].peer_id(), ids[j].pubkey_b64(), format!("node_{j}")).unwrap();
        }
    }

    let shared_disc = Arc::new(MockDiscovery::new_shared());

    let nodes: Vec<_> = (0..2).map(|i| {
        let id_path = td.path().join(format!("id_{i}.json")).to_string_lossy().to_string();
        let disc = Arc::new(shared_disc.clone_shared());
        clipboard_node_new_with_sync_discovery(
            id_path, trust_paths[i].clone(), disc,
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        ).unwrap()
    }).collect();

    let (conn_tx_a, conn_rx_a) = mpsc::channel::<String>();
    let (conn_tx_b, conn_rx_b) = mpsc::channel::<String>();

    let handler_a = RecordingHandler::new();
    let handler_b = RecordingHandler::new();

    // Use sync (not mesh) so we can test history via the sync service directly
    nodes[0].start_sync(0, "A".into(), Box::new({
        let h = handler_a.clone();
        struct H { h: RecordingHandler, tx: mpsc::Sender<String> }
        impl EventHandler for H {
            fn on_clipboard_text(&self, p: String, t: String, ts: u64) { self.h.on_clipboard_text(p, t, ts); }
            fn on_file_received(&self, _: String, _: String, _: String) {}
            fn on_peer_connected(&self, p: String) { self.h.on_peer_connected(p.clone()); let _ = self.tx.send(p); }
            fn on_peer_disconnected(&self, _: String) {}
            fn on_error(&self, _: String) {}
        }
        H { h, tx: conn_tx_a }
    })).unwrap();

    nodes[1].start_sync(0, "B".into(), Box::new({
        let h = handler_b.clone();
        struct H2 { h: RecordingHandler, tx: mpsc::Sender<String> }
        impl EventHandler for H2 {
            fn on_clipboard_text(&self, p: String, t: String, ts: u64) { self.h.on_clipboard_text(p, t, ts); }
            fn on_file_received(&self, _: String, _: String, _: String) {}
            fn on_peer_connected(&self, p: String) { self.h.on_peer_connected(p.clone()); let _ = self.tx.send(p); }
            fn on_peer_disconnected(&self, _: String) {}
            fn on_error(&self, _: String) {}
        }
        H2 { h, tx: conn_tx_b }
    })).unwrap();

    // Wait for connection
    let timeout = std::time::Duration::from_secs(5);
    conn_rx_a.recv_timeout(timeout).unwrap();
    conn_rx_b.recv_timeout(timeout).unwrap();

    // A sends text
    nodes[0].send_clipboard_text("history-test".into()).unwrap();

    // Wait for B to receive
    std::thread::sleep(std::time::Duration::from_millis(500));

    // B's history should have the entry
    let b_history = nodes[1].get_clipboard_history(10);
    assert!(!b_history.is_empty(), "B should have history entries");
    assert_eq!(b_history[0].content, "history-test");

    // B's history for peer A
    let b_peer_history = nodes[1].get_clipboard_history_for_peer(nodes[0].peer_id(), 10);
    assert_eq!(b_peer_history.len(), 1);
    assert_eq!(b_peer_history[0].content, "history-test");

    // B's history for unknown peer
    let empty = nodes[1].get_clipboard_history_for_peer("unknown".into(), 10);
    assert!(empty.is_empty());

    for n in &nodes {
        n.stop_sync();
    }
}
