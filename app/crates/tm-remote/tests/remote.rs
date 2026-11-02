//! Port of `RemoteSelfTest.kt` (9 checks) against an in-memory WebDAV server, plus the upload
//! queue gates from PLAN §P6: a verified upload, a size mismatch keeps the local copy, and the queue
//! survives a restart.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use tm_remote::fake as support;

use std::path::Path;

use tm_remote::mirror::ChapterRef;
use tm_remote::names::{chapter_dir_name, chapter_file_names, md5_hex};
use tm_remote::{
    Lookup, Mirror, RemoteIndex, Role, UploadQueue, UploadStatus, WebDav, WebDavConfig,
};

const PNG: &[u8] =
    b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x06\0\0\0\x1f\x15\xc4\x89";

fn sample_cbz(dir: &Path) -> Vec<u8> {
    let out = dir.join("sample.cbz");
    tm_archive::pack_cbz(
        &[
            ("001.png".into(), PNG.to_vec()),
            ("002.png".into(), PNG.to_vec()),
        ],
        &out,
    )
    .unwrap();
    std::fs::read(out).unwrap()
}

fn dav(base: &str) -> WebDav {
    WebDav::new(&WebDavConfig {
        url: base.into(),
        ..Default::default()
    })
    .unwrap()
}

fn segs(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| (*s).to_owned()).collect()
}

#[tokio::test]
async fn remote_self_test() {
    let tmp = tempfile::tempdir().unwrap();
    let url = "https://example.com/chapter/123";
    let hash = &md5_hex(url)[..6];
    let file = format!("Chapter 1_{hash}.cbz");

    // 1. Mihon names.
    assert_eq!(
        chapter_dir_name("Chapter 1", None, url, false),
        format!("Chapter 1_{hash}")
    );
    assert!(chapter_file_names("Chapter 1", Some("ScanTeam"), url)
        .contains(&format!("ScanTeam_Chapter 1_{hash}.cbz")));

    let srv = support::start().await;
    let cbz = sample_cbz(tmp.path());
    srv.put_file(&format!("/webdav/MangaDex (EN)/Chainsaw Man/{file}"), &cbz);
    let d = dav(&srv.base);

    // 2. PROPFIND depth 0.
    d.test_connection().await.unwrap();

    // 3. PROPFIND depth 1: files only, the folder itself left out.
    let listing = d
        .list(&segs(&["MangaDex (EN)", "Chainsaw Man"]))
        .await
        .unwrap();
    assert_eq!(listing.len(), 1);
    assert_eq!(listing[0].name, file);
    assert!(!listing[0].is_dir);
    assert_eq!(listing[0].size, cbz.len() as u64);
    let folders = d.list(&segs(&["MangaDex (EN)"])).await.unwrap();
    assert_eq!(folders.len(), 1);
    assert!(folders[0].is_dir && folders[0].name == "Chainsaw Man");
    assert!(
        d.list(&segs(&["Nope"])).await.unwrap().is_empty(),
        "missing folder lists as empty"
    );

    // 4. HEAD size.
    let path = segs(&["MangaDex (EN)", "Chainsaw Man", &file]);
    assert_eq!(
        d.size_of(&path).await.unwrap(),
        Some(Some(cbz.len() as u64))
    );
    assert_eq!(
        d.size_of(&segs(&["MangaDex (EN)", "x.cbz"])).await.unwrap(),
        None
    );

    // 5. Index lookups.
    let index_file = tmp.path().join("remote_index.json");
    let index = RemoteIndex::open(&index_file);
    let cands = vec![file.clone()];
    assert_eq!(
        index.resolve("MangaDex (EN)", "Chainsaw Man", &cands),
        Lookup::Unknown
    );
    assert_eq!(
        index
            .refresh(&d, "MangaDex (EN)", "Chainsaw Man")
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        index.resolve("MangaDex (EN)", "Chainsaw Man", &cands),
        Lookup::Found(file.clone())
    );
    assert_eq!(
        index.resolve("MangaDex (EN)", "Chainsaw Man", &["other.cbz".into()]),
        Lookup::Absent
    );

    // 6. Persistence (the Kotlin check re-read the first index by mistake; this reads the second).
    let index2 = RemoteIndex::open(&index_file);
    assert_eq!(
        index2.resolve("MangaDex (EN)", "Chainsaw Man", &cands),
        Lookup::Found(file.clone())
    );

    // 7. Mirror: holds.
    let mirror = Mirror::new(tmp.path().join("cache"), 2048);
    let ch = ChapterRef {
        source_name: Some("MangaDex (EN)"),
        source_id: 100,
        manga_title: "Chainsaw Man",
        chapter_name: "Chapter 1",
        scanlator: None,
        chapter_url: url,
    };
    assert!(mirror.holds(&index, &ch));
    // Unknown source name: found across sources.
    assert!(mirror.holds(
        &index,
        &ChapterRef {
            source_name: None,
            ..ch.clone()
        }
    ));

    // 8. Fetch into the SHA-256 cache.
    let segments = mirror.segments_for(&index, &ch).unwrap();
    let mut seen = 0u64;
    let fetched = mirror
        .fetch(&d, &segments, |n, _| seen = n)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(std::fs::read(&fetched).unwrap(), cbz);
    assert_eq!(seen, cbz.len() as u64);
    assert_eq!(fetched, mirror.cache_file_for(&segments));
    // A second fetch is served from the cache, without a GET.
    let gets = || {
        srv.dav
            .log
            .lock()
            .unwrap()
            .iter()
            .filter(|l| l.starts_with("GET"))
            .count()
    };
    let before = gets();
    mirror
        .fetch(&d, &segments, |_, _| ())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(gets(), before);

    // 9. The fetched archive opens.
    let pages = tm_archive::page_names(&fetched).unwrap();
    assert_eq!(pages.len(), 2);
    assert!(!tm_archive::read_page(&fetched, &pages[0])
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn sign_in_is_checked() {
    let srv = support::start().await;
    // "user:pass" in Basic auth.
    *srv.dav.auth.lock().unwrap() = Some("Basic dXNlcjpwYXNz".into());
    let wrong = WebDav::new(&WebDavConfig {
        url: srv.base.clone(),
        username: "user".into(),
        password: "nope".into(),
    })
    .unwrap();
    let e = wrong.test_connection().await.unwrap_err().to_string();
    assert!(e.contains("Sign-in was refused"), "{e}");
    assert!(
        !format!("{wrong:?}").contains("nope"),
        "Debug never prints the password"
    );
    let right = WebDav::new(&WebDavConfig {
        url: srv.base.clone(),
        username: "user".into(),
        password: "pass".into(),
    })
    .unwrap();
    right.test_connection().await.unwrap();
}

#[tokio::test]
async fn names_are_escaped_on_the_wire() {
    let srv = support::start().await;
    let d = dav(&srv.base);
    let weird = segs(&["Src #1", "100% Manga?", "Ch 1 & 2.cbz"]);
    let tmp = tempfile::tempdir().unwrap();
    let f = tmp.path().join("x.cbz");
    std::fs::write(&f, b"abc").unwrap();
    d.put_file(&weird, &f).await.unwrap();
    assert_eq!(
        srv.file("/webdav/Src #1/100% Manga?/Ch 1 & 2.cbz").unwrap(),
        b"abc"
    );
    let l = d.list(&segs(&["Src #1", "100% Manga?"])).await.unwrap();
    assert_eq!(l[0].name, "Ch 1 & 2.cbz");
    assert!(d.url_for(&segs(&[".."])).is_err());
    d.move_to(&weird, &segs(&["Src #1", "moved.cbz"]))
        .await
        .unwrap();
    assert!(srv.file("/webdav/Src #1/moved.cbz").is_some());
    d.delete(&segs(&["Src #1"])).await.unwrap();
    assert!(srv.file("/webdav/Src #1/moved.cbz").is_none());
    d.delete(&segs(&["Src #1"])).await.unwrap();
}

/// A downloaded chapter folder under `root`, laid out like tm-download writes it.
fn chapter_dir(root: &Path, name: &str) -> std::path::PathBuf {
    let d = root.join("123").join(name);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("001.png"), PNG).unwrap();
    std::fs::write(d.join("002.png"), PNG).unwrap();
    std::fs::write(d.join("meta.json"), b"{}").unwrap();
    std::fs::write(d.join(".done"), b"").unwrap();
    d
}

#[tokio::test]
async fn verified_upload_mirror_keeps_archive_evicts() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("downloads");
    let srv = support::start().await;
    let d = dav(&srv.base);
    let index = RemoteIndex::open(tmp.path().join("idx.json"));
    index.refresh(&d, "Src", "Manga").await.unwrap();
    let q = UploadQueue::open(tmp.path().join("uploads.json"), &root);

    let a = chapter_dir(&root, "a");
    assert!(q.enqueue(
        a.clone(),
        segs(&["Src", "Manga", "Ch 1_abc123.cbz"]),
        Role::Mirror,
        "Ch 1".into()
    ));
    assert!(
        !q.enqueue(
            a.clone(),
            segs(&["Src", "Manga", "Ch 1_abc123.cbz"]),
            Role::Mirror,
            "Ch 1".into()
        ),
        "de-duped"
    );
    let done = q.run_next(&d, &index).await.unwrap();
    assert_eq!(done.status, UploadStatus::Done, "{:?}", done.error);
    assert!(a.join("001.png").exists(), "mirror keeps the local copy");
    let remote = srv.file("/webdav/Src/Manga/Ch 1_abc123.cbz").unwrap();
    let names = {
        let p = tmp.path().join("check.cbz");
        std::fs::write(&p, &remote).unwrap();
        tm_archive::page_names(&p).unwrap()
    };
    assert_eq!(names, ["001.png", "002.png"], "only pages are packed");
    assert_eq!(
        index.resolve("Src", "Manga", &["Ch 1_abc123.cbz".into()]),
        Lookup::Found("Ch 1_abc123.cbz".into())
    );

    let b = chapter_dir(&root, "b");
    q.enqueue(
        b.clone(),
        segs(&["Src", "Manga", "Ch 2.cbz"]),
        Role::Archive,
        "Ch 2".into(),
    );
    assert_eq!(
        q.run_next(&d, &index).await.unwrap().status,
        UploadStatus::Done
    );
    assert!(
        !b.exists(),
        "archive removes the local copy after a verified upload"
    );
    assert!(q.run_next(&d, &index).await.is_none());
    assert!(
        !tmp.path().join("upload-0.cbz").exists(),
        "the packed file is cleaned up"
    );
}

#[tokio::test]
async fn size_mismatch_keeps_the_local_copy() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("downloads");
    let srv = support::start().await;
    srv.dav
        .short_puts
        .store(true, std::sync::atomic::Ordering::Relaxed);
    let d = dav(&srv.base);
    let index = RemoteIndex::open(tmp.path().join("idx.json"));
    let q = UploadQueue::open(tmp.path().join("uploads.json"), &root);
    let a = chapter_dir(&root, "a");
    q.enqueue(
        a.clone(),
        segs(&["Src", "Manga", "Ch 1.cbz"]),
        Role::Archive,
        "Ch 1".into(),
    );
    let j = q.run_next(&d, &index).await.unwrap();
    assert_eq!(j.status, UploadStatus::Failed);
    assert!(j.error.unwrap().contains("local copy was kept"));
    assert!(
        a.join("001.png").exists(),
        "archive never evicts after a bad upload"
    );
    assert_eq!(
        index.resolve("Src", "Manga", &["Ch 1.cbz".into()]),
        Lookup::Unknown
    );
}

#[tokio::test]
async fn queue_survives_restart_and_stays_inside_root() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("downloads");
    let file = tmp.path().join("uploads.json");
    let a = chapter_dir(&root, "a");
    {
        let q = UploadQueue::open(&file, &root);
        q.enqueue(
            a.clone(),
            segs(&["Src", "Manga", "Ch 1.cbz"]),
            Role::Archive,
            "Ch 1".into(),
        );
        q.enqueue(
            tmp.path().to_path_buf(),
            segs(&["Src", "Manga", "evil.cbz"]),
            Role::Archive,
            "evil".into(),
        );
    }
    // Pretend the app died mid-upload.
    let text = std::fs::read_to_string(&file)
        .unwrap()
        .replacen("\"QUEUED\"", "\"UPLOADING\"", 1);
    std::fs::write(&file, text).unwrap();

    let q = UploadQueue::open(&file, &root);
    let jobs = q.jobs();
    assert_eq!(jobs.len(), 2);
    assert!(
        jobs.iter().all(|j| j.status == UploadStatus::Queued),
        "{jobs:?}"
    );

    let srv = support::start().await;
    let d = dav(&srv.base);
    let index = RemoteIndex::open(tmp.path().join("idx.json"));
    assert_eq!(
        q.run_next(&d, &index).await.unwrap().status,
        UploadStatus::Done
    );
    let evil = q.run_next(&d, &index).await.unwrap();
    assert_eq!(evil.status, UploadStatus::Failed);
    assert!(evil
        .error
        .unwrap()
        .contains("not inside the downloads folder"));
    assert!(
        tmp.path().join("uploads.json").exists(),
        "a folder outside the root is never deleted"
    );
    // New ids continue after the saved ones.
    q.enqueue(
        chapter_dir(&root, "c"),
        segs(&["Src", "Manga", "Ch 3.cbz"]),
        Role::Mirror,
        "Ch 3".into(),
    );
    let ids: Vec<u64> = q.jobs().iter().map(|j| j.id).collect();
    assert_eq!(ids, [0, 1, 2]);
}

#[test]
fn listing_parser_handles_entities_and_full_urls() {
    let xml = br#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:">
 <d:response><d:href>http://nas:5005/webdav/A%20B/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop></d:propstat></d:response>
 <d:response><d:href>http://nas:5005/webdav/A%20B/Tom &amp; Jerry.cbz</d:href><d:propstat><d:prop><d:getcontentlength> 42 </d:getcontentlength></d:prop></d:propstat></d:response>
 <d:response><d:href>/webdav/A%20B/Sub/</d:href><d:propstat><d:prop><d:resourcetype><d:collection></d:collection></d:resourcetype></d:prop></d:propstat></d:response>
</d:multistatus>"#;
    let e = tm_remote::webdav::parse_multistatus(xml, "/webdav/A%20B").unwrap();
    assert_eq!(e.len(), 2, "{e:?}");
    assert_eq!(
        (e[0].name.as_str(), e[0].is_dir, e[0].size),
        ("Tom & Jerry.cbz", false, 42)
    );
    assert_eq!((e[1].name.as_str(), e[1].is_dir), ("Sub", true));
    assert!(tm_remote::webdav::parse_multistatus(b"<a><b></a>", "/").is_err());
}
