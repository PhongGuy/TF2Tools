//! The keep setting, and the space figures Settings shows: the cache, its full logs, and what
//! each keep choice would free.

use tf2tools_core::{DataDir, Database, KeepFullLogs, LogsCache, SteamId, TrackedPlayer};

use crate::common;
use crate::common::logstf::{ARIE, seed_stored};

const DAY: i64 = 86_400;
const NOW: i64 = 1_790_000_000;
/// SQLite's default page size, which every new database uses.
const PAGE: u64 = 4_096;

fn tracked(dir: &DataDir) -> LogsCache {
    let cache = LogsCache::new(Database::open(dir).unwrap());
    cache
        .add_player(&TrackedPlayer::new(
            SteamId::from_id64(ARIE).unwrap(),
            "Arie",
        ))
        .unwrap();
    cache
}

#[test]
fn the_keep_setting_defaults_to_a_week_and_saves() {
    let (_tmp, dir) = common::temp_data_dir();
    let cache = tracked(&dir);
    assert_eq!(cache.keep_full_logs().unwrap(), KeepFullLogs::Days7);
    cache.set_keep_full_logs(KeepFullLogs::Days30).unwrap();
    assert_eq!(cache.keep_full_logs().unwrap(), KeepFullLogs::Days30);
    assert_eq!(
        common::setting(&dir, "logstf_keep_full_logs").as_deref(),
        Some("30d")
    );
    cache.set_keep_full_logs(KeepFullLogs::Forever).unwrap();
    assert_eq!(cache.keep_full_logs().unwrap(), KeepFullLogs::Forever);
    common::sql(&dir)
        .execute(
            "UPDATE settings SET value = '5y' WHERE key = 'logstf_keep_full_logs'",
            [],
        )
        .unwrap();
    assert_eq!(cache.keep_full_logs().unwrap(), KeepFullLogs::Days7);
}

#[test]
fn an_empty_cache_holds_one_page_per_table_and_index_and_frees_nothing() {
    let (_tmp, dir) = common::temp_data_dir();
    let cache = tracked(&dir);
    let space = cache.space(NOW).unwrap();
    // Five tables, three indexes and the three primary-key indexes SQLite adds itself.
    assert_eq!(space.cache_bytes(), 11 * PAGE);
    assert_eq!(space.full_bytes(), PAGE);
    assert_eq!(space.full_logs(), 0);
    for keep in KeepFullLogs::ALL {
        assert_eq!(space.frees(keep), 0, "{keep:?}");
    }
}

#[test]
fn each_choice_frees_its_share_of_the_full_logs() {
    let (_tmp, dir) = common::temp_data_dir();
    let cache = tracked(&dir);
    // Ten logs two days old, twenty twenty days old, thirty sixty days old; one body each,
    // all the same size, so each choice frees an exact share.
    for id in 1..=60 {
        let age = match id {
            1..=10 => 2,
            11..=30 => 20,
            _ => 60,
        };
        seed_stored(&dir, id, NOW - (age + 1) * DAY, NOW - age * DAY);
    }
    // A trimmed log holds no body and counts toward nothing.
    seed_stored(&dir, 61, NOW - 70 * DAY, NOW - 69 * DAY);
    common::sql(&dir)
        .execute(
            "UPDATE logstf_logs SET body = NULL, state = 3 WHERE id = 61",
            [],
        )
        .unwrap();
    let space = cache.space(NOW).unwrap();
    assert_eq!(space.full_logs(), 60);
    let full = space.full_bytes();
    assert!(full > 60 * 4_000, "{full} bytes for 60 bodies");
    assert!(space.cache_bytes() > full);
    let share = |n: u64| full * n / 60;
    let expected = [
        (KeepFullLogs::Day1, share(60)),
        (KeepFullLogs::Days3, share(50)),
        (KeepFullLogs::Days7, share(50)),
        (KeepFullLogs::Days14, share(50)),
        (KeepFullLogs::Days30, share(30)),
        (KeepFullLogs::Days90, 0),
        (KeepFullLogs::Forever, 0),
    ];
    for (keep, frees) in expected {
        assert_eq!(space.frees(keep), frees, "{keep:?}");
    }
    // The figures describe what a trim with that choice drops.
    assert_eq!(cache.trim(KeepFullLogs::Days30, NOW).unwrap().trimmed(), 30);
    assert_eq!(cache.space(NOW).unwrap().full_logs(), 30);
}
