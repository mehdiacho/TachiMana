//! One-off copies between linked devices (`roles/1`, `docs/protocol/capabilities/roles-1.md`):
//! "take everything from it once", "send everything to it once", and the other device asking
//! this one to take its library (`POST /roles/take`), which the person confirms here.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tm_sync::queue::DoneEvent;
use tm_sync::roles::How;
use tm_sync::{Link, SyncClient};
use ts_rs::TS;

use crate::dto::CmdError;
use crate::state::AppState;

type R<T> = Result<T, CmdError>;

/// How long "send everything" waits for the other device's answer.
const ANSWER_WAIT: Duration = Duration::from_secs(15 * 60);
const ANSWER_POLL: Duration = Duration::from_secs(5);
/// Requests kept waiting for the person at most; older ones are dropped.
const MAX_PENDING: usize = 8;

/// A linked device asked this one to take its library, and the person has not answered yet.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TakeAskDto {
    pub request_id: String,
    pub peer_device_id: String,
    pub peer_name: String,
    /// `"merge"` or `"replace"`.
    #[ts(type = "\"merge\" | \"replace\"")]
    pub how: String,
    /// Manga in this profile's library now, for "Replace this PC's 212 manga".
    pub library_count: u32,
}

/// Requests waiting for the person, held in [`crate::sync::Sync`].
#[derive(Default)]
pub struct Takes {
    pending: Mutex<Vec<TakeAskDto>>,
}

impl Takes {
    pub fn list(&self) -> Vec<TakeAskDto> {
        self.pending.lock().map(|g| g.clone()).unwrap_or_default()
    }

    fn take_out(&self, request_id: &str) -> Option<TakeAskDto> {
        let mut g = self.pending.lock().ok()?;
        let i = g.iter().position(|t| t.request_id == request_id)?;
        Some(g.remove(i))
    }
}

fn how_name(how: How) -> &'static str {
    match how {
        How::Merge => "merge",
        How::Replace => "replace",
    }
}

fn library_count(state: &AppState) -> u32 {
    state
        .db
        .library()
        .map(|l| u32::try_from(l.len()).unwrap_or(u32::MAX))
        .unwrap_or(0)
}

/// The server's `POST /roles/take` hook: a merge from a device the person already trusts runs at
/// once; anything else waits for the person.
pub fn server_hook(app: &AppHandle) -> Arc<tm_sync::server::TakeFn> {
    let app = app.clone();
    Arc::new(move |link: &Link, how: How, request_id: String| {
        let state = app.state::<AppState>();
        if how == How::Merge && link.auto_merge {
            let (app, link) = (app.clone(), link.clone());
            tauri::async_runtime::spawn(async move {
                let _ = run(&app, &link, how, Some(&request_id)).await;
            });
            return;
        }
        let ask = TakeAskDto {
            request_id,
            peer_device_id: link.peer_device_id.clone(),
            peer_name: link.peer_name.clone(),
            how: how_name(how).to_owned(),
            library_count: library_count(&state),
        };
        if let Ok(mut g) = state.sync.takes.pending.lock() {
            g.retain(|t| t.peer_device_id != ask.peer_device_id);
            g.push(ask.clone());
            let extra = g.len().saturating_sub(MAX_PENDING);
            g.drain(..extra);
        }
        let _ = app.emit("take-asked", &ask);
    })
}

/// The server's `PUT /roles` hook: tell the person the other device changed who leads.
pub fn roles_hook(app: &AppHandle) -> Arc<tm_sync::server::RolesChangedFn> {
    let app = app.clone();
    Arc::new(move |link: &Link| {
        let text = match link.mode {
            tm_sync::roles::Mode::Both => {
                format!("{} and this PC now stay in step both ways.", link.peer_name)
            }
            tm_sync::roles::Mode::ILead => format!("{} now follows this PC.", link.peer_name),
            tm_sync::roles::Mode::TheyLead => format!("This PC now follows {}.", link.peer_name),
        };
        let _ = app.emit("notice", text);
        let _ = app.emit("links-changed", ());
    })
}

/// Copies `link`'s library into this profile once. With `Replace`, this profile's library is
/// saved to a backup file first and what the other device does not have leaves the library.
/// When `request_id` is set (the other device asked), the result goes into its outbox.
pub async fn run(app: &AppHandle, link: &Link, how: How, request_id: Option<&str>) -> R<String> {
    let out = copy(app, link, how).await;
    let state = app.state::<AppState>();
    if let Some(id) = request_id {
        let (ok, message) = match &out {
            Ok(_) => (true, None),
            Err(e) => (false, Some(e.message.clone())),
        };
        state.sync.phone.outbox.add(
            &link.peer_device_id,
            DoneEvent::take(&state.sync.identity.id, id, ok, message),
        );
    }
    let _ = app.emit(
        "notice",
        match &out {
            Ok(text) => text.clone(),
            Err(e) => format!("Copying {}'s library failed: {}", link.peer_name, e.message),
        },
    );
    out
}

async fn copy(app: &AppHandle, link: &Link, how: How) -> R<String> {
    let state = app.state::<AppState>();
    let peer = crate::changes::peer_of(&state, link).ok_or_else(|| {
        CmdError::new(
            "unreachable",
            format!("{} has no address yet.", link.peer_name),
        )
    })?;
    let client = SyncClient::new()?;
    // What the other device keeps in its library, to know what leaves this one on a replace.
    let theirs: Option<HashSet<(i64, String)>> = match how {
        How::Merge => None,
        How::Replace => {
            let bytes = client.fetch_backup(&peer, &link.key).await?;
            let b = tm_backup::codec::decode(&bytes)?;
            Some(
                b.backup_manga
                    .iter()
                    .filter(|m| m.favorite.unwrap_or(true))
                    .filter_map(|m| Some((m.source?, m.url.clone()?)))
                    .collect(),
            )
        }
    };
    let saved = if theirs.is_some() {
        Some(save_backup(&state, &link.peer_name)?)
    } else {
        None
    };
    // This side's older entries must not follow the copy back and undo it.
    if let Some(jr) = &state.sync.changes.journal {
        if let (Ok(head), Ok(mut cur)) = (jr.head(), jr.cursor(&link.peer_device_id)) {
            cur.pushed = cur.pushed.max(head);
            let _ = jr.set_cursor(&link.peer_device_id, cur);
        }
    }
    let report = crate::sync::pull_from(app, peer, link.key.clone()).await?;
    let mut removed = 0usize;
    if let Some(theirs) = theirs {
        for m in state.db.library()? {
            if !theirs.contains(&(m.source, m.url.clone())) {
                state.db.set_favorite(m.id, false)?;
                crate::changes::library(&state, m.id);
                removed += 1;
            }
        }
        let _ = app.emit("library-changed", ());
    }
    let added = report.summary.manga_added;
    Ok(match saved {
        Some(file) => format!(
            "This PC now has {}'s library: {added} manga added, {removed} removed. The old library is saved in {file}.",
            link.peer_name
        ),
        None => format!(
            "Merged {}'s library: {added} manga added, {} brought up to date.",
            link.peer_name, report.summary.manga_updated
        ),
    })
}

/// Saves this profile's library to `backups/` in its data folder before a replace.
fn save_backup(state: &AppState, other: &str) -> R<String> {
    let dir = state.paths.data_dir().join("backups");
    std::fs::create_dir_all(&dir)
        .map_err(|e| CmdError::new("io", format!("Couldn't make the backups folder: {e}")))?;
    let safe: String = other
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .take(40)
        .collect();
    let path = dir.join(format!(
        "before-{safe}-{}.tachibk",
        tm_sync::journal::now_ms()
    ));
    crate::sync::export_to(state, &path)?;
    Ok(path.display().to_string())
}

fn link_of(state: &AppState, peer_device_id: &str) -> R<Link> {
    state
        .sync
        .links
        .by_peer(peer_device_id)
        .ok_or_else(|| CmdError::not_found("That device"))
}

fn parse_how(how: &str) -> R<How> {
    match how {
        "merge" => Ok(How::Merge),
        "replace" => Ok(How::Replace),
        _ => Err(CmdError::new("invalid", "Choose merge or replace.")),
    }
}

// ---------------------------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------------------------

#[tauri::command]
pub fn take_asks(app: AppHandle) -> Vec<TakeAskDto> {
    app.state::<AppState>().sync.takes.list()
}

/// The person's answer to a linked device asking to send its library here. `remember` (merge
/// only) lets that device merge again later without asking.
#[tauri::command]
pub async fn take_answer(
    app: AppHandle,
    request_id: String,
    accept: bool,
    remember: bool,
) -> R<()> {
    let state = app.state::<AppState>();
    let ask = state
        .sync
        .takes
        .take_out(&request_id)
        .ok_or_else(|| CmdError::not_found("That request"))?;
    let mut link = link_of(&state, &ask.peer_device_id)?;
    if !accept {
        state.sync.phone.outbox.add(
            &link.peer_device_id,
            DoneEvent::take(
                &state.sync.identity.id,
                &request_id,
                false,
                Some("Declined".into()),
            ),
        );
        return Ok(());
    }
    let how = parse_how(&ask.how)?;
    if remember && how == How::Merge && !link.auto_merge {
        link.auto_merge = true;
        state
            .sync
            .links
            .put(link.clone())
            .map_err(|e| CmdError::new("io", e.to_string()))?;
    }
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = run(&app2, &link, how, Some(&request_id)).await;
    });
    Ok(())
}

/// "Copy its library here once" on a linked device.
#[tauri::command]
pub async fn take_from(app: AppHandle, peer_device_id: String, how: String) -> R<String> {
    let link = link_of(&app.state::<AppState>(), &peer_device_id)?;
    run(&app, &link, parse_how(&how)?, None).await
}

/// "Send this PC's library once": asks the linked device to take it; it confirms there. The
/// answer arrives as a `notice` event when it comes.
#[tauri::command]
pub async fn send_to(app: AppHandle, peer_device_id: String, how: String) -> R<()> {
    let how = parse_how(&how)?;
    let state = app.state::<AppState>();
    let link = link_of(&state, &peer_device_id)?;
    let peer = crate::changes::peer_of(&state, &link).ok_or_else(|| {
        CmdError::new(
            "unreachable",
            format!("{} has no address yet.", link.peer_name),
        )
    })?;
    let client = SyncClient::new()?;
    let request_id = tm_sync::queue::new_event_id();
    if !client.ask_take(&peer, &link.key, how, &request_id).await? {
        return Err(CmdError::new(
            "unsupported",
            format!(
                "{} can't take a library this way yet. Update it, or copy from there instead.",
                link.peer_name
            ),
        ));
    }
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        let deadline = std::time::Instant::now() + ANSWER_WAIT;
        while std::time::Instant::now() < deadline {
            tokio::time::sleep(ANSWER_POLL).await;
            let Ok(events) = client.outbox(&peer, &link.key, None).await else {
                continue;
            };
            let Some(t) = events
                .iter()
                .filter_map(|e| e.take.as_ref())
                .find(|t| t.request_id == request_id)
            else {
                continue;
            };
            let text = if t.ok {
                format!("{} took this PC's library.", link.peer_name)
            } else if t.message.as_deref() == Some("Declined") {
                format!("{} declined this PC's library.", link.peer_name)
            } else {
                format!(
                    "{} couldn't take this PC's library: {}",
                    link.peer_name,
                    t.message.as_deref().unwrap_or("no reason given")
                )
            };
            let _ = app2.emit("notice", text);
            return;
        }
        let _ = app2.emit(
            "notice",
            format!(
                "{} hasn't answered about this PC's library.",
                link.peer_name
            ),
        );
    });
    Ok(())
}
