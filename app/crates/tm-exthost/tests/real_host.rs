//! Against the real Kotlin host (PLAN §P5 gate). Ignored by default; run with
//! `TM_EXTHOST_DIR=<folder with ext-host.jar> cargo test -p tm-exthost -- --ignored`.
//! Needs Java and network access (Keiyoushi repo, MangaDex).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use tm_exthost::model::Manga;
use tm_exthost::{ExtHost, HostConfig};

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs TM_EXTHOST_DIR, Java and network"]
async fn install_browse_read() {
    let host_dir = std::env::var_os("TM_EXTHOST_DIR").expect("set TM_EXTHOST_DIR");
    let data = tempfile::tempdir().unwrap();
    let cfg = HostConfig {
        host_dir: host_dir.into(),
        data_dir: data.path().into(),
        java: None,
    };
    let host = ExtHost::spawn(&cfg).unwrap();
    let c = host.client();

    assert_eq!(c.health().await.unwrap().extensions, 0);
    let repo = c
        .inspect_repo("https://raw.githubusercontent.com/keiyoushi/extensions/repo")
        .await
        .unwrap();
    assert!(repo.index_url.ends_with("index.pb"));
    let catalog = c.catalog(&repo.index_url).await.unwrap();
    let mangadex = catalog
        .iter()
        .find(|e| e.pkg_name.contains("mangadex"))
        .expect("MangaDex in catalog");
    let ext = c
        .install(&repo.index_url, &mangadex.pkg_name)
        .await
        .unwrap();
    let source = ext
        .sources
        .iter()
        .find(|s| s.lang == "en")
        .unwrap_or(&ext.sources[0])
        .id;

    let popular = c.popular(source, 1).await.unwrap();
    assert!(!popular.mangas.is_empty());
    let mut chapters = Vec::new();
    for m in popular.mangas.iter().take(5) {
        let m = Manga {
            url: m.url.clone(),
            title: m.title.clone(),
            ..Default::default()
        };
        if let Ok(u) = c.manga(source, &m, true, true).await {
            if !u.chapters.is_empty() {
                chapters = u.chapters;
                break;
            }
        }
    }
    assert!(
        !chapters.is_empty(),
        "no title with chapters in the first five"
    );
    let pages = c.pages(source, &chapters[0]).await.unwrap();
    assert!(!pages.is_empty());
    let img = c.page_image(source, &pages[0]).await.unwrap();
    assert!(
        img.content_type.starts_with("image/"),
        "{}",
        img.content_type
    );
    assert!(img.bytes.len() > 100);
    c.shutdown().await.unwrap();
}
