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
    /// Source ids are 64-bit and exceed 2^53, so they travel as strings.
    #[serde(serialize_with = "id_str::serialize")]
    #[ts(type = "string")]
    pub source: i64,
    pub url: String,
    pub title: String,
    pub favorite: bool,
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
            favorite: m.favorite,
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
    /// The source's chapter url (a relative path for local chapters).
    pub url: String,
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
    /// Complete on disk (downloaded); always false for local chapters, which are files already.
    pub downloaded: bool,
}

impl From<&tm_db::Chapter> for ChapterDto {
    fn from(c: &tm_db::Chapter) -> Self {
        Self {
            id: c.id,
            manga_id: c.manga_id,
            url: c.url.clone(),
            name: c.name.clone(),
            scanlator: c.scanlator.clone(),
            read: c.read,
            bookmark: c.bookmark,
            last_page_read: c.last_page_read,
            pages_left: c.pages_left,
            chapter_number: c.chapter_number,
            source_order: c.source_order,
            date_upload: c.date_upload,
            downloaded: false,
        }
    }
}

/// One row of the download queue, sent as the `downloads` event.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DownloadJobDto {
    #[ts(type = "number")]
    pub id: u64,
    /// Source ids are 64-bit and exceed 2^53, so they travel as strings.
    pub source_id: String,
    pub manga_url: String,
    pub manga_title: String,
    pub chapter_url: String,
    pub chapter_name: String,
    /// `QUEUED`, `DOWNLOADING`, `DONE` or `FAILED`.
    pub status: String,
    pub downloaded: u32,
    pub total: u32,
    pub error: Option<String>,
}

impl From<&tm_download::DownloadJob> for DownloadJobDto {
    fn from(j: &tm_download::DownloadJob) -> Self {
        let status = match j.status {
            tm_download::DownloadStatus::Queued => "QUEUED",
            tm_download::DownloadStatus::Downloading => "DOWNLOADING",
            tm_download::DownloadStatus::Done => "DONE",
            tm_download::DownloadStatus::Failed => "FAILED",
        };
        Self {
            id: j.id,
            source_id: j.chapter.source_id.to_string(),
            manga_url: j.chapter.manga_url.clone(),
            manga_title: j.chapter.manga_title.clone(),
            chapter_url: j.chapter.chapter_url.clone(),
            chapter_name: j.chapter.chapter_name.clone(),
            status: status.into(),
            downloaded: j.downloaded,
            total: j.total,
            error: j.error.clone(),
        }
    }
}

/// A downloaded chapter found on disk (the Downloads screen's "On this computer" list).
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DownloadedChapterDto {
    /// Library rows, when the manga and chapter are in the database.
    #[ts(type = "number | null")]
    pub manga_id: Option<i64>,
    #[ts(type = "number | null")]
    pub chapter_id: Option<i64>,
    pub manga_title: String,
    pub chapter_name: String,
    pub page_count: u32,
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
    pub cloudflare_bypass_enabled: bool,
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
    #[ts(optional)]
    pub cloudflare_bypass_enabled: Option<bool>,
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

/// Serialises an i64 id as a JSON string (commands take ids back as strings and parse them).
pub mod id_str {
    use serde::Serializer;

    pub fn serialize<S: Serializer>(v: &i64, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&v.to_string())
    }
}

// ---------------------------------------------------------------------------------------------
// Online sources (P5). Source ids are strings on this side of the IPC boundary (see `id_str`).

/// Whether the extension host can be used, and why not.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BrowseStatusDto {
    pub available: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RepoDto {
    pub base_url: String,
    pub name: String,
    pub short_name: Option<String>,
    pub website: String,
    pub signing_key_fingerprint: String,
}

impl From<&tm_db::ExtensionRepo> for RepoDto {
    fn from(r: &tm_db::ExtensionRepo) -> Self {
        Self {
            base_url: r.base_url.clone(),
            name: r.name.clone(),
            short_name: r.short_name.clone(),
            website: r.website.clone(),
            signing_key_fingerprint: r.signing_key_fingerprint.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SourceRefDto {
    #[serde(serialize_with = "id_str::serialize")]
    #[ts(type = "string")]
    pub id: i64,
    pub name: String,
    pub lang: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct InstalledExtensionDto {
    pub pkg_name: String,
    pub name: String,
    pub version_name: String,
    pub lang: String,
    pub nsfw: bool,
    pub icon_url: Option<String>,
    pub sources: Vec<SourceRefDto>,
}

impl From<&tm_exthost::model::Extension> for InstalledExtensionDto {
    fn from(e: &tm_exthost::model::Extension) -> Self {
        Self {
            pkg_name: e.pkg_name.clone(),
            name: e.name.clone(),
            version_name: e.version_name.clone(),
            lang: e.lang.clone(),
            nsfw: e.nsfw,
            icon_url: e.icon_url.clone(),
            sources: e
                .sources
                .iter()
                .map(|s| SourceRefDto {
                    id: s.id,
                    name: s.name.clone(),
                    lang: s.lang.clone(),
                })
                .collect(),
        }
    }
}

/// One extension a repo offers, with its install state.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CatalogEntryDto {
    pub repo_url: String,
    pub repo_name: String,
    pub pkg_name: String,
    pub name: String,
    pub version_name: String,
    pub lang: String,
    pub nsfw: bool,
    pub icon_url: String,
    /// Installed version, if any.
    pub installed_version: Option<String>,
    pub update_available: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RepoFailureDto {
    pub repo_name: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CatalogDto {
    pub entries: Vec<CatalogEntryDto>,
    pub failures: Vec<RepoFailureDto>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SourceDto {
    #[serde(serialize_with = "id_str::serialize")]
    #[ts(type = "string")]
    pub id: i64,
    pub name: String,
    pub lang: String,
    pub extension_pkg: String,
    pub extension_name: String,
    pub icon_url: Option<String>,
    pub supports_latest: bool,
    pub is_configurable: bool,
}

/// A manga as a source lists it while browsing.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OnlineMangaDto {
    pub url: String,
    pub title: String,
    pub thumbnail_url: Option<String>,
    /// The library row, when this manga is a favourite.
    #[ts(type = "number | null")]
    pub library_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BrowsePageDto {
    pub mangas: Vec<OnlineMangaDto>,
    pub has_next_page: bool,
}

/// A source preference (api.md "Preference").
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PreferenceDto {
    pub key: String,
    pub kind: String,
    pub title: String,
    pub summary: Option<String>,
    pub enabled: bool,
    #[ts(type = "boolean | string | Array<string> | null")]
    pub value: serde_json::Value,
    pub entries: Vec<String>,
    pub entry_values: Vec<String>,
    pub accepted: Option<bool>,
}

impl From<tm_exthost::model::Preference> for PreferenceDto {
    fn from(p: tm_exthost::model::Preference) -> Self {
        Self {
            key: p.key,
            kind: p.kind,
            title: p.title,
            summary: p.summary,
            enabled: p.enabled,
            value: p.value.unwrap_or(serde_json::Value::Null),
            entries: p.entries.unwrap_or_default(),
            entry_values: p.entry_values.unwrap_or_default(),
            accepted: p.accepted,
        }
    }
}

/// Progress of the library update, sent as the `library-update` event.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LibraryUpdateDto {
    pub running: bool,
    pub done: u32,
    pub total: u32,
    /// Title being refreshed now.
    pub current: Option<String>,
    pub chapters_added: u32,
    /// Titles whose source failed, with the error.
    pub failures: Vec<String>,
}

/// Result of refreshing an online manga from its source.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RefreshDto {
    pub detail: MangaDetailDto,
    pub chapters_added: u32,
    /// Set when the source failed; the detail is then what was stored before.
    pub error: Option<String>,
}

/// One row of reading history (`recentsFlow`): the manga, the chapter last read and when.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RecentDto {
    pub manga: MangaDto,
    pub chapter: ChapterDto,
    /// Epoch milliseconds.
    #[ts(type = "number")]
    pub last_read: i64,
}
