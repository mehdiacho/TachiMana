//! Wire types of the LAN sync protocol (`SyncTransport.kt`, `LanSyncServer.kt`). JSON keys are
//! camelCase to match the Kotlin app; both apps speak the same protocol.
//!
//! Routes (HTTP/1.1, plain text on the LAN):
//! - `GET /ping` -> [`PingInfo`]. No pairing code needed.
//! - `GET /manifest` -> [`SyncManifest`].
//! - `GET /backup` -> the `.tachibk` container bytes.
//! - `GET /file?path=<manifest path>` -> the file's bytes.
//!
//! - `POST /link` with [`LinkRequest`] -> [`LinkResponse`]. Needs the pairing code; answers 409
//!   with the server's profile (no key) when the profiles differ.
//!
//! Every route but `/ping` needs the pairing code in the [`PAIR_HEADER`] header, or the key of a
//! link made with `/link` in the [`LINK_HEADER`] header
//! (`docs/rust-port/profiles-and-remote-downloads.md` §2).

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const PAIR_HEADER: &str = "X-Pair-Code";
pub const LINK_HEADER: &str = "X-Link-Key";
pub const DEFAULT_PORT: u16 = 27193;
pub const PORT_ATTEMPTS: u16 = 20;
pub const SERVICE_TYPE: &str = "_tachimana._tcp.local.";

/// A peer found by mDNS or entered by hand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerDevice {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
}

impl PeerDevice {
    /// `host:port`, with brackets around an IPv6 host.
    pub fn address(&self) -> String {
        if self.host.contains(':') && !self.host.starts_with('[') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PingInfo {
    pub device_id: String,
    pub device_name: String,
    /// The profile this server serves; absent on older apps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_name: Option<String>,
}

/// Body of `POST /link`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkRequest {
    pub device_id: String,
    pub device_name: String,
    pub profile_id: String,
    pub profile_name: String,
    /// `"android"` or `"desktop"`.
    pub platform: String,
    /// The port the asking device serves on, so this side can reach it back. Optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
}

/// Answer to `POST /link`. `key` is set on 200 and absent on 409 (other profile).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkResponse {
    pub device_id: String,
    pub device_name: String,
    pub profile_id: String,
    pub profile_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

impl std::fmt::Debug for LinkResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinkResponse")
            .field("device_id", &self.device_id)
            .field("device_name", &self.device_name)
            .field("profile_id", &self.profile_id)
            .field("profile_name", &self.profile_name)
            .field("key", &self.key.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

/// Longest id, name or platform accepted in a link request.
pub const MAX_LINK_FIELD: usize = 128;

impl LinkRequest {
    /// Every field present, not too long, and a known platform.
    pub fn is_valid(&self) -> bool {
        let ok = |s: &str| !s.trim().is_empty() && s.chars().count() <= MAX_LINK_FIELD;
        ok(&self.device_id)
            && ok(&self.device_name)
            && ok(&self.profile_id)
            && ok(&self.profile_name)
            && matches!(self.platform.as_str(), "android" | "desktop")
    }
}

/// What a `tachimana://link?…` code carries (shown as a QR code and as text).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkInvite {
    /// `host:port`.
    pub address: String,
    pub device_id: String,
    pub profile_id: String,
    pub profile_name: String,
    pub code: String,
}

const LINK_URI_PREFIX: &str = "tachimana://link?";

impl LinkInvite {
    pub fn to_uri(&self) -> String {
        let enc = |s: &str| {
            percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string()
        };
        format!(
            "{LINK_URI_PREFIX}h={}&d={}&p={}&n={}&c={}",
            enc(&self.address),
            enc(&self.device_id),
            enc(&self.profile_id),
            enc(&self.profile_name),
            enc(&self.code)
        )
    }

    /// Parses a link code. `None` unless the address, device id, profile id and code are all
    /// there. `+` reads as a space, as some encoders write it.
    pub fn parse(uri: &str) -> Option<Self> {
        let query = uri.trim().strip_prefix(LINK_URI_PREFIX)?;
        if query.len() > 4096 {
            return None;
        }
        let mut inv = LinkInvite {
            address: String::new(),
            device_id: String::new(),
            profile_id: String::new(),
            profile_name: String::new(),
            code: String::new(),
        };
        for pair in query.split('&') {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            let v = v.replace('+', " ");
            let v = percent_encoding::percent_decode_str(&v)
                .decode_utf8()
                .ok()?
                .into_owned();
            match k {
                "h" => inv.address = v,
                "d" => inv.device_id = v,
                "p" => inv.profile_id = v,
                "n" => inv.profile_name = v,
                "c" => inv.code = v,
                _ => {}
            }
        }
        let filled = [&inv.address, &inv.device_id, &inv.profile_id, &inv.code]
            .iter()
            .all(|s| !s.is_empty() && s.chars().count() <= MAX_LINK_FIELD);
        filled.then_some(inv)
    }

    /// The address split into host and port; `None` when it isn't `host:port`.
    pub fn host_port(&self) -> Option<(String, u16)> {
        split_address(&self.address)
    }
}

/// `host:port` (or `[v6]:port`) to its parts; `None` without a usable port.
pub fn split_address(address: &str) -> Option<(String, u16)> {
    let (host, port) = address.rsplit_once(':')?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let port: u16 = port.parse().ok()?;
    (!host.is_empty() && port != 0).then(|| (host.to_owned(), port))
}

/// One local-library file, path relative to the library root, forward slashes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestFile {
    pub path: String,
    pub size: u64,
    pub last_modified: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncManifest {
    pub device_id: String,
    pub device_name: String,
    pub backup_size: u64,
    pub files: Vec<ManifestFile>,
}

/// Map a manifest path (`"<manga>/<file>"`) to a path under `root`, or `None` if it is not
/// exactly two plain components. Manifest paths come from the other device, so this is the
/// only thing between a peer and the rest of the disk, on both the serving and pulling side.
pub fn resolve_manifest_path(root: &Path, path: &str) -> Option<PathBuf> {
    if path.is_empty() || path.contains('\\') || path.contains('\0') {
        return None;
    }
    let mut parts = path.split('/');
    let (Some(dir), Some(file), None) = (parts.next(), parts.next(), parts.next()) else {
        return None;
    };
    for part in [dir, file] {
        let mut comps = Path::new(part).components();
        if !matches!(
            (comps.next(), comps.next()),
            (Some(Component::Normal(_)), None)
        ) || part.starts_with('.')
            || part.contains(':')
        {
            return None;
        }
    }
    Some(root.join(dir).join(file))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn manifest_paths_stay_under_the_root() {
        let r = Path::new("/lib");
        assert_eq!(
            resolve_manifest_path(r, "My Manga/Chapter 1.cbz"),
            Some(PathBuf::from("/lib/My Manga/Chapter 1.cbz"))
        );
        for bad in [
            "",
            "a",
            "a/b/c",
            "../b",
            "a/..",
            "/etc/passwd",
            "a//b",
            "./b",
            "a/.hidden",
            "a\\..\\b",
            "C:/x",
            "a/C:x",
            "a/b\0",
        ] {
            assert_eq!(resolve_manifest_path(r, bad), None, "{bad:?}");
        }
    }

    #[test]
    fn link_codes_round_trip_and_reject_junk() {
        let inv = LinkInvite {
            address: "192.168.0.5:27193".into(),
            device_id: "pc-1".into(),
            profile_id: "5b1e…".into(),
            profile_name: "Main & more".into(),
            code: "123456".into(),
        };
        let uri = inv.to_uri();
        assert!(uri.starts_with("tachimana://link?h=192%2E168"), "{uri}");
        assert_eq!(LinkInvite::parse(&uri), Some(inv.clone()));
        assert_eq!(inv.host_port(), Some(("192.168.0.5".to_owned(), 27193)));
        let plus =
            LinkInvite::parse("tachimana://link?h=[fe80::1]:9&d=a&p=b&n=Second+phone&c=1").unwrap();
        assert_eq!(plus.profile_name, "Second phone");
        assert_eq!(plus.host_port(), Some(("fe80::1".to_owned(), 9)));
        for bad in [
            "",
            "https://x?h=a:1&d=a&p=b&c=1",
            "tachimana://link?h=a:1&d=a&p=b",
            "tachimana://link?h=a:1&d=a&p=b&c=%FF",
        ] {
            assert_eq!(LinkInvite::parse(bad), None, "{bad:?}");
        }
        assert_eq!(
            LinkInvite {
                address: "nope".into(),
                ..inv
            }
            .host_port(),
            None
        );
    }

    #[test]
    fn old_pings_still_parse_and_link_requests_are_checked() {
        let p: PingInfo = serde_json::from_str(r#"{"deviceId":"a","deviceName":"b"}"#).unwrap();
        assert_eq!(p.profile_id, None);
        assert_eq!(
            serde_json::to_string(&p).unwrap(),
            r#"{"deviceId":"a","deviceName":"b"}"#
        );
        let mut r = LinkRequest {
            device_id: "a".into(),
            device_name: "Phone".into(),
            profile_id: "p".into(),
            profile_name: "Main".into(),
            platform: "android".into(),
            port: None,
        };
        assert!(r.is_valid());
        r.platform = "ios".into();
        assert!(!r.is_valid());
        r.platform = "desktop".into();
        r.device_name = "x".repeat(MAX_LINK_FIELD + 1);
        assert!(!r.is_valid());
        let resp = LinkResponse {
            device_id: "a".into(),
            device_name: "b".into(),
            profile_id: "c".into(),
            profile_name: "d".into(),
            key: Some("secret".into()),
        };
        assert!(!format!("{resp:?}").contains("secret"));
    }

    #[test]
    fn json_matches_the_kotlin_keys() {
        let m = SyncManifest {
            device_id: "A".into(),
            device_name: "PC".into(),
            backup_size: 3,
            files: vec![ManifestFile {
                path: "a/b".into(),
                size: 1,
                last_modified: 2,
            }],
        };
        let j = serde_json::to_string(&m).unwrap();
        assert_eq!(
            j,
            r#"{"deviceId":"A","deviceName":"PC","backupSize":3,"files":[{"path":"a/b","size":1,"lastModified":2}]}"#
        );
    }
}
