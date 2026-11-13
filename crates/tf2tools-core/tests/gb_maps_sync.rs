//! The maps catalogue sync: tree, pages of 50, count-only confirmation, floor-guarded prune.

use crate::common;

use common::{StoreEnv, count, fast_config, hours_ago, meta, set_meta, sql, store_env};
use serde_json::{Value, json};
use tf2tools_core::{StoreClient, SyncError, SyncOutcome};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

fn record(id: u64, sub: Option<u32>) -> Value {
    let mut r = json!({"_idRow": id, "_sName": format!("map_{id}")});
    if let Some(sub) = sub {
        r["_aSubCategory"] = json!({"_sName": "Sub", "_sProfileUrl": format!("https://gamebanana.com/mods/cats/{sub}")});
    }
    r
}

fn page(records: Vec<Value>, total: u32, complete: bool) -> Vec<u8> {
    json!({"_aMetadata": {"_nRecordCount": total, "_bIsComplete": complete, "_nPerpage": 50},
        "_aRecords": records})
    .to_string()
    .into_bytes()
}

async fn mount_page(env: &StoreEnv, n: u32, body: Vec<u8>) {
    Mock::given(method("GET"))
        .and(path("/apiv11/Mod/Index"))
        .and(query_param("_aFilters[Generic_Category]", "5371"))
        .and(query_param("_nPage", n.to_string()))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
        .mount(&env.server)
        .await;
}

async fn mount_categories(env: &StoreEnv, parent: u32, rows: &[(u32, &str, u32)]) {
    let rows: Vec<Value> = rows.iter().map(|(id, name, children)|
        json!({"_idRow": id, "_sName": name, "_nItemCount": 1, "_nCategoryCount": children})).collect();
    Mock::given(method("GET"))
        .and(path("/apiv11/Mod/Categories"))
        .and(query_param("_idCategoryRow", parent.to_string()))
        .respond_with(ResponseTemplate::new(200).set_body_json(rows))
        .mount(&env.server)
        .await;
}

/// 5371 lists Achievement/Idle (one child, Trade) and Control Points.
async fn mount_tree(env: &StoreEnv) {
    mount_categories(
        env,
        5371,
        &[(5383, "Achievement/Idle", 1), (5373, "Control Points", 0)],
    )
    .await;
    mount_categories(env, 5383, &[(5388, "Trade", 0)]).await;
}

fn client(env: &StoreEnv, pages: u32) -> StoreClient {
    env.client
        .clone()
        .with_config(fast_config().with_gb_maps_page_limit(pages))
}

async fn sync(client: &StoreClient, force: bool) -> Result<SyncOutcome, SyncError> {
    client.sync_gb_maps(force, &|_| {}).await
}

/// Each request as "path?k=v&...", query decoded, in order.
async fn asked(env: &StoreEnv) -> Vec<String> {
    env.server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|r| {
            let query: Vec<String> = r
                .url
                .query_pairs()
                .map(|(k, v)| format!("{k}={v}"))
                .collect();
            format!("{}?{}", r.url.path(), query.join("&"))
        })
        .collect()
}

fn ids(env: &StoreEnv) -> Vec<i64> {
    let conn = sql(&env.dir);
    let mut s = conn
        .prepare("SELECT gb_id FROM store_gb_maps ORDER BY gb_id")
        .unwrap();
    s.query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn seed_stale(env: &StoreEnv, id: i64, fetched_at: &str) {
    sql(&env.dir)
        .execute(
            "INSERT INTO store_gb_maps (gb_id, name, download_rank, fetched_at)
        VALUES (?1, 'stale', 9999, ?2)",
            rusqlite::params![id, fetched_at],
        )
        .unwrap();
}

#[tokio::test]
async fn a_fresh_walk_reads_the_tree_then_pages_of_fifty_and_the_count_confirms_it() {
    let env = store_env().await;
    seed_stale(&env, 1, &hours_ago(2));
    mount_tree(&env).await;
    mount_page(
        &env,
        1,
        page(
            vec![
                record(7, Some(5373)),
                record(8, Some(5388)),
                record(9, None),
            ],
            3,
            true,
        ),
    )
    .await;
    let outcome = sync(&client(&env, 500), false).await.unwrap();
    assert_eq!((outcome.fetched(), outcome.rows()), (3, 3));
    let asked = asked(&env).await;
    assert_eq!(asked.len(), 3, "{asked:?}");
    assert!(
        asked[0].starts_with("/apiv11/Mod/Categories?_idCategoryRow=5371&")
            && asked[0].contains("_bShowEmpty=true"),
        "{asked:?}"
    );
    assert!(
        asked[1].starts_with("/apiv11/Mod/Categories?_idCategoryRow=5383&"),
        "{asked:?}"
    );
    for part in [
        "_nPage=1",
        "_nPerpage=50",
        "_aFilters[Generic_Category]=5371",
        "_aFilters[Generic_Game]=297",
        "_sSort=Generic_MostDownloaded",
    ] {
        assert!(
            asked[2].starts_with("/apiv11/Mod/Index?") && asked[2].contains(part),
            "{part}: {asked:?}"
        );
    }
    let rank = |id: i64| -> (i64, Option<i64>) {
        sql(&env.dir)
            .query_row(
                "SELECT download_rank, gamemode_id FROM store_gb_maps WHERE gb_id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap()
    };
    assert_eq!(
        [rank(7), rank(8), rank(9)],
        [(1, Some(5373)), (2, Some(5383)), (3, None)]
    );
    assert_eq!(count(&env.dir, "store_gb_map_categories"), 3);
    assert_eq!(
        ids(&env),
        [7, 8, 9],
        "the count confirms the end, so the unseen row goes"
    );
    assert!(meta(&env.dir, "gb_maps_last_synced").is_some());
    assert_eq!(meta(&env.dir, "gb_maps_sync_page"), None);
    assert_eq!(meta(&env.dir, "gb_maps_listing_end_start"), None);
}

/// Ids 100..=137: with one more map, 39 of 40 rows, above the 95% floor.
fn base() -> Vec<Value> {
    (100..=137).map(|i| record(i, None)).collect()
}

#[tokio::test]
async fn a_near_complete_walk_keeps_rows_until_a_second_one_agrees() {
    let env = store_env().await;
    seed_stale(&env, 1, &hours_ago(3));
    mount_tree(&env).await;
    mount_page(
        &env,
        1,
        page([base(), vec![record(7, None)]].concat(), 40, true),
    )
    .await;
    sync(&client(&env, 500), false).await.unwrap();
    assert!(ids(&env).contains(&1), "one listing end prunes nothing");
    assert!(meta(&env.dir, "gb_maps_listing_end_start").is_some());
    env.server.reset().await;
    mount_tree(&env).await;
    // Map 7 drifted out of this walk; the first walk saw it, so it stays.
    mount_page(
        &env,
        1,
        page([base(), vec![record(8, None)]].concat(), 40, true),
    )
    .await;
    sync(&client(&env, 500), true).await.unwrap();
    let ids = ids(&env);
    assert!(
        !ids.contains(&1) && ids.contains(&7) && ids.contains(&8),
        "{ids:?}"
    );
}

#[tokio::test]
async fn truncated_or_thin_walks_keep_every_row() {
    let env = store_env().await;
    seed_stale(&env, 1, &hours_ago(3));
    // Twice: a full page, then an empty page in place of the rest of 6,802 maps.
    for force in [false, true] {
        env.server.reset().await;
        mount_tree(&env).await;
        mount_page(
            &env,
            1,
            page((100..150).map(|i| record(i, None)).collect(), 6802, false),
        )
        .await;
        mount_page(&env, 2, page(vec![], 6802, false)).await;
        sync(&client(&env, 500), force).await.unwrap();
        assert_eq!(meta(&env.dir, "gb_maps_listing_end_start"), None);
    }
    // A real last page, but 2 of 40 rows is under the floor.
    env.server.reset().await;
    mount_tree(&env).await;
    mount_page(
        &env,
        1,
        page(vec![record(7, None), record(8, None)], 40, true),
    )
    .await;
    sync(&client(&env, 500), true).await.unwrap();
    assert_eq!(meta(&env.dir, "gb_maps_listing_end_start"), None);
    assert!(ids(&env).contains(&1));
}

#[tokio::test]
async fn a_walk_stopped_by_the_page_limit_forgets_the_listing_end() {
    let env = store_env().await;
    set_meta(&env.dir, "gb_maps_listing_end_start", &hours_ago(1));
    seed_stale(&env, 1, &hours_ago(2));
    mount_tree(&env).await;
    let full: Vec<Value> = (100..150).map(|i| record(i, Some(5373))).collect();
    mount_page(&env, 1, page(full, 500, false)).await;
    sync(&client(&env, 1), false).await.unwrap();
    assert_eq!(meta(&env.dir, "gb_maps_listing_end_start"), None);
    assert_eq!(count(&env.dir, "store_gb_maps"), 51);
}

#[tokio::test]
async fn a_resumed_walk_reuses_the_stored_tree() {
    let env = store_env().await;
    sql(&env.dir)
        .execute_batch("INSERT INTO store_gb_map_categories VALUES (5373, 'Control Points', 5373);")
        .unwrap();
    set_meta(&env.dir, "gb_maps_sync_page", "2");
    set_meta(&env.dir, "gb_maps_sync_start", &hours_ago(1));
    set_meta(&env.dir, "gb_maps_sync_fetched", "50");
    mount_page(&env, 2, page(vec![record(7, Some(5373))], 51, true)).await;
    sync(&client(&env, 500), false).await.unwrap();
    assert!(
        !asked(&env)
            .await
            .iter()
            .any(|a| a.contains("/Mod/Categories"))
    );
    let row: (i64, Option<i64>) = sql(&env.dir)
        .query_row(
            "SELECT download_rank, gamemode_id FROM store_gb_maps WHERE gb_id = 7",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(row, (51, Some(5373)));
}

#[tokio::test]
async fn a_failed_category_call_fails_before_any_page_and_keeps_the_old_tree() {
    let env = store_env().await;
    sql(&env.dir)
        .execute_batch("INSERT INTO store_gb_map_categories VALUES (5373, 'Control Points', 5373);")
        .unwrap();
    seed_stale(&env, 1, &hours_ago(2));
    Mock::given(method("GET"))
        .and(path("/apiv11/Mod/Categories"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&env.server)
        .await;
    let result = sync(&client(&env, 500), false).await;
    assert!(
        matches!(result, Err(SyncError::Http { status: 404, .. })),
        "{result:?}"
    );
    assert!(!asked(&env).await.iter().any(|a| a.contains("/Mod/Index")));
    assert_eq!(count(&env.dir, "store_gb_map_categories"), 1);
    assert_eq!(ids(&env), [1]);
    assert_eq!(meta(&env.dir, "gb_maps_last_synced"), None);
}
