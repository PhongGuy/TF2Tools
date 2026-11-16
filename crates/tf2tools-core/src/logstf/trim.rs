//! Trimming: dropping the full body of logs nobody opened lately, and giving the space back.

use rusqlite::types::ValueRef;
use rusqlite::{Connection, OptionalExtension, params};

use super::cache::{BodyRead, STATE_TRIMMED, db_error, deflate};
use super::{KeepFullLogs, LogsCache, PROVISIONAL_SECS};
use crate::error::LogsCacheError;

/// The `settings` key holding the chosen keep choice's key.
const KEEP_KEY: &str = "logstf_keep_full_logs";
/// The `settings` key counting logs trimmed since the file was last rewritten.
const TRIMS_KEY: &str = "logstf_trimmed_since_vacuum";
/// Trimmed logs that leave about a MiB of near-empty pages, when the file is rewritten.
const VACUUM_AFTER_TRIMS: i64 = 256;
/// A file without auto_vacuum is rewritten once its free pages pass this many bytes.
const VACUUM_FREE_BYTES: i64 = 1024 * 1024;
/// Plain bodies read per recompression batch.
const RECOMPRESS_BATCH: i64 = 50;
/// Seconds in a day, turning a keep choice's days into the cutoff both `trim` and `space` use.
const DAY_SECS: i64 = 86_400;

/// Matches a log a pass would trim, with ?1 the provisional gap and ?2 the cutoff: stored,
/// both backfills counted, settled, and last opened (or fetched) at or before the cutoff.
macro_rules! trimmable {
    () => {
        "state = 0 AND body IS NOT NULL AND weapons_counted = 1 AND dt_counted = 1
         AND fetched_at - date >= ?1 AND COALESCE(opened_at, fetched_at, 0) <= ?2"
    };
}

/// What one trim pass did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct TrimReport {
    trimmed: usize,
    recompressed: usize,
    bytes_before: u64,
    bytes_after: u64,
}

impl TrimReport {
    /// Logs whose full body the pass dropped.
    pub fn trimmed(&self) -> usize {
        self.trimmed
    }

    /// Plain bodies of an earlier build the pass compressed.
    pub fn recompressed(&self) -> usize {
        self.recompressed
    }

    /// The database file's size before the pass, in bytes.
    pub fn bytes_before(&self) -> u64 {
        self.bytes_before
    }

    /// The database file's size after the pass and its compaction, in bytes.
    pub fn bytes_after(&self) -> u64 {
        self.bytes_after
    }
}

/// What the logs.tf cache takes on disk, and what each keep choice would free.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LogsSpace {
    cache_bytes: u64,
    full_bytes: u64,
    full_logs: u64,
    frees: [u64; 7],
}

impl LogsSpace {
    /// Bytes of every logs.tf table and index.
    pub fn cache_bytes(&self) -> u64 {
        self.cache_bytes
    }

    /// Bytes of the table holding the full logs.
    pub fn full_bytes(&self) -> u64 {
        self.full_bytes
    }

    /// Logs whose full body is stored.
    pub fn full_logs(&self) -> u64 {
        self.full_logs
    }

    /// Bytes a trim with `keep` would free now, as a share of `full_bytes`.
    pub fn frees(&self, keep: KeepFullLogs) -> u64 {
        KeepFullLogs::ALL
            .iter()
            .position(|k| *k == keep)
            .map_or(0, |i| self.frees[i])
    }
}

/// What opening a log found in the cache.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum OpenedBody {
    /// The stored full log.
    Body(String),
    /// The full log was trimmed; its stats stay and it can be downloaded again.
    Trimmed,
    /// No such log, or no full log stored and it was not trimmed.
    Missing,
    /// A stored body that does not inflate or decode to text.
    Damaged,
}

impl LogsCache {
    /// Stamps `log_id` opened at `now`, then reads its body. The stamp lands first, so a trim
    /// pass running meanwhile spares the log.
    pub fn open_body(&self, log_id: u64, now: i64) -> Result<OpenedBody, LogsCacheError> {
        let Ok(log_id) = i64::try_from(log_id) else {
            return Ok(OpenedBody::Missing);
        };
        self.mark_opened(log_id, now)?;
        Ok(match self.read_body(log_id)? {
            BodyRead::Text(body) => OpenedBody::Body(body),
            BodyRead::Damaged => OpenedBody::Damaged,
            BodyRead::Missing => {
                let state = self
                    .db
                    .read(|conn| {
                        conn.query_row(
                            "SELECT state FROM logstf_logs WHERE id = ?1",
                            [log_id],
                            |row| row.get::<_, i64>(0),
                        )
                        .optional()
                    })
                    .map_err(db_error)?;
                if state == Some(STATE_TRIMMED) {
                    OpenedBody::Trimmed
                } else {
                    OpenedBody::Missing
                }
            }
        })
    }

    /// Records that the player opened `log_id` at `now`, which keeps a trim pass off it.
    pub(crate) fn mark_opened(&self, log_id: i64, now: i64) -> Result<(), LogsCacheError> {
        self.db
            .write(|conn| {
                conn.execute(
                    "UPDATE logstf_logs SET opened_at = ?2 WHERE id = ?1",
                    params![log_id, now],
                )
            })
            .map(drop)
            .map_err(db_error)
    }

    /// The chosen keep choice; a missing or unknown setting reads as the default, a week.
    pub fn keep_full_logs(&self) -> Result<KeepFullLogs, LogsCacheError> {
        let key = self
            .db
            .read(|conn| {
                conn.query_row(
                    "SELECT value FROM settings WHERE key = ?1",
                    [KEEP_KEY],
                    |row| row.get::<_, String>(0),
                )
                .optional()
            })
            .map_err(db_error)?;
        Ok(key
            .as_deref()
            .and_then(KeepFullLogs::from_key)
            .unwrap_or_default())
    }

    /// Stores `keep` as the chosen keep choice.
    pub fn set_keep_full_logs(&self, keep: KeepFullLogs) -> Result<(), LogsCacheError> {
        self.db
            .write(|conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
                    params![KEEP_KEY, keep.key()],
                )
            })
            .map(drop)
            .map_err(db_error)
    }

    /// Measures the cache at `now`. Each choice's figure is its trimmable bodies' share of the
    /// full-log table, so pages a body shares with its row are counted fairly.
    pub fn space(&self, now: i64) -> Result<LogsSpace, LogsCacheError> {
        self.db
            .read(|conn| {
                // One read transaction, so a pass committing between statements cannot make
                // the figures disagree.
                let tx = conn.unchecked_transaction()?;
                let cache_bytes: i64 = tx.query_row(
                    r"SELECT COALESCE(SUM(d.pgsize), 0) FROM dbstat d
                      JOIN sqlite_schema s ON s.name = d.name
                      WHERE s.tbl_name LIKE 'logstf\_%' ESCAPE '\'",
                    [],
                    |r| r.get(0),
                )?;
                let full_bytes: i64 = tx.query_row(
                    "SELECT COALESCE(SUM(pgsize), 0) FROM dbstat WHERE name = 'logstf_logs'",
                    [],
                    |r| r.get(0),
                )?;
                let (full_logs, total): (i64, i64) = tx.query_row(
                    "SELECT COUNT(*), COALESCE(SUM(octet_length(body)), 0) FROM logstf_logs
                     WHERE body IS NOT NULL",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?;
                let mut frees = [0_u64; 7];
                for (slot, keep) in frees.iter_mut().zip(KeepFullLogs::ALL) {
                    let Some(days) = keep.days() else { continue };
                    let picked: i64 = tx.query_row(
                        concat!(
                            "SELECT COALESCE(SUM(octet_length(body)), 0) FROM logstf_logs WHERE ",
                            trimmable!()
                        ),
                        params![PROVISIONAL_SECS, now - days * DAY_SECS],
                        |r| r.get(0),
                    )?;
                    if total > 0 {
                        let share = i128::from(full_bytes) * i128::from(picked) / i128::from(total);
                        *slot = u64::try_from(share).unwrap_or(0);
                    }
                }
                let unsigned = |n: i64| u64::try_from(n).unwrap_or(0);
                Ok(LogsSpace {
                    cache_bytes: unsigned(cache_bytes),
                    full_bytes: unsigned(full_bytes),
                    full_logs: unsigned(full_logs),
                    frees,
                })
            })
            .map_err(db_error)
    }

    /// Drops the body of every counted, settled log not opened (or fetched) within `keep`'s
    /// days of `now`, compresses plain bodies left by earlier builds, and returns free pages to
    /// the file system. A failed compaction is logged and retried on the next pass.
    pub fn trim(&self, keep: KeepFullLogs, now: i64) -> Result<TrimReport, LogsCacheError> {
        let bytes_before = self.db.write(|conn| file_bytes(conn)).map_err(db_error)?;
        let trimmed = match keep.days() {
            None => 0,
            Some(days) => self
                .db
                .write(|conn| {
                    conn.execute(
                        concat!(
                            "UPDATE logstf_logs SET body = NULL, state = ?3 WHERE ",
                            trimmable!()
                        ),
                        params![PROVISIONAL_SECS, now - days * DAY_SECS, STATE_TRIMMED],
                    )
                })
                .map_err(db_error)?,
        };
        let recompressed = self.recompress()?;
        let bytes_after = self
            .db
            .write(|conn| {
                if let Err(e) = compact(conn, trimmed, recompressed > 0) {
                    tracing::warn!(error = %e, "the logs.tf cache could not be compacted");
                }
                file_bytes(conn)
            })
            .map_err(db_error)?;
        Ok(TrimReport {
            trimmed,
            recompressed,
            bytes_before,
            bytes_after,
        })
    }

    /// Compresses every plain TEXT body, in id order. A body that is not UTF-8 is damaged and
    /// stays; the id cursor steps past it. The write-back only replaces a body still TEXT, so a
    /// body restored meanwhile is never overwritten by the stale one.
    fn recompress(&self) -> Result<usize, LogsCacheError> {
        let mut after = 0_i64;
        let mut changed = 0;
        loop {
            let batch = self
                .db
                .read(|conn| {
                    let mut stmt = conn.prepare(
                        "SELECT id, body FROM logstf_logs
                         WHERE typeof(body) = 'text' AND id > ?1 ORDER BY id LIMIT ?2",
                    )?;
                    let rows = stmt.query_map(params![after, RECOMPRESS_BATCH], |row| {
                        let text = match row.get_ref(1)? {
                            ValueRef::Text(text) => String::from_utf8(text.to_vec()).ok(),
                            ValueRef::Null
                            | ValueRef::Integer(_)
                            | ValueRef::Real(_)
                            | ValueRef::Blob(_) => None,
                        };
                        Ok((row.get::<_, i64>(0)?, text))
                    })?;
                    rows.collect::<rusqlite::Result<Vec<_>>>()
                })
                .map_err(db_error)?;
            let Some(&(last, _)) = batch.last() else {
                return Ok(changed);
            };
            after = last;
            let mut packed = Vec::new();
            for (id, text) in batch {
                if let Some(text) = text {
                    packed.push((id, deflate(&text)?));
                }
            }
            changed += self
                .db
                .write(|conn| {
                    let tx = conn.transaction()?;
                    let mut n = 0;
                    {
                        let mut set = tx.prepare(
                            "UPDATE logstf_logs SET body = ?2 WHERE id = ?1 AND typeof(body) = 'text'",
                        )?;
                        for (id, bytes) in &packed {
                            n += set.execute(params![id, bytes])?;
                        }
                    }
                    tx.commit()?;
                    Ok(n)
                })
                .map_err(db_error)?;
        }
    }
}

/// The database file's size as SQLite sees it: its page count times its page size.
fn file_bytes(conn: &Connection) -> rusqlite::Result<u64> {
    let pages: i64 = conn.pragma_query_value(None, "page_count", |r| r.get(0))?;
    let size: i64 = conn.pragma_query_value(None, "page_size", |r| r.get(0))?;
    Ok(u64::try_from(pages.saturating_mul(size)).unwrap_or(0))
}

// Gives free pages back to the file system. A trimmed row stays in a near-empty leaf page that
// only VACUUM packs, so the file is rewritten once enough trims add up, or once when an old file
// without auto_vacuum holds many free pages; the rewrite also turns incremental vacuum on.
// Between rewrites an incremental file frees pages in place. A pass that changed nothing and
// left no free pages skips the vacuum but still truncates the -wal a sync grew. A busy
// checkpoint is not an error.
fn compact(conn: &Connection, trimmed: usize, recompressed: bool) -> rusqlite::Result<()> {
    let free: i64 = conn.pragma_query_value(None, "freelist_count", |r| r.get(0))?;
    if trimmed > 0 || recompressed || free > 0 {
        vacuum(conn, trimmed, free)?;
    }
    conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
}

// Counts the pass's trims and rewrites the file or frees its pages in place.
fn vacuum(conn: &Connection, trimmed: usize, free: i64) -> rusqlite::Result<()> {
    let counted: i64 = conn
        .query_row(
            "SELECT CAST(value AS INTEGER) FROM settings WHERE key = ?1",
            [TRIMS_KEY],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(0);
    let since = counted.saturating_add(i64::try_from(trimmed).unwrap_or(i64::MAX));
    let store = |n: i64| {
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            params![TRIMS_KEY, n.to_string()],
        )
    };
    if trimmed > 0 {
        store(since)?;
    }
    let auto: i64 = conn.pragma_query_value(None, "auto_vacuum", |r| r.get(0))?;
    let page: i64 = conn.pragma_query_value(None, "page_size", |r| r.get(0))?;
    let old_file_full = auto != 2 && free.saturating_mul(page) > VACUUM_FREE_BYTES;
    if since >= VACUUM_AFTER_TRIMS || old_file_full {
        conn.execute_batch("PRAGMA auto_vacuum = INCREMENTAL; VACUUM;")?;
        if since != 0 {
            store(0)?;
        }
    } else if auto == 2 && free > 0 {
        // Each step frees one page, so the statement is stepped to its end.
        let mut stmt = conn.prepare("PRAGMA incremental_vacuum")?;
        let mut rows = stmt.query([])?;
        while rows.next()?.is_some() {}
    }
    Ok(())
}
