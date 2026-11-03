//! The Rust side of the extension host: starts the Kotlin `ext-host` process and talks to it over
//! its localhost HTTP API. The contract lives in `docs/rust-port/api.md`.

pub mod client;
pub mod host;
pub mod install;
pub mod model;

pub use client::Client;
pub use host::{ExtHost, HostConfig};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// `ext-host.jar` is not where the app expected it.
    #[error("the extension host is not installed (missing {0})")]
    NotInstalled(std::path::PathBuf),
    #[error("could not start the extension host: {0}")]
    Spawn(String),
    #[error("could not reach the extension host: {0}")]
    Transport(String),
    /// The host answered with an error. `kind` is the host's error kind (api.md).
    #[error("{message}")]
    Host {
        status: u16,
        kind: String,
        message: String,
    },
    #[error("unexpected answer from the extension host: {0}")]
    Decode(String),
    #[error("the extension host sent more data than allowed")]
    TooLarge,
}

impl Error {
    /// The host's error kind, or a local one (`notInstalled`, `spawn`, `transport`, `decode`,
    /// `tooLarge`).
    pub fn kind(&self) -> &str {
        match self {
            Error::NotInstalled(_) => "notInstalled",
            Error::Spawn(_) => "spawn",
            Error::Transport(_) => "transport",
            Error::Host { kind, .. } => kind,
            Error::Decode(_) => "decode",
            Error::TooLarge => "tooLarge",
        }
    }
}
