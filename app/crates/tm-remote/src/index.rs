//! Which chapters the NAS holds, ported from `RemoteIndex.kt`: one listing per manga folder,
//! fetched on demand, kept in memory and saved to disk so a restart does not re-list everything.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Mutex;

use crate::webdav::WebDav;
use crate::Result;

/// The answer for one chapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    /// The NAS holds it under this file name.
    Found(String),
    /// The folder was listed and the chapter is not there.
    Absent,
    /// The folder has not been listed yet; call [`RemoteIndex::refresh`].
    Unknown,
}

/// `<source dir>/<manga dir>` → chapter file names.
type Entries = BTreeMap<String, BTreeSet<String>>;

#[derive(Debug)]
pub struct RemoteIndex {
    file: PathBuf,
    entries: Mutex<Entries>,
}

pub fn key_of(source_dir: &str, manga_dir: &str) -> String {
    format!("{source_dir}/{manga_dir}")
}

impl RemoteIndex {
    /// Loads the saved index from `file` (missing or unreadable means empty).
    pub fn open(file: impl Into<PathBuf>) -> Self {
        let file = file.into();
        let entries = std::fs::read(&file)
            .ok()
            .and_then(|b| serde_json::from_slice::<BTreeMap<String, Vec<String>>>(&b).ok())
            .map(|m| {
                m.into_iter()
                    .map(|(k, v)| (k, v.into_iter().collect()))
                    .collect()
            })
            .unwrap_or_default();
        Self {
            file,
            entries: Mutex::new(entries),
        }
    }

    fn with<T>(&self, f: impl FnOnce(&mut Entries) -> T) -> T {
        // A poisoned lock only means another thread panicked mid-update; the map is still usable.
        let mut g = self
            .entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        f(&mut g)
    }

    fn save(&self) {
        let snapshot = self.with(|e| e.clone());
        let write = || -> std::io::Result<()> {
            if let Some(dir) = self.file.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let part = self.file.with_extension("json.part");
            let body = serde_json::to_vec_pretty(&snapshot).map_err(std::io::Error::other)?;
            std::fs::write(&part, body)?;
            std::fs::rename(&part, &self.file)
        };
        if let Err(e) = write() {
            tracing::warn!("remote index save failed: {e}");
        }
    }

    /// The first of `candidates` the NAS holds.
    pub fn resolve(&self, source_dir: &str, manga_dir: &str, candidates: &[String]) -> Lookup {
        self.with(|e| match e.get(&key_of(source_dir, manga_dir)) {
            None => Lookup::Unknown,
            Some(set) => candidates
                .iter()
                .find(|c| set.contains(c.as_str()))
                .map_or(Lookup::Absent, |c| Lookup::Found(c.clone())),
        })
    }

    /// How many chapter files one manga folder holds, if listed.
    pub fn count_for(&self, source_dir: &str, manga_dir: &str) -> Option<usize> {
        self.with(|e| e.get(&key_of(source_dir, manga_dir)).map(BTreeSet::len))
    }

    /// Lists one manga folder now and stores the files in it. Returns how many there are.
    pub async fn refresh(&self, dav: &WebDav, source_dir: &str, manga_dir: &str) -> Result<usize> {
        let listing = dav
            .list(&[source_dir.to_owned(), manga_dir.to_owned()])
            .await?;
        let names: BTreeSet<String> = listing
            .into_iter()
            .filter(|e| !e.is_dir)
            .map(|e| e.name)
            .collect();
        let n = names.len();
        self.with(|e| e.insert(key_of(source_dir, manga_dir), names));
        self.save();
        Ok(n)
    }

    /// Any source folder holding this manga folder and one of the names, for when the source's
    /// name is not known: `(source dir, file name)`.
    pub fn find_across_sources(
        &self,
        manga_dir: &str,
        candidates: &[String],
    ) -> Option<(String, String)> {
        let suffix = format!("/{manga_dir}");
        self.with(|e| {
            e.iter().find_map(|(k, files)| {
                let source = k.strip_suffix(&suffix)?;
                let name = candidates.iter().find(|c| files.contains(c.as_str()))?;
                Some((source.to_owned(), name.clone()))
            })
        })
    }

    /// Records an upload to `<source>/<manga>/<file>` (only when that folder is already known, as
    /// the Kotlin index did; an unknown folder is listed on next use).
    pub fn on_uploaded(&self, segments: &[String]) {
        let [source, manga, file] = segments else {
            return;
        };
        let changed = self.with(|e| match e.get_mut(&key_of(source, manga)) {
            Some(set) => set.insert(file.clone()),
            None => false,
        });
        if changed {
            self.save();
        }
    }

    pub fn on_deleted(&self, segments: &[String]) {
        let [source, manga, file] = segments else {
            return;
        };
        let changed = self.with(|e| {
            e.get_mut(&key_of(source, manga))
                .is_some_and(|s| s.remove(file))
        });
        if changed {
            self.save();
        }
    }

    /// Forget everything (the server or folder changed).
    pub fn invalidate(&self) {
        self.with(BTreeMap::clear);
        let _ = std::fs::remove_file(&self.file);
    }
}
