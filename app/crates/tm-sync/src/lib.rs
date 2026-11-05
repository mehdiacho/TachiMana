//! LAN sync between TachiMana installs, Kotlin or Rust (`sync/.../lan/`, `SyncManager.kt`).
//!
//! A device serves its library ([`server::SyncServer`]) and advertises itself over mDNS
//! ([`discovery::Discovery`]). Another device pulls with the six-digit pairing code shown on the
//! first one ([`pull`]): it imports the peer's backup, then copies over the local-library files
//! it does not have, byte for byte.

pub mod client;
pub mod discovery;
pub mod identity;
pub mod links;
pub mod protocol;
pub mod server;

use std::path::Path;
use std::sync::Arc;

pub use client::SyncClient;
pub use identity::DeviceIdentity;
pub use links::{Link, LinkStore};
pub use protocol::{ManifestFile, PeerDevice, SyncManifest};
use tm_backup::ImportSummary;
use tm_db::Db;

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("I/O error: {0}")]
    Io(std::io::Error),
    #[error("{0}")]
    Http(String),
    #[error("wrong pairing code")]
    WrongCode,
    /// A link key the other device no longer knows: it unlinked this one.
    #[error("the other device no longer has this link")]
    Unlinked,
    /// `POST /link` to a device on another profile; carries that device's profile.
    #[error("the other device is on another profile ({})", .0.profile_name)]
    OtherProfile(Box<protocol::LinkResponse>),
    #[error("{0}")]
    Bind(String),
    #[error("{0}")]
    Protocol(String),
    #[error(transparent)]
    Backup(#[from] tm_backup::BackupError),
    #[error(transparent)]
    Local(#[from] tm_local::LocalError),
}

pub type Result<T> = std::result::Result<T, SyncError>;

/// Outcome of a completed pull.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PullReport {
    pub peer_name: String,
    pub summary: ImportSummary,
    pub files_pulled: usize,
    pub bytes_pulled: u64,
}

/// Progress of a running pull, for the UI.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "stage", rename_all = "camelCase")]
pub enum PullProgress {
    Manifest,
    Backup {
        bytes: u64,
    },
    Importing,
    File {
        index: usize,
        total: usize,
        path: String,
    },
}

/// Pull `peer`'s library into `db` and `local_root`: import its backup, then fetch each local
/// file that is missing here or has a different size. Idempotent: a second pull fetches nothing.
pub async fn pull<D: AsRef<Db> + Send + Sync + 'static>(
    client: &SyncClient,
    peer: &PeerDevice,
    code: &str,
    db: Arc<D>,
    local_root: &Path,
    progress: &(dyn Fn(PullProgress) + Send + Sync),
) -> Result<PullReport> {
    progress(PullProgress::Manifest);
    let manifest = client.fetch_manifest(peer, code).await?;
    progress(PullProgress::Backup {
        bytes: manifest.backup_size,
    });
    let bytes = client.fetch_backup(peer, code).await?;
    progress(PullProgress::Importing);
    let backup = tm_backup::codec::decode(&bytes)?;
    let summary = {
        let db = Arc::clone(&db);
        tokio::task::spawn_blocking(move || tm_backup::import((*db).as_ref(), &backup))
            .await
            .map_err(|e| SyncError::Protocol(format!("import task failed: {e}")))??
    };

    let total = manifest.files.len();
    let mut files_pulled = 0usize;
    let mut bytes_pulled = 0u64;
    for (i, f) in manifest.files.iter().enumerate() {
        let Some(dest) = protocol::resolve_manifest_path(local_root, &f.path) else {
            tracing::warn!("skipping unsafe manifest path {:?}", f.path);
            continue;
        };
        let have = tokio::fs::metadata(&dest).await.ok();
        if have.is_some_and(|m| m.is_file() && m.len() == f.size) {
            continue;
        }
        progress(PullProgress::File {
            index: i.saturating_add(1),
            total,
            path: f.path.clone(),
        });
        let n = client
            .fetch_file(peer, code, &f.path, &dest, f.size)
            .await?;
        files_pulled += 1;
        bytes_pulled = bytes_pulled.saturating_add(n);
    }

    // Covers point at this machine's files; pick up anything only a scan finds.
    let root = local_root.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let db = (*db).as_ref();
        tm_backup::relink_local_covers(db, &root)?;
        tm_local::scan(db, &root)?;
        Ok(())
    })
    .await
    .map_err(|e| SyncError::Protocol(format!("scan task failed: {e}")))??;

    Ok(PullReport {
        peer_name: peer.name.clone(),
        summary,
        files_pulled,
        bytes_pulled,
    })
}
