//! Ports of the Kotlin self-tests that exercise the database: MigrationSelfTest (all checks),
//! LibraryUpdateSelfTest's `syncChapters` semantics, and CategorySelfTest's repo checks.

use std::collections::BTreeSet;

use rusqlite::Connection;
use tm_db::{Db, SourceChapter, SourceManga, SCHEMA_VERSION};

fn ch(url: &str, name: &str) -> SourceChapter {
    SourceChapter {
        url: url.into(),
        name: name.into(),
        date_upload: 1,
        chapter_number: 1.0,
        scanlator: None,
    }
}

fn manga(url: &str, title: &str) -> SourceManga {
    SourceManga {
        url: url.into(),
        title: title.into(),
        ..Default::default()
    }
}

#[test]
fn fresh_database_has_kotlin_schema_version() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("t.db");
    drop(Db::open(&p).unwrap());
    let c = Connection::open(&p).unwrap();
    let v: i64 = c
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, 30);
    assert_eq!(SCHEMA_VERSION, 30);
}

#[test]
fn migration_29_to_30_preserves_rows() {
    // Port of MigrationSelfTest: build current schema, roll back what 29.sqm adds, reopen.
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("migrate.db");
    {
        let db = Db::open(&p).unwrap();
        db.add_to_library(0, &manga("/keepme", "Survivor"), &[ch("c1", "One")])
            .unwrap();
    }
    {
        let c = Connection::open(&p).unwrap();
        for col in [
            "notes",
            "version",
            "last_modified_at",
            "favorite_modified_at",
            "memo",
        ] {
            c.execute_batch(&format!("ALTER TABLE mangas DROP COLUMN {col}"))
                .unwrap();
        }
        for col in ["last_modified_at", "version", "memo"] {
            c.execute_batch(&format!("ALTER TABLE chapters DROP COLUMN {col}"))
                .unwrap();
        }
        c.execute_batch("DROP TABLE backup_passthrough; PRAGMA user_version = 29;")
            .unwrap();
    }
    let db = Db::open(&p).unwrap();
    let lib = db.library().unwrap();
    assert_eq!(lib.len(), 1);
    let m = &lib[0];
    assert_eq!(m.title, "Survivor");
    assert_eq!(m.notes, "");
    assert_eq!(m.memo, "{}");
    assert_eq!(m.version, 0);
    assert_eq!(m.last_modified_at, 0);
    db.write(|c| {
        c.execute(
            "INSERT INTO backup_passthrough(kind, payload) VALUES ('preferences', x'010203')",
            [],
        )?;
        Ok(())
    })
    .unwrap();
}

#[test]
fn newer_database_opens_read_only() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("new.db");
    drop(Db::open(&p).unwrap());
    Connection::open(&p)
        .unwrap()
        .execute_batch("PRAGMA user_version = 31")
        .unwrap();
    let db = Db::open(&p).unwrap();
    assert!(db.is_read_only());
    assert!(db.library().is_ok());
    assert!(db.create_category("x").is_err());
}

#[test]
fn add_to_library_upserts_and_keeps_read_state() {
    let db = Db::open_in_memory().unwrap();
    let id = db
        .add_to_library(
            7,
            &manga("/m", "Title"),
            &[ch("c2", "Two"), ch("c1", "One")],
        )
        .unwrap();
    let chapters = db.chapters(id).unwrap();
    assert_eq!(chapters.len(), 2);
    let one = chapters.iter().find(|c| c.url == "c1").unwrap();
    db.mark_progress(one.id, 5, 3, true).unwrap();
    // Unfavourite then re-add: same row, read state intact, no duplicate chapters.
    db.remove_from_library(7, "/m").unwrap();
    assert!(db.library().unwrap().is_empty());
    let again = db
        .add_to_library(
            7,
            &manga("/m", "Title 2"),
            &[ch("c2", "Two"), ch("c1", "One")],
        )
        .unwrap();
    assert_eq!(again, id);
    let chapters = db.chapters(id).unwrap();
    assert_eq!(chapters.len(), 2);
    let one = chapters.iter().find(|c| c.url == "c1").unwrap();
    assert!(one.read);
    assert_eq!(one.last_page_read, 5);
    assert_eq!(db.library().unwrap()[0].title, "Title 2");
}

#[test]
fn sync_chapters_adds_new_and_never_touches_read_state() {
    let db = Db::open_in_memory().unwrap();
    let id = db
        .add_to_library(7, &manga("/m", "T"), &[ch("c2", "Two"), ch("c1", "One")])
        .unwrap();
    let c1 = db
        .chapters(id)
        .unwrap()
        .into_iter()
        .find(|c| c.url == "c1")
        .unwrap();
    db.mark_progress(c1.id, 9, 0, true).unwrap();

    let fresh = [ch("c3", "Three"), ch("c2", "Two (fixed)"), ch("c1", "One")];
    assert_eq!(db.sync_chapters(id, &fresh).unwrap(), 1);
    let list = db.chapters(id).unwrap();
    assert_eq!(list.len(), 3);
    let by = |u: &str| list.iter().find(|c| c.url == u).unwrap().clone();
    assert_eq!(by("c3").source_order, 0);
    assert_eq!(by("c2").source_order, 1);
    assert_eq!(by("c2").name, "Two (fixed)");
    assert_eq!(by("c1").source_order, 2);
    assert!(by("c1").read);
    assert_eq!(by("c1").last_page_read, 9);
    assert_eq!(db.sync_chapters(id, &fresh).unwrap(), 0, "idempotent");
    assert_eq!(db.unread_counts().unwrap().get(&id), Some(&2));
}

#[test]
fn categories_crud_order_and_assignment() {
    let db = Db::open_in_memory().unwrap();
    let a = db.create_category("Action").unwrap().unwrap();
    let b = db.create_category("  Romance ").unwrap().unwrap();
    assert_eq!(
        db.create_category("action").unwrap(),
        Some(a),
        "case-insensitive names"
    );
    assert_eq!(db.create_category("   ").unwrap(), None);
    let names = |db: &Db| {
        db.categories()
            .unwrap()
            .into_iter()
            .map(|c| c.name)
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&db), ["Action", "Romance"]);
    db.move_category(b, true).unwrap();
    assert_eq!(names(&db), ["Romance", "Action"]);
    db.move_category(b, true).unwrap();
    assert_eq!(
        names(&db),
        ["Romance", "Action"],
        "moving the first up is a no-op"
    );
    let sorts: Vec<i64> = db.categories().unwrap().iter().map(|c| c.sort).collect();
    assert_eq!(sorts, [0, 1], "sort stays dense");
    db.rename_category(a, "Shounen").unwrap();

    let m = db.add_to_library(0, &manga("x", "X"), &[]).unwrap();
    db.set_manga_categories(m, &BTreeSet::from([a, b])).unwrap();
    assert_eq!(
        db.category_assignments().unwrap()[&m],
        BTreeSet::from([a, b])
    );
    db.set_manga_categories(m, &BTreeSet::from([b])).unwrap();
    assert_eq!(db.category_assignments().unwrap()[&m], BTreeSet::from([b]));
    db.delete_category(b).unwrap();
    assert!(!db.category_assignments().unwrap().contains_key(&m));
    assert_eq!(names(&db), ["Shounen"]);
}

#[test]
fn history_drives_recents() {
    let db = Db::open_in_memory().unwrap();
    let m = db
        .add_to_library(0, &manga("x", "X"), &[ch("a", "A"), ch("b", "B")])
        .unwrap();
    let chs = db.chapters(m).unwrap();
    db.record_history_at(chs[0].id, 100).unwrap();
    db.record_history_at(chs[1].id, 200).unwrap();
    db.record_history_at(chs[0].id, 300).unwrap();
    let r = db.recents().unwrap();
    assert_eq!(r.len(), 2);
    assert_eq!(r[0].chapter.id, chs[0].id);
    assert_eq!(r[0].last_read, 300);
    assert_eq!(r[0].manga.title, "X");
}

#[test]
fn extension_repos_round_trip() {
    let db = Db::open_in_memory().unwrap();
    let r = tm_db::ExtensionRepo {
        base_url: "https://x/index.pb".into(),
        name: "X".into(),
        short_name: None,
        website: "https://x".into(),
        signing_key_fingerprint: "ab".into(),
    };
    db.upsert_extension_repo(&r).unwrap();
    db.upsert_extension_repo(&tm_db::ExtensionRepo {
        name: "Y".into(),
        ..r.clone()
    })
    .unwrap();
    let all = db.extension_repos().unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].name, "Y");
    db.delete_extension_repo(&r.base_url).unwrap();
    assert!(db.extension_repos().unwrap().is_empty());
}
