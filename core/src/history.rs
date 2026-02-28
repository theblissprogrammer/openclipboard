//! Bounded, thread-safe clipboard history store.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// A single clipboard history entry.
#[derive(Debug, Clone)]
pub struct ClipboardEntry {
    pub id: String,
    pub content: String,
    pub source_peer: String,
    pub timestamp: u64,
}

/// Thread-safe bounded clipboard history.
pub struct ClipboardHistory {
    max_entries: usize,
    entries: Mutex<VecDeque<ClipboardEntry>>,
}

impl ClipboardHistory {
    pub fn new(max_entries: usize) -> Self {
        Self {
            max_entries: max_entries.max(1),
            entries: Mutex::new(VecDeque::new()),
        }
    }

    /// Record a clipboard event. Returns the generated entry id.
    pub fn record(&self, content: String, source_peer: String) -> String {
        let id = format!("{:032x}", rand::random::<u128>());
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let entry = ClipboardEntry {
            id: id.clone(),
            content,
            source_peer,
            timestamp,
        };

        let mut entries = self.entries.lock().unwrap();
        entries.push_back(entry);
        while entries.len() > self.max_entries {
            entries.pop_front();
        }

        id
    }

    /// Get most recent entries (newest first), up to `limit`.
    pub fn get_recent(&self, limit: usize) -> Vec<ClipboardEntry> {
        let entries = self.entries.lock().unwrap();
        entries.iter().rev().take(limit).cloned().collect()
    }

    /// Get most recent entries for a specific peer (newest first), up to `limit`.
    pub fn get_for_peer(&self, peer_name: &str, limit: usize) -> Vec<ClipboardEntry> {
        let entries = self.entries.lock().unwrap();
        entries
            .iter()
            .rev()
            .filter(|e| e.source_peer == peer_name)
            .take(limit)
            .cloned()
            .collect()
    }

    /// Look up an entry by id.
    pub fn get_by_id(&self, id: &str) -> Option<ClipboardEntry> {
        let entries = self.entries.lock().unwrap();
        entries.iter().find(|e| e.id == id).cloned()
    }

    /// Current number of entries stored.
    pub fn len(&self) -> usize {
        self.entries.lock().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_retrieve() {
        let h = ClipboardHistory::new(100);
        h.record("hello".into(), "local".into());
        h.record("world".into(), "phone".into());

        let all = h.get_recent(10);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].content, "world"); // newest first
        assert_eq!(all[1].content, "hello");
    }

    #[test]
    fn filter_by_peer() {
        let h = ClipboardHistory::new(100);
        h.record("a".into(), "local".into());
        h.record("b".into(), "phone".into());
        h.record("c".into(), "local".into());

        let local = h.get_for_peer("local", 10);
        assert_eq!(local.len(), 2);
        assert_eq!(local[0].content, "c");
        assert_eq!(local[1].content, "a");

        let phone = h.get_for_peer("phone", 10);
        assert_eq!(phone.len(), 1);
        assert_eq!(phone[0].content, "b");
    }

    #[test]
    fn eviction_when_full() {
        let h = ClipboardHistory::new(3);
        h.record("a".into(), "local".into());
        h.record("b".into(), "local".into());
        h.record("c".into(), "local".into());
        assert_eq!(h.len(), 3);

        h.record("d".into(), "local".into());
        assert_eq!(h.len(), 3);

        let all = h.get_recent(10);
        assert_eq!(all[0].content, "d");
        assert_eq!(all[1].content, "c");
        assert_eq!(all[2].content, "b");
        // "a" was evicted
    }

    #[test]
    fn get_by_id() {
        let h = ClipboardHistory::new(100);
        let id = h.record("findme".into(), "local".into());
        let entry = h.get_by_id(&id).unwrap();
        assert_eq!(entry.content, "findme");
        assert!(h.get_by_id("nonexistent").is_none());
    }

    #[test]
    fn limit_works() {
        let h = ClipboardHistory::new(100);
        for i in 0..10 {
            h.record(format!("item{i}"), "local".into());
        }
        let limited = h.get_recent(3);
        assert_eq!(limited.len(), 3);
    }
}
