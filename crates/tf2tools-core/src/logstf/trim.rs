//! Trimming: dropping the full body of logs nobody opened lately, and giving the space back.

use rusqlite::types::ValueRef;
use rusqlite::{Connection, OptionalExtension, params};

use super::cache::{STATE_TRIMMED, db_error, deflate};
use super::{KeepFullLogs, LogsCache, PROVISIONAL_SECS};
use crate::error::LogsCacheError;

/// The `settings` key counting logs trimmed since the file was last rewritten.
const TRIMS_KEY: &str = "logstf_trimmed_since_vacuum";
/// Trimmed logs that leave about a MiB of near-empty pages, when the file is rewritten.
const VACUUM_AFTER_TRIMS: i64 = 256;
/// A file without auto_vacuum is rewritten once its free pages pass this many bytes.
const VACUUM_FREE_BYTES: i64 = 1024 * 1024;
/// Plain bodies read per recompression batch.
const RECOMPRESS_BATCH: i64 = 50;

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

impl LogsCache {
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
                        params![PROVISIONAL_SECS, now - days * 86_400, STATE_TRIMMED],
                    )
                })
                .map_err(db_error)?,
        };
        let recompressed = self.recompress()?;
        let bytes_after = self
            .db
            .write(|conn| {
                if let Err(e) = compact(conn, trimmed) {
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
// Between rewrites an incremental file frees pages in place. A busy checkpoint is not an error.
fn compact(conn: &Connection, trimmed: usize) -> rusqlite::Result<()> {
    let since: i64 = conn
        .query_row(
            "SELECT CAST(value AS INTEGER) FROM settings WHERE key = ?1",
            [TRIMS_KEY],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(0);
    let since = since.saturating_add(i64::try_from(trimmed).unwrap_or(i64::MAX));
    let store = |n: i64| {
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            params![TRIMS_KEY, n.to_string()],
        )
    };
    store(since)?;
    let auto: i64 = conn.pragma_query_value(None, "auto_vacuum", |r| r.get(0))?;
    let free: i64 = conn.pragma_query_value(None, "freelist_count", |r| r.get(0))?;
    let page: i64 = conn.pragma_query_value(None, "page_size", |r| r.get(0))?;
    let old_file_full = auto != 2 && free.saturating_mul(page) > VACUUM_FREE_BYTES;
    if since >= VACUUM_AFTER_TRIMS || old_file_full {
        conn.execute_batch("PRAGMA auto_vacuum = INCREMENTAL; VACUUM;")?;
        store(0)?;
    } else if auto == 2 && free > 0 {
        // Each step frees one page, so the statement is stepped to its end.
        let mut stmt = conn.prepare("PRAGMA incremental_vacuum")?;
        let mut rows = stmt.query([])?;
        while rows.next()?.is_some() {}
    }
    conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
}
