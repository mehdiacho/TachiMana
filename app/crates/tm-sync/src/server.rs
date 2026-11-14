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
use crate::queue::{Outbox, OutboxList};
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
    /// Done events for the linked devices, served by `GET /outbox`.
    pub outbox: Arc<Outbox>,
    /// The change journal and how to apply an entry (`changes/1`); `None` serves neither.
    pub changes: Option<Changes>,
}

/// Applies a received entry's winning value to the library (blocking; it writes the database).
pub type ApplyFn = dyn Fn(&crate::journal::Entry, &serde_json::Value) + Send + Sync;

/// What `GET /changes` and `POST /changes` work on.
#[derive(Clone)]
pub struct Changes {
    pub journal: Arc<crate::journal::Journal>,
    pub apply: Arc<ApplyFn>,
    /// Called after a `POST /changes` applied something, so the UI can refresh.
    pub changed: Arc<dyn Fn() + Send + Sync>,
}

/// Largest `POST /changes` body read.
const MAX_CHANGES_BODY: usize = 16 * 1024 * 1024;

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
            .route("/outbox", get(outbox_route))
            .route("/changes", get(changes_get).post(changes_post))
            .route("/roles", get(roles_get).put(roles_put))
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
            drop(failures);
            l.links.touch(&link.peer_device_id);
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

#[derive(Deserialize)]
struct OutboxQuery {
    since: Option<String>,
}

/// The link a request's key belongs to; `Err` is the answer to send instead.
fn linked(
    s: &Arc<ServerState>,
    headers: &HeaderMap,
) -> std::result::Result<(Linking, Link), Box<Response>> {
    let Some(linking) = s.linking.clone() else {
        return Err(Box::new(
            (StatusCode::UNAUTHORIZED, "needs a link key").into_response(),
        ));
    };
    if headers.get(LINK_HEADER).is_none() {
        return Err(Box::new(
            (StatusCode::UNAUTHORIZED, "needs a link key").into_response(),
        ));
    }
    match check_as(s, headers, true) {
        Ok(Some(link)) => Ok((linking, link)),
        Ok(None) => Err(Box::new(
            (StatusCode::UNAUTHORIZED, "needs a link key").into_response(),
        )),
        Err(r) => Err(r),
    }
}

fn error_body(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(serde_json::json!({"error": code, "message": message})),
    )
        .into_response()
}

#[derive(Deserialize)]
struct ChangesQuery {
    after: Option<i64>,
    limit: Option<usize>,
}

/// `GET /changes?after=<seq>&limit=<n>`: this device's journal after the caller's cursor, without
/// the caller's own entries (`changes/1`).
async fn changes_get(
    State(s): State<Arc<ServerState>>,
    headers: HeaderMap,
    Query(q): Query<ChangesQuery>,
) -> Response {
    let (linking, link) = match linked(&s, &headers) {
        Ok(x) => x,
        Err(r) => return *r,
    };
    let Some(ch) = linking.changes else {
        return (StatusCode::NOT_FOUND, "no changes/1 here").into_response();
    };
    let after = q.after.unwrap_or(0).max(0);
    let limit = q.limit.unwrap_or(crate::journal::DEFAULT_LIMIT);
    let peer = link.peer_device_id.clone();
    let page = tokio::task::spawn_blocking(move || ch.journal.after(after, &peer, limit)).await;
    match page {
        Ok(Ok(mut p)) => {
            // A device that leads gets nothing from its follower's side of the link: the
            // follower's changes stay on it (D2). Its cursor still moves.
            if !link.mode.sends_mine() {
                p.changes.clear();
            }
            Json(p).into_response()
        }
        Ok(Err(crate::journal::JournalError::CursorTooOld { oldest })) => (
            StatusCode::GONE,
            Json(serde_json::json!({"error": "cursor-too-old", "oldest": oldest})),
        )
            .into_response(),
        Ok(Err(e)) => error_body(StatusCode::INTERNAL_SERVER_ERROR, "journal", &e.to_string()),
        Err(e) => error_body(StatusCode::INTERNAL_SERVER_ERROR, "journal", &e.to_string()),
    }
}

#[derive(Deserialize)]
struct ChangesBody {
    changes: Vec<crate::journal::Entry>,
}

/// `POST /changes`: entries the caller made or applied, to take now (`changes/1`).
async fn changes_post(
    State(s): State<Arc<ServerState>>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let (linking, link) = match linked(&s, &headers) {
        Ok(x) => x,
        Err(r) => return *r,
    };
    let Some(ch) = linking.changes else {
        return (StatusCode::NOT_FOUND, "no changes/1 here").into_response();
    };
    if body.len() > MAX_CHANGES_BODY {
        return error_body(
            StatusCode::PAYLOAD_TOO_LARGE,
            "too-large",
            "send fewer entries at once",
        );
    }
    let parsed: ChangesBody = match serde_json::from_slice(&body) {
        Ok(b) => b,
        Err(e) => return error_body(StatusCode::BAD_REQUEST, "bad-body", &e.to_string()),
    };
    if parsed.changes.len() > crate::journal::MAX_LIMIT {
        return error_body(
            StatusCode::PAYLOAD_TOO_LARGE,
            "too-large",
            "at most 2000 entries",
        );
    }
    let total = parsed.changes.len();
    // This device leads: the follower's changes are not taken (D2).
    if !link.mode.takes_theirs() {
        return Json(serde_json::json!({"applied": 0, "ignored": total})).into_response();
    }
    let done = tokio::task::spawn_blocking(move || {
        let mut applied = 0usize;
        for e in &parsed.changes {
            match ch.journal.receive(e) {
                Ok(Some(v)) => {
                    (ch.apply)(e, &v);
                    applied += 1;
                }
                Ok(None) => {}
                Err(err) => tracing::warn!("journal: {err}"),
            }
        }
        if applied > 0 {
            (ch.changed)();
        }
        applied
    })
    .await
    .unwrap_or(0);
    Json(serde_json::json!({"applied": done, "ignored": total.saturating_sub(done)}))
        .into_response()
}

/// `GET /roles`: who leads on this link, named from this side as the server (`roles/1`).
async fn roles_get(State(s): State<Arc<ServerState>>, headers: HeaderMap) -> Response {
    let (_, link) = match linked(&s, &headers) {
        Ok(x) => x,
        Err(r) => return *r,
    };
    Json(crate::roles::RolesWire {
        mode: link.mode.as_server().into(),
        mode_at: link.mode_at,
    })
    .into_response()
}

/// `PUT /roles`: the caller changed who leads; kept when it is newer than what is stored.
async fn roles_put(
    State(s): State<Arc<ServerState>>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let (linking, link) = match linked(&s, &headers) {
        Ok(x) => x,
        Err(r) => return *r,
    };
    if body.len() > MAX_LINK_BODY {
        return error_body(StatusCode::PAYLOAD_TOO_LARGE, "too-large", "");
    }
    let Ok(want) = serde_json::from_slice::<crate::roles::RolesWire>(&body) else {
        return error_body(
            StatusCode::BAD_REQUEST,
            "bad-body",
            "expected mode and modeAt",
        );
    };
    let Some(mode) = crate::roles::Mode::from_request(&want.mode) else {
        return error_body(
            StatusCode::BAD_REQUEST,
            "bad-mode",
            "mode is both, callerLeads or serverLeads",
        );
    };
    let links = Arc::clone(&linking.links);
    let peer = link.peer_device_id.clone();
    let at = want.mode_at;
    let saved = tokio::task::spawn_blocking(move || links.set_mode(&peer, mode, at)).await;
    match saved {
        Ok(Ok(Some((l, changed)))) => {
            let body = Json(crate::roles::RolesWire {
                mode: l.mode.as_server().into(),
                mode_at: l.mode_at,
            });
            if changed {
                (StatusCode::OK, body).into_response()
            } else {
                (StatusCode::CONFLICT, body).into_response()
            }
        }
        Ok(Ok(None)) => (StatusCode::UNAUTHORIZED, "unknown link").into_response(),
        Ok(Err(e)) => error_body(StatusCode::INTERNAL_SERVER_ERROR, "save", &e.to_string()),
        Err(e) => error_body(StatusCode::INTERNAL_SERVER_ERROR, "save", &e.to_string()),
    }
}

/// `GET /outbox?since=<eventId>`: the done events for the asking linked device, after `since`.
async fn outbox_route(
    State(s): State<Arc<ServerState>>,
    headers: HeaderMap,
    Query(q): Query<OutboxQuery>,
) -> Response {
    let Some(linking) = s.linking.as_ref() else {
        return (StatusCode::UNAUTHORIZED, "needs a link key").into_response();
    };
    if headers.get(LINK_HEADER).is_none() {
        return (StatusCode::UNAUTHORIZED, "needs a link key").into_response();
    }
    let link = match check_as(&s, &headers, true) {
        Ok(Some(link)) => link,
        Ok(None) => return (StatusCode::UNAUTHORIZED, "needs a link key").into_response(),
        Err(r) => return *r,
    };
    let since = q
        .since
        .as_deref()
        .filter(|v| !v.is_empty() && v.len() <= 64);
    Json(OutboxList {
        events: linking.outbox.after(&link.peer_device_id, since),
    })
    .into_response()
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
        last_seen: now_ms(),
        mode: crate::roles::Mode::Both,
        mode_at: 0,
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
