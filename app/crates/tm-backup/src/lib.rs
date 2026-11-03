//! Mihon-lineage backups: build one from the library database and apply one back.
//! Ported from `desktop/.../sync/BackupRepo.kt`; the protobuf is in [`model`], the container in
//! [`codec`].
//!
//! Import upserts manga by (source, url) and chapters by (url, manga), so a repeated restore or
//! sync is idempotent. Read progress only moves forward. Categories are matched by name and added,
//! never removed. Extension stores the backup lists are added (data only; nothing is installed).
//! What a restore can then offer — the extensions to install, the chapters to download again,
//! the trackers to sign in to — comes back in [`ImportSummary`] for the app to ask about.

pub mod codec;
pub mod model;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Component, Path};

use rusqlite::{named_params, params, Connection, OptionalExtension};
use tm_db::{q, Category, Chapter, Db, ExtensionRepo, Manga, Track, LOCAL_SOURCE};

pub use model::*;
pub use prost;

#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    #[error(transparent)]
    Db(#[from] tm_db::DbError),
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("I/O error: {0}")]
    Io(std::io::Error),
    #[error("{0}")]
    Format(String),
}

pub type Result<T> = std::result::Result<T, BackupError>;

/// What an import did, and what it can offer next, for the UI.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct ImportSummary {
    pub manga_added: usize,
    pub manga_updated: usize,
    pub chapters_added: usize,
    pub chapters_updated: usize,
    pub categories_added: usize,
    /// Extension stores this install did not have, now added.
    pub repos_added: Vec<BackupExtensionStore>,
    /// The extensions the backup says were installed (tag 900), first copy of each package.
    pub extensions: Vec<BackupExtension>,
    /// Online source ids the library uses, for backups without tag 900 (stock Mihon).
    pub source_ids: Vec<i64>,
    /// Chapters marked downloaded on the device that wrote the backup, as rows here.
    pub downloaded: Vec<DownloadedChapter>,
    /// Tracker ids (`sync_id`) with at least one restored row.
    pub trackers: Vec<i64>,
    /// Manga that could not be restored (each rolled back on its own), at most [`MAX_ERRORS`].
    pub errors: Vec<RestoreError>,
}

/// A manga a restore skipped, and why.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RestoreError {
    pub title: String,
    pub message: String,
}

/// Cap on the errors an import reports.
pub const MAX_ERRORS: usize = 500;

/// A chapter that was downloaded on the backup's device, mapped to this library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct DownloadedChapter {
    pub manga_id: i64,
    pub chapter_id: i64,
    pub read: bool,
}

/// Cap on the extensions an import reports; a real install has a few hundred at most.
const MAX_EXTENSIONS: usize = 2_000;
/// Cap on the downloaded chapters an import reports.
const MAX_DOWNLOADED: usize = 100_000;

const KIND_PREFERENCES: &str = "preferences";
const KIND_SOURCE_PREFERENCES: &str = "source_preferences";

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------------------------

/// Build a backup of the favourited library. `source_names` labels the sources section
/// (unknown ids are written as "Source <id>", like the Kotlin app).
pub fn export(db: &Db, source_names: &HashMap<i64, String>) -> Result<Backup> {
    Ok(db.read(|c| export_in(c, source_names).map_err(into_db))?)
}

/// Inside a database closure only database errors can occur; keep their type.
fn into_db(e: BackupError) -> tm_db::DbError {
    match e {
        BackupError::Db(e) => e,
        BackupError::Sqlite(e) => e.into(),
        other => tm_db::DbError::NotFound(other.to_string()),
    }
}

fn export_in(c: &Connection, source_names: &HashMap<i64, String>) -> Result<Backup> {
    let mut st = c.prepare_cached(q("mangas", "findAll")?)?;
    let mangas = st
        .query_map([], Manga::from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .filter(|m| m.favorite)
        .collect::<Vec<_>>();

    let mut cats = all_categories(c)?;
    cats.sort_by_key(|k| k.sort);
    let backup_categories = cats
        .iter()
        .map(|k| BackupCategory {
            name: Some(k.name.clone()),
            order: k.sort,
            id: k.id,
            flags: k.flags,
        })
        .collect();

    let mut backup_manga = Vec::with_capacity(mangas.len());
    for m in &mangas {
        backup_manga.push(export_manga(c, m)?);
    }

    let mut seen = BTreeSet::new();
    let backup_sources = mangas
        .iter()
        .map(|m| m.source)
        .filter(|id| seen.insert(*id))
        .map(|id| BackupSource {
            name: source_names.get(&id).cloned().unwrap_or_else(|| {
                if id == LOCAL_SOURCE {
                    "Local source".to_owned()
                } else {
                    format!("Source {id}")
                }
            }),
            source_id: Some(id),
        })
        .collect();

    let mut st = c.prepare_cached(q("extension_repos", "findAll")?)?;
    let backup_extension_stores = st
        .query_map([], ExtensionRepo::from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(|r| BackupExtensionStore {
            index_url: Some(r.base_url),
            name: Some(r.name),
            badge_label: r.short_name,
            contact_website: r.website,
            signing_key: r.signing_key_fingerprint,
            ..Default::default()
        })
        .collect();

    Ok(Backup {
        backup_manga,
        backup_categories,
        backup_sources,
        backup_preferences: passthrough(c, KIND_PREFERENCES)?,
        backup_source_preferences: passthrough(c, KIND_SOURCE_PREFERENCES)?,
        backup_extension_stores,
        // Only the app knows these; it fills them in with `annotate`.
        backup_extensions: Vec::new(),
    })
}

/// Add what only the app knows to an exported backup: the installed extensions (tag 900) and,
/// per chapter, whether it is downloaded here. `is_downloaded(source, manga_url, chapter_url)`.
pub fn annotate(
    backup: &mut Backup,
    extensions: Vec<BackupExtension>,
    is_downloaded: impl Fn(i64, &str, &str) -> bool,
) {
    backup.backup_extensions = extensions;
    for bm in &mut backup.backup_manga {
        let (source, url) = (bm.source(), bm.url().to_owned());
        if source == LOCAL_SOURCE {
            continue;
        }
        for bc in &mut bm.chapters {
            if is_downloaded(source, &url, bc.url()) {
                bc.downloaded = Some(true);
            }
        }
    }
}

fn export_manga(c: &Connection, m: &Manga) -> Result<BackupManga> {
    let mut chapters = tm_db::chapters_tx(c, m.id)?;
    chapters.sort_by_key(|ch| ch.source_order);
    let chapters = chapters.iter().map(export_chapter).collect();

    let tracking = tracks(c, m.id)?
        .into_iter()
        .map(|t| BackupTracking {
            sync_id: Some(i32::try_from(t.sync_id).unwrap_or(0)),
            library_id: Some(t.library_id.unwrap_or(0)),
            media_id_int: i32::try_from(t.remote_id).unwrap_or(0),
            tracking_url: t.remote_url,
            title: t.title,
            last_chapter_read: t.last_chapter_read as f32,
            total_chapters: i32::try_from(t.total_chapters).unwrap_or(0),
            score: t.score as f32,
            status: i32::try_from(t.status).unwrap_or(0),
            started_reading_date: t.start_date,
            finished_reading_date: t.finish_date,
            private: false,
            media_id: t.remote_id,
        })
        .collect();

    let mut st = c.prepare_cached(q("categories", "findAllByMangaId")?)?;
    let categories = st
        .query_map(named_params! {":mangaId": m.id}, Category::from_row)?
        .map(|r| r.map(|k| k.sort))
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut st = c.prepare_cached(
        "SELECT chapters.url, history.history_last_read, history.history_time_read \
         FROM history JOIN chapters ON history.history_chapter_id = chapters._id \
         WHERE chapters.manga_id = ?1 AND ifnull(history.history_last_read, 0) > 0 \
         ORDER BY history.history_last_read",
    )?;
    let history = st
        .query_map(params![m.id], |r| {
            Ok(BackupHistory {
                url: Some(r.get(0)?),
                last_read: Some(r.get::<_, Option<i64>>(1)?.unwrap_or(0)),
                read_duration: r.get::<_, Option<i64>>(2)?.unwrap_or(0),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(BackupManga {
        source: Some(m.source),
        url: Some(m.url.clone()),
        title: m.title.clone(),
        artist: m.artist.clone(),
        author: m.author.clone(),
        description: m.description.clone(),
        genre: m
            .genre
            .as_deref()
            .unwrap_or("")
            .split(", ")
            .filter(|g| !g.trim().is_empty())
            .map(str::to_owned)
            .collect(),
        status: i32::try_from(m.status).unwrap_or(0),
        thumbnail_url: m.thumbnail_url.clone(),
        date_added: m.date_added.unwrap_or(0),
        viewer: 0,
        chapters,
        categories,
        tracking,
        favorite: Some(m.favorite),
        chapter_flags: i32::try_from(m.chapter_flags).unwrap_or(0),
        viewer_flags: Some(i32::try_from(m.viewer).unwrap_or(0)),
        history,
        update_strategy: i32::try_from(m.update_strategy).unwrap_or(0),
        last_modified_at: m.last_modified_at,
        favorite_modified_at: m.favorite_modified_at,
        excluded_scanlators: Vec::new(),
        version: m.version,
        notes: m.notes.clone(),
        initialized: m.initialized,
        memo: Some(m.memo.clone()),
    })
}

fn export_chapter(ch: &Chapter) -> BackupChapter {
    BackupChapter {
        url: Some(ch.url.clone()),
        name: Some(ch.name.clone()),
        scanlator: ch.scanlator.clone(),
        read: ch.read,
        bookmark: ch.bookmark,
        last_page_read: ch.last_page_read,
        date_fetch: ch.date_fetch,
        date_upload: ch.date_upload,
        chapter_number: ch.chapter_number as f32,
        source_order: ch.source_order,
        last_modified_at: ch.last_modified_at,
        version: ch.version,
        memo: Some(ch.memo.clone()),
        pages_left: i32::try_from(ch.pages_left).unwrap_or(0),
        downloaded: None,
    }
}

fn all_categories(c: &Connection) -> Result<Vec<Category>> {
    let mut st = c.prepare_cached(q("categories", "findAll")?)?;
    let v = st
        .query_map([], Category::from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

fn tracks(c: &Connection, manga_id: i64) -> Result<Vec<Track>> {
    let mut st = c.prepare_cached(q("manga_sync", "getAllByMangaId")?)?;
    let v = st
        .query_map(named_params! {":mangaId": manga_id}, Track::from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

fn passthrough(c: &Connection, kind: &str) -> Result<Vec<Vec<u8>>> {
    let mut st = c.prepare_cached(q("backup_passthrough", "findByKind")?)?;
    let v = st
        .query_map(named_params! {":kind": kind}, |r| r.get::<_, Vec<u8>>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

// ---------------------------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------------------------

/// Apply a backup to the database. Categories, then manga in batches of [`BATCH`], then repos
/// and preferences, each in its own transaction so the app can read in between. A manga that
/// cannot be written is rolled back on its own and listed in [`ImportSummary::errors`].
pub fn import(db: &Db, backup: &Backup) -> Result<ImportSummary> {
    import_with(db, backup, &mut |_, _, _| {})
}

/// [`import`], calling `progress(done, total, title)` before each manga and once at the end
/// (`done == total`). A manga that fails is rolled back on its own and listed in `errors`.
pub fn import_with(
    db: &Db,
    backup: &Backup,
    progress: &mut dyn FnMut(usize, usize, &str),
) -> Result<ImportSummary> {
    let now = now_ms();
    let mut s = ImportSummary::default();
    let category_by_order = db.write(|c| import_categories(c, backup, &mut s).map_err(into_db))?;
    let total = backup.backup_manga.len();
    let mut done = 0usize;
    for chunk in backup.backup_manga.chunks(BATCH) {
        db.write(|c| {
            for bm in chunk {
                progress(done, total, &bm.title);
                done = done.saturating_add(1);
                import_one(c, bm, now, &category_by_order, &mut s).map_err(into_db)?;
            }
            Ok(())
        })?;
    }
    progress(total, total, "");
    db.write(|c| import_tail(c, backup, &mut s).map_err(into_db))?;
    Ok(s)
}

/// Manga per transaction: big enough to be fast, small enough that the app is never locked out
/// of the database for long.
pub const BATCH: usize = 25;

/// Add the backup's categories by name; returns backup order value -> our category id.
fn import_categories(
    c: &Connection,
    backup: &Backup,
    s: &mut ImportSummary,
) -> Result<HashMap<i64, i64>> {
    // Categories first, matched by name (the cross-app key).
    let mut by_name: HashMap<String, Category> = all_categories(c)?
        .into_iter()
        .map(|k| (k.name.to_lowercase(), k))
        .collect();
    let mut next_sort = by_name
        .values()
        .map(|k| k.sort)
        .max()
        .map_or(0, |m| m.saturating_add(1));
    for bc in &backup.backup_categories {
        let name = bc.name().trim();
        if name.is_empty() || by_name.contains_key(&name.to_lowercase()) {
            continue;
        }
        c.execute(
            q("categories", "insert")?,
            named_params! {":name": name, ":sort": next_sort, ":flags": bc.flags, ":mangaOrder": ""},
        )?;
        let id = c.last_insert_rowid();
        by_name.insert(
            name.to_lowercase(),
            Category {
                id,
                name: name.to_owned(),
                sort: next_sort,
                flags: bc.flags,
                manga_order: String::new(),
            },
        );
        next_sort = next_sort.saturating_add(1);
        s.categories_added += 1;
    }
    Ok(backup
        .backup_categories
        .iter()
        .filter_map(|bc| {
            by_name
                .get(&bc.name().trim().to_lowercase())
                .map(|k| (bc.order, k.id))
        })
        .collect())
}

/// One manga in a savepoint: a failure rolls back only this manga and is reported.
fn import_one(
    c: &Connection,
    bm: &BackupManga,
    now: i64,
    category_by_order: &HashMap<i64, i64>,
    s: &mut ImportSummary,
) -> Result<()> {
    if bm.url().is_empty() {
        return Ok(());
    }
    let mark = Mark::of(s);
    c.execute_batch("SAVEPOINT tm_manga")?;
    match import_manga(c, bm, now, category_by_order, s) {
        Ok(()) => c.execute_batch("RELEASE tm_manga")?,
        Err(e) => {
            c.execute_batch("ROLLBACK TO tm_manga; RELEASE tm_manga")?;
            mark.restore(s);
            let title = if bm.title.trim().is_empty() {
                bm.url()
            } else {
                bm.title.as_str()
            };
            tracing::warn!(%title, "manga not restored: {e}");
            if s.errors.len() < MAX_ERRORS {
                s.errors.push(RestoreError {
                    title: title.to_owned(),
                    message: e.to_string(),
                });
            }
        }
    }
    Ok(())
}

/// Repos, the extension and source lists for the restore offer, and preferences.
fn import_tail(c: &Connection, backup: &Backup, s: &mut ImportSummary) -> Result<()> {
    // Stores are added (they are only addresses and a signing key); installing from them is the
    // user's call, offered from `extensions` / `source_ids`.
    let have: HashSet<String> = {
        let mut st = c.prepare_cached(q("extension_repos", "findAll")?)?;
        let v = st
            .query_map([], ExtensionRepo::from_row)?
            .map(|r| r.map(|x| x.base_url.trim_end_matches('/').to_owned()))
            .collect::<rusqlite::Result<_>>()?;
        v
    };
    let mut seen = HashSet::new();
    for store in &backup.backup_extension_stores {
        let key = store.index_url().trim().trim_end_matches('/').to_owned();
        if key.is_empty() || have.contains(&key) || !seen.insert(key.clone()) {
            continue;
        }
        if !(key.starts_with("https://") || key.starts_with("http://")) {
            tracing::warn!(url = %key, "backup lists a repo that is not a web address; skipped");
            continue;
        }
        c.execute(
            q("extension_repos", "upsert")?,
            named_params! {":base_url": key, ":name": if store.name().trim().is_empty() {
                key.as_str() } else { store.name() },
            ":short_name": store.badge_label, ":website": store.contact_website,
            ":fingerprint": store.signing_key},
        )?;
        s.repos_added.push(store.clone());
    }

    let mut pkgs = HashSet::new();
    s.extensions = backup
        .backup_extensions
        .iter()
        .filter(|e| !e.pkg_name.trim().is_empty() && pkgs.insert(e.pkg_name.clone()))
        .take(MAX_EXTENSIONS)
        .cloned()
        .collect();
    let mut ids = BTreeSet::new();
    s.source_ids = backup
        .backup_sources
        .iter()
        .filter_map(|b| b.source_id)
        .chain(backup.backup_manga.iter().map(BackupManga::source))
        .filter(|id| *id != LOCAL_SOURCE && ids.insert(*id))
        .take(MAX_EXTENSIONS)
        .collect();

    // Park what we don't interpret so a re-export hands it back unchanged.
    park(c, KIND_PREFERENCES, &backup.backup_preferences)?;
    park(
        c,
        KIND_SOURCE_PREFERENCES,
        &backup.backup_source_preferences,
    )?;
    Ok(())
}

/// Counters an aborted manga must not leave behind.
struct Mark {
    manga_added: usize,
    manga_updated: usize,
    chapters_added: usize,
    chapters_updated: usize,
    downloaded: usize,
    trackers: usize,
}

impl Mark {
    fn of(s: &ImportSummary) -> Self {
        Self {
            manga_added: s.manga_added,
            manga_updated: s.manga_updated,
            chapters_added: s.chapters_added,
            chapters_updated: s.chapters_updated,
            downloaded: s.downloaded.len(),
            trackers: s.trackers.len(),
        }
    }

    fn restore(&self, s: &mut ImportSummary) {
        s.manga_added = self.manga_added;
        s.manga_updated = self.manga_updated;
        s.chapters_added = self.chapters_added;
        s.chapters_updated = self.chapters_updated;
        s.downloaded.truncate(self.downloaded);
        s.trackers.truncate(self.trackers);
    }
}

fn import_manga(
    c: &Connection,
    bm: &BackupManga,
    now: i64,
    category_by_order: &HashMap<i64, i64>,
    s: &mut ImportSummary,
) -> Result<()> {
    let existing = tm_db::manga_by_url_tx(c, bm.url(), bm.source())?;
    let manga_id = match &existing {
        Some(m) => {
            s.manga_updated += 1;
            m.id
        }
        None => {
            s.manga_added += 1;
            insert_manga(c, bm, now)?
        }
    };

    // Mihon fields: keep whichever side was modified later, so a restore never rewinds
    // the user's own edits.
    if bm.last_modified_at >= existing.as_ref().map_or(0, |m| m.last_modified_at) {
        c.execute(
            q("mangas", "setMihonFields")?,
            named_params! {":notes": bm.notes, ":version": bm.version,
            ":lastModifiedAt": bm.last_modified_at,
            ":favoriteModifiedAt": bm.favorite_modified_at, ":memo": bm.memo(),
            ":mangaId": manga_id},
        )?;
    }

    import_chapters(c, manga_id, bm, now, s)?;
    if bm.source() != LOCAL_SOURCE && bm.chapters.iter().any(|bc| bc.downloaded()) {
        note_downloaded(c, manga_id, bm, s)?;
    }
    import_history(c, manga_id, bm)?;

    // Category membership is additive.
    if !bm.categories.is_empty() {
        let mut st = c.prepare_cached(q("categories", "findAllByMangaId")?)?;
        let already = st
            .query_map(named_params! {":mangaId": manga_id}, Category::from_row)?
            .map(|r| r.map(|k| k.id))
            .collect::<rusqlite::Result<HashSet<_>>>()?;
        let wanted: BTreeSet<i64> = bm
            .categories
            .iter()
            .filter_map(|o| category_by_order.get(o).copied())
            .filter(|id| !already.contains(id))
            .collect();
        for cat in wanted {
            c.execute(
                q("mangas_categories", "insert")?,
                named_params! {":mangaId": manga_id, ":categoryId": cat},
            )?;
        }
    }

    import_tracking(c, manga_id, bm)?;
    for t in &bm.tracking {
        if let Some(id) = t.sync_id.map(i64::from) {
            if !s.trackers.contains(&id) {
                s.trackers.push(id);
            }
        }
    }
    Ok(())
}

fn insert_manga(c: &Connection, bm: &BackupManga, now: i64) -> Result<i64> {
    let title = if bm.title.trim().is_empty() {
        bm.url()
    } else {
        bm.title.as_str()
    };
    let genre = (!bm.genre.is_empty()).then(|| bm.genre.join(", "));
    // A local cover path is absolute on the sender; relink after the files arrive. Remote
    // thumbnails are URLs and stay.
    let thumb = if bm.source() == LOCAL_SOURCE {
        None
    } else {
        bm.thumbnail_url.clone()
    };
    let date_added = if bm.date_added != 0 {
        bm.date_added
    } else {
        now
    };
    c.execute(
        q("mangas", "insert")?,
        named_params! {
            ":source": bm.source(), ":url": bm.url(), ":artist": bm.artist,
            ":author": bm.author, ":description": bm.description, ":genre": genre,
            ":title": title, ":status": i64::from(bm.status), ":thumbnailUrl": thumb,
            ":favorite": bm.favorite(), ":lastUpdate": now, ":initialized": true,
            ":viewer": i64::from(bm.viewer_flags.unwrap_or(bm.viewer)), ":hideTitle": false,
            ":chapterFlags": i64::from(bm.chapter_flags), ":dateAdded": date_added,
            ":filteredScanlators": None::<String>,
            ":updateStrategy": i64::from(bm.update_strategy), ":coverLastModified": 0i64,
        },
    )?;
    Ok(c.last_insert_rowid())
}

fn import_chapters(
    c: &Connection,
    manga_id: i64,
    bm: &BackupManga,
    now: i64,
    s: &mut ImportSummary,
) -> Result<()> {
    let mut existing: HashMap<String, Chapter> = tm_db::chapters_tx(c, manga_id)?
        .into_iter()
        .map(|ch| (ch.url.clone(), ch))
        .collect();
    for bc in &bm.chapters {
        if bc.url().is_empty() {
            continue;
        }
        match existing.get(bc.url()) {
            None => {
                c.execute(
                    q("chapters", "insert")?,
                    named_params! {
                        ":mangaId": manga_id, ":url": bc.url(), ":name": bc.name(),
                        ":scanlator": bc.scanlator, ":read": bc.read, ":bookmark": bc.bookmark,
                        ":lastPageRead": bc.last_page_read,
                        ":pagesLeft": i64::from(bc.pages_left),
                        ":chapterNumber": f64::from(bc.chapter_number),
                        ":sourceOrder": bc.source_order,
                        ":dateFetch": if bc.date_fetch != 0 { bc.date_fetch } else { now },
                        ":dateUpload": bc.date_upload,
                    },
                )?;
                let id = c.last_insert_rowid();
                c.execute(
                    q("chapters", "setMihonFields")?,
                    named_params! {":lastModifiedAt": bc.last_modified_at,
                    ":version": bc.version, ":memo": bc.memo(), ":chapterId": id},
                )?;
                s.chapters_added += 1;
                // A backup can list a chapter twice; the second copy merges into the first.
                if let Some(row) = tm_db::chapter_tx(c, id)? {
                    existing.insert(row.url.clone(), row);
                }
            }
            Some(ex) => {
                // Furthest progress wins.
                let read = ex.read || bc.read;
                let page = ex.last_page_read.max(bc.last_page_read);
                let bookmark = ex.bookmark || bc.bookmark;
                if read != ex.read || page != ex.last_page_read || bookmark != ex.bookmark {
                    tm_db::update_chapter_tx(
                        c,
                        ex.id,
                        &tm_db::ChapterUpdate {
                            read: Some(read),
                            bookmark: Some(bookmark),
                            last_page_read: Some(page),
                            ..Default::default()
                        },
                    )?;
                    s.chapters_updated += 1;
                    let id = ex.id;
                    if let Some(e) = existing.get_mut(bc.url()) {
                        e.read = read;
                        e.last_page_read = page;
                        e.bookmark = bookmark;
                    } else {
                        tracing::debug!(id, "chapter vanished during import");
                    }
                }
            }
        }
    }
    Ok(())
}

/// History by chapter url; the later `last_read` and the longer duration win.
fn import_history(c: &Connection, manga_id: i64, bm: &BackupManga) -> Result<()> {
    for h in &bm.history {
        let (Some(url), Some(last_read)) = (h.url.as_deref(), h.last_read) else {
            continue;
        };
        if last_read <= 0 {
            continue;
        }
        let chapter_id: Option<i64> = c
            .query_row(
                "SELECT _id FROM chapters WHERE manga_id = ?1 AND url = ?2",
                params![manga_id, url],
                |r| r.get(0),
            )
            .optional()?;
        let Some(chapter_id) = chapter_id else {
            continue;
        };
        c.execute(
            "INSERT INTO history(history_chapter_id, history_last_read, history_time_read) \
             VALUES (?1, ?2, ?3) ON CONFLICT(history_chapter_id) DO UPDATE SET \
             history_last_read = max(ifnull(history_last_read, 0), excluded.history_last_read), \
             history_time_read = max(ifnull(history_time_read, 0), excluded.history_time_read)",
            params![chapter_id, last_read, h.read_duration.max(0)],
        )?;
    }
    Ok(())
}

/// Tracking rows keyed by (manga, service); an existing row only moves forward.
/// Record this manga's chapters the backup marks downloaded, as rows of this library.
fn note_downloaded(
    c: &Connection,
    manga_id: i64,
    bm: &BackupManga,
    s: &mut ImportSummary,
) -> Result<()> {
    let wanted: HashSet<&str> = bm
        .chapters
        .iter()
        .filter(|bc| bc.downloaded())
        .map(BackupChapter::url)
        .collect();
    for ch in tm_db::chapters_tx(c, manga_id)? {
        if s.downloaded.len() >= MAX_DOWNLOADED {
            break;
        }
        if wanted.contains(ch.url.as_str()) {
            s.downloaded.push(DownloadedChapter {
                manga_id,
                chapter_id: ch.id,
                read: ch.read,
            });
        }
    }
    Ok(())
}

fn import_tracking(c: &Connection, manga_id: i64, bm: &BackupManga) -> Result<()> {
    let existing = tracks(c, manga_id)?;
    for t in &bm.tracking {
        let Some(sync_id) = t.sync_id else { continue };
        let remote_id = if t.media_id != 0 {
            t.media_id
        } else {
            i64::from(t.media_id_int)
        };
        if remote_id == 0 {
            continue;
        }
        match existing.iter().find(|e| e.sync_id == i64::from(sync_id)) {
            None => {
                c.execute(
                    q("manga_sync", "insert")?,
                    named_params! {
                        ":mangaId": manga_id, ":syncId": i64::from(sync_id),
                        ":remoteId": remote_id, ":libraryId": t.library_id.unwrap_or(0),
                        ":title": t.title, ":lastChapterRead": f64::from(t.last_chapter_read),
                        ":totalChapters": i64::from(t.total_chapters),
                        ":status": i64::from(t.status), ":score": f64::from(t.score),
                        ":remoteUrl": t.tracking_url, ":startDate": t.started_reading_date,
                        ":finishDate": t.finished_reading_date,
                    },
                )?;
            }
            Some(ex) if f64::from(t.last_chapter_read) > ex.last_chapter_read => {
                c.execute(
                    q("manga_sync", "update")?,
                    named_params! {
                        ":mangaId": None::<i64>, ":syncId": None::<i64>, ":remoteId": None::<i64>,
                        ":libraryId": None::<i64>, ":title": None::<String>,
                        ":lastChapterRead": f64::from(t.last_chapter_read),
                        ":totalChapters": None::<i64>, ":status": None::<i64>,
                        ":score": None::<f64>, ":remoteUrl": None::<String>,
                        ":startDate": None::<i64>, ":finishDate": None::<i64>, ":id": ex.id,
                    },
                )?;
            }
            Some(_) => {}
        }
    }
    Ok(())
}

/// Replace the parked blobs of `kind`. Last backup wins: these are whole-app settings, and a
/// merge of two sets would be a configuration nobody chose.
fn park(c: &Connection, kind: &str, blobs: &[Vec<u8>]) -> Result<()> {
    if blobs.is_empty() {
        return Ok(());
    }
    c.execute(
        q("backup_passthrough", "deleteByKind")?,
        named_params! {":kind": kind},
    )?;
    for b in blobs {
        c.execute(
            q("backup_passthrough", "insert")?,
            named_params! {":kind": kind, ":payload": b},
        )?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// After an import
// ---------------------------------------------------------------------------------------------

/// Point local-source manga at their cover file under `local_root` on this machine. Call after
/// a pull or restore: a backup's local thumbnail path belongs to the sender.
pub fn relink_local_covers(db: &Db, local_root: &Path) -> Result<usize> {
    let mangas = db.library()?;
    let mut changed = 0;
    for m in mangas.iter().filter(|m| m.source == LOCAL_SOURCE) {
        if !is_single_component(&m.url) {
            continue;
        }
        let Some(cover) = tm_local::find_cover(&local_root.join(&m.url)) else {
            continue;
        };
        let path = cover.to_string_lossy().into_owned();
        if m.thumbnail_url.as_deref() == Some(path.as_str()) {
            continue;
        }
        let modified = tm_local::mtime_ms(&cover);
        db.write(|c| {
            c.execute(
                "UPDATE mangas SET thumbnail_url = ?1, cover_last_modified = ?2 WHERE _id = ?3",
                params![path, modified, m.id],
            )?;
            Ok(())
        })?;
        changed += 1;
    }
    Ok(changed)
}

/// A local manga url is one folder name; anything else could point outside the library.
fn is_single_component(url: &str) -> bool {
    let mut comps = Path::new(url).components();
    matches!(
        (comps.next(), comps.next()),
        (Some(Component::Normal(_)), None)
    )
}
