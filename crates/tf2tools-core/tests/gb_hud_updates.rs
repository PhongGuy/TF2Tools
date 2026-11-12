//! GameBanana HUDs: file dates, the daily check, version marks and replacing downloads.

use crate::common::{self, StoreEnv, sql, store_env, zip_bytes};

use serde_json::json;
use tf2tools_core::{
    DataDir, GbId, HudEntry, HudId, LibraryEntry, StoreFile, StoreItemId, StoreKind,
};
use tf2tools_core::{Database, Endpoints, GbAddonCheck, GbAddonCheckError, StoreClient};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

const HUDS: StoreKind = StoreKind::GameBananaHuds;

fn hud_zip(root: &str, body: &str) -> Vec<u8> {
    let info = format!("{root}/info.vdf");
    let layout = format!("{root}/resource/ui/hudlayout.res");
    zip_bytes(&[
        (info.as_str(), b"\"hud\" {}".as_slice()),
        (layout.as_str(), body.as_bytes()),
    ])
}

/// Mod `gb_id` lists `files` (file name, root folder, layout text, date added), file i served
/// at `/dl/hud/<gb_id>/<i>`; returns them as `item_files` reads them.
async fn listed(env: &StoreEnv, gb_id: u64, files: &[(&str, &str, &str, i64)]) -> Vec<StoreFile> {
    let uri = env.server.uri();
    let mut records = Vec::new();
    for (i, (name, root, body, date)) in files.iter().enumerate() {
        let route = format!("/dl/hud/{gb_id}/{i}");
        Mock::given(method("GET"))
            .and(path(route.as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(hud_zip(root, body)))
            .mount(&env.server)
            .await;
        records.push(
            json!({"_idRow": i + 1, "_sFile": name, "_nFilesize": 1, "_tsDateAdded": date,
            "_sDownloadUrl": format!("{uri}{route}"), "_sMd5Checksum": ""}),
        );
    }
    Mock::given(method("GET"))
        .and(path(format!("/apiv11/Mod/{gb_id}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"_idRow": gb_id, "_sName": "darhud", "_aFiles": records})),
        )
        .mount(&env.server)
        .await;
    env.client
        .item_files(HUDS, &StoreItemId::GameBanana(GbId::new(gb_id)))
        .await
        .unwrap()
}

fn only_hud(entries: Vec<LibraryEntry>) -> HudEntry {
    match entries.into_iter().next() {
        Some(LibraryEntry::Hud(hud)) => hud,
        other => panic!("not a HUD: {other:?}"),
    }
}

async fn download_hud(env: &StoreEnv, file: &StoreFile) -> HudEntry {
    only_hud(env.client.download(HUDS, file, &|_| {}).await.unwrap())
}

fn file_date(dir: &DataDir, id: &HudId) -> Option<i64> {
    sql(dir)
        .query_row(
            "SELECT gb_file_date FROM library_huds WHERE id = ?1",
            [id.as_str()],
            |r| r.get(0),
        )
        .unwrap()
}

#[tokio::test]
async fn a_gamebanana_hud_download_records_its_file_date() {
    let env = store_env().await;
    let files = listed(
        &env,
        42,
        &[
            ("darhud-202618.zip", "darhud-202618", "v2", 1_700_000_500),
            ("darhud-202517.zip", "darhud-202517", "v1", 1_700_000_100),
        ],
    )
    .await;
    let dates: Vec<Option<i64>> = files.iter().map(StoreFile::date_added).collect();
    assert_eq!(dates, [Some(1_700_000_500), Some(1_700_000_100)]);
    let hud = download_hud(&env, &files[1]).await;
    assert_eq!(file_date(&env.dir, hud.id()), Some(1_700_000_100));
    assert_eq!(
        common::count(&env.dir, "gb_hud_checks"),
        0,
        "the HUDs page seeds check rows, not the download"
    );
}

const NOW: i64 = 1_800_000_000;
const DAY: i64 = 86_400;

fn ids(list: &[u64]) -> Vec<GbId> {
    list.iter().copied().map(GbId::new).collect()
}

fn check_row(dir: &DataDir, id: u64) -> (Option<i64>, Option<i64>, bool) {
    sql(dir)
        .query_row(
            "SELECT latest_file_date, checked_at, gone FROM gb_hud_checks WHERE gb_id = ?1",
            [id as i64],
            |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? != 0)),
        )
        .unwrap()
}

fn set_checked(env: &StoreEnv, id: u64, at: Option<i64>) {
    sql(&env.dir)
        .execute(
            "UPDATE gb_hud_checks SET checked_at = ?1 WHERE gb_id = ?2",
            rusqlite::params![at, id as i64],
        )
        .unwrap();
}

async fn profile(env: &StoreEnv, id: u64, body: serde_json::Value) {
    Mock::given(method("GET"))
        .and(path(format!("/apiv11/Mod/{id}/ProfilePage")))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&env.server)
        .await;
}

#[tokio::test]
async fn due_items_are_the_unchecked_then_the_oldest_checks() {
    let env = store_env().await;
    env.lib.ensure_gb_hud_checks(&ids(&[1, 2, 3, 4])).unwrap();
    set_checked(&env, 1, Some(NOW - DAY - 1));
    set_checked(&env, 2, Some(NOW - 10));
    set_checked(&env, 4, Some(NOW + 30 * DAY));
    assert_eq!(env.lib.gb_huds_due_for_check(NOW).unwrap(), ids(&[3, 1, 4]));
}

#[tokio::test]
async fn ensuring_an_item_again_keeps_its_check() {
    let env = store_env().await;
    env.lib.ensure_gb_hud_checks(&ids(&[5])).unwrap();
    env.lib
        .record_gb_hud_check(GbId::new(5), GbAddonCheck::Latest { file_date: 300 }, NOW)
        .unwrap();
    env.lib.ensure_gb_hud_checks(&ids(&[5, 6])).unwrap();
    assert_eq!(check_row(&env.dir, 5), (Some(300), Some(NOW), false));
    assert_eq!(check_row(&env.dir, 6), (None, None, false));
}

#[tokio::test]
async fn a_check_records_the_newest_file_gone_items_and_failures() {
    let env = store_env().await;
    env.lib
        .ensure_gb_hud_checks(&ids(&[10, 11, 12, 13]))
        .unwrap();
    let older = common::gb_addon_file(&env, 10, 0, "a.zip", b"a", 100).await;
    let newer = common::gb_addon_file(&env, 10, 1, "b.zip", b"b", 300).await;
    profile(
        &env,
        10,
        json!({"_idRow": 10, "_sName": "A", "_aFiles": [older, newer]}),
    )
    .await;
    profile(
        &env,
        11,
        json!({"_idRow": 11, "_sName": "B", "_bIsTrashed": true}),
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/apiv11/Mod/13/ProfilePage"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&env.server)
        .await;
    // 12 has no ProfilePage, so wiremock answers 404.
    assert_eq!(env.client.check_gb_hud_updates(NOW).await.unwrap(), 4);
    assert_eq!(check_row(&env.dir, 10), (Some(300), Some(NOW), false));
    assert_eq!(check_row(&env.dir, 11), (None, Some(NOW), true));
    assert_eq!(check_row(&env.dir, 12), (None, Some(NOW), true));
    assert_eq!(check_row(&env.dir, 13), (None, Some(NOW), false));
    assert!(env.lib.gb_huds_due_for_check(NOW + 60).unwrap().is_empty());
}

#[tokio::test]
async fn an_unreachable_gamebanana_stops_the_check_and_records_nothing() {
    let env = store_env().await;
    env.lib.ensure_gb_hud_checks(&ids(&[20])).unwrap();
    let db = Database::open(&env.dir).unwrap();
    let offline = StoreClient::new(
        db,
        env.dir.clone(),
        Endpoints::default().with_gamebanana("http://127.0.0.1:9/apiv11"),
    )
    .with_config(common::fast_config());
    let result = offline.check_gb_hud_updates(NOW).await;
    assert!(
        matches!(result, Err(GbAddonCheckError::Browse(_))),
        "{result:?}"
    );
    assert_eq!(check_row(&env.dir, 20), (None, None, false));
}
