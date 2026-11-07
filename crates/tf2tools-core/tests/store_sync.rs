//! GameBanana sync: pages, checkpoints, resume, stale-row cleanup and the page cap.

use crate::common;

use std::sync::Mutex;

use common::{
    StoreEnv, assert_store_timestamp, count, fast_config, hours_ago, meta, set_meta, set_setting,
    sql, store_env, store_fixture,
};
use tf2tools_core::{StoreKind, SyncError, SyncOutcome, SyncProgress};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn mount_page(server: &MockServer, model: &str, page: u32, body: Vec<u8>) {
    Mock::given(method("GET"))
        .and(path(format!("/apiv11/{model}/Index")))
        .and(query_param("_nPage", page.to_string()))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
        .mount(server)
        .await;
}

/// Page 1 holds 3 sounds and is incomplete; page 2 holds 2 and is the last.
async fn mount_sound_catalogue(server: &MockServer) {
    mount_page(server, "Sound", 1, store_fixture("gb-sounds-page1.json")).await;
    mount_page(server, "Sound", 2, store_fixture("gb-sounds-last.json")).await;
}

fn empty_page(total: u32) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "_aMetadata": {"_nRecordCount": total, "_bIsComplete": false, "_nPerpage": 15},
        "_aRecords": []
    }))
    .unwrap()
}

async fn requested_pages(server: &MockServer) -> Vec<String> {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter_map(|r| {
            r.url
                .query_pairs()
                .find(|(k, _)| k == "_nPage")
                .map(|(_, v)| v.into_owned())
        })
        .collect()
}

async fn sync_sounds(
    env: &StoreEnv,
    force: bool,
) -> (Result<SyncOutcome, SyncError>, Vec<(u32, u32)>) {
    let seen = Mutex::new(Vec::new());
    let result = env
        .client
        .sync(StoreKind::Sounds, force, &|p: SyncProgress| {
            seen.lock().unwrap().push((p.fetched(), p.total()))
        })
        .await;
    (result, seen.into_inner().unwrap())
}

fn names(env: &StoreEnv, table: &str) -> Vec<String> {
    let conn = sql(&env.dir);
    let mut stmt = conn
        .prepare(&format!("SELECT name FROM {table} ORDER BY gb_id"))
        .unwrap();
    stmt.query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[tokio::test]
async fn a_sound_sync_walks_every_page_and_stores_rows() {
    let env = store_env().await;
    mount_sound_catalogue(&env.server).await;
    let (result, seen) = sync_sounds(&env, false).await;
    let outcome = result.unwrap();
    assert!(!outcome.is_fresh());
    assert_eq!((outcome.fetched(), outcome.rows()), (5, 5));
    assert_eq!(seen, [(3, 19718), (5, 19718)]);
    assert_eq!(requested_pages(&env.server).await, ["1", "2"]);

    let row = sql(&env.dir)
        .query_row(
            "SELECT name, author, category, preview_url, thumbnail_url, like_count, view_count,
                    tags_json, profile_url, date_added FROM store_sounds WHERE gb_id = 53978",
            [],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, i64>(6)?,
                    r.get::<_, String>(7)?,
                    r.get::<_, String>(8)?,
                    r.get::<_, i64>(9)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        row,
        (
            "Musical Events".to_owned(),
            "ball_hurt".to_owned(),
            "sound".to_owned(),
            Some("https://files.gamebanana.com/preview/sounds/something_59646.mp3".to_owned()),
            Some("https://images.gamebanana.com/img/av/64b51efd2cd1a.png".to_owned()),
            130,
            62465,
            r#"["Purity: Fun","Bitrate: 320 kbit/s"]"#.to_owned(),
            "https://gamebanana.com/sounds/53978".to_owned(),
            1623684181,
        )
    );
    // A "Hitsound" root category maps to v5's label; a null like count reads as 0.
    let hit: (String, i64) = sql(&env.dir)
        .query_row(
            "SELECT category, like_count FROM store_sounds WHERE gb_id = 93300",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(hit, ("hitsound".to_owned(), 0));
    let fetched_at: String = sql(&env.dir)
        .query_row(
            "SELECT fetched_at FROM store_sounds WHERE gb_id = 9873",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_store_timestamp(&fetched_at);
    assert_store_timestamp(&meta(&env.dir, "sounds_last_synced").unwrap());
    for key in [
        "sounds_sync_page",
        "sounds_sync_start",
        "sounds_sync_fetched",
    ] {
        assert_eq!(meta(&env.dir, key), None, "{key}");
    }
}

#[tokio::test]
async fn a_fresh_store_is_skipped_without_a_request() {
    let env = store_env().await;
    set_meta(&env.dir, "sounds_sync_order", "newest");
    set_meta(&env.dir, "sounds_last_synced", &hours_ago(1));
    let (result, seen) = sync_sounds(&env, false).await;
    assert!(result.unwrap().is_fresh());
    assert!(seen.is_empty());
    assert!(env.server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_store_older_than_sync_cache_hours_syncs_again() {
    let env = store_env().await;
    set_setting(&env.dir, "sync_cache_hours", "1");
    set_meta(&env.dir, "sounds_last_synced", &hours_ago(2));
    mount_sound_catalogue(&env.server).await;
    let (result, _) = sync_sounds(&env, false).await;
    assert_eq!(result.unwrap().fetched(), 5);
}

#[tokio::test]
async fn force_resyncs_a_fresh_store_from_page_one() {
    let env = store_env().await;
    set_meta(&env.dir, "sounds_last_synced", &hours_ago(1));
    set_meta(&env.dir, "sounds_sync_page", "7");
    set_meta(&env.dir, "sounds_sync_start", &hours_ago(1));
    set_meta(&env.dir, "sounds_sync_fetched", "90");
    mount_sound_catalogue(&env.server).await;
    let (result, seen) = sync_sounds(&env, true).await;
    assert_eq!(result.unwrap().fetched(), 5);
    assert_eq!(seen, [(3, 19718), (5, 19718)]);
    assert_eq!(requested_pages(&env.server).await, ["1", "2"]);
}

#[tokio::test]
async fn an_interrupted_sync_resumes_from_its_checkpoint_and_drops_stale_rows() {
    let env = store_env().await;
    set_meta(&env.dir, "sounds_sync_order", "newest");
    set_meta(&env.dir, "sounds_sync_page", "2");
    set_meta(&env.dir, "sounds_sync_start", &hours_ago(1));
    set_meta(&env.dir, "sounds_sync_fetched", "3");
    sql(&env.dir)
        .execute(
            "INSERT INTO store_sounds (gb_id, name, fetched_at) VALUES (1, 'stale', ?1), (2, 'this session', ?2)",
            [hours_ago(2), hours_ago(0)],
        )
        .unwrap();
    mount_page(
        &env.server,
        "Sound",
        2,
        store_fixture("gb-sounds-last.json"),
    )
    .await;
    let (result, seen) = sync_sounds(&env, false).await;
    let outcome = result.unwrap();
    assert_eq!(requested_pages(&env.server).await, ["2"]);
    assert_eq!(seen, [(5, 19718)]);
    assert_eq!((outcome.fetched(), outcome.rows()), (5, 3));
    assert_eq!(
        names(&env, "store_sounds"),
        [
            "this session",
            "Spiralmouth Ubercharge 002",
            "jeremy kim (sexy mulan man) hitsound and killsound"
        ]
    );
}

#[tokio::test]
async fn a_rate_limited_page_keeps_the_checkpoint_for_the_next_sync() {
    let env = store_env().await;
    mount_page(
        &env.server,
        "Sound",
        1,
        store_fixture("gb-sounds-page1.json"),
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/apiv11/Sound/Index"))
        .and(query_param("_nPage", "2"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&env.server)
        .await;
    let (result, _) = sync_sounds(&env, false).await;
    assert!(
        matches!(result, Err(SyncError::RateLimited { .. })),
        "{result:?}"
    );
    assert_eq!(
        requested_pages(&env.server).await,
        ["1", "2", "2", "2", "2"]
    );
    assert_eq!(meta(&env.dir, "sounds_sync_page").as_deref(), Some("2"));
    assert_eq!(meta(&env.dir, "sounds_sync_fetched").as_deref(), Some("3"));
    assert_store_timestamp(&meta(&env.dir, "sounds_sync_start").unwrap());
    assert_eq!(meta(&env.dir, "sounds_last_synced"), None);
    assert_eq!(count(&env.dir, "store_sounds"), 3);

    env.server.reset().await;
    mount_page(
        &env.server,
        "Sound",
        2,
        store_fixture("gb-sounds-last.json"),
    )
    .await;
    let (result, _) = sync_sounds(&env, false).await;
    assert_eq!(result.unwrap().fetched(), 5);
    assert_eq!(requested_pages(&env.server).await, ["2"]);
    assert_eq!(count(&env.dir, "store_sounds"), 5);
    assert_eq!(meta(&env.dir, "sounds_sync_page"), None);
}

#[tokio::test]
async fn a_malformed_page_fails_without_writing() {
    let env = store_env().await;
    mount_page(&env.server, "Sound", 1, b"<html>busy</html>".to_vec()).await;
    let (result, _) = sync_sounds(&env, false).await;
    assert!(
        matches!(result, Err(SyncError::Malformed { .. })),
        "{result:?}"
    );
    assert_eq!(count(&env.dir, "store_sounds"), 0);
    assert_eq!(meta(&env.dir, "sounds_sync_page"), None);
    assert_eq!(meta(&env.dir, "sounds_last_synced"), None);
}

#[tokio::test]
async fn sync_updates_existing_rows_in_place() {
    let env = store_env().await;
    let conn = sql(&env.dir);
    // An UPDATE trigger fires for ON CONFLICT DO UPDATE but not for INSERT OR REPLACE,
    // which deletes and reinserts (and would break a future FTS5 index).
    conn.execute_batch(
        "CREATE TABLE upsert_log (gb_id INTEGER);
         CREATE TRIGGER log_update AFTER UPDATE ON store_sounds
         BEGIN INSERT INTO upsert_log VALUES (new.gb_id); END;",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO store_sounds (gb_id, name, fetched_at) VALUES (53978, 'Old name', ?1)",
        [hours_ago(1)],
    )
    .unwrap();
    mount_sound_catalogue(&env.server).await;
    sync_sounds(&env, false).await.0.unwrap();
    let name: String = conn
        .query_row(
            "SELECT name FROM store_sounds WHERE gb_id = 53978",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(name, "Musical Events");
    let logged: Vec<i64> = conn
        .prepare("SELECT gb_id FROM upsert_log")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(logged, [53978]);
}

#[tokio::test]
async fn crosshair_and_hud_syncs_read_mod_pages_concurrently() {
    let env = store_env().await;
    for (category, fixture, total) in [
        ("1105", "gb-crosshairs-page1.json", 97),
        ("1649", "gb-huds-page1.json", 800),
    ] {
        for (page, body) in [
            ("1", store_fixture(fixture)),
            ("2", with_meta(fixture, total, true)),
        ] {
            Mock::given(method("GET"))
                .and(path("/apiv11/Mod/Index"))
                .and(query_param("_aFilters[Generic_Category]", category))
                .and(query_param("_nPage", page))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
                .mount(&env.server)
                .await;
        }
    }
    let (crosshairs, huds) = tokio::join!(
        env.client.sync(StoreKind::Crosshairs, false, &|_| {}),
        env.client.sync(StoreKind::GameBananaHuds, false, &|_| {}),
    );
    assert_eq!(crosshairs.unwrap().rows(), 3);
    assert_eq!(huds.unwrap().rows(), 3);

    let crosshair = sql(&env.dir)
        .query_row(
            "SELECT name, author, thumbnail_url, images_json, like_count, tags_json, date_added
             FROM store_crosshairs WHERE gb_id = 456577",
            [],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, i64>(6)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        crosshair,
        (
            "Dyselxic Crosshair - Kunai".to_owned(),
            "Vexcenot".to_owned(),
            Some("https://images.gamebanana.com/img/ss/mods/100-90_66c071a63156d.jpg".to_owned()),
            r#"["https://images.gamebanana.com/img/ss/mods/66c071a63156d.jpg","https://images.gamebanana.com/img/ss/mods/64b53c8423359.jpg","https://images.gamebanana.com/img/ss/mods/64b4bf4cabfe3.jpg"]"#.to_owned(),
            77,
            "[]".to_owned(),
            1689567250,
        )
    );
    let hud: (String, String, i64, String) = sql(&env.dir)
        .query_row(
            "SELECT name, author, like_count, tags_json FROM store_huds_gb WHERE gb_id = 463048",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        hud,
        (
            "DarHUD".to_owned(),
            "DarSitam".to_owned(),
            445,
            r#"["Software Used: VTFEdit"]"#.to_owned()
        )
    );
    assert_store_timestamp(&meta(&env.dir, "crosshairs_last_synced").unwrap());
    assert_store_timestamp(&meta(&env.dir, "huds_gb_last_synced").unwrap());
}

#[tokio::test]
async fn a_dropped_sync_keeps_its_last_committed_page() {
    let env = store_env().await;
    mount_page(
        &env.server,
        "Sound",
        1,
        store_fixture("gb-sounds-page1.json"),
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/apiv11/Sound/Index"))
        .and(query_param("_nPage", "2"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(store_fixture("gb-sounds-last.json"))
                .set_delay(std::time::Duration::from_secs(5)),
        )
        .mount(&env.server)
        .await;
    let attempt = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        env.client.sync(StoreKind::Sounds, false, &|_| {}),
    )
    .await;
    assert!(
        attempt.is_err(),
        "the sync should still be waiting on page 2"
    );
    assert_eq!(meta(&env.dir, "sounds_sync_page").as_deref(), Some("2"));
    assert_eq!(count(&env.dir, "store_sounds"), 3);

    env.server.reset().await;
    mount_page(
        &env.server,
        "Sound",
        2,
        store_fixture("gb-sounds-last.json"),
    )
    .await;
    let (result, _) = sync_sounds(&env, false).await;
    assert_eq!(result.unwrap().fetched(), 5);
    assert_eq!(requested_pages(&env.server).await, ["2"]);
}

#[tokio::test]
async fn a_sync_can_run_on_a_spawned_task() {
    let env = store_env().await;
    mount_sound_catalogue(&env.server).await;
    let client = env.client.clone();
    let outcome = tokio::spawn(async move { client.sync(StoreKind::Sounds, false, &|_| {}).await })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(outcome.fetched(), 5);
}

#[tokio::test]
async fn a_sync_counts_the_429_responses_it_retried() {
    let env = store_env().await;
    Mock::given(method("GET"))
        .and(path("/apiv11/Sound/Index"))
        .and(query_param("_nPage", "1"))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(2)
        .with_priority(1)
        .mount(&env.server)
        .await;
    mount_sound_catalogue(&env.server).await;
    let (result, _) = sync_sounds(&env, false).await;
    let outcome = result.unwrap();
    assert_eq!(outcome.fetched(), 5);
    assert_eq!(outcome.rate_limited(), 2);
}

#[tokio::test]
async fn a_clean_sync_and_a_skipped_sync_report_no_429s() {
    let env = store_env().await;
    mount_sound_catalogue(&env.server).await;
    let (first, _) = sync_sounds(&env, false).await;
    assert_eq!(first.unwrap().rate_limited(), 0);
    let (skipped, _) = sync_sounds(&env, false).await;
    let skipped = skipped.unwrap();
    assert!(skipped.is_fresh());
    assert_eq!(skipped.rate_limited(), 0);
}

#[tokio::test]
async fn concurrent_syncs_count_only_their_own_429s() {
    let env = store_env().await;
    Mock::given(method("GET"))
        .and(path("/apiv11/Sound/Index"))
        .and(query_param("_nPage", "1"))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&env.server)
        .await;
    mount_sound_catalogue(&env.server).await;
    for (page, body) in [
        ("1", store_fixture("gb-crosshairs-page1.json")),
        ("2", with_meta("gb-crosshairs-page1.json", 97, true)),
    ] {
        Mock::given(method("GET"))
            .and(path("/apiv11/Mod/Index"))
            .and(query_param("_aFilters[Generic_Category]", "1105"))
            .and(query_param("_nPage", page))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
            .mount(&env.server)
            .await;
    }
    let (sounds, crosshairs) = tokio::join!(
        env.client.sync(StoreKind::Sounds, false, &|_| {}),
        env.client.sync(StoreKind::Crosshairs, false, &|_| {}),
    );
    assert_eq!(sounds.unwrap().rate_limited(), 1);
    assert_eq!(crosshairs.unwrap().rate_limited(), 0);
}

#[tokio::test]
async fn with_user_agent_replaces_the_header_on_api_requests() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tf2tools_core::DataDir::at(tmp.path().join("data"));
    let db = tf2tools_core::Database::open(&dir).unwrap();
    let server = MockServer::start().await;
    for (page, fixture) in [(1, "gb-sounds-page1.json"), (2, "gb-sounds-last.json")] {
        Mock::given(method("GET"))
            .and(path("/apiv11/Sound/Index"))
            .and(query_param("_nPage", page.to_string()))
            .and(wiremock::matchers::header(
                "user-agent",
                "tf2tools-research (+https://github.com)",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(store_fixture(fixture)))
            .mount(&server)
            .await;
    }
    let endpoints =
        tf2tools_core::Endpoints::default().with_gamebanana(format!("{}/apiv11", server.uri()));
    let client = tf2tools_core::StoreClient::new(db, dir, endpoints).with_config(
        common::fast_config().with_user_agent("tf2tools-research (+https://github.com)"),
    );
    let outcome = client
        .sync(StoreKind::Sounds, false, &|_| {})
        .await
        .unwrap();
    assert_eq!(outcome.fetched(), 5);
}

/// A recorded index page with its catalogue size and completeness replaced.
fn with_meta(fixture: &str, total: u32, complete: bool) -> Vec<u8> {
    let mut page: serde_json::Value = serde_json::from_slice(&store_fixture(fixture)).unwrap();
    page["_aMetadata"]["_nRecordCount"] = total.into();
    page["_aMetadata"]["_bIsComplete"] = complete.into();
    serde_json::to_vec(&page).unwrap()
}

/// Serves `body` for every page of `model`'s index.
async fn mount_every_page(server: &MockServer, model: &str, body: Vec<u8>) {
    Mock::given(method("GET"))
        .and(path(format!("/apiv11/{model}/Index")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
        .mount(server)
        .await;
}

fn seed_stale_sound(env: &StoreEnv) {
    sql(&env.dir)
        .execute(
            "INSERT INTO store_sounds (gb_id, name, fetched_at) VALUES (1, 'stale', ?1)",
            [hours_ago(2)],
        )
        .unwrap();
}

#[tokio::test]
async fn an_empty_page_before_the_end_fails_and_keeps_every_cached_row() {
    let env = store_env().await;
    seed_stale_sound(&env);
    mount_page(
        &env.server,
        "Sound",
        1,
        store_fixture("gb-sounds-page1.json"),
    )
    .await;
    mount_page(&env.server, "Sound", 2, empty_page(19718)).await;
    let (result, _) = sync_sounds(&env, false).await;
    assert!(
        matches!(result, Err(SyncError::Malformed { .. })),
        "{result:?}"
    );
    assert_eq!(count(&env.dir, "store_sounds"), 4);
    assert_eq!(meta(&env.dir, "sounds_last_synced"), None);
    assert_eq!(meta(&env.dir, "sounds_sync_page").as_deref(), Some("2"));
}

#[tokio::test]
async fn reaching_the_catalogue_size_confirms_the_end_and_drops_unseen_rows() {
    let env = store_env().await;
    seed_stale_sound(&env);
    mount_page(
        &env.server,
        "Sound",
        1,
        with_meta("gb-sounds-page1.json", 3, false),
    )
    .await;
    let (result, _) = sync_sounds(&env, false).await;
    let outcome = result.unwrap();
    assert_eq!(requested_pages(&env.server).await, ["1"]);
    assert_eq!((outcome.fetched(), outcome.rows()), (3, 3));
    assert!(!names(&env, "store_sounds").contains(&"stale".to_owned()));
}

#[tokio::test]
async fn repeated_rows_cannot_confirm_the_end_so_the_sound_limit_keeps_unseen_rows() {
    let env = store_env().await;
    seed_stale_sound(&env);
    // 30 records give a sound limit of 3 pages; every page repeats the same 3 rows.
    mount_every_page(
        &env.server,
        "Sound",
        with_meta("gb-sounds-page1.json", 30, false),
    )
    .await;
    let (result, _) = sync_sounds(&env, false).await;
    let outcome = result.unwrap();
    assert_eq!(requested_pages(&env.server).await, ["1", "2", "3"]);
    assert_eq!((outcome.fetched(), outcome.rows()), (9, 4));
    assert!(names(&env, "store_sounds").contains(&"stale".to_owned()));
    assert_store_timestamp(&meta(&env.dir, "sounds_last_synced").unwrap());
    assert_eq!(meta(&env.dir, "sounds_sync_page"), None);
}

#[tokio::test]
async fn a_mod_store_stops_at_its_page_limit() {
    let env = store_env().await;
    let client = env
        .client
        .clone()
        .with_config(fast_config().with_page_limit(StoreKind::Crosshairs, 2));
    mount_every_page(
        &env.server,
        "Mod",
        store_fixture("gb-crosshairs-page1.json"),
    )
    .await;
    let outcome = client
        .sync(StoreKind::Crosshairs, false, &|_| {})
        .await
        .unwrap();
    assert_eq!(requested_pages(&env.server).await, ["1", "2"]);
    assert_eq!((outcome.fetched(), outcome.rows()), (6, 3));
    assert!(meta(&env.dir, "crosshairs_last_synced").is_some());
}

/// A full 15-record sound page with the given completeness, ids 1000..1015.
fn full_page(complete: bool) -> Vec<u8> {
    let mut page: serde_json::Value =
        serde_json::from_slice(&store_fixture("gb-sounds-page1.json")).unwrap();
    let record = page["_aRecords"][0].clone();
    page["_aRecords"] = (0..15)
        .map(|i| {
            let mut r = record.clone();
            r["_idRow"] = (1000 + i).into();
            r
        })
        .collect::<Vec<_>>()
        .into();
    page["_aMetadata"]["_bIsComplete"] = complete.into();
    serde_json::to_vec(&page).unwrap()
}

#[tokio::test]
async fn a_full_page_claiming_completeness_is_not_an_end() {
    let env = store_env().await;
    seed_stale_sound(&env);
    mount_page(&env.server, "Sound", 1, full_page(true)).await;
    mount_page(&env.server, "Sound", 2, empty_page(19718)).await;
    let (result, _) = sync_sounds(&env, false).await;
    assert!(
        matches!(result, Err(SyncError::Malformed { .. })),
        "{result:?}"
    );
    assert!(requested_pages(&env.server).await.contains(&"2".to_owned()));
    assert_eq!(
        count(&env.dir, "store_sounds"),
        16,
        "the stale row survives"
    );
    assert_eq!(meta(&env.dir, "sounds_last_synced"), None);
}

#[tokio::test]
async fn an_empty_page_is_retried_once_and_the_walk_goes_on() {
    let env = store_env().await;
    Mock::given(method("GET"))
        .and(path("/apiv11/Sound/Index"))
        .and(query_param("_nPage", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(empty_page(19718)))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&env.server)
        .await;
    mount_sound_catalogue(&env.server).await;
    let (result, _) = sync_sounds(&env, false).await;
    assert_eq!(result.unwrap().rows(), 5);
    assert_eq!(requested_pages(&env.server).await, ["1", "2", "2"]);
}

#[tokio::test]
async fn an_empty_body_is_retried_once() {
    let env = store_env().await;
    Mock::given(method("GET"))
        .and(path("/apiv11/Sound/Index"))
        .and(query_param("_nPage", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_string(""))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&env.server)
        .await;
    mount_sound_catalogue(&env.server).await;
    let (result, _) = sync_sounds(&env, false).await;
    assert_eq!(result.unwrap().rows(), 5);
    assert_eq!(requested_pages(&env.server).await, ["1", "1", "2"]);
}

#[tokio::test]
async fn a_page_empty_twice_fails_after_one_retry() {
    let env = store_env().await;
    mount_page(
        &env.server,
        "Sound",
        1,
        store_fixture("gb-sounds-page1.json"),
    )
    .await;
    mount_page(&env.server, "Sound", 2, empty_page(19718)).await;
    let (result, _) = sync_sounds(&env, false).await;
    assert!(
        matches!(result, Err(SyncError::Malformed { .. })),
        "{result:?}"
    );
    assert_eq!(requested_pages(&env.server).await, ["1", "2", "2"]);
}

#[tokio::test]
async fn sounds_are_walked_newest_first_and_mods_by_most_liked() {
    let env = store_env().await;
    for (page, fixture) in [("1", "gb-sounds-page1.json"), ("2", "gb-sounds-last.json")] {
        Mock::given(method("GET"))
            .and(path("/apiv11/Sound/Index"))
            .and(query_param("_nPage", page))
            .and(query_param("_sSort", "Generic_Newest"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(store_fixture(fixture)))
            .mount(&env.server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/apiv11/Mod/Index"))
        .and(query_param("_sSort", "Generic_MostLiked"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(with_meta(
            "gb-crosshairs-page1.json",
            3,
            false,
        )))
        .mount(&env.server)
        .await;
    let (sounds, _) = sync_sounds(&env, false).await;
    assert_eq!(sounds.unwrap().rows(), 5);
    let crosshairs = env.client.sync(StoreKind::Crosshairs, false, &|_| {}).await;
    assert_eq!(crosshairs.unwrap().rows(), 3);
}

#[tokio::test]
async fn a_sound_walk_records_its_order_and_a_failed_forced_sync_keeps_it() {
    let env = store_env().await;
    mount_sound_catalogue(&env.server).await;
    sync_sounds(&env, false).await.0.unwrap();
    assert_eq!(
        meta(&env.dir, "sounds_sync_order").as_deref(),
        Some("newest")
    );
    env.server.reset().await;
    Mock::given(method("GET"))
        .and(path("/apiv11/Sound/Index"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&env.server)
        .await;
    let (result, _) = sync_sounds(&env, true).await;
    assert!(
        matches!(result, Err(SyncError::Http { status: 500, .. })),
        "{result:?}"
    );
    assert_eq!(
        meta(&env.dir, "sounds_sync_order").as_deref(),
        Some("newest")
    );
}

#[tokio::test]
async fn a_v5_checkpoint_without_the_order_key_is_discarded() {
    let env = store_env().await;
    set_meta(&env.dir, "sounds_sync_page", "12");
    set_meta(&env.dir, "sounds_sync_start", "2026-07-04T17:44:56.764Z");
    set_meta(&env.dir, "sounds_sync_fetched", "165");
    mount_sound_catalogue(&env.server).await;
    let (result, seen) = sync_sounds(&env, false).await;
    assert_eq!(result.unwrap().fetched(), 5);
    assert_eq!(seen, [(3, 19718), (5, 19718)]);
    assert_eq!(requested_pages(&env.server).await, ["1", "2"]);
}

#[tokio::test]
async fn a_fresh_v5_sound_cache_without_the_order_key_walks_again() {
    let env = store_env().await;
    set_meta(&env.dir, "sounds_last_synced", &hours_ago(1));
    mount_sound_catalogue(&env.server).await;
    let (result, _) = sync_sounds(&env, false).await;
    assert!(!result.unwrap().is_fresh());
    assert_eq!(requested_pages(&env.server).await, ["1", "2"]);
}
