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
use tm_backup::{BackupExtension, BackupExtensionStore, DownloadedChapter, ImportSummary};
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
    /// What the last restore or pull can still do: install extensions, download chapters again.
    offer: Mutex<RestoreOffer>,
    /// Installed extensions as last seen from the host, written into backups (tag 900).
    installed: Mutex<Vec<BackupExtension>>,
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
            offer: Mutex::new(RestoreOffer::default()),
            installed: Mutex::new(Vec::new()),
            pulling: AtomicBool::new(false),
        }
    }

    fn add_peer(&self, peer: PeerDevice) {
        let mut peers = lock(&self.peers);
        peers.retain(|p| p.id != peer.id);
        peers.push(peer);
        peers.sort_by_key(|p| p.name.to_lowercase());
    }

    fn set_offer(&self, s: &ImportSummary) {
        *lock(&self.offer) = RestoreOffer {
            repos_added: s.repos_added.clone(),
            extensions: s.extensions.clone(),
            source_ids: s.source_ids.clone(),
            downloaded: s.downloaded.clone(),
            anilist: s.trackers.contains(&tm_db::ANILIST_SYNC_ID),
        };
    }

    /// Remember what the host has installed, for the next backup.
    pub fn set_installed(&self, exts: &[tm_exthost::model::Extension]) {
        *lock(&self.installed) = exts
            .iter()
            .map(|e| BackupExtension {
                pkg_name: e.pkg_name.clone(),
                name: Some(e.name.clone()),
                version_code: Some(e.version_code),
                version_name: Some(e.version_name.clone()),
                lang: Some(e.lang.clone()),
                source_ids: e.sources.iter().map(|s| s.id).collect(),
                ..Default::default()
            })
            .collect();
    }
}

/// The follow-up a restore offers. Cleared piece by piece as the user acts on it.
#[derive(Debug, Clone, Default)]
struct RestoreOffer {
    repos_added: Vec<BackupExtensionStore>,
    extensions: Vec<BackupExtension>,
    source_ids: Vec<i64>,
    downloaded: Vec<DownloadedChapter>,
    anilist: bool,
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
    pub offer: RestoreOfferDto,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RestoreDto {
    pub file: String,
    pub summary: ImportSummaryDto,
    /// How long the restore took, for the summary line.
    #[ts(type = "number")]
    pub elapsed_ms: u64,
    /// Manga that could not be restored, and why.
    pub errors: Vec<RestoreErrorDto>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RestoreErrorDto {
    pub title: String,
    pub message: String,
}

/// Sent as `restore-progress` while a restore runs.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RestoreProgressDto {
    /// "read", "library", "local" or "done".
    pub stage: String,
    pub done: u32,
    pub total: u32,
    /// The manga being restored (library stage).
    pub title: String,
}

/// What the last restore or pull can still do, for the Sync screen.
#[derive(Debug, Clone, Default, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RestoreOfferDto {
    /// Extension repos the restore added.
    pub repos_added: Vec<PendingRepoDto>,
    /// Whether there are extensions or sources to look up and offer to install.
    pub has_extensions: bool,
    /// Chapters that were downloaded on the other device, and how many of them are unread.
    pub downloaded: u32,
    pub downloaded_unread: u32,
    /// AniList rows came back but this install is not signed in.
    pub anilist_sign_in: bool,
}

/// The extensions a restore can install, and the repos that could not be read to find them.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RestoreExtensionsDto {
    pub extensions: Vec<RestoreExtensionDto>,
    /// "Repo name: error" for each repo whose catalog failed; its extensions show as not found.
    pub failures: Vec<String>,
}

/// One extension a restore can install, or why it can't.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RestoreExtensionDto {
    pub pkg_name: String,
    pub name: String,
    /// Repo to install from; `None` when no added repo offers it.
    pub repo_url: Option<String>,
    pub installed: bool,
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
        offer: offer_of(state),
    }
}

fn offer_of(state: &AppState) -> RestoreOfferDto {
    let o = lock(&state.sync.offer);
    RestoreOfferDto {
        repos_added: o.repos_added.iter().map(PendingRepoDto::from).collect(),
        has_extensions: !o.extensions.is_empty() || !o.source_ids.is_empty(),
        downloaded: n(o.downloaded.len()),
        downloaded_unread: n(o.downloaded.iter().filter(|d| !d.read).count()),
        anilist_sign_in: o.anilist && state.track.auth.session().is_none(),
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
    let mut b =
        tm_backup::export(&state.db, &state.nas.source_names()).map_err(|e| e.to_string())?;
    let installed = lock(&state.sync.installed).clone();
    let dm = state.downloads.get();
    tm_backup::annotate(&mut b, installed, |source, manga, chapter| {
        dm.is_some_and(|d| d.is_downloaded(source, manga, chapter))
    });
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
            state.sync.set_offer(&report.summary);
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

/// Restore `path` into the library. The repos it lists are added; installing extensions and
/// downloading chapters again are offered (see [`RestoreOfferDto`]).
#[cfg(test)]
pub fn restore_from(state: &AppState, path: &Path) -> R<RestoreDto> {
    restore_with(state, path, &mut |_| {})
}

/// [`restore_from`], reporting each step to `progress`.
pub fn restore_with(
    state: &AppState,
    path: &Path,
    progress: &mut dyn FnMut(RestoreProgressDto),
) -> R<RestoreDto> {
    let started = std::time::Instant::now();
    let step = |stage: &str, done: usize, total: usize, title: &str| RestoreProgressDto {
        stage: stage.to_owned(),
        done: n(done),
        total: n(total),
        title: title.to_owned(),
    };
    progress(step("read", 0, 0, ""));
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
    drop(bytes);
    let summary = tm_backup::import_with(&state.db, &backup, &mut |done, total, title| {
        progress(step("library", done, total, title));
    })?;
    progress(step("local", 0, 0, ""));
    let local = state.paths.local_library_dir();
    tm_backup::relink_local_covers(&state.db, &local)?;
    tm_local::scan(&state.db, &local)?;
    state.sync.set_offer(&summary);
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    tracing::info!(
        manga = summary.manga_added,
        chapters = summary.chapters_added,
        errors = summary.errors.len(),
        elapsed_ms,
        "backup restored"
    );
    progress(step("done", 0, 0, ""));
    Ok(RestoreDto {
        file: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        summary: (&summary).into(),
        elapsed_ms,
        errors: summary
            .errors
            .iter()
            .map(|e| RestoreErrorDto {
                title: e.title.clone(),
                message: e.message.clone(),
            })
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
    let state = app.state::<AppState>();
    // Refresh the installed list when the host is set up; a backup without it is still a backup.
    if crate::online::host_available(&state) {
        match crate::online::client(&state) {
            Ok(c) => match c.extensions().await {
                Ok(exts) => state.sync.set_installed(&exts),
                Err(e) => tracing::warn!("backup without the extension list: {e}"),
            },
            Err(e) => tracing::warn!("backup without the extension list: {}", e.message),
        }
    }
    export_to(&state, &path)?;
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
    restore_in_background(app, path).await.map(Some)
}

/// Restore a backup the user dropped on the window (they confirmed it in the UI).
#[tauri::command]
pub async fn backup_restore_path(app: AppHandle, path: String) -> R<RestoreDto> {
    let path = PathBuf::from(path.trim());
    if !path.is_absolute() || !path.is_file() {
        return Err(CmdError::new(
            "invalid",
            "That is not a file on this computer.",
        ));
    }
    restore_in_background(app, path).await
}

/// Set while a restore runs, so a second one (a double drop) waits its turn with an error.
static RESTORING: AtomicBool = AtomicBool::new(false);

/// Run the restore on a blocking thread, sending `restore-progress` at most every 100 ms.
async fn restore_in_background(app: AppHandle, path: PathBuf) -> R<RestoreDto> {
    if RESTORING.swap(true, Ordering::AcqRel) {
        return Err(CmdError::new("busy", "A restore is already running."));
    }
    let handle = app.clone();
    let joined = tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        let mut last: Option<std::time::Instant> = None;
        restore_with(&state, &path, &mut |p| {
            let due = last.is_none_or(|t| t.elapsed() >= std::time::Duration::from_millis(100));
            if due || p.stage != "library" || p.done == p.total {
                last = Some(std::time::Instant::now());
                let _ = handle.emit("restore-progress", p);
            }
        })
    })
    .await;
    RESTORING.store(false, Ordering::Release);
    let out = joined.map_err(|e| CmdError::internal(format!("The restore stopped: {e}")))??;
    let _ = app.emit("library-changed", ());
    emit(&app);
    Ok(out)
}

/// The extensions the last restore can install: the backup's own list when it has one (tag
/// 900), otherwise every catalog extension that provides one of the library's sources.
#[tauri::command]
pub async fn restore_extensions(state: State<'_, AppState>) -> R<RestoreExtensionsDto> {
    let (wanted, source_ids) = {
        let o = lock(&state.sync.offer);
        (o.extensions.clone(), o.source_ids.clone())
    };
    if wanted.is_empty() && source_ids.is_empty() {
        return Ok(RestoreExtensionsDto {
            extensions: Vec::new(),
            failures: Vec::new(),
        });
    }
    let c = crate::online::client(&state)?;
    let installed: std::collections::HashSet<String> = c
        .extensions()
        .await?
        .into_iter()
        .map(|e| e.pkg_name)
        .collect();
    let mut catalog = Vec::new();
    let mut failures = Vec::new();
    for repo in state.db.extension_repos()? {
        match c.catalog(&repo.base_url).await {
            Ok(list) => catalog.extend(list.into_iter().map(|e| (repo.base_url.clone(), e))),
            Err(e) => {
                tracing::warn!(repo = %repo.base_url, "catalog failed: {e}");
                failures.push(format!("{}: {e}", repo.name));
            }
        }
    }
    Ok(RestoreExtensionsDto {
        extensions: plan_extensions(&wanted, &source_ids, &installed, &catalog),
        failures,
    })
}

/// Match a backup's extensions (or, lacking those, its source ids) against the catalog.
fn plan_extensions(
    wanted: &[BackupExtension],
    source_ids: &[i64],
    installed: &std::collections::HashSet<String>,
    catalog: &[(String, tm_exthost::model::AvailableExtension)],
) -> Vec<RestoreExtensionDto> {
    let find = |pkg: &str| catalog.iter().find(|(_, e)| e.pkg_name == pkg);
    let mut out: Vec<RestoreExtensionDto> = if wanted.is_empty() {
        let mut seen = std::collections::HashSet::new();
        catalog
            .iter()
            .filter(|(_, e)| e.sources.iter().any(|s| source_ids.contains(&s.id)))
            .filter(|(_, e)| seen.insert(e.pkg_name.clone()))
            .map(|(repo, e)| RestoreExtensionDto {
                pkg_name: e.pkg_name.clone(),
                name: e.name.clone(),
                repo_url: Some(repo.clone()),
                installed: installed.contains(&e.pkg_name),
            })
            .collect()
    } else {
        wanted
            .iter()
            .map(|w| {
                let hit = find(&w.pkg_name);
                RestoreExtensionDto {
                    pkg_name: w.pkg_name.clone(),
                    name: hit
                        .map(|(_, e)| e.name.clone())
                        .or_else(|| w.name.clone())
                        .unwrap_or_else(|| w.pkg_name.clone()),
                    repo_url: hit.map(|(r, _)| r.clone()),
                    installed: installed.contains(&w.pkg_name),
                }
            })
            .collect()
    };
    out.sort_by_key(|e| e.name.to_lowercase());
    out
}

/// Queue the chapters the backup's device had downloaded, all of them or only the unread ones.
/// Returns how many were queued; the offer is cleared either way.
#[tauri::command]
pub fn restore_redownload(app: AppHandle, state: State<'_, AppState>, unread_only: bool) -> R<u32> {
    let wanted = std::mem::take(&mut lock(&state.sync.offer).downloaded);
    let dm = crate::downloads::manager(&state)?;
    let mut queued = 0u32;
    let mut by_manga: std::collections::BTreeMap<i64, Vec<i64>> = Default::default();
    for d in wanted.iter().filter(|d| !(unread_only && d.read)) {
        by_manga.entry(d.manga_id).or_default().push(d.chapter_id);
    }
    for (manga_id, ids) in by_manga {
        let Some(manga) = state.db.manga(manga_id)? else {
            continue;
        };
        if manga.is_local() {
            continue;
        }
        let ids: std::collections::HashSet<i64> = ids.into_iter().collect();
        for ch in state
            .db
            .chapters(manga_id)?
            .iter()
            .filter(|c| ids.contains(&c.id))
        {
            if dm.enqueue(crate::downloads::request(&manga, ch)) {
                queued = queued.saturating_add(1);
            }
        }
    }
    emit(&app);
    Ok(queued)
}

/// Put away what is left of the restore offer.
#[tauri::command]
pub fn restore_dismiss(app: AppHandle, state: State<'_, AppState>) {
    *lock(&state.sync.offer) = RestoreOffer::default();
    emit(&app);
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
    use super::*;

    fn avail(pkg: &str, name: &str, source: i64) -> tm_exthost::model::AvailableExtension {
        tm_exthost::model::AvailableExtension {
            name: name.into(),
            pkg_name: pkg.into(),
            version_name: "1.4.1".into(),
            version_code: 1,
            lib_version: "1.4".into(),
            lang: "en".into(),
            nsfw: false,
            icon_url: String::new(),
            has_jar: true,
            sources: vec![tm_exthost::model::AvailableSource {
                id: source,
                name: name.into(),
                lang: "en".into(),
                base_url: String::new(),
            }],
        }
    }

    #[test]
    fn restore_plans_extensions_from_the_list_or_the_sources() {
        let repo = "https://example.com/index.min.json".to_owned();
        let catalog = vec![
            (repo.clone(), avail("pkg.a", "Alpha", 1)),
            (repo.clone(), avail("pkg.b", "Beta", 2)),
            (repo.clone(), avail("pkg.c", "Gamma", 3)),
        ];
        let installed: std::collections::HashSet<String> = ["pkg.b".to_owned()].into();

        // A backup with tag 900 asks for exactly its packages; missing ones have no repo.
        let wanted = vec![
            BackupExtension {
                pkg_name: "pkg.b".into(),
                ..Default::default()
            },
            BackupExtension {
                pkg_name: "pkg.gone".into(),
                name: Some("Gone".into()),
                ..Default::default()
            },
        ];
        let p = plan_extensions(&wanted, &[1, 3], &installed, &catalog);
        let got: Vec<_> = p
            .iter()
            .map(|e| (e.name.as_str(), e.repo_url.is_some(), e.installed))
            .collect();
        assert_eq!(got, vec![("Beta", true, true), ("Gone", false, false)]);

        // A stock Mihon backup: the extensions providing its sources.
        let p = plan_extensions(&[], &[1, 3, 99], &installed, &catalog);
        let names: Vec<_> = p.iter().map(|e| e.pkg_name.as_str()).collect();
        assert_eq!(names, vec!["pkg.a", "pkg.c"]);
    }

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
        assert_eq!(r.summary.manga_added, 0);
        assert_eq!(b.db.extension_repos().unwrap().len(), 1, "repos are added");
        assert_eq!(offer_of(&b).repos_added.len(), 1);
    }
}
