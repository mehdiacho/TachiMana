//! NAS storage for TachiMana (PLAN §P6.3): Mihon-compatible names, a WebDAV client, the remote
//! index, the read mirror and the upload queue. No Tauri here; the app wires it up.

pub mod index;
pub mod mirror;
pub mod names;
pub mod upload;
pub mod webdav;

pub use index::{Lookup, RemoteIndex};
pub use mirror::Mirror;
pub use upload::{Role, UploadJob, UploadQueue, UploadStatus};
pub use webdav::{RemoteEntry, WebDav, WebDavConfig};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Config(String),
    #[error("{0}")]
    Server(String),
    #[error("Couldn't reach the server: {0}")]
    Http(#[from] reqwest::Error),
    #[error("{0}: {1}")]
    Io(String, std::io::Error),
    #[error(transparent)]
    Archive(#[from] tm_archive::ArchiveError),
}

pub type Result<T> = std::result::Result<T, Error>;
