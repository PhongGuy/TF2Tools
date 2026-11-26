//! Finding a demo's missing map on GameBanana through the local map catalogue.

// Tasks 2 and 3 use the download helpers; Task 3 removes this allowance.
#![allow(dead_code, unused_imports)]

use std::sync::Mutex;
use std::time::Duration;

use crate::common::{self, StoreEnv, gb_addon_file, store_env};
use serde_json::{Value, json};
use tf2tools_core::{GbId, GbMapDownload, GbMapSearchError, GbMapSearchProgress};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

fn bsp(tag: &[u8]) -> Vec<u8> {
    let mut body = b"VBSP".to_vec();
    body.extend_from_slice(tag);
    body
}

/// Catalogue rows (gb_id, name, download_rank, obsolete), as a sync leaves them.
fn seed(env: &StoreEnv, rows: &[(u64, &str, i64, bool)]) {
    let conn = common::sql(&env.dir);
    for &(id, name, rank, obsolete) in rows {
        conn.execute(
            "INSERT INTO store_gb_maps (gb_id, name, download_rank, obsolete, fetched_at)
             VALUES (?1, ?2, ?3, ?4, 'x')",
            rusqlite::params![i64::try_from(id).unwrap(), name, rank, obsolete],
        )
        .unwrap();
    }
}

/// Mounts item `id`'s ProfilePage listing `files`, without reading it back.
async fn profile(env: &StoreEnv, id: u64, files: Vec<Value>) {
    Mock::given(method("GET"))
        .and(path(format!("/apiv11/Mod/{id}/ProfilePage")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "_idRow": id, "_sName": format!("Item {id}"), "_aSubmitter": {"_sName": "vex"},
            "_sProfileUrl": format!("https://gamebanana.com/mods/{id}"), "_aFiles": files,
        })))
        .mount(&env.server)
        .await;
}

/// Item `id` whose one clean file `file` holds `body`, served at `/dl/addon/<id>/0`.
async fn item(env: &StoreEnv, id: u64, file: &str, body: &[u8]) {
    let f = gb_addon_file(env, id, 0, file, body, 100).await;
    profile(env, id, vec![f]).await;
}

async fn find(
    env: &StoreEnv,
    name: &str,
) -> (
    Result<GbMapDownload, GbMapSearchError>,
    Vec<GbMapSearchProgress>,
) {
    let seen = Mutex::new(Vec::new());
    let result = env
        .client
        .find_gb_map(name, &|p: GbMapSearchProgress| seen.lock().unwrap().push(p))
        .await;
    (result, seen.into_inner().unwrap())
}

/// The item ids whose ProfilePage was asked for, in order.
async fn profiles_asked(env: &StoreEnv) -> Vec<u64> {
    env.server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter_map(|r| {
            r.url
                .path()
                .strip_prefix("/apiv11/Mod/")?
                .strip_suffix("/ProfilePage")?
                .parse()
                .ok()
        })
        .collect()
}

/// The paths of the file downloads asked for, in order.
async fn downloads_asked(env: &StoreEnv) -> Vec<String> {
    env.server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|r| r.url.path().to_owned())
        .filter(|p| p.starts_with("/dl/"))
        .collect()
}

fn assert_send_sync<T: Send + Sync + 'static>() {}

#[tokio::test]
async fn candidates_come_tier_by_tier_in_download_order_without_obsolete_rows() {
    assert_send_sync::<GbMapSearchError>();
    assert_send_sync::<GbMapSearchProgress>();
    let env = store_env().await;
    seed(
        &env,
        &[
            (1, "KOTH_X_RC1", 40, false),
            (2, "koth_x_rc1", 20, false),
            (8, "koth_x_rc1", 20, false),
            (3, "Koth_X_RC1 Pack", 5, false),
            (4, "koth_x_b2", 30, false),
            (5, "koth_x", 1, false),
            (6, "koth_x_rc1", 2, true),
            (7, "pl_y", 3, false),
            (9, "kothYx", 4, false),
        ],
    );
    for id in [1, 2, 3, 4, 5, 8] {
        profile(&env, id, vec![]).await;
    }
    let (result, progress) = find(&env, "koth_x_rc1").await;
    assert!(
        matches!(&result, Err(GbMapSearchError::NoMatch { name, tried: 0 }) if name == "koth_x_rc1"),
        "{result:?}"
    );
    assert_eq!(profiles_asked(&env).await, [2, 8, 1, 3, 5, 4]);
    assert!(progress.is_empty());
}

#[tokio::test]
async fn one_trailing_version_suffix_is_stripped_whatever_its_case() {
    let env = store_env().await;
    seed(
        &env,
        &[
            (1, "koth_q_rc1", 1, false),
            (2, "koth_q", 2, false),
            (3, "pl_w", 3, false),
            (4, "pl_w_b7 fixed", 4, false),
        ],
    );
    for id in 1..=4 {
        profile(&env, id, vec![]).await;
    }
    let (result, _) = find(&env, "koth_q_rc1_FINAL").await;
    assert!(
        matches!(result, Err(GbMapSearchError::NoMatch { .. })),
        "{result:?}"
    );
    assert_eq!(profiles_asked(&env).await, [1], "only _FINAL is stripped");
    env.server.reset().await;
    for id in 1..=4 {
        profile(&env, id, vec![]).await;
    }
    let (result, _) = find(&env, "PL_W_B7").await;
    assert!(
        matches!(result, Err(GbMapSearchError::NoMatch { .. })),
        "{result:?}"
    );
    assert_eq!(profiles_asked(&env).await, [4, 3]);
}

#[tokio::test]
async fn an_empty_catalogue_is_not_synced_and_asks_nothing() {
    let env = store_env().await;
    let (result, _) = find(&env, "koth_x").await;
    assert!(
        matches!(result, Err(GbMapSearchError::NotSynced)),
        "{result:?}"
    );
    assert!(env.server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_catalogue_without_the_map_is_no_match() {
    let env = store_env().await;
    seed(&env, &[(1, "pl_y", 1, false)]);
    let (result, _) = find(&env, "koth_x").await;
    assert!(
        matches!(result, Err(GbMapSearchError::NoMatch { tried: 0, .. })),
        "{result:?}"
    );
    assert!(env.server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_name_that_is_no_map_file_name_is_refused() {
    let env = store_env().await;
    seed(&env, &[(1, "koth_x", 1, false)]);
    for bad in ["", "../koth_x", "koth x", ".koth_x"] {
        let (result, _) = find(&env, bad).await;
        assert!(
            matches!(&result, Err(GbMapSearchError::BadName { name }) if name == bad),
            "{bad:?}: {result:?}"
        );
    }
    assert!(env.server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_pack_matches_by_its_one_map_and_holds_only_that_map() {
    let env = store_env().await;
    seed(&env, &[(3, "cp_a and cp_b", 1, false)]);
    let body = common::zip_bytes(&[
        ("Pack/maps/cp_a.bsp", &bsp(b"a")),
        ("Pack/maps/cp_b.bsp", &bsp(b"b")),
    ]);
    item(&env, 3, "pack.zip", &body).await;
    let (result, progress) = find(&env, "cp_b").await;
    let d = result.unwrap();
    let names: Vec<&str> = d.maps().iter().map(|m| m.name()).collect();
    assert_eq!(names, ["cp_b"]);
    assert_eq!(
        (d.gb_id(), d.file_id(), d.file_name()),
        (GbId::new(3), 100, "pack.zip")
    );
    assert_eq!(
        (
            progress[0].candidate,
            progress[0].of,
            progress[0].download.downloaded()
        ),
        (1, 1, 0)
    );
    assert!(progress.iter().all(|p| (p.candidate, p.of) == (1, 1)));
    assert!(!common::tmp_is_empty(&env.dir));
    drop(d);
    assert!(
        common::tmp_is_empty(&env.dir),
        "dropping it removes the scratch dir"
    );
}

#[tokio::test]
async fn the_name_matches_in_any_case_and_keeps_the_archive_spelling() {
    let env = store_env().await;
    seed(&env, &[(1, "koth_x", 1, false)]);
    item(
        &env,
        1,
        "pack.zip",
        &common::zip_bytes(&[("maps/koth_x.bsp", &bsp(b"x"))]),
    )
    .await;
    let d = find(&env, "Koth_X").await.0.unwrap();
    assert_eq!(d.maps()[0].name(), "koth_x");
}

#[tokio::test]
async fn another_version_never_stands_in() {
    let env = store_env().await;
    seed(&env, &[(1, "koth_x", 1, false)]);
    item(
        &env,
        1,
        "pack.zip",
        &common::zip_bytes(&[("maps/koth_x_rc1.bsp", &bsp(b"r"))]),
    )
    .await;
    let (result, _) = find(&env, "koth_x").await;
    assert!(
        matches!(result, Err(GbMapSearchError::NoMatch { tried: 1, .. })),
        "{result:?}"
    );
    assert!(common::tmp_is_empty(&env.dir));
}

#[tokio::test]
async fn a_map_file_without_vbsp_magic_is_no_map() {
    let env = store_env().await;
    seed(&env, &[(1, "koth_x", 1, false)]);
    item(
        &env,
        1,
        "pack.zip",
        &common::zip_bytes(&[("maps/koth_x.bsp", b"not a map")]),
    )
    .await;
    let (result, _) = find(&env, "koth_x").await;
    assert!(
        matches!(result, Err(GbMapSearchError::NoMatch { tried: 1, .. })),
        "{result:?}"
    );
}

#[tokio::test]
async fn at_most_three_archives_are_downloaded() {
    let env = store_env().await;
    seed(
        &env,
        &[
            (1, "koth_x", 1, false),
            (2, "koth_x", 2, false),
            (3, "koth_x", 3, false),
            (4, "koth_x", 4, false),
        ],
    );
    let other = common::zip_bytes(&[("maps/cp_other.bsp", &bsp(b"o"))]);
    for id in 1..=4 {
        item(&env, id, "pack.zip", &other).await;
    }
    let (result, progress) = find(&env, "koth_x").await;
    assert!(
        matches!(result, Err(GbMapSearchError::NoMatch { tried: 3, .. })),
        "{result:?}"
    );
    assert_eq!(profiles_asked(&env).await, [1, 2, 3, 4]);
    assert_eq!(
        downloads_asked(&env).await,
        ["/dl/addon/1/0", "/dl/addon/2/0", "/dl/addon/3/0"]
    );
    assert!(progress.iter().all(|p| p.of == 3));
    let mut seen: Vec<u8> = progress.iter().map(|p| p.candidate).collect();
    seen.dedup();
    assert_eq!(seen, [1, 2, 3]);
}

#[tokio::test]
async fn at_most_six_profiles_are_fetched() {
    let env = store_env().await;
    let rows: Vec<(u64, &str, i64, bool)> = (1..=7)
        .map(|i| (i, "koth_x", i64::try_from(i).unwrap(), false))
        .collect();
    seed(&env, &rows);
    for id in 1..=7 {
        profile(&env, id, vec![]).await;
    }
    let (result, _) = find(&env, "koth_x").await;
    assert!(
        matches!(result, Err(GbMapSearchError::NoMatch { tried: 0, .. })),
        "{result:?}"
    );
    assert_eq!(profiles_asked(&env).await, [1, 2, 3, 4, 5, 6]);
}

#[tokio::test]
async fn a_file_named_for_the_map_is_tried_first() {
    let env = store_env().await;
    seed(
        &env,
        &[
            (1, "koth_x", 1, false),
            (2, "koth_x", 2, false),
            (3, "koth_x", 3, false),
            (4, "koth_x", 4, false),
        ],
    );
    let other = common::zip_bytes(&[("maps/cp_other.bsp", &bsp(b"o"))]);
    for id in 1..=3 {
        item(&env, id, "pack.zip", &other).await;
    }
    item(
        &env,
        4,
        "KOTH_X.zip",
        &common::zip_bytes(&[("maps/koth_x.bsp", &bsp(b"x"))]),
    )
    .await;
    let (result, progress) = find(&env, "koth_x").await;
    assert_eq!(result.unwrap().gb_id(), GbId::new(4));
    assert_eq!(downloads_asked(&env).await, ["/dl/addon/4/0"]);
    assert_eq!((progress[0].candidate, progress[0].of), (1, 3));
}
