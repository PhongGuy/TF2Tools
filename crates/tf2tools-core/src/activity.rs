//! The activity log over v5's `logs` table: append entries, page through them newest
//! first with optional filters, clear the table, and trim it to a cap.

use rusqlite::{OptionalExtension, params};

use crate::db::Database;
use crate::error::LogError;
use crate::timestamp;

/// How many rows the log keeps when trimmed to its cap.
pub const LOG_KEEP: u32 = 10_000;

/// The severity of a log entry, stored as v5's `info`, `warning` or `error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LogLevel {
    /// Something happened as expected.
    Info,
    /// Something worked but needs attention.
    Warning,
    /// Something failed.
    Error,
}

impl LogLevel {
    fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }

    // v5 wrote only the three known levels; anything else reads as the mildest.
    fn from_text(text: &str) -> Self {
        match text {
            "warning" => Self::Warning,
            "error" => Self::Error,
            _ => Self::Info,
        }
    }
}

/// An entry to append; the log stamps its id and timestamp.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct NewLogEntry {
    level: LogLevel,
    category: String,
    action: String,
    message: String,
    details: Option<String>,
}

impl NewLogEntry {
    /// An entry without details.
    pub fn new(
        level: LogLevel,
        category: impl Into<String>,
        action: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            level,
            category: category.into(),
            action: action.into(),
            message: message.into(),
            details: None,
        }
    }

    /// The same entry with extra detail text, such as an error chain.
    #[must_use]
    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }
}

/// A stored log row.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LogEntry {
    id: i64,
    timestamp: String,
    level: LogLevel,
    category: String,
    action: String,
    message: String,
    details: Option<String>,
}

impl LogEntry {
    /// The row id, increasing with insertion order.
    pub fn id(&self) -> i64 {
        self.id
    }

    /// When the entry was written, in the database's RFC 3339 format.
    pub fn timestamp(&self) -> &str {
        &self.timestamp
    }

    /// The entry's severity.
    pub fn level(&self) -> LogLevel {
        self.level
    }

    /// The area the entry belongs to, such as `hud` or `sound_store`.
    pub fn category(&self) -> &str {
        &self.category
    }

    /// The operation that produced the entry.
    pub fn action(&self) -> &str {
        &self.action
    }

    /// The human-readable summary.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Extra detail text, if any.
    pub fn details(&self) -> Option<&str> {
        self.details.as_deref()
    }
}

/// One page of the log: at most `limit` rows, newest first, optionally filtered by
/// category and level and starting below a row id.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LogQuery {
    category: Option<String>,
    level: Option<LogLevel>,
    before: Option<i64>,
    limit: u32,
}

impl LogQuery {
    /// The newest `limit` rows, unfiltered.
    pub fn new(limit: u32) -> Self {
        Self {
            category: None,
            level: None,
            before: None,
            limit,
        }
    }

    /// Only rows in `category` or its store category (`<category>_store`), as v5 filters.
    #[must_use]
    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }

    /// Only rows of `level`.
    #[must_use]
    pub fn with_level(mut self, level: LogLevel) -> Self {
        self.level = Some(level);
        self
    }

    /// Only rows older than the row `id`, for paging.
    #[must_use]
    pub fn with_before(mut self, id: i64) -> Self {
        self.before = Some(id);
        self
    }
}

/// The activity log, a handle over the shared database.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct ActivityLog {
    db: Database,
}

impl ActivityLog {
    /// A log writing to `db`.
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Records `entry` stamped with the current time.
    pub fn append(&self, entry: &NewLogEntry) -> Result<(), LogError> {
        self.db
            .write(|conn| {
                conn.execute(
                    "INSERT INTO logs (timestamp, level, category, action, message, details)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        timestamp::now(),
                        entry.level.as_str(),
                        entry.category,
                        entry.action,
                        entry.message,
                        entry.details,
                    ],
                )
            })
            .map(drop)
            .map_err(db_error)
    }

    /// The rows matching `q`, newest first. Every filter is an optional bind in one
    /// statement, and `before` walks the id primary key as a keyset.
    pub fn query(&self, q: &LogQuery) -> Result<Vec<LogEntry>, LogError> {
        self.db
            .read(|conn| {
                let mut stmt = conn.prepare_cached(
                    "SELECT id, timestamp, level, category, action, message, details FROM logs
                     WHERE (?1 IS NULL OR category IN (?1, ?1 || '_store'))
                       AND (?2 IS NULL OR level = ?2)
                       AND (?3 IS NULL OR id < ?3)
                     ORDER BY id DESC LIMIT ?4",
                )?;
                let rows = stmt.query_map(
                    params![q.category, q.level.map(LogLevel::as_str), q.before, q.limit],
                    |r| {
                        Ok(LogEntry {
                            id: r.get(0)?,
                            timestamp: r.get(1)?,
                            level: LogLevel::from_text(&r.get::<_, String>(2)?),
                            category: r.get(3)?,
                            action: r.get(4)?,
                            message: r.get(5)?,
                            details: r.get(6)?,
                        })
                    },
                )?;
                rows.collect()
            })
            .map_err(db_error)
    }

    /// Deletes every row.
    pub fn clear(&self) -> Result<(), LogError> {
        self.db
            .write(|conn| conn.execute("DELETE FROM logs", []))
            .map(drop)
            .map_err(db_error)
    }

    /// Deletes all but the newest `keep` rows and returns how many were removed.
    pub fn trim(&self, keep: u32) -> Result<u64, LogError> {
        self.db
            .write(|conn| {
                // The row just past the newest `keep` is the highest id to drop; none means
                // the log is already within the cap.
                let cutoff: Option<i64> = conn
                    .query_row(
                        "SELECT id FROM logs ORDER BY id DESC LIMIT 1 OFFSET ?1",
                        [keep],
                        |r| r.get(0),
                    )
                    .optional()?;
                match cutoff {
                    Some(id) => conn.execute("DELETE FROM logs WHERE id <= ?1", [id]),
                    None => Ok(0),
                }
            })
            .map(|removed| u64::try_from(removed).unwrap_or(u64::MAX))
            .map_err(db_error)
    }
}

fn db_error(e: rusqlite::Error) -> LogError {
    LogError::Database {
        message: e.to_string(),
    }
}
