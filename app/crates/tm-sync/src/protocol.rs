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

/// Major version of the protocol's ground rules (`docs/protocol/README.md` §2).
pub const PROTOCOL: &str = "1";

/// What this app implements its side of, as listed by `GET /hello`.
pub const CAPABILITIES: &[&str] = &[
    "hello/1",
    "link/2",
    "pull/1",
    "queue/1",
    "done/1",
    "file/1",
    "changes/1",
    "roles/1",
];

/// What a device with no `/hello` is taken to implement: the apps released before it existed.
pub const LEGACY_CAPABILITIES: &[&str] = &["link/2", "pull/1"];

/// Most capabilities kept from a `/hello` answer, and the longest one kept.
const MAX_CAPABILITIES: usize = 64;
const MAX_CAPABILITY_LEN: usize = 64;

/// The app a device runs, as it names itself.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
}

/// Answer of `GET /hello` (`hello/1`): who a device is and what it implements.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hello {
    #[serde(default)]
    pub protocol: String,
    pub device_id: String,
    pub device_name: String,
    #[serde(default)]
    pub platform: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_name: Option<String>,
    #[serde(default)]
    pub app: AppInfo,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

impl Hello {
    /// What a device that answers `/ping` but has no `/hello` is taken to be.
    pub fn legacy(ping: PingInfo) -> Self {
        Self {
            protocol: PROTOCOL.to_owned(),
            device_id: ping.device_id,
            device_name: ping.device_name,
            platform: String::new(),
            profile_id: ping.profile_id,
            profile_name: ping.profile_name,
            app: AppInfo::default(),
            capabilities: LEGACY_CAPABILITIES
                .iter()
                .map(|c| (*c).to_owned())
                .collect(),
        }
    }

    /// What to take a device to be from the bodies of its `/hello` and `/ping` answers (each
    /// `None` when that route gave no success). A `/hello` that does not parse counts as absent.
    pub fn from_answers(hello: Option<&[u8]>, ping: Option<&[u8]>) -> Option<Self> {
        if let Some(h) = hello.and_then(|b| serde_json::from_slice::<Self>(b).ok()) {
            return Some(h.tidy());
        }
        let ping: PingInfo = serde_json::from_slice(ping?).ok()?;
        Some(Self::legacy(ping))
    }

    /// A received answer made safe to keep: the list is capped, and entries that are not a
    /// `name/major` are dropped.
    pub fn tidy(mut self) -> Self {
        self.capabilities.retain(|c| is_capability(c));
        self.capabilities.truncate(MAX_CAPABILITIES);
        self
    }

    /// Whether the two devices can use `capability` (`name/major`) together: the protocol major
    /// matches and the device lists it.
    pub fn supports(&self, capability: &str) -> bool {
        self.protocol == PROTOCOL && self.capabilities.iter().any(|c| c == capability)
    }
}

/// `name/major`: a lower-case name (letters, digits, `-`), a slash, and a number.
fn is_capability(c: &str) -> bool {
    if c.len() > MAX_CAPABILITY_LEN {
        return false;
    }
    let Some((name, major)) = c.split_once('/') else {
        return false;
    };
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !major.is_empty()
        && major.bytes().all(|b| b.is_ascii_digit())
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

    /// Runs every file in docs/protocol/vectors/hello-1.
    #[test]
    fn hello_vectors() {
        let dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs/protocol/vectors/hello-1");
        let mut ran = 0;
        for f in std::fs::read_dir(&dir).unwrap() {
            let path = f.unwrap().path();
            let v: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            let body = |k: &str| {
                v.get(k).filter(|b| !b.is_null()).map(|b| match b.as_str() {
                    Some(raw) => raw.as_bytes().to_vec(),
                    None => serde_json::to_vec(b).unwrap(),
                })
            };
            let got = Hello::from_answers(body("hello").as_deref(), body("ping").as_deref());
            let expect = &v["expect"];
            assert_eq!(
                got.is_some(),
                expect["reachable"].as_bool().unwrap(),
                "{path:?}"
            );
            if let Some(h) = got {
                let caps: Vec<String> =
                    serde_json::from_value(expect["capabilities"].clone()).unwrap();
                assert_eq!(h.capabilities, caps, "{path:?}");
                for (cap, want) in expect["supports"].as_object().unwrap() {
                    assert_eq!(h.supports(cap), want.as_bool().unwrap(), "{path:?} {cap}");
                }
            }
            ran += 1;
        }
        assert!(ran >= 5, "only {ran} hello vectors ran");
    }

    #[test]
    fn our_own_list_is_well_formed_and_capped() {
        assert!(CAPABILITIES.iter().all(|c| is_capability(c)));
        assert!(LEGACY_CAPABILITIES.iter().all(|c| is_capability(c)));
        let many = Hello {
            capabilities: (0..500).map(|i| format!("x{i}/1")).collect(),
            ..Hello::legacy(PingInfo {
                device_id: "d".into(),
                device_name: "n".into(),
                profile_id: None,
                profile_name: None,
            })
        };
        assert_eq!(many.tidy().capabilities.len(), MAX_CAPABILITIES);
    }
}
