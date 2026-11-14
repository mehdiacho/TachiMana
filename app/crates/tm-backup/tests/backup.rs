//! Ports of the backup half of `desktop/.../sync/SyncSelfTest.kt`, plus the wire details the
//! Kotlin decoder depends on.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::collections::HashMap;
use std::path::Path;

use prost::Message;
use tm_backup::*;
use tm_db::{Db, ExtensionRepo};

const PNG_1X1: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0B, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x60, 0x60, 0x60, 0x60,
    0x00, 0x00, 0x00, 0x04, 0x00, 0x01, 0xF6, 0x17, 0x38, 0x55, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

fn write_cbz(path: &Path) {
    use std::io::Write;
    let f = std::fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(f);
    for p in ["001.png", "002.png"] {
        zip.start_file(p, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(PNG_1X1).unwrap();
    }
    zip.finish().unwrap();
}

/// `local/Sample Manga/` with a cover and two chapters, scanned into a fresh database.
fn seeded(dir: &Path) -> Db {
    let local = dir.join("local");
    let m = local.join("Sample Manga");
    std::fs::create_dir_all(&m).unwrap();
    std::fs::write(m.join("cover.png"), PNG_1X1).unwrap();
    write_cbz(&m.join("Chapter 1.cbz"));
    write_cbz(&m.join("Chapter 2.cbz"));
    let db = Db::open(&dir.join("a.db")).unwrap();
    tm_local::scan(&db, &local).unwrap();
    db
}

fn none() -> HashMap<i64, String> {
    HashMap::new()
}

#[test]
fn restore_from_file_imports_library_and_relinks_covers() {
    let tmp = tempfile::tempdir().unwrap();
    let a = seeded(&tmp.path().join("A"));
    let file = tmp.path().join("export.tachibk");
    std::fs::write(&file, codec::encode(&export(&a, &none()).unwrap()).unwrap()).unwrap();
    assert!(std::fs::metadata(&file).unwrap().len() > 0);

    // Instance C: copy the files over (a pull does this), then restore.
    let c_dir = tmp.path().join("C");
    let c_local = c_dir.join("local").join("Sample Manga");
    std::fs::create_dir_all(&c_local).unwrap();
    for f in ["cover.png", "Chapter 1.cbz", "Chapter 2.cbz"] {
        std::fs::copy(
            tmp.path().join("A/local/Sample Manga").join(f),
            c_local.join(f),
        )
        .unwrap();
    }
    let c = Db::open(&c_dir.join("c.db")).unwrap();
    let s = import(&c, &codec::decode(&std::fs::read(&file).unwrap()).unwrap()).unwrap();
    assert_eq!((s.manga_added, s.chapters_added), (1, 2));
    let lib = c.library().unwrap();
    assert_eq!(lib.len(), 1);
    assert_eq!(lib[0].title, "Sample Manga");
    assert_eq!(lib[0].source, 0, "local source");
    // The sender's absolute cover path is not carried over...
    assert_eq!(lib[0].thumbnail_url, None);
    // ...until relinked to this machine's own file.
    assert_eq!(relink_local_covers(&c, &c_dir.join("local")).unwrap(), 1);
    let thumb = c.library().unwrap()[0].thumbnail_url.clone().unwrap();
    assert!(thumb.starts_with(c_dir.join("local").to_str().unwrap()));
    assert!(Path::new(&thumb).exists());

    // Idempotent: the same backup again adds nothing.
    let again = import(&c, &codec::decode(&std::fs::read(&file).unwrap()).unwrap()).unwrap();
    assert_eq!((again.manga_added, again.chapters_added), (0, 0));
    assert_eq!(again.chapters_updated, 0);
}

#[test]
fn extension_stores_in_a_backup_are_added_on_restore() {
    let tmp = tempfile::tempdir().unwrap();
    let a = seeded(tmp.path());
    a.upsert_extension_repo(&ExtensionRepo {
        base_url: "https://example.com/testrepo".into(),
        name: "Test Repo".into(),
        short_name: Some("TR".into()),
        website: "https://example.com".into(),
        signing_key_fingerprint: "aa:bb:cc".into(),
    })
    .unwrap();
    let bytes = codec::encode(&export(&a, &none()).unwrap()).unwrap();
    let decoded = codec::decode(&bytes).unwrap();
    assert_eq!(decoded.backup_extension_stores.len(), 1);
    assert_eq!(
        decoded.backup_extension_stores[0].index_url(),
        "https://example.com/testrepo"
    );

    let c = Db::open_in_memory().unwrap();
    let s = import(&c, &decoded).unwrap();
    assert_eq!(s.repos_added.len(), 1);
    assert_eq!(s.repos_added[0].name(), "Test Repo");
    let repos = c.extension_repos().unwrap();
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0].base_url, "https://example.com/testrepo");
    assert_eq!(repos[0].signing_key_fingerprint, "aa:bb:cc");
    assert!(import(&c, &decoded).unwrap().repos_added.is_empty());
}

#[test]
fn a_repo_that_is_not_a_web_address_is_not_added() {
    let mut b = Backup::default();
    b.backup_extension_stores.push(BackupExtensionStore {
        index_url: Some("file:///etc/passwd".into()),
        ..Default::default()
    });
    let c = Db::open_in_memory().unwrap();
    assert!(import(&c, &b).unwrap().repos_added.is_empty());
    assert!(c.extension_repos().unwrap().is_empty());
}

#[test]
fn installed_extensions_and_download_state_survive_a_round_trip() {
    let src = Db::open_in_memory().unwrap();
    let mut seed = Backup::default();
    let mut m = BackupManga::new(42, "/m/dl");
    m.title = "Downloaded".into();
    m.chapters = vec![
        BackupChapter::new("/c/1", "Ch 1"),
        BackupChapter::new("/c/2", "Ch 2"),
        BackupChapter::new("/c/3", "Ch 3"),
    ];
    m.chapters[0].read = true;
    m.tracking = vec![BackupTracking {
        sync_id: Some(2),
        media_id: 777,
        ..Default::default()
    }];
    seed.backup_manga.push(m);
    import(&src, &seed).unwrap();

    let mut b = export(&src, &none()).unwrap();
    let ext = BackupExtension {
        pkg_name: "eu.kanade.tachiyomi.extension.en.test".into(),
        name: Some("Test".into()),
        version_code: Some(5),
        repo_index_url: Some("https://example.com/index.min.json".into()),
        source_ids: vec![42],
        ..Default::default()
    };
    annotate(
        &mut b,
        vec![ext.clone(), ext.clone()],
        |source, manga, ch| source == 42 && manga == "/m/dl" && ch != "/c/2",
    );
    let decoded = codec::decode(&codec::encode(&b).unwrap()).unwrap();
    assert_eq!(decoded.backup_extensions.len(), 2);

    let dst = Db::open_in_memory().unwrap();
    let s = import(&dst, &decoded).unwrap();
    assert_eq!(s.extensions, vec![ext], "one entry per package");
    assert_eq!(s.source_ids, vec![42]);
    assert_eq!(s.trackers, vec![2]);
    let id = dst.library().unwrap()[0].id;
    let chapters = dst.chapters(id).unwrap();
    let by_url = |u: &str| chapters.iter().find(|c| c.url == u).unwrap().id;
    let mut got: Vec<(i64, bool)> = s
        .downloaded
        .iter()
        .map(|d| (d.chapter_id, d.read))
        .collect();
    got.sort_unstable();
    let mut want = vec![(by_url("/c/1"), true), (by_url("/c/3"), false)];
    want.sort_unstable();
    assert_eq!(got, want);
    assert!(s.downloaded.iter().all(|d| d.manga_id == id));
}

#[test]
fn a_stock_mihon_backup_offers_its_sources_and_no_downloads() {
    let db = Db::open_in_memory().unwrap();
    let s = import(&db, &mihon_backup()).unwrap();
    assert!(s.extensions.is_empty());
    assert!(s.downloaded.is_empty());
    assert!(s.source_ids.contains(&99));
}

fn mihon_backup() -> Backup {
    let mut m = BackupManga::new(99, "/m/1");
    m.title = "Mihon Title".into();
    m.date_added = 1_700_000_000;
    m.categories = vec![0];
    m.notes = "reread after vol 3".into();
    m.version = 7;
    m.last_modified_at = 1_700_000_500;
    m.favorite_modified_at = Some(1_700_000_400);
    m.memo = Some("{\"pinned\":true}".into());
    m.excluded_scanlators = vec!["BadGroup".into()];
    let mut ch = BackupChapter::new("/m/1/c1", "Chapter 1");
    ch.read = true;
    ch.last_page_read = 12;
    ch.last_modified_at = 1_700_000_600;
    ch.version = 3;
    ch.memo = Some("{\"note\":\"x\"}".into());
    m.chapters = vec![ch];
    m.history = vec![BackupHistory {
        url: Some("/m/1/c1".into()),
        last_read: Some(1_700_000_700),
        read_duration: 90_000,
    }];
    m.tracking = vec![BackupTracking {
        sync_id: Some(2),
        library_id: Some(5),
        media_id: 4242,
        private: true,
        ..Default::default()
    }];
    Backup {
        backup_manga: vec![m],
        backup_categories: vec![BackupCategory {
            name: Some("Reading".into()),
            order: 0,
            id: 11,
            flags: 0,
        }],
        backup_sources: vec![BackupSource {
            name: "Some Source".into(),
            source_id: Some(99),
        }],
        backup_preferences: vec![vec![10, 3, 97, 98, 99]],
        backup_source_preferences: vec![vec![10, 2, 120, 121]],
        backup_extension_stores: vec![BackupExtensionStore {
            index_url: Some("https://example.com/index.pb".into()),
            name: Some("Idx".into()),
            badge_label: Some("I".into()),
            contact_website: "https://example.com".into(),
            signing_key: "ff:ee".into(),
            contact_discord: Some("https://discord.gg/x".into()),
            is_legacy: Some(false),
            extension_list_url: Some("https://example.com/list.pb".into()),
        }],
        backup_extensions: Vec::new(),
        remote_storage: None,
    }
}

#[test]
fn a_phone_backup_brings_its_nas_connection_and_sealed_password() {
    let sealed = tm_backup::seal::seal("pw", "phrase", [1; 16], [2; 12]).unwrap();
    let field = |tag: u8, b: &[u8]| {
        let mut out = vec![(tag << 3) | 2, u8::try_from(b.len()).unwrap()];
        out.extend_from_slice(b);
        out
    };
    let mut section = field(1, b"nas.local");
    section.extend(field(4, b"Mihon"));
    section.extend(field(6, &sealed));
    let mut b = mihon_backup();
    b.remote_storage = Some(section);
    // Through the file format, as a restore reads it.
    let decoded = codec::decode(&codec::encode(&b).unwrap()).unwrap();
    let db = Db::open_in_memory().unwrap();
    let s = import(&db, &decoded).unwrap();
    let nas = s.nas.unwrap();
    assert_eq!(nas.host, "nas.local");
    assert_eq!(nas.folder.as_deref(), Some("Mihon"));
    assert_eq!(
        tm_backup::seal::unseal(&s.nas_sealed_password.unwrap(), "phrase")
            .unwrap()
            .as_str(),
        "pw"
    );
    // The computer's own backups never carry the section.
    assert_eq!(export(&db, &none()).unwrap().remote_storage, None);
}

#[test]
fn mihon_round_trip_keeps_every_field_we_store() {
    let db = Db::open_in_memory().unwrap();
    let mihon = mihon_backup();
    import(&db, &mihon).unwrap();
    let round = export(&db, &none()).unwrap();

    let rm = round
        .backup_manga
        .iter()
        .find(|m| m.url() == "/m/1")
        .unwrap();
    assert_eq!(rm.notes, "reread after vol 3");
    assert_eq!((rm.version, rm.last_modified_at), (7, 1_700_000_500));
    assert_eq!(rm.favorite_modified_at, Some(1_700_000_400));
    assert_eq!(rm.memo(), "{\"pinned\":true}");
    let rc = &rm.chapters[0];
    assert_eq!((rc.version, rc.last_modified_at), (3, 1_700_000_600));
    assert_eq!(rc.memo(), "{\"note\":\"x\"}");
    assert_eq!(rc.last_page_read, 12);
    assert!(rc.read);
    assert_eq!(rm.history.len(), 1);
    assert_eq!(rm.history[0].last_read, Some(1_700_000_700));
    assert_eq!(rm.history[0].read_duration, 90_000);
    assert_eq!(rm.tracking.len(), 1);
    assert_eq!(rm.tracking[0].media_id, 4242);
    assert_eq!(rm.tracking[0].sync_id, Some(2));
    assert_eq!(
        rm.categories,
        vec![0],
        "member of the 'Reading' category (order 0)"
    );
    assert_eq!(round.backup_categories[0].name(), "Reading");
    assert_eq!(
        round.backup_extension_stores[0].index_url(),
        "https://example.com/index.pb"
    );
    assert_eq!(round.backup_preferences, vec![vec![10u8, 3, 97, 98, 99]]);
    assert_eq!(
        round.backup_source_preferences,
        vec![vec![10u8, 2, 120, 121]]
    );

    // And through the real container.
    let re = codec::decode(&codec::encode(&round).unwrap()).unwrap();
    assert_eq!(re, round);
}

#[test]
fn passthrough_sections_reexport_byte_identical() {
    // Preference entries are opaque sub-messages, including ones that are not valid protobuf.
    let prefs = vec![
        vec![0x0a, 0x05, b'h', b'e', b'l', b'l', b'o', 0x12, 0x00],
        vec![0xff, 0x00, 0x7f],
    ];
    let src = vec![vec![0x08, 0x96, 0x01]];
    let b = Backup {
        backup_preferences: prefs.clone(),
        backup_source_preferences: src.clone(),
        ..Default::default()
    };
    let db = Db::open_in_memory().unwrap();
    import(&db, &codec::decode(&codec::encode(&b).unwrap()).unwrap()).unwrap();
    let out = export(&db, &none()).unwrap();
    assert_eq!(out.backup_preferences, prefs);
    assert_eq!(out.backup_source_preferences, src);
    // The encoded sections are byte-identical to the input's encoding of them.
    let section = |b: &Backup| {
        Backup {
            backup_preferences: b.backup_preferences.clone(),
            backup_source_preferences: b.backup_source_preferences.clone(),
            ..Default::default()
        }
        .encode_to_vec()
    };
    assert_eq!(section(&out), section(&b));
}

#[test]
fn required_fields_are_on_the_wire_even_when_zero() {
    // kotlinx protobuf rejects a BackupManga without `source`; local manga have source 0.
    let m = BackupManga::new(0, "");
    let bytes = m.encode_to_vec();
    assert!(bytes.starts_with(&[0x08, 0x00]), "{bytes:?}");
    assert!(bytes.windows(2).any(|w| w == [0x12, 0x00]), "url present");
    let t = BackupTracking {
        sync_id: Some(0),
        library_id: Some(0),
        ..Default::default()
    };
    assert_eq!(&t.encode_to_vec()[..4], &[0x08, 0x00, 0x10, 0x00]);
}

#[test]
fn missing_defaults_read_as_kotlin_defaults() {
    // source=5, url="u" and nothing else: favourite must be true, memo "{}".
    let bytes = [0x08, 0x05, 0x12, 0x01, b'u'];
    let m = BackupManga::decode(bytes.as_slice()).unwrap();
    assert!(m.favorite());
    assert_eq!(m.memo(), "{}");
    let ch = BackupChapter::decode([0x0a, 0x01, b'c', 0x12, 0x00].as_slice()).unwrap();
    assert_eq!(ch.memo(), "{}");
}

#[test]
fn packed_and_unpacked_categories_both_decode() {
    // Unpacked (what kotlinx and Mihon write): tag 17 varint, twice.
    let mut unpacked = vec![0x08, 0x01, 0x12, 0x00];
    unpacked.extend([0x88, 0x01, 0x03, 0x88, 0x01, 0x04]);
    assert_eq!(
        BackupManga::decode(unpacked.as_slice()).unwrap().categories,
        vec![3, 4]
    );
    // We write unpacked too.
    let mut m = BackupManga::new(1, "");
    m.categories = vec![3, 4];
    let bytes = m.encode_to_vec();
    assert!(bytes
        .windows(6)
        .any(|w| w == [0x88, 0x01, 0x03, 0x88, 0x01, 0x04]));
}

#[test]
fn progress_only_moves_forward_and_categories_are_additive() {
    let db = Db::open_in_memory().unwrap();
    let mut b = mihon_backup();
    import(&db, &b).unwrap();
    let mine = db.create_category("Mine").unwrap().unwrap();
    let id = db.library().unwrap()[0].id;
    let mut cats: std::collections::BTreeSet<i64> =
        db.category_assignments().unwrap().remove(&id).unwrap();
    cats.insert(mine);
    db.set_manga_categories(id, &cats).unwrap();

    // An older copy: unread, page 3, no categories, history earlier.
    let m = &mut b.backup_manga[0];
    m.chapters[0].read = false;
    m.chapters[0].last_page_read = 3;
    m.categories.clear();
    m.history[0].last_read = Some(1);
    m.notes = "older".into();
    m.last_modified_at = 1;
    let s = import(&db, &b).unwrap();
    assert_eq!(s.chapters_updated, 0);
    let ch = &db.chapters(id).unwrap()[0];
    assert!(ch.read);
    assert_eq!(ch.last_page_read, 12);
    assert_eq!(db.manga(id).unwrap().unwrap().notes, "reread after vol 3");
    assert_eq!(db.category_assignments().unwrap()[&id].len(), 2);
    assert_eq!(db.recents().unwrap()[0].last_read, 1_700_000_700);

    // A newer copy moves things on.
    b.backup_manga[0].chapters[0].bookmark = true;
    b.backup_manga[0].chapters[0].last_page_read = 20;
    assert_eq!(import(&db, &b).unwrap().chapters_updated, 1);
    let ch = &db.chapters(id).unwrap()[0];
    assert!(ch.bookmark);
    assert_eq!(ch.last_page_read, 20);
}

#[test]
fn categories_match_by_name_case_insensitively() {
    let db = Db::open_in_memory().unwrap();
    db.create_category("reading").unwrap();
    let s = import(&db, &mihon_backup()).unwrap();
    assert_eq!(s.categories_added, 0);
    assert_eq!(db.categories().unwrap().len(), 1);
    let id = db.library().unwrap()[0].id;
    assert_eq!(db.category_assignments().unwrap()[&id].len(), 1);
}

#[test]
fn hostile_input_is_an_error_not_a_panic() {
    assert!(
        codec::decode(b"").is_ok(),
        "empty protobuf is an empty backup"
    );
    assert!(codec::decode(&[0x1f, 0x8b, 0x08, 0x00, 1, 2, 3]).is_err());
    assert!(codec::decode(&[0xff, 0xff, 0xff, 0xff]).is_err());
    // A gzip bomb is refused rather than inflated without bound.
    let zeros = vec![0u8; 1024 * 1024];
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    for _ in 0..300 {
        std::io::Write::write_all(&mut gz, &zeros).unwrap();
    }
    let bomb = gz.finish().unwrap();
    assert!(bomb.len() < 2 * 1024 * 1024);
    assert!(codec::decode(&bomb).is_err());
}

#[test]
fn relink_never_leaves_the_local_root() {
    let tmp = tempfile::tempdir().unwrap();
    let outside = tmp.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("cover.png"), PNG_1X1).unwrap();
    let root = tmp.path().join("local");
    std::fs::create_dir_all(&root).unwrap();
    let db = Db::open_in_memory().unwrap();
    let mut b = Backup::default();
    b.backup_manga.push(BackupManga::new(0, "../outside"));
    import(&db, &b).unwrap();
    assert_eq!(relink_local_covers(&db, &root).unwrap(), 0);
    assert_eq!(db.library().unwrap()[0].thumbnail_url, None);
}

/// A `.tachibk` written by the Kotlin app (`SyncInterop.kt seed` + `export`): a local manga, a
/// "Mihon" manga with progress, a category, tracking, an extension store and preference blobs.
const KOTLIN_FIXTURE: &[u8] = include_bytes!("fixtures/kotlin-app.tachibk");

#[test]
fn restores_a_backup_made_by_the_kotlin_app() {
    let b = codec::decode(KOTLIN_FIXTURE).unwrap();
    assert_eq!(b.backup_manga.len(), 2);
    let db = Db::open_in_memory().unwrap();
    let s = import(&db, &b).unwrap();
    assert_eq!(
        (s.manga_added, s.chapters_added, s.categories_added),
        (2, 4, 1)
    );
    assert_eq!(s.repos_added.len(), 1);
    assert_eq!(s.repos_added[0].index_url(), "https://example.com/index.pb");

    let lib = db.library().unwrap();
    let mihon = lib.iter().find(|m| m.url == "/m/1").unwrap();
    assert_eq!(mihon.source, 99);
    assert_eq!(mihon.notes, "reread after vol 3");
    assert_eq!(mihon.memo, "{\"pinned\":true}");
    let chs = db.chapters(mihon.id).unwrap();
    let c1 = chs.iter().find(|c| c.url == "/m/1/c1").unwrap();
    let c2 = chs.iter().find(|c| c.url == "/m/1/c2").unwrap();
    assert!(c1.read && c1.last_page_read == 12);
    assert!(c2.bookmark && c2.last_page_read == 4);
    assert_eq!(db.tracks(mihon.id).unwrap()[0].remote_id, 4242);
    let local = lib.iter().find(|m| m.url == "Sample Manga").unwrap();
    assert_eq!(local.source, 0);
    assert_eq!(db.chapters(local.id).unwrap().len(), 2);

    // The preference sections come back out byte for byte.
    let out = export(&db, &none()).unwrap();
    assert_eq!(out.backup_preferences, b.backup_preferences);
    assert_eq!(out.backup_source_preferences, b.backup_source_preferences);
    assert_eq!(out.backup_preferences, vec![vec![10u8, 3, 97, 98, 99]]);
}

#[test]
fn one_bad_manga_is_reported_and_the_rest_is_restored() {
    let db = Db::open_in_memory().unwrap();
    db.write(|c| {
        c.execute_batch(
            "CREATE TRIGGER no_bad BEFORE INSERT ON mangas WHEN NEW.title = 'Bad' \
             BEGIN SELECT RAISE(ABORT, 'refused'); END",
        )?;
        Ok(())
    })
    .unwrap();
    let mut b = Backup::default();
    for (url, title) in [("/a", "Good A"), ("/bad", "Bad"), ("/b", "Good B")] {
        let mut m = BackupManga::new(7, url);
        m.title = title.into();
        m.chapters = vec![BackupChapter::new(format!("{url}/1"), "1")];
        b.backup_manga.push(m);
    }
    let mut seen = Vec::new();
    let s = import_with(&db, &b, &mut |done, total, title| {
        seen.push((done, total, title.to_owned()));
    })
    .unwrap();
    assert_eq!((s.manga_added, s.chapters_added), (2, 2));
    assert_eq!(s.errors.len(), 1);
    assert_eq!(s.errors[0].title, "Bad");
    assert!(
        s.errors[0].message.contains("refused"),
        "{}",
        s.errors[0].message
    );
    let titles: Vec<String> = db.library().unwrap().into_iter().map(|m| m.title).collect();
    assert_eq!(titles.len(), 2);
    assert!(!titles.iter().any(|t| t == "Bad"));
    assert_eq!(
        seen,
        vec![
            (0, 3, "Good A".to_owned()),
            (1, 3, "Bad".to_owned()),
            (2, 3, "Good B".to_owned()),
            (3, 3, String::new()),
        ]
    );
}

/// A raw `BackupPreference` with only a key (field 1), enough for parking.
fn pref_with_key(key: &str) -> Vec<u8> {
    let mut v = vec![10, u8::try_from(key.len()).unwrap()];
    v.extend_from_slice(key.as_bytes());
    v
}

#[test]
fn private_preferences_are_not_carried_into_a_later_backup() {
    let db = Db::open_in_memory().unwrap();
    let mut b = mihon_backup();
    b.backup_preferences = vec![
        pref_with_key("anilist_score_type"),
        pref_with_key("__PRIVATE_track_token_2"),
        pref_with_key("__PRIVATE_pref_mangasync_password_2"),
    ];
    import(&db, &b).unwrap();
    let round = export(&db, &none()).unwrap();
    assert_eq!(
        round.backup_preferences,
        vec![pref_with_key("anilist_score_type")]
    );
}
