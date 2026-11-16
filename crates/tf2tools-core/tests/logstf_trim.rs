//! Trimming: which full logs a pass drops, the recompression of old plain bodies, and the
//! database file shrinking after.

use tf2tools_core::{DataDir, Database, KeepFullLogs, LogsCache, SteamId, TrackedPlayer};

use crate::common;
use crate::common::logstf::{ARIE, log_state, logstf_fixture, seed_stored};

const DAY: i64 = 86_400;
/// A fixed clock, so ages never depend on when the test runs.
const NOW: i64 = 1_790_000_000;

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

fn body_type(dir: &DataDir, id: i64) -> String {
    common::sql(dir)
        .query_row(
            "SELECT typeof(body) FROM logstf_logs WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .unwrap()
}

fn pragma(dir: &DataDir, name: &str) -> i64 {
    common::sql(dir)
        .pragma_query_value(None, name, |r| r.get(0))
        .unwrap()
}

/// The main file's size and the -wal's (0 when absent), after a checkpoint by a fresh connection.
fn file_sizes(dir: &DataDir) -> (u64, u64) {
    let wal = dir.db_path().with_extension("db-wal");
    (
        std::fs::metadata(dir.db_path()).unwrap().len(),
        std::fs::metadata(wal).map_or(0, |m| m.len()),
    )
}

fn checkpoint(dir: &DataDir) {
    common::sql(dir)
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
        .unwrap();
}

#[test]
fn keep_options_reach_back_their_days_and_default_to_a_week() {
    let days: Vec<Option<i64>> = KeepFullLogs::ALL.iter().map(|k| k.days()).collect();
    assert_eq!(
        days,
        [
            Some(1),
            Some(3),
            Some(7),
            Some(14),
            Some(30),
            Some(90),
            None
        ]
    );
    let keys: Vec<&str> = KeepFullLogs::ALL.iter().map(|k| k.key()).collect();
    assert_eq!(keys, ["1d", "3d", "7d", "14d", "30d", "90d", "forever"]);
    for keep in KeepFullLogs::ALL {
        assert_eq!(KeepFullLogs::from_key(keep.key()), Some(keep));
    }
    assert_eq!(KeepFullLogs::from_key("2w"), None);
    assert_eq!(KeepFullLogs::default(), KeepFullLogs::Days7);
}

#[test]
fn only_settled_counted_logs_older_than_the_choice_are_trimmed() {
    let (_tmp, dir) = common::temp_data_dir();
    let cache = tracked(&dir);
    let old = NOW - 10 * DAY;
    // 1 trims: counted, settled, fetched ten days ago, never opened.
    seed_stored(&dir, 1, old - DAY, old);
    // 2 was opened two days ago, which is its age whatever its fetch.
    seed_stored(&dir, 2, old - DAY, old);
    // 3 and 4 wait for the weapon and the damage-taken backfill.
    seed_stored(&dir, 3, old - DAY, old);
    seed_stored(&dir, 4, old - DAY, old);
    // 5 was fetched ten minutes after upload, so its settle refetch is still due.
    seed_stored(&dir, 5, old - 600, old);
    // 6 is not a stored log (state 1).
    seed_stored(&dir, 6, old - DAY, old);
    // 7 is exactly seven days old and trims; 8 is a second younger and stays.
    seed_stored(&dir, 7, NOW - 8 * DAY, NOW - 7 * DAY);
    seed_stored(&dir, 8, NOW - 8 * DAY, NOW - 7 * DAY + 1);
    common::sql(&dir)
        .execute_batch(&format!(
            "UPDATE logstf_logs SET opened_at = {} WHERE id = 2;
             UPDATE logstf_logs SET weapons_counted = 0 WHERE id = 3;
             UPDATE logstf_logs SET dt_counted = 0 WHERE id = 4;
             UPDATE logstf_logs SET state = 1 WHERE id = 6;",
            NOW - 2 * DAY
        ))
        .unwrap();
    let class_rows = common::count(&dir, "logstf_class_rows");
    let weapon_rows = common::count(&dir, "logstf_weapon_rows");

    let report = cache.trim(KeepFullLogs::Days7, NOW).unwrap();
    assert_eq!(report.trimmed(), 2);
    assert_eq!(log_state(&dir, 1), (3, false));
    assert_eq!(log_state(&dir, 7), (3, false));
    for id in [2, 3, 4, 5, 8] {
        assert_eq!(log_state(&dir, id), (0, true), "log {id}");
    }
    assert_eq!(log_state(&dir, 6), (1, true));
    assert_eq!(common::count(&dir, "logstf_class_rows"), class_rows);
    assert_eq!(common::count(&dir, "logstf_weapon_rows"), weapon_rows);

    assert_eq!(cache.trim(KeepFullLogs::Forever, NOW).unwrap().trimmed(), 0);
    assert_eq!(cache.trim(KeepFullLogs::Day1, NOW).unwrap().trimmed(), 2);
    assert_eq!(log_state(&dir, 2), (3, false));
    assert_eq!(log_state(&dir, 8), (3, false));
}

#[test]
fn old_plain_bodies_are_trimmed_when_old_and_recompressed_when_fresh() {
    let (_tmp, dir) = common::temp_data_dir();
    let cache = tracked(&dir);
    let plain = logstf_fixture("log-4137103-6v6.json");
    seed_stored(&dir, 1, NOW - 20 * DAY, NOW - 19 * DAY);
    seed_stored(&dir, 2, NOW - 2 * DAY, NOW - DAY);
    seed_stored(&dir, 3, NOW - 2 * DAY, NOW - DAY);
    let conn = common::sql(&dir);
    conn.execute(
        "UPDATE logstf_logs SET body = ?1 WHERE id IN (1, 2)",
        [&plain],
    )
    .unwrap();
    // A TEXT body that is not UTF-8 is damaged; it stays as it is.
    conn.execute(
        "UPDATE logstf_logs SET body = CAST(x'fffe' AS TEXT) WHERE id = 3",
        [],
    )
    .unwrap();
    drop(conn);
    assert_eq!(body_type(&dir, 2), "text");

    let report = cache.trim(KeepFullLogs::Days7, NOW).unwrap();
    assert_eq!((report.trimmed(), report.recompressed()), (1, 1));
    assert_eq!(log_state(&dir, 1), (3, false));
    assert_eq!(body_type(&dir, 2), "blob");
    assert_eq!(cache.log_body(2).unwrap(), Some(plain));
    assert_eq!(body_type(&dir, 3), "text");

    let again = cache.trim(KeepFullLogs::Days7, NOW).unwrap();
    assert_eq!((again.trimmed(), again.recompressed()), (0, 0));
}

#[test]
fn trimming_shrinks_a_file_an_earlier_build_made() {
    let (_tmp, dir) = common::temp_data_dir();
    drop(Database::open(&dir).unwrap());
    // Earlier builds created files without auto_vacuum, which never shrink on their own.
    common::sql(&dir)
        .execute_batch("PRAGMA auto_vacuum = NONE; VACUUM;")
        .unwrap();
    assert_eq!(pragma(&dir, "auto_vacuum"), 0);
    let cache = tracked(&dir);
    for id in 1..=300 {
        seed_stored(&dir, id, NOW - 20 * DAY, NOW - 19 * DAY);
    }
    checkpoint(&dir);
    let (before, _) = file_sizes(&dir);

    let report = cache.trim(KeepFullLogs::Days7, NOW).unwrap();
    assert_eq!(report.trimmed(), 300);
    let (after, wal) = file_sizes(&dir);
    assert!(after + 2_000_000 < before, "{before} -> {after} bytes");
    assert_eq!(wal, 0, "the checkpoint truncates the -wal");
    assert!(report.bytes_after() + 2_000_000 < report.bytes_before());
    assert_eq!(
        pragma(&dir, "auto_vacuum"),
        2,
        "the rewrite turns on incremental vacuum"
    );
    assert_eq!(pragma(&dir, "freelist_count"), 0);
}

#[test]
fn a_small_trim_of_an_incremental_file_returns_its_free_pages() {
    let (_tmp, dir) = common::temp_data_dir();
    let cache = tracked(&dir);
    for id in 1..=100 {
        seed_stored(&dir, id, NOW - 20 * DAY, NOW - 19 * DAY);
    }
    checkpoint(&dir);
    let (before, _) = file_sizes(&dir);
    assert_eq!(cache.trim(KeepFullLogs::Days7, NOW).unwrap().trimmed(), 100);
    let (after, wal) = file_sizes(&dir);
    assert_eq!(
        pragma(&dir, "freelist_count"),
        0,
        "every freed page is returned"
    );
    // Each 6v6 body spills one overflow page; those pages go back at once.
    assert!(after + 100 * 4_096 <= before, "{before} -> {after} bytes");
    assert_eq!(wal, 0);
    assert_eq!(
        common::setting(&dir, "logstf_trimmed_since_vacuum").as_deref(),
        Some("100")
    );
}

#[test]
fn a_pass_with_nothing_to_do_leaves_the_trim_counter_alone() {
    let (_tmp, dir) = common::temp_data_dir();
    let cache = tracked(&dir);
    seed_stored(&dir, 1, NOW - 2 * DAY, NOW - DAY);
    checkpoint(&dir);
    let report = cache.trim(KeepFullLogs::Days7, NOW).unwrap();
    assert_eq!((report.trimmed(), report.recompressed()), (0, 0));
    assert_eq!(common::setting(&dir, "logstf_trimmed_since_vacuum"), None);
}

#[test]
fn trims_that_add_up_rewrite_the_file() {
    let (_tmp, dir) = common::temp_data_dir();
    let cache = tracked(&dir);
    for id in 1..=300 {
        seed_stored(&dir, id, NOW - 20 * DAY, NOW - 19 * DAY);
    }
    checkpoint(&dir);
    let (before, _) = file_sizes(&dir);
    assert_eq!(cache.trim(KeepFullLogs::Days7, NOW).unwrap().trimmed(), 300);
    let (after, wal) = file_sizes(&dir);
    // The rewrite also packs the near-empty pages the trimmed rows sit in.
    assert!(after + 2_000_000 < before, "{before} -> {after} bytes");
    assert_eq!(wal, 0);
    assert_eq!(
        common::setting(&dir, "logstf_trimmed_since_vacuum").as_deref(),
        Some("0")
    );
}

#[test]
fn an_old_file_is_rewritten_once_its_free_pages_pass_a_mib() {
    let (_tmp, dir) = common::temp_data_dir();
    drop(Database::open(&dir).unwrap());
    common::sql(&dir)
        .execute_batch("PRAGMA auto_vacuum = NONE; VACUUM;")
        .unwrap();
    let cache = tracked(&dir);
    // A hundred trims stay under the counter's rewrite; their 16 KiB bodies free over a MiB.
    for id in 1..=100 {
        seed_stored(&dir, id, NOW - 20 * DAY, NOW - 19 * DAY);
    }
    common::sql(&dir)
        .execute("UPDATE logstf_logs SET body = randomblob(16384)", [])
        .unwrap();
    checkpoint(&dir);
    assert_eq!(pragma(&dir, "auto_vacuum"), 0);
    let (before, _) = file_sizes(&dir);

    assert_eq!(cache.trim(KeepFullLogs::Days7, NOW).unwrap().trimmed(), 100);
    let (after, _) = file_sizes(&dir);
    assert!(after + 1024 * 1024 < before, "{before} -> {after} bytes");
    assert_eq!(pragma(&dir, "auto_vacuum"), 2);
    assert_eq!(
        common::setting(&dir, "logstf_trimmed_since_vacuum").as_deref(),
        Some("0")
    );
}

#[test]
fn a_pass_that_changes_nothing_still_truncates_the_wal() {
    let (_tmp, dir) = common::temp_data_dir();
    let cache = tracked(&dir);
    // Fresh logs leave the -wal grown while the cache's connection keeps it open.
    for id in 1..=20 {
        seed_stored(&dir, id, NOW - 2 * DAY, NOW - DAY);
    }
    assert!(file_sizes(&dir).1 > 0, "the seed writes go to the -wal");
    let report = cache.trim(KeepFullLogs::Days7, NOW).unwrap();
    assert_eq!((report.trimmed(), report.recompressed()), (0, 0));
    assert_eq!(pragma(&dir, "freelist_count"), 0);
    assert_eq!(file_sizes(&dir).1, 0, "the checkpoint truncates the -wal");
}
