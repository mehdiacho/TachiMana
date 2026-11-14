//! `changes/1` in the app (`docs/protocol/capabilities/changes-1.md`): every change the person
//! makes is written into this profile's journal, entries from linked devices are applied to the
//! library, and linked devices are pulled from and pushed to in the background.
//!
//! Who leads (`roles/1`) decides which way entries travel: a device that follows still journals
//! its own changes but never sends them, and a device that leads never takes its follower's.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};
use tm_db::{Db, Manga, SourceManga};
use tm_sync::client::Pulled;
use tm_sync::journal::{self as j, Entry, Journal};
use tm_sync::roles::Mode;
use tm_sync::{Link, PeerDevice, SyncClient};
use ts_rs::TS;

use crate::dto::CmdError;
use crate::state::AppState;

type R<T> = Result<T, CmdError>;

/// How long after a change the push waits, so a burst (a bulk action) goes in one request.
const SETTLE: Duration = Duration::from_secs(2);
/// How often linked devices are pulled from while the app runs.
const EVERY: Duration = Duration::from_secs(5 * 60);
/// A round woken by a change pulls from a device at most this often (it always pushes).
const PULL_GAP_MS: i64 = 60_000;
/// Entries asked for or sent per request.
const PAGE: usize = 500;
/// Pages taken from one device in one round, so a huge journal cannot hold the loop forever.
const MAX_PAGES: usize = 200;

/// The journal and the background sync's state, held in [`crate::sync::Sync`].
pub struct ChangeLog {
    /// `None` when the journal file could not be opened; changes then only travel by full syncs.
    pub journal: Option<Arc<Journal>>,
    wake: Arc<tokio::sync::Notify>,
    running: AtomicBool,
    last: Mutex<Option<RoundDto>>,
    /// When each linked device was last pulled from (ms), so pushes after each change do not
    /// pull every time.
    pulled_at: Mutex<std::collections::HashMap<String, i64>>,
}

impl ChangeLog {
    pub fn open(data_dir: &std::path::Path, device_id: &str) -> Self {
        let journal = match Journal::open(&data_dir.join("journal.db"), device_id) {
            Ok(jr) => {
                if let Err(e) = jr.prune(j::now_ms().saturating_sub(j::KEEP_MS)) {
                    tracing::warn!("journal prune: {e}");
                }
                Some(Arc::new(jr))
            }
            Err(e) => {
                tracing::warn!("change journal unavailable: {e}");
                None
            }
        };
        Self {
            journal,
            wake: Arc::new(tokio::sync::Notify::new()),
            running: AtomicBool::new(false),
            last: Mutex::new(None),
            pulled_at: Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Asks the background loop to pull and push soon.
    pub fn nudge(&self) {
        self.wake.notify_one();
    }

    fn write(&self, kind: &str, target: Value, value: Value) {
        let Some(jr) = &self.journal else { return };
        match jr.record(kind, target, value) {
            Ok(_) => self.nudge(),
            Err(e) => tracing::warn!("journal: {e}"),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Targets
// ---------------------------------------------------------------------------------------------

fn manga_target(m: &Manga) -> Value {
    json!({"sourceId": m.source.to_string(), "mangaUrl": m.url})
}

fn chapter_target(m: &Manga, chapter_url: &str) -> Value {
    json!({"sourceId": m.source.to_string(), "mangaUrl": m.url, "chapterUrl": chapter_url})
}

fn source_of(target: &Value) -> Option<i64> {
    match target.get("sourceId")? {
        Value::String(s) => s
            .parse::<i64>()
            .ok()
            // A source id written unsigned (above i64::MAX) is the same 64 bits.
            .or_else(|| {
                s.parse::<u64>()
                    .ok()
                    .map(|u| i64::from_ne_bytes(u.to_ne_bytes()))
            }),
        Value::Number(n) => n.as_i64(),
        _ => None,
    }
}

fn str_of<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn genres_of(m: &Manga) -> Vec<String> {
    m.genre
        .as_deref()
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|g| !g.is_empty())
        .map(str::to_owned)
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Writing down what the person did here
// ---------------------------------------------------------------------------------------------

/// The manga went into or out of the library.
pub fn library(state: &AppState, manga_id: i64) {
    let Ok(Some(m)) = state.db.manga(manga_id) else {
        return;
    };
    let value = if m.favorite {
        json!({
            "inLibrary": true,
            "dateAdded": m.date_added.unwrap_or(0),
            "manga": {
                "title": m.title,
                "author": m.author,
                "artist": m.artist,
                "description": m.description,
                "genres": genres_of(&m),
                "status": m.status,
                "thumbnailUrl": m.thumbnail_url,
            },
        })
    } else {
        json!({"inLibrary": false})
    };
    state
        .sync
        .changes
        .write(j::MANGA_LIBRARY, manga_target(&m), value);
}

/// The manga's categories were set.
pub fn manga_categories(state: &AppState, manga_id: i64) {
    let (Ok(Some(m)), Ok(cats), Ok(assigned)) = (
        state.db.manga(manga_id),
        state.db.categories(),
        state.db.category_assignments(),
    ) else {
        return;
    };
    let ids = assigned.get(&manga_id).cloned().unwrap_or_default();
    let names: Vec<&str> = cats
        .iter()
        .filter(|c| ids.contains(&c.id))
        .map(|c| c.name.as_str())
        .collect();
    state.sync.changes.write(
        j::MANGA_CATEGORIES,
        manga_target(&m),
        json!({"categories": names}),
    );
}

/// Categories were created or moved: the order of each, as it is now.
pub fn category_order(state: &AppState, only: Option<i64>) {
    let Ok(cats) = state.db.categories() else {
        return;
    };
    for c in cats.iter().filter(|c| only.is_none_or(|id| id == c.id)) {
        state.sync.changes.write(
            j::CATEGORY_SET,
            json!({"name": c.name}),
            json!({"order": c.sort}),
        );
    }
}

pub fn category_renamed(state: &AppState, old: &str, new: &str) {
    if old != new {
        state.sync.changes.write(
            j::CATEGORY_RENAME,
            json!({"name": old}),
            json!({"name": new}),
        );
    }
}

pub fn category_deleted(state: &AppState, name: &str) {
    state
        .sync
        .changes
        .write(j::CATEGORY_DELETE, json!({"name": name}), json!({}));
}

/// The chapter's read mark or page changed.
pub fn chapter_read(state: &AppState, chapter_id: i64) {
    let Ok(Some(ch)) = state.db.chapter(chapter_id) else {
        return;
    };
    let Ok(Some(m)) = state.db.manga(ch.manga_id) else {
        return;
    };
    state.sync.changes.write(
        j::CHAPTER_READ,
        chapter_target(&m, &ch.url),
        json!({"read": ch.read, "lastPageRead": ch.last_page_read}),
    );
}

/// The chapter was bookmarked or unbookmarked.
pub fn chapter_bookmark(state: &AppState, chapter_id: i64) {
    let Ok(Some(ch)) = state.db.chapter(chapter_id) else {
        return;
    };
    let Ok(Some(m)) = state.db.manga(ch.manga_id) else {
        return;
    };
    state.sync.changes.write(
        j::CHAPTER_BOOKMARK,
        chapter_target(&m, &ch.url),
        json!({"bookmark": ch.bookmark}),
    );
}

/// The chapter was read now. Reading time is not measured on the computer, so it adds none.
pub fn history(state: &AppState, chapter_id: i64) {
    let Ok(Some(ch)) = state.db.chapter(chapter_id) else {
        return;
    };
    let Ok(Some(m)) = state.db.manga(ch.manga_id) else {
        return;
    };
    state.sync.changes.write(
        j::HISTORY_READ,
        chapter_target(&m, &ch.url),
        json!({"lastRead": j::now_ms(), "timeRead": 0}),
    );
}

pub fn viewer(state: &AppState, manga_id: i64) {
    let Ok(Some(m)) = state.db.manga(manga_id) else {
        return;
    };
    state.sync.changes.write(
        j::MANGA_VIEWER,
        manga_target(&m),
        json!({"viewer": m.viewer}),
    );
}

// ---------------------------------------------------------------------------------------------
// Applying a linked device's entries
// ---------------------------------------------------------------------------------------------

fn find_manga(db: &Db, target: &Value) -> Option<Manga> {
    let source = source_of(target)?;
    let url = str_of(target, "mangaUrl")?;
    db.manga_by_url(url, source).ok().flatten()
}

fn find_chapter(db: &Db, target: &Value) -> Option<tm_db::Chapter> {
    let m = find_manga(db, target)?;
    let url = str_of(target, "chapterUrl")?;
    db.chapters(m.id).ok()?.into_iter().find(|c| c.url == url)
}

fn category_id(db: &Db, name: &str) -> Option<i64> {
    let lower = name.to_lowercase();
    db.categories()
        .ok()?
        .into_iter()
        .find(|c| c.name.to_lowercase() == lower)
        .map(|c| c.id)
}

/// Applies an entry that won (`value` is what to hold now) to the library. Entries for manga or
/// chapters this device does not have are dropped; the next refresh from the source or full sync
/// brings those.
pub fn apply(db: &Db, e: &Entry, value: &Value) -> Result<(), tm_db::DbError> {
    let t = &e.target;
    match e.kind.as_str() {
        j::MANGA_LIBRARY => apply_library(db, t, value)?,
        j::MANGA_CATEGORIES => {
            let Some(m) = find_manga(db, t) else {
                return Ok(());
            };
            let mut ids = BTreeSet::new();
            for name in value
                .get("categories")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                if let Some(id) = db.create_category(name)? {
                    ids.insert(id);
                }
            }
            db.set_manga_categories(m.id, &ids)?;
        }
        j::CATEGORY_SET => {
            let Some(name) = str_of(t, "name") else {
                return Ok(());
            };
            if let Some(id) = db.create_category(name)? {
                if let Some(order) = value.get("order").and_then(Value::as_i64) {
                    db.set_category_sort(id, order)?;
                }
            }
        }
        j::CATEGORY_RENAME => {
            let (Some(old), Some(new)) = (str_of(t, "name"), str_of(value, "name")) else {
                return Ok(());
            };
            if let Some(id) = category_id(db, old) {
                match category_id(db, new) {
                    // Same category, another spelling of it.
                    Some(other) if other == id => db.rename_category(id, new)?,
                    // A category with the new name exists: move the manga across.
                    Some(other) => {
                        let assigned = db.category_assignments()?;
                        for (manga, cats) in assigned.into_iter().filter(|(_, c)| c.contains(&id)) {
                            let mut next = cats;
                            next.remove(&id);
                            next.insert(other);
                            db.set_manga_categories(manga, &next)?;
                        }
                        db.delete_category(id)?;
                    }
                    None => db.rename_category(id, new)?,
                }
            }
        }
        j::CATEGORY_DELETE => {
            if let Some(id) = str_of(t, "name").and_then(|n| category_id(db, n)) {
                db.delete_category(id)?;
            }
        }
        j::CHAPTER_READ => {
            let Some(ch) = find_chapter(db, t) else {
                return Ok(());
            };
            let read = value
                .get("read")
                .and_then(Value::as_bool)
                .unwrap_or(ch.read);
            let page = value
                .get("lastPageRead")
                .and_then(Value::as_i64)
                .unwrap_or(ch.last_page_read)
                .max(0);
            db.mark_progress(ch.id, page, ch.pages_left, read)?;
        }
        j::CHAPTER_BOOKMARK => {
            if let (Some(ch), Some(b)) = (
                find_chapter(db, t),
                value.get("bookmark").and_then(Value::as_bool),
            ) {
                db.set_bookmark(ch.id, b)?;
            }
        }
        j::HISTORY_READ => {
            if let Some(ch) = find_chapter(db, t) {
                let last = value.get("lastRead").and_then(Value::as_i64).unwrap_or(0);
                // The held value is the running total; the library adds this entry's own time.
                let added = e.value.get("timeRead").and_then(Value::as_i64).unwrap_or(0);
                db.add_history(ch.id, last, added)?;
            }
        }
        j::MANGA_VIEWER => {
            if let (Some(m), Some(v)) = (
                find_manga(db, t),
                value.get("viewer").and_then(Value::as_i64),
            ) {
                db.set_viewer(m.id, v)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn apply_library(db: &Db, t: &Value, value: &Value) -> Result<(), tm_db::DbError> {
    let in_library = value
        .get("inLibrary")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let existing = find_manga(db, t);
    if !in_library {
        if let Some(m) = existing.filter(|m| m.favorite) {
            db.set_favorite(m.id, false)?;
        }
        return Ok(());
    }
    let id = match existing {
        Some(m) => m.id,
        None => {
            let (Some(source), Some(url)) = (source_of(t), str_of(t, "mangaUrl")) else {
                return Ok(());
            };
            let d = value.get("manga").cloned().unwrap_or(Value::Null);
            let text = |k: &str| str_of(&d, k).map(str::to_owned).filter(|s| !s.is_empty());
            let genres: Vec<&str> = d
                .get("genres")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            let manga = SourceManga {
                url: url.to_owned(),
                title: text("title").unwrap_or_else(|| url.to_owned()),
                artist: text("artist"),
                author: text("author"),
                description: text("description"),
                genre: (!genres.is_empty()).then(|| genres.join(", ")),
                status: d.get("status").and_then(Value::as_i64).unwrap_or(0),
                thumbnail_url: text("thumbnailUrl"),
                initialized: false,
            };
            db.upsert_online(source, &manga, &[])?.0
        }
    };
    db.set_favorite(id, true)?;
    if let Some(date) = value
        .get("dateAdded")
        .and_then(Value::as_i64)
        .filter(|d| *d > 0)
    {
        db.update_manga(
            id,
            &tm_db::MangaUpdate {
                date_added: Some(date),
                ..Default::default()
            },
        )?;
    }
    Ok(())
}

/// Takes entries from a linked device: each one that wins is applied. Returns how many were.
fn take(db: &Db, jr: &Journal, entries: &[Entry]) -> usize {
    let mut applied = 0;
    for e in entries {
        // Never re-apply this device's own entries handed back by a third device.
        if e.device_id == jr.device_id() {
            continue;
        }
        match jr.receive(e) {
            Ok(Some(v)) => match apply(db, e, &v) {
                Ok(()) => applied += 1,
                Err(err) => tracing::warn!(kind = %e.kind, "applying a change: {err}"),
            },
            Ok(None) => {}
            Err(err) => tracing::warn!("journal: {err}"),
        }
    }
    applied
}

// ---------------------------------------------------------------------------------------------
// Pulling and pushing
// ---------------------------------------------------------------------------------------------

/// The result of the last round with linked devices, for the Sync screen.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RoundDto {
    #[ts(type = "number")]
    pub at: i64,
    pub received: u32,
    pub sent: u32,
    /// Linked devices that could not be reached or answered with an error, by name.
    pub missed: Vec<String>,
}

pub(crate) fn peer_of(state: &AppState, link: &Link) -> Option<PeerDevice> {
    if let Some(p) = state.sync.peer(&link.peer_device_id) {
        return Some(p);
    }
    let (host, port) = link.last_address.rsplit_once(':')?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    Some(PeerDevice {
        id: link.peer_device_id.clone(),
        name: link.peer_name.clone(),
        host: host.to_owned(),
        port: port.parse().ok()?,
    })
}

#[derive(Default)]
struct PeerOutcome {
    received: usize,
    sent: usize,
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    tauri::async_runtime::spawn_blocking(f).await.ok()
}

/// One round with one linked device: agree on who leads, take its entries, send ours.
async fn with_peer(
    app: &AppHandle,
    client: &SyncClient,
    jr: &Arc<Journal>,
    link: Link,
    pull: bool,
) -> Result<PeerOutcome, String> {
    let state = app.state::<AppState>();
    let peer = peer_of(&state, &link).ok_or("no address")?;
    let mut mode = link.mode;
    // A mode changed here while the other device was away reaches it now; a newer one there wins.
    if link.mode_at > 0 {
        if let Ok(Some((m, at))) = client.put_roles(&peer, &link.key, mode, link.mode_at).await {
            if m != mode {
                let _ = state.sync.links.set_mode(&link.peer_device_id, m, at);
                mode = m;
            }
        }
    }
    let mut out = PeerOutcome::default();
    let mut cursor = jr.cursor(&link.peer_device_id).map_err(|e| e.to_string())?;

    if pull && mode.takes_theirs() {
        for _ in 0..MAX_PAGES {
            match client
                .changes_after(&peer, &link.key, cursor.pulled, PAGE)
                .await
                .map_err(|e| e.to_string())?
            {
                Pulled::NotSupported => break,
                Pulled::TooOld { oldest } => {
                    // Entries were dropped there before this device saw them: merge in full.
                    crate::sync::pull_from(app, peer.clone(), link.key.clone())
                        .await
                        .map_err(|e| e.message)?;
                    cursor.pulled = oldest.saturating_sub(1).max(0);
                }
                Pulled::Page(page) => {
                    let (db_jr, entries) = (Arc::clone(jr), page.changes);
                    let handle = app.clone();
                    out.received +=
                        blocking(move || take(&handle.state::<AppState>().db, &db_jr, &entries))
                            .await
                            .unwrap_or(0);
                    cursor.pulled = page.next.max(cursor.pulled);
                    jr.set_cursor(&link.peer_device_id, cursor)
                        .map_err(|e| e.to_string())?;
                    if !page.more {
                        break;
                    }
                }
            }
        }
    }

    if mode.sends_mine() {
        for _ in 0..MAX_PAGES {
            let page = match jr.after(cursor.pushed, &link.peer_device_id, PAGE) {
                Ok(p) => p,
                Err(j::JournalError::CursorTooOld { oldest }) => {
                    // Older entries were pruned here; the other side merges in full on its own.
                    cursor.pushed = oldest.saturating_sub(1).max(0);
                    continue;
                }
                Err(e) => return Err(e.to_string()),
            };
            if !page.changes.is_empty() {
                match client
                    .push_changes(&peer, &link.key, jr.device_id(), &page.changes)
                    .await
                    .map_err(|e| e.to_string())?
                {
                    Some(_) => out.sent += page.changes.len(),
                    None => break,
                }
            }
            cursor.pushed = page.next.max(cursor.pushed);
            jr.set_cursor(&link.peer_device_id, cursor)
                .map_err(|e| e.to_string())?;
            if !page.more {
                break;
            }
        }
    }
    Ok(out)
}

/// Whether to pull from `peer` in this round: always when `force`, else once a minute.
fn pull_due(log: &ChangeLog, peer: &str, force: bool) -> bool {
    let now = j::now_ms();
    let Ok(mut g) = log.pulled_at.lock() else {
        return true;
    };
    let due = force
        || g.get(peer)
            .is_none_or(|t| now.saturating_sub(*t) >= PULL_GAP_MS);
    if due {
        g.insert(peer.to_owned(), now);
    }
    due
}

/// One round with every linked device: push to each, and pull from each when `force` or a minute
/// has passed. Runs once at a time.
pub async fn round(app: &AppHandle, force: bool) -> Option<RoundDto> {
    let state = app.state::<AppState>();
    let log = &state.sync.changes;
    let jr = log.journal.as_ref().map(Arc::clone)?;
    if log.running.swap(true, Ordering::AcqRel) {
        return None;
    }
    let links = state.sync.links.list();
    let mut dto = RoundDto {
        at: j::now_ms(),
        received: 0,
        sent: 0,
        missed: Vec::new(),
    };
    if let Ok(client) = SyncClient::new() {
        for link in links {
            let name = link.peer_name.clone();
            let pull = pull_due(log, &link.peer_device_id, force);
            match with_peer(app, &client, &jr, link, pull).await {
                Ok(o) => {
                    dto.received = dto
                        .received
                        .saturating_add(u32::try_from(o.received).unwrap_or(u32::MAX));
                    dto.sent = dto
                        .sent
                        .saturating_add(u32::try_from(o.sent).unwrap_or(u32::MAX));
                }
                Err(e) => {
                    tracing::debug!(peer = %name, "change sync: {e}");
                    dto.missed.push(name);
                }
            }
        }
    }
    log.running.store(false, Ordering::Release);
    if dto.received > 0 {
        let _ = app.emit("library-changed", ());
    }
    if let Ok(mut g) = log.last.lock() {
        *g = Some(dto.clone());
    }
    let _ = app.emit("changes-round", &dto);
    Some(dto)
}

/// The background loop: a round shortly after each change, when a linked device shows up, and
/// every few minutes.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let wake = Arc::clone(&app.state::<AppState>().sync.changes.wake);
        loop {
            let force = tokio::select! {
                () = wake.notified() => {
                    tokio::time::sleep(SETTLE).await;
                    false
                }
                () = tokio::time::sleep(EVERY) => true,
            };
            if !app.state::<AppState>().sync.links.list().is_empty() {
                round(&app, force).await;
            }
        }
    });
}

/// What the server side runs for `POST /changes`: take the entries, then refresh the library.
pub fn server_changes(app: &AppHandle) -> Option<tm_sync::server::Changes> {
    let jr = Arc::clone(app.state::<AppState>().sync.changes.journal.as_ref()?);
    let (h1, h2) = (app.clone(), app.clone());
    Some(tm_sync::server::Changes {
        journal: jr,
        apply: Arc::new(move |e: &Entry, v: &Value| {
            if let Err(err) = apply(&h1.state::<AppState>().db, e, v) {
                tracing::warn!(kind = %e.kind, "applying a pushed change: {err}");
            }
        }),
        changed: Arc::new(move || {
            let _ = h2.emit("library-changed", ());
        }),
    })
}

// ---------------------------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------------------------

/// "Sync now" on the Sync screen.
#[tauri::command]
pub async fn changes_sync_now(app: AppHandle) -> R<Option<RoundDto>> {
    if app.state::<AppState>().sync.changes.journal.is_none() {
        return Err(CmdError::new(
            "unavailable",
            "Changes can't be tracked on this device right now. Sync from another device still works.",
        ));
    }
    Ok(round(&app, true).await)
}

#[tauri::command]
pub fn changes_last(app: AppHandle) -> Option<RoundDto> {
    app.state::<AppState>()
        .sync
        .changes
        .last
        .lock()
        .ok()
        .and_then(|g| g.clone())
}

/// Sets who leads on a link ("both", "iLead", "theyLead") and tells the other device when it can
/// be reached; otherwise it is told on the next round. Answers the mode that holds.
#[tauri::command]
pub async fn link_set_mode(app: AppHandle, peer_device_id: String, mode: String) -> R<String> {
    let want = match mode.as_str() {
        "both" => Mode::Both,
        "iLead" => Mode::ILead,
        "theyLead" => Mode::TheyLead,
        _ => return Err(CmdError::new("invalid", "Unknown mode.")),
    };
    let state = app.state::<AppState>();
    let at = j::now_ms();
    let (link, _) = state
        .sync
        .links
        .set_mode(&peer_device_id, want, at)
        .map_err(|e| CmdError::new("io", e.to_string()))?
        .ok_or_else(|| CmdError::not_found("That device"))?;
    let mut held = link.mode;
    if let (Some(peer), Ok(client)) = (peer_of(&state, &link), SyncClient::new()) {
        if let Ok(Some((m, theirs_at))) = client.put_roles(&peer, &link.key, want, at).await {
            if m != want {
                let _ = state.sync.links.set_mode(
                    &peer_device_id,
                    m,
                    theirs_at.max(at.saturating_add(1)),
                );
                held = m;
            }
        }
    }
    state.sync.changes.nudge();
    Ok(mode_name(held).to_owned())
}

pub fn mode_name(m: Mode) -> &'static str {
    match m {
        Mode::Both => "both",
        Mode::ILead => "iLead",
        Mode::TheyLead => "theyLead",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: &str, target: Value, value: Value) -> Entry {
        Entry {
            change_id: format!("c-{kind}-{}", target_key(&target)),
            device_id: "phone".into(),
            at: 1_000,
            seq: 0,
            kind: kind.into(),
            target,
            value,
        }
    }

    fn target_key(t: &Value) -> String {
        j::target_key(t)
    }

    fn fixture() -> (Db, Journal) {
        let db = Db::open_in_memory().unwrap();
        let m = SourceManga {
            url: "/m/1".into(),
            title: "One".into(),
            ..Default::default()
        };
        let ch = |u: &str| tm_db::SourceChapter {
            url: u.into(),
            name: u.into(),
            ..Default::default()
        };
        db.add_to_library(7, &m, &[ch("/c/1"), ch("/c/2")]).unwrap();
        (db, Journal::open_in_memory("pc").unwrap())
    }

    #[test]
    fn a_phone_entry_reads_a_chapter_and_adds_a_new_manga() {
        let (db, jr) = fixture();
        let entries = vec![
            entry(
                j::CHAPTER_READ,
                json!({"sourceId": "7", "mangaUrl": "/m/1", "chapterUrl": "/c/2"}),
                json!({"read": true, "lastPageRead": 3}),
            ),
            entry(
                j::MANGA_LIBRARY,
                json!({"sourceId": "7", "mangaUrl": "/m/new"}),
                json!({"inLibrary": true, "dateAdded": 500,
                       "manga": {"title": "New one", "genres": ["Action", "Drama"]}}),
            ),
            entry(
                j::MANGA_CATEGORIES,
                json!({"sourceId": "7", "mangaUrl": "/m/new"}),
                json!({"categories": ["Reading"]}),
            ),
            // A chapter this device does not have: dropped, not an error.
            entry(
                j::CHAPTER_READ,
                json!({"sourceId": "7", "mangaUrl": "/m/1", "chapterUrl": "/c/9"}),
                json!({"read": true, "lastPageRead": 0}),
            ),
        ];
        assert_eq!(take(&db, &jr, &entries), 4);
        let m = db.manga_by_url("/m/1", 7).unwrap().unwrap();
        let c2 = db
            .chapters(m.id)
            .unwrap()
            .into_iter()
            .find(|c| c.url == "/c/2")
            .unwrap();
        assert!(c2.read);
        assert_eq!(c2.last_page_read, 3);
        let added = db.manga_by_url("/m/new", 7).unwrap().unwrap();
        assert!(added.favorite);
        assert_eq!(added.title, "New one");
        assert_eq!(added.genre.as_deref(), Some("Action, Drama"));
        assert_eq!(added.date_added, Some(500));
        let reading = category_id(&db, "reading").unwrap();
        assert!(db.category_assignments().unwrap()[&added.id].contains(&reading));
        // The same entries again are repeats.
        assert_eq!(take(&db, &jr, &entries), 0);
    }

    #[test]
    fn this_devices_own_entries_are_never_taken_back() {
        let (db, jr) = fixture();
        let mut e = entry(
            j::MANGA_LIBRARY,
            json!({"sourceId": "7", "mangaUrl": "/m/1"}),
            json!({"inLibrary": false}),
        );
        e.device_id = "pc".into();
        assert_eq!(take(&db, &jr, &[e]), 0);
        assert!(db.manga_by_url("/m/1", 7).unwrap().unwrap().favorite);
    }

    #[test]
    fn a_rename_onto_an_existing_category_moves_the_manga() {
        let (db, jr) = fixture();
        let m = db.manga_by_url("/m/1", 7).unwrap().unwrap();
        let old = db.create_category("Old").unwrap().unwrap();
        let new = db.create_category("New").unwrap().unwrap();
        db.set_manga_categories(m.id, &BTreeSet::from([old]))
            .unwrap();
        let e = entry(
            j::CATEGORY_RENAME,
            json!({"name": "Old"}),
            json!({"name": "New"}),
        );
        assert_eq!(take(&db, &jr, &[e]), 1);
        assert!(category_id(&db, "Old").is_none());
        assert_eq!(
            db.category_assignments().unwrap()[&m.id],
            BTreeSet::from([new])
        );
    }

    #[test]
    fn source_ids_read_signed_or_unsigned() {
        assert_eq!(source_of(&json!({"sourceId": "-5"})), Some(-5));
        assert_eq!(
            source_of(&json!({"sourceId": "18446744073709551615"})),
            Some(-1)
        );
        assert_eq!(source_of(&json!({"sourceId": 12})), Some(12));
        assert_eq!(source_of(&json!({"sourceId": "x"})), None);
    }
}
