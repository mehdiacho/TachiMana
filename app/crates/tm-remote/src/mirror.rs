//! Reading from the NAS, ported from `RemoteMirror.kt`: a chapter archive is fetched into
//! `cache/remote_mirror/<sha256(path)>.cbz`, checked against the server's size, and the folder is
//! trimmed to the `remoteStorageCacheMb` budget, oldest first.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use sha2::{Digest, Sha256};

use crate::index::{Lookup, RemoteIndex};
use crate::webdav::WebDav;
use crate::{names, Error, Result};

/// Largest chapter archive fetched.
const MAX_ARCHIVE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct Mirror {
    cache_dir: PathBuf,
    budget_bytes: u64,
}

/// What the mirror needs to know about a chapter.
#[derive(Debug, Clone)]
pub struct ChapterRef<'a> {
    /// The source's display name, when the extension is known.
    pub source_name: Option<&'a str>,
    pub source_id: i64,
    pub manga_title: &'a str,
    pub chapter_name: &'a str,
    pub scanlator: Option<&'a str>,
    pub chapter_url: &'a str,
}

impl Mirror {
    pub fn new(cache_dir: impl Into<PathBuf>, budget_mb: u64) -> Self {
        Self {
            cache_dir: cache_dir.into(),
            budget_bytes: budget_mb.saturating_mul(1024 * 1024),
        }
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// `<sha256 of the joined segments>.cbz` in the cache folder.
    pub fn cache_file_for(&self, segments: &[String]) -> PathBuf {
        let hex: String = Sha256::digest(segments.join("/").as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        self.cache_dir.join(format!("{hex}.cbz"))
    }

    /// Where a chapter lives on the NAS, by the index. `None` when the index says it is absent;
    /// an unlisted folder gives the best-guess current name.
    pub fn segments_for(&self, index: &RemoteIndex, c: &ChapterRef<'_>) -> Option<Vec<String>> {
        let manga_dir = names::manga_dir_name(c.manga_title);
        let candidates = names::chapter_file_names(c.chapter_name, c.scanlator, c.chapter_url);
        if let Some(src) = c.source_name {
            let source_dir = names::source_dir_name(src);
            match index.resolve(&source_dir, &manga_dir, &candidates) {
                Lookup::Found(name) => return Some(vec![source_dir, manga_dir, name]),
                Lookup::Absent => {}
                Lookup::Unknown => {
                    return candidates
                        .into_iter()
                        .next()
                        .map(|n| vec![source_dir, manga_dir, n])
                }
            }
        }
        index
            .find_across_sources(&manga_dir, &candidates)
            .map(|(source_dir, name)| vec![source_dir, manga_dir, name])
    }

    /// Whether the index knows the NAS holds this chapter.
    pub fn holds(&self, index: &RemoteIndex, c: &ChapterRef<'_>) -> bool {
        let manga_dir = names::manga_dir_name(c.manga_title);
        let candidates = names::chapter_file_names(c.chapter_name, c.scanlator, c.chapter_url);
        if let Some(src) = c.source_name {
            if matches!(
                index.resolve(&names::source_dir_name(src), &manga_dir, &candidates),
                Lookup::Found(_)
            ) {
                return true;
            }
        }
        index.find_across_sources(&manga_dir, &candidates).is_some()
    }

    /// A cached copy, if present (and marks it recently used).
    pub fn cached(&self, segments: &[String]) -> Option<PathBuf> {
        let f = self.cache_file_for(segments);
        let len = std::fs::metadata(&f).ok()?.len();
        if len == 0 {
            return None;
        }
        if let Ok(h) = std::fs::File::options().write(true).open(&f) {
            let _ = h.set_modified(SystemTime::now());
        }
        Some(f)
    }

    /// Fetch `segments` into the cache. `Ok(None)` when the NAS does not have the file.
    pub async fn fetch(
        &self,
        dav: &WebDav,
        segments: &[String],
        progress: impl FnMut(u64, Option<u64>),
    ) -> Result<Option<PathBuf>> {
        if segments.is_empty() {
            return Ok(None);
        }
        if let Some(f) = self.cached(segments) {
            return Ok(Some(f));
        }
        let Some(remote_size) = dav.size_of(segments).await? else {
            return Ok(None);
        };
        let target = self.cache_file_for(segments);
        let part = target.with_extension("cbz.part");
        let got = dav
            .get_to(segments, &part, MAX_ARCHIVE_BYTES, progress)
            .await;
        let check = match got {
            Ok(n) if remote_size.is_some_and(|s| s != n) => Err(Error::Server(format!(
                "Size mismatch for {}: got {n} bytes, the server said {}",
                segments.join("/"),
                remote_size.unwrap_or_default()
            ))),
            Ok(_) => std::fs::rename(&part, &target)
                .map_err(|e| Error::Io(target.display().to_string(), e)),
            Err(e) => Err(e),
        };
        if let Err(e) = check {
            let _ = std::fs::remove_file(&part);
            return Err(e);
        }
        self.trim();
        Ok(Some(target))
    }

    /// Delete the oldest files until the folder fits the budget.
    pub fn trim(&self) {
        let Ok(rd) = std::fs::read_dir(&self.cache_dir) else {
            return;
        };
        let mut files: Vec<(SystemTime, u64, PathBuf)> = rd
            .filter_map(std::result::Result::ok)
            .filter_map(|e| {
                let m = e.metadata().ok()?;
                m.is_file().then(|| {
                    (
                        m.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                        m.len(),
                        e.path(),
                    )
                })
            })
            .collect();
        let mut total: u64 = files.iter().map(|f| f.1).fold(0, u64::saturating_add);
        if total <= self.budget_bytes {
            return;
        }
        files.sort_by_key(|f| f.0);
        for (_, len, path) in files {
            if total <= self.budget_bytes {
                break;
            }
            if std::fs::remove_file(&path).is_ok() {
                total = total.saturating_sub(len);
            }
        }
    }

    /// Delete every cached archive. Returns bytes freed.
    pub fn clear(&self) -> u64 {
        let Ok(rd) = std::fs::read_dir(&self.cache_dir) else {
            return 0;
        };
        let mut freed = 0u64;
        for e in rd.filter_map(std::result::Result::ok) {
            let len = e.metadata().map(|m| m.len()).unwrap_or(0);
            if e.path().is_file() && std::fs::remove_file(e.path()).is_ok() {
                freed = freed.saturating_add(len);
            }
        }
        freed
    }
}
