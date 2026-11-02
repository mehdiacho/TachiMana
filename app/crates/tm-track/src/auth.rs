//! AniList sign-in (`AniListAuth.kt`): OAuth2 implicit grant for a desktop app. The browser opens
//! [`authorize_url`]; AniList redirects to `http://localhost:43815/auth/anilist#access_token=…`.
//! A fragment never reaches a server, so the first page served there forwards it as a query
//! string, and the second request carries the token. Pasting a token is the fallback. Only the
//! public client id is involved, no secret.
//!
//! The session is kept in `anilist.json` in the same shape as the Kotlin app's, so either app can
//! read the other's.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::extract::Query;
use axum::response::Html;
use axum::routing::get;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::api::AniListApi;
use crate::{Result, TrackError};

/// TachiMana's registered AniList API client (a public identifier, not a secret).
pub const CLIENT_ID: u32 = 46514;
pub const CALLBACK_PORT: u16 = 43815;
pub const REDIRECT_PATH: &str = "/auth/anilist";

/// AniList tokens live about a year; the Kotlin app records 360 days and so do we.
const TOKEN_LIFE_MS: i64 = 360 * 24 * 60 * 60 * 1000;
const MAX_TOKEN_LEN: usize = 8 * 1024;

pub fn authorize_url() -> String {
    format!("https://anilist.co/api/v2/oauth/authorize?client_id={CLIENT_ID}&response_type=token")
}

/// A connected account. Field names match the Kotlin `AniListSession`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub token: String,
    /// Unix milliseconds.
    pub expires_at: i64,
    pub user_id: i64,
    pub user_name: String,
}

#[derive(Debug)]
pub struct Auth {
    file: PathBuf,
    session: Mutex<Option<Session>>,
}

impl Auth {
    /// Reads `file`. A missing, unreadable or expired session means "not connected".
    pub fn open(file: &Path) -> Self {
        let session = std::fs::read(file)
            .ok()
            .and_then(|b| serde_json::from_slice::<Session>(&b).ok())
            .filter(|s| s.expires_at > now_ms() && clean_token(&s.token).is_some());
        Self {
            file: file.to_owned(),
            session: Mutex::new(session),
        }
    }

    pub fn session(&self) -> Option<Session> {
        self.lock().clone()
    }

    /// Checks `raw` (a captured or pasted token) by asking AniList whose it is, then saves it.
    pub async fn adopt(&self, api: &AniListApi, raw: &str) -> Result<Session> {
        let token =
            clean_token(raw).ok_or_else(|| TrackError::Invalid("Paste the token first.".into()))?;
        let user = api.viewer(&token).await?;
        let session = Session {
            token,
            expires_at: now_ms().saturating_add(TOKEN_LIFE_MS),
            user_id: user.id,
            user_name: user.name,
        };
        write_private(
            &self.file,
            &serde_json::to_vec_pretty(&session).map_err(io_err)?,
        )?;
        *self.lock() = Some(session.clone());
        Ok(session)
    }

    pub fn disconnect(&self) -> Result<()> {
        *self.lock() = None;
        match std::fs::remove_file(&self.file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Option<Session>> {
        // The guarded value is a plain Option that is only ever replaced whole, so a panic
        // elsewhere cannot leave it half-written.
        self.session.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Trims a pasted token (AniList's pin page shows it bare; people also paste `Bearer …`).
/// `None` when nothing usable is left.
pub fn clean_token(raw: &str) -> Option<String> {
    let t = raw.trim();
    let t = match t.strip_prefix("Bearer") {
        Some(rest) if rest.is_empty() || rest.starts_with(char::is_whitespace) => rest.trim(),
        _ => t,
    };
    let ok = !t.is_empty() && t.len() <= MAX_TOKEN_LEN && t.chars().all(|c| c.is_ascii_graphic());
    ok.then(|| t.to_owned())
}

/// Serves the redirect page on `127.0.0.1:port` until AniList's redirect hands over a token,
/// the user declines, or `wait` passes. Call it before opening the browser at [`authorize_url`]:
/// the returned future is already listening once `listening` fires.
pub async fn capture_token(port: u16, wait: Duration, listening: impl FnOnce()) -> Result<String> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|e| TrackError::Listen {
            port,
            message: e.to_string(),
        })?;
    type Slot = Arc<Mutex<Option<oneshot::Sender<std::result::Result<String, String>>>>>;
    let (tx, rx) = oneshot::channel();
    let slot: Slot = Arc::new(Mutex::new(Some(tx)));
    let handler = move |Query(q): Query<HashMap<String, String>>| {
        let slot = Arc::clone(&slot);
        async move {
            let outcome = if let Some(t) = q.get("access_token") {
                Some(clean_token(t).ok_or_else(|| "AniList sent an empty token".to_owned()))
            } else {
                q.get("error").map(|e| {
                    Err(match q.get("error_description") {
                        Some(d) => format!("{e}: {d}"),
                        None => e.clone(),
                    })
                })
            };
            match outcome {
                None => Html(CAPTURE_PAGE),
                Some(r) => {
                    let ok = r.is_ok();
                    if let Some(tx) = slot.lock().unwrap_or_else(|e| e.into_inner()).take() {
                        let _ = tx.send(r);
                    }
                    Html(if ok { DONE_PAGE } else { DECLINED_PAGE })
                }
            }
        }
    };
    let app = axum::Router::new().route(REDIRECT_PATH, get(handler));
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    listening();
    let got = tokio::time::timeout(wait, rx).await;
    // Let the last page finish sending before the listener goes away.
    tokio::time::sleep(Duration::from_millis(200)).await;
    server.abort();
    match got {
        Err(_) => Err(TrackError::Timeout),
        Ok(Err(_)) => Err(TrackError::Protocol("sign-in listener stopped".into())),
        Ok(Ok(Err(e))) => Err(TrackError::Api(format!("sign-in was not completed ({e})"))),
        Ok(Ok(Ok(t))) => Ok(t),
    }
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    {
        use std::io::Write;
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn io_err(e: serde_json::Error) -> TrackError {
    TrackError::Io(std::io::Error::other(e))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

const CAPTURE_PAGE: &str = r#"<!doctype html><meta charset="utf-8"><title>Connecting…</title>
<body style="font-family:sans-serif;background:#1c1c1d;color:#eee;display:grid;place-items:center;height:100vh">
<p>Connecting to TachiMana…</p>
<script>location.replace('/auth/anilist?' + (location.hash.substring(1) || 'error=no_token'));</script>"#;

const DONE_PAGE: &str = r#"<!doctype html><meta charset="utf-8"><title>Connected</title>
<body style="font-family:sans-serif;background:#1c1c1d;color:#eee;display:grid;place-items:center;height:100vh">
<p>TachiMana has the sign-in. You can close this tab and go back to the app.</p>"#;

const DECLINED_PAGE: &str = r#"<!doctype html><meta charset="utf-8"><title>Not connected</title>
<body style="font-family:sans-serif;background:#1c1c1d;color:#eee;display:grid;place-items:center;height:100vh">
<p>AniList was not connected. You can close this tab and try again from TachiMana.</p>"#;

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
    use super::*;

    #[test]
    fn tokens_are_trimmed_and_checked() {
        assert_eq!(
            clean_token("  Bearer abc.def  ").as_deref(),
            Some("abc.def")
        );
        assert_eq!(clean_token("abc").as_deref(), Some("abc"));
        assert_eq!(clean_token("   "), None);
        assert_eq!(clean_token("Bearer "), None);
        assert_eq!(clean_token("a b"), None);
        assert_eq!(clean_token(&"x".repeat(MAX_TOKEN_LEN + 1)), None);
    }

    #[test]
    fn reads_the_kotlin_session_file_and_drops_expired_ones() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("anilist.json");
        let future = now_ms() + 60_000;
        std::fs::write(
            &f,
            format!(r#"{{"token":"t0k","expiresAt":{future},"userId":7,"userName":"mehdi"}}"#),
        )
        .unwrap();
        let s = Auth::open(&f).session().unwrap();
        assert_eq!(
            (s.token.as_str(), s.user_id, s.user_name.as_str()),
            ("t0k", 7, "mehdi")
        );
        std::fs::write(
            &f,
            r#"{"token":"t0k","expiresAt":1,"userId":7,"userName":"m"}"#,
        )
        .unwrap();
        assert!(Auth::open(&f).session().is_none());
        std::fs::write(&f, "not json").unwrap();
        assert!(Auth::open(&f).session().is_none());
        let a = Auth::open(&dir.path().join("missing.json"));
        assert!(a.session().is_none());
        a.disconnect().unwrap();
    }
}
