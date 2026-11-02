//! `settings.json`, shared with the Kotlin app (`AppSettingsData` in
//! `desktop/src/main/kotlin/tachimana/data/AppSettings.kt`).
//!
//! The file is kept as a JSON object map so keys this version does not know (written by the
//! Kotlin app or a newer Rust app) survive a write. Typed accessors apply the Kotlin defaults.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::{io_err, Result};

/// Defaults copied from `AppSettingsData`, plus Rust-only keys (marked).
pub mod keys {
    pub const CLOUDFLARE_BYPASS_ENABLED: &str = "cloudflareBypassEnabled";
    pub const READER_FIT: &str = "readerFit";
    pub const READER_ZOOM: &str = "readerZoom";
    pub const READER_COVER_SOLO: &str = "readerCoverSolo";
    pub const LIBRARY_GRID: &str = "libraryGrid";
    pub const LIBRARY_SORT: &str = "librarySort";
    pub const LIBRARY_SORT_DESCENDING: &str = "librarySortDescending";
    pub const LIBRARY_UNREAD_ONLY: &str = "libraryUnreadOnly";
    pub const LIBRARY_DOWNLOADED_ONLY: &str = "libraryDownloadedOnly";
    pub const REMOTE_ENABLED: &str = "remoteStorageEnabled";
    pub const REMOTE_HOST: &str = "remoteStorageHost";
    pub const REMOTE_PORT: &str = "remoteStoragePort";
    pub const REMOTE_USE_HTTPS: &str = "remoteStorageUseHttps";
    pub const REMOTE_FOLDER: &str = "remoteStorageFolder";
    pub const REMOTE_USERNAME: &str = "remoteStorageUsername";
    pub const REMOTE_PASSWORD: &str = "remoteStoragePassword";
    pub const REMOTE_CACHE_MB: &str = "remoteStorageCacheMb";
    pub const REMOTE_REDOWNLOAD: &str = "remoteStorageRedownloadFromRemote";
    /// Rust-only: page disk cache cap in MB (default 10 GB).
    pub const DISK_CACHE_MB: &str = "diskCacheMb";
    /// Rust-only: UI theme name.
    pub const THEME: &str = "theme";
    /// Rust-only: where the extension host zip is downloaded from.
    pub const EXT_HOST_BASE_URL: &str = "extHostBaseUrl";
    /// Rust-only: NAS role, "mirror" (keep local copy) or "archive" (evict after verified upload).
    pub const REMOTE_ROLE: &str = "remoteStorageRole";
}

/// The settings document plus where it lives.
#[derive(Debug, Clone)]
pub struct Settings {
    path: PathBuf,
    map: Map<String, Value>,
}

impl Settings {
    /// Load `path`. A missing or corrupt file yields defaults (like the Kotlin app); a corrupt
    /// file is kept aside as `settings.json.corrupt` so nothing is silently lost.
    pub fn load(path: &Path) -> Self {
        let map = match std::fs::read(path) {
            Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
                Ok(Value::Object(m)) => m,
                _ => {
                    let _ = std::fs::copy(path, path.with_extension("json.corrupt"));
                    Map::new()
                }
            },
            Err(_) => Map::new(),
        };
        Self {
            path: path.to_path_buf(),
            map,
        }
    }

    /// Write atomically (temp file, then rename).
    pub fn save(&self) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(io_err(dir))?;
        }
        let tmp = self.path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(&Value::Object(self.map.clone()))?;
        std::fs::write(&tmp, text).map_err(io_err(&tmp))?;
        std::fs::rename(&tmp, &self.path).map_err(io_err(&self.path))?;
        Ok(())
    }

    pub fn raw(&self) -> &Map<String, Value> {
        &self.map
    }

    pub fn bool(&self, key: &str) -> bool {
        self.map
            .get(key)
            .and_then(Value::as_bool)
            .unwrap_or_else(|| default_bool(key))
    }

    pub fn string(&self, key: &str) -> String {
        self.map
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| default_string(key).to_owned())
    }

    pub fn f64(&self, key: &str) -> f64 {
        self.map
            .get(key)
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite())
            .unwrap_or_else(|| default_f64(key))
    }

    pub fn i64(&self, key: &str) -> i64 {
        self.map
            .get(key)
            .and_then(Value::as_i64)
            .unwrap_or_else(|| default_i64(key))
    }

    pub fn set(&mut self, key: &str, value: impl Into<Value>) {
        self.map.insert(key.to_owned(), value.into());
    }

    /// `remoteServerUrl()` from the Kotlin app.
    pub fn remote_server_url(&self) -> String {
        let host = self.string(keys::REMOTE_HOST);
        let host = host.trim().trim_end_matches('/');
        let host = host.split_once("://").map_or(host, |(_, h)| h);
        if host.is_empty() {
            return String::new();
        }
        let scheme = if self.bool(keys::REMOTE_USE_HTTPS) {
            "https"
        } else {
            "http"
        };
        let port = self
            .string(keys::REMOTE_PORT)
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|p| (1..=65535).contains(p));
        match port {
            Some(p) => format!("{scheme}://{host}:{p}"),
            None => format!("{scheme}://{host}"),
        }
    }

    /// `remoteCollectionUrl()` from the Kotlin app.
    pub fn remote_collection_url(&self) -> String {
        let server = self.remote_server_url();
        if server.is_empty() {
            return server;
        }
        let folder = self.string(keys::REMOTE_FOLDER);
        let folder = folder.trim().trim_matches('/');
        if folder.is_empty() {
            server
        } else {
            format!("{server}/{folder}")
        }
    }

    /// `remoteStorageEnabled` getter from the Kotlin app: on and a host is set.
    pub fn remote_enabled(&self) -> bool {
        self.bool(keys::REMOTE_ENABLED) && !self.string(keys::REMOTE_HOST).trim().is_empty()
    }
}

fn default_bool(key: &str) -> bool {
    matches!(
        key,
        keys::READER_COVER_SOLO | keys::REMOTE_USE_HTTPS | keys::REMOTE_REDOWNLOAD
    )
}

fn default_string(key: &str) -> &'static str {
    match key {
        keys::READER_FIT => "FIT_SCREEN",
        keys::LIBRARY_SORT => "TITLE",
        keys::THEME => "YOKAI_DARK",
        keys::REMOTE_ROLE => "mirror",
        _ => "",
    }
}

fn default_f64(key: &str) -> f64 {
    match key {
        keys::READER_ZOOM => 1.0,
        _ => 0.0,
    }
}

fn default_i64(key: &str) -> i64 {
    match key {
        keys::REMOTE_CACHE_MB => 2048,
        keys::DISK_CACHE_MB => 10240,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_kotlin() {
        let dir = tempfile::tempdir().unwrap();
        let s = Settings::load(&dir.path().join("settings.json"));
        assert_eq!(s.string(keys::READER_FIT), "FIT_SCREEN");
        assert!((s.f64(keys::READER_ZOOM) - 1.0).abs() < f64::EPSILON);
        assert!(s.bool(keys::READER_COVER_SOLO));
        assert!(!s.bool(keys::CLOUDFLARE_BYPASS_ENABLED));
        assert!(s.bool(keys::REMOTE_USE_HTTPS));
        assert_eq!(s.i64(keys::REMOTE_CACHE_MB), 2048);
        assert_eq!(s.i64(keys::DISK_CACHE_MB), 10240);
    }

    #[test]
    fn unknown_keys_survive_a_write() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("settings.json");
        std::fs::write(&p, r#"{"futureKey": {"a": 1}, "readerZoom": 1.5}"#).unwrap();
        let mut s = Settings::load(&p);
        assert!((s.f64(keys::READER_ZOOM) - 1.5).abs() < f64::EPSILON);
        s.set(keys::LIBRARY_GRID, true);
        s.save().unwrap();
        let back: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
        assert_eq!(back["futureKey"]["a"], 1);
        assert_eq!(back["libraryGrid"], true);
    }

    #[test]
    fn corrupt_file_is_kept_aside() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("settings.json");
        std::fs::write(&p, "{not json").unwrap();
        let s = Settings::load(&p);
        assert_eq!(s.string(keys::LIBRARY_SORT), "TITLE");
        assert!(dir.path().join("settings.json.corrupt").exists());
    }

    #[test]
    fn remote_urls_match_kotlin() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Settings::load(&dir.path().join("s.json"));
        assert_eq!(s.remote_server_url(), "");
        s.set(keys::REMOTE_HOST, "http://nas.local/");
        s.set(keys::REMOTE_PORT, "5006");
        s.set(keys::REMOTE_FOLDER, "/manga/");
        assert_eq!(s.remote_collection_url(), "https://nas.local:5006/manga");
        s.set(keys::REMOTE_PORT, "99999");
        s.set(keys::REMOTE_USE_HTTPS, false);
        assert_eq!(s.remote_server_url(), "http://nas.local");
        assert!(!s.remote_enabled());
        s.set(keys::REMOTE_ENABLED, true);
        assert!(s.remote_enabled());
    }
}
