//! Exhaustive tests for ClipboardHistory.

use openclipboard_core::{ClipboardHistory, ClipboardEntry};
use std::sync::Arc;
use std::thread;

#[test]
fn empty_history_returns_empty() {
    let h = ClipboardHistory::new(100);
    assert_eq!(h.len(), 0);
    assert!(h.get_recent(10).is_empty());
    assert!(h.get_for_peer("any", 10).is_empty());
    assert!(h.get_by_id("anything").is_none());
}

#[test]
fn record_single_entry() {
    let h = ClipboardHistory::new(100);
    let id = h.record("hello".into(), "local".into());
    assert!(!id.is_empty());
    assert_eq!(h.len(), 1);

    let entries = h.get_recent(10);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].content, "hello");
    assert_eq!(entries[0].source_peer, "local");
    assert_eq!(entries[0].id, id);
}

#[test]
fn record_multiple_entries_ordering() {
    let h = ClipboardHistory::new(100);
    h.record("first".into(), "a".into());
    h.record("second".into(), "b".into());
    h.record("third".into(), "c".into());

    let entries = h.get_recent(10);
    assert_eq!(entries.len(), 3);
    // Newest first
    assert_eq!(entries[0].content, "third");
    assert_eq!(entries[1].content, "second");
    assert_eq!(entries[2].content, "first");
}

#[test]
fn max_capacity_eviction() {
    let h = ClipboardHistory::new(3);
    let id_a = h.record("a".into(), "p".into());
    h.record("b".into(), "p".into());
    h.record("c".into(), "p".into());
    assert_eq!(h.len(), 3);

    h.record("d".into(), "p".into());
    assert_eq!(h.len(), 3);

    // "a" should be evicted
    assert!(h.get_by_id(&id_a).is_none());

    let entries = h.get_recent(10);
    let contents: Vec<&str> = entries.iter().map(|e| e.content.as_str()).collect();
    assert_eq!(contents, vec!["d", "c", "b"]);
}

#[test]
fn capacity_of_one() {
    let h = ClipboardHistory::new(1);
    h.record("a".into(), "p".into());
    h.record("b".into(), "p".into());
    assert_eq!(h.len(), 1);
    assert_eq!(h.get_recent(10)[0].content, "b");
}

#[test]
fn capacity_zero_clamps_to_one() {
    // new(0) should clamp to max(1)
    let h = ClipboardHistory::new(0);
    h.record("a".into(), "p".into());
    assert_eq!(h.len(), 1);
    h.record("b".into(), "p".into());
    assert_eq!(h.len(), 1);
    assert_eq!(h.get_recent(10)[0].content, "b");
}

#[test]
fn peer_filtering_multiple_peers() {
    let h = ClipboardHistory::new(100);
    h.record("a1".into(), "alice".into());
    h.record("b1".into(), "bob".into());
    h.record("a2".into(), "alice".into());
    h.record("b2".into(), "bob".into());
    h.record("c1".into(), "charlie".into());

    let alice = h.get_for_peer("alice", 10);
    assert_eq!(alice.len(), 2);
    assert_eq!(alice[0].content, "a2");
    assert_eq!(alice[1].content, "a1");

    let bob = h.get_for_peer("bob", 10);
    assert_eq!(bob.len(), 2);
    assert_eq!(bob[0].content, "b2");

    let charlie = h.get_for_peer("charlie", 10);
    assert_eq!(charlie.len(), 1);
}

#[test]
fn unknown_peer_returns_empty() {
    let h = ClipboardHistory::new(100);
    h.record("x".into(), "known".into());
    assert!(h.get_for_peer("unknown", 10).is_empty());
}

#[test]
fn get_by_id_valid() {
    let h = ClipboardHistory::new(100);
    let id = h.record("target".into(), "p".into());
    let entry = h.get_by_id(&id).unwrap();
    assert_eq!(entry.content, "target");
    assert_eq!(entry.id, id);
}

#[test]
fn get_by_id_invalid() {
    let h = ClipboardHistory::new(100);
    h.record("x".into(), "p".into());
    assert!(h.get_by_id("nonexistent-id").is_none());
}

#[test]
fn get_by_id_after_eviction() {
    let h = ClipboardHistory::new(2);
    let id1 = h.record("first".into(), "p".into());
    h.record("second".into(), "p".into());
    h.record("third".into(), "p".into());
    // id1 should be evicted
    assert!(h.get_by_id(&id1).is_none());
}

#[test]
fn limit_zero() {
    let h = ClipboardHistory::new(100);
    h.record("a".into(), "p".into());
    h.record("b".into(), "p".into());
    assert!(h.get_recent(0).is_empty());
    assert!(h.get_for_peer("p", 0).is_empty());
}

#[test]
fn limit_one() {
    let h = ClipboardHistory::new(100);
    h.record("a".into(), "p".into());
    h.record("b".into(), "p".into());
    let entries = h.get_recent(1);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].content, "b");
}

#[test]
fn limit_more_than_available() {
    let h = ClipboardHistory::new(100);
    h.record("a".into(), "p".into());
    h.record("b".into(), "p".into());
    let entries = h.get_recent(999);
    assert_eq!(entries.len(), 2);
}

#[test]
fn limit_exact_count() {
    let h = ClipboardHistory::new(100);
    for i in 0..5 {
        h.record(format!("item{i}"), "p".into());
    }
    let entries = h.get_recent(5);
    assert_eq!(entries.len(), 5);
}

#[test]
fn concurrent_access() {
    let h = Arc::new(ClipboardHistory::new(1000));
    let mut handles = vec![];

    for t in 0..10 {
        let h = Arc::clone(&h);
        handles.push(thread::spawn(move || {
            for i in 0..100 {
                h.record(format!("t{t}-i{i}"), format!("peer{t}"));
            }
        }));
    }

    for handle in handles {
        handle.join().unwrap();
    }

    // All 1000 entries should be recorded
    assert_eq!(h.len(), 1000);
    let all = h.get_recent(1000);
    assert_eq!(all.len(), 1000);
}

#[test]
fn concurrent_access_with_eviction() {
    let h = Arc::new(ClipboardHistory::new(50));
    let mut handles = vec![];

    for t in 0..10 {
        let h = Arc::clone(&h);
        handles.push(thread::spawn(move || {
            for i in 0..100 {
                h.record(format!("t{t}-i{i}"), "p".into());
            }
        }));
    }

    for handle in handles {
        handle.join().unwrap();
    }

    // Should never exceed capacity
    assert!(h.len() <= 50);
}

#[test]
fn timestamps_are_monotonic() {
    let h = ClipboardHistory::new(100);
    for i in 0..20 {
        h.record(format!("item{i}"), "p".into());
    }

    let entries = h.get_recent(20);
    // entries are newest first, so timestamps should be non-increasing
    for window in entries.windows(2) {
        assert!(window[0].timestamp >= window[1].timestamp,
            "timestamps not monotonic: {} < {}", window[0].timestamp, window[1].timestamp);
    }
}

#[test]
fn timestamps_are_reasonable() {
    let h = ClipboardHistory::new(100);
    h.record("test".into(), "p".into());

    let entry = h.get_recent(1)[0].clone();
    // Timestamp should be after 2024-01-01 and before 2030-01-01 (reasonable range)
    assert!(entry.timestamp > 1_704_067_200_000, "timestamp too old: {}", entry.timestamp);
    assert!(entry.timestamp < 1_893_456_000_000, "timestamp too far in future: {}", entry.timestamp);
}

#[test]
fn content_special_characters() {
    let h = ClipboardHistory::new(100);

    let special = "héllo wörld 🎉 \n\t\r\0 日本語 <script>alert('xss')</script>";
    let id = h.record(special.into(), "p".into());
    let entry = h.get_by_id(&id).unwrap();
    assert_eq!(entry.content, special);
}

#[test]
fn content_empty_string() {
    let h = ClipboardHistory::new(100);
    let id = h.record(String::new(), "p".into());
    let entry = h.get_by_id(&id).unwrap();
    assert_eq!(entry.content, "");
}

#[test]
fn content_very_long_string() {
    let h = ClipboardHistory::new(100);
    let long = "x".repeat(1_000_000);
    let id = h.record(long.clone(), "p".into());
    let entry = h.get_by_id(&id).unwrap();
    assert_eq!(entry.content.len(), 1_000_000);
    assert_eq!(entry.content, long);
}

#[test]
fn unique_ids() {
    let h = ClipboardHistory::new(1000);
    let mut ids = vec![];
    for i in 0..100 {
        ids.push(h.record(format!("item{i}"), "p".into()));
    }
    // All IDs should be unique
    let mut sorted = ids.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), ids.len());
}

#[test]
fn peer_filter_with_limit() {
    let h = ClipboardHistory::new(100);
    for i in 0..10 {
        h.record(format!("a{i}"), "alice".into());
        h.record(format!("b{i}"), "bob".into());
    }

    let alice_3 = h.get_for_peer("alice", 3);
    assert_eq!(alice_3.len(), 3);
    assert_eq!(alice_3[0].content, "a9");
    assert_eq!(alice_3[1].content, "a8");
    assert_eq!(alice_3[2].content, "a7");
}
