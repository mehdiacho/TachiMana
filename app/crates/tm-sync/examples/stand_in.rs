//! The stand-in peer (`docs/protocol/README.md` §6): a correct TachiMana Link Protocol device with
//! an empty library, for trying one app without the other.
//!
//! ```text
//! cargo run -p tm-sync --example stand_in -- serve [--port N] [--code 123456] [--no-changes]
//! cargo run -p tm-sync --example stand_in -- check HOST:PORT [--code 123456]
//! ```
//!
//! `serve` runs this crate's real server, so what it answers is what the computer app answers.
//! It prints every change it receives and every one-off copy it is asked for. `--no-changes`
//! plays an app without a change journal.
//!
//! `check` calls another device and reports what it finds, one line per check, `ok` or `FAIL`.
//! With `--code` it also links (the other device then lists "Stand-in peer" as linked: use a
//! test profile) and tries the linked routes. It exits 1 when a check fails.

use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, Mutex};

use tm_sync::client::Pulled;
use tm_sync::journal::{Entry, Journal};
use tm_sync::protocol::{split_address, LinkRequest, PROTOCOL};
use tm_sync::server::{BackupFn, Changes, Linking, SyncServer};
use tm_sync::{DeviceIdentity, PeerDevice, SyncClient};

const DEVICE_ID: &str = "stand-in-peer";
const DEVICE_NAME: &str = "Stand-in peer";
/// The checker links as another device, so it can be pointed at a stand-in that is serving.
const CHECKER_ID: &str = "stand-in-checker";

type Fallible<T> = Result<T, Box<dyn Error>>;

/// The value after `flag`, when the flag is given.
fn value_of(args: &[String], flag: &str) -> Option<String> {
    let at = args.iter().position(|a| a == flag)?;
    args.get(at.checked_add(1)?).cloned()
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Fallible<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("serve") => serve(&args).await,
        Some("check") => {
            let failed = check(&args).await?;
            if failed > 0 {
                println!("{failed} check(s) failed");
                std::process::exit(1);
            }
            println!("all checks passed");
            Ok(())
        }
        _ => {
            println!("usage: stand_in serve [--port N] [--code 123456] [--no-changes]");
            println!("       stand_in check HOST:PORT [--code 123456]");
            Ok(())
        }
    }
}

async fn serve(args: &[String]) -> Fallible<()> {
    let code = value_of(args, "--code").unwrap_or_else(|| "123456".to_owned());
    let port = match value_of(args, "--port") {
        Some(p) => Some(p.parse::<u16>()?),
        None => None,
    };
    let dir = tempfile::tempdir()?;
    let local = dir.path().join("local");
    std::fs::create_dir_all(&local)?;
    let db = Arc::new(tm_db::Db::open(&dir.path().join("lib.db"))?);
    let backup: Arc<BackupFn> = Arc::new(move || {
        let b = tm_backup::export(&db, &HashMap::new()).map_err(|e| e.to_string())?;
        tm_backup::codec::encode(&b).map_err(|e| e.to_string())
    });
    let changes = if args.iter().any(|a| a == "--no-changes") {
        None
    } else {
        Some(Changes {
            journal: Arc::new(Journal::open_in_memory(DEVICE_ID)?),
            apply: Arc::new(|e: &Entry, v: &serde_json::Value| {
                println!("change applied: {} {} = {v}", e.kind, e.target);
            }),
            changed: Arc::new(|| {}),
        })
    };
    let server = SyncServer::start_with(
        DeviceIdentity {
            id: DEVICE_ID.to_owned(),
            name: DEVICE_NAME.to_owned(),
            pairing_code: code.clone(),
        },
        local,
        backup,
        port,
        Some(Linking {
            profile_id: "stand-in-profile".to_owned(),
            profile_name: "Stand-in".to_owned(),
            links: Arc::new(tm_sync::LinkStore::open(&dir.path().join("links.json"))),
            refused: Arc::new(Mutex::new(None)),
            outbox: Arc::new(tm_sync::queue::Outbox::open(
                &dir.path().join("outbox.json"),
            )),
            changes,
            take: Some(Arc::new(|l: &tm_sync::Link, how, id: String| {
                println!(
                    "asked for a one-off copy by {}: {how:?} (request {id})",
                    l.peer_name
                );
            })),
            roles_changed: Some(Arc::new(|l: &tm_sync::Link| {
                println!("{} changed who leads: {:?}", l.peer_name, l.mode);
            })),
        }),
    )
    .await?;
    println!(
        "stand-in peer on port {}, pairing code {code}",
        server.port()
    );
    println!("profile id stand-in-profile (a device linking from another profile gets 409)");
    println!("Ctrl+C stops it; nothing is kept.");
    std::future::pending::<()>().await;
    Ok(())
}

/// Prints one check and counts a failure.
fn report(failed: &mut u32, ok: bool, what: &str) {
    println!("{} {what}", if ok { "ok  " } else { "FAIL" });
    if !ok {
        *failed = failed.saturating_add(1);
    }
}

async fn check(args: &[String]) -> Fallible<u32> {
    let address = args.get(1).ok_or("give HOST:PORT")?;
    let (host, port) = split_address(address).ok_or("HOST:PORT expected")?;
    let client = SyncClient::new()?;
    let mut failed = 0u32;

    let Some(hello) = client.hello(&host, port).await else {
        report(&mut failed, false, "the device answers /hello or /ping");
        return Ok(failed);
    };
    report(&mut failed, true, "the device answers /hello or /ping");
    // The fallback for an app without /hello names no app; a real answer does.
    let has_hello = !hello.app.name.is_empty();
    report(
        &mut failed,
        has_hello,
        "/hello answers and names its app (an app without it is treated as link/2, pull/1)",
    );
    report(
        &mut failed,
        hello.protocol == PROTOCOL,
        &format!("protocol is \"{PROTOCOL}\" (got \"{}\")", hello.protocol),
    );
    report(
        &mut failed,
        !hello.device_id.is_empty(),
        "deviceId is not empty",
    );
    let ping = client.probe(&host, port).await;
    report(
        &mut failed,
        ping.is_some_and(|p| p.id == hello.device_id),
        "/ping names the same device as /hello",
    );
    println!(
        "     {} \"{}\" on {}, app {} {}",
        hello.device_id, hello.device_name, hello.platform, hello.app.name, hello.app.version
    );
    println!("     lists: {}", hello.capabilities.join(", "));

    let Some(code) = value_of(args, "--code") else {
        println!("     (no --code given: the linked routes were not tried)");
        return Ok(failed);
    };
    let req = LinkRequest {
        device_id: CHECKER_ID.to_owned(),
        device_name: DEVICE_NAME.to_owned(),
        profile_id: hello.profile_id.clone().unwrap_or_default(),
        profile_name: hello.profile_name.clone().unwrap_or_default(),
        platform: "desktop".to_owned(),
        port: None,
    };
    let wrong = if code == "000000" { "111111" } else { "000000" };
    report(
        &mut failed,
        client.link(&host, port, wrong, &req).await.is_err(),
        "POST /link with a wrong code is refused",
    );
    let key = match client.link(&host, port, &code, &req).await {
        Ok(r) => r.key,
        Err(e) => {
            println!("     link failed: {e}");
            None
        }
    };
    report(
        &mut failed,
        key.is_some(),
        "POST /link with the code gives a key",
    );
    let Some(key) = key else {
        return Ok(failed);
    };
    let peer = PeerDevice {
        id: hello.device_id.clone(),
        name: hello.device_name.clone(),
        host,
        port,
    };

    let pulled = client.changes_after(&peer, &key, 0, 100).await;
    let lists_changes = hello.supports("changes/1");
    let served = matches!(pulled, Ok(Pulled::Page(_) | Pulled::TooOld { .. }));
    report(
        &mut failed,
        served == lists_changes,
        "GET /changes answers exactly when changes/1 is listed",
    );
    if lists_changes {
        report(
            &mut failed,
            client
                .changes_after(&peer, "not-the-key", 0, 100)
                .await
                .is_err(),
            "GET /changes with a wrong key is refused",
        );
        // An entry from this device that changes nothing the person can see: an unread mark on
        // a chapter of a manga that does not exist.
        let journal = Journal::open_in_memory(CHECKER_ID)?;
        let entry = journal.record(
            "chapter.read",
            serde_json::json!({"sourceId": "0", "mangaUrl": "/stand-in", "chapterUrl": "/stand-in/1"}),
            serde_json::json!({"read": false, "lastPageRead": 0}),
        )?;
        let first = client
            .push_changes(&peer, &key, CHECKER_ID, std::slice::from_ref(&entry))
            .await;
        report(
            &mut failed,
            matches!(first, Ok(Some(_))),
            "POST /changes takes an entry",
        );
        let again = client
            .push_changes(&peer, &key, CHECKER_ID, std::slice::from_ref(&entry))
            .await;
        report(
            &mut failed,
            matches!(again, Ok(Some(0))),
            "the same entry sent again is dropped (applied 0)",
        );
    }
    Ok(failed)
}
