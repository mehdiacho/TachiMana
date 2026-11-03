//! The Rust half of the Kotlin <-> Rust sync interop check. Same commands and output as
//! `desktop/.../sync/SyncInterop.kt`, so the two can be run against each other and diffed.
//!
//!   cargo run -p tm-sync --example sync_peer -- serve  <dir> <code>
//!   cargo run -p tm-sync --example sync_peer -- pull   <dir> <host> <port> <code>
//!   cargo run -p tm-sync --example sync_peer -- export <dir> <file>
//!   cargo run -p tm-sync --example sync_peer -- import <dir> <file>
//!   cargo run -p tm-sync --example sync_peer -- print  <dir>
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tm_db::Db;
use tm_sync::server::SyncServer;
use tm_sync::{DeviceIdentity, SyncClient};

fn fail(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(1)
}

fn usage() -> ! {
    fail("usage: serve|pull|export|import|print <dir> ...")
}

fn print(db: &Db, local: &Path) {
    let local = local.to_string_lossy().into_owned();
    let cats: HashMap<i64, String> = db
        .categories()
        .unwrap()
        .into_iter()
        .map(|c| (c.id, c.name))
        .collect();
    let assign = db.category_assignments().unwrap();
    let mut lib = db.library().unwrap();
    lib.sort_by(|a, b| a.url.cmp(&b.url));
    for m in lib {
        let mut names: Vec<String> = assign
            .get(&m.id)
            .map(|s| s.iter().filter_map(|i| cats.get(i).cloned()).collect())
            .unwrap_or_default();
        names.sort();
        let tracks: Vec<String> = db
            .tracks(m.id)
            .unwrap()
            .iter()
            .map(|t| format!("{}:{}", t.sync_id, t.remote_id))
            .collect();
        let cover = match &m.thumbnail_url {
            Some(t) if t.starts_with(&local) => "local".to_owned(),
            Some(t) => t.clone(),
            None => "null".to_owned(),
        };
        println!(
            "MANGA source={} url={} title={} notes={} cats=[{}] tracks=[{}] cover={}",
            m.source,
            m.url,
            m.title,
            m.notes,
            names.join(", "),
            tracks.join(", "),
            cover
        );
        let mut chs = db.chapters(m.id).unwrap();
        chs.sort_by(|a, b| a.url.cmp(&b.url));
        for c in chs {
            println!(
                "  CHAPTER url={} read={} page={} bookmark={}",
                c.url, c.read, c.last_page_read, c.bookmark
            );
        }
    }
    let repos: Vec<String> = db
        .extension_repos()
        .unwrap()
        .into_iter()
        .map(|r| r.base_url)
        .collect();
    println!("REPOS [{}]", repos.join(", "));
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().cloned().unwrap_or_else(|| usage());
    let dir = PathBuf::from(args.get(1).cloned().unwrap_or_else(|| usage()));
    let local = dir.join("local");
    std::fs::create_dir_all(&local).unwrap();
    let db = Arc::new(Db::open(&dir.join("lib.db")).unwrap());
    tm_local::scan(&db, &local).unwrap();
    let name = dir.file_name().unwrap().to_string_lossy().into_owned();

    match cmd.as_str() {
        "serve" => {
            let code = args.get(2).cloned().unwrap_or_else(|| usage());
            let id = DeviceIdentity {
                id: format!("rust-{name}"),
                name: format!("Rust {name}"),
                pairing_code: code,
            };
            let backup_db = Arc::clone(&db);
            let server = SyncServer::start(
                id,
                local.clone(),
                Arc::new(move || {
                    let b = tm_backup::export(&backup_db, &HashMap::new())
                        .map_err(|e| e.to_string())?;
                    tm_backup::codec::encode(&b).map_err(|e| e.to_string())
                }),
                None,
            )
            .await
            .unwrap();
            println!("READY port={}", server.port());
            tokio::task::spawn_blocking(|| {
                let mut sink = [0u8; 64];
                while std::io::stdin()
                    .read(&mut sink)
                    .map(|n| n > 0)
                    .unwrap_or(false)
                {}
            })
            .await
            .unwrap();
        }
        "pull" => {
            let host = args.get(2).cloned().unwrap_or_else(|| usage());
            let port: u16 = args
                .get(3)
                .and_then(|p| p.parse().ok())
                .unwrap_or_else(|| usage());
            let code = args.get(4).cloned().unwrap_or_else(|| usage());
            let client = SyncClient::new().unwrap();
            let peer = client
                .probe(&host, port)
                .await
                .unwrap_or_else(|| fail(&format!("no peer at {host}:{port}")));
            let r = tm_sync::pull(&client, &peer, &code, Arc::clone(&db), &local, &|_| {})
                .await
                .unwrap_or_else(|e| fail(&format!("pull failed: {e}")));
            println!(
                "PULLED from={} manga+={} chapters+={} files={} bytes={}",
                r.peer_name,
                r.summary.manga_added,
                r.summary.chapters_added,
                r.files_pulled,
                r.bytes_pulled
            );
            print(&db, &local);
        }
        "export" => {
            let file = args.get(2).cloned().unwrap_or_else(|| usage());
            let b = tm_backup::export(&db, &HashMap::new()).unwrap();
            std::fs::write(file, tm_backup::codec::encode(&b).unwrap()).unwrap();
            println!("EXPORTED");
        }
        "import" => {
            let file = args.get(2).cloned().unwrap_or_else(|| usage());
            let b = tm_backup::codec::decode(&std::fs::read(file).unwrap()).unwrap();
            let s = tm_backup::import(&db, &b).unwrap();
            tm_backup::relink_local_covers(&db, &local).unwrap();
            tm_local::scan(&db, &local).unwrap();
            let pending: Vec<&str> = s.pending_repos.iter().map(|r| r.index_url()).collect();
            println!(
                "IMPORTED manga+={} chapters+={} categories+={} pending=[{}]",
                s.manga_added,
                s.chapters_added,
                s.categories_added,
                pending.join(", ")
            );
            let show = |v: &Vec<Vec<u8>>| {
                v.iter()
                    .map(|x| {
                        let parts: Vec<String> = x.iter().map(|b| (*b as i8).to_string()).collect();
                        format!("[{}]", parts.join(", "))
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            println!(
                "PREFS [{}] [{}]",
                show(&b.backup_preferences),
                show(&b.backup_source_preferences)
            );
            print(&db, &local);
        }
        "print" => print(&db, &local),
        _ => usage(),
    }
}
