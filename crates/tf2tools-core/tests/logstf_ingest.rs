//! What a first sync writes: order, progress, every player's class rows, medic totals.

use std::sync::Mutex;

use tf2tools_core::{LogsCancel, LogsProgress, LogsRange, SteamId, TrackedPlayer};

use crate::common;
use crate::common::logstf::{
    ARIE, LogsEnv, PlayerSpec, class_rows, days_ago, list_body, log_body, log_state, logs_env,
    logstf_fixture, mount_list, mount_log, paths, solo,
};

fn arie() -> SteamId {
    SteamId::from_id64(ARIE).unwrap()
}

async fn tracked_env() -> LogsEnv {
    let env = logs_env().await;
    env.client
        .cache()
        .add_player(&TrackedPlayer::new(arie(), "Arie"))
        .unwrap();
    env
}

#[tokio::test]
async fn first_sync_downloads_newest_first_with_progress_and_every_players_rows() {
    let env = tracked_env().await;
    mount_list(
        &env.server,
        ARIE,
        list_body(&[(3, days_ago(1)), (2, days_ago(2)), (1, days_ago(3))]),
    )
    .await;
    mount_log(&env.server, 3, 200, logstf_fixture("log-4137103-6v6.json")).await;
    let small = log_body(600, &[solo(231_702, ("scout", 600, 1, 1, 1, 100))]);
    mount_log(&env.server, 2, 200, small.clone()).await;
    mount_log(&env.server, 1, 200, small).await;

    let seen = Mutex::new(Vec::new());
    let summary = env
        .client
        .sync_player(
            arie(),
            LogsRange::Months3,
            false,
            &LogsCancel::new(),
            &|p| {
                if let LogsProgress::Downloading { done, total, .. } = p {
                    seen.lock().unwrap().push((done, total));
                }
            },
        )
        .await
        .unwrap();
    assert_eq!(
        paths(&env.server).await,
        ["/api/v1/log", "/json/3", "/json/2", "/json/1"]
    );
    assert_eq!(*seen.lock().unwrap(), [(0, 3), (1, 3), (2, 3), (3, 3)]);
    assert_eq!((summary.listed(), summary.downloaded()), (3, 3));
    assert_eq!(log_state(&env.dir, 3), (0, true));
    assert_eq!(
        class_rows(&env.dir, 3),
        29,
        "every player's classes in the 6v6 log"
    );
    assert!(
        env.client.cache().players().unwrap()[0]
            .last_synced()
            .is_some()
    );
}

#[tokio::test]
async fn medic_heals_go_on_the_medic_row_only_and_unknown_classes_are_dropped() {
    let env = tracked_env().await;
    mount_list(&env.server, ARIE, list_body(&[(30, days_ago(1))])).await;
    let medic = PlayerSpec {
        account: 231_702,
        name: "Arie",
        team: "Blue",
        heal: 5000,
        ubers: 3,
        drops: 1,
        classes: vec![
            ("medic", 500, 0, 1, 4, 80),
            ("scout", 100, 2, 1, 0, 300),
            ("undefined", 20, 0, 0, 0, 0),
        ],
    };
    mount_log(&env.server, 30, 200, log_body(620, &[medic])).await;
    env.client
        .sync_player(
            arie(),
            LogsRange::Months3,
            false,
            &LogsCancel::new(),
            &|_| {},
        )
        .await
        .unwrap();
    let rows: Vec<(String, i64, i64, i64)> = common::sql(&env.dir)
        .prepare("SELECT class, heal, ubers, drops FROM logstf_class_rows WHERE log_id = 30 ORDER BY class")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        rows,
        [
            ("medic".to_owned(), 5000, 3, 1),
            ("scout".to_owned(), 0, 0, 0)
        ]
    );
}

/// (class, seconds, kills, dmg, heal) of every class row of log `id`, by class.
fn rows_of(env: &LogsEnv, id: i64) -> Vec<(String, i64, i64, i64, i64)> {
    common::sql(&env.dir)
        .prepare(
            "SELECT class, seconds, kills, dmg, heal FROM logstf_class_rows
             WHERE log_id = ?1 ORDER BY class",
        )
        .unwrap()
        .query_map([id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[tokio::test]
async fn a_class_listed_twice_for_one_player_folds_into_one_row() {
    let env = tracked_env().await;
    mount_list(&env.server, ARIE, list_body(&[(40, days_ago(1))])).await;
    let mut twice = solo(231_702, ("scout", 300, 1, 0, 0, 100));
    twice.classes.push(("scout", 200, 2, 1, 0, 50));
    mount_log(&env.server, 40, 200, log_body(500, &[twice])).await;

    env.client
        .sync_player(
            arie(),
            LogsRange::Months3,
            false,
            &LogsCancel::new(),
            &|_| {},
        )
        .await
        .unwrap();
    assert_eq!(log_state(&env.dir, 40), (0, true));
    assert_eq!(rows_of(&env, 40), [("scout".to_owned(), 500, 3, 150, 0)]);
}

#[tokio::test]
async fn two_keys_for_one_steam_id_fold_into_one_row_per_class() {
    let env = tracked_env().await;
    mount_list(&env.server, ARIE, list_body(&[(41, days_ago(1))])).await;
    let medic = |account, heal| PlayerSpec {
        account,
        name: "Arie",
        team: "Red",
        heal,
        ubers: 1,
        drops: 0,
        classes: vec![("medic", 300, 0, 1, 2, 40)],
    };
    // `[U:1:0231702]` names the same account as `[U:1:231702]`.
    let body = log_body(600, &[medic(231_702, 1000), medic(999, 500)])
        .replace("[U:1:999]", "[U:1:0231702]");
    mount_log(&env.server, 41, 200, body).await;

    env.client
        .sync_player(
            arie(),
            LogsRange::Months3,
            false,
            &LogsCancel::new(),
            &|_| {},
        )
        .await
        .unwrap();
    assert_eq!(log_state(&env.dir, 41), (0, true));
    assert_eq!(rows_of(&env, 41), [("medic".to_owned(), 600, 0, 80, 1500)]);
}

#[tokio::test]
async fn a_body_that_is_not_utf8_is_unreadable_and_not_stored() {
    let env = tracked_env().await;
    mount_list(&env.server, ARIE, list_body(&[(42, days_ago(1))])).await;
    let mut named = solo(231_702, ("scout", 600, 1, 1, 1, 100));
    named.name = "BADNAME";
    let body = log_body(600, &[named]).replace("BADNAME", "\u{1}");
    let bytes: Vec<u8> = body
        .bytes()
        .map(|b| if b == 1 { 0xff } else { b })
        .collect();
    wiremock::Mock::given(wiremock::matchers::path("/json/42"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_bytes(bytes))
        .mount(&env.server)
        .await;

    let summary = env
        .client
        .sync_player(
            arie(),
            LogsRange::Months3,
            false,
            &LogsCancel::new(),
            &|_| {},
        )
        .await
        .unwrap();
    assert_eq!(summary.unreadable(), 1);
    assert_eq!(log_state(&env.dir, 42), (2, false));
}
