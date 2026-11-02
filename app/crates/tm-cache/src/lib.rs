//! A size-capped LRU disk cache for page and cover bytes (`<data>/cache/pages/`).
//!
//! - Keys are hashed (SHA-256) to file names, so any string is a safe key.
//! - `index.json` keeps size, last access and pin state; it is rebuilt from the files if lost.
//! - Writes go to a temp file and are renamed into place, so a crash never leaves a half file.
//! - When the total exceeds the cap, least recently used unpinned entries are evicted.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("cache I/O error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cache lock poisoned")]
    Poisoned,
}

pub type Result<T> = std::result::Result<T, CacheError>;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
struct Entry {
    size: u64,
    /// Monotonic access counter (not wall-clock), so ordering is exact and testable.
    tick: u64,
    #[serde(default)]
    pinned: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Index {
    tick: u64,
    entries: HashMap<String, Entry>,
}

/// Usage numbers for the Settings screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Usage {
    pub bytes: u64,
    pub entries: usize,
    pub cap_bytes: u64,
}

#[derive(Debug)]
pub struct DiskCache {
    dir: PathBuf,
    cap: Mutex<u64>,
    index: Mutex<Index>,
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> CacheError + '_ {
    move |source| CacheError::Io {
        path: path.to_path_buf(),
        source,
    }
}

pub fn hash_key(key: &str) -> String {
    let d = Sha256::digest(key.as_bytes());
    d.iter().map(|b| format!("{b:02x}")).collect()
}

impl DiskCache {
    /// Open (or create) a cache in `dir` capped at `cap_bytes`.
    pub fn open(dir: &Path, cap_bytes: u64) -> Result<Self> {
        std::fs::create_dir_all(dir).map_err(io(dir))?;
        let index_path = dir.join("index.json");
        let mut index: Index = std::fs::read(&index_path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        // Reconcile with the files on disk: drop entries whose file is gone, adopt orphans,
        // delete leftover temp files from a crash.
        let mut on_disk: HashMap<String, u64> = HashMap::new();
        for e in std::fs::read_dir(dir).map_err(io(dir))?.flatten() {
            let Some(name) = e.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if name.ends_with(".tmp") {
                let _ = std::fs::remove_file(e.path());
                continue;
            }
            if name.len() == 64 && name.bytes().all(|b| b.is_ascii_hexdigit()) {
                let size = e.metadata().map(|m| m.len()).unwrap_or(0);
                on_disk.insert(name, size);
            }
        }
        index.entries.retain(|k, _| on_disk.contains_key(k));
        for (k, size) in on_disk {
            index.entries.entry(k).or_insert(Entry {
                size,
                tick: 0,
                pinned: false,
            });
        }
        let cache = Self {
            dir: dir.to_path_buf(),
            cap: Mutex::new(cap_bytes),
            index: Mutex::new(index),
        };
        cache.evict_and_save()?;
        Ok(cache)
    }

    fn path_of(&self, hashed: &str) -> PathBuf {
        self.dir.join(hashed)
    }

    /// Bytes for `key`, marking it recently used.
    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let h = hash_key(key);
        let p = self.path_of(&h);
        {
            let mut idx = self.index.lock().map_err(|_| CacheError::Poisoned)?;
            if !idx.entries.contains_key(&h) {
                return Ok(None);
            }
            idx.tick = idx.tick.saturating_add(1);
            let tick = idx.tick;
            if let Some(e) = idx.entries.get_mut(&h) {
                e.tick = tick;
            }
        }
        match std::fs::read(&p) {
            Ok(b) => Ok(Some(b)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let mut idx = self.index.lock().map_err(|_| CacheError::Poisoned)?;
                idx.entries.remove(&h);
                Ok(None)
            }
            Err(e) => Err(io(&p)(e)),
        }
    }

    pub fn contains(&self, key: &str) -> bool {
        let h = hash_key(key);
        self.index
            .lock()
            .map(|i| i.entries.contains_key(&h))
            .unwrap_or(false)
    }

    /// Store `bytes` under `key`, then evict to the cap.
    pub fn put(&self, key: &str, bytes: &[u8]) -> Result<()> {
        let h = hash_key(key);
        let p = self.path_of(&h);
        let tmp = self.dir.join(format!("{h}.tmp"));
        std::fs::write(&tmp, bytes).map_err(io(&tmp))?;
        std::fs::rename(&tmp, &p).map_err(io(&p))?;
        {
            let mut idx = self.index.lock().map_err(|_| CacheError::Poisoned)?;
            idx.tick = idx.tick.saturating_add(1);
            let tick = idx.tick;
            let pinned = idx.entries.get(&h).is_some_and(|e| e.pinned);
            idx.entries.insert(
                h,
                Entry {
                    size: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
                    tick,
                    pinned,
                },
            );
        }
        self.evict_and_save()
    }

    /// Pinned entries are never evicted (pinned manga's pages).
    pub fn set_pinned(&self, key: &str, pinned: bool) -> Result<()> {
        let h = hash_key(key);
        {
            let mut idx = self.index.lock().map_err(|_| CacheError::Poisoned)?;
            if let Some(e) = idx.entries.get_mut(&h) {
                e.pinned = pinned;
            }
        }
        self.evict_and_save()
    }

    pub fn remove(&self, key: &str) -> Result<()> {
        let h = hash_key(key);
        let p = self.path_of(&h);
        let _ = std::fs::remove_file(&p);
        let mut idx = self.index.lock().map_err(|_| CacheError::Poisoned)?;
        idx.entries.remove(&h);
        drop(idx);
        self.save()
    }

    /// Delete every unpinned entry.
    pub fn clear(&self) -> Result<()> {
        let mut idx = self.index.lock().map_err(|_| CacheError::Poisoned)?;
        let victims: Vec<String> = idx
            .entries
            .iter()
            .filter(|(_, e)| !e.pinned)
            .map(|(k, _)| k.clone())
            .collect();
        for k in victims {
            let _ = std::fs::remove_file(self.path_of(&k));
            idx.entries.remove(&k);
        }
        drop(idx);
        self.save()
    }

    pub fn set_cap(&self, cap_bytes: u64) -> Result<()> {
        *self.cap.lock().map_err(|_| CacheError::Poisoned)? = cap_bytes;
        self.evict_and_save()
    }

    pub fn usage(&self) -> Result<Usage> {
        let idx = self.index.lock().map_err(|_| CacheError::Poisoned)?;
        let cap = *self.cap.lock().map_err(|_| CacheError::Poisoned)?;
        Ok(Usage {
            bytes: idx
                .entries
                .values()
                .fold(0u64, |a, e| a.saturating_add(e.size)),
            entries: idx.entries.len(),
            cap_bytes: cap,
        })
    }

    fn evict_and_save(&self) -> Result<()> {
        let cap = *self.cap.lock().map_err(|_| CacheError::Poisoned)?;
        {
            let mut idx = self.index.lock().map_err(|_| CacheError::Poisoned)?;
            let mut total = idx
                .entries
                .values()
                .fold(0u64, |a, e| a.saturating_add(e.size));
            if total > cap {
                let mut order: Vec<(String, Entry)> = idx
                    .entries
                    .iter()
                    .filter(|(_, e)| !e.pinned)
                    .map(|(k, e)| (k.clone(), *e))
                    .collect();
                order.sort_by_key(|(_, e)| e.tick);
                for (k, e) in order {
                    if total <= cap {
                        break;
                    }
                    let _ = std::fs::remove_file(self.path_of(&k));
                    idx.entries.remove(&k);
                    total = total.saturating_sub(e.size);
                }
            }
        }
        self.save()
    }

    fn save(&self) -> Result<()> {
        let idx = self.index.lock().map_err(|_| CacheError::Poisoned)?;
        let text = serde_json::to_vec(&*idx).unwrap_or_default();
        drop(idx);
        let p = self.dir.join("index.json");
        let tmp = self.dir.join("index.json.tmp");
        std::fs::write(&tmp, text).map_err(io(&tmp))?;
        std::fs::rename(&tmp, &p).map_err(io(&p))
    }
}

/// Milliseconds since the epoch (helper for callers that log cache times).
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_get_and_lru_eviction() {
        let dir = tempfile::tempdir().unwrap();
        let c = DiskCache::open(dir.path(), 10).unwrap();
        c.put("a", &[0; 4]).unwrap();
        c.put("b", &[0; 4]).unwrap();
        assert!(c.get("a").unwrap().is_some()); // a is now more recent than b
        c.put("c", &[0; 4]).unwrap(); // 12 > 10: evict LRU = b
        assert!(c.contains("a"));
        assert!(!c.contains("b"));
        assert!(c.contains("c"));
        assert_eq!(c.usage().unwrap().bytes, 8);
    }

    #[test]
    fn pinned_entries_survive_eviction_and_clear() {
        let dir = tempfile::tempdir().unwrap();
        let c = DiskCache::open(dir.path(), 5).unwrap();
        c.put("keep", &[0; 4]).unwrap();
        c.set_pinned("keep", true).unwrap();
        c.put("x", &[0; 4]).unwrap(); // over cap: x is the only unpinned entry
        assert!(c.contains("keep"));
        assert!(!c.contains("x"));
        c.put("y", &[0; 1]).unwrap();
        c.clear().unwrap();
        assert!(c.contains("keep"));
        assert!(!c.contains("y"));
    }

    #[test]
    fn survives_reopen_and_lost_index_and_crash_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        {
            let c = DiskCache::open(dir.path(), 100).unwrap();
            c.put("k", b"hello").unwrap();
        }
        std::fs::remove_file(dir.path().join("index.json")).unwrap();
        std::fs::write(dir.path().join("deadbeef.tmp"), b"half").unwrap();
        let c = DiskCache::open(dir.path(), 100).unwrap();
        assert_eq!(c.get("k").unwrap().as_deref(), Some(&b"hello"[..]));
        assert!(!dir.path().join("deadbeef.tmp").exists());
    }

    #[test]
    fn shrinking_the_cap_evicts() {
        let dir = tempfile::tempdir().unwrap();
        let c = DiskCache::open(dir.path(), 100).unwrap();
        for k in ["1", "2", "3"] {
            c.put(k, &[0; 10]).unwrap();
        }
        c.set_cap(15).unwrap();
        assert_eq!(c.usage().unwrap().entries, 1);
        assert!(c.contains("3"));
    }
}
