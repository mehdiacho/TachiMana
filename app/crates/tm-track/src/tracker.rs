//! AniList tracking over `manga_sync` rows (`AniListTracker.kt`). Rows use the lineage sync id 2,
//! so they travel in `.tachibk` backups and LAN sync like any other track.

use tm_db::{Db, Track, ANILIST_SYNC_ID};

use crate::api::{status, AniListApi, EntryChange, Media};
use crate::auth::Session;
use crate::{Result, TrackError};

/// Largest chapter number pushed. Chapter numbers come from sources and file names.
const MAX_CHAPTER: f64 = 100_000.0;

pub fn track(db: &Db, manga_id: i64) -> Result<Option<Track>> {
    Ok(db.track(manga_id, ANILIST_SYNC_ID)?)
}

fn bound(db: &Db, manga_id: i64) -> Result<Track> {
    track(db, manga_id)?.ok_or_else(|| TrackError::Invalid("This manga isn't tracked.".into()))
}

/// Binds `manga_id` to `media`. An entry already on the user's list is adopted as it is
/// (progress, status, score); otherwise one is created as Reading at chapter 0.
pub async fn bind(
    db: &Db,
    api: &AniListApi,
    session: &Session,
    manga_id: i64,
    media: &Media,
) -> Result<Track> {
    let entry = match api
        .find_entry(&session.token, session.user_id, media.id)
        .await?
    {
        Some(e) => e,
        None => {
            let change = EntryChange {
                status: Some("CURRENT"),
                progress: Some(0),
                score: None,
            };
            api.save_entry(&session.token, media.id, &change).await?
        }
    };
    Ok(db.put_track(&Track {
        id: 0,
        manga_id,
        sync_id: ANILIST_SYNC_ID,
        remote_id: media.id,
        library_id: Some(entry.id),
        title: media.title.clone(),
        last_chapter_read: entry.progress as f64,
        total_chapters: media.chapters.unwrap_or(0).max(0),
        status: status::from_api(&entry.status),
        score: entry.score,
        remote_url: media.url(),
        start_date: 0,
        finish_date: 0,
    })?)
}

pub fn unbind(db: &Db, manga_id: i64) -> Result<()> {
    Ok(db.delete_track(manga_id, ANILIST_SYNC_ID)?)
}

/// After a chapter is finished: raise AniList's progress to that chapter. Does nothing when the
/// manga is untracked, when no account is connected, for chapter numbers below 1, or when
/// AniList (or our row) is already at or past it, so remote progress never goes down. Marks the
/// entry Completed on reaching the last chapter. Returns whether AniList was changed.
pub async fn push_progress(
    db: &Db,
    api: &AniListApi,
    session: Option<&Session>,
    manga_id: i64,
    chapter_number: f64,
) -> Result<bool> {
    let Some(session) = session else {
        return Ok(false);
    };
    let Some(row) = track(db, manga_id)? else {
        return Ok(false);
    };
    if !chapter_number.is_finite() || !(1.0..MAX_CHAPTER).contains(&chapter_number) {
        return Ok(false);
    }
    // In range, so the cast is exact.
    let target = chapter_number.floor() as i64;
    if (target as f64) <= row.last_chapter_read {
        return Ok(false);
    }
    // The list may have moved on from another app since we last looked.
    if let Some(remote) = api
        .find_entry(&session.token, session.user_id, row.remote_id)
        .await?
    {
        if remote.progress >= target {
            db.update_track(
                row.id,
                Some(remote.progress as f64),
                Some(status::from_api(&remote.status)),
                None,
            )?;
            return Ok(false);
        }
    }
    let completed = row.total_chapters > 0 && target >= row.total_chapters;
    let change = EntryChange {
        status: completed.then_some("COMPLETED"),
        progress: Some(target),
        score: None,
    };
    let saved = api
        .save_entry(&session.token, row.remote_id, &change)
        .await?;
    db.update_track(
        row.id,
        Some(saved.progress as f64),
        Some(status::from_api(&saved.status)),
        None,
    )?;
    Ok(true)
}

/// The user picked a status in the tracking dialog.
pub async fn set_status(
    db: &Db,
    api: &AniListApi,
    session: &Session,
    manga_id: i64,
    value: i64,
) -> Result<Track> {
    if !status::ALL.contains(&value) {
        return Err(TrackError::Invalid("Unknown status.".into()));
    }
    let row = bound(db, manga_id)?;
    let change = EntryChange {
        status: Some(status::to_api(value)),
        ..Default::default()
    };
    let saved = api
        .save_entry(&session.token, row.remote_id, &change)
        .await?;
    db.update_track(row.id, None, Some(status::from_api(&saved.status)), None)?;
    bound(db, manga_id)
}

/// The user set the chapter count with the stepper. Unlike [`push_progress`] this may lower it:
/// the user asked for that number.
pub async fn set_progress(
    db: &Db,
    api: &AniListApi,
    session: &Session,
    manga_id: i64,
    progress: i64,
) -> Result<Track> {
    let row = bound(db, manga_id)?;
    let max = if row.total_chapters > 0 {
        row.total_chapters
    } else {
        MAX_CHAPTER as i64
    };
    if !(0..=max).contains(&progress) {
        return Err(TrackError::Invalid(format!(
            "Chapters must be between 0 and {max}."
        )));
    }
    let change = EntryChange {
        progress: Some(progress),
        ..Default::default()
    };
    let saved = api
        .save_entry(&session.token, row.remote_id, &change)
        .await?;
    db.update_track(
        row.id,
        Some(saved.progress as f64),
        Some(status::from_api(&saved.status)),
        None,
    )?;
    bound(db, manga_id)
}

/// The user set a score out of 10 (0 clears it on AniList).
pub async fn set_score(
    db: &Db,
    api: &AniListApi,
    session: &Session,
    manga_id: i64,
    score: f64,
) -> Result<Track> {
    if !score.is_finite() || !(0.0..=10.0).contains(&score) {
        return Err(TrackError::Invalid(
            "Score must be between 0 and 10.".into(),
        ));
    }
    let row = bound(db, manga_id)?;
    let change = EntryChange {
        score: Some(score),
        ..Default::default()
    };
    let saved = api
        .save_entry(&session.token, row.remote_id, &change)
        .await?;
    db.update_track(row.id, None, None, Some(saved.score))?;
    bound(db, manga_id)
}
