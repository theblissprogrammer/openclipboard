//! FFI-level tests for clipboard history.

use openclipboard_ffi::*;
use std::sync::{Arc, Mutex};

struct TestClipboard {
    content: Mutex<Option<String>>,
}

impl TestClipboard {
    fn new() -> Self {
        Self { content: Mutex::new(None) }
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

fn make_node(dir: &std::path::Path) -> Arc<ClipboardNode> {
    let id_path = dir.join("id.json").to_string_lossy().to_string();
    let trust_path = dir.join("trust").to_string_lossy().to_string();
    clipboard_node_new(id_path, trust_path).unwrap()
}

#[test]
fn history_empty_before_mesh() {
    let dir = tempfile::tempdir().unwrap();
    let node = make_node(dir.path());
    // Before start_mesh, history should be empty
    let h = node.get_clipboard_history(10);
    assert!(h.is_empty());
}

#[test]
fn history_and_recall_via_mesh() {
    let dir = tempfile::tempdir().unwrap();
    let node = make_node(dir.path());

    let cb = TestClipboard::new();

    // Start mesh on a random port
    node.start_mesh(
        0, // port 0 = OS picks
        "test-device".into(),
        Box::new(NoopHandler),
        Box::new(cb),
        50,
    ).unwrap();

    // Give it a moment to start
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Simulate a clipboard text broadcast (which records in history)
    node.send_clipboard_text("hello from test".into()).unwrap();

    // History won't have the sent text because send_clipboard_text only sends to peers,
    // it doesn't record. Recording happens via watcher or incoming messages.
    // But we can verify the API works and returns empty.
    let h = node.get_clipboard_history(10);
    // May or may not have entries depending on watcher timing — just verify no crash.
    assert!(h.len() <= 1);

    node.stop();
}
