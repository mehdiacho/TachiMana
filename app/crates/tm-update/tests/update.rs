#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use axum::routing::get;
use serde_json::json;
use sha2::{Digest, Sha256};
use tm_update::{pick, Asset, Platform, UpdateError, Updater};

fn listing(base: &str) -> Vec<u8> {
    let asset = |name: &str, size: usize| json!({"name": name, "size": size, "browser_download_url": format!("{base}/dl/{name}")});
    json!([
        // The Kotlin app's releases share the repo and must be ignored.
        {"tag_name": "v9.9.9", "html_url": "k", "assets": [asset("TachiMana-9.9.9.exe", 1)]},
        {"tag_name": "app-v0.3.0", "draft": true, "html_url": "d", "assets": []},
        {"tag_name": "app-v0.4.0-beta", "prerelease": true, "html_url": "p", "assets": []},
        {"tag_name": "app-v0.2.0", "body": "Notes", "html_url": "https://github.com/x/releases/tag/app-v0.2.0", "assets": [
            asset("TachiMana_0.2.0_amd64.AppImage", 5),
            asset("TachiMana_0.2.0_x64-setup.exe", 11),
            asset("TachiMana_0.2.0_x64-setup.exe.sha256", 64),
        ]},
        {"tag_name": "app-v0.1.5", "html_url": "o", "assets": []}
    ])
    .to_string()
    .into_bytes()
}

#[test]
fn picks_the_newest_app_release_and_its_installer() {
    let l = listing("http://h");
    let u = pick(&l, "0.1.0", Platform::Windows).unwrap().unwrap();
    assert_eq!(u.version, "0.2.0");
    assert_eq!(u.notes, "Notes");
    let a = u.installer.unwrap();
    assert_eq!(a.name, "TachiMana_0.2.0_x64-setup.exe");
    assert_eq!(a.size, 11);
    assert_eq!(
        a.sha256_url.as_deref(),
        Some("http://h/dl/TachiMana_0.2.0_x64-setup.exe.sha256")
    );

    let other = pick(&l, "0.1.0", Platform::Other).unwrap().unwrap();
    assert!(other.installer.is_none());
    assert!(other.page_url.ends_with("app-v0.2.0"));

    assert_eq!(pick(&l, "0.2.0", Platform::Windows).unwrap(), None);
    assert_eq!(pick(&l, "0.10.0", Platform::Windows).unwrap(), None);
    assert!(pick(b"{}", "0.1.0", Platform::Windows).is_err());
    assert_eq!(pick(b"[]", "0.1.0", Platform::Windows).unwrap(), None);
}

const BODY: &[u8] = b"installer!!";

async fn server() -> String {
    let good = format!(
        "{:x}  TachiMana_0.2.0_x64-setup.exe\n",
        Sha256::digest(BODY)
    );
    let app = axum::Router::new()
        .route("/dl/ok.exe", get(|| async { BODY }))
        .route("/dl/ok.exe.sha256", get(move || async move { good }))
        .route("/dl/bad.sha256", get(|| async { "0".repeat(64) }))
        .route("/dl/junk.sha256", get(|| async { "not a hash" }))
        .route(
            "/dl/missing.exe",
            get(|| async { (axum::http::StatusCode::NOT_FOUND, "") }),
        );
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    format!("http://{addr}")
}

fn asset(base: &str, file: &str, size: u64, sha: Option<&str>) -> Asset {
    Asset {
        name: "TachiMana_0.2.0_x64-setup.exe".into(),
        url: format!("{base}/dl/{file}"),
        size,
        sha256_url: sha.map(|s| format!("{base}/dl/{s}")),
    }
}

#[tokio::test]
async fn downloads_and_verifies_the_installer() {
    let base = server().await;
    let up = Updater::with(&base, "x/y").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut seen = Vec::new();
    let f = up
        .download(
            &asset(&base, "ok.exe", 11, Some("ok.exe.sha256")),
            dir.path(),
            |n, t| seen.push((n, t)),
        )
        .await
        .unwrap();
    assert_eq!(std::fs::read(&f).unwrap(), BODY);
    assert_eq!(seen.last(), Some(&(11, 11)));
    // No checksum published: the size check still applies.
    up.download(&asset(&base, "ok.exe", 11, None), dir.path(), |_, _| {})
        .await
        .unwrap();
}

#[tokio::test]
async fn a_bad_download_leaves_nothing_behind() {
    let base = server().await;
    let up = Updater::with(&base, "x/y").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let cases = [
        asset(&base, "ok.exe", 11, Some("bad.sha256")),
        asset(&base, "ok.exe", 11, Some("junk.sha256")),
        asset(&base, "ok.exe", 5, None),
        asset(&base, "ok.exe", 50, None),
        asset(&base, "missing.exe", 11, None),
    ];
    for a in &cases {
        assert!(
            up.download(a, dir.path(), |_, _| {}).await.is_err(),
            "{a:?}"
        );
    }
    assert!(matches!(
        up.download(&cases[0], dir.path(), |_, _| {}).await,
        Err(UpdateError::Checksum)
    ));
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        0,
        "no installer or .part left"
    );
    // A name that would leave the folder is refused before anything is fetched.
    let evil = Asset {
        name: "../evil.exe".into(),
        ..cases[2].clone()
    };
    assert!(matches!(
        up.download(&evil, dir.path(), |_, _| {}).await,
        Err(UpdateError::Protocol(_))
    ));
}

#[tokio::test]
async fn check_reads_the_listing() {
    let app =
        axum::Router::new().route("/repos/x/y/releases", get(|| async { listing("http://h") }));
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    let up = Updater::with(&format!("http://{addr}"), "x/y").unwrap();
    assert_eq!(
        up.check("0.1.0", Platform::Windows)
            .await
            .unwrap()
            .unwrap()
            .version,
        "0.2.0"
    );
    let missing = Updater::with(&format!("http://{addr}"), "x/z").unwrap();
    assert!(matches!(
        missing.check("0.1.0", Platform::Windows).await,
        Err(UpdateError::Status(404))
    ));
}
