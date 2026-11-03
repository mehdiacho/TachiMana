//! Pull side of LAN sync (`LanSyncClient.kt`).

use std::path::Path;
use std::time::Duration;

use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;

use crate::protocol::{PeerDevice, PingInfo, SyncManifest, PAIR_HEADER};
use crate::{Result, SyncError};

/// Largest backup accepted from a peer.
pub const MAX_BACKUP_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct SyncClient {
    http: reqwest::Client,
}

impl SyncClient {
    pub fn new() -> Result<Self> {
        // reqwest's rustls feature is on elsewhere in the build; give it a provider so building
        // a client never panics. Sync itself is plain HTTP on the LAN.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .no_proxy()
            .build()
            .map_err(|e| SyncError::Http(e.to_string()))?;
        Ok(Self { http })
    }

    /// Reachability and identity of `host:port`, no pairing code needed.
    pub async fn probe(&self, host: &str, port: u16) -> Option<PeerDevice> {
        let peer = PeerDevice {
            id: String::new(),
            name: String::new(),
            host: host.trim().to_owned(),
            port,
        };
        let resp = self
            .http
            .get(format!("http://{}/ping", peer.address()))
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let info: PingInfo = serde_json::from_slice(&resp.bytes().await.ok()?).ok()?;
        Some(PeerDevice {
            id: info.device_id,
            name: info.device_name,
            ..peer
        })
    }

    async fn get(&self, peer: &PeerDevice, code: &str, path: &str) -> Result<reqwest::Response> {
        let resp = self
            .http
            .get(format!("http://{}{path}", peer.address()))
            .header(PAIR_HEADER, code)
            .send()
            .await
            .map_err(|e| SyncError::Http(format!("{}: {e}", peer.name)))?;
        match resp.status().as_u16() {
            200..=299 => Ok(resp),
            401 => Err(SyncError::WrongCode),
            429 => Err(SyncError::Http(
                "too many wrong codes; wait a minute and try again".into(),
            )),
            s => Err(SyncError::Http(format!("{} answered {s}", peer.name))),
        }
    }

    pub async fn fetch_manifest(&self, peer: &PeerDevice, code: &str) -> Result<SyncManifest> {
        let resp = self.get(peer, code, "/manifest").await?;
        let body = read_capped(resp, 64 * 1024 * 1024).await?;
        serde_json::from_slice(&body).map_err(|e| SyncError::Protocol(format!("manifest: {e}")))
    }

    pub async fn fetch_backup(&self, peer: &PeerDevice, code: &str) -> Result<Vec<u8>> {
        let resp = self.get(peer, code, "/backup").await?;
        read_capped(resp, MAX_BACKUP_BYTES).await
    }

    /// Stream one file to `dest` (via a `.part` file, renamed at the end) and return its size.
    /// `expected` is the manifest size; a different length is an error and nothing is kept.
    pub async fn fetch_file(
        &self,
        peer: &PeerDevice,
        code: &str,
        path: &str,
        dest: &Path,
        expected: u64,
    ) -> Result<u64> {
        let q: String =
            percent_encoding::utf8_percent_encode(path, percent_encoding::NON_ALPHANUMERIC)
                .to_string();
        let resp = self.get(peer, code, &format!("/file?path={q}")).await?;
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(SyncError::Io)?;
        }
        let mut part_name = dest.as_os_str().to_owned();
        part_name.push(".part");
        let part = std::path::PathBuf::from(part_name);
        let result = async {
            let mut f = tokio::fs::File::create(&part)
                .await
                .map_err(SyncError::Io)?;
            let mut n: u64 = 0;
            let mut stream = resp.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|e| SyncError::Http(e.to_string()))?;
                n = n.saturating_add(u64::try_from(chunk.len()).unwrap_or(u64::MAX));
                if n > expected {
                    return Err(SyncError::Protocol(format!(
                        "{path}: larger than announced"
                    )));
                }
                f.write_all(&chunk).await.map_err(SyncError::Io)?;
            }
            f.flush().await.map_err(SyncError::Io)?;
            if n != expected {
                return Err(SyncError::Protocol(format!(
                    "{path}: got {n} bytes, expected {expected}"
                )));
            }
            Ok(n)
        }
        .await;
        match result {
            Ok(n) => {
                tokio::fs::rename(&part, dest)
                    .await
                    .map_err(SyncError::Io)?;
                Ok(n)
            }
            Err(e) => {
                let _ = tokio::fs::remove_file(&part).await;
                Err(e)
            }
        }
    }
}

async fn read_capped(resp: reqwest::Response, cap: u64) -> Result<Vec<u8>> {
    if resp.content_length().is_some_and(|l| l > cap) {
        return Err(SyncError::Protocol("response too large".into()));
    }
    let mut out = Vec::new();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| SyncError::Http(e.to_string()))?;
        if u64::try_from(out.len().saturating_add(chunk.len())).unwrap_or(u64::MAX) > cap {
            return Err(SyncError::Protocol("response too large".into()));
        }
        out.extend_from_slice(&chunk);
    }
    Ok(out)
}
