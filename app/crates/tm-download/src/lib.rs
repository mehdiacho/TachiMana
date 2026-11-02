//! Chapter downloads for offline reading (PLAN §P6.1, port of `download/DownloadManager.kt`).
//!
//! One worker drains the queue one chapter at a time, which is kind to sources' rate limits. Pages
//! land in the Kotlin app's layout (see [`layout`]), so either app reads the other's downloads.

pub mod layout;

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::Serialize;
use tokio::sync::mpsc;

pub use layout::ChapterMeta;

/// Most pages one chapter may have; a source listing more is refused.
pub const MAX_PAGES: usize = 5000;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Where a chapter's pages come from. The app backs it with the extension host (and the NAS);
/// tests use a fake.
pub trait PageSource: Send + Sync + 'static {
    /// Opens the chapter and returns its page count.
    fn open<'a>(&'a self, chapter: &'a ChapterRequest) -> BoxFuture<'a, Result<usize, String>>;
    /// Bytes of page `index` (0-based) of a chapter that [`PageSource::open`] opened.
    fn page<'a>(
        &'a self,
        chapter: &'a ChapterRequest,
        index: usize,
    ) -> BoxFuture<'a, Result<Vec<u8>, String>>;
}

/// What to download.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterRequest {
    pub source_id: i64,
    pub manga_url: String,
    pub manga_title: String,
    pub chapter_url: String,
    pub chapter_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DownloadStatus {
    Queued,
    Downloading,
    Done,
    Failed,
}

/// One chapter's download, as the Downloads screen shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadJob {
    pub id: u64,
    #[serde(flatten)]
    pub chapter: ChapterRequest,
    pub status: DownloadStatus,
    pub downloaded: u32,
    pub total: u32,
    pub error: Option<String>,
}

type OnChange = Box<dyn Fn(&[DownloadJob]) + Send + Sync>;

struct Inner {
    root: PathBuf,
    jobs: Mutex<Vec<DownloadJob>>,
    next_id: AtomicU64,
    tx: mpsc::UnboundedSender<u64>,
    on_change: OnChange,
}

/// The download queue. Cheap to clone.
#[derive(Clone)]
pub struct DownloadManager {
    inner: Arc<Inner>,
}

impl DownloadManager {
    /// Starts the worker on `rt`. `on_change` gets the whole job list after every change.
    pub fn new(
        root: impl Into<PathBuf>,
        source: Arc<dyn PageSource>,
        rt: &tokio::runtime::Handle,
        on_change: impl Fn(&[DownloadJob]) + Send + Sync + 'static,
    ) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let inner = Arc::new(Inner {
            root: root.into(),
            jobs: Mutex::new(Vec::new()),
            next_id: AtomicU64::new(0),
            tx,
            on_change: Box::new(on_change),
        });
        rt.spawn(worker(Arc::clone(&inner), source, rx));
        Self { inner }
    }

    pub fn root(&self) -> &Path {
        &self.inner.root
    }

    /// Queues a chapter unless it is downloaded or already queued (a failed job does not block a
    /// retry). Returns whether it was queued. The check and the append happen under one lock, so
    /// concurrent calls for one chapter queue it once.
    pub fn enqueue(&self, chapter: ChapterRequest) -> bool {
        let queued = {
            let mut jobs = lock(&self.inner.jobs);
            if self.is_downloaded(chapter.source_id, &chapter.manga_url, &chapter.chapter_url) {
                return false;
            }
            let already = jobs.iter().any(|j| {
                j.chapter.source_id == chapter.source_id
                    && j.chapter.manga_url == chapter.manga_url
                    && j.chapter.chapter_url == chapter.chapter_url
                    && j.status != DownloadStatus::Failed
            });
            if already {
                return false;
            }
            let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
            jobs.push(DownloadJob {
                id,
                chapter,
                status: DownloadStatus::Queued,
                downloaded: 0,
                total: 0,
                error: None,
            });
            if self.inner.tx.send(id).is_err() {
                // The worker is gone (runtime shut down); do not leave a job that never runs.
                jobs.pop();
                return false;
            }
            jobs.clone()
        };
        (self.inner.on_change)(&queued);
        true
    }

    pub fn jobs(&self) -> Vec<DownloadJob> {
        lock(&self.inner.jobs).clone()
    }

    /// Removes a job from the list. A queued job is skipped by the worker; files are kept.
    pub fn dismiss(&self, id: u64) {
        self.edit(|jobs| jobs.retain(|j| j.id != id || j.status == DownloadStatus::Downloading));
    }

    /// Drops finished and failed jobs from the list.
    pub fn clear_finished(&self) {
        self.edit(|jobs| {
            jobs.retain(|j| {
                matches!(
                    j.status,
                    DownloadStatus::Queued | DownloadStatus::Downloading
                )
            })
        });
    }

    pub fn is_downloaded(&self, source: i64, manga_url: &str, chapter_url: &str) -> bool {
        layout::done_marker(&layout::chapter_dir(
            &self.inner.root,
            source,
            manga_url,
            chapter_url,
        ))
        .is_file()
    }

    /// Whether a manga has at least one complete chapter (the library's Downloaded filter).
    pub fn has_any_download(&self, source: i64, manga_url: &str) -> bool {
        let dir = layout::manga_dir(&self.inner.root, source, manga_url);
        std::fs::read_dir(dir)
            .map(|rd| {
                rd.filter_map(Result::ok)
                    .any(|e| layout::done_marker(&e.path()).is_file())
            })
            .unwrap_or(false)
    }

    /// Page files of a downloaded chapter in order; empty when it is not downloaded.
    pub fn downloaded_pages(
        &self,
        source: i64,
        manga_url: &str,
        chapter_url: &str,
    ) -> Vec<PathBuf> {
        layout::downloaded_pages(&layout::chapter_dir(
            &self.inner.root,
            source,
            manga_url,
            chapter_url,
        ))
    }

    /// Deletes a chapter's files. Refused while that chapter is downloading.
    pub fn delete_downloaded(&self, source: i64, manga_url: &str, chapter_url: &str) -> bool {
        let busy = lock(&self.inner.jobs).iter().any(|j| {
            j.status == DownloadStatus::Downloading
                && j.chapter.source_id == source
                && j.chapter.manga_url == manga_url
                && j.chapter.chapter_url == chapter_url
        });
        if busy {
            return false;
        }
        let dir = layout::chapter_dir(&self.inner.root, source, manga_url, chapter_url);
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
            Err(e) => {
                tracing::warn!("could not delete {}: {e}", dir.display());
                false
            }
        }
    }

    /// Every complete chapter on disk, read from the `meta.json` files (the list survives restarts
    /// and includes what the Kotlin app downloaded). Folders without readable meta are skipped.
    pub fn completed(&self) -> Vec<ChapterMeta> {
        let mut out = Vec::new();
        for source in read_dirs(&self.inner.root) {
            for manga in read_dirs(&source) {
                for chapter in read_dirs(&manga) {
                    if !layout::done_marker(&chapter).is_file() {
                        continue;
                    }
                    let meta = std::fs::read(chapter.join("meta.json"))
                        .ok()
                        .and_then(|b| serde_json::from_slice::<ChapterMeta>(&b).ok());
                    if let Some(m) = meta {
                        out.push(m);
                    }
                }
            }
        }
        out
    }

    fn edit(&self, f: impl FnOnce(&mut Vec<DownloadJob>)) {
        self.inner.edit(f);
    }
}

impl Inner {
    fn edit(&self, f: impl FnOnce(&mut Vec<DownloadJob>)) {
        let snapshot = {
            let mut jobs = lock(&self.jobs);
            f(&mut jobs);
            jobs.clone()
        };
        (self.on_change)(&snapshot);
    }

    fn update(&self, id: u64, f: impl FnOnce(&mut DownloadJob)) {
        self.edit(|jobs| {
            if let Some(j) = jobs.iter_mut().find(|j| j.id == id) {
                f(j);
            }
        });
    }
}

/// A poisoned lock only means another thread panicked mid-update; the list is still usable.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn read_dirs(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default()
}

async fn worker(
    inner: Arc<Inner>,
    source: Arc<dyn PageSource>,
    mut rx: mpsc::UnboundedReceiver<u64>,
) {
    while let Some(id) = rx.recv().await {
        let chapter = {
            let mut jobs = lock(&inner.jobs);
            match jobs
                .iter_mut()
                .find(|j| j.id == id && j.status == DownloadStatus::Queued)
            {
                Some(j) => {
                    j.status = DownloadStatus::Downloading;
                    Some(j.chapter.clone())
                }
                None => None, // dismissed while queued
            }
        };
        let Some(chapter) = chapter else { continue };
        let snapshot = lock(&inner.jobs).clone();
        (inner.on_change)(&snapshot);
        let result = download(&inner, source.as_ref(), id, &chapter).await;
        inner.update(id, |j| match result {
            Ok(()) => j.status = DownloadStatus::Done,
            Err(e) => {
                j.status = DownloadStatus::Failed;
                j.error = Some(e);
            }
        });
    }
}

async fn download(
    inner: &Inner,
    source: &dyn PageSource,
    id: u64,
    chapter: &ChapterRequest,
) -> Result<(), String> {
    let count = source.open(chapter).await?;
    if count == 0 {
        return Err("Chapter has no pages".into());
    }
    if count > MAX_PAGES {
        return Err(format!(
            "The source lists {count} pages; at most {MAX_PAGES} are downloaded"
        ));
    }
    let total = u32::try_from(count).unwrap_or(u32::MAX);
    inner.update(id, |j| j.total = total);

    // Start clean: an earlier failed attempt may have left pages under other names or counts.
    let dir = layout::chapter_dir(
        &inner.root,
        chapter.source_id,
        &chapter.manga_url,
        &chapter.chapter_url,
    );
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("Could not clear {}: {e}", dir.display())),
    }
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Could not create {}: {e}", dir.display()))?;

    for index in 0..count {
        let bytes = source
            .page(chapter, index)
            .await
            .map_err(|e| format!("Page {} failed to download: {e}", index.saturating_add(1)))?;
        if bytes.is_empty() {
            return Err(format!("Page {} was empty", index.saturating_add(1)));
        }
        let name = layout::page_name(index, layout::sniff_extension(&bytes));
        std::fs::write(dir.join(&name), &bytes)
            .map_err(|e| format!("Could not save {name}: {e}"))?;
        let done = u32::try_from(index.saturating_add(1)).unwrap_or(u32::MAX);
        inner.update(id, |j| j.downloaded = done);
    }

    let meta = ChapterMeta {
        source_id: chapter.source_id,
        manga_url: chapter.manga_url.clone(),
        manga_title: chapter.manga_title.clone(),
        chapter_url: chapter.chapter_url.clone(),
        chapter_name: chapter.chapter_name.clone(),
        page_count: total,
    };
    if let Ok(json) = serde_json::to_vec_pretty(&meta) {
        if let Err(e) = std::fs::write(dir.join("meta.json"), json) {
            tracing::warn!("could not write meta.json: {e}");
        }
    }
    // The marker goes last, so a partial download never reads back as complete.
    std::fs::write(layout::done_marker(&dir), total.to_string())
        .map_err(|e| format!("Could not mark the chapter complete: {e}"))
}
