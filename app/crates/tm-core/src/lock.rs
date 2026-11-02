//! Single-instance lock: only one app (Rust or Kotlin-era tooling that honours it) writes the DB.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};

use crate::{io_err, CoreError, Result};

/// Holds an exclusive OS lock on `<data>/app.lock` until dropped.
#[derive(Debug)]
pub struct AppLock {
    _file: File,
    path: PathBuf,
}

impl AppLock {
    /// Take the lock, or fail with [`CoreError::Locked`] if another process holds it.
    pub fn acquire(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)
            .map_err(io_err(path))?;
        match file.try_lock() {
            Ok(()) => Ok(Self {
                _file: file,
                path: path.to_path_buf(),
            }),
            Err(TryLockError::WouldBlock) => Err(CoreError::Locked(path.to_path_buf())),
            Err(TryLockError::Error(e)) => Err(io_err(path)(e)),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_lock_fails_until_first_drops() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("app.lock");
        let first = AppLock::acquire(&p).unwrap();
        assert!(matches!(AppLock::acquire(&p), Err(CoreError::Locked(_))));
        drop(first);
        assert!(AppLock::acquire(&p).is_ok());
    }
}
