//! v5's resumable progressive sync for the GameBanana stores: one transaction per page
//! that upserts the rows and saves a checkpoint, then, only after a confirmed end of the
//! catalogue, a sweep of rows the session did not see.

use rusqlite::{Connection, OptionalExtension, params};

use super::gamebanana::{self, IndexPage, Records};
use super::pacer::{Priority, Reply};
use super::{StoreClient, StoreKind, SyncOutcome, SyncProgress};
use crate::error::SyncError;
use crate::timestamp;

/// The meta key naming the order the sound cache was walked in, and the current order.
/// It outlives checkpoints: `clear_session` never removes it.
const ORDER_KEY: &str = "sounds_sync_order";
const SOUND_ORDER: &str = "newest";

/// An interrupted sync's checkpoint: the next page, when it started and items so far.
struct Session {
    page: u32,
    start: String,
    fetched: u32,
}

/// Syncs a GameBanana store page by page, resuming from a checkpoint when one exists.
pub(crate) async fn gamebanana(
    client: &StoreClient,
    kind: StoreKind,
    force: bool,
    progress: &(dyn Fn(SyncProgress) + Send + Sync),
) -> Result<SyncOutcome, SyncError> {
    let key = kind.key();
    // A sound cache not walked newest-first (v5's most-liked walk) is stale and its
    // checkpoint points into the wrong order, so both are discarded.
    let reset = kind == StoreKind::Sounds
        && client
            .db
            .read(|conn| get_meta(conn, ORDER_KEY))
            .map_err(database)?
            .as_deref()
            != Some(SOUND_ORDER);
    let stale = reset || stale(client, kind)?;
    let session = client
        .db
        .read(|conn| read_session(conn, key))
        .map_err(database)?;
    if !(stale || force || session.is_some()) {
        return Ok(SyncOutcome::skipped());
    }

    let session = if force || reset {
        client
            .db
            .write(|conn| clear_session(conn, key))
            .map_err(database)?;
        None
    } else {
        session
    };
    let Session {
        mut page,
        start,
        mut fetched,
    } = session.unwrap_or_else(|| Session {
        page: 1,
        start: timestamp::now(),
        fetched: 0,
    });

    let mut total = 0;
    // GameBanana's count can exceed its listing. Once a full page is in, a parsed page with
    // no records is the page past the end, and ends the walk as unconfirmed.
    let mut full_seen = fetched >= gamebanana::PAGE_SIZE;
    let confirmed = loop {
        let index = fetch_page(client, kind, page, full_seen).await?;
        let count = u32::try_from(index.records.len()).unwrap_or(u32::MAX);
        if count == 0 {
            break false;
        }
        full_seen |= count >= gamebanana::PAGE_SIZE;
        if total == 0 {
            total = index.total;
        }
        fetched = fetched.saturating_add(count);

        // Rows, checkpoint and the run's distinct-row count share one transaction, so a
        // sync dropped or failed between pages resumes after the last page it stored.
        let distinct = client
            .db
            .write(|conn| {
                let tx = conn.transaction()?;
                upsert(&tx, kind, &index.records, &timestamp::now())?;
                if kind == StoreKind::Sounds {
                    set_meta(&tx, ORDER_KEY, SOUND_ORDER)?;
                }
                save_checkpoint(&tx, key, page + 1, &start, fetched)?;
                let distinct = seen_since(&tx, kind, &start)?;
                tx.commit()?;
                Ok(distinct)
            })
            .map_err(database)?;
        progress(SyncProgress { fetched, total });

        // A confirmed end: a short last page, or every catalogue item seen this run. A full
        // page that claims completeness is not trusted, so one bad flag cannot wipe rows.
        let short_last = !index.more && count < gamebanana::PAGE_SIZE;
        if short_last || (total > 0 && distinct >= u64::from(total)) {
            break true;
        }
        if page >= client.config.page_limit(kind, total) {
            break false;
        }
        page += 1;
    };
    if !confirmed {
        tracing::warn!(
            ?kind,
            page,
            "store sync stopped before a confirmed end; unseen rows kept"
        );
    }

    let rows = client
        .db
        .write(|conn| {
            let tx = conn.transaction()?;
            if confirmed {
                drop_unseen(&tx, kind, &start)?;
            }
            let rows = stamp(&tx, kind)?;
            clear_session(&tx, key)?;
            tx.commit()?;
            Ok(rows)
        })
        .map_err(database)?;
    Ok(SyncOutcome {
        fresh: false,
        fetched,
        rows,
        rate_limited: 0,
    })
}

/// Fetches one index page as a background call through the pacer. A blank body, or a page
/// with no records before `full_seen`, is retried until it fails as `Throttled`; after it,
/// that page is returned as the listing's end. Other unparseable bodies fail `Malformed`.
async fn fetch_page(
    client: &StoreClient,
    kind: StoreKind,
    page: u32,
    full_seen: bool,
) -> Result<IndexPage, SyncError> {
    let Some(url) = gamebanana::index_url(client.endpoints.gamebanana(), kind, page) else {
        return Err(malformed(String::new(), "not a GameBanana store"));
    };
    let read = |body: &[u8]| match gamebanana::parse_index(kind, body) {
        Ok(index) if index.records.len() > 0 => Reply::Done(Ok(index)),
        Ok(index) if full_seen => Reply::Rejected(Ok(index)),
        Ok(_) => Reply::Empty,
        Err(_) if body.trim_ascii().is_empty() => Reply::Empty,
        Err(e) => Reply::Rejected(Err(e.to_string())),
    };
    client
        .gamebanana_call(&url, Priority::Background, read)
        .await?
        .map_err(|message| malformed(url, &message))
}

/// A sync's closing steps inside the caller's transaction: drop rows older than `start`,
/// stamp `{key}_last_synced` and return the table's row count.
pub(super) fn finish(tx: &Connection, kind: StoreKind, start: &str) -> rusqlite::Result<u64> {
    drop_unseen(tx, kind, start)?;
    stamp(tx, kind)
}

/// Deletes the rows this run (started at `start`) did not see.
fn drop_unseen(tx: &Connection, kind: StoreKind, start: &str) -> rusqlite::Result<()> {
    // The table name is one of the crate's own constants, never caller text.
    tx.execute(
        &format!("DELETE FROM {} WHERE fetched_at < ?1", kind.table()),
        [start],
    )?;
    Ok(())
}

/// Stamps `{key}_last_synced` and returns the table's row count.
fn stamp(tx: &Connection, kind: StoreKind) -> rusqlite::Result<u64> {
    set_meta(
        tx,
        &format!("{}_last_synced", kind.key()),
        &timestamp::now(),
    )?;
    let rows: i64 = tx.query_row(&format!("SELECT COUNT(*) FROM {}", kind.table()), [], |r| {
        r.get(0)
    })?;
    Ok(u64::try_from(rows).unwrap_or(0))
}

/// The distinct rows this run has stored, compared as timestamp text like the cleanup.
fn seen_since(conn: &Connection, kind: StoreKind, start: &str) -> rusqlite::Result<u64> {
    let rows: i64 = conn.query_row(
        &format!(
            "SELECT COUNT(*) FROM {} WHERE fetched_at >= ?1",
            kind.table()
        ),
        [start],
        |r| r.get(0),
    )?;
    Ok(u64::try_from(rows).unwrap_or(0))
}

/// True when the store has never synced or its last sync is `sync_cache_hours` old.
pub(super) fn stale(client: &StoreClient, kind: StoreKind) -> Result<bool, SyncError> {
    let hours = client.db.settings().map_err(database)?.sync_cache_hours();
    client
        .db
        .read(|conn| is_stale(conn, kind.key(), hours))
        .map_err(database)
}

/// True when `{key}_last_synced` is missing, unparseable or at least `hours` old.
pub(super) fn is_stale(conn: &Connection, key: &str, hours: u32) -> rusqlite::Result<bool> {
    let Some(last) = get_meta(conn, &format!("{key}_last_synced"))? else {
        return Ok(true);
    };
    Ok(match chrono::DateTime::parse_from_rfc3339(&last) {
        Ok(at) => (chrono::Utc::now() - at.to_utc()).num_hours() >= i64::from(hours),
        Err(_) => true,
    })
}

/// The saved checkpoint, or None unless all three keys are present and parse.
fn read_session(conn: &Connection, key: &str) -> rusqlite::Result<Option<Session>> {
    let page = get_meta(conn, &format!("{key}_sync_page"))?;
    let start = get_meta(conn, &format!("{key}_sync_start"))?;
    let fetched = get_meta(conn, &format!("{key}_sync_fetched"))?;
    Ok(match (page, start, fetched) {
        (Some(page), Some(start), Some(fetched)) => match (page.parse(), fetched.parse()) {
            (Ok(page), Ok(fetched)) => Some(Session {
                page,
                start,
                fetched,
            }),
            _ => None,
        },
        _ => None,
    })
}

fn save_checkpoint(
    conn: &Connection,
    key: &str,
    next_page: u32,
    start: &str,
    fetched: u32,
) -> rusqlite::Result<()> {
    set_meta(conn, &format!("{key}_sync_page"), &next_page.to_string())?;
    set_meta(conn, &format!("{key}_sync_start"), start)?;
    set_meta(conn, &format!("{key}_sync_fetched"), &fetched.to_string())
}

fn clear_session(conn: &Connection, key: &str) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM store_cache_meta WHERE key IN (?1, ?2, ?3)",
        [
            format!("{key}_sync_page"),
            format!("{key}_sync_start"),
            format!("{key}_sync_fetched"),
        ],
    )?;
    Ok(())
}

fn get_meta(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT value FROM store_cache_meta WHERE key = ?1",
        [key],
        |r| r.get(0),
    )
    .optional()
}

pub(super) fn set_meta(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO store_cache_meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [key, value],
    )?;
    Ok(())
}

/// Upserts in place (an UPDATE, not a delete and reinsert) so row identity survives.
fn upsert(
    conn: &Connection,
    kind: StoreKind,
    records: &Records,
    fetched_at: &str,
) -> rusqlite::Result<()> {
    match records {
        Records::Sounds(rows) => {
            let mut stmt = conn.prepare(
                "INSERT INTO store_sounds (gb_id, name, author, category, preview_url,
                     thumbnail_url, like_count, view_count, tags_json, profile_url,
                     date_added, fetched_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                 ON CONFLICT(gb_id) DO UPDATE SET name = excluded.name,
                     author = excluded.author, category = excluded.category,
                     preview_url = excluded.preview_url, thumbnail_url = excluded.thumbnail_url,
                     like_count = excluded.like_count, view_count = excluded.view_count,
                     tags_json = excluded.tags_json, profile_url = excluded.profile_url,
                     date_added = excluded.date_added, fetched_at = excluded.fetched_at",
            )?;
            for r in rows {
                stmt.execute(params![
                    r.gb_id,
                    r.name,
                    r.author,
                    r.category,
                    r.preview_url,
                    r.thumbnail_url,
                    r.like_count,
                    r.view_count,
                    r.tags_json,
                    r.profile_url,
                    r.date_added,
                    fetched_at
                ])?;
            }
        }
        Records::Mods(rows) => {
            // The table name is one of the crate's own constants, never caller text.
            let mut stmt = conn.prepare(&format!(
                "INSERT INTO {} (gb_id, name, author, thumbnail_url, images_json, like_count,
                     view_count, tags_json, profile_url, date_added, fetched_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT(gb_id) DO UPDATE SET name = excluded.name,
                     author = excluded.author, thumbnail_url = excluded.thumbnail_url,
                     images_json = excluded.images_json, like_count = excluded.like_count,
                     view_count = excluded.view_count, tags_json = excluded.tags_json,
                     profile_url = excluded.profile_url, date_added = excluded.date_added,
                     fetched_at = excluded.fetched_at",
                kind.table()
            ))?;
            for r in rows {
                stmt.execute(params![
                    r.gb_id,
                    r.name,
                    r.author,
                    r.thumbnail_url,
                    r.images_json,
                    r.like_count,
                    r.view_count,
                    r.tags_json,
                    r.profile_url,
                    r.date_added,
                    fetched_at
                ])?;
            }
        }
    }
    Ok(())
}

pub(super) fn database(e: impl std::fmt::Display) -> SyncError {
    SyncError::Database {
        message: e.to_string(),
    }
}

pub(super) fn malformed(url: String, message: &str) -> SyncError {
    SyncError::Malformed {
        url,
        message: message.to_owned(),
    }
}
