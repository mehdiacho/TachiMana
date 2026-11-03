//! Row types. Field names follow the database columns.

use rusqlite::Row;
use serde::{Deserialize, Serialize};

/// Source id of the local library (`local/<manga>/<chapter>`).
pub const LOCAL_SOURCE: i64 = 0;

/// `manga_sync.sync_id` of AniList.
pub const ANILIST_SYNC_ID: i64 = 2;

/// The lineage `viewer` flag values for reading modes.
pub mod viewer {
    pub const DEFAULT: i64 = 0;
    pub const LEFT_TO_RIGHT: i64 = 1;
    pub const RIGHT_TO_LEFT: i64 = 2;
    pub const STRIP: i64 = 5;
    pub const DOUBLE: i64 = 6;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manga {
    pub id: i64,
    pub source: i64,
    pub url: String,
    pub artist: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub genre: Option<String>,
    pub title: String,
    pub status: i64,
    pub thumbnail_url: Option<String>,
    pub favorite: bool,
    pub last_update: Option<i64>,
    pub initialized: bool,
    pub viewer: i64,
    pub hide_title: bool,
    pub chapter_flags: i64,
    pub date_added: Option<i64>,
    pub filtered_scanlators: Option<String>,
    pub update_strategy: i64,
    pub cover_last_modified: i64,
    pub notes: String,
    pub version: i64,
    pub last_modified_at: i64,
    pub favorite_modified_at: Option<i64>,
    pub memo: String,
}

impl Manga {
    pub fn from_row(r: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: r.get("_id")?,
            source: r.get("source")?,
            url: r.get("url")?,
            artist: r.get("artist")?,
            author: r.get("author")?,
            description: r.get("description")?,
            genre: r.get("genre")?,
            title: r.get("title")?,
            status: r.get("status")?,
            thumbnail_url: r.get("thumbnail_url")?,
            favorite: r.get("favorite")?,
            last_update: r.get("last_update")?,
            initialized: r.get("initialized")?,
            viewer: r.get("viewer")?,
            hide_title: r.get("hide_title")?,
            chapter_flags: r.get("chapter_flags")?,
            date_added: r.get("date_added")?,
            filtered_scanlators: r.get("filtered_scanlators")?,
            update_strategy: r.get("update_strategy")?,
            cover_last_modified: r.get("cover_last_modified")?,
            notes: r.get("notes")?,
            version: r.get("version")?,
            last_modified_at: r.get("last_modified_at")?,
            favorite_modified_at: r.get("favorite_modified_at")?,
            memo: r.get("memo")?,
        })
    }

    pub fn is_local(&self) -> bool {
        self.source == LOCAL_SOURCE
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chapter {
    pub id: i64,
    pub manga_id: i64,
    pub url: String,
    pub name: String,
    pub scanlator: Option<String>,
    pub read: bool,
    pub bookmark: bool,
    pub last_page_read: i64,
    pub pages_left: i64,
    pub chapter_number: f64,
    pub source_order: i64,
    pub date_fetch: i64,
    pub date_upload: i64,
    pub last_modified_at: i64,
    pub version: i64,
    pub memo: String,
}

impl Chapter {
    pub fn from_row(r: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: r.get("_id")?,
            manga_id: r.get("manga_id")?,
            url: r.get("url")?,
            name: r.get("name")?,
            scanlator: r.get("scanlator")?,
            read: r.get("read")?,
            bookmark: r.get("bookmark")?,
            last_page_read: r.get("last_page_read")?,
            pages_left: r.get("pages_left")?,
            chapter_number: r.get("chapter_number")?,
            source_order: r.get("source_order")?,
            date_fetch: r.get("date_fetch")?,
            date_upload: r.get("date_upload")?,
            last_modified_at: r.get("last_modified_at")?,
            version: r.get("version")?,
            memo: r.get("memo")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Category {
    pub id: i64,
    pub name: String,
    pub sort: i64,
    pub flags: i64,
    pub manga_order: String,
}

impl Category {
    pub fn from_row(r: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: r.get("_id")?,
            name: r.get("name")?,
            sort: r.get("sort")?,
            flags: r.get("flags")?,
            manga_order: r.get("manga_order")?,
        })
    }
}

/// One Recents entry: a chapter read, its manga, and when.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecentEntry {
    pub manga: Manga,
    pub chapter: Chapter,
    pub last_read: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: i64,
    pub manga_id: i64,
    pub sync_id: i64,
    pub remote_id: i64,
    pub library_id: Option<i64>,
    pub title: String,
    pub last_chapter_read: f64,
    pub total_chapters: i64,
    pub status: i64,
    pub score: f64,
    pub remote_url: String,
    pub start_date: i64,
    pub finish_date: i64,
}

impl Track {
    pub fn from_row(r: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: r.get("_id")?,
            manga_id: r.get("manga_id")?,
            sync_id: r.get("sync_id")?,
            remote_id: r.get("remote_id")?,
            library_id: r.get("library_id")?,
            title: r.get("title")?,
            last_chapter_read: r.get("last_chapter_read")?,
            total_chapters: r.get("total_chapters")?,
            status: r.get("status")?,
            score: r.get("score")?,
            remote_url: r.get("remote_url")?,
            start_date: r.get("start_date")?,
            finish_date: r.get("finish_date")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionRepo {
    pub base_url: String,
    pub name: String,
    pub short_name: Option<String>,
    pub website: String,
    pub signing_key_fingerprint: String,
}

impl ExtensionRepo {
    pub fn from_row(r: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            base_url: r.get("base_url")?,
            name: r.get("name")?,
            short_name: r.get("short_name")?,
            website: r.get("website")?,
            signing_key_fingerprint: r.get("signing_key_fingerprint")?,
        })
    }
}

/// Fields a source returns for a manga (`SManga`), used when adding to the library.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SourceManga {
    pub url: String,
    pub title: String,
    pub artist: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub genre: Option<String>,
    pub status: i64,
    pub thumbnail_url: Option<String>,
    pub initialized: bool,
}

/// Fields a source returns for a chapter (`SChapter`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SourceChapter {
    pub url: String,
    pub name: String,
    pub date_upload: i64,
    pub chapter_number: f64,
    pub scanlator: Option<String>,
}
