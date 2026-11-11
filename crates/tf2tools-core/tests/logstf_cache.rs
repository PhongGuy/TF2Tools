//! The tracked-player list, the active player and pruning.

use tf2tools_core::{
    DataDir, Database, LogsCache, PRUNE_SECS, STEAM_ID64_BASE, SteamId, TrackedPlayer,
};

use crate::common;

fn sid(account: u64) -> SteamId {
    SteamId::from_id64(STEAM_ID64_BASE + account).unwrap()
}

fn open() -> (tempfile::TempDir, DataDir, LogsCache) {
    let (tmp, dir) = common::temp_data_dir();
    let db = Database::open(&dir).expect("open database");
    (tmp, dir, LogsCache::new(db))
}

fn names(cache: &LogsCache) -> Vec<String> {
    cache
        .players()
        .unwrap()
        .iter()
        .map(|p| p.name().to_owned())
        .collect()
}

/// Inserts a downloaded log with one membership and one class row for `account`.
fn seed_log(dir: &DataDir, id: i64, date: i64, account: u64) {
    let steam = i64::try_from(STEAM_ID64_BASE + account).unwrap();
    common::sql(dir)
        .execute_batch(&format!(
            "INSERT INTO logstf_logs (id, title, map, date) VALUES ({id}, 't', 'm', {date});
             INSERT INTO logstf_player_logs (steam_id, log_id) VALUES ({steam}, {id});
             INSERT INTO logstf_class_rows (log_id, steam_id, class, seconds, kills, deaths, assists, dmg)
                 VALUES ({id}, {steam}, 'scout', 60, 1, 1, 1, 100);"
        ))
        .unwrap();
}

#[test]
fn players_keep_insertion_order_and_the_last_added_is_active() {
    let (_tmp, _dir, cache) = open();
    assert_eq!(cache.active_player().unwrap(), None);
    for (account, name) in [(1, "a"), (2, "b"), (3, "c")] {
        cache
            .add_player(&TrackedPlayer::new(sid(account), name))
            .unwrap();
    }
    assert_eq!(names(&cache), ["a", "b", "c"]);
    assert_eq!(cache.active_player().unwrap(), Some(sid(3)));
    assert_eq!(cache.players().unwrap()[0].last_synced(), None);
}

#[test]
fn re_adding_a_tracked_player_renames_and_activates_without_a_duplicate() {
    let (_tmp, _dir, cache) = open();
    cache
        .add_player(&TrackedPlayer::new(sid(1), sid(1).to_string()))
        .unwrap();
    cache.add_player(&TrackedPlayer::new(sid(2), "b")).unwrap();
    cache
        .add_player(&TrackedPlayer::new(sid(1), "Alice"))
        .unwrap();
    assert_eq!(names(&cache), ["Alice", "b"]);
    assert_eq!(cache.active_player().unwrap(), Some(sid(1)));
    // An offline re-add carries only the placeholder; the real name stays.
    cache
        .add_player(&TrackedPlayer::new(sid(1), sid(1).to_string()))
        .unwrap();
    assert_eq!(names(&cache), ["Alice", "b"]);
}

#[test]
fn removing_the_active_player_falls_back_to_the_first() {
    let (_tmp, _dir, cache) = open();
    for (account, name) in [(1, "a"), (2, "b"), (3, "c")] {
        cache
            .add_player(&TrackedPlayer::new(sid(account), name))
            .unwrap();
    }
    cache.remove_player(sid(3)).unwrap();
    assert_eq!(names(&cache), ["a", "b"]);
    assert_eq!(cache.active_player().unwrap(), Some(sid(1)));
    cache.set_active_player(sid(2)).unwrap();
    assert_eq!(cache.active_player().unwrap(), Some(sid(2)));
    cache.remove_player(sid(1)).unwrap();
    cache.remove_player(sid(2)).unwrap();
    assert_eq!(cache.active_player().unwrap(), None);
}

#[test]
fn a_stale_active_setting_means_the_first_chip() {
    let (_tmp, dir, cache) = open();
    cache.add_player(&TrackedPlayer::new(sid(1), "a")).unwrap();
    cache.add_player(&TrackedPlayer::new(sid(2), "b")).unwrap();
    common::sql(&dir)
        .execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('logstf_active_player', '123')",
            [],
        )
        .unwrap();
    assert_eq!(cache.active_player().unwrap(), Some(sid(1)));
}

#[test]
fn removing_a_player_drops_memberships_but_keeps_their_logs() {
    let (_tmp, dir, cache) = open();
    cache.add_player(&TrackedPlayer::new(sid(1), "a")).unwrap();
    seed_log(&dir, 10, 1_700_000_000, 1);
    cache.remove_player(sid(1)).unwrap();
    assert_eq!(common::count(&dir, "logstf_player_logs"), 0);
    assert_eq!(common::count(&dir, "logstf_logs"), 1);
}

#[test]
fn pruning_deletes_old_untracked_logs_and_cascades_to_their_rows() {
    let (_tmp, dir, cache) = open();
    seed_log(&dir, 1, 1_000, 1);
    seed_log(&dir, 2, 5_000, 1);
    assert_eq!(cache.prune(2_000 + PRUNE_SECS).unwrap(), 1);
    assert_eq!(common::count(&dir, "logstf_logs"), 1);
    assert_eq!(common::count(&dir, "logstf_player_logs"), 1);
    assert_eq!(common::count(&dir, "logstf_class_rows"), 1);
}
