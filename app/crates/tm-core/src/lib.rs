//! Shared foundations for the TachiMana Rust app: data paths, `settings.json`, the single-instance
//! lock, and the error type those use.
//!
//! Everything here must stay compatible with the Kotlin app (`desktop/.../AppPaths.kt`,
//! `desktop/.../data/AppSettings.kt`), because both apps share one data folder.

pub mod lock;
pub mod paths;
pub mod settings;

pub use lock::AppLock;
pub use paths::AppPaths;
pub use settings::Settings;

/// Errors from the core crate.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("settings file is not valid JSON: {0}")]
    SettingsJson(#[from] serde_json::Error),
    #[error("could not find a data folder (no LOCALAPPDATA / HOME)")]
    NoDataDir,
    #[error("another TachiMana instance is already using {0}")]
    Locked(std::path::PathBuf),
}

pub type Result<T> = std::result::Result<T, CoreError>;

pub(crate) fn io_err(path: &std::path::Path) -> impl FnOnce(std::io::Error) -> CoreError + '_ {
    move |source| CoreError::Io {
        path: path.to_path_buf(),
        source,
    }
}
