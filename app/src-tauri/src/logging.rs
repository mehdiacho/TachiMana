//! Log file and crash reports in `<data>/logs`, so a crash on someone's PC leaves evidence.
//!
//! `tachimana.log` gets everything at info and above (it is moved to `tachimana.old.log` when it
//! passes [`MAX_LOG`] at start-up). A panic writes `crash-<unix seconds>.txt` with the message,
//! the location and a backtrace, then runs the default hook.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tracing_subscriber::fmt::writer::MakeWriterExt;

/// Size at which the log is rotated at start-up.
const MAX_LOG: u64 = 5 * 1024 * 1024;
/// Crash reports kept; older ones are deleted.
const KEEP_CRASHES: usize = 10;

/// Start logging to stderr and, when the folder can be written, to the log file.
pub fn init(logs: &Path) {
    let file = std::fs::create_dir_all(logs)
        .ok()
        .and_then(|()| open_log(logs));
    let builder = tracing_subscriber::fmt()
        .with_target(false)
        .with_ansi(false)
        .with_max_level(tracing::Level::INFO);
    match file {
        Some(f) => builder
            .with_writer(Mutex::new(f).and(std::io::stderr))
            .init(),
        None => builder.with_writer(std::io::stderr).init(),
    }
    install_panic_hook(logs.to_path_buf());
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "TachiMana started");
}

fn open_log(logs: &Path) -> Option<File> {
    let path = logs.join("tachimana.log");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_LOG) {
        let _ = std::fs::rename(&path, logs.join("tachimana.old.log"));
    }
    OpenOptions::new().create(true).append(true).open(path).ok()
}

fn install_panic_hook(logs: PathBuf) {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let thread = std::thread::current();
        let report = format!(
            "TachiMana {} crashed\nthread: {}\n{info}\n\nbacktrace:\n{}\n",
            env!("CARGO_PKG_VERSION"),
            thread.name().unwrap_or("unnamed"),
            std::backtrace::Backtrace::force_capture(),
        );
        tracing::error!("panic: {info}");
        if let Ok(mut f) = File::create(logs.join(format!("crash-{secs}.txt"))) {
            let _ = f.write_all(report.as_bytes());
        }
        prune(&logs);
        default(info);
    }));
}

/// Keep the newest [`KEEP_CRASHES`] crash reports.
fn prune(logs: &Path) {
    let Ok(dir) = std::fs::read_dir(logs) else {
        return;
    };
    let mut crashes: Vec<PathBuf> = dir
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("crash-") && n.ends_with(".txt"))
        })
        .collect();
    crashes.sort();
    let extra = crashes.len().saturating_sub(KEEP_CRASHES);
    for p in crashes.iter().take(extra) {
        let _ = std::fs::remove_file(p);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn a_big_log_is_rotated_and_old_crashes_pruned() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("tachimana.log");
        std::fs::write(&log, vec![b'x'; usize::try_from(MAX_LOG + 1).unwrap()]).unwrap();
        let f = open_log(dir.path()).unwrap();
        assert_eq!(f.metadata().unwrap().len(), 0);
        assert!(dir.path().join("tachimana.old.log").is_file());

        for i in 0..(KEEP_CRASHES + 3) {
            std::fs::write(dir.path().join(format!("crash-{:010}.txt", i)), "x").unwrap();
        }
        prune(dir.path());
        let left = std::fs::read_dir(dir.path())
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with("crash-")
            })
            .count();
        assert_eq!(left, KEEP_CRASHES);
        assert!(!dir.path().join("crash-0000000000.txt").exists());
    }
}
