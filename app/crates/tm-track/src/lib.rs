//! AniList tracking, ported from the Kotlin app's `track/` package:
//!
//! - [`api`]: the GraphQL client (`AniListApi.kt`) and the lineage status mapping.
//! - [`auth`]: the implicit-grant login with a localhost capture page and the pasted-token
//!   fallback, saved to `anilist.json` (`AniListAuth.kt`).
//! - [`tracker`]: binding library manga to AniList entries through `manga_sync` rows
//!   (sync id 2, so they travel in `.tachibk` backups) and pushing read progress without ever
//!   lowering AniList's (`AniListTracker.kt`).

pub mod api;
pub mod auth;
pub mod tracker;

pub use api::{status, AniListApi, Entry, Media, User};
pub use auth::{Auth, Session};

#[derive(Debug, thiserror::Error)]
pub enum TrackError {
    #[error("couldn't reach AniList: {0}")]
    Http(String),
    /// An error AniList itself returned, already worded by AniList.
    #[error("AniList: {0}")]
    Api(String),
    /// AniList answered 404: nothing there (or a list it won't show us).
    #[error("AniList: {0}")]
    Missing(String),
    #[error("AniList sent something unexpected: {0}")]
    Protocol(String),
    #[error("not connected to AniList")]
    NotConnected,
    #[error("{0}")]
    Invalid(String),
    #[error("the AniList sign-in timed out")]
    Timeout,
    #[error("couldn't listen for the AniList sign-in on port {port}: {message}")]
    Listen { port: u16, message: String },
    #[error(transparent)]
    Db(#[from] tm_db::DbError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, TrackError>;
