//! Port of `download/DownloadSelfTest.kt` (16 checks), plus the restart rebuild and delete guards.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tm_download::{BoxFuture, ChapterRequest, DownloadManager, DownloadStatus, PageSource};

/// A minimal valid 1x1 PNG.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0x0D, 0x49, 0x48, 0x44, 0x52, 0, 0, 0,
    1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0x1F, 0x15, 0xC4, 0x89,
];
const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xD9];

/// Chapters are 3 PNG pages, except "boom" (page 2 fails) and "/ch/retry" (attempt 1: 3 JPEGs then a
/// failure on page 4; attempt 2: 2 PNGs).
#[derive(Default)]
struct Fake {
    opens: AtomicUsize,
    retry_attempts: AtomicUsize,
}

impl PageSource for Fake {
    fn open<'a>(&'a self, c: &'a ChapterRequest) -> BoxFuture<'a, Result<usize, String>> {
        Box::pin(async move {
            self.opens.fetch_add(1, Ordering::SeqCst);
            Ok(if c.chapter_url.contains("boom") {
                2
            } else if c.chapter_url == "/ch/retry" {
                if self.retry_attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                    4
                } else {
                    2
                }
            } else {
                3
            })
        })
    }

    fn page<'a>(
        &'a self,
        c: &'a ChapterRequest,
        i: usize,
    ) -> BoxFuture<'a, Result<Vec<u8>, String>> {
        Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(5)).await;
            if c.chapter_url.contains("boom") && i == 1 {
                return Err("fake failure".into());
            }
            if c.chapter_url == "/ch/retry" {
                let attempt = self.retry_attempts.load(Ordering::SeqCst);
                if attempt == 1 {
                    return if i < 3 {
                        Ok(JPEG.to_vec())
                    } else {
                        Err("fake failure".into())
                    };
                }
            }
            Ok(PNG.to_vec())
        })
    }
}

fn chapter(url: &str) -> ChapterRequest {
    ChapterRequest {
        source_id: 1,
        manga_url: "/manga/1".into(),
        manga_title: "Test Manga".into(),
        chapter_url: url.into(),
        chapter_name: format!("Chapter {url}"),
    }
}

async fn wait_for(dm: &DownloadManager, f: impl Fn(&[tm_download::DownloadJob]) -> bool) {
    for _ in 0..250 {
        if f(&dm.jobs()) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn status(dm: &DownloadManager, url: &str) -> Option<DownloadStatus> {
    dm.jobs()
        .iter()
        .find(|j| j.chapter.chapter_url == url)
        .map(|j| j.status)
}

fn names(paths: &[std::path::PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn download_self_test() {
    let root = tempfile::tempdir().unwrap();
    let fake = Arc::new(Fake::default());
    let changes = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&changes);
    let dm = DownloadManager::new(
        root.path(),
        Arc::clone(&fake) as Arc<dyn PageSource>,
        &tokio::runtime::Handle::current(),
        move |_| {
            seen.fetch_add(1, Ordering::SeqCst);
        },
    );
    let m = "/manga/1";

    // 1. Enqueue → DONE with 3/3.
    assert!(dm.enqueue(chapter("/ch/1")), "enqueue accepted");
    wait_for(&dm, |j| {
        j.first().is_some_and(|j| j.status == DownloadStatus::Done)
    })
    .await;
    let done = dm.jobs()[0].clone();
    assert_eq!(
        (done.status, done.downloaded, done.total),
        (DownloadStatus::Done, 3, 3)
    );
    assert!(changes.load(Ordering::SeqCst) > 0, "progress reported");

    // 2. Files, marker and meta.
    assert!(dm.is_downloaded(1, m, "/ch/1"));
    let pages = dm.downloaded_pages(1, m, "/ch/1");
    assert_eq!(names(&pages), ["001.png", "002.png", "003.png"]);
    assert!(pages
        .iter()
        .all(|p| std::fs::metadata(p).unwrap().len() > 0));

    // 3. Re-enqueue of a downloaded chapter is skipped and does not open the source.
    let opens = fake.opens.load(Ordering::SeqCst);
    assert!(!dm.enqueue(chapter("/ch/1")));
    assert_eq!(fake.opens.load(Ordering::SeqCst), opens);

    // 4. A failing page ends FAILED, not downloaded, with the error kept.
    dm.enqueue(chapter("/ch/boom"));
    wait_for(&dm, |_| {
        status(&dm, "/ch/boom") == Some(DownloadStatus::Failed)
    })
    .await;
    assert_eq!(status(&dm, "/ch/boom"), Some(DownloadStatus::Failed));
    assert!(!dm.is_downloaded(1, m, "/ch/boom"));
    let err = dm
        .jobs()
        .into_iter()
        .find(|j| j.chapter.chapter_url == "/ch/boom")
        .unwrap()
        .error;
    assert!(err.unwrap_or_default().contains("Page 2"));

    // 5. A second chapter coexists on disk.
    dm.enqueue(chapter("/ch/2"));
    wait_for(&dm, |_| status(&dm, "/ch/2") == Some(DownloadStatus::Done)).await;
    assert!(dm.is_downloaded(1, m, "/ch/2") && dm.is_downloaded(1, m, "/ch/1"));
    assert!(dm.has_any_download(1, m));
    assert!(!dm.has_any_download(1, "/manga/other"));

    // 6. clear_finished drops DONE and FAILED.
    dm.clear_finished();
    assert!(dm
        .jobs()
        .iter()
        .all(|j| !matches!(j.status, DownloadStatus::Done | DownloadStatus::Failed)));

    // 7. Delete removes it.
    assert!(dm.delete_downloaded(1, m, "/ch/1"));
    assert!(!dm.is_downloaded(1, m, "/ch/1"));

    // 8. A retry after a partial failure leaves no stale pages.
    dm.enqueue(chapter("/ch/retry"));
    wait_for(&dm, |_| {
        status(&dm, "/ch/retry") == Some(DownloadStatus::Failed)
    })
    .await;
    assert!(!dm.is_downloaded(1, m, "/ch/retry"));
    // A failed job does not block a retry.
    assert!(dm.enqueue(chapter("/ch/retry")));
    wait_for(&dm, |j| {
        j.iter()
            .any(|j| j.chapter.chapter_url == "/ch/retry" && j.status == DownloadStatus::Done)
    })
    .await;
    assert_eq!(
        names(&dm.downloaded_pages(1, m, "/ch/retry")),
        ["001.png", "002.png"]
    );

    // 9. Concurrent enqueues of one chapter queue it once.
    let accepted = Arc::new(AtomicUsize::new(0));
    let tasks: Vec<_> = (0..12)
        .map(|_| {
            let dm = dm.clone();
            let accepted = Arc::clone(&accepted);
            tokio::spawn(async move {
                if dm.enqueue(chapter("/ch/hammer")) {
                    accepted.fetch_add(1, Ordering::SeqCst);
                }
            })
        })
        .collect();
    for t in tasks {
        t.await.unwrap();
    }
    assert_eq!(accepted.load(Ordering::SeqCst), 1);
    assert_eq!(
        dm.jobs()
            .iter()
            .filter(|j| j.chapter.chapter_url == "/ch/hammer")
            .count(),
        1
    );
    wait_for(&dm, |_| {
        status(&dm, "/ch/hammer") == Some(DownloadStatus::Done)
    })
    .await;

    // New: the completed list is rebuilt from disk by a fresh manager (a restart).
    let again = DownloadManager::new(
        root.path(),
        Arc::new(Fake::default()) as Arc<dyn PageSource>,
        &tokio::runtime::Handle::current(),
        |_| {},
    );
    let mut urls: Vec<String> = again
        .completed()
        .into_iter()
        .map(|c| c.chapter_url)
        .collect();
    urls.sort();
    assert_eq!(urls, ["/ch/2", "/ch/hammer", "/ch/retry"]);
    let meta = again
        .completed()
        .into_iter()
        .find(|c| c.chapter_url == "/ch/2")
        .unwrap();
    assert_eq!(
        (meta.page_count, meta.manga_title.as_str()),
        (3, "Test Manga")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_dismissed_queued_job_is_not_downloaded() {
    let root = tempfile::tempdir().unwrap();
    let fake = Arc::new(Fake::default());
    let dm = DownloadManager::new(
        root.path(),
        Arc::clone(&fake) as Arc<dyn PageSource>,
        &tokio::runtime::Handle::current(),
        |_| {},
    );
    // The first job keeps the worker busy while the second is dismissed.
    dm.enqueue(chapter("/ch/a"));
    dm.enqueue(chapter("/ch/b"));
    let b = dm
        .jobs()
        .into_iter()
        .find(|j| j.chapter.chapter_url == "/ch/b")
        .unwrap();
    dm.dismiss(b.id);
    wait_for(&dm, |_| status(&dm, "/ch/a") == Some(DownloadStatus::Done)).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!dm.is_downloaded(1, "/manga/1", "/ch/b"));
    assert_eq!(fake.opens.load(Ordering::SeqCst), 1);
}

#[test]
fn incomplete_chapters_have_no_pages() {
    let root = tempfile::tempdir().unwrap();
    let dir = tm_download::layout::chapter_dir(root.path(), 1, "/m", "/c");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("001.png"), PNG).unwrap();
    assert!(
        tm_download::layout::downloaded_pages(&dir).is_empty(),
        "no .done marker"
    );
    std::fs::write(dir.join(".done"), "1").unwrap();
    std::fs::write(dir.join("meta.json"), "{}").unwrap();
    std::fs::write(dir.join("010.jpg"), JPEG).unwrap();
    std::fs::write(dir.join("002.png"), PNG).unwrap();
    assert_eq!(
        names(&tm_download::layout::downloaded_pages(&dir)),
        ["001.png", "002.png", "010.jpg"],
        "numeric order, meta and marker skipped"
    );
}
