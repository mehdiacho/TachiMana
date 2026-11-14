//! Linked devices: other installs of the same profile that this one talks to without the
//! six-digit code (`docs/rust-port/profiles-and-remote-downloads.md` §2).
//!
//! A link is made once with `POST /link` and the pairing code. Both sides then keep a random key
//! and send it as `X-Link-Key`. Links belong to a profile, so the file lives in the profile's
//! folder (`links.json`).

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};

/// A linked device.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Link {
    pub peer_device_id: String,
    pub peer_name: String,
    pub profile_id: String,
    /// `"android"` or `"desktop"`.
    pub platform: String,
    /// The shared secret, base64url without padding.
    pub key: String,
    /// `host:port` it was last reached at.
    pub last_address: String,
    /// Unix milliseconds.
    pub linked_at: i64,
    /// When it was last heard from or seen on the network, Unix milliseconds (0 = not since
    /// linking). Saved at most once a minute, so it is that coarse.
    #[serde(default)]
    pub last_seen: i64,
    /// Who leads (`roles/1`), from this device's point of view. Links made before roles are
    /// two-way.
    #[serde(default)]
    pub mode: crate::roles::Mode,
    /// When `mode` was last changed, on the device that changed it (0 = never).
    #[serde(default)]
    pub mode_at: i64,
}

impl std::fmt::Debug for Link {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Link")
            .field("peer_device_id", &self.peer_device_id)
            .field("peer_name", &self.peer_name)
            .field("profile_id", &self.profile_id)
            .field("platform", &self.platform)
            .field("key", &"<redacted>")
            .field("last_address", &self.last_address)
            .field("last_seen", &self.last_seen)
            .field("mode", &self.mode)
            .finish()
    }
}

/// How stale the saved "last seen" time may get before it is written again.
const SEEN_EVERY_MS: i64 = 60_000;

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// Most links one profile keeps; a new one past this replaces the oldest.
pub const MAX_LINKS: usize = 16;

#[derive(Debug)]
pub struct LinkStore {
    file: PathBuf,
    links: Mutex<Vec<Link>>,
}

impl LinkStore {
    /// Reads `file`. A missing or damaged file means no links (the other side then gets 401 and
    /// links again); it never stops sync from starting.
    pub fn open(file: &Path) -> Self {
        let links = std::fs::read(file)
            .ok()
            .and_then(|b| serde_json::from_slice::<Vec<Link>>(&b).ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|l| is_key(&l.key) && !l.peer_device_id.is_empty())
            .take(MAX_LINKS)
            .collect();
        Self {
            file: file.to_owned(),
            links: Mutex::new(links),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Link>> {
        // Replaced whole under the lock, so a poisoned guard still holds a consistent list.
        self.links.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn list(&self) -> Vec<Link> {
        self.lock().clone()
    }

    /// The link whose key is `given`, compared without an early exit.
    pub fn by_key(&self, given: &str) -> Option<Link> {
        self.lock()
            .iter()
            .find(|l| same(l.key.as_bytes(), given.as_bytes()))
            .cloned()
    }

    pub fn by_peer(&self, peer_device_id: &str) -> Option<Link> {
        self.lock()
            .iter()
            .find(|l| l.peer_device_id == peer_device_id)
            .cloned()
    }

    /// Adds `link`, replacing an older link to the same device, and saves.
    pub fn put(&self, link: Link) -> std::io::Result<()> {
        let mut g = self.lock();
        let mut next: Vec<Link> = g
            .iter()
            .filter(|l| l.peer_device_id != link.peer_device_id)
            .cloned()
            .collect();
        next.push(link);
        if next.len() > MAX_LINKS {
            next.sort_by_key(|l| std::cmp::Reverse(l.linked_at));
            next.truncate(MAX_LINKS);
        }
        save(&self.file, &next)?;
        *g = next;
        Ok(())
    }

    /// Records where a linked device was just reached.
    pub fn seen_at(&self, peer_device_id: &str, address: &str) {
        self.heard(peer_device_id, Some(address), now_ms());
    }

    /// Notes that `peer_device_id` called in with its key just now.
    pub fn touch(&self, peer_device_id: &str) {
        self.heard(peer_device_id, None, now_ms());
    }

    /// Records the time (and address) a linked device was last heard from. The file is written
    /// when the address changes or the saved time is a minute old, not on every request.
    fn heard(&self, peer_device_id: &str, address: Option<&str>, now: i64) {
        let mut g = self.lock();
        let mut next = g.clone();
        let Some(l) = next.iter_mut().find(|l| l.peer_device_id == peer_device_id) else {
            return;
        };
        let moved = address.is_some_and(|a| !a.is_empty() && l.last_address != a);
        if !moved && now.saturating_sub(l.last_seen) < SEEN_EVERY_MS {
            return;
        }
        if let Some(a) = address.filter(|_| moved) {
            a.clone_into(&mut l.last_address);
        }
        l.last_seen = now;
        if save(&self.file, &next).is_ok() {
            *g = next;
        }
    }

    /// Sets who leads on the link to `peer_device_id` when `at` is later than the stored change.
    /// Returns the link as it is afterwards, with `true` when it changed; `None` when there is no
    /// such link.
    pub fn set_mode(
        &self,
        peer_device_id: &str,
        mode: crate::roles::Mode,
        at: i64,
    ) -> std::io::Result<Option<(Link, bool)>> {
        let mut g = self.lock();
        let mut next = g.clone();
        let Some(l) = next.iter_mut().find(|l| l.peer_device_id == peer_device_id) else {
            return Ok(None);
        };
        if at <= l.mode_at {
            return Ok(Some((l.clone(), false)));
        }
        l.mode = mode;
        l.mode_at = at;
        let out = l.clone();
        save(&self.file, &next)?;
        *g = next;
        Ok(Some((out, true)))
    }

    /// Forgets the link to `peer_device_id`. `false` when there was none.
    pub fn remove(&self, peer_device_id: &str) -> std::io::Result<bool> {
        let mut g = self.lock();
        let next: Vec<Link> = g
            .iter()
            .filter(|l| l.peer_device_id != peer_device_id)
            .cloned()
            .collect();
        if next.len() == g.len() {
            return Ok(false);
        }
        save(&self.file, &next)?;
        *g = next;
        Ok(true)
    }
}

fn save(file: &Path, links: &[Link]) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(links).map_err(std::io::Error::other)?;
    let mut tmp = file.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, file)
}

fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// 32 random bytes as base64url without padding (43 characters). `None` when the OS has no
/// randomness: a link key is a long-term secret, so there is no weak fallback.
pub fn new_key() -> Option<String> {
    let mut b = [0u8; 32];
    getrandom::fill(&mut b).ok()?;
    Some(base64url(&b))
}

/// 43 characters of the base64url alphabet.
pub fn is_key(k: &str) -> bool {
    k.len() == 43
        && k.bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

fn base64url(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().saturating_mul(4) / 3 + 3);
    for chunk in bytes.chunks(3) {
        let b0 = chunk.first().copied().unwrap_or(0);
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        let n = (u32::from(b0) << 16) | (u32::from(b1) << 8) | u32::from(b2);
        let take = chunk.len() + 1;
        for i in 0..take {
            let idx = usize::try_from((n >> (18 - 6 * i)) & 63).unwrap_or(0);
            if let Some(&c) = A.get(idx) {
                out.push(char::from(c));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
    use super::*;

    fn link(peer: &str, key: &str) -> Link {
        Link {
            peer_device_id: peer.into(),
            peer_name: "Phone".into(),
            profile_id: "p".into(),
            platform: "android".into(),
            key: key.into(),
            last_address: "10.0.0.2:27193".into(),
            linked_at: 1,
            last_seen: 0,
            mode: crate::roles::Mode::Both,
            mode_at: 0,
        }
    }

    #[test]
    fn keys_are_43_base64url_chars_and_differ() {
        let a = new_key().unwrap();
        assert!(is_key(&a), "{a}");
        assert_ne!(a, new_key().unwrap());
        assert_eq!(base64url(b"\xff\xff\xff"), "____");
        assert_eq!(base64url(b"Ma"), "TWE");
        assert!(!is_key("123456"));
    }

    #[test]
    fn put_find_replace_remove_and_reload() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("links.json");
        let s = LinkStore::open(&f);
        let k1 = new_key().unwrap();
        let k2 = new_key().unwrap();
        s.put(link("phone", &k1)).unwrap();
        assert_eq!(s.by_key(&k1).unwrap().peer_device_id, "phone");
        assert!(s.by_key(&k2).is_none());
        assert!(s.by_key("").is_none());
        s.put(link("phone", &k2)).unwrap();
        assert!(s.by_key(&k1).is_none(), "relinking replaces the old key");
        assert_eq!(s.list().len(), 1);
        s.seen_at("phone", "10.0.0.9:27193");
        let back = LinkStore::open(&f);
        assert_eq!(
            back.by_peer("phone").unwrap().last_address,
            "10.0.0.9:27193"
        );
        assert!(back.remove("phone").unwrap());
        assert!(!back.remove("phone").unwrap());
        assert!(LinkStore::open(&f).list().is_empty());
    }

    #[test]
    fn a_damaged_file_means_no_links_and_debug_hides_keys() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("links.json");
        std::fs::write(&f, b"[{\"nope\":1}").unwrap();
        assert!(LinkStore::open(&f).list().is_empty());
        let k = new_key().unwrap();
        assert!(!format!("{:?}", link("x", &k)).contains(&k));
    }

    #[test]
    fn last_seen_is_saved_at_most_once_a_minute_unless_the_address_moves() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("links.json");
        let s = LinkStore::open(&f);
        s.put(link("phone", &new_key().unwrap())).unwrap();
        s.heard("phone", None, 100_000);
        assert_eq!(
            LinkStore::open(&f).by_peer("phone").unwrap().last_seen,
            100_000
        );
        s.heard("phone", None, 130_000);
        assert_eq!(
            LinkStore::open(&f).by_peer("phone").unwrap().last_seen,
            100_000
        );
        s.heard("phone", Some("10.0.0.7:1"), 131_000);
        let back = LinkStore::open(&f).by_peer("phone").unwrap();
        assert_eq!(
            (back.last_seen, back.last_address.as_str()),
            (131_000, "10.0.0.7:1")
        );
        s.heard("phone", Some(""), 500_000);
        assert_eq!(
            LinkStore::open(&f).by_peer("phone").unwrap().last_address,
            "10.0.0.7:1"
        );
        s.heard("nobody", None, 900_000);
        assert_eq!(LinkStore::open(&f).list().len(), 1);
    }

    #[test]
    fn a_file_from_before_last_seen_still_loads() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("links.json");
        let key = new_key().unwrap();
        std::fs::write(
            &f,
            format!(r#"[{{"peerDeviceId":"p","peerName":"P","profileId":"x","platform":"android","key":"{key}","lastAddress":"a:1","linkedAt":5}}]"#),
        )
        .unwrap();
        assert_eq!(LinkStore::open(&f).by_peer("p").unwrap().last_seen, 0);
    }

    #[test]
    fn a_newer_mode_replaces_an_older_one_and_is_saved() {
        use crate::roles::Mode;
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("links.json");
        let s = LinkStore::open(&f);
        s.put(link("phone", &new_key().unwrap())).unwrap();
        let (l, changed) = s.set_mode("phone", Mode::TheyLead, 50).unwrap().unwrap();
        assert!(changed && l.mode == Mode::TheyLead);
        let (l, changed) = s.set_mode("phone", Mode::Both, 40).unwrap().unwrap();
        assert!(
            !changed && l.mode == Mode::TheyLead,
            "an older change loses"
        );
        assert_eq!(
            LinkStore::open(&f).by_peer("phone").unwrap().mode,
            Mode::TheyLead
        );
        assert!(s.set_mode("nobody", Mode::Both, 99).unwrap().is_none());
    }
}
