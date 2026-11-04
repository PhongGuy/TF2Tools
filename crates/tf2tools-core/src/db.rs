//! The v5-compatible library database: one writer and one read-only reader connection,
//! opened in an order that never writes to a database from a newer TF2Tools.

mod migrations;

use std::fmt;
use std::io;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use rusqlite::{Connection, ErrorCode, OpenFlags};

use crate::error::OpenError;
use crate::paths::{DataDir, LibraryKind};
use migrations::SCHEMA_VERSION;

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// The v5-compatible library database. Cheap to clone; all clones share two connections.
#[derive(Clone)]
#[non_exhaustive]
pub struct Database {
    inner: Arc<Inner>,
}

struct Inner {
    writer: Mutex<Connection>,
    reader: Mutex<Connection>,
    schema_version: u32,
}

impl fmt::Debug for Database {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Database")
            .field("schema_version", &self.inner.schema_version)
            .finish_non_exhaustive()
    }
}

impl Database {
    /// Creates the data directory layout, clears `tmp/`, opens `tf2tools.db` and migrates it
    /// to the current schema. Fails with `NewerSchema` before writing anything when the file
    /// comes from a newer TF2Tools.
    pub fn open(dir: &DataDir) -> Result<Self, OpenError> {
        let path = dir.db_path();
        create_dir(dir.root())?;
        for kind in LibraryKind::ALL {
            create_dir(&dir.library_dir(kind))?;
        }
        create_dir(&dir.tmp_dir())?;

        // A garbage file opens fine; the first read is where SQLite notices.
        let mut writer = Connection::open(&path).map_err(|e| open_error(&path, e))?;
        writer
            .busy_timeout(BUSY_TIMEOUT)
            .map_err(|e| open_error(&path, e))?;
        let found: u32 = writer
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(|e| open_error(&path, e))?;
        if found > SCHEMA_VERSION {
            return Err(OpenError::NewerSchema {
                path,
                found,
                supported: SCHEMA_VERSION,
            });
        }
        clear_tmp(&dir.tmp_dir())?;

        writer
            .pragma_update(None, "foreign_keys", true)
            .map_err(|e| sqlite_error(&path, e))?;
        writer
            .pragma_update_and_check(None, "journal_mode", "WAL", |r| r.get::<_, String>(0))
            .map_err(|e| sqlite_error(&path, e))?;
        // WAL makes NORMAL durable against corruption; only the last commits can be lost on power cut.
        writer
            .pragma_update(None, "synchronous", "NORMAL")
            .map_err(|e| sqlite_error(&path, e))?;
        let schema_version =
            migrations::migrate(&mut writer, found).map_err(|(step, e)| OpenError::Migration {
                path: path.clone(),
                step,
                message: e.to_string(),
            })?;

        let reader = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| sqlite_error(&path, e))?;
        reader
            .busy_timeout(BUSY_TIMEOUT)
            .map_err(|e| sqlite_error(&path, e))?;
        reader
            .pragma_update(None, "foreign_keys", true)
            .map_err(|e| sqlite_error(&path, e))?;
        reader
            .pragma_update(None, "synchronous", "NORMAL")
            .map_err(|e| sqlite_error(&path, e))?;

        Ok(Self {
            inner: Arc::new(Inner {
                writer: Mutex::new(writer),
                reader: Mutex::new(reader),
                schema_version,
            }),
        })
    }

    /// The schema version the database is at after opening.
    pub fn schema_version(&self) -> u32 {
        self.inner.schema_version
    }

    /// Runs `f` on the writer connection; migrations, upserts and checkpoints go here.
    pub(crate) fn write<T>(
        &self,
        f: impl FnOnce(&mut Connection) -> rusqlite::Result<T>,
    ) -> rusqlite::Result<T> {
        // A panic mid-call leaves the connection usable, so a poisoned lock is recovered.
        let mut conn = self
            .inner
            .writer
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        f(&mut conn)
    }

    /// Runs `f` on the read-only connection; never waits for a write transaction.
    pub(crate) fn read<T>(
        &self,
        f: impl FnOnce(&Connection) -> rusqlite::Result<T>,
    ) -> rusqlite::Result<T> {
        let conn = self
            .inner
            .reader
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        f(&conn)
    }
}

fn create_dir(path: &Path) -> Result<(), OpenError> {
    std::fs::create_dir_all(path).map_err(|source| OpenError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Empties `tmp/` of partial downloads left by a previous run.
fn clear_tmp(tmp: &Path) -> Result<(), OpenError> {
    match std::fs::remove_dir_all(tmp) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(OpenError::Io {
                path: tmp.to_path_buf(),
                source,
            });
        }
    }
    create_dir(tmp)
}

/// Errors before migration: a file SQLite cannot read as a database is `Corrupt`.
fn open_error(path: &Path, e: rusqlite::Error) -> OpenError {
    match e.sqlite_error_code() {
        Some(ErrorCode::NotADatabase | ErrorCode::DatabaseCorrupt) => OpenError::Corrupt {
            path: path.to_path_buf(),
            message: e.to_string(),
        },
        _ => sqlite_error(path, e),
    }
}

fn sqlite_error(path: &Path, e: rusqlite::Error) -> OpenError {
    OpenError::Sqlite {
        path: path.to_path_buf(),
        message: e.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_connections_use_synchronous_normal() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&DataDir::at(tmp.path().join("data"))).unwrap();
        let mode =
            |c: &Connection| c.pragma_query_value(None, "synchronous", |r| r.get::<_, i64>(0));
        assert_eq!(db.write(|c| mode(c)).unwrap(), 1, "writer is NORMAL");
        assert_eq!(db.read(mode).unwrap(), 1, "reader is NORMAL");
    }
}
