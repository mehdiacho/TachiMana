//! Tauri commands. Each one is thin: check the input, call a crate, map the error.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use tauri::State;
use tm_core::settings::keys;
use tm_db::Chapter;

use crate::dto::*;
use crate::state::{AppState, OpenChapter};

type R<T> = Result<T, CmdError>;

/// Largest page count a chapter may report; beyond this the archive is treated as broken.
const MAX_PAGES: usize = 5000;
const MAX_NAME: usize = 200;

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        data_dir: state.paths.data_dir().to_string_lossy().into_owned(),
        local_dir: state
            .paths
            .local_library_dir()
            .to_string_lossy()
            .into_owned(),
        read_only: state.db.is_read_only(),
    }
}

/// Each manga's end first, then a join back: one pass over the chapters. A correlated subquery
/// (re-scanning a manga's chapters for every one of its chapters) took 7 s, 45 s in a debug
/// build, on a 172,000-chapter library.
const LATEST_SQL: &str = "WITH pick AS (SELECT c.manga_id AS manga_id, CASE WHEN m.source = 0 \
     THEN MAX(c.source_order) ELSE MIN(c.source_order) END AS so \
     FROM chapters c JOIN mangas m ON m._id = c.manga_id GROUP BY c.manga_id) \
     SELECT c.manga_id, c.name FROM pick p \
     JOIN chapters c ON c.manga_id = p.manga_id AND c.source_order = p.so";

/// Newest chapter name per manga. Online sources list newest first (`source_order` 0 is the
/// newest); the local importer numbers files in natural order (0 is the oldest).
pub fn latest_names(db: &tm_db::Db) -> R<HashMap<i64, String>> {
    Ok(db.read(|c| {
        let mut st = c.prepare(LATEST_SQL)?;
        let rows = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        let mut out = HashMap::new();
        for row in rows {
            let (id, name) = row?;
            out.insert(id, name);
        }
        Ok(out)
    })?)
}

/// The library list. Runs off the UI thread: it reads every chapter, and a big library must
/// never freeze the window while it does.
#[tauri::command]
pub async fn library(app: tauri::AppHandle) -> R<Vec<LibraryEntryDto>> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || library_now(&app.state::<AppState>()))
        .await
        .map_err(|e| CmdError::internal(format!("Reading the library failed: {e}")))?
}

fn library_now(state: &AppState) -> R<Vec<LibraryEntryDto>> {
    let mangas = state.db.library()?;
    let unread = state.db.unread_counts()?;
    let cats = state.db.category_assignments()?;
    let latest = latest_names(&state.db)?;
    Ok(mangas
        .iter()
        .map(|m| LibraryEntryDto {
            manga: MangaDto::from(m),
            unread: unread.get(&m.id).copied().unwrap_or(0),
            category_ids: cats
                .get(&m.id)
                .map(|s| s.iter().copied().collect())
                .unwrap_or_default(),
            downloaded: crate::downloads::has_any(state, m),
            latest_chapter: latest.get(&m.id).cloned(),
        })
        .collect())
}

/// Recently read chapters, newest first (at most 50, one per chapter).
#[tauri::command]
pub fn recents(state: State<'_, AppState>) -> R<Vec<crate::dto::RecentDto>> {
    Ok(state
        .db
        .recents()?
        .iter()
        .map(|r| crate::dto::RecentDto {
            manga: MangaDto::from(&r.manga),
            chapter: ChapterDto::from(&r.chapter),
            last_read: r.last_read,
        })
        .collect())
}

#[tauri::command]
pub fn categories(state: State<'_, AppState>) -> R<Vec<CategoryDto>> {
    Ok(state
        .db
        .categories()?
        .iter()
        .map(CategoryDto::from)
        .collect())
}

fn clean_name(name: &str) -> R<String> {
    let n = name.trim();
    if n.is_empty() {
        return Err(CmdError::new("invalid", "A category needs a name."));
    }
    if n.chars().count() > MAX_NAME {
        return Err(CmdError::new("invalid", "That name is too long."));
    }
    Ok(n.to_owned())
}

#[tauri::command]
pub fn category_create(state: State<'_, AppState>, name: String) -> R<Option<i64>> {
    let n = clean_name(&name)?;
    Ok(state.db.create_category(&n)?)
}

#[tauri::command]
pub fn category_rename(state: State<'_, AppState>, id: i64, name: String) -> R<()> {
    let n = clean_name(&name)?;
    Ok(state.db.rename_category(id, &n)?)
}

#[tauri::command]
pub fn category_delete(state: State<'_, AppState>, id: i64) -> R<()> {
    Ok(state.db.delete_category(id)?)
}

#[tauri::command]
pub fn category_move(state: State<'_, AppState>, id: i64, up: bool) -> R<()> {
    Ok(state.db.move_category(id, up)?)
}

#[tauri::command]
pub fn manga_set_categories(state: State<'_, AppState>, manga_id: i64, ids: Vec<i64>) -> R<()> {
    let set: BTreeSet<i64> = ids.into_iter().collect();
    Ok(state.db.set_manga_categories(manga_id, &set)?)
}

#[tauri::command]
pub fn manga_detail(state: State<'_, AppState>, id: i64) -> R<MangaDetailDto> {
    detail_of(&state, id)
}

pub fn detail_of(state: &AppState, id: i64) -> R<MangaDetailDto> {
    let manga = state
        .db
        .manga(id)?
        .ok_or_else(|| CmdError::not_found("That manga"))?;
    let chapters = state.db.chapters(id)?;
    let cats = state.db.category_assignments()?;
    Ok(MangaDetailDto {
        manga: MangaDto::from(&manga),
        chapters: chapters
            .iter()
            .map(|c| ChapterDto {
                downloaded: crate::downloads::is_downloaded(state, &manga, c),
                on_nas: crate::nas::holds(state, &manga, c),
                ..ChapterDto::from(c)
            })
            .collect(),
        category_ids: cats
            .get(&id)
            .map(|s| s.iter().copied().collect())
            .unwrap_or_default(),
    })
}

#[tauri::command]
pub async fn rescan_local(state: State<'_, AppState>) -> R<ImportResultDto> {
    let root = state.paths.local_library_dir();
    std::fs::create_dir_all(&root)
        .map_err(|e| CmdError::new("io", format!("Couldn't create the local folder: {e}")))?;
    let res = tm_local::scan(&state.db, &root)?;
    Ok(res.into())
}

#[tauri::command]
pub fn set_viewer(state: State<'_, AppState>, manga_id: i64, viewer: i64) -> R<()> {
    if !matches!(viewer, 0 | 1 | 2 | 5 | 6) {
        return Err(CmdError::new("invalid", "Unknown reading mode."));
    }
    Ok(state.db.set_viewer(manga_id, viewer)?)
}

pub fn chapter_or_404(state: &AppState, id: i64) -> R<Chapter> {
    state
        .db
        .chapter(id)?
        .ok_or_else(|| CmdError::not_found("That chapter"))
}

/// Resolve where a chapter's pages live on disk: the local folder, or a finished download of an
/// online chapter (plan §5.4 order; the NAS joins later). Online chapters that are not downloaded
/// come through the extension host instead (`online::page_bytes`).
pub fn resolve_chapter(state: &AppState, chapter_id: i64) -> R<OpenChapter> {
    if let Some(open) = state.chapters()?.get(&chapter_id) {
        return Ok(open.clone());
    }
    let ch = chapter_or_404(state, chapter_id)?;
    let manga = state
        .db
        .manga(ch.manga_id)?
        .ok_or_else(|| CmdError::not_found("That manga"))?;
    let path = if manga.is_local() {
        tm_local::chapter_path(&state.paths.local_library_dir(), &ch.url).ok_or_else(|| {
            CmdError::new(
                "invalid",
                "This chapter's path is not inside the local folder.",
            )
        })?
    } else {
        crate::downloads::downloaded_dir(state, &manga, &ch)
            .ok_or_else(|| CmdError::not_found("A download of that chapter"))?
    };
    let names = tm_archive::page_names(&path)?;
    if names.len() > MAX_PAGES {
        return Err(CmdError::new(
            "archive",
            "This chapter has too many pages to open.",
        ));
    }
    let open = OpenChapter { path, names };
    let mut table = state.chapters()?;
    // Keep the table small: the reader holds a handful of chapters at a time.
    if table.len() > 64 {
        table.clear();
    }
    table.insert(chapter_id, open.clone());
    Ok(open)
}

/// Opens an archive fetched from the NAS for the reader, so page requests are served from it.
pub fn remember_archive(state: &AppState, chapter_id: i64, path: PathBuf) -> R<()> {
    let names = tm_archive::page_names(&path)?;
    if names.is_empty() || names.len() > MAX_PAGES {
        return Err(CmdError::new(
            "archive",
            "The NAS copy of this chapter has no readable pages.",
        ));
    }
    let mut table = state.chapters()?;
    if table.len() > 64 {
        table.clear();
    }
    table.insert(chapter_id, OpenChapter { path, names });
    Ok(())
}

#[tauri::command]
pub async fn chapter_pages(state: State<'_, AppState>, chapter_id: i64) -> R<ChapterPagesDto> {
    let ch = chapter_or_404(&state, chapter_id)?;
    let manga = state
        .db
        .manga(ch.manga_id)?
        .ok_or_else(|| CmdError::not_found("That manga"))?;
    if !manga.is_local() && !crate::downloads::is_downloaded(&state, &manga, &ch) {
        // Read order (plan §P6.2): download, then the NAS, then the source.
        if let Some(cbz) = crate::nas::fetch_chapter(&state, &manga, &ch).await {
            remember_archive(&state, chapter_id, cbz)?;
        } else {
            return crate::online::chapter_pages(&state, &manga, &ch).await;
        }
    }
    let open = resolve_chapter(&state, chapter_id)?;
    let dims = tm_archive::page_dimensions(&open.path, &open.names)?;
    Ok(ChapterPagesDto {
        chapter_id,
        pages: dims
            .into_iter()
            .map(|d| PageDto {
                w: d.map(|(w, _)| w),
                h: d.map(|(_, h)| h),
            })
            .collect(),
    })
}

/// Save reading progress the way the Kotlin app does: page, pages left, read once finished
/// (never un-read), and a history row.
#[tauri::command]
pub fn save_progress(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    chapter_id: i64,
    page: i64,
    total: i64,
) -> R<()> {
    if total <= 0 || page < 0 || page >= total {
        return Err(CmdError::new("invalid", "Page is out of range."));
    }
    let ch = chapter_or_404(&state, chapter_id)?;
    let finished = page >= total.saturating_sub(1);
    let left = total.saturating_sub(1).saturating_sub(page).max(0);
    state
        .db
        .mark_progress(chapter_id, page, left, finished || ch.read)?;
    state.db.record_history(chapter_id)?;
    // Like the Kotlin app: a finished chapter raises AniList's progress (no-op when untracked).
    if finished {
        crate::track::push_finished(&app, ch.manga_id, ch.chapter_number);
    }
    Ok(())
}

#[tauri::command]
pub fn set_chapter_read(state: State<'_, AppState>, chapter_id: i64, read: bool) -> R<()> {
    Ok(state.db.set_read(chapter_id, read)?)
}

fn ui_settings(s: &tm_core::Settings) -> UiSettings {
    UiSettings {
        theme: s.string(keys::THEME),
        reader_fit: s.string(keys::READER_FIT),
        reader_zoom: s.f64(keys::READER_ZOOM),
        reader_cover_solo: s.bool(keys::READER_COVER_SOLO),
        library_grid: s.bool(keys::LIBRARY_GRID),
        library_sort: s.string(keys::LIBRARY_SORT),
        library_sort_descending: s.bool(keys::LIBRARY_SORT_DESCENDING),
        library_unread_only: s.bool(keys::LIBRARY_UNREAD_ONLY),
        library_downloaded_only: s.bool(keys::LIBRARY_DOWNLOADED_ONLY),
        disk_cache_mb: s.i64(keys::DISK_CACHE_MB),
        rotated: s.bool(keys::ROTATED),
        cloudflare_bypass_enabled: s.bool(keys::CLOUDFLARE_BYPASS_ENABLED),
    }
}

#[tauri::command]
pub fn settings_get(state: State<'_, AppState>) -> R<UiSettings> {
    let s = state.settings()?;
    Ok(ui_settings(&s))
}

const THEMES: &[&str] = &["YOKAI_DARK", "YOKAI_LIGHT", "MIDNIGHT", "INK_VERMILION"];
const FITS: &[&str] = &["FIT_SCREEN", "FIT_WIDTH", "FIT_HEIGHT", "ORIGINAL"];
const SORTS: &[&str] = &["TITLE", "UNREAD", "DATE_ADDED", "LAST_UPDATED"];

fn one_of(v: &str, allowed: &[&str], what: &str) -> R<()> {
    if allowed.contains(&v) {
        Ok(())
    } else {
        Err(CmdError::new("invalid", format!("Unknown {what}.")))
    }
}

#[tauri::command]
pub fn settings_set(state: State<'_, AppState>, patch: UiSettingsPatch) -> R<UiSettings> {
    let mut s = state.settings()?;
    if let Some(v) = &patch.theme {
        one_of(v, THEMES, "theme")?;
        s.set(keys::THEME, v.as_str());
    }
    if let Some(v) = &patch.reader_fit {
        one_of(v, FITS, "fit")?;
        s.set(keys::READER_FIT, v.as_str());
    }
    if let Some(v) = patch.reader_zoom {
        if !v.is_finite() {
            return Err(CmdError::new("invalid", "Zoom must be a number."));
        }
        s.set(keys::READER_ZOOM, v.clamp(0.25, 4.0));
    }
    if let Some(v) = &patch.library_sort {
        one_of(v, SORTS, "sort")?;
        s.set(keys::LIBRARY_SORT, v.as_str());
    }
    let bools = [
        (patch.reader_cover_solo, keys::READER_COVER_SOLO),
        (patch.library_grid, keys::LIBRARY_GRID),
        (patch.library_sort_descending, keys::LIBRARY_SORT_DESCENDING),
        (patch.library_unread_only, keys::LIBRARY_UNREAD_ONLY),
        (patch.library_downloaded_only, keys::LIBRARY_DOWNLOADED_ONLY),
        (patch.rotated, keys::ROTATED),
        (
            patch.cloudflare_bypass_enabled,
            keys::CLOUDFLARE_BYPASS_ENABLED,
        ),
    ];
    for (v, key) in bools {
        if let Some(v) = v {
            s.set(key, v);
        }
    }
    if let Some(mb) = patch.disk_cache_mb {
        let mb = mb.clamp(256, 1024 * 1024);
        s.set(keys::DISK_CACHE_MB, mb);
        let bytes = u64::try_from(mb)
            .unwrap_or(10240)
            .saturating_mul(1024 * 1024);
        state.cache.set_cap(bytes)?;
    }
    s.save()?;
    if let Some(enabled) = patch.cloudflare_bypass_enabled {
        crate::online::push_cloudflare(&state, enabled);
    }
    Ok(ui_settings(&s))
}

fn usage(state: &AppState) -> R<CacheUsageDto> {
    let u = state.cache.usage()?;
    // Byte counts go to JS as numbers; exact below 2^53 (8 PiB).
    #[allow(clippy::cast_precision_loss)]
    Ok(CacheUsageDto {
        bytes: u.bytes as f64,
        entries: u32::try_from(u.entries).unwrap_or(u32::MAX),
        cap_bytes: u.cap_bytes as f64,
    })
}

#[tauri::command]
pub fn cache_usage(state: State<'_, AppState>) -> R<CacheUsageDto> {
    usage(&state)
}

#[tauri::command]
pub fn cache_clear(state: State<'_, AppState>) -> R<CacheUsageDto> {
    state.cache.clear()?;
    usage(&state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tm_db::{Db, SourceChapter, SourceManga};

    fn ch(url: &str) -> SourceChapter {
        SourceChapter {
            url: url.into(),
            name: url.into(),
            date_upload: 0,
            chapter_number: 0.0,
            scanlator: None,
        }
    }

    #[test]
    fn latest_chapter_respects_each_ordering() {
        let db = Db::open_in_memory().unwrap();
        let m = |url: &str| SourceManga {
            url: url.into(),
            title: url.into(),
            ..Default::default()
        };
        // Online: the source lists newest first, so index 0 ("c3") is the latest.
        let online = db
            .add_to_library(7, &m("/on"), &[ch("c3"), ch("c2"), ch("c1")])
            .unwrap();
        // Local: natural order, so the highest index ("Ch 2") is the latest.
        let local = db
            .add_to_library(0, &m("loc"), &[ch("Ch 1"), ch("Ch 2")])
            .unwrap();
        let names = latest_names(&db).unwrap();
        assert_eq!(names.get(&online).map(String::as_str), Some("c3"));
        assert_eq!(names.get(&local).map(String::as_str), Some("Ch 2"));
    }

    #[test]
    fn latest_chapter_reads_the_chapters_once() {
        // The plan must not re-scan chapters per chapter (that made big libraries take minutes).
        let db = Db::open_in_memory().unwrap();
        let plan: Vec<String> = db
            .read(|c| {
                let mut st = c.prepare(&format!("EXPLAIN QUERY PLAN {LATEST_SQL}"))?;
                let rows = st.query_map([], |r| r.get::<_, String>(3))?;
                Ok(rows.collect::<Result<Vec<_>, _>>()?)
            })
            .unwrap();
        assert!(plan.iter().all(|p| !p.contains("CORRELATED")), "{plan:?}");
    }
}
