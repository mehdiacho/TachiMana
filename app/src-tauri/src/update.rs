//! Updates in the app (`update/UpdateChecker.kt`): a quiet check a little after start-up, a
//! "Check now" in Settings, and on Windows a download then a separate click to run the installer.
//! Elsewhere the release page opens instead. Failing to check never bothers the user.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tm_update::{Platform, Update, UpdateError, Updater};
use ts_rs::TS;

use crate::dto::CmdError;
use crate::state::AppState;

type R<T> = Result<T, CmdError>;

const CURRENT: &str = env!("CARGO_PKG_VERSION");
const STARTUP_DELAY: Duration = Duration::from_secs(10);

#[derive(Default)]
struct Inner {
    available: Option<Update>,
    checked: bool,
    error: Option<String>,
    /// Bytes so far and total while downloading.
    progress: Option<(u64, u64)>,
    installer: Option<PathBuf>,
}

pub struct Updates {
    updater: Option<Updater>,
    inner: Mutex<Inner>,
    busy: AtomicBool,
}

/// Debug builds can point the check at a fake GitHub API (`TM_UPDATE_API`) to try the update UI.
fn updater() -> Result<Updater, UpdateError> {
    match std::env::var("TM_UPDATE_API") {
        Ok(api) if cfg!(debug_assertions) => Updater::with(&api, tm_update::DEFAULT_REPO),
        _ => Updater::new(),
    }
}

impl Updates {
    pub fn new() -> Self {
        Self {
            updater: updater().ok(),
            inner: Mutex::new(Inner::default()),
            busy: AtomicBool::new(false),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // Plain values replaced whole; a panic elsewhere can't leave them half-written.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateStatusDto {
    pub current: String,
    /// The newer version, when there is one.
    pub version: Option<String>,
    pub notes: Option<String>,
    /// This build can download and run the installer itself (Windows).
    pub can_install: bool,
    pub checked: bool,
    pub busy: bool,
    pub error: Option<String>,
    /// 0..1 while downloading.
    pub progress: Option<f64>,
    /// The installer is downloaded and ready to run.
    pub ready: bool,
}

fn status_of(state: &AppState) -> UpdateStatusDto {
    let u = &state.updates;
    let i = u.lock();
    UpdateStatusDto {
        current: CURRENT.to_owned(),
        version: i.available.as_ref().map(|a| a.version.clone()),
        notes: i.available.as_ref().map(|a| a.notes.clone()),
        can_install: i.available.as_ref().is_some_and(|a| a.installer.is_some()),
        checked: i.checked,
        busy: u.busy.load(Ordering::Relaxed),
        error: i.error.clone(),
        progress: i
            .progress
            .map(|(n, t)| if t > 0 { n as f64 / t as f64 } else { 0.0 }),
        ready: i.installer.is_some(),
    }
}

fn emit(app: &AppHandle) {
    let _ = app.emit("update", status_of(&app.state::<AppState>()));
}

fn words(e: &UpdateError) -> String {
    match e {
        UpdateError::Http(_) => {
            "Couldn't reach GitHub. Check your connection and try again.".into()
        }
        UpdateError::Status(404) => {
            "No releases are published where this app looks for them.".into()
        }
        UpdateError::Checksum => {
            "The download didn't match its checksum, so it was deleted.".into()
        }
        other => other.to_string(),
    }
}

async fn check(app: &AppHandle) -> R<UpdateStatusDto> {
    let state = app.state::<AppState>();
    let u = &state.updates;
    let Some(updater) = &u.updater else {
        return Err(CmdError::internal("The update client could not be set up."));
    };
    if u.busy.swap(true, Ordering::AcqRel) {
        return Ok(status_of(&state));
    }
    emit(app);
    let r = updater.check(CURRENT, Platform::current()).await;
    {
        let mut i = u.lock();
        i.checked = true;
        match r {
            Ok(a) => {
                if a.as_ref().map(|x| &x.version) != i.available.as_ref().map(|x| &x.version) {
                    i.installer = None;
                }
                i.available = a;
                i.error = None;
            }
            Err(e) => {
                tracing::debug!("update check failed: {e}");
                i.error = Some(words(&e));
            }
        }
    }
    u.busy.store(false, Ordering::Release);
    emit(app);
    Ok(status_of(&state))
}

/// One quiet check after start-up.
pub fn init(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(STARTUP_DELAY).await;
        let _ = check(&app).await;
    });
}

#[tauri::command]
pub fn update_status(state: State<'_, AppState>) -> UpdateStatusDto {
    status_of(&state)
}

#[tauri::command]
pub async fn update_check(app: AppHandle) -> R<UpdateStatusDto> {
    check(&app).await
}

/// Downloads the installer of the available update (Windows). Progress arrives as `update` events.
#[tauri::command]
pub async fn update_download(app: AppHandle) -> R<UpdateStatusDto> {
    let state = app.state::<AppState>();
    let u = &state.updates;
    let Some(updater) = &u.updater else {
        return Err(CmdError::internal("The update client could not be set up."));
    };
    let asset = u
        .lock()
        .available
        .as_ref()
        .and_then(|a| a.installer.clone())
        .ok_or_else(|| {
            CmdError::new(
                "invalid",
                "There is no installer to download for this computer.",
            )
        })?;
    if u.busy.swap(true, Ordering::AcqRel) {
        return Err(CmdError::new(
            "busy",
            "An update check or download is already running.",
        ));
    }
    let dir = std::env::temp_dir().join("tachimana-update");
    let progress_app = app.clone();
    let mut last_emit = std::time::Instant::now();
    let r = updater
        .download(&asset, &dir, |n, t| {
            progress_app.state::<AppState>().updates.lock().progress = Some((n, t));
            if last_emit.elapsed() > Duration::from_millis(250) {
                last_emit = std::time::Instant::now();
                emit(&progress_app);
            }
        })
        .await;
    {
        let mut i = u.lock();
        i.progress = None;
        match &r {
            Ok(p) => {
                i.installer = Some(p.clone());
                i.error = None;
            }
            Err(e) => i.error = Some(words(e)),
        }
    }
    u.busy.store(false, Ordering::Release);
    emit(&app);
    match r {
        Ok(_) => Ok(status_of(&state)),
        Err(e) => Err(CmdError::new("update", words(&e))),
    }
}

/// Runs the downloaded installer and quits so it can replace this app's files. The path is the
/// one this app downloaded, never one from the page.
#[tauri::command]
pub fn update_install(app: AppHandle, state: State<'_, AppState>) -> R<()> {
    let path = state
        .updates
        .lock()
        .installer
        .clone()
        .ok_or_else(|| CmdError::new("invalid", "Download the update first."))?;
    std::process::Command::new(&path)
        .spawn()
        .map_err(|e| CmdError::new("update", format!("Couldn't start the installer: {e}")))?;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        app.exit(0);
    });
    Ok(())
}

/// Opens the release page (the way to update where the app can't install itself).
#[tauri::command]
pub fn update_open_page(app: AppHandle, state: State<'_, AppState>) -> R<()> {
    use tauri_plugin_opener::OpenerExt;
    let url = state
        .updates
        .lock()
        .available
        .as_ref()
        .map(|a| a.page_url.clone())
        .filter(|u| u.starts_with("https://github.com/"))
        .unwrap_or_else(|| format!("https://github.com/{}/releases", tm_update::DEFAULT_REPO));
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| CmdError::new("open", format!("Couldn't open the browser: {e}")))
}
