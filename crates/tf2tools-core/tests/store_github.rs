//! The curated HUD store from hud-db's archive and the addon store from mastercomfig's
//! latest GitHub release.

mod common;

use std::sync::Mutex;
use std::time::{Duration, Instant};

use common::{
    StoreEnv, assert_store_timestamp, count, hours_ago, meta, set_meta, sql, store_env,
    store_fixture, tmp_is_empty, zip_bytes,
};
use tf2tools_core::{StoreKind, SyncError, SyncProgress};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

const HUD_DB: &str = "/gh/mastercomfig/hud-db/archive/refs/heads/main.zip";
const RELEASE: &str = "/gh-api/repos/mastercomfig/mastercomfig/releases/latest";

async fn serve(env: &StoreEnv, at: &str, status: u16, body: Vec<u8>) {
    Mock::given(method("GET"))
        .and(path(at))
        .respond_with(ResponseTemplate::new(status).set_body_bytes(body))
        .mount(&env.server)
        .await;
}

fn s(text: &str) -> String {
    text.to_owned()
}

type CuratedRow = (String, String, String, Option<String>, Option<String>);

fn curated(env: &StoreEnv) -> Vec<CuratedRow> {
    let conn = sql(&env.dir);
    let mut stmt = conn
        .prepare("SELECT hud_id, name, author, repo, hash FROM store_huds ORDER BY hud_id")
        .unwrap();
    stmt.query_map([], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
    })
    .unwrap()
    .collect::<Result<_, _>>()
    .unwrap()
}

fn seed_gone_hud(env: &StoreEnv) {
    sql(&env.dir)
        .execute(
            "INSERT INTO store_huds (hud_id, name, author, data_json, fetched_at)
             VALUES ('gone', 'gone', 'x', '{}', ?1)",
            [hours_ago(1)],
        )
        .unwrap();
}

#[tokio::test]
async fn curated_sync_loads_hud_db_and_drops_removed_huds() {
    let env = store_env().await;
    seed_gone_hud(&env);
    serve(&env, HUD_DB, 200, store_fixture("hud-db-main.zip")).await;
    let seen = Mutex::new(Vec::new());
    let outcome = env
        .client
        .sync(StoreKind::CuratedHuds, false, &|p: SyncProgress| {
            seen.lock().unwrap().push((p.fetched(), p.total()))
        })
        .await
        .unwrap();
    assert_eq!((outcome.fetched(), outcome.rows()), (2, 2));
    assert_eq!(seen.into_inner().unwrap(), [(2, 2)]);
    assert_eq!(
        curated(&env),
        [
            (
                s("budhud"),
                s("budhud"),
                s("Whisker"),
                Some(s("https://github.com/rbjaxter/budhud")),
                Some(s("16f7be2bb4f0dcbe46ec5f444486ab6a6ea4c7e6")),
            ),
            (
                s("rayshud"),
                s("rayshud"),
                s("raysfire"),
                Some(s("https://github.com/raysfire/rayshud")),
                Some(s("a22ede014e4f3b1260aa8fd657ff0757d9d006b8")),
            ),
        ]
    );
    let (data_json, fetched_at): (String, String) = sql(&env.dir)
        .query_row(
            "SELECT data_json, fetched_at FROM store_huds WHERE hud_id = 'rayshud'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert!(data_json.contains("\"contributors\""), "{data_json}");
    assert_store_timestamp(&fetched_at);
    assert_store_timestamp(&meta(&env.dir, "curated_last_synced").unwrap());
    assert!(tmp_is_empty(&env.dir));
}

#[tokio::test]
async fn curated_sync_without_hud_data_fails_and_keeps_the_store() {
    let env = store_env().await;
    seed_gone_hud(&env);
    serve(
        &env,
        HUD_DB,
        200,
        zip_bytes(&[("hud-db-main/README.md", b"moved".as_slice())]),
    )
    .await;
    let result = env
        .client
        .sync(StoreKind::CuratedHuds, false, &|_| {})
        .await;
    assert!(
        matches!(result, Err(SyncError::Malformed { .. })),
        "{result:?}"
    );
    assert_eq!(env.server.received_requests().await.unwrap().len(), 1);
    assert_eq!(count(&env.dir, "store_huds"), 1);
    assert_eq!(meta(&env.dir, "curated_last_synced"), None);
    assert!(tmp_is_empty(&env.dir));
}

#[tokio::test]
async fn hud_files_without_name_or_author_fall_back_and_broken_ones_are_skipped() {
    let env = store_env().await;
    let archive = zip_bytes(&[
        ("hud-db-main/README.md", b"".as_slice()),
        ("hud-db-main/hud-data/plain.json", b"{}".as_slice()),
        ("hud-db-main/hud-data/broken.json", b"{".as_slice()),
        ("hud-db-main/hud-data/notes.txt", b"x".as_slice()),
    ]);
    serve(&env, HUD_DB, 200, archive).await;
    let outcome = env
        .client
        .sync(StoreKind::CuratedHuds, false, &|_| {})
        .await
        .unwrap();
    assert_eq!(outcome.rows(), 1);
    assert_eq!(
        curated(&env),
        [(s("plain"), s("plain"), s("Unknown"), None, None)]
    );
}

#[tokio::test]
async fn a_fresh_curated_store_is_skipped() {
    let env = store_env().await;
    set_meta(&env.dir, "curated_last_synced", &hours_ago(1));
    let outcome = env
        .client
        .sync(StoreKind::CuratedHuds, false, &|_| {})
        .await
        .unwrap();
    assert!(outcome.is_fresh());
    assert!(env.server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn addon_sync_stores_the_latest_releases_addon_vpks() {
    let env = store_env().await;
    sql(&env.dir)
        .execute(
            "INSERT INTO store_addons (asset_name, display_name, download_url, release_tag, fetched_at)
             VALUES ('mastercomfig-addon-old.vpk', 'Old', 'u', '9.0.0', ?1)",
            [hours_ago(1)],
        )
        .unwrap();
    serve(
        &env,
        RELEASE,
        200,
        store_fixture("github-mastercomfig-latest.json"),
    )
    .await;
    let outcome = env
        .client
        .sync(StoreKind::Addons, false, &|_| {})
        .await
        .unwrap();
    assert_eq!((outcome.fetched(), outcome.rows()), (2, 2));
    let conn = sql(&env.dir);
    let mut stmt = conn
        .prepare(
            "SELECT asset_name, display_name, description, download_url, file_size,
                    download_count, release_tag FROM store_addons ORDER BY asset_name",
        )
        .unwrap();
    let rows: Vec<(String, String, String, String, i64, i64, String)> = stmt
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
            ))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let release = "https://github.com/mastercomfig/mastercomfig/releases/download/9.100.1";
    assert_eq!(
        rows,
        [
            (
                s("mastercomfig-addon-flat-mouse.vpk"),
                s("Flat Mouse"),
                s(
                    "Ensures consistent raw mouse input with no OS acceleration and 1:1 zoom sensitivity"
                ),
                format!("{release}/mastercomfig-addon-flat-mouse.vpk"),
                299,
                40731,
                s("9.100.1"),
            ),
            (
                s("mastercomfig-addon-lowmem.vpk"),
                s("Low Memory"),
                s(
                    "Reduces memory usage for systems with 2GB RAM or less. May increase disk loading times"
                ),
                format!("{release}/mastercomfig-addon-lowmem.vpk"),
                318,
                4940,
                s("9.100.1"),
            ),
        ]
    );
    assert_store_timestamp(&meta(&env.dir, "addons_last_synced").unwrap());
}

#[tokio::test]
async fn a_github_403_is_rate_limited_and_keeps_the_store() {
    let env = store_env().await;
    sql(&env.dir)
        .execute(
            "INSERT INTO store_addons (asset_name, display_name, download_url, release_tag, fetched_at)
             VALUES ('mastercomfig-addon-old.vpk', 'Old', 'u', '9.0.0', ?1)",
            [hours_ago(1)],
        )
        .unwrap();
    serve(&env, RELEASE, 403, b"{}".to_vec()).await;
    let result = env.client.sync(StoreKind::Addons, false, &|_| {}).await;
    assert!(
        matches!(result, Err(SyncError::RateLimited { .. })),
        "{result:?}"
    );
    assert_eq!(count(&env.dir, "store_addons"), 1);
    assert_eq!(meta(&env.dir, "addons_last_synced"), None);
}

#[tokio::test]
async fn curated_and_addon_syncs_take_turns() {
    let env = store_env().await;
    for (at, body) in [
        (HUD_DB, store_fixture("hud-db-main.zip")),
        (RELEASE, store_fixture("github-mastercomfig-latest.json")),
    ] {
        Mock::given(method("GET"))
            .and(path(at))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(body)
                    .set_delay(Duration::from_millis(300)),
            )
            .mount(&env.server)
            .await;
    }
    let start = Instant::now();
    let (curated, addons) = tokio::join!(
        env.client.sync(StoreKind::CuratedHuds, false, &|_| {}),
        env.client.sync(StoreKind::Addons, false, &|_| {}),
    );
    curated.unwrap();
    addons.unwrap();
    assert!(
        start.elapsed() >= Duration::from_millis(600),
        "{:?}",
        start.elapsed()
    );
}
