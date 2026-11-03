//! The Mihon-lineage backup protobuf, tag for tag with `sync/.../backup/BackupModels.kt`.
//!
//! The tags are the interoperability contract with Mihon, Yōkai, TachiyomiSY, J2K and the Kotlin
//! TachiMana: a `.tachibk` written here decodes there and the other way round. Do not renumber.
//!
//! Two encoding details matter for the Kotlin side, which decodes with kotlinx protobuf:
//! - kotlinx rejects a message that lacks a field with no default (`source`, `url`, `name`, ...).
//!   prost leaves zero values off the wire, so those fields are `Option` here and the
//!   constructors always set them. A local manga has `source = 0`, which prost would otherwise drop.
//! - kotlinx writes repeated numbers unpacked, so `categories` is declared unpacked too.
//!
//! Fields with a non-zero default in Kotlin (`favorite = true`, `memo = "{}"`) are `Option` with
//! that default, so a missing value reads as the default rather than as zero. Read optional
//! fields through prost's generated getters (`m.url()`, `m.favorite()`, ...).

use prost::Message;

#[derive(Clone, PartialEq, Message)]
pub struct Backup {
    #[prost(message, repeated, tag = "1")]
    pub backup_manga: Vec<BackupManga>,
    #[prost(message, repeated, tag = "2")]
    pub backup_categories: Vec<BackupCategory>,
    // Tag 100 (backupBrokenSources) is a legacy, non-compliant tag; deliberately unsupported.
    #[prost(message, repeated, tag = "101")]
    pub backup_sources: Vec<BackupSource>,
    /// App preferences. We have no mapping for Mihon's keys, so each entry is the raw encoded
    /// sub-message, written back unchanged.
    #[prost(bytes = "vec", repeated, tag = "104")]
    pub backup_preferences: Vec<Vec<u8>>,
    /// Per-source preferences, raw like [`Self::backup_preferences`].
    #[prost(bytes = "vec", repeated, tag = "105")]
    pub backup_source_preferences: Vec<Vec<u8>>,
    #[prost(message, repeated, tag = "106")]
    pub backup_extension_stores: Vec<BackupExtensionStore>,
    /// TachiMana / Mehdi's Mihon fork extension (not in stock Mihon, which skips it): the
    /// extensions installed on the device that wrote the backup, so a restore can reinstall them.
    /// Schema: `docs/rust-port/backup-schema.md`.
    #[prost(message, repeated, tag = "900")]
    pub backup_extensions: Vec<BackupExtension>,
}

#[derive(Clone, PartialEq, Message)]
pub struct BackupManga {
    #[prost(int64, optional, tag = "1")]
    pub source: Option<i64>,
    #[prost(string, optional, tag = "2")]
    pub url: Option<String>,
    #[prost(string, tag = "3")]
    pub title: String,
    #[prost(string, optional, tag = "4")]
    pub artist: Option<String>,
    #[prost(string, optional, tag = "5")]
    pub author: Option<String>,
    #[prost(string, optional, tag = "6")]
    pub description: Option<String>,
    #[prost(string, repeated, tag = "7")]
    pub genre: Vec<String>,
    #[prost(int32, tag = "8")]
    pub status: i32,
    #[prost(string, optional, tag = "9")]
    pub thumbnail_url: Option<String>,
    #[prost(int64, tag = "13")]
    pub date_added: i64,
    #[prost(int32, tag = "14")]
    pub viewer: i32,
    #[prost(message, repeated, tag = "16")]
    pub chapters: Vec<BackupChapter>,
    /// Category *order* values, not ids (the lineage contract).
    #[prost(int64, repeated, packed = "false", tag = "17")]
    pub categories: Vec<i64>,
    #[prost(message, repeated, tag = "18")]
    pub tracking: Vec<BackupTracking>,
    #[prost(bool, optional, tag = "100", default = "true")]
    pub favorite: Option<bool>,
    #[prost(int32, tag = "101")]
    pub chapter_flags: i32,
    #[prost(int32, optional, tag = "103")]
    pub viewer_flags: Option<i32>,
    #[prost(message, repeated, tag = "104")]
    pub history: Vec<BackupHistory>,
    #[prost(int32, tag = "105")]
    pub update_strategy: i32,
    #[prost(int64, tag = "106")]
    pub last_modified_at: i64,
    #[prost(int64, optional, tag = "107")]
    pub favorite_modified_at: Option<i64>,
    // Mihon-only fields.
    #[prost(string, repeated, tag = "108")]
    pub excluded_scanlators: Vec<String>,
    #[prost(int64, tag = "109")]
    pub version: i64,
    #[prost(string, tag = "110")]
    pub notes: String,
    #[prost(bool, tag = "111")]
    pub initialized: bool,
    /// Mihon declares this `bytes` holding a JSON object; `string` is the same wire type.
    #[prost(string, optional, tag = "112", default = "{}")]
    pub memo: Option<String>,
}

impl BackupManga {
    pub fn new(source: i64, url: impl Into<String>) -> Self {
        Self {
            source: Some(source),
            url: Some(url.into()),
            favorite: Some(true),
            ..Default::default()
        }
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct BackupChapter {
    #[prost(string, optional, tag = "1")]
    pub url: Option<String>,
    #[prost(string, optional, tag = "2")]
    pub name: Option<String>,
    #[prost(string, optional, tag = "3")]
    pub scanlator: Option<String>,
    #[prost(bool, tag = "4")]
    pub read: bool,
    #[prost(bool, tag = "5")]
    pub bookmark: bool,
    #[prost(int64, tag = "6")]
    pub last_page_read: i64,
    #[prost(int64, tag = "7")]
    pub date_fetch: i64,
    #[prost(int64, tag = "8")]
    pub date_upload: i64,
    #[prost(float, tag = "9")]
    pub chapter_number: f32,
    #[prost(int64, tag = "10")]
    pub source_order: i64,
    #[prost(int64, tag = "11")]
    pub last_modified_at: i64,
    #[prost(int64, tag = "12")]
    pub version: i64,
    #[prost(string, optional, tag = "13", default = "{}")]
    pub memo: Option<String>,
    /// Yōkai/J2K extension, not in Mihon. Kept so Yōkai backups round-trip.
    #[prost(int32, tag = "800")]
    pub pages_left: i32,
    /// TachiMana / Mihon-fork extension: the chapter was downloaded on the device that wrote the
    /// backup. Never means "the files are in this backup".
    #[prost(bool, optional, tag = "900")]
    pub downloaded: Option<bool>,
}

impl BackupChapter {
    pub fn new(url: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            url: Some(url.into()),
            name: Some(name.into()),
            ..Default::default()
        }
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct BackupCategory {
    #[prost(string, optional, tag = "1")]
    pub name: Option<String>,
    #[prost(int64, tag = "2")]
    pub order: i64,
    #[prost(int64, tag = "3")]
    pub id: i64,
    #[prost(int64, tag = "100")]
    pub flags: i64,
}

#[derive(Clone, PartialEq, Message)]
pub struct BackupTracking {
    #[prost(int32, optional, tag = "1")]
    pub sync_id: Option<i32>,
    #[prost(int64, optional, tag = "2")]
    pub library_id: Option<i64>,
    /// Superseded by `media_id`; still written by older clients.
    #[prost(int32, tag = "3")]
    pub media_id_int: i32,
    #[prost(string, tag = "4")]
    pub tracking_url: String,
    #[prost(string, tag = "5")]
    pub title: String,
    #[prost(float, tag = "6")]
    pub last_chapter_read: f32,
    #[prost(int32, tag = "7")]
    pub total_chapters: i32,
    #[prost(float, tag = "8")]
    pub score: f32,
    #[prost(int32, tag = "9")]
    pub status: i32,
    #[prost(int64, tag = "10")]
    pub started_reading_date: i64,
    #[prost(int64, tag = "11")]
    pub finished_reading_date: i64,
    /// Mihon: don't post this entry's progress to the tracker's public feed.
    #[prost(bool, tag = "12")]
    pub private: bool,
    #[prost(int64, tag = "100")]
    pub media_id: i64,
}

#[derive(Clone, PartialEq, Message)]
pub struct BackupHistory {
    #[prost(string, optional, tag = "1")]
    pub url: Option<String>,
    #[prost(int64, optional, tag = "2")]
    pub last_read: Option<i64>,
    #[prost(int64, tag = "3")]
    pub read_duration: i64,
}

#[derive(Clone, PartialEq, Message)]
pub struct BackupSource {
    #[prost(string, tag = "1")]
    pub name: String,
    #[prost(int64, optional, tag = "2")]
    pub source_id: Option<i64>,
}

/// An extension *store* the user had added (tag 106). Field 1 is Mihon's index URL
/// (`.../index.pb`). Restoring these is always opt-in.
#[derive(Clone, PartialEq, Eq, Hash, Message, serde::Serialize)]
pub struct BackupExtensionStore {
    #[prost(string, optional, tag = "1")]
    pub index_url: Option<String>,
    #[prost(string, optional, tag = "2")]
    pub name: Option<String>,
    #[prost(string, optional, tag = "3")]
    pub badge_label: Option<String>,
    #[prost(string, tag = "4")]
    pub contact_website: String,
    #[prost(string, tag = "5")]
    pub signing_key: String,
    #[prost(string, optional, tag = "6")]
    pub contact_discord: Option<String>,
    #[prost(bool, optional, tag = "7")]
    pub is_legacy: Option<bool>,
    #[prost(string, optional, tag = "8")]
    pub extension_list_url: Option<String>,
}

/// An installed extension (tag 900 on [`Backup`]). Field 6 has the same form as
/// [`BackupExtensionStore::index_url`]; it is empty for a sideloaded extension.
#[derive(Clone, PartialEq, Eq, Hash, Message, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupExtension {
    #[prost(string, tag = "1")]
    pub pkg_name: String,
    #[prost(string, optional, tag = "2")]
    pub name: Option<String>,
    #[prost(int64, optional, tag = "3")]
    pub version_code: Option<i64>,
    #[prost(string, optional, tag = "4")]
    pub version_name: Option<String>,
    #[prost(string, optional, tag = "5")]
    pub lang: Option<String>,
    #[prost(string, optional, tag = "6")]
    pub repo_index_url: Option<String>,
    #[prost(int64, repeated, packed = "false", tag = "7")]
    pub source_ids: Vec<i64>,
    #[prost(string, optional, tag = "8")]
    pub signature_hash: Option<String>,
}
