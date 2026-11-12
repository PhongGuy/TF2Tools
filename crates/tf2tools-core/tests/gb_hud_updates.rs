//! GameBanana HUDs: file dates, the daily check, version marks and replacing downloads.

use crate::common::{self, StoreEnv, sql, store_env, zip_bytes};

use serde_json::json;
use tf2tools_core::{
    DataDir, GbId, HudEntry, HudId, LibraryEntry, StoreFile, StoreItemId, StoreKind,
};
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
