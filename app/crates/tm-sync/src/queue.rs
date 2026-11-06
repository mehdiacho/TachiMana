//! The phone's download queue, downloaded on the PC
//! (`docs/rust-port/profiles-and-remote-downloads.md` §3).
//!
//! The phone serves `GET /queue`, `POST /queue/claim`, `POST /queue/release`,
//! `POST /downloads/done` and `PUT /file`; the PC serves `GET /outbox?since=`. All of them need
//! `X-Link-Key`. A chapter is named by `(sourceId, mangaUrl, chapterUrl)`, and `sourceId` is a
//! string on the wire because source ids overflow a JS number.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};

/// Longest url or title accepted in a queue item.
const MAX_FIELD: usize = 4096;
/// Most queue items read from one answer; a real queue is a few hundred.
pub const MAX_QUEUE: usize = 5000;
/// Most events kept for one device; the oldest go first past this.
pub const MAX_OUTBOX: usize = 5000;

mod id_str {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &i64, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&v.to_string())
    }

    /// Takes a string, and also a plain number from a writer that sends one.
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Either {
            S(String),
            N(i64),
        }
        match Either::deserialize(d)? {
            Either::N(n) => Ok(n),
            Either::S(s) => s.trim().parse().map_err(serde::de::Error::custom),
        }
    }
}

/// One chapter, as both apps name it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterRef {
    #[serde(with = "id_str")]
    pub source_id: i64,
    pub manga_url: String,
    pub chapter_url: String,
}

impl ChapterRef {
    pub fn is_valid(&self) -> bool {
        !self.manga_url.is_empty()
            && !self.chapter_url.is_empty()
            && self.manga_url.len() <= MAX_FIELD
            && self.chapter_url.len() <= MAX_FIELD
    }
}

/// A chapter in the phone's queue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueItem {
    #[serde(flatten)]
    pub chapter: ChapterRef,
    #[serde(default)]
    pub manga_title: String,
    #[serde(default)]
    pub chapter_name: String,
    #[serde(default)]
    pub queued_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueueList {
    #[serde(default)]
    pub items: Vec<QueueItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimRequest {
    pub items: Vec<ChapterRef>,
    /// This PC's device id.
    pub by: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClaimResponse {
    #[serde(default)]
    pub claimed: Vec<ChapterRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseRequest {
    pub items: Vec<ChapterRef>,
}

/// Where a chapter the PC downloaded ended up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Where {
    /// On the NAS, in the phone's `RemoteMirror` layout.
    Server,
    /// Kept on the PC (`deviceId`).
    Device,
    /// Uploaded to the phone with `PUT /file`.
    Sent,
}

/// Tells the phone a chapter it queued is done.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoneEvent {
    #[serde(flatten)]
    pub chapter: ChapterRef,
    pub event_id: String,
    #[serde(rename = "where")]
    pub where_: Where,
    pub device_id: String,
    pub at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DoneRequest {
    pub events: Vec<DoneEvent>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DoneResponse {
    #[serde(default)]
    pub acked: Vec<String>,
}

/// The answer to `GET /outbox`: events after `since`, oldest first.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OutboxList {
    pub events: Vec<DoneEvent>,
}

/// A random event id (UUID v4 form).
pub fn new_event_id() -> String {
    let mut b = [0u8; 16];
    if getrandom::fill(&mut b).is_err() {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        b = t.to_le_bytes();
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        h.get(0..8).unwrap_or_default(),
        h.get(8..12).unwrap_or_default(),
        h.get(12..16).unwrap_or_default(),
        h.get(16..20).unwrap_or_default(),
        h.get(20..32).unwrap_or_default()
    )
}

/// Done events per linked device, kept until that device acks them (`outbox.json` in the
/// profile's folder). A damaged file starts empty: the phone's claims time out and it downloads
/// those chapters itself, so nothing is lost for good.
#[derive(Debug)]
pub struct Outbox {
    file: PathBuf,
    events: Mutex<HashMap<String, Vec<DoneEvent>>>,
}

impl Outbox {
    pub fn open(file: &Path) -> Self {
        let events = std::fs::read(file)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Self {
            file: file.to_path_buf(),
            events: Mutex::new(events),
        }
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, Vec<DoneEvent>>> {
        self.events.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn save(&self, map: &HashMap<String, Vec<DoneEvent>>) {
        let Ok(bytes) = serde_json::to_vec(map) else {
            return;
        };
        let tmp = self.file.with_extension("json.part");
        if let Err(e) = std::fs::write(&tmp, bytes).and_then(|()| std::fs::rename(&tmp, &self.file))
        {
            tracing::warn!("couldn't save the outbox: {e}");
        }
    }

    /// Adds `event` for `peer`. An event for the same chapter replaces the older one.
    pub fn add(&self, peer: &str, event: DoneEvent) {
        let mut map = self.lock();
        let list = map.entry(peer.to_owned()).or_default();
        list.retain(|e| e.chapter != event.chapter);
        list.push(event);
        if list.len() > MAX_OUTBOX {
            let extra = list.len() - MAX_OUTBOX;
            list.drain(..extra);
        }
        self.save(&map);
    }

    /// What `peer` hasn't acked yet, oldest first.
    pub fn pending(&self, peer: &str) -> Vec<DoneEvent> {
        self.lock().get(peer).cloned().unwrap_or_default()
    }

    /// Forgets the events `peer` acked.
    pub fn ack(&self, peer: &str, ids: &[String]) {
        if ids.is_empty() {
            return;
        }
        let mut map = self.lock();
        if let Some(list) = map.get_mut(peer) {
            list.retain(|e| !ids.contains(&e.event_id));
        }
        self.save(&map);
    }

    /// For `GET /outbox?since=`: the events after `since`, or all of them when `since` is
    /// missing or unknown. The events up to `since` were seen, so they are forgotten.
    pub fn after(&self, peer: &str, since: Option<&str>) -> Vec<DoneEvent> {
        let mut map = self.lock();
        let Some(list) = map.get_mut(peer) else {
            return Vec::new();
        };
        if let Some(pos) = since.and_then(|s| list.iter().position(|e| e.event_id == s)) {
            list.drain(..=pos);
            let out = list.clone();
            self.save(&map);
            return out;
        }
        list.clone()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn ev(id: &str, ch: &str) -> DoneEvent {
        DoneEvent {
            chapter: ChapterRef {
                source_id: 1,
                manga_url: "/m".into(),
                chapter_url: ch.into(),
            },
            event_id: id.into(),
            where_: Where::Device,
            device_id: "pc".into(),
            at: 1,
        }
    }

    #[test]
    fn wire_shapes_match_the_contract() {
        let item: QueueItem = serde_json::from_str(
            r#"{"sourceId":"9223372036854775807","mangaUrl":"/m","chapterUrl":"/c","mangaTitle":"T","chapterName":"C","queuedAt":5}"#,
        )
        .unwrap();
        assert_eq!(item.chapter.source_id, i64::MAX);
        let e = serde_json::to_value(ev("e1", "/c")).unwrap();
        assert_eq!(e["sourceId"], "1");
        assert_eq!(e["where"], "device");
        assert_eq!(e["eventId"], "e1");
        // A number for the id is read too.
        let r: ChapterRef =
            serde_json::from_str(r#"{"sourceId":7,"mangaUrl":"/m","chapterUrl":"/c"}"#).unwrap();
        assert_eq!(r.source_id, 7);
        assert!(serde_json::from_str::<ChapterRef>(
            r#"{"sourceId":"x","mangaUrl":"/m","chapterUrl":"/c"}"#
        )
        .is_err());
    }

    #[test]
    fn the_outbox_keeps_events_until_acked_or_seen() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("outbox.json");
        let o = Outbox::open(&file);
        o.add("phone", ev("e1", "/1"));
        o.add("phone", ev("e2", "/2"));
        o.add("phone", ev("e3", "/3"));
        // Saved, and read back.
        let o = Outbox::open(&file);
        assert_eq!(o.pending("phone").len(), 3);
        assert!(o.pending("other").is_empty());
        o.ack("phone", &["e2".into()]);
        let ids = |v: Vec<DoneEvent>| v.into_iter().map(|e| e.event_id).collect::<Vec<_>>();
        assert_eq!(ids(o.after("phone", None)), ["e1", "e3"]);
        assert_eq!(ids(o.after("phone", Some("unknown"))), ["e1", "e3"]);
        assert_eq!(ids(o.after("phone", Some("e1"))), ["e3"]);
        assert_eq!(ids(o.pending("phone")), ["e3"]);
        // A newer event for the same chapter replaces the old one.
        o.add("phone", ev("e4", "/3"));
        assert_eq!(ids(o.pending("phone")), ["e4"]);
    }

    #[test]
    fn event_ids_are_uuid_shaped_and_differ() {
        let (a, b) = (new_event_id(), new_event_id());
        assert_eq!(a.len(), 36);
        assert_ne!(a, b);
    }
}
