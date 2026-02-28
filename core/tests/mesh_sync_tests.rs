//! Tests for mesh sync: EchoSuppressor, PeerRegistry, clipboard watcher behavior.

use openclipboard_core::{
    ClipboardHistory, ClipboardContent, ClipboardProvider, EchoSuppressor,
    PeerRegistry, PeerStatus, PeerEntry,
    mesh::start_clipboard_watcher,
    clipboard::MockClipboard,
};
use openclipboard_core::trust::{MemoryTrustStore, TrustStore, TrustRecord};
use std::sync::Arc;
use tokio::sync::Mutex;

// ─── EchoSuppressor ──────────────────────────────────────────────────────────

#[test]
fn echo_suppressor_empty() {
    let s = EchoSuppressor::new(5);
    assert!(!s.should_ignore_local_change("anything"));
}

#[test]
fn echo_suppressor_single() {
    let mut s = EchoSuppressor::new(5);
    s.note_remote_write("hello");
    assert!(s.should_ignore_local_change("hello"));
    assert!(!s.should_ignore_local_change("world"));
}

#[test]
fn echo_suppressor_eviction() {
    let mut s = EchoSuppressor::new(2);
    s.note_remote_write("a");
    s.note_remote_write("b");
    s.note_remote_write("c");
    // "a" should be evicted (cap=2)
    assert!(!s.should_ignore_local_change("a"));
    assert!(s.should_ignore_local_change("b"));
    assert!(s.should_ignore_local_change("c"));
}

#[test]
fn echo_suppressor_dedup_consecutive() {
    let mut s = EchoSuppressor::new(3);
    s.note_remote_write("same");
    s.note_remote_write("same");
    s.note_remote_write("same");
    // Should still only have 1 entry due to consecutive dedup
    s.note_remote_write("other1");
    s.note_remote_write("other2");
    // "same" should still be there (cap=3, only 3 unique entries)
    assert!(s.should_ignore_local_change("same"));
}

#[test]
fn echo_suppressor_cap_one() {
    let mut s = EchoSuppressor::new(1);
    s.note_remote_write("a");
    assert!(s.should_ignore_local_change("a"));
    s.note_remote_write("b");
    assert!(!s.should_ignore_local_change("a"));
    assert!(s.should_ignore_local_change("b"));
}

#[test]
fn echo_suppressor_cap_zero_clamps() {
    let mut s = EchoSuppressor::new(0); // clamps to 1
    s.note_remote_write("x");
    assert!(s.should_ignore_local_change("x"));
}

// ─── PeerRegistry ────────────────────────────────────────────────────────────

#[tokio::test]
async fn peer_registry_empty() {
    let reg = PeerRegistry::new();
    assert!(reg.list_all().await.is_empty());
    assert!(reg.list_online().await.is_empty());
    assert!(reg.get("nonexistent").await.is_none());
}

#[tokio::test]
async fn peer_registry_load_from_trust() {
    let reg = PeerRegistry::new();
    let store = MemoryTrustStore::new();
    store.save(TrustRecord {
        peer_id: "p1".into(),
        identity_pk: vec![1],
        display_name: "Peer1".into(),
        created_at: chrono::Utc::now(),
    }).unwrap();

    reg.load_from_trust(&store).await.unwrap();
    assert_eq!(reg.list_all().await.len(), 1);

    let entry = reg.get("p1").await.unwrap();
    assert_eq!(entry.display_name, "Peer1");
    assert_eq!(entry.status, PeerStatus::Offline);
}

#[tokio::test]
async fn peer_registry_online_offline_toggle() {
    let reg = PeerRegistry::new();
    let store = MemoryTrustStore::new();
    store.save(TrustRecord {
        peer_id: "p1".into(),
        identity_pk: vec![1],
        display_name: "P1".into(),
        created_at: chrono::Utc::now(),
    }).unwrap();
    reg.load_from_trust(&store).await.unwrap();

    assert_eq!(reg.list_online().await.len(), 0);
    reg.set_online("p1", Some("1.2.3.4:5000".into())).await;
    assert_eq!(reg.list_online().await.len(), 1);

    let entry = reg.get("p1").await.unwrap();
    assert_eq!(entry.status, PeerStatus::Online);
    assert_eq!(entry.last_addr.as_deref(), Some("1.2.3.4:5000"));

    reg.set_offline("p1").await;
    assert_eq!(reg.list_online().await.len(), 0);
    assert_eq!(reg.get("p1").await.unwrap().status, PeerStatus::Offline);
}

#[tokio::test]
async fn peer_registry_set_online_unknown_peer_is_noop() {
    let reg = PeerRegistry::new();
    // Setting an unknown peer online should not crash or add it
    reg.set_online("unknown", None).await;
    assert!(reg.list_all().await.is_empty());
}

#[tokio::test]
async fn peer_registry_multiple_peers() {
    let reg = PeerRegistry::new();
    let store = MemoryTrustStore::new();
    for i in 0..5 {
        store.save(TrustRecord {
            peer_id: format!("p{i}"),
            identity_pk: vec![i as u8],
            display_name: format!("Peer{i}"),
            created_at: chrono::Utc::now(),
        }).unwrap();
    }
    reg.load_from_trust(&store).await.unwrap();
    assert_eq!(reg.list_all().await.len(), 5);

    reg.set_online("p1", None).await;
    reg.set_online("p3", None).await;
    assert_eq!(reg.list_online().await.len(), 2);
}

// ─── Clipboard Watcher ──────────────────────────────────────────────────────

#[tokio::test]
async fn watcher_ignores_empty_clipboard() {
    let cb = Arc::new(MockClipboard::new());
    let suppressor = Arc::new(Mutex::new(EchoSuppressor::new(8)));
    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let _handle = start_clipboard_watcher(
        cb.clone(),
        suppressor,
        std::time::Duration::from_millis(30),
        stop_rx,
        move |content| { let _ = tx.send(content); },
    );

    // Don't write anything — clipboard stays empty
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    // Should have received nothing
    assert!(rx.try_recv().is_err());
    let _ = stop_tx.send(true);
}

#[tokio::test]
async fn watcher_deduplicates_same_content() {
    let cb = Arc::new(MockClipboard::new());
    let suppressor = Arc::new(Mutex::new(EchoSuppressor::new(8)));
    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let _handle = start_clipboard_watcher(
        cb.clone(),
        suppressor,
        std::time::Duration::from_millis(30),
        stop_rx,
        move |content| { let _ = tx.send(content); },
    );

    cb.write(ClipboardContent::Text("same".into())).unwrap();

    // Wait for first detection
    let got = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
        .await.unwrap().unwrap();
    assert_eq!(got, ClipboardContent::Text("same".into()));

    // Wait a bit — no second notification should come
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    assert!(rx.try_recv().is_err());

    let _ = stop_tx.send(true);
}

#[tokio::test]
async fn watcher_detects_multiple_changes() {
    let cb = Arc::new(MockClipboard::new());
    let suppressor = Arc::new(Mutex::new(EchoSuppressor::new(8)));
    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let _handle = start_clipboard_watcher(
        cb.clone(),
        suppressor,
        std::time::Duration::from_millis(30),
        stop_rx,
        move |content| { let _ = tx.send(content); },
    );

    cb.write(ClipboardContent::Text("first".into())).unwrap();
    let got = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
        .await.unwrap().unwrap();
    assert_eq!(got, ClipboardContent::Text("first".into()));

    cb.write(ClipboardContent::Text("second".into())).unwrap();
    let got = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
        .await.unwrap().unwrap();
    assert_eq!(got, ClipboardContent::Text("second".into()));

    let _ = stop_tx.send(true);
}

// ─── History integration with recording ──────────────────────────────────────

#[test]
fn history_records_from_multiple_peers() {
    let h = ClipboardHistory::new(100);
    h.record("from-phone".into(), "phone".into());
    h.record("from-laptop".into(), "laptop".into());
    h.record("from-phone2".into(), "phone".into());

    assert_eq!(h.len(), 3);
    assert_eq!(h.get_for_peer("phone", 10).len(), 2);
    assert_eq!(h.get_for_peer("laptop", 10).len(), 1);
}

#[test]
fn history_eviction_preserves_newest() {
    let h = ClipboardHistory::new(5);
    let mut ids = vec![];
    for i in 0..10 {
        ids.push(h.record(format!("item{i}"), "p".into()));
    }

    assert_eq!(h.len(), 5);
    // First 5 should be evicted
    for id in &ids[..5] {
        assert!(h.get_by_id(id).is_none());
    }
    // Last 5 should remain
    for id in &ids[5..] {
        assert!(h.get_by_id(id).is_some());
    }
}
