//! What the app holds while it runs: database, settings, page cache and the paths they live in.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

use tm_cache::DiskCache;
use tm_core::{settings::keys, AppLock, AppPaths, Settings};
use tm_db::Db;

use crate::dto::CmdError;

/// Pages of an opened chapter: the archive or folder and its page names in reading order.
#[derive(Debug, Clone)]
pub struct OpenChapter {
    pub path: PathBuf,
    pub names: Vec<String>,
}

/// Pages of an online chapter as its source listed them.
#[derive(Debug, Clone)]
pub struct OnlineChapter {
    pub source: i64,
    pub chapter_url: String,
    pub pages: Vec<tm_exthost::model::Page>,
}

pub struct AppState {
    pub paths: AppPaths,
    pub db: Db,
    pub settings: Mutex<Settings>,
    pub cache: DiskCache,
    /// Chapters the reader opened, so `tm://page/<chapter>/<i>` does not re-list the archive.
    pub chapters: Mutex<HashMap<i64, OpenChapter>>,
    /// Online chapters the reader opened, keyed by chapter id.
    pub online: Mutex<HashMap<i64, OnlineChapter>>,
    /// The extension host, started on first use.
    pub exthost: Mutex<Option<tm_exthost::ExtHost>>,
    /// Folder holding `ext-host.jar`, set once the Tauri app knows its resource dir.
    pub host_dir: OnceLock<PathBuf>,
    _lock: AppLock,
}

const MB: u64 = 1024 * 1024;

impl AppState {
    pub fn open(paths: AppPaths) -> Result<Self, String> {
        std::fs::create_dir_all(paths.data_dir()).map_err(|e| e.to_string())?;
        let lock = AppLock::acquire(&paths.lock_file()).map_err(|e| e.to_string())?;
        let db = Db::open(&paths.database_file()).map_err(|e| e.to_string())?;
        let settings = Settings::load(&paths.settings_file());
        let cap_mb = u64::try_from(settings.i64(keys::DISK_CACHE_MB).max(64)).unwrap_or(10240);
        let cache = DiskCache::open(&paths.page_cache_dir(), cap_mb.saturating_mul(MB))
            .map_err(|e| e.to_string())?;
        Ok(Self {
            paths,
            db,
            settings: Mutex::new(settings),
            cache,
            chapters: Mutex::new(HashMap::new()),
            online: Mutex::new(HashMap::new()),
            exthost: Mutex::new(None),
            host_dir: OnceLock::new(),
            _lock: lock,
        })
    }

    pub fn settings(&self) -> Result<MutexGuard<'_, Settings>, CmdError> {
        self.settings
            .lock()
            .map_err(|_| CmdError::internal("settings lock poisoned"))
    }

    pub fn online(&self) -> Result<MutexGuard<'_, HashMap<i64, OnlineChapter>>, CmdError> {
        self.online
            .lock()
            .map_err(|_| CmdError::internal("online chapter table lock poisoned"))
    }

    pub fn chapters(&self) -> Result<MutexGuard<'_, HashMap<i64, OpenChapter>>, CmdError> {
        self.chapters
            .lock()
            .map_err(|_| CmdError::internal("chapter table lock poisoned"))
    }
}

impl CmdError {
    pub fn new(kind: &str, message: impl Into<String>) -> Self {
        Self {
            kind: kind.to_owned(),
            message: message.into(),
        }
    }
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new("internal", message)
    }
    pub fn not_found(what: &str) -> Self {
        Self::new(
            "notFound",
            format!("{what} was not found. It may have been removed."),
        )
    }
}

impl From<tm_db::DbError> for CmdError {
    fn from(e: tm_db::DbError) -> Self {
        match e {
            tm_db::DbError::ReadOnly => Self::new(
                "readOnly",
                "This library was saved by a newer TachiMana, so changes are switched off. Update the app to edit it.",
            ),
            tm_db::DbError::NotFound(_) => Self::not_found("That item"),
            other => Self::new("database", other.to_string()),
        }
    }
}

impl From<tm_local::LocalError> for CmdError {
    fn from(e: tm_local::LocalError) -> Self {
        Self::new("local", e.to_string())
    }
}

impl From<tm_archive::ArchiveError> for CmdError {
    fn from(e: tm_archive::ArchiveError) -> Self {
        Self::new("archive", e.to_string())
    }
}

impl From<tm_cache::CacheError> for CmdError {
    fn from(e: tm_cache::CacheError) -> Self {
        Self::new("cache", e.to_string())
    }
}

impl From<tm_core::CoreError> for CmdError {
    fn from(e: tm_core::CoreError) -> Self {
        Self::new("io", e.to_string())
    }
}

impl From<tm_exthost::Error> for CmdError {
    fn from(e: tm_exthost::Error) -> Self {
        let message = match &e {
            tm_exthost::Error::NotInstalled(_) => {
                "The extension host is missing from this install, so online sources are unavailable."
                    .to_owned()
            }
            other => other.to_string(),
        };
        Self::new(e.kind(), message)
    }
}
