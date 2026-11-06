//! Port of the LAN half of `desktop/.../sync/SyncSelfTest.kt`: two libraries in one process,
//! a real server on A, B pulls by address.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tm_db::Db;
use tm_sync::server::{BackupFn, SyncServer};
use tm_sync::{DeviceIdentity, PeerDevice, SyncClient, SyncError};

const PNG_1X1: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0B, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x60, 0x60, 0x60, 0x60,
    0x00, 0x00, 0x00, 0x04, 0x00, 0x01, 0xF6, 0x17, 0x38, 0x55, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

fn write_cbz(path: &Path) {
    use std::io::Write;
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for p in ["001.png", "002.png"] {
        zip.start_file(p, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(PNG_1X1).unwrap();
    }
    zip.finish().unwrap();
}

struct Instance {
    db: Arc<Db>,
    local: PathBuf,
}

fn instance(dir: &Path, seed: bool) -> Instance {
    let local = dir.join("local");
    std::fs::create_dir_all(&local).unwrap();
    if seed {
        let m = local.join("Sample Manga");
        std::fs::create_dir_all(&m).unwrap();
        std::fs::write(m.join("cover.png"), PNG_1X1).unwrap();
        write_cbz(&m.join("Chapter 1.cbz"));
        write_cbz(&m.join("Chapter 2.cbz"));
    }
    let db = Arc::new(Db::open(&dir.join("lib.db")).unwrap());
    tm_local::scan(&db, &local).unwrap();
    Instance { db, local }
}

fn backup_fn(db: &Arc<Db>) -> Arc<BackupFn> {
    let db = Arc::clone(db);
    Arc::new(move || {
        let b = tm_backup::export(&db, &HashMap::new()).map_err(|e| e.to_string())?;
        tm_backup::codec::encode(&b).map_err(|e| e.to_string())
    })
}

async fn serve(i: &Instance, code: &str) -> SyncServer {
    let id = DeviceIdentity {
        id: "A-id".into(),
        name: "Instance A".into(),
        pairing_code: code.into(),
    };
    SyncServer::start(id, i.local.clone(), backup_fn(&i.db), Some(0))
        .await
        .unwrap()
}

fn quiet() -> impl Fn(tm_sync::PullProgress) + Send + Sync {
    |_| {}
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b_pulls_a_over_the_lan() {
    let tmp = tempfile::tempdir().unwrap();
    let a = instance(&tmp.path().join("A"), true);
    let b = instance(&tmp.path().join("B"), false);
    let server = serve(&a, "123456").await;
    let client = SyncClient::new().unwrap();

    let peer = client.probe("127.0.0.1", server.port()).await.unwrap();
    assert_eq!(peer.name, "Instance A");
    assert_eq!(peer.id, "A-id");

    let wrong = tm_sync::pull(
        &client,
        &peer,
        "000000",
        Arc::clone(&b.db),
        &b.local,
        &quiet(),
    )
    .await;
    assert!(matches!(wrong, Err(SyncError::WrongCode)), "{wrong:?}");

    let report = tm_sync::pull(
        &client,
        &peer,
        "123456",
        Arc::clone(&b.db),
        &b.local,
        &quiet(),
    )
    .await
    .unwrap();
    assert_eq!(report.summary.manga_added, 1);
    assert_eq!(report.files_pulled, 3);

    let lib = b.db.library().unwrap();
    assert_eq!(lib.len(), 1);
    assert_eq!(lib[0].title, "Sample Manga");
    assert_eq!(lib[0].source, 0);
    assert_eq!(b.db.chapters(lib[0].id).unwrap().len(), 2);
    let cover = lib[0].thumbnail_url.clone().unwrap();
    assert!(cover.starts_with(b.local.to_str().unwrap()), "{cover}");
    assert!(Path::new(&cover).exists());

    let a_cbz = std::fs::read(a.local.join("Sample Manga/Chapter 1.cbz")).unwrap();
    let b_cbz_path = b.local.join("Sample Manga/Chapter 1.cbz");
    assert_eq!(std::fs::read(&b_cbz_path).unwrap(), a_cbz, "byte-identical");
    let pages = tm_archive::page_names(&b_cbz_path).unwrap();
    assert_eq!(pages.len(), 2);
    assert!(!tm_archive::read_page(&b_cbz_path, &pages[0])
        .unwrap()
        .is_empty());
    assert!(!b.local.join("Sample Manga/Chapter 1.cbz.part").exists());

    let second = tm_sync::pull(
        &client,
        &peer,
        "123456",
        Arc::clone(&b.db),
        &b.local,
        &quiet(),
    )
    .await
    .unwrap();
    assert_eq!((second.summary.manga_added, second.files_pulled), (0, 0));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_server_refuses_paths_outside_the_library() {
    let tmp = tempfile::tempdir().unwrap();
    let a = instance(&tmp.path().join("A"), true);
    std::fs::write(tmp.path().join("A/secret.txt"), "x").unwrap();
    let server = serve(&a, "123456").await;
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    let get = |p: &str| {
        let url = format!(
            "http://127.0.0.1:{}/file?path={}",
            server.port(),
            percent_encoding::utf8_percent_encode(p, percent_encoding::NON_ALPHANUMERIC)
        );
        http.get(url).header("X-Pair-Code", "123456").send()
    };
    assert_eq!(get("Sample Manga/cover.png").await.unwrap().status(), 200);
    for bad in [
        "../secret.txt",
        "Sample Manga/../../secret.txt",
        "/etc/passwd",
        "Sample Manga",
    ] {
        assert_eq!(get(bad).await.unwrap().status(), 404, "{bad}");
    }
    // A symlink inside the library that points out of it is not followed.
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(
            tmp.path().join("A/secret.txt"),
            a.local.join("Sample Manga/link.txt"),
        )
        .unwrap();
        assert_eq!(get("Sample Manga/link.txt").await.unwrap().status(), 404);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn guessing_the_code_is_throttled() {
    let tmp = tempfile::tempdir().unwrap();
    let a = instance(&tmp.path().join("A"), true);
    let server = serve(&a, "123456").await;
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    let url = format!("http://127.0.0.1:{}/manifest", server.port());
    for i in 0..10 {
        let code = format!("{i:06}");
        let s = http
            .get(&url)
            .header("X-Pair-Code", code)
            .send()
            .await
            .unwrap()
            .status();
        assert_eq!(s, 401);
    }
    // Even the right code waits now.
    let s = http
        .get(&url)
        .header("X-Pair-Code", "123456")
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(s, 429);
}

/// A peer whose manifest names files outside the library: they are skipped, never written.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hostile_manifest_cannot_write_outside_the_library() {
    use axum::routing::get;
    let tmp = tempfile::tempdir().unwrap();
    let b = instance(&tmp.path().join("B"), false);
    let empty = tm_backup::codec::encode(&tm_backup::Backup::default()).unwrap();
    let asked = Arc::new(Mutex::new(Vec::<String>::new()));
    let asked2 = Arc::clone(&asked);
    let app = axum::Router::new()
        .route(
            "/manifest",
            get(|| async {
                axum::Json(serde_json::json!({
                    "deviceId": "evil", "deviceName": "Evil", "backupSize": 1,
                    "files": [
                        {"path": "../escape.txt", "size": 1, "lastModified": 0},
                        {"path": "M/../../escape.txt", "size": 1, "lastModified": 0},
                        {"path": "/tmp/escape.txt", "size": 1, "lastModified": 0},
                        {"path": "M/ok.txt", "size": 1, "lastModified": 0}
                    ]
                }))
            }),
        )
        .route("/backup", get(move || async move { empty }))
        .route(
            "/file",
            get(
                move |q: axum::extract::Query<HashMap<String, String>>| async move {
                    asked2
                        .lock()
                        .unwrap()
                        .push(q.get("path").cloned().unwrap_or_default());
                    "x"
                },
            ),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await });

    let peer = PeerDevice {
        id: "evil".into(),
        name: "Evil".into(),
        host: "127.0.0.1".into(),
        port,
    };
    let client = SyncClient::new().unwrap();
    let r = tm_sync::pull(&client, &peer, "1", Arc::clone(&b.db), &b.local, &quiet())
        .await
        .unwrap();
    assert_eq!(r.files_pulled, 1);
    assert_eq!(asked.lock().unwrap().as_slice(), ["M/ok.txt"]);
    assert!(!tmp.path().join("B/escape.txt").exists());
    assert!(!tmp.path().join("escape.txt").exists());
    assert_eq!(std::fs::read(b.local.join("M/ok.txt")).unwrap(), b"x");
}

/// A file whose length differs from the manifest is not kept.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_short_file_is_discarded() {
    use axum::routing::get;
    let tmp = tempfile::tempdir().unwrap();
    let b = instance(&tmp.path().join("B"), false);
    let empty = tm_backup::codec::encode(&tm_backup::Backup::default()).unwrap();
    let app = axum::Router::new()
        .route(
            "/manifest",
            get(|| async {
                axum::Json(serde_json::json!({
                    "deviceId": "p", "deviceName": "P", "backupSize": 1,
                    "files": [{"path": "M/c.cbz", "size": 10, "lastModified": 0}]
                }))
            }),
        )
        .route("/backup", get(move || async move { empty }))
        .route("/file", get(|| async { "short" }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await });
    let peer = PeerDevice {
        id: "p".into(),
        name: "P".into(),
        host: "127.0.0.1".into(),
        port,
    };
    let client = SyncClient::new().unwrap();
    let r = tm_sync::pull(&client, &peer, "1", Arc::clone(&b.db), &b.local, &quiet()).await;
    assert!(matches!(r, Err(SyncError::Protocol(_))), "{r:?}");
    assert!(!b.local.join("M/c.cbz").exists());
    assert!(!b.local.join("M/c.cbz.part").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_port_moves_on_when_taken() {
    let tmp = tempfile::tempdir().unwrap();
    let a = instance(&tmp.path().join("A"), false);
    let first = serve(&a, "123456").await;
    let id = DeviceIdentity {
        id: "x".into(),
        name: "x".into(),
        pairing_code: "123456".into(),
    };
    let second = SyncServer::start(id, a.local.clone(), backup_fn(&a.db), Some(first.port()))
        .await
        .unwrap();
    assert_ne!(second.port(), first.port());
}

fn link_req(profile: &str) -> tm_sync::protocol::LinkRequest {
    tm_sync::protocol::LinkRequest {
        device_id: "B-id".into(),
        device_name: "Phone".into(),
        profile_id: profile.into(),
        profile_name: "Main".into(),
        platform: "android".into(),
        port: Some(27200),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn linking_gives_a_key_that_replaces_the_code() {
    let tmp = tempfile::tempdir().unwrap();
    let a = instance(&tmp.path().join("A"), true);
    let links = Arc::new(tm_sync::LinkStore::open(&tmp.path().join("links.json")));
    let refused = Arc::new(std::sync::Mutex::new(None));
    let outbox = Arc::new(tm_sync::queue::Outbox::open(
        &tmp.path().join("outbox.json"),
    ));
    let id = DeviceIdentity {
        id: "A-id".into(),
        name: "Instance A".into(),
        pairing_code: "123456".into(),
    };
    let server = SyncServer::start_with(
        id,
        a.local.clone(),
        backup_fn(&a.db),
        Some(0),
        Some(tm_sync::server::Linking {
            profile_id: "prof-1".into(),
            profile_name: "Main".into(),
            links: Arc::clone(&links),
            refused: Arc::clone(&refused),
            outbox: Arc::clone(&outbox),
        }),
    )
    .await
    .unwrap();
    let port = server.port();
    let client = SyncClient::new().unwrap();

    let ping = client.probe("127.0.0.1", port).await.unwrap();
    assert_eq!(ping.id, "A-id");

    // Wrong code: refused, nothing kept.
    let wrong = client
        .link("127.0.0.1", port, "000000", &link_req("prof-1"))
        .await;
    assert!(matches!(wrong, Err(SyncError::WrongCode)), "{wrong:?}");
    // Another profile: 409 with A's profile, no key, nothing kept.
    match client
        .link("127.0.0.1", port, "123456", &link_req("prof-2"))
        .await
    {
        Err(SyncError::OtherProfile(r)) => {
            assert_eq!(r.profile_id, "prof-1");
            assert!(r.key.is_none());
        }
        other => panic!("{other:?}"),
    }
    assert!(links.list().is_empty());
    let r = refused.lock().unwrap().clone().unwrap();
    assert_eq!(
        (
            r.peer_device_id.as_str(),
            r.profile_id.as_str(),
            r.address.as_str()
        ),
        ("B-id", "prof-2", "127.0.0.1:27200"),
        "the refused device is remembered so this side can offer to take its profile"
    );

    let ok = client
        .link("127.0.0.1", port, "123456", &link_req("prof-1"))
        .await
        .unwrap();
    let key = ok.key.clone().unwrap();
    assert_eq!(ok.device_id, "A-id");
    let kept = links.by_peer("B-id").unwrap();
    assert_eq!(kept.key, key);
    assert_eq!(kept.last_address, "127.0.0.1:27200");

    // The key works where the code did.
    let peer = PeerDevice {
        id: "A-id".into(),
        name: "A".into(),
        host: "127.0.0.1".into(),
        port,
    };
    let m = client.fetch_manifest(&peer, &key).await.unwrap();
    assert_eq!(m.device_id, "A-id");
    // The outbox answers the linked device with its own events, and needs the key.
    let ev = |id: &str, ch: &str| tm_sync::queue::DoneEvent {
        chapter: tm_sync::queue::ChapterRef {
            source_id: 42,
            manga_url: "/m".into(),
            chapter_url: ch.into(),
        },
        event_id: id.into(),
        where_: tm_sync::queue::Where::Server,
        device_id: "A-id".into(),
        at: 1,
    };
    outbox.add("B-id", ev("e1", "/1"));
    outbox.add("B-id", ev("e2", "/2"));
    outbox.add("C-id", ev("e3", "/3"));
    let got = client.outbox(&peer, &key, None).await.unwrap();
    assert_eq!(got, vec![ev("e1", "/1"), ev("e2", "/2")]);
    let got = client.outbox(&peer, &key, Some("e1")).await.unwrap();
    assert_eq!(got, vec![ev("e2", "/2")]);
    assert!(matches!(
        client.outbox(&peer, "123456", None).await,
        Err(SyncError::Unlinked)
    ));
    // Once A forgets the link, B learns it was unlinked.
    assert!(links.remove("B-id").unwrap());
    let gone = client.fetch_manifest(&peer, &key).await;
    assert!(matches!(gone, Err(SyncError::Unlinked)), "{gone:?}");
    // A made-up key of the right shape is refused too.
    let fake = tm_sync::links::new_key().unwrap();
    assert!(matches!(
        client.fetch_manifest(&peer, &fake).await,
        Err(SyncError::Unlinked)
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_server_without_profiles_does_not_link() {
    let tmp = tempfile::tempdir().unwrap();
    let a = instance(&tmp.path().join("A"), false);
    let server = serve(&a, "123456").await;
    let client = SyncClient::new().unwrap();
    let r = client
        .link("127.0.0.1", server.port(), "123456", &link_req("p"))
        .await;
    assert!(matches!(r, Err(SyncError::Http(_))), "{r:?}");
}
