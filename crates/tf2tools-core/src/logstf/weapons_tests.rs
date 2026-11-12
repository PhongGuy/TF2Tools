//! Weapon rows checked from inside the crate, where `ingest` and `mark_failed` are reachable.

use serde_json::json;
use tempfile::TempDir;

use super::BackfillStep;
use crate::db::Database;
use crate::error::LogsCacheError;
use crate::logstf::cache::{ListedLog, STATE_GONE};
use crate::logstf::{LogsCache, LogsRange, SteamId, TrackedPlayer, parse_log};
use crate::paths::DataDir;
use crate::weapons::TfClass;

const ARIE: u64 = 76_561_197_960_497_430;
const SIXES: &str = include_str!("../../tests/fixtures/logstf/log-4137103-6v6.json");

fn arie() -> SteamId {
    SteamId::from_id64(ARIE).unwrap()
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// A fresh cache with Arie tracked.
fn cache() -> (TempDir, DataDir, LogsCache) {
    let tmp = tempfile::tempdir().unwrap();
    let dir = DataDir::at(tmp.path().join("com.tf2tools.app"));
    let cache = LogsCache::new(Database::open(&dir).unwrap());
    cache
        .add_player(&TrackedPlayer::new(arie(), "Arie"))
        .unwrap();
    (tmp, dir, cache)
}

/// Lists log `id` for Arie a day back and stores `body` as its download.
fn store(cache: &LogsCache, id: i64, body: &str) {
    let now = now();
    let listed = ListedLog {
        id,
        title: format!("Log {id}"),
        map: "m".to_owned(),
        date: now - 86_400,
    };
    cache.record_list(arie(), &[listed], now).unwrap();
    assert!(
        cache
            .ingest(id, body, &parse_log(body).unwrap(), now)
            .unwrap()
    );
}

fn sql(dir: &DataDir) -> rusqlite::Connection {
    rusqlite::Connection::open(dir.db_path()).unwrap()
}

fn weapon_rows(dir: &DataDir, log_id: i64) -> i64 {
    sql(dir)
        .query_row(
            "SELECT COUNT(*) FROM logstf_weapon_rows WHERE log_id = ?1",
            [log_id],
            |r| r.get(0),
        )
        .unwrap()
}

#[test]
fn a_class_listed_twice_folds_into_one_row_per_weapon() {
    let (_tmp, dir, cache) = cache();
    let scout = json!({"type": "scout", "total_time": 300, "kills": 1, "deaths": 0,
        "assists": 0, "dmg": 100,
        "weapon": {"scattergun": {"kills": 1, "dmg": 100, "avg_dmg": 10, "shots": 10, "hits": 5}}});
    let body = json!({"version": 3, "length": 600, "teams": {"Red": {}, "Blue": {}},
        "players": {"[U:1:231702]": {"team": "Red", "kills": 2, "deaths": 0, "assists": 0,
            "dmg": 200, "heal": 0, "ubers": 0, "drops": 0,
            "class_stats": [scout.clone(), scout]}},
        "names": {"[U:1:231702]": "Arie"}, "info": {}})
    .to_string();
    store(&cache, 5, &body);
    assert_eq!(weapon_rows(&dir, 5), 1);
    let row: (i64, i64, i64, i64) = sql(&dir)
        .query_row(
            "SELECT kills, dmg, shots, hits FROM logstf_weapon_rows
             WHERE log_id = 5 AND class = 'scout' AND weapon = 'scattergun'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(row, (2, 200, 20, 10));
}

#[test]
fn marking_a_log_failed_drops_its_weapon_rows() {
    let (_tmp, dir, cache) = cache();
    store(&cache, 3, SIXES);
    assert!(weapon_rows(&dir, 3) > 0);
    cache.mark_failed(3, STATE_GONE, now()).unwrap();
    assert_eq!(weapon_rows(&dir, 3), 0);
}

/// Log 3 stored, then its weapon rows and marker cleared as a step-15 database left them.
fn uncounted(dir: &DataDir, cache: &LogsCache) {
    store(cache, 3, SIXES);
    sql(dir)
        .execute_batch(
            "DELETE FROM logstf_weapon_rows; UPDATE logstf_logs SET weapons_counted = 0;",
        )
        .unwrap();
}

fn mark(dir: &DataDir, id: i64) -> Option<i64> {
    use rusqlite::OptionalExtension as _;
    sql(dir)
        .query_row(
            "SELECT weapons_counted FROM logstf_logs WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .optional()
        .unwrap()
}

#[test]
fn a_log_pruned_between_read_and_write_is_skipped() {
    let (_tmp, dir, cache) = cache();
    uncounted(&dir, &cache);
    let racer = cache.clone();
    let hook = move |step: BackfillStep, _: i64| {
        if step == BackfillStep::BeforeWrite {
            racer.remove_player(arie()).unwrap();
        }
    };
    let done = cache.count_weapons_with(50, &hook).unwrap();
    assert_eq!((done.counted(), done.left()), (0, 0));
    assert_eq!((mark(&dir, 3), weapon_rows(&dir, 3)), (None, 0));
}

#[test]
fn a_log_marked_failed_between_read_and_write_is_skipped() {
    let (_tmp, dir, cache) = cache();
    uncounted(&dir, &cache);
    let racer = cache.clone();
    let hook = move |step: BackfillStep, id: i64| {
        if step == BackfillStep::BeforeWrite {
            racer.mark_failed(id, STATE_GONE, now()).unwrap();
        }
    };
    let done = cache.count_weapons_with(50, &hook).unwrap();
    assert_eq!((done.counted(), done.left()), (0, 0));
    assert_eq!((mark(&dir, 3), weapon_rows(&dir, 3)), (Some(0), 0));
}

#[test]
fn a_reingest_between_read_and_write_keeps_its_own_rows() {
    let (_tmp, dir, cache) = cache();
    uncounted(&dir, &cache);
    let hl = include_str!("../../tests/fixtures/logstf/log-4150405-hl.json")
        .replace("U:1:187445387", "U:1:231702");
    let racer = cache.clone();
    let hook = move |step: BackfillStep, id: i64| {
        if step == BackfillStep::BeforeWrite {
            racer
                .ingest(id, &hl, &parse_log(&hl).unwrap(), now())
                .unwrap();
        }
    };
    let done = cache.count_weapons_with(50, &hook).unwrap();
    assert_eq!((done.counted(), done.left()), (0, 0));
    assert_eq!(mark(&dir, 3), Some(1));
    let count =
        |sql_text: &str| -> i64 { sql(&dir).query_row(sql_text, [], |r| r.get(0)).unwrap() };
    // The highlander body has Arie's guillotine and no Pyro; the 6v6 body read first has Pyro.
    assert_eq!(
        count(
            "SELECT COUNT(*) FROM logstf_weapon_rows WHERE log_id = 3 AND weapon = 'guillotine' AND steam_id = 76561197960497430"
        ),
        1
    );
    assert_eq!(
        count(
            "SELECT COUNT(*) FROM logstf_weapon_rows WHERE log_id = 3 AND class = 'pyro' AND steam_id = 76561197960497430"
        ),
        0
    );
}

#[test]
fn a_body_gone_after_selection_is_skipped() {
    let (_tmp, dir, cache) = cache();
    uncounted(&dir, &cache);
    let db = dir.db_path();
    let hook = move |step: BackfillStep, id: i64| {
        if step == BackfillStep::BeforeRead {
            rusqlite::Connection::open(&db)
                .unwrap()
                .execute("UPDATE logstf_logs SET body = NULL WHERE id = ?1", [id])
                .unwrap();
        }
    };
    let done = cache.count_weapons_with(50, &hook).unwrap();
    assert_eq!((done.counted(), done.left()), (0, 0));
    assert_eq!(mark(&dir, 3), Some(0));
}

#[test]
fn a_database_error_on_read_leaves_the_marker_unset() {
    let (_tmp, dir, cache) = cache();
    uncounted(&dir, &cache);
    let db = dir.db_path();
    let hook = move |step: BackfillStep, _: i64| {
        if step == BackfillStep::BeforeRead {
            rusqlite::Connection::open(&db)
                .unwrap()
                .execute_batch("ALTER TABLE logstf_logs RENAME TO logstf_logs_away;")
                .unwrap();
        }
    };
    let read = cache.count_weapons_with(50, &hook);
    assert!(
        matches!(read, Err(LogsCacheError::Database { .. })),
        "{read:?}"
    );
    sql(&dir)
        .execute_batch("ALTER TABLE logstf_logs_away RENAME TO logstf_logs;")
        .unwrap();
    assert_eq!(mark(&dir, 3), Some(0));
}

#[test]
fn a_batch_committed_mid_read_does_not_skew_the_counts() {
    let (_tmp, dir, cache) = cache();
    uncounted(&dir, &cache);
    // A second handle on the same file stands in for the backfill task; it has its own locks.
    let backfill = LogsCache::new(Database::open(&dir).unwrap());
    let between = || {
        assert_eq!(backfill.count_weapons(50).unwrap().counted(), 1);
    };
    let scout = cache
        .class_weapons_with(arie(), LogsRange::All, TfClass::Scout, &between)
        .unwrap();
    // The sums were read before the batch, so the counts must describe that same state.
    assert!(scout.weapons().is_empty());
    assert_eq!((scout.logs(), scout.pending()), (0, 1));
}
