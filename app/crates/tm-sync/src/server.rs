//! The LAN endpoint that serves this library to paired peers (`LanSyncServer.kt`).

use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::{ConnectInfo, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use crate::links::{Link, LinkStore};
use crate::protocol::{
    resolve_manifest_path, LinkRequest, LinkResponse, ManifestFile, PingInfo, SyncManifest,
    DEFAULT_PORT, LINK_HEADER, PAIR_HEADER, PORT_ATTEMPTS,
};
use crate::{DeviceIdentity, Result, SyncError};

/// Builds the `.tachibk` bytes on demand (blocking; it reads the database).
pub type BackupFn = dyn Fn() -> std::result::Result<Vec<u8>, String> + Send + Sync;

/// Wrong pairing codes allowed per minute before the server stops answering for a while.
/// A six-digit code is only safe against guessing if guessing is slow.
const MAX_FAILURES_PER_MINUTE: usize = 10;

/// Largest `POST /link` body read.
const MAX_LINK_BODY: usize = 4096;

/// The profile this server serves and the devices linked to it. Without it the server speaks
/// only the six-digit code, as before profiles.
#[derive(Clone)]
pub struct Linking {
    pub profile_id: String,
    pub profile_name: String,
    pub links: Arc<LinkStore>,
    /// The last device that asked to link from another profile, so this side can offer to take
    /// on that profile. Only the newest is kept.
    pub refused: Arc<Mutex<Option<RefusedLink>>>,
}

/// A `POST /link` with the right code but another profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefusedLink {
    pub peer_device_id: String,
    pub peer_name: String,
    pub profile_id: String,
    pub profile_name: String,
    pub platform: String,
    /// `host:port` the asking device serves on.
    pub address: String,
    /// Unix milliseconds.
    pub at: i64,
}

struct ServerState {
    identity: DeviceIdentity,
    local_root: PathBuf,
    backup: Arc<BackupFn>,
    linking: Option<Linking>,
    failures: Mutex<Vec<Instant>>,
}

/// A running server. Dropping it stops it.
pub struct SyncServer {
    port: u16,
    stop: Option<oneshot::Sender<()>>,
}

impl SyncServer {
    /// Bind the first free port from [`DEFAULT_PORT`] (or `preferred`) on all interfaces and
    /// start serving on the current tokio runtime.
    pub async fn start(
        identity: DeviceIdentity,
        local_root: PathBuf,
        backup: Arc<BackupFn>,
        preferred: Option<u16>,
    ) -> Result<Self> {
        Self::start_with(identity, local_root, backup, preferred, None).await
    }

    /// [`start`](Self::start), also answering `POST /link` and accepting link keys.
    pub async fn start_with(
        identity: DeviceIdentity,
        local_root: PathBuf,
        backup: Arc<BackupFn>,
        preferred: Option<u16>,
        linking: Option<Linking>,
    ) -> Result<Self> {
        let first = preferred.unwrap_or(DEFAULT_PORT);
        let listener = bind_from(first).await?;
        let port = listener.local_addr().map_err(SyncError::Io)?.port();
        let state = Arc::new(ServerState {
            identity,
            local_root,
            backup,
            linking,
            failures: Mutex::new(Vec::new()),
        });
        let app = Router::new()
            .route("/ping", get(ping))
            .route("/manifest", get(manifest))
            .route("/backup", get(backup_route))
            .route("/file", get(file))
            .route("/link", post(link_route))
            .with_state(state);
        let (tx, rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            let served = axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(async move {
                let _ = rx.await;
            })
            .await;
            if let Err(e) = served {
                tracing::warn!("sync server stopped: {e}");
            }
        });
        Ok(Self {
            port,
            stop: Some(tx),
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn stop(&mut self) {
        if let Some(tx) = self.stop.take() {
            let _ = tx.send(());
        }
    }
}

impl Drop for SyncServer {
    fn drop(&mut self) {
        self.stop();
    }
}

async fn bind_from(first: u16) -> Result<TcpListener> {
    if first == 0 {
        return TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0))
            .await
            .map_err(SyncError::Io);
    }
    let mut last = None;
    for i in 0..PORT_ATTEMPTS {
        let Some(port) = first.checked_add(i) else {
            break;
        };
        match TcpListener::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, port))).await {
            Ok(l) => return Ok(l),
            Err(e) => last = Some(e),
        }
    }
    Err(SyncError::Bind(format!(
        "no free sync port in {first}..{}: {}",
        first.saturating_add(PORT_ATTEMPTS),
        last.map(|e| e.to_string()).unwrap_or_default()
    )))
}

fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// `None` when paired by code or by link key; otherwise the response to send.
fn check(state: &ServerState, headers: &HeaderMap) -> Option<Response> {
    check_as(state, headers, true).err().map(|r| *r)
}

/// Who is asking: `Ok(Some(link))` for a link key, `Ok(None)` for the pairing code. A link key
/// is tried only when `keys` is set. Every wrong code or key counts towards the lockout.
fn check_as(
    state: &ServerState,
    headers: &HeaderMap,
    keys: bool,
) -> std::result::Result<Option<Link>, Box<Response>> {
    let now = Instant::now();
    let mut failures = match state.failures.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    failures.retain(|t| now.duration_since(*t) < Duration::from_secs(60));
    if failures.len() >= MAX_FAILURES_PER_MINUTE {
        return Err(Box::new(
            (StatusCode::TOO_MANY_REQUESTS, "too many attempts").into_response(),
        ));
    }
    if let (true, Some(l), Some(key)) = (
        keys,
        state.linking.as_ref(),
        headers.get(LINK_HEADER).and_then(|v| v.to_str().ok()),
    ) {
        if let Some(link) = l.links.by_key(key) {
            return Ok(Some(link));
        }
        failures.push(now);
        return Err(Box::new(
            (StatusCode::UNAUTHORIZED, "unknown link").into_response(),
        ));
    }
    let given = headers
        .get(PAIR_HEADER)
        .map(|v| v.as_bytes())
        .unwrap_or_default();
    if same(given, state.identity.pairing_code.as_bytes()) {
        return Ok(None);
    }
    failures.push(now);
    Err(Box::new(
        (StatusCode::UNAUTHORIZED, "unauthorized").into_response(),
    ))
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// `POST /link`: with the right pairing code, makes a key for the asking device and keeps the
/// link. A device of another profile gets 409 and this side's profile, and no key.
async fn link_route(
    State(s): State<Arc<ServerState>>,
    ConnectInfo(from): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let Some(linking) = s.linking.clone() else {
        return (StatusCode::NOT_FOUND, "linking is off").into_response();
    };
    if let Err(r) = check_as(&s, &headers, false) {
        return *r;
    }
    let Ok(bytes) = axum::body::to_bytes(body, MAX_LINK_BODY).await else {
        return (StatusCode::PAYLOAD_TOO_LARGE, "too large").into_response();
    };
    let Ok(req) = serde_json::from_slice::<LinkRequest>(&bytes) else {
        return (StatusCode::BAD_REQUEST, "bad link request").into_response();
    };
    if !req.is_valid() || req.device_id == s.identity.id {
        return (StatusCode::BAD_REQUEST, "bad link request").into_response();
    }
    let mut answer = LinkResponse {
        device_id: s.identity.id.clone(),
        device_name: s.identity.name.clone(),
        profile_id: linking.profile_id.clone(),
        profile_name: linking.profile_name.clone(),
        key: None,
    };
    let port = req.port.filter(|p| *p != 0).unwrap_or(DEFAULT_PORT);
    if req.profile_id != linking.profile_id {
        let refused = RefusedLink {
            peer_device_id: req.device_id,
            peer_name: req.device_name,
            profile_id: req.profile_id,
            profile_name: req.profile_name,
            platform: req.platform,
            address: SocketAddr::new(from.ip(), port).to_string(),
            at: now_ms(),
        };
        *linking
            .refused
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(refused);
        return (StatusCode::CONFLICT, Json(answer)).into_response();
    }
    let Some(key) = crate::links::new_key() else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "no randomness").into_response();
    };
    let link = Link {
        peer_device_id: req.device_id,
        peer_name: req.device_name,
        profile_id: req.profile_id,
        platform: req.platform,
        key: key.clone(),
        last_address: SocketAddr::new(from.ip(), port).to_string(),
        linked_at: now_ms(),
    };
    let peer = link.peer_device_id.clone();
    let links = Arc::clone(&linking.links);
    let saved = tokio::task::spawn_blocking(move || links.put(link)).await;
    match saved {
        Ok(Ok(())) => {
            tracing::info!(peer = %peer, "device linked");
            answer.key = Some(key);
            Json(answer).into_response()
        }
        Ok(Err(e)) => {
            tracing::warn!("couldn't save the link: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "couldn't save the link").into_response()
        }
        Err(e) => {
            tracing::warn!("link task failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "couldn't save the link").into_response()
        }
    }
}

async fn ping(State(s): State<Arc<ServerState>>) -> Json<PingInfo> {
    Json(PingInfo {
        device_id: s.identity.id.clone(),
        device_name: s.identity.name.clone(),
        profile_id: s.linking.as_ref().map(|l| l.profile_id.clone()),
        profile_name: s.linking.as_ref().map(|l| l.profile_name.clone()),
    })
}

/// The backup bytes, or `None` after logging why it failed (answer with [`backup_failed`]).
async fn build_backup(s: &Arc<ServerState>) -> Option<Vec<u8>> {
    let f = Arc::clone(&s.backup);
    match tokio::task::spawn_blocking(move || f()).await {
        Ok(Ok(bytes)) => Some(bytes),
        Ok(Err(e)) => {
            tracing::warn!("sync backup failed: {e}");
            None
        }
        Err(e) => {
            tracing::warn!("sync backup task failed: {e}");
            None
        }
    }
}

fn backup_failed() -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, "backup failed").into_response()
}

async fn manifest(State(s): State<Arc<ServerState>>, headers: HeaderMap) -> Response {
    if let Some(r) = check(&s, &headers) {
        return r;
    }
    let Some(bytes) = build_backup(&s).await else {
        return backup_failed();
    };
    let root = s.local_root.clone();
    let files = tokio::task::spawn_blocking(move || list_local_files(&root))
        .await
        .unwrap_or_default();
    Json(SyncManifest {
        device_id: s.identity.id.clone(),
        device_name: s.identity.name.clone(),
        backup_size: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        files,
    })
    .into_response()
}

async fn backup_route(State(s): State<Arc<ServerState>>, headers: HeaderMap) -> Response {
    if let Some(r) = check(&s, &headers) {
        return r;
    }
    match build_backup(&s).await {
        Some(bytes) => {
            ([(header::CONTENT_TYPE, "application/octet-stream")], bytes).into_response()
        }
        None => backup_failed(),
    }
}

#[derive(Deserialize)]
struct FileQuery {
    path: Option<String>,
}

async fn file(
    State(s): State<Arc<ServerState>>,
    headers: HeaderMap,
    Query(q): Query<FileQuery>,
) -> Response {
    if let Some(r) = check(&s, &headers) {
        return r;
    }
    let Some(path) = q.path else {
        return (StatusCode::BAD_REQUEST, "missing path").into_response();
    };
    let Some(target) = servable(&s.local_root, &path) else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };
    match tokio::fs::File::open(&target).await {
        Ok(f) => {
            let len = f.metadata().await.map(|m| m.len()).ok();
            let body = Body::from_stream(tokio_util::io::ReaderStream::new(f));
            let mut resp =
                ([(header::CONTENT_TYPE, "application/octet-stream")], body).into_response();
            if let Some(len) = len.and_then(|l| header::HeaderValue::from_str(&l.to_string()).ok())
            {
                resp.headers_mut().insert(header::CONTENT_LENGTH, len);
            }
            resp
        }
        Err(_) => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

/// The file a manifest path names, if it is a regular file really inside the library root
/// (symlinks out of the root are refused).
fn servable(root: &Path, path: &str) -> Option<PathBuf> {
    let target = resolve_manifest_path(root, path)?;
    let root = root.canonicalize().ok()?;
    let real = target.canonicalize().ok()?;
    (real.starts_with(&root) && real.is_file()).then_some(real)
}

fn mtime(p: &Path) -> i64 {
    tm_local::mtime_ms(p)
}

/// Every servable file: `<manga dir>/<file>`, hidden entries skipped, sorted by path.
pub fn list_local_files(root: &Path) -> Vec<ManifestFile> {
    let mut out = Vec::new();
    let Ok(dirs) = std::fs::read_dir(root) else {
        return out;
    };
    for d in dirs.flatten() {
        let dname = d.file_name().to_string_lossy().into_owned();
        if dname.starts_with('.') || !d.path().is_dir() {
            continue;
        }
        let Ok(files) = std::fs::read_dir(d.path()) else {
            continue;
        };
        for f in files.flatten() {
            let fname = f.file_name().to_string_lossy().into_owned();
            let p = f.path();
            if fname.starts_with('.') || !p.is_file() {
                continue;
            }
            let rel = format!("{dname}/{fname}");
            if resolve_manifest_path(root, &rel).is_none() {
                continue;
            }
            out.push(ManifestFile {
                path: rel,
                size: f.metadata().map(|m| m.len()).unwrap_or(0),
                last_modified: mtime(&p),
            });
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}
