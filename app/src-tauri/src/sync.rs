//! Backup files and LAN sync (`SyncManager.kt`, `ui/sync/SyncPage.kt`).
//!
//! At start-up the app serves its library on the LAN and advertises itself over mDNS, like the
//! Kotlin app. Another TachiMana (Kotlin or Rust) pulls with the pairing code shown on the Sync
//! screen. Backups are Mihon-compatible `.tachibk` files. Settings, including the NAS password,
//! are never part of a backup.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tm_backup::{BackupExtensionStore, ImportSummary};
use tm_db::Db;
use tm_sync::discovery::Discovery;
use tm_sync::server::SyncServer;
use tm_sync::{DeviceIdentity, PeerDevice, PullProgress, PullReport, SyncClient, SyncError};
use ts_rs::TS;

use crate::dto::CmdError;
use crate::state::AppState;

type R<T> = Result<T, CmdError>;

/// Sync state held by [`AppState`].
pub struct Sync {
    pub identity: DeviceIdentity,
    server: Mutex<Option<SyncServer>>,
    serve_error: Mutex<Option<String>>,
    discovery: Mutex<Option<Discovery>>,
    peers: Mutex<Vec<PeerDevice>>,
    progress: Mutex<Option<String>>,
    last_report: Mutex<Option<PullReport>>,
    last_error: Mutex<Option<String>>,
    /// Extension stores from the last restore or pull, waiting for the user to accept.
    pending: Mutex<Vec<BackupExtensionStore>>,
    pulling: AtomicBool,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    }
}

impl Sync {
    pub fn open(identity_file: &Path) -> Self {
        Self {
            identity: DeviceIdentity::load_or_create(identity_file),
            server: Mutex::new(None),
            serve_error: Mutex::new(None),
            discovery: Mutex::new(None),
            peers: Mutex::new(Vec::new()),
            progress: Mutex::new(None),
            last_report: Mutex::new(None),
            last_error: Mutex::new(None),
            pending: Mutex::new(Vec::new()),
            pulling: AtomicBool::new(false),
        }
    }

    fn add_peer(&self, peer: PeerDevice) {
        let mut peers = lock(&self.peers);
        peers.retain(|p| p.id != peer.id);
        peers.push(peer);
        peers.sort_by_key(|p| p.name.to_lowercase());
    }

    fn set_pending(&self, stores: Vec<BackupExtensionStore>) {
        *lock(&self.pending) = stores;
    }
}

// ---------------------------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncDeviceDto {
    pub id: String,
    pub name: String,
    pub pairing_code: String,
    pub port: Option<u16>,
    /// LAN addresses another device can type in by hand.
    pub addresses: Vec<String>,
    /// Why the server or mDNS is not running, if it isn't.
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PeerDto {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
}

impl From<&PeerDevice> for PeerDto {
    fn from(p: &PeerDevice) -> Self {
        Self {
            id: p.id.clone(),
            name: p.name.clone(),
            host: p.host.clone(),
            port: p.port,
        }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImportSummaryDto {
    pub manga_added: u32,
    pub manga_updated: u32,
    pub chapters_added: u32,
    pub chapters_updated: u32,
    pub categories_added: u32,
}

fn n(v: usize) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}

impl From<&ImportSummary> for ImportSummaryDto {
    fn from(s: &ImportSummary) -> Self {
        Self {
            manga_added: n(s.manga_added),
            manga_updated: n(s.manga_updated),
            chapters_added: n(s.chapters_added),
            chapters_updated: n(s.chapters_updated),
            categories_added: n(s.categories_added),
        }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PullReportDto {
    pub peer_name: String,
    pub summary: ImportSummaryDto,
    pub files_pulled: u32,
    #[ts(type = "number")]
    pub bytes_pulled: u64,
}

impl From<&PullReport> for PullReportDto {
    fn from(r: &PullReport) -> Self {
        Self {
            peer_name: r.peer_name.clone(),
            summary: (&r.summary).into(),
            files_pulled: n(r.files_pulled),
            bytes_pulled: r.bytes_pulled,
        }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PendingRepoDto {
    pub index_url: String,
    pub name: String,
    pub badge: Option<String>,
    pub website: String,
}

impl From<&BackupExtensionStore> for PendingRepoDto {
    fn from(s: &BackupExtensionStore) -> Self {
        Self {
            index_url: s.index_url().to_owned(),
            name: s.name().to_owned(),
            badge: s.badge_label.clone(),
            website: s.contact_website.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncStatusDto {
    pub device: SyncDeviceDto,
    pub peers: Vec<PeerDto>,
    /// The running pull's current step, or `None` when idle.
    pub progress: Option<String>,
    pub last_report: Option<PullReportDto>,
    pub last_error: Option<String>,
    pub pending_repos: Vec<PendingRepoDto>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RestoreDto {
    pub file: String,
    pub summary: ImportSummaryDto,
    pub pending_repos: Vec<PendingRepoDto>,
}

fn status_of(state: &AppState) -> SyncStatusDto {
    let s = &state.sync;
    let port = lock(&s.server).as_ref().map(SyncServer::port);
    SyncStatusDto {
        device: SyncDeviceDto {
            id: s.identity.id.clone(),
            name: s.identity.name.clone(),
            pairing_code: s.identity.pairing_code.clone(),
            port,
            addresses: if port.is_some() {
                tm_sync::discovery::local_ipv4s()
            } else {
                Vec::new()
            },
            error: lock(&s.serve_error).clone(),
        },
        peers: lock(&s.peers).iter().map(PeerDto::from).collect(),
        progress: lock(&s.progress).clone(),
        last_report: lock(&s.last_report).as_ref().map(PullReportDto::from),
        last_error: lock(&s.last_error).clone(),
        pending_repos: lock(&s.pending).iter().map(PendingRepoDto::from).collect(),
    }
}

fn emit(app: &AppHandle) {
    let _ = app.emit("sync", status_of(&app.state::<AppState>()));
}

// ---------------------------------------------------------------------------------------------
// Start-up
// ---------------------------------------------------------------------------------------------

/// The database behind the app handle, for work that outlives a command.
struct AppDb(AppHandle);

impl AsRef<Db> for AppDb {
    fn as_ref(&self) -> &Db {
        &self.0.state::<AppState>().inner().db
    }
}

fn backup_bytes(state: &AppState) -> Result<Vec<u8>, String> {
    let b = tm_backup::export(&state.db, &state.nas.source_names()).map_err(|e| e.to_string())?;
    tm_backup::codec::encode(&b).map_err(|e| e.to_string())
}

/// Serve the library and advertise it. Failures are shown on the Sync screen, not fatal.
pub fn init(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let handle = app.clone();
        let backup: Arc<tm_sync::server::BackupFn> =
            Arc::new(move || backup_bytes(&handle.state::<AppState>()));
        let local = state.paths.local_library_dir();
        let started = SyncServer::start(state.sync.identity.clone(), local, backup, None).await;
        match started {
            Ok(server) => {
                let port = server.port();
                *lock(&state.sync.server) = Some(server);
                match start_discovery(&app, port) {
                    Ok(d) => *lock(&state.sync.discovery) = Some(d),
                    Err(e) => {
                        tracing::warn!("mDNS unavailable: {e}");
                        *lock(&state.sync.serve_error) = Some(format!(
                            "Other devices can't find this one automatically ({e}). Add it by address instead."
                        ));
                    }
                }
            }
            Err(e) => {
                tracing::warn!("sync server not started: {e}");
                *lock(&state.sync.serve_error) =
                    Some(format!("This device can't be synced from right now: {e}"));
            }
        }
        emit(&app);
    });
}

fn start_discovery(app: &AppHandle, port: u16) -> Result<Discovery, SyncError> {
    let state = app.state::<AppState>();
    let id = &state.sync.identity;
    let mut d = Discovery::new()?;
    d.advertise(&id.id, &id.name, port)?;
    let handle = app.clone();
    d.browse(
        &id.id,
        Arc::new(move |peer| {
            handle.state::<AppState>().sync.add_peer(peer);
            emit(&handle);
        }),
    )?;
    Ok(d)
}

// ---------------------------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------------------------

impl From<SyncError> for CmdError {
    fn from(e: SyncError) -> Self {
        match e {
            SyncError::WrongCode => Self::new(
                "wrongCode",
                "That pairing code is not right. Check the code shown on the other device.",
            ),
            other => Self::new("sync", other.to_string()),
        }
    }
}

impl From<tm_backup::BackupError> for CmdError {
    fn from(e: tm_backup::BackupError) -> Self {
        match e {
            tm_backup::BackupError::Db(d) => d.into(),
            other => Self::new("backup", other.to_string()),
        }
    }
}

#[tauri::command]
pub fn sync_status(state: State<'_, AppState>) -> SyncStatusDto {
    status_of(&state)
}

/// Check `host:port` and add it to the peers (for networks where mDNS does not work).
#[tauri::command]
pub async fn sync_add_peer(app: AppHandle, host: String, port: u16) -> R<PeerDto> {
    let host = host.trim().to_owned();
    if host.is_empty() {
        return Err(CmdError::new(
            "invalid",
            "Enter the other device's address.",
        ));
    }
    let client = SyncClient::new()?;
    let Some(peer) = client.probe(&host, port).await else {
        return Err(CmdError::new(
            "unreachable",
            format!("No TachiMana answered at {host}:{port}. Check the address and that both devices are on the same network."),
        ));
    };
    let state = app.state::<AppState>();
    if peer.id == state.sync.identity.id {
        return Err(CmdError::new("invalid", "That address is this device."));
    }
    let dto = PeerDto::from(&peer);
    state.sync.add_peer(peer);
    emit(&app);
    Ok(dto)
}

fn progress_text(peer: &str, p: &PullProgress) -> String {
    match p {
        PullProgress::Manifest => format!("Contacting {peer}…"),
        PullProgress::Backup { bytes } => {
            format!("Downloading the library ({} KB)…", bytes / 1024)
        }
        PullProgress::Importing => "Adding to your library…".to_owned(),
        PullProgress::File { index, total, path } => format!("File {index} of {total}: {path}"),
    }
}

/// Pull a peer's library into this one. Progress arrives as `sync` events.
#[tauri::command]
pub async fn sync_pull(app: AppHandle, peer_id: String, code: String) -> R<PullReportDto> {
    let state = app.state::<AppState>();
    let code = code.trim().to_owned();
    if !tm_sync::identity::is_pairing_code(&code) {
        return Err(CmdError::new("invalid", "The pairing code is six digits."));
    }
    let peer = lock(&state.sync.peers)
        .iter()
        .find(|p| p.id == peer_id)
        .cloned()
        .ok_or_else(|| CmdError::not_found("That device"))?;
    if state.sync.pulling.swap(true, Ordering::AcqRel) {
        return Err(CmdError::new("busy", "A sync is already running."));
    }
    *lock(&state.sync.last_error) = None;
    emit(&app);

    let result = async {
        let client = SyncClient::new()?;
        let handle = app.clone();
        let name = peer.name.clone();
        let on_progress = move |p: PullProgress| {
            let st = handle.state::<AppState>();
            *lock(&st.sync.progress) = Some(progress_text(&name, &p));
            emit(&handle);
        };
        tm_sync::pull(
            &client,
            &peer,
            &code,
            Arc::new(AppDb(app.clone())),
            &state.paths.local_library_dir(),
            &on_progress,
        )
        .await
    }
    .await;

    *lock(&state.sync.progress) = None;
    state.sync.pulling.store(false, Ordering::Release);
    let out = match result {
        Ok(report) => {
            state.sync.set_pending(report.summary.pending_repos.clone());
            let dto = PullReportDto::from(&report);
            *lock(&state.sync.last_report) = Some(report);
            let _ = app.emit("library-changed", ());
            Ok(dto)
        }
        Err(e) => {
            let err = CmdError::from(e);
            *lock(&state.sync.last_error) = Some(err.message.clone());
            Err(err)
        }
    };
    emit(&app);
    out
}

fn today() -> String {
    // Days since the epoch to a civil date (Howard Hinnant's algorithm), to name the file.
    let days = i64::try_from(crate::state::now_secs() / 86_400).unwrap_or(0);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Write a backup of the library to `path`.
pub fn export_to(state: &AppState, path: &Path) -> R<()> {
    let bytes = backup_bytes(state).map_err(|e| CmdError::new("backup", e))?;
    let mut part = path.as_os_str().to_owned();
    part.push(".part");
    let part = PathBuf::from(part);
    std::fs::write(&part, &bytes)
        .and_then(|()| std::fs::rename(&part, path))
        .map_err(|e| {
            let _ = std::fs::remove_file(&part);
            CmdError::new("io", format!("Could not save the backup: {e}"))
        })
}

/// Restore `path` into the library; extension stores it lists become pending.
pub fn restore_from(state: &AppState, path: &Path) -> R<RestoreDto> {
    let meta = std::fs::metadata(path)
        .map_err(|e| CmdError::new("io", format!("Could not open the file: {e}")))?;
    if meta.len() > tm_backup::codec::MAX_DECODED {
        return Err(CmdError::new(
            "backup",
            "That file is too large to be a backup.",
        ));
    }
    let bytes = std::fs::read(path)
        .map_err(|e| CmdError::new("io", format!("Could not read the file: {e}")))?;
    let backup = tm_backup::codec::decode(&bytes)?;
    let summary = tm_backup::import(&state.db, &backup)?;
    let local = state.paths.local_library_dir();
    tm_backup::relink_local_covers(&state.db, &local)?;
    tm_local::scan(&state.db, &local)?;
    state.sync.set_pending(summary.pending_repos.clone());
    Ok(RestoreDto {
        file: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        summary: (&summary).into(),
        pending_repos: summary
            .pending_repos
            .iter()
            .map(PendingRepoDto::from)
            .collect(),
    })
}

/// Ask where to save, then save a backup there. `None` when the user cancels.
#[tauri::command]
pub async fn backup_export(app: AppHandle) -> R<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    let name = format!("tachimana_{}.tachibk", today());
    let picked = app
        .dialog()
        .file()
        .set_title("Save backup")
        .set_file_name(&name)
        .add_filter("TachiMana / Mihon backup", &["tachibk"])
        .blocking_save_file();
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    export_to(&app.state::<AppState>(), &path)?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

/// Ask for a backup file, then restore it. `None` when the user cancels.
#[tauri::command]
pub async fn backup_restore(app: AppHandle) -> R<Option<RestoreDto>> {
    use tauri_plugin_dialog::DialogExt;
    let picked = app
        .dialog()
        .file()
        .set_title("Restore from backup")
        .add_filter("Backups", &["tachibk", "gz", "proto"])
        .blocking_pick_file();
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    let state = app.state::<AppState>();
    let out = restore_from(&state, &path)?;
    let _ = app.emit("library-changed", ());
    emit(&app);
    Ok(Some(out))
}

/// Add the pending extension stores the user accepted (by index URL), or all when `None`.
#[tauri::command]
pub fn backup_accept_repos(
    app: AppHandle,
    state: State<'_, AppState>,
    index_urls: Option<Vec<String>>,
) -> R<()> {
    let accepted: Vec<BackupExtensionStore> = {
        let mut pending = lock(&state.sync.pending);
        let (take, keep): (Vec<_>, Vec<_>) = pending.drain(..).partition(|s| {
            index_urls
                .as_ref()
                .is_none_or(|u| u.iter().any(|x| x == s.index_url()))
        });
        *pending = keep;
        take
    };
    tm_backup::add_extension_repos(&state.db, &accepted)?;
    emit(&app);
    Ok(())
}

/// Forget the pending stores without adding them.
#[tauri::command]
pub fn backup_dismiss_repos(app: AppHandle, state: State<'_, AppState>) {
    state.sync.set_pending(Vec::new());
    emit(&app);
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
    use super::*;

    #[test]
    fn today_is_a_date() {
        let t = today();
        assert_eq!(t.len(), 10);
        assert!(t.starts_with("20"));
    }

    #[test]
    fn export_then_restore_into_another_install() {
        let a_dir = tempfile::tempdir().unwrap();
        let a = AppState::open(tm_core::AppPaths::at(a_dir.path()).unwrap()).unwrap();
        a.db.upsert_extension_repo(&tm_db::ExtensionRepo {
            base_url: "https://example.com/index.pb".into(),
            name: "Example".into(),
            short_name: None,
            website: String::new(),
            signing_key_fingerprint: "ab".into(),
        })
        .unwrap();
        {
            let mut s = a.settings().unwrap();
            s.set(tm_core::settings::keys::REMOTE_PASSWORD, "hunter2-secret");
        }
        let file = a_dir.path().join("out.tachibk");
        export_to(&a, &file).unwrap();
        // Look inside the container: settings never travel, so the password is absent.
        let inner = {
            use tm_backup::prost::Message;
            tm_backup::codec::decode(&std::fs::read(&file).unwrap())
                .unwrap()
                .encode_to_vec()
        };
        assert!(!inner.is_empty());
        assert!(
            !inner.windows(7).any(|w| w == b"hunter2"),
            "no password in backups"
        );

        let b_dir = tempfile::tempdir().unwrap();
        let b = AppState::open(tm_core::AppPaths::at(b_dir.path()).unwrap()).unwrap();
        let r = restore_from(&b, &file).unwrap();
        assert_eq!(r.pending_repos.len(), 1);
        assert!(b.db.extension_repos().unwrap().is_empty());
        assert_eq!(lock(&b.sync.pending).len(), 1);
    }
}
