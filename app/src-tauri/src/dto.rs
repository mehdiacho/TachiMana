//! Types that cross the IPC boundary. Each derives `TS`; `cargo test -p tachimana` regenerates
//! `app/ui/src/gen/*.ts` so the UI never hand-copies a shape.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Library manga as the UI needs it (a subset of the `mangas` row).
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MangaDto {
    #[ts(type = "number")]
    pub id: i64,
    #[ts(type = "number")]
    pub source: i64,
    pub url: String,
    pub title: String,
    pub author: Option<String>,
    pub artist: Option<String>,
    pub description: Option<String>,
    pub genres: Vec<String>,
    #[ts(type = "number")]
    pub status: i64,
    #[ts(type = "number")]
    pub viewer: i64,
    #[ts(type = "number")]
    pub date_added: i64,
    #[ts(type = "number")]
    pub last_update: i64,
    pub is_local: bool,
    /// Bumped when the cover changes, so `tm://cover/<id>?v=<coverVersion>` busts the cache.
    #[ts(type = "number")]
    pub cover_version: i64,
}

impl From<&tm_db::Manga> for MangaDto {
    fn from(m: &tm_db::Manga) -> Self {
        Self {
            id: m.id,
            source: m.source,
            url: m.url.clone(),
            title: m.title.clone(),
            author: m.author.clone().filter(|s| !s.trim().is_empty()),
            artist: m.artist.clone().filter(|s| !s.trim().is_empty()),
            description: m.description.clone().filter(|s| !s.trim().is_empty()),
            genres: m
                .genre
                .as_deref()
                .unwrap_or("")
                .split(',')
                .map(str::trim)
                .filter(|g| !g.is_empty())
                .map(str::to_owned)
                .collect(),
            status: m.status,
            viewer: m.viewer,
            date_added: m.date_added.unwrap_or(0),
            last_update: m.last_update.unwrap_or(0),
            is_local: m.is_local(),
            cover_version: m.cover_last_modified,
        }
    }
}

/// One library row: the manga plus what the list needs to filter and sort without more calls.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LibraryEntryDto {
    pub manga: MangaDto,
    #[ts(type = "number")]
    pub unread: i64,
    #[ts(type = "Array<number>")]
    pub category_ids: Vec<i64>,
    /// Has at least one downloaded chapter (downloads arrive in P6; false until then).
    pub downloaded: bool,
    /// Name of the most recently uploaded chapter, for the row subtitle.
    pub latest_chapter: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CategoryDto {
    #[ts(type = "number")]
    pub id: i64,
    pub name: String,
    #[ts(type = "number")]
    pub sort: i64,
}

impl From<&tm_db::Category> for CategoryDto {
    fn from(c: &tm_db::Category) -> Self {
        Self {
            id: c.id,
            name: c.name.clone(),
            sort: c.sort,
        }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChapterDto {
    #[ts(type = "number")]
    pub id: i64,
    #[ts(type = "number")]
    pub manga_id: i64,
    pub name: String,
    pub scanlator: Option<String>,
    pub read: bool,
    pub bookmark: bool,
    #[ts(type = "number")]
    pub last_page_read: i64,
    #[ts(type = "number")]
    pub pages_left: i64,
    pub chapter_number: f64,
    #[ts(type = "number")]
    pub source_order: i64,
    #[ts(type = "number")]
    pub date_upload: i64,
}

impl From<&tm_db::Chapter> for ChapterDto {
    fn from(c: &tm_db::Chapter) -> Self {
        Self {
            id: c.id,
            manga_id: c.manga_id,
            name: c.name.clone(),
            scanlator: c.scanlator.clone(),
            read: c.read,
            bookmark: c.bookmark,
            last_page_read: c.last_page_read,
            pages_left: c.pages_left,
            chapter_number: c.chapter_number,
            source_order: c.source_order,
            date_upload: c.date_upload,
        }
    }
}

/// Manga detail: the manga and its chapters, newest first (by `source_order`).
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MangaDetailDto {
    pub manga: MangaDto,
    pub chapters: Vec<ChapterDto>,
    #[ts(type = "Array<number>")]
    pub category_ids: Vec<i64>,
}

/// A reader page: its pixel size when the header could be read, so the strip can reserve the
/// right height before the image loads (no jumping).
#[derive(Debug, Clone, Copy, Serialize, TS)]
#[ts(export)]
pub struct PageDto {
    pub w: Option<u32>,
    pub h: Option<u32>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChapterPagesDto {
    #[ts(type = "number")]
    pub chapter_id: i64,
    pub pages: Vec<PageDto>,
}

#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImportResultDto {
    pub manga_added: u32,
    pub chapters_added: u32,
    pub manga_total: u32,
}

impl From<tm_local::ImportResult> for ImportResultDto {
    fn from(r: tm_local::ImportResult) -> Self {
        let n = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);
        Self {
            manga_added: n(r.manga_added),
            chapters_added: n(r.chapters_added),
            manga_total: n(r.manga_total),
        }
    }
}

/// Settings the UI reads and writes. Keys and defaults are shared with the Kotlin app where
/// they exist there (see `tm_core::settings::keys`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UiSettings {
    pub theme: String,
    pub reader_fit: String,
    pub reader_zoom: f64,
    pub reader_cover_solo: bool,
    pub library_grid: bool,
    pub library_sort: String,
    pub library_sort_descending: bool,
    pub library_unread_only: bool,
    pub library_downloaded_only: bool,
    #[ts(type = "number")]
    pub disk_cache_mb: i64,
    pub rotated: bool,
}

/// A partial update: only the fields present are written.
#[derive(Debug, Clone, Default, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UiSettingsPatch {
    #[ts(optional)]
    pub theme: Option<String>,
    #[ts(optional)]
    pub reader_fit: Option<String>,
    #[ts(optional)]
    pub reader_zoom: Option<f64>,
    #[ts(optional)]
    pub reader_cover_solo: Option<bool>,
    #[ts(optional)]
    pub library_grid: Option<bool>,
    #[ts(optional)]
    pub library_sort: Option<String>,
    #[ts(optional)]
    pub library_sort_descending: Option<bool>,
    #[ts(optional)]
    pub library_unread_only: Option<bool>,
    #[ts(optional)]
    pub library_downloaded_only: Option<bool>,
    #[ts(optional, as = "Option<f64>")]
    pub disk_cache_mb: Option<i64>,
    #[ts(optional)]
    pub rotated: Option<bool>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AppInfo {
    pub version: String,
    pub data_dir: String,
    pub local_dir: String,
    /// The database was written by a newer app; changes are refused.
    pub read_only: bool,
}

#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CacheUsageDto {
    pub bytes: f64,
    pub entries: u32,
    pub cap_bytes: f64,
}

/// Error shape every command returns: a stable `kind` for the UI to branch on, and a message
/// worded for people.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CmdError {
    pub kind: String,
    pub message: String,
}
