//! Weapon rows checked from inside the crate, where `ingest` and `mark_failed` are reachable.

use serde_json::json;
use tempfile::TempDir;

use crate::db::Database;
use crate::logstf::cache::{ListedLog, STATE_GONE};
use crate::logstf::{LogsCache, SteamId, TrackedPlayer, parse_log};
use crate::paths::DataDir;

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
