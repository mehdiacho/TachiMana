//! AniList tracking in the app (`track/`, `ui/track/TrackDialog.kt`): the account, the tracking
//! dialog's commands, and pushing progress when a chapter is finished.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tm_track::{auth, status, tracker, AniListApi, Auth, Media, TrackError};
use ts_rs::TS;

use crate::dto::CmdError;
use crate::state::AppState;

type R<T> = Result<T, CmdError>;

/// How long the browser sign-in may take.
const SIGN_IN_WAIT: Duration = Duration::from_secs(5 * 60);

/// Tracking state held by [`AppState`].
pub struct Tracking {
    api: Option<AniListApi>,
    pub auth: Auth,
    signing_in: AtomicBool,
}

impl Tracking {
    pub fn open(paths: &tm_core::AppPaths) -> Self {
        Self {
            api: AniListApi::new().ok(),
            auth: Auth::open(&paths.data_dir().join("anilist.json")),
            signing_in: AtomicBool::new(false),
        }
    }

    fn api(&self) -> R<&AniListApi> {
        self.api
            .as_ref()
            .ok_or_else(|| CmdError::internal("The AniList client could not be set up."))
    }

    fn session(&self) -> R<tm_track::Session> {
        self.auth
            .session()
            .ok_or_else(|| CmdError::new("notConnected", "Connect your AniList account first."))
    }
}

impl From<TrackError> for CmdError {
    fn from(e: TrackError) -> Self {
        match e {
            TrackError::Invalid(m) => CmdError::new("invalid", m),
            TrackError::NotConnected => {
                CmdError::new("notConnected", "Connect your AniList account first.")
            }
            TrackError::Http(_) => CmdError::new(
                "network",
                "Couldn't reach AniList. Check your connection and try again.",
            ),
            TrackError::Timeout => CmdError::new(
                "timeout",
                "The AniList sign-in timed out. Try again, or paste a token instead.",
            ),
            TrackError::Api(m)
                if ["invalid token", "unauthorized"]
                    .iter()
                    .any(|k| m.to_ascii_lowercase().contains(k)) =>
            {
                CmdError::new(
                    "notConnected",
                    "AniList didn't accept the sign-in: the token is wrong or has expired. Sign out and connect again.",
                )
            }
            TrackError::Db(d) => d.into(),
            other => CmdError::new("anilist", other.to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AniListAccountDto {
    pub connected: bool,
    pub user_name: Option<String>,
    /// Unix milliseconds the token stops working.
    #[ts(type = "number | null")]
    pub expires_at: Option<i64>,
    /// A browser sign-in is waiting for AniList.
    pub signing_in: bool,
    /// Where to sign in when the browser could not be opened.
    pub authorize_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MediaDto {
    #[ts(type = "number")]
    pub id: i64,
    pub title: String,
    #[ts(type = "number | null")]
    pub chapters: Option<i64>,
    pub format: Option<String>,
    pub status: Option<String>,
    #[ts(type = "number | null")]
    pub year: Option<i64>,
}

impl From<Media> for MediaDto {
    fn from(m: Media) -> Self {
        Self {
            id: m.id,
            title: m.title,
            chapters: m.chapters,
            format: m.format,
            status: m.status,
            year: m.year,
        }
    }
}

impl From<MediaDto> for Media {
    fn from(m: MediaDto) -> Self {
        Self {
            id: m.id,
            title: m.title,
            chapters: m.chapters,
            format: m.format,
            status: m.status,
            year: m.year,
        }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TrackDto {
    #[ts(type = "number")]
    pub media_id: i64,
    pub title: String,
    pub url: String,
    /// Lineage status value (1 Reading … 6 Rereading).
    #[ts(type = "number")]
    pub status: i64,
    pub status_label: String,
    #[ts(type = "number")]
    pub progress: i64,
    /// 0 when AniList doesn't know the chapter count.
    #[ts(type = "number")]
    pub total: i64,
    pub score: f64,
}

impl From<&tm_db::Track> for TrackDto {
    fn from(t: &tm_db::Track) -> Self {
        Self {
            media_id: t.remote_id,
            title: t.title.clone(),
            url: t.remote_url.clone(),
            status: t.status,
            status_label: status::label(t.status).to_owned(),
            // Progress is a whole chapter count; the column is REAL in the lineage schema.
            progress: if t.last_chapter_read.is_finite() {
                t.last_chapter_read.clamp(0.0, 1e9) as i64
            } else {
                0
            },
            total: t.total_chapters.max(0),
            score: if t.score.is_finite() { t.score } else { 0.0 },
        }
    }
}

fn account_of(state: &AppState) -> AniListAccountDto {
    let s = state.track.auth.session();
    AniListAccountDto {
        connected: s.is_some(),
        user_name: s.as_ref().map(|s| s.user_name.clone()),
        expires_at: s.as_ref().map(|s| s.expires_at),
        signing_in: state.track.signing_in.load(Ordering::Relaxed),
        authorize_url: auth::authorize_url(),
    }
}

fn emit_account(app: &AppHandle) {
    let _ = app.emit("anilist", account_of(&app.state::<AppState>()));
}

fn track_dto(state: &AppState, manga_id: i64) -> R<Option<TrackDto>> {
    Ok(tracker::track(&state.db, manga_id)?
        .as_ref()
        .map(TrackDto::from))
}

#[tauri::command]
pub fn anilist_account(state: State<'_, AppState>) -> AniListAccountDto {
    account_of(&state)
}

/// Browser sign-in: listen on the registered localhost port, open AniList's authorize page,
/// wait for the redirect, then check and save the token.
#[tauri::command]
pub async fn anilist_connect(app: AppHandle) -> R<AniListAccountDto> {
    let state = app.state::<AppState>();
    if state.track.signing_in.swap(true, Ordering::AcqRel) {
        return Err(CmdError::new(
            "busy",
            "Finish the sign-in that is already open in your browser.",
        ));
    }
    emit_account(&app);
    let opener = app.clone();
    let token = auth::capture_token(auth::CALLBACK_PORT, SIGN_IN_WAIT, move || {
        use tauri_plugin_opener::OpenerExt;
        if let Err(e) = opener
            .opener()
            .open_url(auth::authorize_url(), None::<&str>)
        {
            tracing::warn!("couldn't open the browser for AniList: {e}");
        }
    })
    .await;
    let result = match token {
        Ok(t) => match state.track.api() {
            Ok(api) => state
                .track
                .auth
                .adopt(api, &t)
                .await
                .map_err(CmdError::from),
            Err(e) => Err(e),
        },
        Err(e) => Err(e.into()),
    };
    state.track.signing_in.store(false, Ordering::Release);
    emit_account(&app);
    result.map(|_| account_of(&state))
}

/// The fallback: a token pasted from AniList's pin page.
#[tauri::command]
pub async fn anilist_connect_token(app: AppHandle, token: String) -> R<AniListAccountDto> {
    let state = app.state::<AppState>();
    state.track.auth.adopt(state.track.api()?, &token).await?;
    emit_account(&app);
    Ok(account_of(&state))
}

#[tauri::command]
pub fn anilist_disconnect(app: AppHandle) -> R<AniListAccountDto> {
    let state = app.state::<AppState>();
    state.track.auth.disconnect()?;
    emit_account(&app);
    Ok(account_of(&state))
}

#[tauri::command]
pub fn track_get(state: State<'_, AppState>, manga_id: i64) -> R<Option<TrackDto>> {
    track_dto(&state, manga_id)
}

#[tauri::command]
pub async fn track_search(state: State<'_, AppState>, query: String) -> R<Vec<MediaDto>> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let q: String = q.chars().take(200).collect();
    let hits = state.track.api()?.search(&q).await?;
    Ok(hits.into_iter().map(MediaDto::from).collect())
}

#[tauri::command]
pub async fn track_bind(state: State<'_, AppState>, manga_id: i64, media: MediaDto) -> R<TrackDto> {
    let session = state.track.session()?;
    state
        .db
        .manga(manga_id)?
        .ok_or_else(|| CmdError::not_found("That manga"))?;
    let t = tracker::bind(
        &state.db,
        state.track.api()?,
        &session,
        manga_id,
        &media.into(),
    )
    .await?;
    Ok(TrackDto::from(&t))
}

#[tauri::command]
pub fn track_unbind(state: State<'_, AppState>, manga_id: i64) -> R<()> {
    Ok(tracker::unbind(&state.db, manga_id)?)
}

#[tauri::command]
pub async fn track_set_status(
    state: State<'_, AppState>,
    manga_id: i64,
    status: i64,
) -> R<TrackDto> {
    let session = state.track.session()?;
    let t = tracker::set_status(&state.db, state.track.api()?, &session, manga_id, status).await?;
    Ok(TrackDto::from(&t))
}

#[tauri::command]
pub async fn track_set_progress(
    state: State<'_, AppState>,
    manga_id: i64,
    progress: i64,
) -> R<TrackDto> {
    let session = state.track.session()?;
    let t =
        tracker::set_progress(&state.db, state.track.api()?, &session, manga_id, progress).await?;
    Ok(TrackDto::from(&t))
}

#[tauri::command]
pub async fn track_set_score(state: State<'_, AppState>, manga_id: i64, score: f64) -> R<TrackDto> {
    let session = state.track.session()?;
    let t = tracker::set_score(&state.db, state.track.api()?, &session, manga_id, score).await?;
    Ok(TrackDto::from(&t))
}

/// Opens the manga's AniList page in the browser. The address is built from the media id, never
/// taken from the row (rows can come from a restored backup).
#[tauri::command]
pub fn track_open(app: AppHandle, state: State<'_, AppState>, manga_id: i64) -> R<()> {
    use tauri_plugin_opener::OpenerExt;
    let t =
        tracker::track(&state.db, manga_id)?.ok_or_else(|| CmdError::not_found("That track"))?;
    let url = format!("https://anilist.co/manga/{}", t.remote_id);
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| CmdError::new("open", format!("Couldn't open the browser: {e}")))
}

/// A chapter was just finished: raise AniList's progress in the background. Failures (offline,
/// expired token) are logged and never reach the reader, like the Kotlin app.
pub fn push_finished(app: &AppHandle, manga_id: i64, chapter_number: f64) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let Ok(api) = state.track.api() else { return };
        let session = state.track.auth.session();
        match tracker::push_progress(&state.db, api, session.as_ref(), manga_id, chapter_number)
            .await
        {
            Ok(true) => {
                let _ = app.emit("track-changed", manga_id);
            }
            Ok(false) => {}
            Err(e) => tracing::warn!("AniList progress for manga {manga_id} not sent: {e}"),
        }
    });
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
    use super::*;

    #[test]
    fn track_rows_become_whole_chapter_counts() {
        let t = tm_db::Track {
            id: 1,
            manga_id: 1,
            sync_id: 2,
            remote_id: 30013,
            library_id: None,
            title: "One Piece".into(),
            last_chapter_read: 41.9,
            total_chapters: -3,
            status: status::PAUSED,
            score: f64::NAN,
            remote_url: "u".into(),
            start_date: 0,
            finish_date: 0,
        };
        let d = TrackDto::from(&t);
        assert_eq!(
            (d.progress, d.total, d.score, d.status_label.as_str()),
            (41, 0, 0.0, "Paused")
        );
        let d = TrackDto::from(&tm_db::Track {
            last_chapter_read: f64::INFINITY,
            ..t
        });
        assert_eq!(d.progress, 0);
    }

    #[test]
    fn a_fresh_install_is_not_connected() {
        let dir = tempfile::tempdir().unwrap();
        let s = AppState::open(tm_core::AppPaths::at(dir.path()).unwrap()).unwrap();
        let a = account_of(&s);
        assert!(!a.connected && a.user_name.is_none() && !a.signing_in);
        assert!(a.authorize_url.contains("client_id=46514"));
        assert_eq!(s.track.session().unwrap_err().kind, "notConnected");
        let e = CmdError::from(TrackError::Api("Invalid token".into()));
        assert_eq!(e.kind, "notConnected");
        assert!(e.message.contains("Sign out and connect again"));
        assert_eq!(
            CmdError::from(TrackError::Api("Validation".into())).kind,
            "anilist"
        );
    }
}
