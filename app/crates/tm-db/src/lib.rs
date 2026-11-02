//! The TachiMana library database: the same SQLite file and schema as the Kotlin app.
//!
//! [`Db::open`] creates the schema on a new file, migrates an older one, and opens a newer one
//! read-only so a newer app's data is never damaged.

mod models;
pub mod queries;
mod repo;
pub mod schema;

use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};

pub use models::*;
pub use repo::*;
pub use schema::SCHEMA_VERSION;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("this library was written by a newer TachiMana (schema {found}, this app knows {known}); opened read-only")]
    NewerSchema { found: i64, known: i64 },
    #[error("database is read-only")]
    ReadOnly,
    #[error("migration {version} failed: {source}")]
    Migration {
        version: i64,
        #[source]
        source: rusqlite::Error,
    },
    #[error("unknown query {0}")]
    UnknownQuery(String),
    #[error("database lock poisoned")]
    Poisoned,
    #[error("not found: {0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, DbError>;

/// A handle to the library database. One connection guarded by a mutex: SQLite serialises
/// writes anyway, and one connection keeps `last_insert_rowid()` meaningful.
#[derive(Debug)]
pub struct Db {
    conn: Mutex<Connection>,
    read_only: bool,
}

impl Db {
    /// Open (create or migrate) the database at `path`.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        configure(&conn)?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > SCHEMA_VERSION {
            drop(conn);
            let ro = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
            ro.busy_timeout(Duration::from_secs(5))?;
            tracing::warn!(version, "database is newer than this app; read-only");
            return Ok(Self {
                conn: Mutex::new(ro),
                read_only: true,
            });
        }
        prepare_schema(&conn, version)?;
        Ok(Self {
            conn: Mutex::new(conn),
            read_only: false,
        })
    }

    /// A fresh in-memory database (tests).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        prepare_schema(&conn, 0)?;
        Ok(Self {
            conn: Mutex::new(conn),
            read_only: false,
        })
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// Lock the connection.
    pub fn conn(&self) -> Result<MutexGuard<'_, Connection>> {
        self.conn.lock().map_err(|_| DbError::Poisoned)
    }

    /// Run `f` inside a transaction (refused on a read-only database).
    pub fn write<T>(&self, f: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<T>) -> Result<T> {
        if self.read_only {
            return Err(DbError::ReadOnly);
        }
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let out = f(&tx)?;
        tx.commit()?;
        Ok(out)
    }

    /// Run a read.
    pub fn read<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let conn = self.conn()?;
        f(&conn)
    }
}

fn configure(conn: &Connection) -> Result<()> {
    conn.busy_timeout(Duration::from_secs(5))?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    // WAL lets the UI read while a refresh writes. The Kotlin app copes with WAL files too.
    let _mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
    Ok(())
}

fn prepare_schema(conn: &Connection, version: i64) -> Result<()> {
    let has_tables: i64 = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
        [],
        |r| r.get(0),
    )?;
    if version == 0 && has_tables == 0 {
        let tx = conn.unchecked_transaction()?;
        for stmt in schema::create_statements() {
            tx.execute_batch(&stmt)?;
        }
        tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        tx.commit()?;
        return Ok(());
    }
    if version >= SCHEMA_VERSION {
        return Ok(());
    }
    // SQLDelight: migration N.sqm upgrades N to N+1. A version-0 database with tables is a
    // pre-versioning file; treat it as version 1 like the driver does.
    let from = version.max(1);
    let tx = conn.unchecked_transaction()?;
    for v in from..SCHEMA_VERSION {
        let idx = usize::try_from(v - 1).map_err(|_| DbError::UnknownQuery(format!("{v}.sqm")))?;
        let Some((_, text)) = schema::MIGRATIONS.get(idx) else {
            return Err(DbError::UnknownQuery(format!("{v}.sqm")));
        };
        for stmt in schema::migration_statements(text) {
            tx.execute_batch(&stmt)
                .map_err(|source| DbError::Migration { version: v, source })?;
        }
    }
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    tx.commit()?;
    Ok(())
}

/// Look up a named Kotlin query, as an error rather than a panic if it is missing.
pub(crate) fn q(table: &str, label: &str) -> Result<&'static str> {
    queries::sql(table, label).ok_or_else(|| DbError::UnknownQuery(format!("{table}.{label}")))
}
