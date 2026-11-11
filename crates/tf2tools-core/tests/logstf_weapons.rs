//! Weapon rows: written at ingest, summed per class over the window, and counted for logs
//! stored before they existed.

use std::collections::HashMap;

use tf2tools_core::{
    LogsCancel, LogsRange, SteamId, TfClass, TrackedPlayer, logstf_class, parse_log,
};

use crate::common;
use crate::common::logstf::{
    ARIE, LogsEnv, days_ago, list_body, logs_env, logstf_fixture, mount_list, mount_log,
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
