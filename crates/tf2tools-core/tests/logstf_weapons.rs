//! Weapon rows: written at ingest, summed per class over the window, and counted for logs
//! stored before they existed.

use std::collections::HashMap;

use tf2tools_core::{
    ClassWeapons, LogsCacheError, LogsCancel, LogsRange, SteamId, TfClass, TrackedPlayer,
    logstf_class, parse_log,
};

use crate::common;
use crate::common::logstf::{
    ARIE, LogsEnv, days_ago, list_body, log_state, logs_env, logstf_fixture, mount_list, mount_log,
};

fn arie() -> SteamId {
    SteamId::from_id64(ARIE).unwrap()
}

/// (steam id, class, weapon) to [kills, dmg, shots, hits].
type WeaponMap = HashMap<(i64, TfClass, String), [i64; 4]>;

fn stored_weapons(env: &LogsEnv, log_id: i64) -> WeaponMap {
    let conn = common::sql(&env.dir);
    let mut stmt = conn
        .prepare(
            "SELECT steam_id, class, weapon, kills, dmg, shots, hits
             FROM logstf_weapon_rows WHERE log_id = ?1",
        )
        .unwrap();
    stmt.query_map([log_id], |r| {
        Ok((
            (
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ),
            [r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?],
        ))
    })
    .unwrap()
    .map(|row| {
        let ((id, class, weapon), stats) = row.unwrap();
        (
            (id, logstf_class(&class).expect("a logs.tf class"), weapon),
            stats,
        )
    })
    .collect()
}

fn weapons_counted(env: &LogsEnv, log_id: i64) -> i64 {
    common::sql(&env.dir)
        .query_row(
            "SELECT weapons_counted FROM logstf_logs WHERE id = ?1",
            [log_id],
            |r| r.get(0),
        )
        .unwrap()
}

/// The highlander fixture with one scout's key turned into Arie's, so Arie has a second log.
fn highlander_as_arie() -> String {
    logstf_fixture("log-4150405-hl.json").replace("U:1:187445387", "U:1:231702")
}

/// Arie tracked with log 3 (the 6v6 fixture, a day old) and log 2 (`highlander_as_arie`,
/// 200 days old), both downloaded.
async fn two_logs() -> LogsEnv {
    let env = logs_env().await;
    env.client
        .cache()
        .add_player(&TrackedPlayer::new(arie(), "Arie"))
        .unwrap();
    mount_list(
        &env.server,
        ARIE,
        list_body(&[(3, days_ago(1)), (2, days_ago(200))]),
    )
    .await;
    mount_log(&env.server, 3, 200, logstf_fixture("log-4137103-6v6.json")).await;
    mount_log(&env.server, 2, 200, highlander_as_arie()).await;
    env.client
        .sync_player(arie(), LogsRange::All, false, &LogsCancel::new(), &|_| {})
        .await
        .unwrap();
    env
}

/// Every weapon line of `body`, keyed and summed as the cache stores them.
fn expected(body: &str) -> WeaponMap {
    let detail = parse_log(body).unwrap();
    let mut want = WeaponMap::new();
    for p in detail.players() {
        let id = i64::try_from(p.steam_id().id64()).unwrap();
        for c in p.classes() {
            for w in c.weapons() {
                let row = want
                    .entry((id, c.class(), w.name().to_owned()))
                    .or_default();
                for (sum, n) in row
                    .iter_mut()
                    .zip([w.kills(), w.dmg(), w.shots(), w.hits()])
                {
                    *sum += i64::from(n);
                }
            }
        }
    }
    want
}

#[tokio::test]
async fn ingest_writes_a_row_per_player_class_and_weapon() {
    let env = two_logs().await;
    let sixes = expected(&logstf_fixture("log-4137103-6v6.json"));
    assert!(!sixes.is_empty());
    assert_eq!(stored_weapons(&env, 3), sixes);
    assert_eq!(stored_weapons(&env, 2), expected(&highlander_as_arie()));
    let arie_id = i64::try_from(ARIE).unwrap();
    assert_eq!(
        sixes[&(arie_id, TfClass::Scout, "scattergun".to_owned())],
        [7, 3334, 290, 161]
    );
    assert_eq!(
        sixes[&(arie_id, TfClass::Heavy, "long_heatmaker".to_owned())],
        [0, 283, 0, 0]
    );
    assert_eq!((weapons_counted(&env, 3), weapons_counted(&env, 2)), (1, 1));
}

fn table(w: &ClassWeapons) -> Vec<(&str, i64, i64, i64, i64)> {
    w.weapons()
        .iter()
        .map(|s| (s.name(), s.kills(), s.dmg(), s.shots(), s.hits()))
        .collect()
}

#[tokio::test]
async fn a_class_sums_its_weapons_over_the_range_by_damage() {
    let env = two_logs().await;
    let cache = env.client.cache();
    let year = cache
        .class_weapons(arie(), LogsRange::Year1, TfClass::Scout)
        .unwrap();
    assert_eq!((year.logs(), year.pending()), (2, 0));
    assert_eq!(
        table(&year),
        [
            ("scattergun", 24, 13_340, 949, 579),
            ("pistol_scout", 3, 1_090, 547, 93),
            ("guillotine", 0, 382, 0, 0),
            ("wrap_assassin", 0, 153, 0, 0),
        ]
    );
    // Three months back from the newest log holds log 3 only, as the averages do.
    let months = cache
        .class_weapons(arie(), LogsRange::Months3, TfClass::Scout)
        .unwrap();
    assert_eq!(
        table(&months),
        [
            ("scattergun", 7, 3_334, 290, 161),
            ("pistol_scout", 0, 177, 201, 21)
        ]
    );
    let averages = cache.class_averages(arie(), LogsRange::Months3).unwrap();
    assert_eq!(months.logs(), averages[0].logs());
}

#[tokio::test]
async fn accuracy_is_hits_over_shots_and_none_without_shots() {
    let env = two_logs().await;
    let cache = env.client.cache();
    let pyro = cache
        .class_weapons(arie(), LogsRange::Year1, TfClass::Pyro)
        .unwrap();
    assert_eq!(
        table(&pyro),
        [
            ("shotgun_pyro", 0, 56, 13, 7),
            ("flamethrower", 0, 46, 0, 0)
        ]
    );
    assert_eq!(pyro.weapons()[0].accuracy(), Some(7.0 / 13.0));
    assert_eq!(pyro.weapons()[1].accuracy(), None);
    let medic = cache
        .class_weapons(arie(), LogsRange::Year1, TfClass::Medic)
        .unwrap();
    assert_eq!(
        (medic.logs(), medic.pending(), medic.weapons().len()),
        (0, 0, 0)
    );
}

#[tokio::test]
async fn equal_damage_sorts_by_kills_then_name() {
    let env = logs_env().await;
    let steam = i64::try_from(ARIE).unwrap();
    common::sql(&env.dir)
        .execute_batch(&format!(
            "INSERT INTO logstf_logs (id, title, map, date, body, state, weapons_counted)
                 VALUES (9, 'Log 9', 'm', {}, x'00', 0, 1);
             INSERT INTO logstf_player_logs (steam_id, log_id) VALUES ({steam}, 9);
             INSERT INTO logstf_weapon_rows (log_id, steam_id, class, weapon, kills, dmg, shots, hits)
                 VALUES (9, {steam}, 'soldier', 'a_gun', 1, 100, 0, 0),
                        (9, {steam}, 'soldier', 'c_gun', 2, 100, 0, 0),
                        (9, {steam}, 'soldier', 'b_gun', 2, 100, 0, 0),
                        (9, {steam}, 'soldier', 'd_gun', 0, 300, 0, 0);",
            days_ago(1)
        ))
        .unwrap();
    let soldier = env
        .client
        .cache()
        .class_weapons(arie(), LogsRange::All, TfClass::Soldier)
        .unwrap();
    let names: Vec<&str> = soldier.weapons().iter().map(|w| w.name()).collect();
    assert_eq!(names, ["d_gun", "b_gun", "c_gun", "a_gun"]);
    assert_eq!(soldier.logs(), 1);
}

#[tokio::test]
async fn logs_not_yet_counted_show_as_pending() {
    let env = two_logs().await;
    common::sql(&env.dir)
        .execute_batch(
            "DELETE FROM logstf_weapon_rows WHERE log_id = 3;
             UPDATE logstf_logs SET weapons_counted = 0 WHERE id = 3;",
        )
        .unwrap();
    let cache = env.client.cache();
    let scout = cache
        .class_weapons(arie(), LogsRange::Year1, TfClass::Scout)
        .unwrap();
    assert_eq!((scout.logs(), scout.pending()), (1, 1));
    assert_eq!(table(&scout)[0], ("scattergun", 17, 10_006, 659, 418));
    let pyro = cache
        .class_weapons(arie(), LogsRange::Year1, TfClass::Pyro)
        .unwrap();
    assert_eq!(
        (pyro.logs(), pyro.pending(), pyro.weapons().len()),
        (0, 1, 0)
    );
}

/// Clears every weapon row and marker, as a database from before step 16 had them.
fn reset_weapons(env: &LogsEnv) {
    common::sql(&env.dir)
        .execute_batch(
            "DELETE FROM logstf_weapon_rows; UPDATE logstf_logs SET weapons_counted = 0;",
        )
        .unwrap();
}

#[tokio::test]
async fn the_backfill_counts_bodies_stored_without_weapon_rows() {
    let env = two_logs().await;
    let (three, two) = (stored_weapons(&env, 3), stored_weapons(&env, 2));
    reset_weapons(&env);
    let cache = env.client.cache();
    let before = cache
        .class_weapons(arie(), LogsRange::Year1, TfClass::Scout)
        .unwrap();
    assert_eq!((before.logs(), before.pending()), (0, 2));

    let done = cache.count_weapons(50).unwrap();
    assert_eq!((done.counted(), done.left()), (2, 0));
    assert_eq!(stored_weapons(&env, 3), three);
    assert_eq!(stored_weapons(&env, 2), two);
    assert_eq!((weapons_counted(&env, 3), weapons_counted(&env, 2)), (1, 1));
    let again = cache.count_weapons(50).unwrap();
    assert_eq!((again.counted(), again.left()), (0, 0));
}

#[tokio::test]
async fn an_interrupted_backfill_resumes_oldest_first() {
    let env = two_logs().await;
    reset_weapons(&env);
    let cache = env.client.cache();
    let first = cache.count_weapons(1).unwrap();
    assert_eq!((first.counted(), first.left()), (1, 1));
    assert_eq!((weapons_counted(&env, 2), weapons_counted(&env, 3)), (1, 0));
    let partial = cache
        .class_weapons(arie(), LogsRange::Year1, TfClass::Scout)
        .unwrap();
    assert_eq!((partial.logs(), partial.pending()), (1, 1));
    let rest = cache.count_weapons(1).unwrap();
    assert_eq!((rest.counted(), rest.left()), (1, 0));
}

#[tokio::test]
async fn a_damaged_body_is_counted_with_no_rows() {
    let env = logs_env().await;
    let steam = i64::try_from(ARIE).unwrap();
    common::sql(&env.dir)
        .execute_batch(&format!(
            "INSERT INTO logstf_logs (id, title, map, date, body, state) VALUES
                 (11, 'not zlib', 'm', 1, x'00010203', 0),
                 (12, 'not utf-8', 'm', 1, CAST(x'fffe' AS TEXT), 0),
                 (13, 'a number', 'm', 1, 7, 0),
                 (14, 'not a log', 'm', 1, '{{}}', 0);
             INSERT INTO logstf_class_rows (log_id, steam_id, class, seconds, kills, deaths, assists, dmg)
                 VALUES (14, {steam}, 'scout', 60, 0, 0, 0, 0);"
        ))
        .unwrap();
    let done = env.client.cache().count_weapons(50).unwrap();
    assert_eq!((done.counted(), done.left()), (4, 0));
    for id in 11..=14 {
        assert_eq!(weapons_counted(&env, id), 1, "log {id}");
    }
    assert_eq!(common::count(&env.dir, "logstf_weapon_rows"), 0);
    assert_eq!(
        common::count(&env.dir, "logstf_class_rows"),
        1,
        "class rows stay"
    );
    assert_eq!(log_state(&env.dir, 14).0, 0, "the state stays");
}

#[tokio::test]
async fn a_failed_write_leaves_the_log_for_the_next_run() {
    let env = two_logs().await;
    reset_weapons(&env);
    common::sql(&env.dir)
        .execute_batch(
            "CREATE TRIGGER no_mark BEFORE UPDATE OF weapons_counted ON logstf_logs
             BEGIN SELECT RAISE(ABORT, 'disk full'); END;",
        )
        .unwrap();
    let cache = env.client.cache();
    assert!(matches!(
        cache.count_weapons(50),
        Err(LogsCacheError::Database { .. })
    ));
    assert_eq!((weapons_counted(&env, 2), weapons_counted(&env, 3)), (0, 0));
    assert_eq!(common::count(&env.dir, "logstf_weapon_rows"), 0);
    common::sql(&env.dir)
        .execute_batch("DROP TRIGGER no_mark;")
        .unwrap();
    assert_eq!(cache.count_weapons(50).unwrap().left(), 0);
}
