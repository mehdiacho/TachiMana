//! Downloads in the app (PLAN §P6.1): the queue, its commands and the `downloads` event. Pages come
//! from the extension host; a downloaded chapter is read from disk before anything else.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Manager, State};
use tm_db::{Chapter, Manga};
use tm_download::{BoxFuture, ChapterRequest, DownloadManager, PageSource};
use tm_exthost::model;

use crate::dto::{CmdError, DownloadJobDto, DownloadedChapterDto};
use crate::state::AppState;

type R<T> = Result<T, CmdError>;

/// Most chapters one request may queue.
const MAX_ENQUEUE: usize = 2000;

/// Where an opened chapter's pages come from.
#[derive(Clone)]
enum Listed {
    Host(Vec<model::Page>),
    /// A CBZ fetched from the NAS ("re-download from NAS" setting).
    Archive(PathBuf, Vec<String>),
}

/// Pages through the extension host, or from the NAS when it holds the chapter. `open` remembers
/// the listing per chapter url until the next `open`, which is all one worker needs.
struct HostPages {
    app: AppHandle,
    listed: Mutex<HashMap<(i64, String), Listed>>,
}

impl HostPages {
    fn listed_of(&self, c: &ChapterRequest) -> Option<Listed> {
        self.listed
            .lock()
            .ok()?
            .get(&(c.source_id, c.chapter_url.clone()))
            .cloned()
    }

    fn remember(&self, c: &ChapterRequest, l: Listed) {
        if let Ok(mut t) = self.listed.lock() {
            t.clear();
            t.insert((c.source_id, c.chapter_url.clone()), l);
        }
    }
}

impl PageSource for HostPages {
    fn open<'a>(&'a self, c: &'a ChapterRequest) -> BoxFuture<'a, Result<usize, String>> {
        Box::pin(async move {
            let state = self.app.state::<AppState>();
            let (manga, ch) = rows_of(&state, c).map_err(|e| e.message)?;
            let from_nas = state
                .settings
                .lock()
                .is_ok_and(|s| s.bool(tm_core::settings::keys::REMOTE_REDOWNLOAD));
            if from_nas {
                if let Some(cbz) = crate::nas::fetch_chapter(&state, &manga, &ch).await {
                    if let Ok(names) = tm_archive::page_names(&cbz) {
                        if !names.is_empty() {
                            let n = names.len();
                            self.remember(c, Listed::Archive(cbz, names));
                            return Ok(n);
                        }
                    }
                }
            }
            let open = crate::online::load_online_chapter(&state, &manga, &ch)
                .await
                .map_err(|e| e.message)?;
            let n = open.pages.len();
            self.remember(c, Listed::Host(open.pages));
            Ok(n)
        })
    }

    fn page<'a>(
        &'a self,
        c: &'a ChapterRequest,
        index: usize,
    ) -> BoxFuture<'a, Result<Vec<u8>, String>> {
        Box::pin(async move {
            let pages = match self.listed_of(c).ok_or("the chapter was not opened")? {
                Listed::Archive(path, names) => {
                    let name = names.get(index).ok_or("no such page")?;
                    return tm_archive::read_page(&path, name).map_err(|e| e.to_string());
                }
                Listed::Host(pages) => pages,
            };
            let page = pages.get(index).ok_or("no such page")?;
            let state = self.app.state::<AppState>();
            let client = crate::online::client(&state).map_err(|e| e.message)?;
            let img = client
                .page_image(c.source_id, page)
                .await
                .map_err(|e| e.to_string())?;
            Ok(img.bytes)
        })
    }
}

fn rows_of(state: &AppState, c: &ChapterRequest) -> R<(Manga, Chapter)> {
    let manga = state
        .db
        .manga_by_url(&c.manga_url, c.source_id)?
        .ok_or_else(|| CmdError::not_found("That manga"))?;
    let ch = state
        .db
        .chapters(manga.id)?
        .into_iter()
        .find(|ch| ch.url == c.chapter_url)
        .ok_or_else(|| CmdError::not_found("That chapter"))?;
    Ok((manga, ch))
}

/// Creates the queue; called once from `setup`.
pub fn init(app: &AppHandle) {
    let state = app.state::<AppState>();
    let source = Arc::new(HostPages {
        app: app.clone(),
        listed: Mutex::new(HashMap::new()),
    });
    let emitter = app.clone();
    let root = state.paths.downloads_root();
    // Jobs already handed to the NAS upload queue.
    let handed: Mutex<std::collections::HashSet<u64>> = Mutex::new(Default::default());
    let dm = DownloadManager::new(
        root.clone(),
        source,
        tauri::async_runtime::handle().inner(),
        move |jobs| {
            let dto: Vec<DownloadJobDto> = jobs.iter().map(DownloadJobDto::from).collect();
            let _ = emitter.emit("downloads", dto);
            for j in jobs
                .iter()
                .filter(|j| j.status == tm_download::DownloadStatus::Done)
            {
                if handed.lock().is_ok_and(|mut h| h.insert(j.id)) {
                    let c = &j.chapter;
                    let dir = tm_download::layout::chapter_dir(
                        &root,
                        c.source_id,
                        &c.manga_url,
                        &c.chapter_url,
                    );
                    crate::nas::upload_finished(
                        &emitter,
                        c.source_id,
                        &c.manga_url,
                        &c.chapter_url,
                        dir,
                    );
                }
            }
        },
    );
    let _ = state.downloads.set(dm);
}

pub fn manager(state: &AppState) -> R<&DownloadManager> {
    state
        .downloads
        .get()
        .ok_or_else(|| CmdError::internal("downloads are not ready yet"))
}

pub(crate) fn request(manga: &Manga, ch: &Chapter) -> ChapterRequest {
    ChapterRequest {
        source_id: manga.source,
        manga_url: manga.url.clone(),
        manga_title: manga.title.clone(),
        chapter_url: ch.url.clone(),
        chapter_name: ch.name.clone(),
    }
}

/// The folder of a downloaded chapter, when it is complete on disk.
pub fn downloaded_dir(state: &AppState, manga: &Manga, ch: &Chapter) -> Option<PathBuf> {
    if manga.is_local() {
        return None;
    }
    let dm = state.downloads.get()?;
    dm.is_downloaded(manga.source, &manga.url, &ch.url)
        .then(|| tm_download::layout::chapter_dir(dm.root(), manga.source, &manga.url, &ch.url))
}

/// Whether a chapter is complete on disk.
pub fn is_downloaded(state: &AppState, manga: &Manga, ch: &Chapter) -> bool {
    downloaded_dir(state, manga, ch).is_some()
}

/// Whether an online manga has any downloaded chapter.
pub fn has_any(state: &AppState, manga: &Manga) -> bool {
    !manga.is_local()
        && state
            .downloads
            .get()
            .is_some_and(|dm| dm.has_any_download(manga.source, &manga.url))
}

fn online_manga(state: &AppState, id: i64) -> R<Manga> {
    let m = state
        .db
        .manga(id)?
        .ok_or_else(|| CmdError::not_found("That manga"))?;
    if m.is_local() {
        return Err(CmdError::new(
            "invalid",
            "Local manga are already on this computer.",
        ));
    }
    Ok(m)
}

#[tauri::command]
pub fn downloads_list(state: State<'_, AppState>) -> R<Vec<DownloadJobDto>> {
    Ok(manager(&state)?
        .jobs()
        .iter()
        .map(DownloadJobDto::from)
        .collect())
}

/// Queues chapters of one online manga. Returns how many were queued (downloaded or already
/// queued chapters are skipped).
#[tauri::command]
pub fn download_chapters(
    state: State<'_, AppState>,
    manga_id: i64,
    chapter_ids: Vec<i64>,
) -> R<u32> {
    let manga = online_manga(&state, manga_id)?;
    let dm = manager(&state)?;
    let wanted: std::collections::HashSet<i64> =
        chapter_ids.into_iter().take(MAX_ENQUEUE).collect();
    let mut queued = 0u32;
    for ch in state
        .db
        .chapters(manga_id)?
        .iter()
        .filter(|c| wanted.contains(&c.id))
    {
        if dm.enqueue(request(&manga, ch)) {
            queued = queued.saturating_add(1);
        }
    }
    Ok(queued)
}

/// Queues every unread chapter of an online manga, oldest first.
#[tauri::command]
pub fn download_unread(state: State<'_, AppState>, manga_id: i64) -> R<u32> {
    let manga = online_manga(&state, manga_id)?;
    let dm = manager(&state)?;
    let mut chapters: Vec<Chapter> = state
        .db
        .chapters(manga_id)?
        .into_iter()
        .filter(|c| !c.read)
        .collect();
    // Online sources number chapters newest first (source_order 0 = newest).
    chapters.sort_by_key(|c| std::cmp::Reverse(c.source_order));
    let mut queued = 0u32;
    for ch in chapters.iter().take(MAX_ENQUEUE) {
        if dm.enqueue(request(&manga, ch)) {
            queued = queued.saturating_add(1);
        }
    }
    Ok(queued)
}

/// Deletes downloaded chapters of one manga. Returns how many were deleted.
#[tauri::command]
pub fn download_delete(state: State<'_, AppState>, manga_id: i64, chapter_ids: Vec<i64>) -> R<u32> {
    let manga = online_manga(&state, manga_id)?;
    let dm = manager(&state)?;
    let wanted: std::collections::HashSet<i64> = chapter_ids.into_iter().collect();
    let mut deleted = 0u32;
    for ch in state
        .db
        .chapters(manga_id)?
        .iter()
        .filter(|c| wanted.contains(&c.id))
    {
        if dm.is_downloaded(manga.source, &manga.url, &ch.url)
            && dm.delete_downloaded(manga.source, &manga.url, &ch.url)
        {
            deleted = deleted.saturating_add(1);
            // The reader may hold the folder's page list.
            if let Ok(mut t) = state.chapters() {
                t.remove(&ch.id);
            }
        }
    }
    Ok(deleted)
}

#[tauri::command]
pub fn download_dismiss(state: State<'_, AppState>, id: u64) -> R<()> {
    manager(&state)?.dismiss(id);
    Ok(())
}

#[tauri::command]
pub fn downloads_clear_finished(state: State<'_, AppState>) -> R<()> {
    manager(&state)?.clear_finished();
    Ok(())
}

/// Every complete chapter on disk, matched to library rows where they exist.
#[tauri::command]
pub fn downloads_on_disk(state: State<'_, AppState>) -> R<Vec<DownloadedChapterDto>> {
    let dm = manager(&state)?;
    let mut out = Vec::new();
    // Library row and chapters per (source, manga url), looked up once.
    type Rows = Option<(i64, Vec<Chapter>)>;
    let mut mangas: HashMap<(i64, String), Rows> = HashMap::new();
    for meta in dm.completed() {
        let entry = mangas
            .entry((meta.source_id, meta.manga_url.clone()))
            .or_insert_with(|| {
                let m = state
                    .db
                    .manga_by_url(&meta.manga_url, meta.source_id)
                    .ok()
                    .flatten()?;
                let chapters = state.db.chapters(m.id).unwrap_or_default();
                Some((m.id, chapters))
            });
        let (manga_id, chapter_id) = match entry {
            Some((mid, chapters)) => (
                Some(*mid),
                chapters
                    .iter()
                    .find(|c| c.url == meta.chapter_url)
                    .map(|c| c.id),
            ),
            None => (None, None),
        };
        out.push(DownloadedChapterDto {
            manga_id,
            chapter_id,
            manga_title: meta.manga_title,
            chapter_name: meta.chapter_name,
            page_count: meta.page_count,
        });
    }
    out.sort_by(|a, b| {
        a.manga_title
            .to_lowercase()
            .cmp(&b.manga_title.to_lowercase())
            .then_with(|| a.chapter_name.cmp(&b.chapter_name))
    });
    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )]
    use super::*;
    use tm_db::{SourceChapter, SourceManga};

    /// Smallest valid PNG header bytes are enough: the layout sniffs the type from them.
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n0000";

    struct TwoPages;
    impl PageSource for TwoPages {
        fn open<'a>(&'a self, _: &'a ChapterRequest) -> BoxFuture<'a, Result<usize, String>> {
            Box::pin(async { Ok(2) })
        }
        fn page<'a>(
            &'a self,
            _: &'a ChapterRequest,
            _: usize,
        ) -> BoxFuture<'a, Result<Vec<u8>, String>> {
            Box::pin(async { Ok(PNG.to_vec()) })
        }
    }

    #[test]
    fn a_downloaded_online_chapter_is_read_from_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let state =
            AppState::open(tm_core::AppPaths::at(tmp.path().join("data")).unwrap()).unwrap();
        let source = 2_499_283_573_021_220_255_i64;
        let (manga_id, _) = state
            .db
            .upsert_online(
                source,
                &SourceManga {
                    url: "/m/1".into(),
                    title: "Online".into(),
                    ..Default::default()
                },
                &[
                    SourceChapter {
                        url: "/c/2".into(),
                        name: "Ch 2".into(),
                        ..Default::default()
                    },
                    SourceChapter {
                        url: "/c/1".into(),
                        name: "Ch 1".into(),
                        ..Default::default()
                    },
                ],
            )
            .unwrap();
        state.db.set_favorite(manga_id, true).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let dm = DownloadManager::new(
            state.paths.downloads_root(),
            Arc::new(TwoPages),
            tauri::async_runtime::handle().inner(),
            move |jobs| {
                if jobs
                    .iter()
                    .all(|j| j.status == tm_download::DownloadStatus::Done)
                {
                    let _ = tx.send(());
                }
            },
        );
        assert!(state.downloads.set(dm).is_ok());

        let chapters = state.db.chapters(manga_id).unwrap();
        let ch1 = chapters.iter().find(|c| c.url == "/c/1").unwrap().clone();
        let manga = state.db.manga(manga_id).unwrap().unwrap();
        assert!(
            crate::commands::resolve_chapter(&state, ch1.id).is_err(),
            "not on disk yet"
        );

        assert!(manager(&state).unwrap().enqueue(request(&manga, &ch1)));
        rx.recv_timeout(std::time::Duration::from_secs(10))
            .expect("download finished");

        let open = crate::commands::resolve_chapter(&state, ch1.id).unwrap();
        assert_eq!(open.names, ["001.png", "002.png"]);
        assert_eq!(tm_archive::read_page(&open.path, "002.png").unwrap(), PNG);

        let detail = crate::commands::detail_of(&state, manga_id).unwrap();
        let flags: Vec<(String, bool)> = detail
            .chapters
            .iter()
            .map(|c| (c.name.clone(), c.downloaded))
            .collect();
        assert!(flags.contains(&("Ch 1".into(), true)));
        assert!(flags.contains(&("Ch 2".into(), false)));
        assert!(has_any(&state, &manga));
    }
}
