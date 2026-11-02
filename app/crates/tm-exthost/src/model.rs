//! Wire types of the ext-host API (docs/rust-port/api.md). Field names follow the JSON exactly.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ErrorBody {
    pub error: String,
    pub kind: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Health {
    pub version: String,
    pub extensions: u32,
    pub sources: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceRef {
    pub id: i64,
    pub name: String,
    pub lang: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Extension {
    pub pkg_name: String,
    pub name: String,
    pub version_name: String,
    pub version_code: i64,
    pub lang: String,
    pub nsfw: bool,
    #[serde(default)]
    pub icon_url: Option<String>,
    pub sources: Vec<SourceRef>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AvailableSource {
    pub id: i64,
    pub name: String,
    pub lang: String,
    pub base_url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AvailableExtension {
    pub name: String,
    pub pkg_name: String,
    pub version_name: String,
    pub version_code: i64,
    pub lib_version: String,
    pub lang: String,
    pub nsfw: bool,
    pub icon_url: String,
    pub has_jar: bool,
    pub sources: Vec<AvailableSource>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Repo {
    pub index_url: String,
    pub name: String,
    #[serde(default)]
    pub short_name: Option<String>,
    pub website: String,
    pub signing_key_fingerprint: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub id: i64,
    pub name: String,
    pub lang: String,
    pub extension_pkg: String,
    pub supports_latest: bool,
    pub is_configurable: bool,
}

/// `SManga` as the extension API names it.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
pub struct Manga {
    pub url: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    #[serde(default)]
    pub status: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
    #[serde(default = "always_update")]
    pub update_strategy: String,
    #[serde(default)]
    pub initialized: bool,
    #[serde(default)]
    pub memo: Map<String, Value>,
}

fn always_update() -> String {
    "ALWAYS_UPDATE".into()
}

/// `SChapter` as the extension API names it.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
pub struct Chapter {
    pub url: String,
    pub name: String,
    #[serde(default)]
    pub date_upload: i64,
    #[serde(default = "minus_one")]
    pub chapter_number: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scanlator: Option<String>,
    #[serde(default)]
    pub memo: Map<String, Value>,
}

fn minus_one() -> f32 {
    -1.0
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MangasPage {
    pub mangas: Vec<Manga>,
    pub has_next_page: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub index: i32,
    #[serde(default)]
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct MangaUpdate {
    pub manga: Manga,
    pub chapters: Vec<Chapter>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Preference {
    pub key: String,
    /// `toggle`, `list`, `multiselect`, `text` or `plain`.
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub summary: Option<String>,
    pub enabled: bool,
    #[serde(default)]
    pub value: Option<Value>,
    #[serde(default)]
    pub entries: Option<Vec<String>>,
    #[serde(default)]
    pub entry_values: Option<Vec<String>>,
    #[serde(default)]
    pub accepted: Option<bool>,
}

/// Image bytes and the content type the host reported.
#[derive(Debug, Clone)]
pub struct Image {
    pub bytes: Vec<u8>,
    pub content_type: String,
}
