//! The history range: keys, reach, the setting, and the window each range selects.

use tf2tools_core::{DataDir, Database, LogsCache, LogsRange, SteamId, window_start};

use crate::common;
use crate::common::logstf::ARIE;

const DAY: i64 = 86_400;
/// A fixed newest-log date, so window edges never depend on the clock.
const BASE: i64 = 1_790_000_000;

fn arie() -> SteamId {
    SteamId::from_id64(ARIE).unwrap()
}

/// `count` downloaded logs for Arie, one a day back from BASE, one scout minute each.
fn seed_daily(dir: &DataDir, count: i64) {
    let steam = i64::try_from(ARIE).unwrap();
    let mut sql = String::new();
    for d in 0..count {
        sql.push_str(&format!(
            "INSERT INTO logstf_logs (id, title, map, date, body, state)
                 VALUES ({id}, 't', 'm', {date}, '{{}}', 0);
             INSERT INTO logstf_player_logs (steam_id, log_id) VALUES ({steam}, {id});
             INSERT INTO logstf_class_rows (log_id, steam_id, class, seconds, kills, deaths, assists, dmg)
                 VALUES ({id}, {steam}, 'scout', 60, 1, 0, 0, 0);",
            id = d + 1,
            date = BASE - d * DAY,
        ));
    }
    common::sql(dir).execute_batch(&sql).unwrap();
}

#[test]
fn range_keys_round_trip_and_reach_back_the_stated_days() {
    let keys: Vec<&str> = LogsRange::STOPS.iter().map(|r| r.key()).collect();
    assert_eq!(keys, ["1m", "3m", "6m", "1y", "2y", "all"]);
    for range in LogsRange::STOPS {
        assert_eq!(LogsRange::from_key(range.key()), Some(range));
    }
    assert_eq!(LogsRange::from_key("5y"), None);
    let days: Vec<Option<i64>> = LogsRange::STOPS
        .iter()
        .map(|r| r.reach_secs().map(|s| s / DAY))
        .collect();
    assert_eq!(
        days,
        [Some(30), Some(90), Some(180), Some(365), Some(730), None]
    );
    assert_eq!(LogsRange::default(), LogsRange::Months3);
    assert!(
        LogsRange::STOPS.windows(2).all(|w| w[0] < w[1]),
        "narrow to wide"
    );
}

#[test]
fn window_start_counts_back_from_the_anchor() {
    assert_eq!(window_start(BASE, LogsRange::Month1), Some(BASE - 30 * DAY));
    assert_eq!(
        window_start(BASE, LogsRange::Years2),
        Some(BASE - 730 * DAY)
    );
    assert_eq!(window_start(BASE, LogsRange::All), None);
}

#[test]
fn the_range_setting_defaults_to_three_months() {
    let (_tmp, dir) = common::temp_data_dir();
    let cache = LogsCache::new(Database::open(&dir).unwrap());
    assert_eq!(cache.range().unwrap(), LogsRange::Months3);
    cache.set_range(LogsRange::All).unwrap();
    assert_eq!(cache.range().unwrap(), LogsRange::All);
    assert_eq!(
        common::setting(&dir, "logstf_range").as_deref(),
        Some("all")
    );
    common::sql(&dir)
        .execute(
            "UPDATE settings SET value = 'forever' WHERE key = 'logstf_range'",
            [],
        )
        .unwrap();
    assert_eq!(cache.range().unwrap(), LogsRange::Months3);
}

#[test]
fn every_range_selects_its_own_window_with_no_cap() {
    let (_tmp, dir) = common::temp_data_dir();
    let cache = LogsCache::new(Database::open(&dir).unwrap());
    seed_daily(&dir, 500);
    // Day 0 is the anchor, so a 30-day reach holds days 0..=30.
    let expected = [31usize, 91, 181, 366, 500, 500];
    for (range, n) in LogsRange::STOPS.into_iter().zip(expected) {
        assert_eq!(cache.logs(arie(), range).unwrap().len(), n, "{range:?}");
        let avg = cache.class_averages(arie(), range).unwrap();
        assert_eq!(
            (avg[0].logs() as usize, avg[0].seconds()),
            (n, 60 * n as u64),
            "{range:?}"
        );
    }
}
