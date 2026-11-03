//! Port of `TrackSelfTest.kt` plus the tracker rules, against a fake AniList GraphQL endpoint.
//! `live_search` hits the real AniList and is ignored by default (`cargo test -- --ignored`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use serde_json::{json, Value};
use tm_db::{Db, SourceManga, ANILIST_SYNC_ID};
use tm_track::api::EntryChange;
use tm_track::{auth, status, tracker, AniListApi, Auth, Media, Session, TrackError};

#[derive(Default)]
struct Fake {
    /// media id → (entry id, status, progress, score)
    entries: HashMap<i64, (i64, String, i64, f64)>,
    saves: Vec<Value>,
}

type Shared = Arc<Mutex<Fake>>;

async fn graphql(
    State(st): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Json<Value> {
    let q = body["query"].as_str().unwrap_or_default().to_owned();
    let v = &body["variables"];
    let authed = headers.get("authorization").and_then(|h| h.to_str().ok()) == Some("Bearer good");
    let mut st = st.lock().unwrap();
    if q.contains("Viewer") {
        return Json(if authed {
            json!({"data": {"Viewer": {"id": 7, "name": "mehdi"}}})
        } else {
            json!({"errors": [{"message": "Invalid token", "status": 400}], "data": null})
        });
    }
    if q.contains("Page(") {
        return Json(json!({"data": {"Page": {"media": [
            {"id": 30013, "title": {"userPreferred": "ONE PIECE"}, "chapters": null, "format": "MANGA", "status": "RELEASING", "startDate": {"year": 1997}},
            {"id": 95552, "title": {"userPreferred": "Wan Piece"}, "chapters": 10, "format": "MANGA", "status": "FINISHED", "startDate": {"year": null}}
        ]}}}));
    }
    if !authed {
        return Json(
            json!({"errors": [{"message": "Unauthorized.", "status": 401}], "data": null}),
        );
    }
    if q.contains("MediaList(") {
        let id = v["mediaId"].as_i64().unwrap();
        return Json(match st.entries.get(&id) {
            Some((eid, s, p, sc)) => {
                json!({"data": {"MediaList": {"id": eid, "status": s, "progress": p, "score": sc}}})
            }
            None => {
                json!({"errors": [{"message": "Not Found.", "status": 404}], "data": {"MediaList": null}})
            }
        });
    }
    if q.contains("SaveMediaListEntry") {
        st.saves.push(v.clone());
        let id = v["mediaId"].as_i64().unwrap();
        let next = 1000 + i64::try_from(st.entries.len()).unwrap();
        let e = st
            .entries
            .entry(id)
            .or_insert((next, "CURRENT".into(), 0, 0.0));
        if let Some(s) = v.get("status").and_then(Value::as_str) {
            e.1 = s.into();
        }
        if let Some(p) = v.get("progress").and_then(Value::as_i64) {
            e.2 = p;
        }
        assert!(
            v.get("score").is_none(),
            "score is read in the user's own format; send scoreRaw"
        );
        if let Some(raw) = v.get("scoreRaw").and_then(Value::as_i64) {
            e.3 = raw as f64 / 10.0;
        }
        let (eid, s, p, sc) = e.clone();
        return Json(
            json!({"data": {"SaveMediaListEntry": {"id": eid, "status": s, "progress": p, "score": sc}}}),
        );
    }
    Json(json!({"errors": [{"message": "unexpected query"}]}))
}

async fn fake() -> (AniListApi, Shared) {
    let st: Shared = Arc::default();
    let app = axum::Router::new()
        .route("/", axum::routing::post(graphql))
        .with_state(Arc::clone(&st));
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    (
        AniListApi::with_endpoint(&format!("http://{addr}/")).unwrap(),
        st,
    )
}

fn session() -> Session {
    Session {
        token: "good".into(),
        expires_at: i64::MAX,
        user_id: 7,
        user_name: "mehdi".into(),
    }
}

fn library() -> (Db, i64) {
    let db = Db::open_in_memory().unwrap();
    let m = SourceManga {
        url: "/op".into(),
        title: "One Piece".into(),
        ..Default::default()
    };
    let id = db.add_to_library(5, &m, &[]).unwrap();
    (db, id)
}

fn media(id: i64, chapters: Option<i64>) -> Media {
    Media {
        id,
        title: "ONE PIECE".into(),
        chapters,
        format: Some("MANGA".into()),
        status: None,
        year: None,
    }
}

#[tokio::test]
async fn search_parses_hits() {
    let (api, _) = fake().await;
    let hits = api.search("one piece").await.unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(
        (hits[0].id, hits[0].title.as_str(), hits[0].chapters),
        (30013, "ONE PIECE", None)
    );
    assert_eq!(hits[0].year, Some(1997));
    assert_eq!(hits[1].chapters, Some(10));
    assert_eq!(hits[1].url(), "https://anilist.co/manga/95552");
}

#[tokio::test]
async fn adopt_checks_the_token_and_saves_a_kotlin_shaped_session() {
    let (api, _) = fake().await;
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("anilist.json");
    let a = Auth::open(&f);
    assert!(matches!(
        a.adopt(&api, "bad").await,
        Err(TrackError::Api(_))
    ));
    assert!(a.session().is_none() && !f.exists());
    assert!(matches!(
        a.adopt(&api, "  ").await,
        Err(TrackError::Invalid(_))
    ));
    let s = a.adopt(&api, "Bearer good").await.unwrap();
    assert_eq!(
        (s.user_id, s.user_name.as_str(), s.token.as_str()),
        (7, "mehdi", "good")
    );
    let on_disk: Value = serde_json::from_slice(&std::fs::read(&f).unwrap()).unwrap();
    for k in ["token", "expiresAt", "userId", "userName"] {
        assert!(on_disk.get(k).is_some(), "{k} missing");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&f).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    assert_eq!(Auth::open(&f).session(), Some(s));
    a.disconnect().unwrap();
    assert!(a.session().is_none() && !f.exists());
}

#[tokio::test]
async fn bind_adopts_an_existing_entry_or_creates_one() {
    let (api, st) = fake().await;
    let (db, id) = library();
    st.lock()
        .unwrap()
        .entries
        .insert(30013, (55, "PAUSED".into(), 12, 8.0));
    let t = tracker::bind(&db, &api, &session(), id, &media(30013, None))
        .await
        .unwrap();
    assert_eq!(
        (
            t.remote_id,
            t.library_id,
            t.last_chapter_read,
            t.status,
            t.score
        ),
        (30013, Some(55), 12.0, status::PAUSED, 8.0)
    );
    assert_eq!(t.sync_id, ANILIST_SYNC_ID);
    assert!(
        st.lock().unwrap().saves.is_empty(),
        "an existing entry is not rewritten"
    );

    // Rebinding to another manga replaces the row and creates the entry as Reading at 0.
    let t = tracker::bind(&db, &api, &session(), id, &media(95552, Some(10)))
        .await
        .unwrap();
    assert_eq!(
        (t.remote_id, t.last_chapter_read, t.total_chapters, t.status),
        (95552, 0.0, 10, status::READING)
    );
    assert_eq!(db.tracks(id).unwrap().len(), 1);
    tracker::unbind(&db, id).unwrap();
    assert!(tracker::track(&db, id).unwrap().is_none());
}

#[tokio::test]
async fn push_progress_only_ever_raises_and_completes_at_the_end() {
    let (api, st) = fake().await;
    let (db, id) = library();
    let s = session();
    // Not tracked, or not connected: nothing happens.
    assert!(!tracker::push_progress(&db, &api, Some(&s), id, 3.0)
        .await
        .unwrap());
    tracker::bind(&db, &api, &s, id, &media(95552, Some(10)))
        .await
        .unwrap();
    assert!(!tracker::push_progress(&db, &api, None, id, 3.0)
        .await
        .unwrap());

    assert!(tracker::push_progress(&db, &api, Some(&s), id, 3.7)
        .await
        .unwrap());
    assert_eq!(
        tracker::track(&db, id).unwrap().unwrap().last_chapter_read,
        3.0
    );
    // Re-reading an earlier chapter, junk numbers and chapter 0 never lower anything.
    for n in [2.0, 3.0, f64::NAN, f64::INFINITY, -1.0, 0.5, 1e12] {
        assert!(
            !tracker::push_progress(&db, &api, Some(&s), id, n)
                .await
                .unwrap(),
            "{n}"
        );
    }
    // Progress made in another app is adopted instead of being overwritten.
    st.lock().unwrap().entries.get_mut(&95552).unwrap().2 = 8;
    assert!(!tracker::push_progress(&db, &api, Some(&s), id, 5.0)
        .await
        .unwrap());
    assert_eq!(
        tracker::track(&db, id).unwrap().unwrap().last_chapter_read,
        8.0
    );
    assert_eq!(st.lock().unwrap().entries[&95552].2, 8);

    assert!(tracker::push_progress(&db, &api, Some(&s), id, 10.0)
        .await
        .unwrap());
    let t = tracker::track(&db, id).unwrap().unwrap();
    assert_eq!((t.last_chapter_read, t.status), (10.0, status::COMPLETED));
    let last = st.lock().unwrap().saves.last().cloned().unwrap();
    assert_eq!(last["status"], "COMPLETED");
}

#[tokio::test]
async fn dialog_edits_go_to_anilist_then_the_row() {
    let (api, st) = fake().await;
    let (db, id) = library();
    let s = session();
    assert!(matches!(
        tracker::set_status(&db, &api, &s, id, status::PAUSED).await,
        Err(TrackError::Invalid(_))
    ));
    tracker::bind(&db, &api, &s, id, &media(95552, Some(10)))
        .await
        .unwrap();
    let t = tracker::set_status(&db, &api, &s, id, status::PAUSED)
        .await
        .unwrap();
    assert_eq!(t.status, status::PAUSED);
    assert!(matches!(
        tracker::set_status(&db, &api, &s, id, 42).await,
        Err(TrackError::Invalid(_))
    ));
    let t = tracker::set_progress(&db, &api, &s, id, 4).await.unwrap();
    assert_eq!(t.last_chapter_read, 4.0);
    assert!(tracker::set_progress(&db, &api, &s, id, 11).await.is_err());
    assert!(tracker::set_progress(&db, &api, &s, id, -1).await.is_err());
    let t = tracker::set_score(&db, &api, &s, id, 7.5).await.unwrap();
    assert_eq!(t.score, 7.5);
    for bad in [f64::NAN, -0.1, 10.5] {
        assert!(tracker::set_score(&db, &api, &s, id, bad).await.is_err());
    }
    assert_eq!(
        st.lock().unwrap().entries[&95552],
        (1000, "PAUSED".into(), 4, 7.5)
    );
    // A save with nothing but the media id is still a valid mutation.
    api.save_entry("good", 95552, &EntryChange::default())
        .await
        .unwrap();
}

#[tokio::test]
async fn the_track_row_survives_a_backup_round_trip() {
    let (api, _) = fake().await;
    let (db, id) = library();
    st_bind(&db, &api, id).await;
    tracker::set_progress(&db, &api, &session(), id, 9)
        .await
        .unwrap();
    let b = tm_backup_roundtrip(&db);
    let t = tracker::track(&b, b.library().unwrap()[0].id)
        .unwrap()
        .unwrap();
    assert_eq!(
        (t.remote_id, t.last_chapter_read, t.sync_id),
        (95552, 9.0, ANILIST_SYNC_ID)
    );
}

async fn st_bind(db: &Db, api: &AniListApi, id: i64) {
    tracker::bind(db, api, &session(), id, &media(95552, Some(10)))
        .await
        .unwrap();
}

fn tm_backup_roundtrip(db: &Db) -> Db {
    let backup = tm_backup::export(db, &HashMap::new()).unwrap();
    let bytes = tm_backup::codec::encode(&backup).unwrap();
    let other = Db::open_in_memory().unwrap();
    tm_backup::import(&other, &tm_backup::codec::decode(&bytes).unwrap()).unwrap();
    other
}

#[tokio::test]
async fn capture_page_forwards_the_fragment_and_returns_the_token() {
    let port = free_port();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(auth::capture_token(
        port,
        Duration::from_secs(10),
        move || {
            let _ = tx.send(());
        },
    ));
    rx.await.unwrap();
    let base = format!("http://127.0.0.1:{port}{}", auth::REDIRECT_PATH);
    let first = get(&base).await;
    assert!(
        first.contains("location.hash"),
        "first visit serves the forwarding page"
    );
    let done = get(&format!(
        "{base}?access_token=abc%2Edef&token_type=Bearer&expires_in=31536000"
    ))
    .await;
    assert!(done.contains("go back to the app"));
    assert_eq!(task.await.unwrap().unwrap(), "abc.def");
}

#[tokio::test]
async fn capture_reports_a_declined_sign_in_and_times_out() {
    let port = free_port();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(auth::capture_token(
        port,
        Duration::from_secs(10),
        move || {
            let _ = tx.send(());
        },
    ));
    rx.await.unwrap();
    get(&format!(
        "http://127.0.0.1:{port}{}?error=access_denied",
        auth::REDIRECT_PATH
    ))
    .await;
    assert!(matches!(task.await.unwrap(), Err(TrackError::Api(m)) if m.contains("access_denied")));

    let r = auth::capture_token(free_port(), Duration::from_millis(100), || {}).await;
    assert!(matches!(r, Err(TrackError::Timeout)));
}

#[tokio::test]
async fn capture_fails_cleanly_when_the_port_is_taken() {
    let held = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = held.local_addr().unwrap().port();
    let r = auth::capture_token(port, Duration::from_secs(1), || {}).await;
    assert!(matches!(r, Err(TrackError::Listen { .. })));
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

async fn get(url: &str) -> String {
    // reqwest is built without a default TLS provider; the library installs one when it makes its
    // own client, but a capture test can run before any other test has done that.
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::get(url).await.unwrap().text().await.unwrap()
}

#[tokio::test]
#[ignore = "talks to the real AniList"]
async fn live_search() {
    let hits = AniListApi::new()
        .unwrap()
        .search("One Piece")
        .await
        .unwrap();
    assert!(hits.iter().any(|m| m.id == 30013), "{hits:?}");
}
