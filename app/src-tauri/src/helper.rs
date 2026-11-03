//! The extension helper (Kotlin ext-host plus its own trimmed Java) is not inside the installer
//! (PLAN.md §P8.4). Settings installs it from the zip published with this version's release, or
//! from a zip the user picked, into `<data>/ext-host`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tm_exthost::install::{install_zip, InstallError};
use tm_update::{Asset, UpdateError, Updater};
use ts_rs::TS;

use crate::dto::CmdError;
use crate::state::AppState;

type R<T> = Result<T, CmdError>;

const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// Set while a helper install runs.
pub static INSTALLING: AtomicBool = AtomicBool::new(false);

/// The zip name for this computer, when a helper is built for it.
pub fn platform() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Some("windows-x64"),
        ("linux", "x86_64") => Some("linux-x64"),
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HelperStatusDto {
    /// `custom` (TM_EXTHOST_DIR), `installed` (data folder), `bundled`, `dev` or `missing`.
    pub source: String,
    pub folder: String,
    /// A helper zip is published for this computer, so Download can work.
    pub can_download: bool,
    pub busy: bool,
}

pub fn status_of(state: &AppState) -> HelperStatusDto {
    let (source, folder) = crate::online::host_location(state);
    HelperStatusDto {
        source: source.to_owned(),
        folder: folder.to_string_lossy().into_owned(),
        can_download: platform().is_some(),
        busy: INSTALLING.load(Ordering::Relaxed),
    }
}

#[tauri::command]
pub fn helper_status(state: State<'_, AppState>) -> HelperStatusDto {
    status_of(&state)
}

fn words(e: InstallError) -> CmdError {
    CmdError::new("helper", format!("Couldn't install the helper: {e}"))
}

/// Stops a running host, then unpacks `zip` into the data folder.
async fn install(app: &AppHandle, zip: PathBuf) -> R<HelperStatusDto> {
    if INSTALLING.swap(true, Ordering::AcqRel) {
        return Err(CmdError::new(
            "busy",
            "The helper is already being installed.",
        ));
    }
    let state = app.state::<AppState>();
    let dest = state.paths.ext_host_dir();
    // The host may be running from that folder; dropping it closes and waits for it.
    if let Ok(mut slot) = state.exthost.lock() {
        *slot = None;
    }
    let r = tauri::async_runtime::spawn_blocking(move || install_zip(&zip, &dest)).await;
    INSTALLING.store(false, Ordering::Release);
    match r {
        Ok(Ok(n)) => {
            tracing::info!("helper installed ({n} files)");
            Ok(status_of(&state))
        }
        Ok(Err(e)) => Err(words(e)),
        Err(e) => Err(CmdError::internal(e.to_string())),
    }
}

/// Installs from a zip the user picks. `None` when the dialog is cancelled.
#[tauri::command]
pub async fn helper_install_file(app: AppHandle) -> R<Option<HelperStatusDto>> {
    use tauri_plugin_dialog::DialogExt;
    let picked = app
        .dialog()
        .file()
        .set_title("Install the extension helper")
        .add_filter("Helper zip", &["zip"])
        .blocking_pick_file();
    let Some(file) = picked else { return Ok(None) };
    let path = file
        .into_path()
        .map_err(|e| CmdError::new("invalid", format!("That file can't be opened: {e}")))?;
    install(&app, path).await.map(Some)
}

/// Where the helper zips are: the `extHostBaseUrl` setting when set (a folder holding
/// `tachimana-ext-host-<platform>.zip` and its `.sha256`, for example on a NAS), else this
/// version's release in the public releases repository ([`tm_update::DEFAULT_REPO`]).
fn download_base(state: &AppState) -> R<String> {
    let custom = state
        .settings
        .lock()
        .map(|s| s.string(tm_core::settings::keys::EXT_HOST_BASE_URL))
        .unwrap_or_default();
    let custom = custom.trim().trim_end_matches('/');
    if custom.is_empty() {
        return Ok(format!(
            "https://github.com/{}/releases/download/{}{CURRENT}",
            tm_update::DEFAULT_REPO,
            tm_update::TAG_PREFIX
        ));
    }
    if custom.starts_with("https://") || custom.starts_with("http://") {
        Ok(custom.to_owned())
    } else {
        Err(CmdError::new(
            "invalid",
            "The helper download address in settings must start with https:// or http://.",
        ))
    }
}

/// Downloads this version's helper from [`download_base`] (checking its SHA-256), then installs it.
#[tauri::command]
pub async fn helper_download(app: AppHandle) -> R<HelperStatusDto> {
    let plat = platform().ok_or_else(|| {
        CmdError::new(
            "invalid",
            "No helper is built for this kind of computer. Install one from a file.",
        )
    })?;
    let name = format!("tachimana-ext-host-{plat}.zip");
    let base = download_base(&app.state::<AppState>())?;
    let asset = Asset {
        url: format!("{base}/{name}"),
        sha256_url: Some(format!("{base}/{name}.sha256")),
        name,
        size: 0,
    };
    let updater = Updater::new().map_err(|e| CmdError::internal(e.to_string()))?;
    let dir = std::env::temp_dir().join("tachimana-helper");
    let progress_app = app.clone();
    let mut last = 0u64;
    let zip = updater
        .download(&asset, &dir, move |done, total| {
            // The Sync screen shows this as a progress line; every 2 MB is plenty.
            if done != total && done.saturating_sub(last) < 2 * 1024 * 1024 {
                return;
            }
            last = done;
            let _ = progress_app.emit(
                "helper-progress",
                serde_json::json!({ "done": done, "total": total }),
            );
        })
        .await
        .map_err(|e| match e {
            UpdateError::Status(404) => CmdError::new(
                "helper",
                format!(
                    "No helper for version {CURRENT} was found there. Install one from a file."
                ),
            ),
            UpdateError::Http(_) => CmdError::new(
                "network",
                "Couldn't reach GitHub. Check your connection and try again.",
            ),
            other => CmdError::new("helper", other.to_string()),
        })?;
    let r = install(&app, zip.clone()).await;
    let _ = std::fs::remove_file(&zip);
    r
}
