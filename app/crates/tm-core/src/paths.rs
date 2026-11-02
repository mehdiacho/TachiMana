//! Data folder layout. Mirrors `desktop/src/main/kotlin/tachimana/AppPaths.kt` exactly.

use std::path::{Path, PathBuf};

use crate::{io_err, CoreError, Result};

/// Resolved data folder and every sub-path the app uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    data_dir: PathBuf,
}

impl AppPaths {
    /// Resolve the data folder from the environment, using the same rules as the Kotlin app:
    /// `TACHIMANA_DATA_DIR` override; Windows `%LOCALAPPDATA%\TachiMana`; elsewhere
    /// `$XDG_DATA_HOME/TachiMana` or `~/.local/share/TachiMana`; last resort
    /// `~/.tachimana-base/TachiMana`.
    pub fn from_env() -> Result<Self> {
        let get = |k: &str| {
            std::env::var_os(k)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let dir = resolve(
            cfg!(windows),
            get("TACHIMANA_DATA_DIR"),
            get("LOCALAPPDATA"),
            get("XDG_DATA_HOME"),
            get("HOME").or_else(|| get("USERPROFILE")),
        )
        .ok_or(CoreError::NoDataDir)?;
        Self::at(dir)
    }

    /// Use an explicit data folder (created if missing).
    pub fn at(data_dir: impl Into<PathBuf>) -> Result<Self> {
        let data_dir = data_dir.into();
        std::fs::create_dir_all(&data_dir).map_err(io_err(&data_dir))?;
        Ok(Self { data_dir })
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }
    pub fn database_file(&self) -> PathBuf {
        self.data_dir.join("tachimana.db")
    }
    pub fn identity_file(&self) -> PathBuf {
        self.data_dir.join("device.json")
    }
    pub fn settings_file(&self) -> PathBuf {
        self.data_dir.join("settings.json")
    }
    pub fn lock_file(&self) -> PathBuf {
        self.data_dir.join("app.lock")
    }
    pub fn local_library_dir(&self) -> PathBuf {
        self.data_dir.join("local")
    }
    pub fn extension_apks_dir(&self) -> PathBuf {
        self.data_dir.join("extensions").join("apks")
    }
    pub fn extension_jars_dir(&self) -> PathBuf {
        self.data_dir.join("extensions").join("jars")
    }
    pub fn android_compat_root(&self) -> PathBuf {
        self.data_dir.join("android-compat")
    }
    pub fn cef_bin_dir(&self) -> PathBuf {
        self.data_dir.join("cef").join("bin")
    }
    pub fn cef_cache_dir(&self) -> PathBuf {
        self.data_dir.join("cef").join("cache")
    }
    pub fn downloads_root(&self) -> PathBuf {
        self.data_dir.join("downloads")
    }
    pub fn remote_cache_dir(&self) -> PathBuf {
        self.data_dir.join("cache").join("remote_mirror")
    }
    /// Rust-only: the size-capped page cache.
    pub fn page_cache_dir(&self) -> PathBuf {
        self.data_dir.join("cache").join("pages")
    }
    /// Rust-only: where the extension host is installed (`<data>/ext-host/<version>/`).
    pub fn ext_host_dir(&self) -> PathBuf {
        self.data_dir.join("ext-host")
    }
    /// Rust-only: log files.
    pub fn logs_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }
    /// AniList session written by the Kotlin app (`anilist.json`).
    pub fn anilist_file(&self) -> PathBuf {
        self.data_dir.join("anilist.json")
    }
}

/// Pure resolution logic, separated so it can be tested on any OS.
fn resolve(
    windows: bool,
    override_dir: Option<PathBuf>,
    local_app_data: Option<PathBuf>,
    xdg_data_home: Option<PathBuf>,
    home: Option<PathBuf>,
) -> Option<PathBuf> {
    if let Some(dir) = override_dir {
        return Some(dir);
    }
    let base = if windows {
        local_app_data
    } else {
        xdg_data_home.or_else(|| home.as_ref().map(|h| h.join(".local").join("share")))
    };
    let base = base.or_else(|| home.map(|h| h.join(".tachimana-base")))?;
    Some(base.join("TachiMana"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_wins() {
        let p = resolve(
            true,
            Some("/x".into()),
            Some("/l".into()),
            None,
            Some("/h".into()),
        );
        assert_eq!(p, Some(PathBuf::from("/x")));
    }

    #[test]
    fn windows_uses_local_app_data() {
        let p = resolve(
            true,
            None,
            Some("C:/L".into()),
            Some("/xdg".into()),
            Some("/h".into()),
        );
        assert_eq!(p, Some(PathBuf::from("C:/L").join("TachiMana")));
    }

    #[test]
    fn linux_prefers_xdg_then_home() {
        let p = resolve(
            false,
            None,
            Some("/l".into()),
            Some("/xdg".into()),
            Some("/h".into()),
        );
        assert_eq!(p, Some(PathBuf::from("/xdg").join("TachiMana")));
        let p = resolve(false, None, None, None, Some("/h".into()));
        assert_eq!(p, Some(PathBuf::from("/h/.local/share").join("TachiMana")));
    }

    #[test]
    fn fallback_and_none() {
        let p = resolve(true, None, None, None, Some("/h".into()));
        assert_eq!(
            p,
            Some(PathBuf::from("/h/.tachimana-base").join("TachiMana"))
        );
        assert_eq!(resolve(true, None, None, None, None), None);
    }

    #[test]
    fn sub_paths_match_kotlin() {
        let dir = tempfile::tempdir().unwrap();
        let p = AppPaths::at(dir.path()).unwrap();
        assert_eq!(p.database_file(), dir.path().join("tachimana.db"));
        assert_eq!(p.extension_apks_dir(), dir.path().join("extensions/apks"));
        assert_eq!(p.remote_cache_dir(), dir.path().join("cache/remote_mirror"));
    }
}
