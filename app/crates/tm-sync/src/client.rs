//! Pull side of LAN sync (`LanSyncClient.kt`).

use std::path::Path;
use std::time::Duration;

use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;

use crate::protocol::{
    LinkRequest, LinkResponse, PeerDevice, PingInfo, SyncManifest, LINK_HEADER, PAIR_HEADER,
};
use crate::queue::{
    ChapterRef, ClaimRequest, ClaimResponse, DoneEvent, DoneRequest, DoneResponse, QueueItem,
    QueueList, ReleaseRequest, MAX_QUEUE,
};
use crate::{Result, SyncError};

/// Largest `GET /changes` answer read.
pub const MAX_CHANGES_BYTES: u64 = 32 * 1024 * 1024;

/// What `GET /changes` gave.
#[derive(Debug)]
pub enum Pulled {
    Page(crate::journal::ChangesPage),
    /// The peer has no `changes/1` (an app from before it).
    NotSupported,
    /// The cursor is older than what the peer keeps; merge in full, then go on from `oldest`.
    TooOld {
        oldest: i64,
    },
}

/// Largest backup accepted from a peer.
pub const MAX_BACKUP_BYTES: u64 = 256 * 1024 * 1024;
/// Largest chapter sent with `PUT /file` (the phone takes at most 1 GiB).
pub const MAX_PUT_BYTES: u64 = 1024 * 1024 * 1024;

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

    /// `GET /changes?after=<after>` on a linked device (`changes/1`).
    pub async fn changes_after(
        &self,
        peer: &PeerDevice,
        key: &str,
        after: i64,
        limit: usize,
    ) -> Result<Pulled> {
        let resp = self
            .http
            .get(format!(
                "http://{}/changes?after={after}&limit={limit}",
                peer.address()
            ))
            .header(LINK_HEADER, key)
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .map_err(|e| SyncError::Http(format!("{}: {e}", peer.name)))?;
        match resp.status().as_u16() {
            200 => {
                let bytes = read_capped(resp, MAX_CHANGES_BYTES).await?;
                serde_json::from_slice(&bytes)
                    .map(Pulled::Page)
                    .map_err(|e| SyncError::Protocol(format!("/changes: {e}")))
            }
            404 | 405 => Ok(Pulled::NotSupported),
            401 => Err(SyncError::Unlinked),
            410 => {
                let bytes = read_capped(resp, 4096).await.unwrap_or_default();
                let oldest = serde_json::from_slice::<serde_json::Value>(&bytes)
                    .ok()
                    .and_then(|v| v.get("oldest").and_then(serde_json::Value::as_i64))
                    .unwrap_or(0);
                Ok(Pulled::TooOld { oldest })
            }
            s => Err(SyncError::Http(format!("{} answered {s}", peer.name))),
        }
    }

    /// `POST /changes`: hands a linked device entries to take now. `Ok(None)` when it has no
    /// `changes/1`.
    pub async fn push_changes(
        &self,
        peer: &PeerDevice,
        key: &str,
        device_id: &str,
        changes: &[crate::journal::Entry],
    ) -> Result<Option<usize>> {
        let body = serde_json::json!({"deviceId": device_id, "changes": changes});
        match self
            .call_json::<_, serde_json::Value>(
                reqwest::Method::POST,
                peer,
                key,
                "/changes",
                Some(&body),
                4096,
            )
            .await
        {
            Ok(v) => Ok(Some(
                v.get("applied")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|n| usize::try_from(n).ok())
                    .unwrap_or(0),
            )),
            Err(SyncError::Http(m)) if m.contains("can't do that yet") => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// `PUT /roles`: tells a linked device who leads now. Answers the mode that holds afterwards
    /// (the other side's, when its change was newer), seen from this device; `None` when the
    /// device has no `roles/1`.
    pub async fn put_roles(
        &self,
        peer: &PeerDevice,
        key: &str,
        mode: crate::roles::Mode,
        at: i64,
    ) -> Result<Option<(crate::roles::Mode, i64)>> {
        let body = crate::roles::RolesWire {
            mode: mode.as_caller().into(),
            mode_at: at,
        };
        let resp = self
            .http
            .put(format!("http://{}/roles", peer.address()))
            .header(LINK_HEADER, key)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(serde_json::to_vec(&body).map_err(|e| SyncError::Protocol(e.to_string()))?)
            .timeout(Duration::from_secs(15))
            .send()
            .await
            .map_err(|e| SyncError::Http(format!("{}: {e}", peer.name)))?;
        match resp.status().as_u16() {
            200 | 409 => {
                let bytes = read_capped(resp, 4096).await?;
                let w: crate::roles::RolesWire = serde_json::from_slice(&bytes)
                    .map_err(|e| SyncError::Protocol(format!("/roles: {e}")))?;
                Ok(crate::roles::Mode::from_answer(&w.mode).map(|m| (m, w.mode_at)))
            }
            404 | 405 => Ok(None),
            401 => Err(SyncError::Unlinked),
            s => Err(SyncError::Http(format!("{} answered {s}", peer.name))),
        }
    }

    /// `code` is either the six-digit pairing code or a link key (43 characters); the header
    /// follows from which it is.
    async fn get(&self, peer: &PeerDevice, code: &str, path: &str) -> Result<reqwest::Response> {
        let is_key = crate::links::is_key(code);
        let header = if is_key { LINK_HEADER } else { PAIR_HEADER };
        let resp = self
            .http
            .get(format!("http://{}{path}", peer.address()))
            .header(header, code)
            .send()
            .await
            .map_err(|e| SyncError::Http(format!("{}: {e}", peer.name)))?;
        match resp.status().as_u16() {
            200..=299 => Ok(resp),
            401 if is_key => Err(SyncError::Unlinked),
            401 => Err(SyncError::WrongCode),
            429 => Err(SyncError::Http(
                "too many wrong codes; wait a minute and try again".into(),
            )),
            s => Err(SyncError::Http(format!("{} answered {s}", peer.name))),
        }
    }

    /// `POST /link` to `host:port` with the pairing code shown there. On success the answer
    /// carries the key; [`SyncError::OtherProfile`] when that device is on another profile.
    pub async fn link(
        &self,
        host: &str,
        port: u16,
        code: &str,
        req: &LinkRequest,
    ) -> Result<LinkResponse> {
        let peer = PeerDevice {
            id: String::new(),
            name: host.to_owned(),
            host: host.trim().to_owned(),
            port,
        };
        let resp = self
            .http
            .post(format!("http://{}/link", peer.address()))
            .header(PAIR_HEADER, code)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(serde_json::to_vec(req).map_err(|e| SyncError::Protocol(e.to_string()))?)
            .timeout(Duration::from_secs(15))
            .send()
            .await
            .map_err(|e| SyncError::Http(format!("{}: {e}", peer.address())))?;
        let status = resp.status().as_u16();
        let parse = |body: &[u8]| -> Result<LinkResponse> {
            serde_json::from_slice(body).map_err(|e| SyncError::Protocol(format!("link: {e}")))
        };
        match status {
            200..=299 => {
                let r = parse(&read_capped(resp, 64 * 1024).await?)?;
                if r.key.as_deref().is_some_and(crate::links::is_key) && !r.device_id.is_empty() {
                    Ok(r)
                } else {
                    Err(SyncError::Protocol(
                        "link: no usable key in the answer".into(),
                    ))
                }
            }
            409 => Err(SyncError::OtherProfile(Box::new(parse(
                &read_capped(resp, 64 * 1024).await?,
            )?))),
            401 => Err(SyncError::WrongCode),
            404 => Err(SyncError::Http(
                "that device can't link yet; update it first".into(),
            )),
            429 => Err(SyncError::Http(
                "too many wrong codes; wait a minute and try again".into(),
            )),
            s => Err(SyncError::Http(format!("{} answered {s}", peer.address()))),
        }
    }

    /// Sends `body` as JSON with the link `key` and reads a JSON answer of at most `cap` bytes.
    async fn call_json<B: serde::Serialize, T: serde::de::DeserializeOwned>(
        &self,
        method: reqwest::Method,
        peer: &PeerDevice,
        key: &str,
        path: &str,
        body: Option<&B>,
        cap: u64,
    ) -> Result<T> {
        let mut req = self
            .http
            .request(method, format!("http://{}{path}", peer.address()))
            .header(LINK_HEADER, key)
            .timeout(Duration::from_secs(30));
        if let Some(b) = body {
            req = req
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_vec(b).map_err(|e| SyncError::Protocol(e.to_string()))?);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| SyncError::Http(format!("{}: {e}", peer.name)))?;
        match resp.status().as_u16() {
            200..=299 => {}
            401 => return Err(SyncError::Unlinked),
            404 | 405 => {
                return Err(SyncError::Http(format!(
                    "{} can't do that yet; update it first",
                    peer.name
                )))
            }
            s => return Err(SyncError::Http(format!("{} answered {s}", peer.name))),
        }
        let bytes = read_capped(resp, cap).await?;
        serde_json::from_slice(&bytes).map_err(|e| SyncError::Protocol(format!("{path}: {e}")))
    }

    /// The linked phone's download queue, in order, at most [`MAX_QUEUE`] items.
    pub async fn queue(&self, peer: &PeerDevice, key: &str) -> Result<Vec<QueueItem>> {
        let list: QueueList = self
            .call_json::<(), _>(
                reqwest::Method::GET,
                peer,
                key,
                "/queue",
                None,
                32 * 1024 * 1024,
            )
            .await?;
        Ok(list
            .items
            .into_iter()
            .filter(|i| i.chapter.is_valid())
            .take(MAX_QUEUE)
            .collect())
    }

    /// Claims `items` from the phone's queue for `by` (this PC). Answers what was claimed.
    pub async fn claim(
        &self,
        peer: &PeerDevice,
        key: &str,
        items: Vec<ChapterRef>,
        by: &str,
    ) -> Result<Vec<ChapterRef>> {
        let req = ClaimRequest {
            items,
            by: by.to_owned(),
        };
        let r: ClaimResponse = self
            .call_json(
                reqwest::Method::POST,
                peer,
                key,
                "/queue/claim",
                Some(&req),
                32 * 1024 * 1024,
            )
            .await?;
        Ok(r.claimed)
    }

    /// Gives claimed `items` back to the phone. Only the status matters.
    pub async fn release(
        &self,
        peer: &PeerDevice,
        key: &str,
        items: Vec<ChapterRef>,
    ) -> Result<()> {
        let req = ReleaseRequest { items };
        let _: serde_json::Value = self
            .call_json(
                reqwest::Method::POST,
                peer,
                key,
                "/queue/release",
                Some(&req),
                32 * 1024 * 1024,
            )
            .await?;
        Ok(())
    }

    /// Sends done events. Answers the ids the phone acked.
    pub async fn done(
        &self,
        peer: &PeerDevice,
        key: &str,
        events: Vec<DoneEvent>,
    ) -> Result<Vec<String>> {
        let req = DoneRequest { events };
        let r: DoneResponse = self
            .call_json(
                reqwest::Method::POST,
                peer,
                key,
                "/downloads/done",
                Some(&req),
                4 * 1024 * 1024,
            )
            .await?;
        Ok(r.acked)
    }

    /// `GET /outbox?since=` on a linked PC: its done events for this device after `since`.
    pub async fn outbox(
        &self,
        peer: &PeerDevice,
        key: &str,
        since: Option<&str>,
    ) -> Result<Vec<DoneEvent>> {
        let path = match since {
            Some(s) => format!(
                "/outbox?since={}",
                percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC)
            ),
            None => "/outbox".to_owned(),
        };
        let r: crate::queue::OutboxList = self
            .call_json::<(), _>(
                reqwest::Method::GET,
                peer,
                key,
                &path,
                None,
                32 * 1024 * 1024,
            )
            .await?;
        Ok(r.events)
    }

    /// Uploads a chapter archive to the phone's downloads folder, at `path`
    /// (`downloads/<source>/<manga>/<chapter>.cbz`).
    pub async fn put_file(
        &self,
        peer: &PeerDevice,
        key: &str,
        path: &str,
        file: &Path,
    ) -> Result<()> {
        let meta = tokio::fs::metadata(file).await.map_err(SyncError::Io)?;
        let len = meta.len();
        if len == 0 || len > MAX_PUT_BYTES {
            return Err(SyncError::Protocol(format!("{len} bytes is not a chapter")));
        }
        let f = tokio::fs::File::open(file).await.map_err(SyncError::Io)?;
        let body = reqwest::Body::wrap_stream(tokio_util::io::ReaderStream::new(f));
        let q: String =
            percent_encoding::utf8_percent_encode(path, percent_encoding::NON_ALPHANUMERIC)
                .collect();
        let resp = self
            .http
            .put(format!("http://{}/file?path={q}", peer.address()))
            .header(LINK_HEADER, key)
            .header(reqwest::header::CONTENT_LENGTH, len)
            .body(body)
            .timeout(Duration::from_secs(600))
            .send()
            .await
            .map_err(|e| SyncError::Http(format!("{}: {e}", peer.name)))?;
        match resp.status().as_u16() {
            200..=299 => Ok(()),
            401 => Err(SyncError::Unlinked),
            404 => Err(SyncError::Http(format!(
                "{} doesn't have that chapter in its library",
                peer.name
            ))),
            s => {
                let why = read_capped(resp, 4096).await.unwrap_or_default();
                Err(SyncError::Http(format!(
                    "{} answered {s}: {}",
                    peer.name,
                    String::from_utf8_lossy(&why).trim()
                )))
            }
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
