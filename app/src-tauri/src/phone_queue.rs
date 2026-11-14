//! A linked phone's download queue, downloaded on this PC
//! (`docs/rust-port/profiles-and-remote-downloads.md` §3).
//!
//! "Download here" claims the phone's queued chapters, queues them in this PC's downloader, and
//! when each one is done puts it where the user chose:
//! - **This PC**: kept here; the phone is told it is on this device.
//! - **NAS**: uploaded in the phone's layout, then removed from the PC; the phone is told it is on
//!   the server.
//! - **The phone**: packed and sent with `PUT /file`, then removed from the PC. A phone that
//!   can't be reached keeps it waiting here, and it goes the next time the queue is looked at.
//!
//! Each outcome is a done event kept in the outbox until the phone acks it; the phone also pulls
//! `GET /outbox`. A chapter that fails to download is given back to the phone.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tm_download::ChapterRequest;
use tm_sync::queue::{new_event_id, ChapterRef, DoneEvent, Outbox, QueueItem, Where};
use tm_sync::{PeerDevice, SyncClient, SyncError};
use ts_rs::TS;

use crate::dto::CmdError;
use crate::state::AppState;

type R<T> = Result<T, CmdError>;

/// Most chapters one "Download here" claims.
const MAX_CLAIM: usize = 2000;
/// Most queue items sent to the UI; the count is always the whole queue.
const MAX_SHOWN: usize = 300;
/// Most jobs kept, finished ones first to go.
const MAX_JOBS: usize = 5000;

/// Where a chapter downloaded for the phone ends up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Dest {
    Pc,
    Nas,
    Phone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Stage {
    /// In this PC's download queue.
    Downloading,
    /// Waiting on the NAS upload.
    Uploading,
    /// Downloaded, waiting to be sent to the phone.
    Sending,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Job {
    peer: String,
    chapter: ChapterRef,
    dest: Dest,
    stage: Stage,
    label: String,
    error: Option<String>,
    /// The NAS place while uploading.
    #[serde(default)]
    segments: Vec<String>,
    /// The chapter was on this PC before the phone asked, so its copy here stays.
    #[serde(default)]
    keep_local: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Saved {
    jobs: Vec<Job>,
    /// The destination last chosen per linked device.
    dests: HashMap<String, Dest>,
}

/// The jobs and the outbox of one profile.
pub struct PhoneQueue {
    file: PathBuf,
    saved: Mutex<Saved>,
    pub outbox: Arc<Outbox>,
    /// Set while a send or outbox push runs, so polling doesn't start a second one.
    flushing: AtomicBool,
}

impl PhoneQueue {
    pub fn open(data_dir: &Path) -> Self {
        let file = data_dir.join("phone-jobs.json");
        let saved = std::fs::read(&file)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Self {
            file,
            saved: Mutex::new(saved),
            outbox: Arc::new(Outbox::open(&data_dir.join("outbox.json"))),
            flushing: AtomicBool::new(false),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Saved> {
        self.saved.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn save(&self, s: &Saved) {
        let Ok(bytes) = serde_json::to_vec(s) else {
            return;
        };
        let tmp = self.file.with_extension("json.part");
        if let Err(e) = std::fs::write(&tmp, bytes).and_then(|()| std::fs::rename(&tmp, &self.file))
        {
            tracing::warn!("couldn't save the phone jobs: {e}");
        }
    }

    /// Changes the saved state and writes it.
    fn change<T>(&self, f: impl FnOnce(&mut Saved) -> T) -> T {
        let mut s = self.lock();
        let out = f(&mut s);
        if s.jobs.len() > MAX_JOBS {
            let mut extra = s.jobs.len() - MAX_JOBS;
            s.jobs.retain(|j| {
                let drop = extra > 0 && matches!(j.stage, Stage::Done | Stage::Failed);
                if drop {
                    extra -= 1;
                }
                !drop
            });
        }
        self.save(&s);
        out
    }

    /// The open job (not done or failed) for `chapter`.
    fn open_job(&self, chapter: &ChapterRef) -> Option<Job> {
        self.lock()
            .jobs
            .iter()
            .find(|j| &j.chapter == chapter && !matches!(j.stage, Stage::Done | Stage::Failed))
            .cloned()
    }

    fn set_stage(&self, chapter: &ChapterRef, stage: Stage, error: Option<String>) {
        self.change(|s| {
            for j in s.jobs.iter_mut().filter(|j| {
                &j.chapter == chapter && !matches!(j.stage, Stage::Done | Stage::Failed)
            }) {
                j.stage = stage;
                j.error.clone_from(&error);
            }
        });
    }
}

fn chapter_of(c: &ChapterRequest) -> ChapterRef {
    ChapterRef {
        source_id: c.source_id,
        manga_url: c.manga_url.clone(),
        chapter_url: c.chapter_url.clone(),
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

fn emit(app: &AppHandle) {
    let _ = app.emit("phone-queue", ());
    // A chapter sent to the phone or the NAS may have left this computer.
    let _ = app.emit("downloads-changed", ());
}

/// The linked device `peer` as something to call, and its key. Prefers the address mDNS last
/// saw over the one saved with the link.
fn peer_of(state: &AppState, peer: &str) -> Option<(PeerDevice, String)> {
    let link = state.sync.links.by_peer(peer)?;
    if let Some(seen) = state.sync.peer(peer) {
        return Some((seen, link.key));
    }
    let (host, port) = tm_sync::protocol::split_address(&link.last_address)?;
    Some((
        PeerDevice {
            id: link.peer_device_id.clone(),
            name: link.peer_name.clone(),
            host,
            port,
        },
        link.key,
    ))
}

// ---------------------------------------------------------------------------------------------
// From the downloader and the NAS uploads
// ---------------------------------------------------------------------------------------------

/// Where a chapter being downloaded for a phone goes; `None` when no phone asked for it.
pub fn dest_of(app: &AppHandle, c: &ChapterRequest) -> Option<Dest> {
    let state = app.state::<AppState>();
    state.sync.phone.open_job(&chapter_of(c)).map(|j| j.dest)
}

/// A chapter downloaded for a phone: keep it, wait for its NAS upload, or send it.
pub fn downloaded(app: &AppHandle, c: &ChapterRequest) {
    let state = app.state::<AppState>();
    let chapter = chapter_of(c);
    let Some(job) = state.sync.phone.open_job(&chapter) else {
        return;
    };
    match job.dest {
        Dest::Pc => finish(app, &job, Where::Device),
        Dest::Nas => {
            match crate::nas::place_of(&state, c.source_id, &c.manga_url, &c.chapter_url) {
                Some(p) if p.found => {
                    if !job.keep_local {
                        remove_local(&state, &chapter);
                    }
                    finish(app, &job, Where::Server);
                }
                Some(p) => state.sync.phone.change(|s| {
                    for j in s.jobs.iter_mut().filter(|j| j.chapter == chapter) {
                        if j.stage == Stage::Downloading {
                            j.stage = Stage::Uploading;
                            j.segments.clone_from(&p.segments);
                        }
                    }
                }),
                None => {
                    // The NAS was switched off meanwhile: it stays here, and the phone is told so.
                    finish(app, &job, Where::Device);
                }
            }
        }
        Dest::Phone => {
            state.sync.phone.set_stage(&chapter, Stage::Sending, None);
            flush(app, &job.peer);
        }
    }
    emit(app);
}

/// A chapter that failed to download: give it back to the phone.
pub fn failed(app: &AppHandle, c: &ChapterRequest, error: Option<&str>) {
    let state = app.state::<AppState>();
    let chapter = chapter_of(c);
    let Some(job) = state.sync.phone.open_job(&chapter) else {
        return;
    };
    state.sync.phone.set_stage(
        &chapter,
        Stage::Failed,
        Some(error.unwrap_or("The download failed").to_owned()),
    );
    emit(app);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let Some((peer, key)) = peer_of(&state, &job.peer) else {
            return;
        };
        if let Ok(client) = SyncClient::new() {
            if let Err(e) = client.release(&peer, &key, vec![chapter]).await {
                tracing::info!("couldn't give a chapter back to {}: {e}", peer.name);
            }
        }
    });
}

/// A NAS upload finished: the phone job waiting on it is done.
pub fn uploaded(app: &AppHandle, segments: &[String]) {
    let state = app.state::<AppState>();
    let job = state
        .sync
        .phone
        .lock()
        .jobs
        .iter()
        .find(|j| j.stage == Stage::Uploading && j.segments == segments)
        .cloned();
    if let Some(job) = job {
        finish(app, &job, Where::Server);
        emit(app);
    }
}

fn remove_local(state: &AppState, c: &ChapterRef) {
    if let Some(dm) = state.downloads.get() {
        dm.delete_downloaded(c.source_id, &c.manga_url, &c.chapter_url);
    }
}

/// Marks `job` done, adds its event to the outbox, and pushes the outbox.
fn finish(app: &AppHandle, job: &Job, where_: Where) {
    let state = app.state::<AppState>();
    state.sync.phone.set_stage(&job.chapter, Stage::Done, None);
    state.sync.phone.outbox.add(
        &job.peer,
        DoneEvent {
            chapter: job.chapter.clone(),
            event_id: new_event_id(),
            where_,
            device_id: state.sync.identity.id.clone(),
            at: now_ms(),
            take: None,
        },
    );
    flush(app, &job.peer);
}

/// Sends what waits for `peer` (chapters to send, then the outbox), in the background. One run
/// at a time; the next poll picks up anything left.
pub fn flush(app: &AppHandle, peer: &str) {
    let state = app.state::<AppState>();
    if state.sync.phone.flushing.swap(true, Ordering::AcqRel) {
        return;
    }
    let app = app.clone();
    let peer = peer.to_owned();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        if let Err(e) = flush_now(&app, &peer).await {
            tracing::info!("phone {peer} not reached: {}", e.message);
        }
        state.sync.phone.flushing.store(false, Ordering::Release);
        emit(&app);
    });
}

async fn flush_now(app: &AppHandle, peer_id: &str) -> R<()> {
    let state = app.state::<AppState>();
    let (peer, key) = peer_of(&state, peer_id).ok_or_else(|| CmdError::not_found("That device"))?;
    let client = SyncClient::new()?;
    let sending: Vec<Job> = state
        .sync
        .phone
        .lock()
        .jobs
        .iter()
        .filter(|j| j.peer == peer_id && j.stage == Stage::Sending)
        .cloned()
        .collect();
    for job in sending {
        match send(&state, &client, &peer, &key, &job.chapter).await {
            Ok(()) => {
                if !job.keep_local {
                    remove_local(&state, &job.chapter);
                }
                finish_quiet(&state, &job, Where::Sent);
            }
            Err(e) => {
                state
                    .sync
                    .phone
                    .set_stage(&job.chapter, Stage::Sending, Some(e.message.clone()));
                if matches!(e.kind.as_str(), "unreachable" | "unlinked") {
                    return Err(e);
                }
            }
        }
    }
    let pending = state.sync.phone.outbox.pending(peer_id);
    if !pending.is_empty() {
        let acked = client.done(&peer, &key, pending).await.map_err(sync_err)?;
        state.sync.phone.outbox.ack(peer_id, &acked);
    }
    Ok(())
}

/// [`finish`] without starting another flush (we're in one).
fn finish_quiet(state: &AppState, job: &Job, where_: Where) {
    state.sync.phone.set_stage(&job.chapter, Stage::Done, None);
    state.sync.phone.outbox.add(
        &job.peer,
        DoneEvent {
            chapter: job.chapter.clone(),
            event_id: new_event_id(),
            where_,
            device_id: state.sync.identity.id.clone(),
            at: now_ms(),
            take: None,
        },
    );
}

fn sync_err(e: SyncError) -> CmdError {
    match e {
        SyncError::Unlinked => CmdError::new(
            "unlinked",
            "The phone no longer has this link. Link it again on the Sync screen.",
        ),
        SyncError::Http(m) => CmdError::new("unreachable", m),
        other => CmdError::new("sync", other.to_string()),
    }
}

/// Packs the downloaded chapter and sends it to the phone's downloads folder.
async fn send(
    state: &AppState,
    client: &SyncClient,
    peer: &PeerDevice,
    key: &str,
    c: &ChapterRef,
) -> R<()> {
    let segments = crate::nas::segments_of(state, c.source_id, &c.manga_url, &c.chapter_url)
        .ok_or_else(|| CmdError::new("invalid", "This chapter's source has no name here yet."))?;
    let dm = crate::downloads::manager(state)?;
    let dir =
        tm_download::layout::chapter_dir(dm.root(), c.source_id, &c.manga_url, &c.chapter_url);
    let work = state
        .paths
        .data_dir()
        .join(format!("send-{}.cbz", new_event_id()));
    let packed = {
        let dir = dir.clone();
        let work = work.clone();
        tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
            let names = tm_archive::page_names(&dir).map_err(|e| e.to_string())?;
            if names.is_empty() {
                return Err("the chapter has no pages".into());
            }
            let mut pages = Vec::with_capacity(names.len());
            for n in names {
                let bytes = tm_archive::read_page(&dir, &n).map_err(|e| e.to_string())?;
                pages.push((n, bytes));
            }
            tm_archive::pack_cbz(&pages, &work).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| CmdError::internal(e.to_string()))?
    };
    if let Err(e) = packed {
        let _ = std::fs::remove_file(&work);
        return Err(CmdError::new(
            "io",
            format!("Couldn't pack the chapter: {e}"),
        ));
    }
    let path = format!("downloads/{}", segments.join("/"));
    let sent = client.put_file(peer, key, &path, &work).await;
    let _ = std::fs::remove_file(&work);
    sent.map_err(sync_err)
}

// ---------------------------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PhoneQueueItemDto {
    pub manga_title: String,
    pub chapter_name: String,
    /// Whether this PC's library has the chapter, so it can download it.
    pub here: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PhoneJobDto {
    pub label: String,
    /// `downloading`, `uploading`, `sending`, `done` or `failed`.
    pub stage: String,
    pub dest: Dest,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PhoneQueueDto {
    pub peer_device_id: String,
    pub peer_name: String,
    /// Why the queue couldn't be read, when it couldn't.
    pub error: Option<String>,
    /// The phone's queue, first [`MAX_SHOWN`] items.
    pub items: Vec<PhoneQueueItemDto>,
    pub total: u32,
    pub dest: Dest,
    pub nas_ready: bool,
    /// What this PC is doing for the phone, newest first.
    pub jobs: Vec<PhoneJobDto>,
    /// Done events the phone hasn't acked yet.
    pub unsent_events: u32,
}

fn n(v: usize) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}

/// The phone's library rows for a queue item, when this PC has them.
fn request_of(state: &AppState, c: &ChapterRef) -> Option<ChapterRequest> {
    let manga = state.db.manga_by_url(&c.manga_url, c.source_id).ok()??;
    let ch = state
        .db
        .chapters(manga.id)
        .ok()?
        .into_iter()
        .find(|ch| ch.url == c.chapter_url)?;
    Some(crate::downloads::request(&manga, &ch))
}

fn jobs_of(state: &AppState, peer: &str) -> Vec<PhoneJobDto> {
    let s = state.sync.phone.lock();
    s.jobs
        .iter()
        .rev()
        .filter(|j| j.peer == peer)
        .take(MAX_SHOWN)
        .map(|j| PhoneJobDto {
            label: j.label.clone(),
            stage: format!("{:?}", j.stage).to_lowercase(),
            dest: j.dest,
            error: j.error.clone(),
        })
        .collect()
}

fn dest_for(state: &AppState, peer: &str) -> Dest {
    let saved = state.sync.phone.lock().dests.get(peer).copied();
    saved.unwrap_or(Dest::Pc)
}

async fn read_queue(state: &AppState, peer_id: &str) -> R<(PeerDevice, String, Vec<QueueItem>)> {
    let (peer, key) = peer_of(state, peer_id).ok_or_else(|| CmdError::not_found("That device"))?;
    let items = SyncClient::new()?
        .queue(&peer, &key)
        .await
        .map_err(sync_err)?;
    Ok((peer, key, items))
}

/// The linked phone's queue and what this PC is doing for it. Also sends what waits for it.
#[tauri::command]
pub async fn phone_queue(app: AppHandle, peer_device_id: String) -> R<PhoneQueueDto> {
    let state = app.state::<AppState>();
    let link = state
        .sync
        .links
        .by_peer(&peer_device_id)
        .ok_or_else(|| CmdError::not_found("That device"))?;
    let (items, error) = match read_queue(&state, &peer_device_id).await {
        Ok((_, _, items)) => (items, None),
        Err(e) => (Vec::new(), Some(e.message)),
    };
    if error.is_none() {
        flush(&app, &peer_device_id);
    }
    // Claimed chapters stay in the phone's queue, paused; leave out the ones this PC has.
    let ours: std::collections::HashSet<ChapterRef> = state
        .sync
        .phone
        .lock()
        .jobs
        .iter()
        .filter(|j| j.peer == peer_device_id && !matches!(j.stage, Stage::Done | Stage::Failed))
        .map(|j| j.chapter.clone())
        .collect();
    let waiting: Vec<&QueueItem> = items
        .iter()
        .filter(|i| !ours.contains(&i.chapter))
        .collect();
    Ok(PhoneQueueDto {
        peer_name: link.peer_name.clone(),
        peer_device_id: peer_device_id.clone(),
        error,
        total: n(waiting.len()),
        items: waiting
            .iter()
            .take(MAX_SHOWN)
            .map(|i| PhoneQueueItemDto {
                manga_title: i.manga_title.clone(),
                chapter_name: i.chapter_name.clone(),
                here: request_of(&state, &i.chapter).is_some(),
            })
            .collect(),
        dest: dest_for(&state, &peer_device_id),
        nas_ready: crate::nas::is_enabled(&state),
        jobs: jobs_of(&state, &peer_device_id),
        unsent_events: n(state.sync.phone.outbox.pending(&peer_device_id).len()),
    })
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ResumeDto {
    /// Chapters this PC took over.
    pub claimed: u32,
    /// Chapters this PC's library doesn't have (sync the library first); left on the phone.
    pub missing: u32,
    /// Chapters the phone kept (already downloading there, or claimed by another PC).
    pub kept: u32,
}

/// Takes over the phone's queue: claims it, downloads it here, and puts each chapter at `dest`.
#[tauri::command]
pub async fn phone_queue_resume(
    app: AppHandle,
    peer_device_id: String,
    dest: Dest,
) -> R<ResumeDto> {
    let state = app.state::<AppState>();
    if dest == Dest::Nas && !crate::nas::is_enabled(&state) {
        return Err(CmdError::new(
            "invalid",
            "Set up the NAS first (Settings, then Remote storage), or pick another place.",
        ));
    }
    state.sync.phone.change(|s| {
        s.dests.insert(peer_device_id.clone(), dest);
    });
    let dm = crate::downloads::manager(&state)?.clone();
    let (peer, key, items) = read_queue(&state, &peer_device_id).await?;
    let mut wanted = Vec::new();
    let mut missing = Vec::new();
    for i in items.into_iter().take(MAX_CLAIM) {
        if state.sync.phone.open_job(&i.chapter).is_some() {
            continue;
        }
        match request_of(&state, &i.chapter) {
            Some(r) => wanted.push((i, r)),
            None => missing.push(i),
        }
    }
    let client = SyncClient::new()?;
    let refs: Vec<ChapterRef> = wanted.iter().map(|(i, _)| i.chapter.clone()).collect();
    let asked = refs.len();
    let claimed: std::collections::HashSet<ChapterRef> = if refs.is_empty() {
        Default::default()
    } else {
        client
            .claim(&peer, &key, refs, &state.sync.identity.id)
            .await
            .map_err(sync_err)?
            .into_iter()
            .collect()
    };
    let mine: Vec<(QueueItem, ChapterRequest)> = wanted
        .into_iter()
        .filter(|(i, _)| claimed.contains(&i.chapter))
        .collect();
    state.sync.phone.change(|s| {
        for (i, r) in &mine {
            s.jobs.push(Job {
                peer: peer_device_id.clone(),
                chapter: i.chapter.clone(),
                dest,
                stage: Stage::Downloading,
                label: format!("{} · {}", r.manga_title, r.chapter_name),
                error: None,
                segments: Vec::new(),
                keep_local: dm.is_downloaded(r.source_id, &r.manga_url, &r.chapter_url),
            });
        }
    });
    for (_, r) in &mine {
        // Already on this PC: route it now, as if it had just finished. One already queued here
        // finishes through the downloader.
        if dm.enqueue(r.clone()) || !dm.is_downloaded(r.source_id, &r.manga_url, &r.chapter_url) {
            continue;
        }
        if dest == Dest::Nas {
            let dir = tm_download::layout::chapter_dir(
                dm.root(),
                r.source_id,
                &r.manga_url,
                &r.chapter_url,
            );
            crate::nas::upload_finished(
                &app,
                r.source_id,
                &r.manga_url,
                &r.chapter_url,
                dir,
                false,
            );
        }
        downloaded(&app, r);
    }
    tracing::info!(
        peer = %peer_device_id,
        claimed = mine.len(),
        missing = missing.len(),
        "took over the phone's queue"
    );
    emit(&app);
    Ok(ResumeDto {
        claimed: n(mine.len()),
        missing: n(missing.len()),
        kept: n(asked.saturating_sub(mine.len())),
    })
}

/// Remembers where chapters downloaded for `peer_device_id` go.
#[tauri::command]
pub fn phone_queue_set_dest(state: State<'_, AppState>, peer_device_id: String, dest: Dest) {
    state.sync.phone.change(|s| {
        s.dests.insert(peer_device_id, dest);
    });
}

/// Forgets the finished and failed jobs for `peer_device_id`.
#[tauri::command]
pub fn phone_queue_clear_finished(state: State<'_, AppState>, peer_device_id: String) {
    state.sync.phone.change(|s| {
        s.jobs.retain(|j| {
            j.peer != peer_device_id || !matches!(j.stage, Stage::Done | Stage::Failed)
        });
    });
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn job(ch: &str, stage: Stage) -> Job {
        Job {
            peer: "phone".into(),
            chapter: ChapterRef {
                source_id: 1,
                manga_url: "/m".into(),
                chapter_url: ch.into(),
            },
            dest: Dest::Phone,
            stage,
            label: ch.into(),
            error: None,
            segments: Vec::new(),
            keep_local: false,
        }
    }

    #[test]
    fn jobs_and_destinations_survive_a_restart_and_finished_ones_are_dropped_first() {
        let dir = tempfile::tempdir().unwrap();
        let q = PhoneQueue::open(dir.path());
        q.change(|s| {
            s.dests.insert("phone".into(), Dest::Nas);
            s.jobs.push(job("/1", Stage::Done));
            s.jobs.push(job("/2", Stage::Sending));
        });
        let q = PhoneQueue::open(dir.path());
        assert_eq!(q.lock().dests.get("phone"), Some(&Dest::Nas));
        assert!(
            q.open_job(&job("/1", Stage::Done).chapter).is_none(),
            "done is not open"
        );
        assert_eq!(
            q.open_job(&job("/2", Stage::Done).chapter).unwrap().stage,
            Stage::Sending
        );
        q.change(|s| {
            for i in 0..MAX_JOBS {
                s.jobs.push(job(&format!("/x{i}"), Stage::Downloading));
            }
        });
        let s = q.lock();
        // Only finished jobs are dropped; open ones are work still owed to the phone.
        assert_eq!(s.jobs.len(), MAX_JOBS + 1);
        assert!(
            s.jobs.iter().all(|j| j.chapter.chapter_url != "/1"),
            "the done job went first"
        );
        assert!(s.jobs.iter().any(|j| j.chapter.chapter_url == "/2"));
    }

    #[test]
    fn a_damaged_file_starts_empty() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("phone-jobs.json"), b"{not json").unwrap();
        let q = PhoneQueue::open(dir.path());
        assert!(q.lock().jobs.is_empty());
    }
}
