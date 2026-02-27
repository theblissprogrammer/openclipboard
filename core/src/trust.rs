//! Trust store: manage trusted peers (paired devices).

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// A record of a trusted peer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustRecord {
    pub peer_id: String,
    pub identity_pk: Vec<u8>,
    pub display_name: String,
    pub created_at: DateTime<Utc>,
}

/// Persistent trust storage.
pub trait TrustStore: Send + Sync {
    fn save(&self, record: TrustRecord) -> Result<()>;
    fn get(&self, peer_id: &str) -> Result<Option<TrustRecord>>;
    fn list(&self) -> Result<Vec<TrustRecord>>;
    fn remove(&self, peer_id: &str) -> Result<bool>;

    fn is_trusted(&self, peer_id: &str) -> Result<bool> {
        Ok(self.get(peer_id)?.is_some())
    }
}

/// In-memory trust store (useful for tests).
#[derive(Default)]
pub struct MemoryTrustStore {
    records: Mutex<HashMap<String, TrustRecord>>,
}

impl MemoryTrustStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl TrustStore for MemoryTrustStore {
    fn save(&self, record: TrustRecord) -> Result<()> {
        self.records.lock().unwrap().insert(record.peer_id.clone(), record);
        Ok(())
    }

    fn get(&self, peer_id: &str) -> Result<Option<TrustRecord>> {
        Ok(self.records.lock().unwrap().get(peer_id).cloned())
    }

    fn list(&self) -> Result<Vec<TrustRecord>> {
        Ok(self.records.lock().unwrap().values().cloned().collect())
    }

    fn remove(&self, peer_id: &str) -> Result<bool> {
        Ok(self.records.lock().unwrap().remove(peer_id).is_some())
    }
}

/// File-backed trust store (JSON file).
///
/// Format: an array of `TrustRecord`.
pub struct FileTrustStore {
    path: PathBuf,
    cache: Mutex<HashMap<String, TrustRecord>>,
}

impl FileTrustStore {
    pub fn new(path: PathBuf) -> Result<Self> {
        let cache = if path.exists() {
            let data = std::fs::read_to_string(&path)?;
            let records: Vec<TrustRecord> = serde_json::from_str(&data)?;
            records.into_iter().map(|r| (r.peer_id.clone(), r)).collect()
        } else {
            HashMap::new()
        };

        Ok(Self {
            path,
            cache: Mutex::new(cache),
        })
    }

    fn flush(&self) -> Result<()> {
        let records: Vec<TrustRecord> = self.cache.lock().unwrap().values().cloned().collect();
        let data = serde_json::to_string_pretty(&records)?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.path, data)?;
        Ok(())
    }
}

impl TrustStore for FileTrustStore {
    fn save(&self, record: TrustRecord) -> Result<()> {
        self.cache.lock().unwrap().insert(record.peer_id.clone(), record);
        self.flush()
    }

    fn get(&self, peer_id: &str) -> Result<Option<TrustRecord>> {
        Ok(self.cache.lock().unwrap().get(peer_id).cloned())
    }

    fn list(&self) -> Result<Vec<TrustRecord>> {
        Ok(self.cache.lock().unwrap().values().cloned().collect())
    }

    fn remove(&self, peer_id: &str) -> Result<bool> {
        let removed = self.cache.lock().unwrap().remove(peer_id).is_some();
        if removed {
            self.flush()?;
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_crud() {
        let store = MemoryTrustStore::new();
        let record = TrustRecord {
            peer_id: "peer-a".into(),
            identity_pk: vec![1, 2, 3],
            display_name: "Alice".into(),
            created_at: Utc::now(),
        };

        store.save(record.clone()).unwrap();
        assert!(store.is_trusted("peer-a").unwrap());
        assert!(!store.is_trusted("peer-b").unwrap());

        let got = store.get("peer-a").unwrap().unwrap();
        assert_eq!(got, record);

        let list = store.list().unwrap();
        assert_eq!(list.len(), 1);

        assert!(store.remove("peer-a").unwrap());
        assert!(!store.remove("peer-a").unwrap());
        assert!(!store.is_trusted("peer-a").unwrap());
    }

    #[test]
    fn file_store_persist_roundtrip() {
        let base = std::env::temp_dir().join(format!(
            "openclipboard_trust_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let path = base.join("trust.json");

        // Write
        {
            let store = FileTrustStore::new(path.clone()).unwrap();
            store
                .save(TrustRecord {
                    peer_id: "peer-x".into(),
                    identity_pk: vec![9, 8, 7],
                    display_name: "Xavier".into(),
                    created_at: Utc::now(),
                })
                .unwrap();
            assert!(store.is_trusted("peer-x").unwrap());
        }

        // Read
        {
            let store = FileTrustStore::new(path.clone()).unwrap();
            let rec = store.get("peer-x").unwrap().unwrap();
            assert_eq!(rec.display_name, "Xavier");
        }

        let _ = std::fs::remove_dir_all(base);
    }
}
