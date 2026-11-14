//! The change journal of `changes/1` (`docs/protocol/capabilities/changes-1.md`).
//!
//! Every change the person makes on this device is written here, in order, and so is every entry
//! received from a linked device. A linked device asks for the entries after its cursor
//! (`GET /changes`) or is sent them (`POST /changes`). [`decide`] holds the "who wins" rules; it is
//! pure so the test files in `docs/protocol/vectors/changes-1/` can run against it.
//!
//! The journal is its own SQLite file in the profile's folder (`journal.db`), not a table in the
//! library database, whose schema belongs to the Mihon/Kotlin lineage.

use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Entries younger than this are never dropped.
pub const KEEP_MS: i64 = 90 * 24 * 60 * 60 * 1000;
/// Most entries one `GET /changes` answers with.
pub const MAX_LIMIT: usize = 2000;
pub const DEFAULT_LIMIT: usize = 500;

pub const MANGA_LIBRARY: &str = "manga.library";
pub const MANGA_CATEGORIES: &str = "manga.categories";
pub const CATEGORY_SET: &str = "category.set";
pub const CATEGORY_RENAME: &str = "category.rename";
pub const CATEGORY_DELETE: &str = "category.delete";
pub const CHAPTER_READ: &str = "chapter.read";
pub const CHAPTER_BOOKMARK: &str = "chapter.bookmark";
pub const HISTORY_READ: &str = "history.read";
pub const MANGA_VIEWER: &str = "manga.viewer";

/// Every kind this app understands. Others are stored and passed on, never applied.
pub const KINDS: &[&str] = &[
    MANGA_LIBRARY,
    MANGA_CATEGORIES,
    CATEGORY_SET,
    CATEGORY_RENAME,
    CATEGORY_DELETE,
    CHAPTER_READ,
    CHAPTER_BOOKMARK,
    HISTORY_READ,
    MANGA_VIEWER,
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub change_id: String,
    pub device_id: String,
    pub at: i64,
    /// Position in the serving device's journal; ignored on the way in.
    #[serde(default)]
    pub seq: i64,
    pub kind: String,
    pub target: Value,
    #[serde(default)]
    pub value: Value,
}

/// What this device holds for one `(kind, target)`.
#[derive(Debug, Clone, PartialEq)]
pub struct Held {
    pub at: i64,
    pub change_id: String,
    pub value: Value,
}

/// The outcome of [`decide`]: the value to hold (and apply) from now on, or nothing to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    Apply(Value),
    Ignore,
}

#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    #[error("journal database: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("journal entry: {0}")]
    Json(#[from] serde_json::Error),
    #[error("journal lock poisoned")]
    Poisoned,
    #[error("the cursor is older than the oldest entry kept ({oldest})")]
    CursorTooOld { oldest: i64 },
}

pub type Result<T> = std::result::Result<T, JournalError>;

/// The identity of a target inside its kind: its names joined in a fixed order, so the same
/// manga or chapter always gives the same key whatever order the JSON keys came in.
pub fn target_key(target: &Value) -> String {
    ["sourceId", "mangaUrl", "chapterUrl", "name"]
        .iter()
        .filter_map(|k| match target.get(*k) {
            Some(Value::String(s)) => Some(format!("{k}={s}")),
            Some(Value::Number(n)) => Some(format!("{k}={n}")),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\u{1f}")
}

fn bool_of(v: &Value, k: &str) -> Option<bool> {
    v.get(k).and_then(Value::as_bool)
}

fn int_of(v: &Value, k: &str) -> Option<i64> {
    v.get(k).and_then(Value::as_i64)
}

/// The "who wins" rules of `changes/1`, for an entry that is not a repeat.
pub fn decide(held: Option<&Held>, e: &Entry) -> Verdict {
    let Some(h) = held else {
        return Verdict::Apply(e.value.clone());
    };
    if e.kind == HISTORY_READ {
        // Merged, never replaced: the later time, and the reading time added once per entry.
        let last = int_of(&h.value, "lastRead")
            .unwrap_or(0)
            .max(int_of(&e.value, "lastRead").unwrap_or(0));
        let time = int_of(&h.value, "timeRead")
            .unwrap_or(0)
            .saturating_add(int_of(&e.value, "timeRead").unwrap_or(0).max(0));
        return Verdict::Apply(json!({"lastRead": last, "timeRead": time}));
    }
    let newer = match e.at.cmp(&h.at) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => tie_goes_to_incoming(h, e),
    };
    if e.kind == MANGA_LIBRARY
        && bool_of(&h.value, "inLibrary") == Some(true)
        && bool_of(&e.value, "inLibrary") == Some(true)
    {
        // Both say "in the library": the winner's details, the earlier date added.
        let held_date = int_of(&h.value, "dateAdded").unwrap_or(i64::MAX);
        let new_date = int_of(&e.value, "dateAdded").unwrap_or(i64::MAX);
        let date = held_date.min(new_date);
        let mut v = if newer {
            e.value.clone()
        } else {
            h.value.clone()
        };
        if let (Some(obj), true) = (v.as_object_mut(), date != i64::MAX) {
            obj.insert("dateAdded".into(), json!(date));
        }
        return if newer || date < held_date {
            Verdict::Apply(v)
        } else {
            Verdict::Ignore
        };
    }
    if newer {
        Verdict::Apply(e.value.clone())
    } else {
        Verdict::Ignore
    }
}

fn tie_goes_to_incoming(h: &Held, e: &Entry) -> bool {
    if e.kind == CHAPTER_READ {
        let (hr, er) = (bool_of(&h.value, "read"), bool_of(&e.value, "read"));
        if hr != er {
            return er == Some(true);
        }
        let (hp, ep) = (
            int_of(&h.value, "lastPageRead"),
            int_of(&e.value, "lastPageRead"),
        );
        if hp != ep {
            return ep.unwrap_or(0) > hp.unwrap_or(0);
        }
    }
    if e.kind == MANGA_LIBRARY {
        let (hi, ei) = (
            bool_of(&h.value, "inLibrary"),
            bool_of(&e.value, "inLibrary"),
        );
        if hi != ei {
            return ei == Some(true);
        }
    }
    e.change_id > h.change_id
}

/// A page of entries after a cursor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangesPage {
    pub device_id: String,
    pub changes: Vec<Entry>,
    pub next: i64,
    pub more: bool,
}

/// Where this device is with one linked device.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Cursor {
    /// The peer's `seq` this device has pulled up to.
    pub pulled: i64,
    /// This device's own `seq` it has pushed up to.
    pub pushed: i64,
}

#[derive(Debug)]
pub struct Journal {
    conn: Mutex<Connection>,
    device_id: String,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS entries(
  seq INTEGER PRIMARY KEY AUTOINCREMENT,
  change_id TEXT NOT NULL UNIQUE,
  device_id TEXT NOT NULL,
  at INTEGER NOT NULL,
  kind TEXT NOT NULL,
  target TEXT NOT NULL,
  value TEXT NOT NULL,
  recorded_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS entries_recorded ON entries(recorded_at);
CREATE TABLE IF NOT EXISTS held(
  kind TEXT NOT NULL,
  target TEXT NOT NULL,
  at INTEGER NOT NULL,
  change_id TEXT NOT NULL,
  value TEXT NOT NULL,
  PRIMARY KEY(kind, target)
);
CREATE TABLE IF NOT EXISTS cursors(
  peer TEXT PRIMARY KEY,
  pulled INTEGER NOT NULL DEFAULT 0,
  pushed INTEGER NOT NULL DEFAULT 0
);
";

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

impl Journal {
    pub fn open(path: &Path, device_id: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        let _mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
        Self::with(conn, device_id)
    }

    pub fn open_in_memory(device_id: &str) -> Result<Self> {
        Self::with(Connection::open_in_memory()?, device_id)
    }

    fn with(conn: Connection, device_id: &str) -> Result<Self> {
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: Mutex::new(conn),
            device_id: device_id.to_owned(),
        })
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    fn lock(&self) -> Result<MutexGuard<'_, Connection>> {
        self.conn.lock().map_err(|_| JournalError::Poisoned)
    }

    /// Writes down a change the person made here, now.
    pub fn record(&self, kind: &str, target: Value, value: Value) -> Result<Entry> {
        let e = Entry {
            change_id: crate::queue::new_event_id(),
            device_id: self.device_id.clone(),
            at: now_ms(),
            seq: 0,
            kind: kind.to_owned(),
            target,
            value,
        };
        let mut c = self.lock()?;
        let tx = c.transaction()?;
        let held = held_of(&tx, &e)?;
        let keep = match decide(held.as_ref(), &e) {
            Verdict::Apply(v) => v,
            // A local change is the newest by definition; only a clock set backwards lands here.
            Verdict::Ignore => e.value.clone(),
        };
        let seq = insert(&tx, &e)?;
        put_held(&tx, &e, &keep)?;
        tx.commit()?;
        Ok(Entry { seq, ..e })
    }

    /// Takes an entry from a linked device. `Ok(Some(value))` is the value to apply to the
    /// library; `Ok(None)` means it was a repeat or lost to what is held. Either way it is kept
    /// in the journal (once), so a third device can be given it.
    pub fn receive(&self, e: &Entry) -> Result<Option<Value>> {
        let mut c = self.lock()?;
        let tx = c.transaction()?;
        let seen: Option<i64> = tx
            .query_row(
                "SELECT seq FROM entries WHERE change_id = ?1",
                [&e.change_id],
                |r| r.get(0),
            )
            .optional()?;
        if seen.is_some() {
            return Ok(None);
        }
        let held = held_of(&tx, e)?;
        let verdict = decide(held.as_ref(), e);
        insert(&tx, e)?;
        let out = match verdict {
            Verdict::Apply(v) => {
                put_held(&tx, e, &v)?;
                Some(v)
            }
            Verdict::Ignore => None,
        };
        tx.commit()?;
        Ok(out)
    }

    /// The entries after `after`, oldest first, leaving out those made on `exclude`.
    pub fn after(&self, after: i64, exclude: &str, limit: usize) -> Result<ChangesPage> {
        let limit = limit.clamp(1, MAX_LIMIT);
        let c = self.lock()?;
        let oldest: Option<i64> = c.query_row("SELECT MIN(seq) FROM entries", [], |r| r.get(0))?;
        let last: Option<i64> = c.query_row("SELECT MAX(seq) FROM entries", [], |r| r.get(0))?;
        // A gap between the cursor and what is kept means entries were dropped unseen.
        if let Some(o) = oldest {
            if after > 0 && after.saturating_add(1) < o {
                return Err(JournalError::CursorTooOld {
                    oldest: o.saturating_sub(1),
                });
            }
        }
        let mut st = c.prepare(
            "SELECT seq, change_id, device_id, at, kind, target, value FROM entries \
             WHERE seq > ?1 ORDER BY seq LIMIT ?2",
        )?;
        let rows = st.query_map(params![after, i64::try_from(limit).unwrap_or(500)], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
            ))
        })?;
        let mut changes = Vec::new();
        let mut next = after;
        for row in rows {
            let (seq, change_id, device_id, at, kind, target, value) = row?;
            next = seq;
            if device_id == exclude {
                continue;
            }
            changes.push(Entry {
                change_id,
                device_id,
                at,
                seq,
                kind,
                target: serde_json::from_str(&target)?,
                value: serde_json::from_str(&value)?,
            });
        }
        Ok(ChangesPage {
            device_id: self.device_id.clone(),
            changes,
            next,
            more: next < last.unwrap_or(0),
        })
    }

    /// The last `seq` written (0 when empty).
    pub fn head(&self) -> Result<i64> {
        let c = self.lock()?;
        Ok(
            c.query_row("SELECT COALESCE(MAX(seq), 0) FROM entries", [], |r| {
                r.get(0)
            })?,
        )
    }

    pub fn cursor(&self, peer: &str) -> Result<Cursor> {
        let c = self.lock()?;
        Ok(c.query_row(
            "SELECT pulled, pushed FROM cursors WHERE peer = ?1",
            [peer],
            |r| {
                Ok(Cursor {
                    pulled: r.get(0)?,
                    pushed: r.get(1)?,
                })
            },
        )
        .optional()?
        .unwrap_or_default())
    }

    pub fn set_cursor(&self, peer: &str, cur: Cursor) -> Result<()> {
        let c = self.lock()?;
        c.execute(
            "INSERT INTO cursors(peer, pulled, pushed) VALUES (?1, ?2, ?3) \
             ON CONFLICT(peer) DO UPDATE SET pulled = excluded.pulled, pushed = excluded.pushed",
            params![peer, cur.pulled, cur.pushed],
        )?;
        Ok(())
    }

    /// Drops entries recorded before `before` (keeps what `held` needs for the rules).
    pub fn prune(&self, before: i64) -> Result<usize> {
        let c = self.lock()?;
        Ok(c.execute("DELETE FROM entries WHERE recorded_at < ?1", [before])?)
    }
}

fn held_of(c: &Connection, e: &Entry) -> Result<Option<Held>> {
    let row: Option<(i64, String, String)> = c
        .query_row(
            "SELECT at, change_id, value FROM held WHERE kind = ?1 AND target = ?2",
            params![e.kind, target_key(&e.target)],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    match row {
        None => Ok(None),
        Some((at, change_id, value)) => Ok(Some(Held {
            at,
            change_id,
            value: serde_json::from_str(&value)?,
        })),
    }
}

fn insert(c: &Connection, e: &Entry) -> Result<i64> {
    c.execute(
        "INSERT INTO entries(change_id, device_id, at, kind, target, value, recorded_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            e.change_id,
            e.device_id,
            e.at,
            e.kind,
            serde_json::to_string(&e.target)?,
            serde_json::to_string(&e.value)?,
            now_ms()
        ],
    )?;
    Ok(c.last_insert_rowid())
}

fn put_held(c: &Connection, e: &Entry, value: &Value) -> Result<()> {
    // The held time only moves forward: a merged older entry does not make the value look older.
    c.execute(
        "INSERT INTO held(kind, target, at, change_id, value) VALUES (?1, ?2, ?3, ?4, ?5) \
         ON CONFLICT(kind, target) DO UPDATE SET \
           change_id = CASE WHEN excluded.at >= held.at THEN excluded.change_id ELSE held.change_id END, \
           at = MAX(held.at, excluded.at), value = excluded.value",
        params![
            e.kind,
            target_key(&e.target),
            e.at,
            e.change_id,
            serde_json::to_string(value)?
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    struct Want {
        values: Vec<WantValue>,
        applied: usize,
        ignored: usize,
    }
    #[derive(Deserialize)]
    struct WantValue {
        kind: String,
        target: Value,
        value: Value,
    }
    #[derive(Deserialize)]
    struct Case {
        held: Vec<Entry>,
        incoming: Vec<Entry>,
        expect: Want,
    }

    /// Runs every file in docs/protocol/vectors/changes-1 against a fresh journal.
    #[test]
    fn protocol_test_files_pass() {
        let dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs/protocol/vectors/changes-1");
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        files.sort();
        assert!(files.len() >= 6, "{files:?}");
        for f in files {
            let case: Case = serde_json::from_slice(&std::fs::read(&f).unwrap()).unwrap();
            let j = Journal::open_in_memory("me").unwrap();
            for h in &case.held {
                assert!(
                    j.receive(h).unwrap().is_some(),
                    "{f:?}: held entry not taken"
                );
            }
            let (mut applied, mut ignored) = (0, 0);
            for e in &case.incoming {
                match j.receive(e).unwrap() {
                    Some(_) => applied += 1,
                    None => ignored += 1,
                }
            }
            assert_eq!(
                (applied, ignored),
                (case.expect.applied, case.expect.ignored),
                "{f:?}"
            );
            for w in &case.expect.values {
                let probe = Entry {
                    change_id: String::new(),
                    device_id: String::new(),
                    at: 0,
                    seq: 0,
                    kind: w.kind.clone(),
                    target: w.target.clone(),
                    value: Value::Null,
                };
                let c = j.lock().unwrap();
                let held = held_of(&c, &probe).unwrap().unwrap();
                for (k, v) in w.value.as_object().unwrap() {
                    assert_eq!(held.value.get(k), Some(v), "{f:?}: {k}");
                }
            }
        }
    }

    #[test]
    fn after_skips_the_callers_own_and_pages() {
        let j = Journal::open_in_memory("pc").unwrap();
        let t = json!({"sourceId": "1", "mangaUrl": "/m"});
        for i in 0..5 {
            j.record(MANGA_VIEWER, t.clone(), json!({"viewer": i}))
                .unwrap();
        }
        let theirs = Entry {
            change_id: "x".into(),
            device_id: "phone".into(),
            at: now_ms() + 10_000,
            seq: 99,
            kind: CHAPTER_BOOKMARK.into(),
            target: json!({"sourceId": "1", "mangaUrl": "/m", "chapterUrl": "/c"}),
            value: json!({"bookmark": true}),
        };
        assert!(j.receive(&theirs).unwrap().is_some());
        let p = j.after(0, "phone", 3).unwrap();
        assert_eq!((p.changes.len(), p.next, p.more), (3, 3, true));
        let p = j.after(p.next, "phone", 3).unwrap();
        // Seq 6 is the phone's own entry: skipped, but the cursor moves past it.
        assert_eq!((p.changes.len(), p.next, p.more), (2, 6, false));
        let p = j.after(6, "phone", 3).unwrap();
        assert_eq!((p.changes.len(), p.next, p.more), (0, 6, false));
        // A third device does get the phone's entry, with its origin.
        let p = j.after(5, "tablet", 10).unwrap();
        assert_eq!(
            p.changes.first().map(|e| e.device_id.as_str()),
            Some("phone")
        );
    }

    #[test]
    fn a_cursor_behind_what_was_pruned_is_too_old() {
        let j = Journal::open_in_memory("pc").unwrap();
        let t = json!({"sourceId": "1", "mangaUrl": "/m"});
        for i in 0..4 {
            j.record(MANGA_VIEWER, t.clone(), json!({"viewer": i}))
                .unwrap();
        }
        j.lock()
            .unwrap()
            .execute("DELETE FROM entries WHERE seq <= 2", [])
            .unwrap();
        assert!(matches!(
            j.after(1, "x", 10),
            Err(JournalError::CursorTooOld { oldest: 2 })
        ));
        assert_eq!(j.after(2, "x", 10).unwrap().changes.len(), 2);
        // A device that never pulled starts from what is kept.
        assert_eq!(j.after(0, "x", 10).unwrap().changes.len(), 2);
    }

    #[test]
    fn cursors_round_trip() {
        let j = Journal::open_in_memory("pc").unwrap();
        assert_eq!(j.cursor("phone").unwrap(), Cursor::default());
        j.set_cursor(
            "phone",
            Cursor {
                pulled: 7,
                pushed: 3,
            },
        )
        .unwrap();
        assert_eq!(
            j.cursor("phone").unwrap(),
            Cursor {
                pulled: 7,
                pushed: 3
            }
        );
    }

    #[test]
    fn target_keys_ignore_key_order() {
        let a = json!({"sourceId": "1", "mangaUrl": "/m", "chapterUrl": "/c"});
        let b = json!({"chapterUrl": "/c", "mangaUrl": "/m", "sourceId": "1"});
        assert_eq!(target_key(&a), target_key(&b));
        assert_ne!(
            target_key(&a),
            target_key(&json!({"sourceId": "1", "mangaUrl": "/m"}))
        );
    }
}
