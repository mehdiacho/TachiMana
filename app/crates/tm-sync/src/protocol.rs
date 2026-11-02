//! Wire types of the LAN sync protocol (`SyncTransport.kt`, `LanSyncServer.kt`). JSON keys are
//! camelCase to match the Kotlin app; both apps speak the same protocol.
//!
//! Routes (HTTP/1.1, plain text on the LAN):
//! - `GET /ping` -> [`PingInfo`]. No pairing code needed.
//! - `GET /manifest` -> [`SyncManifest`].
//! - `GET /backup` -> the `.tachibk` container bytes.
//! - `GET /file?path=<manifest path>` -> the file's bytes.
//!
//! Every route but `/ping` needs the pairing code in the [`PAIR_HEADER`] header.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const PAIR_HEADER: &str = "X-Pair-Code";
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
