//! v5's resumable progressive sync for the GameBanana stores: one transaction per page
//! that upserts the rows and saves a checkpoint, then, only after a confirmed end of the
//! catalogue, a sweep of rows the session did not see.

use rusqlite::{Connection, OptionalExtension, params};

use super::gamebanana::{self, IndexPage, Records};
use super::gb_maps;
use super::pacer::{Priority, Reply};
use super::{Endpoints, StoreClient, StoreConfig, StoreKind, SyncOutcome, SyncProgress};
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

/// How a walk decides it has seen the whole listing, which is what licenses the prune.
pub(crate) enum Confirm {
    /// A short last page, or as many distinct rows as `_nRecordCount`.
    ShortPageOrCount,
    /// As many distinct rows as `_nRecordCount` only. A real last page short of the count
    /// prunes when two such walks in a row, both above the floor, agree; see `settle_listing_end`.
    CountOrRepeatedEnd,
}

/// The share of `_nRecordCount` (percent) a walk ending on the listing's real last page must
/// have seen before it may take part in a two-walk prune.
const PRUNE_FLOOR_PERCENT: u64 = 95;

/// How a walk ended.
#[derive(Clone, Copy, PartialEq, Eq)]
enum End {
    /// The walk confirmed it saw the whole listing.
    Confirmed,
    /// A short page with `_bIsComplete`, under a walk that does not take that as confirmation.
    LastPage,
    /// An empty page after a full one, or the page limit.
    Stopped,
}

/// A walk's page cap from `_nRecordCount`, its page URL, and its page parser. Each is shared
/// with the sync future, which must stay `Send`.
pub(crate) type PageLimit = Box<dyn Fn(&StoreConfig, u32) -> u32 + Send + Sync>;
pub(crate) type PageUrl = Box<dyn Fn(&Endpoints, u32) -> Option<String> + Send + Sync>;
pub(crate) type ParsePage =
    Box<dyn Fn(&[u8]) -> Result<IndexPage, serde_json::Error> + Send + Sync>;

/// One paged GameBanana listing the engine can walk: where its pages come from, how they
/// parse, the table they land in and the `store_cache_meta` prefix of its checkpoint.
pub(crate) struct Walk {
    pub(crate) key: &'static str,
    pub(crate) table: &'static str,
    pub(crate) page_size: u32,
    /// The most pages to walk, from `_nRecordCount`.
    pub(crate) page_limit: PageLimit,
    pub(crate) url: PageUrl,
    pub(crate) parse: ParsePage,
    pub(crate) confirm: Confirm,
    /// A meta key and value written in each page's transaction.
    pub(crate) page_meta: Option<(&'static str, &'static str)>,
}

impl Walk {
    /// The walk of a GameBanana store's index; None for a store GameBanana does not list.
    pub(crate) fn for_store(kind: StoreKind) -> Option<Walk> {
        match kind {
            StoreKind::Sounds | StoreKind::Crosshairs | StoreKind::GameBananaHuds => {}
            StoreKind::CuratedHuds | StoreKind::Addons => return None,
        }
        Some(Walk {
            key: kind.key(),
            table: kind.table(),
            page_size: gamebanana::PAGE_SIZE,
            page_limit: Box::new(move |config, total| config.page_limit(kind, total)),
            url: Box::new(move |endpoints, page| {
                gamebanana::index_url(endpoints.gamebanana(), kind, page)
            }),
            parse: Box::new(move |body| gamebanana::parse_index(kind, body)),
            confirm: Confirm::ShortPageOrCount,
            page_meta: (kind == StoreKind::Sounds).then_some((ORDER_KEY, SOUND_ORDER)),
        })
    }
}

/// Where a walk begins: a saved checkpoint (`resumed`) or page 1 of a fresh run.
pub(crate) struct Start {
    session: Session,
    pub(crate) resumed: bool,
}

/// Syncs a GameBanana store page by page, resuming from a checkpoint when one exists.
pub(crate) async fn gamebanana(
    client: &StoreClient,
    kind: StoreKind,
    force: bool,
    progress: &(dyn Fn(SyncProgress) + Send + Sync),
) -> Result<SyncOutcome, SyncError> {
    // A sound cache not walked newest-first (v5's most-liked walk) is stale and its
    // checkpoint points into the wrong order, so both are discarded.
    let reset = kind == StoreKind::Sounds
        && client
            .db
            .read(|conn| get_meta(conn, ORDER_KEY))
            .map_err(database)?
            .as_deref()
            != Some(SOUND_ORDER);
    let Some(walk) = Walk::for_store(kind) else {
        return Err(malformed(String::new(), "not a GameBanana store"));
    };
    match begin(client, &walk, force, reset)? {
        Some(start) => run(client, &walk, start, progress).await,
        None => Ok(SyncOutcome::skipped()),
    }
}

/// None when the cache is fresh and no checkpoint waits; `force` or `reset` drop the checkpoint.
pub(crate) fn begin(
    client: &StoreClient,
    walk: &Walk,
    force: bool,
    reset: bool,
) -> Result<Option<Start>, SyncError> {
    let stale = reset || stale_key(client, walk.key)?;
    let session = client
        .db
        .read(|conn| read_session(conn, walk.key))
        .map_err(database)?;
    if !(stale || force || session.is_some()) {
        return Ok(None);
    }

    let session = if force || reset {
        client
            .db
            .write(|conn| clear_session(conn, walk.key))
            .map_err(database)?;
        None
    } else {
        session
    };
    let resumed = session.is_some();
    let session = session.unwrap_or_else(|| Session {
        page: 1,
        start: timestamp::now(),
        fetched: 0,
    });
    Ok(Some(Start { session, resumed }))
}

/// Walks the listing from `start` to a confirmed or unconfirmed end, then closes the run:
/// a confirmed end drops rows the run did not see, and either end stamps the cache.
pub(crate) async fn run(
    client: &StoreClient,
    walk: &Walk,
    start: Start,
    progress: &(dyn Fn(SyncProgress) + Send + Sync),
) -> Result<SyncOutcome, SyncError> {
    let Start {
        session:
            Session {
                mut page,
                start,
                mut fetched,
            },
        resumed,
    } = start;
    tracing::debug!(key = walk.key, page, resumed, "store sync walking");

    let mut total = 0;
    let mut distinct = 0;
    // GameBanana's count can exceed its listing. Once a full page is in, a parsed page with
    // no records is the page past the end, and ends the walk as unconfirmed.
    let mut full_seen = fetched >= walk.page_size;
    let end = loop {
        let index = fetch_page(client, walk, page, full_seen).await?;
        let count = u32::try_from(index.records.len()).unwrap_or(u32::MAX);
        if count == 0 {
            break End::Stopped;
        }
        full_seen |= count >= walk.page_size;
        if total == 0 {
            total = index.total;
        }
        fetched = fetched.saturating_add(count);

        // Rows, checkpoint and the run's distinct-row count share one transaction, so a
        // sync dropped or failed between pages resumes after the last page it stored.
        distinct = client
            .db
            .write(|conn| {
                let tx = conn.transaction()?;
                upsert(&tx, walk.table, &index.records, page, &timestamp::now())?;
                if let Some((key, value)) = walk.page_meta {
                    set_meta(&tx, key, value)?;
                }
                save_checkpoint(&tx, walk.key, page + 1, &start, fetched)?;
                let distinct = seen_since(&tx, walk.table, &start)?;
                tx.commit()?;
                Ok(distinct)
            })
            .map_err(database)?;
        progress(SyncProgress { fetched, total });

        // A confirmed end: every catalogue item seen this run or, where the walk trusts it, a
        // short last page. A full page that claims completeness is not trusted, so one bad
        // flag cannot wipe rows.
        let short_last = !index.more && count < walk.page_size;
        let counted = total > 0 && distinct >= u64::from(total);
        match walk.confirm {
            Confirm::ShortPageOrCount if short_last || counted => break End::Confirmed,
            Confirm::CountOrRepeatedEnd if counted => break End::Confirmed,
            Confirm::CountOrRepeatedEnd if short_last => break End::LastPage,
            Confirm::ShortPageOrCount | Confirm::CountOrRepeatedEnd => {}
        }
        if page >= (walk.page_limit)(&client.config, total) {
            break End::Stopped;
        }
        page += 1;
    };
    if end != End::Confirmed {
        tracing::warn!(
            key = walk.key,
            page,
            "store sync stopped before a confirmed end; unseen rows kept"
        );
    }

    let rows = client
        .db
        .write(|conn| {
            let tx = conn.transaction()?;
            if end == End::Confirmed {
                drop_unseen(&tx, walk.table, &start)?;
            }
            if let Confirm::CountOrRepeatedEnd = walk.confirm {
                settle_listing_end(&tx, walk, end, distinct, total, &start)?;
            }
            let rows = stamp(&tx, walk.key, walk.table)?;
            clear_session(&tx, walk.key)?;
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
    walk: &Walk,
    page: u32,
    full_seen: bool,
) -> Result<IndexPage, SyncError> {
    let Some(url) = (walk.url)(&client.endpoints, page) else {
        return Err(malformed(String::new(), "not a GameBanana store"));
    };
    let read = |body: &[u8]| match (walk.parse)(body) {
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
    drop_unseen(tx, kind.table(), start)?;
    stamp(tx, kind.key(), kind.table())
}

/// Keeps `{key}_listing_end_start`, the start of the last walk that ended on the real last
/// page with at least `PRUNE_FLOOR_PERCENT` of the count. A second such walk drops the rows
/// neither walk saw; any other end forgets the marker, and a confirmed end has already pruned.
fn settle_listing_end(
    tx: &Connection,
    walk: &Walk,
    end: End,
    distinct: u64,
    total: u32,
    start: &str,
) -> rusqlite::Result<()> {
    let key = format!("{}_listing_end_start", walk.key);
    let near_complete = total > 0
        && distinct.saturating_mul(100) >= u64::from(total).saturating_mul(PRUNE_FLOOR_PERCENT);
    if end != End::LastPage || !near_complete {
        tx.execute("DELETE FROM store_cache_meta WHERE key = ?1", [&key])?;
        return Ok(());
    }
    if let Some(marker) = get_meta(tx, &key)? {
        let dropped = drop_unseen(tx, walk.table, &marker)?;
        tracing::warn!(
            key = walk.key,
            dropped,
            distinct,
            total,
            "two walks ended on the listing's last page; rows neither saw dropped"
        );
    }
    set_meta(tx, &key, start)
}

/// Deletes the rows this run (started at `start`) did not see, returning how many.
fn drop_unseen(tx: &Connection, table: &str, start: &str) -> rusqlite::Result<usize> {
    // The table name is one of the crate's own constants, never caller text.
    tx.execute(
        &format!("DELETE FROM {table} WHERE fetched_at < ?1"),
        [start],
    )
}

/// Stamps `{key}_last_synced` and returns the table's row count.
fn stamp(tx: &Connection, key: &str, table: &str) -> rusqlite::Result<u64> {
    set_meta(tx, &format!("{key}_last_synced"), &timestamp::now())?;
    let rows: i64 = tx.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?;
    Ok(u64::try_from(rows).unwrap_or(0))
}

/// The distinct rows this run has stored, compared as timestamp text like the cleanup.
fn seen_since(conn: &Connection, table: &str, start: &str) -> rusqlite::Result<u64> {
    let rows: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM {table} WHERE fetched_at >= ?1"),
        [start],
        |r| r.get(0),
    )?;
    Ok(u64::try_from(rows).unwrap_or(0))
}

/// True when the store has never synced or its last sync is `sync_cache_hours` old.
pub(super) fn stale(client: &StoreClient, kind: StoreKind) -> Result<bool, SyncError> {
    stale_key(client, kind.key())
}

/// True when the cache under `key` has never synced or is `sync_cache_hours` old.
pub(super) fn stale_key(client: &StoreClient, key: &str) -> Result<bool, SyncError> {
    let hours = client.db.settings().map_err(database)?.sync_cache_hours();
    client
        .db
        .read(|conn| is_stale(conn, key, hours))
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
    table: &str,
    records: &Records,
    page: u32,
    fetched_at: &str,
) -> rusqlite::Result<()> {
    match records {
        Records::Maps(rows) => gb_maps::upsert_maps(conn, rows, page, fetched_at)?,
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
                "INSERT INTO {table} (gb_id, name, author, thumbnail_url, images_json, like_count,
                     view_count, tags_json, profile_url, date_added, fetched_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT(gb_id) DO UPDATE SET name = excluded.name,
                     author = excluded.author, thumbnail_url = excluded.thumbnail_url,
                     images_json = excluded.images_json, like_count = excluded.like_count,
                     view_count = excluded.view_count, tags_json = excluded.tags_json,
                     profile_url = excluded.profile_url, date_added = excluded.date_added,
                     fetched_at = excluded.fetched_at"
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::paths::DataDir;
    use crate::store::Endpoints;

    fn client(tmp: &tempfile::TempDir) -> StoreClient {
        let dir = DataDir::at(tmp.path().join("data"));
        let db = Database::open(&dir).unwrap();
        StoreClient::new(db, dir, Endpoints::loopback("http://127.0.0.1:1"))
    }

    #[test]
    fn begin_tells_a_resumed_checkpoint_from_a_fresh_walk() {
        let tmp = tempfile::tempdir().unwrap();
        let client = client(&tmp);
        let walk = Walk::for_store(StoreKind::Crosshairs).unwrap();
        let start = begin(&client, &walk, false, false)
            .unwrap()
            .expect("never synced");
        assert!(!start.resumed);
        assert_eq!((start.session.page, start.session.fetched), (1, 0));
        client
            .db
            .write(|c| {
                set_meta(c, "crosshairs_sync_page", "4")?;
                set_meta(
                    c,
                    "crosshairs_sync_start",
                    "2026-11-13T00:00:00.000000+00:00",
                )?;
                set_meta(c, "crosshairs_sync_fetched", "45")
            })
            .unwrap();
        let start = begin(&client, &walk, false, false).unwrap().unwrap();
        assert!(start.resumed);
        assert_eq!((start.session.page, start.session.fetched), (4, 45));
        let start = begin(&client, &walk, true, false).unwrap().unwrap();
        assert!(!start.resumed, "force drops the checkpoint");
        assert_eq!(start.session.page, 1);
    }

    #[test]
    fn begin_skips_a_fresh_store_without_a_checkpoint() {
        let tmp = tempfile::tempdir().unwrap();
        let client = client(&tmp);
        client
            .db
            .write(|c| set_meta(c, "crosshairs_last_synced", &timestamp::now()))
            .unwrap();
        let walk = Walk::for_store(StoreKind::Crosshairs).unwrap();
        assert!(begin(&client, &walk, false, false).unwrap().is_none());
        assert!(begin(&client, &walk, true, false).unwrap().is_some());
        assert!(Walk::for_store(StoreKind::Addons).is_none());
    }
}
