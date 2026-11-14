//! The PC's side of the phone-queue routes, against a stand-in phone that answers as the fork
//! documents it (mehdiacho/mihon#2, #3).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::sync::{Arc, Mutex};

use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde_json::{json, Value};
use tm_sync::queue::{ChapterRef, DoneEvent, Where};
use tm_sync::{PeerDevice, SyncClient, SyncError};

const KEY: &str = "kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk";

#[derive(Default)]
struct Phone {
    claimed: Vec<Value>,
    released: Vec<Value>,
    done: Vec<Value>,
    files: Vec<(String, usize)>,
}

type S = Arc<Mutex<Phone>>;

fn authed(h: &HeaderMap) -> Result<(), StatusCode> {
    (h.get("X-Link-Key").and_then(|v| v.to_str().ok()) == Some(KEY))
        .then_some(())
        .ok_or(StatusCode::UNAUTHORIZED)
}

async fn queue(h: HeaderMap) -> Result<Json<Value>, StatusCode> {
    authed(&h)?;
    Ok(Json(json!({"items": [
        {"sourceId": "9223372036854775807", "mangaUrl": "/m/1", "chapterUrl": "/c/1",
         "mangaTitle": "Lantern Road", "chapterName": "Chapter 1", "queuedAt": 5},
        {"sourceId": "2", "mangaUrl": "", "chapterUrl": "/bad", "queuedAt": 6},
        {"sourceId": "2", "mangaUrl": "/m/2", "chapterUrl": "/c/2"}
    ]})))
}

async fn claim(
    State(s): State<S>,
    h: HeaderMap,
    Json(b): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    authed(&h)?;
    if b["by"] != "pc-id" {
        return Err(StatusCode::BAD_REQUEST);
    }
    let items = b["items"].as_array().cloned().unwrap_or_default();
    s.lock().unwrap().claimed.extend(items.clone());
    // The second one is already downloading on the phone, so it is left out.
    Ok(Json(
        json!({"claimed": items.into_iter().take(1).collect::<Vec<_>>()}),
    ))
}

async fn release(
    State(s): State<S>,
    h: HeaderMap,
    Json(b): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    authed(&h)?;
    s.lock()
        .unwrap()
        .released
        .extend(b["items"].as_array().cloned().unwrap_or_default());
    Ok(Json(b))
}

async fn done(
    State(s): State<S>,
    h: HeaderMap,
    Json(b): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    authed(&h)?;
    let events = b["events"].as_array().cloned().unwrap_or_default();
    let acked: Vec<Value> = events.iter().map(|e| e["eventId"].clone()).collect();
    s.lock().unwrap().done.extend(events);
    Ok(Json(json!({"acked": acked})))
}

#[derive(serde::Deserialize)]
struct PathQ {
    path: String,
}

async fn file(
    State(s): State<S>,
    h: HeaderMap,
    Query(q): Query<PathQ>,
    body: Bytes,
) -> Result<Json<Value>, (StatusCode, &'static str)> {
    authed(&h).map_err(|c| (c, "needs a link key"))?;
    if !q.path.starts_with("downloads/") || !q.path.ends_with(".cbz") {
        return Err((StatusCode::BAD_REQUEST, "bad path"));
    }
    if q.path.contains("Unknown") {
        return Err((StatusCode::NOT_FOUND, "no chapter here by that name"));
    }
    s.lock().unwrap().files.push((q.path, body.len()));
    Ok(Json(json!({"result": "written"})))
}

async fn phone() -> (PeerDevice, S) {
    let s: S = Arc::default();
    let app = Router::new()
        .route("/queue", get(queue))
        .route("/queue/claim", post(claim))
        .route("/queue/release", post(release))
        .route("/downloads/done", post(done))
        .route("/file", put(file))
        .with_state(Arc::clone(&s));
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    (
        PeerDevice {
            id: "phone".into(),
            name: "Phone".into(),
            host: "127.0.0.1".into(),
            port,
        },
        s,
    )
}

fn r(source: i64, m: &str, c: &str) -> ChapterRef {
    ChapterRef {
        source_id: source,
        manga_url: m.into(),
        chapter_url: c.into(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_pc_reads_claims_releases_reports_and_sends() {
    let (peer, s) = phone().await;
    let c = SyncClient::new().unwrap();

    let items = c.queue(&peer, KEY).await.unwrap();
    assert_eq!(items.len(), 2, "the item with no manga url is dropped");
    assert_eq!(items[0].chapter, r(i64::MAX, "/m/1", "/c/1"));
    assert_eq!(items[0].manga_title, "Lantern Road");
    assert!(matches!(
        c.queue(&peer, "wrong").await,
        Err(SyncError::Unlinked)
    ));

    let wanted: Vec<ChapterRef> = items.iter().map(|i| i.chapter.clone()).collect();
    let claimed = c.claim(&peer, KEY, wanted, "pc-id").await.unwrap();
    assert_eq!(claimed, vec![r(i64::MAX, "/m/1", "/c/1")]);
    assert_eq!(
        s.lock().unwrap().claimed[0]["sourceId"],
        "9223372036854775807",
        "ids go out as strings"
    );
    assert!(c.claim(&peer, KEY, vec![], "someone-else").await.is_err());

    c.release(&peer, KEY, claimed.clone()).await.unwrap();
    assert_eq!(s.lock().unwrap().released.len(), 1);

    let ev = DoneEvent {
        chapter: r(2, "/m/2", "/c/2"),
        event_id: "e1".into(),
        where_: Where::Sent,
        device_id: "pc-id".into(),
        at: 7,
        take: None,
    };
    assert_eq!(
        c.done(&peer, KEY, vec![ev]).await.unwrap(),
        vec!["e1".to_owned()]
    );
    assert_eq!(s.lock().unwrap().done[0]["where"], "sent");

    let tmp = tempfile::tempdir().unwrap();
    let cbz = tmp.path().join("c.cbz");
    std::fs::write(&cbz, vec![7u8; 3200]).unwrap();
    c.put_file(
        &peer,
        KEY,
        "downloads/Src (EN)/Lantern Road/Chapter 1_abcdef.cbz",
        &cbz,
    )
    .await
    .unwrap();
    assert_eq!(
        s.lock().unwrap().files,
        vec![(
            "downloads/Src (EN)/Lantern Road/Chapter 1_abcdef.cbz".to_owned(),
            3200
        )]
    );
    let missing = c
        .put_file(&peer, KEY, "downloads/Src (EN)/Unknown/Chapter 1.cbz", &cbz)
        .await;
    assert!(
        matches!(missing, Err(SyncError::Http(ref m)) if m.contains("doesn't have")),
        "{missing:?}"
    );
    std::fs::write(&cbz, b"").unwrap();
    assert!(
        c.put_file(&peer, KEY, "downloads/a/b/c.cbz", &cbz)
            .await
            .is_err(),
        "empty file"
    );
}
