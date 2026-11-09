//! Listing an item's downloadable files, and fetching store audio previews.

use std::time::{Duration, Instant};

use crate::common;

use common::{StoreEnv, fast_config, sql, store_env, store_fixture};
use tf2tools_core::{
    AddonName, Database, Endpoints, FilesError, GbId, PreviewError, StoreClient, StoreItemId,
    StoreKind,
};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

async fn mount_files(env: &StoreEnv, model: &str, id: u64, body: Vec<u8>) {
    Mock::given(method("GET"))
        .and(path(format!("/apiv11/{model}/{id}")))
        .and(query_param("_csvProperties", "_idRow,_sName,_aFiles"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
        .mount(&env.server)
        .await;
}

fn gb(id: u64) -> StoreItemId {
    StoreItemId::GameBanana(GbId::new(id))
}

#[tokio::test]
async fn a_sounds_gamebanana_files_are_listed() {
    let env = store_env().await;
    mount_files(&env, "Sound", 53978, store_fixture("gb-sound-files.json")).await;
    let files = env
        .client
        .item_files(StoreKind::Sounds, &gb(53978))
        .await
        .unwrap();
    assert_eq!(files.len(), 7);
    let first = &files[0];
    assert_eq!(
        (first.kind(), first.item()),
        (StoreKind::Sounds, &gb(53978))
    );
    assert_eq!(first.file_name(), "musical_events_78d81.zip");
    assert_eq!(first.size(), Some(400533259));
    assert_eq!(first.md5(), Some("21dffdce376a8089b5a6d451424f7ebe"));
    assert_eq!(first.url(), "https://gamebanana.com/dl/1146490");
}

#[tokio::test]
async fn crosshair_and_hud_files_come_from_the_mod_endpoint() {
    let env = store_env().await;
    mount_files(
        &env,
        "Mod",
        456577,
        store_fixture("gb-crosshair-files.json"),
    )
    .await;
    for kind in [StoreKind::Crosshairs, StoreKind::GameBananaHuds] {
        let files = env.client.item_files(kind, &gb(456577)).await.unwrap();
        let [file] = files.as_slice() else {
            panic!("{files:?}")
        };
        assert_eq!(file.kind(), kind);
        assert_eq!(file.file_name(), "-dyselxic-crosshair-main_aefd4.zip");
        assert_eq!(
            (file.size(), file.md5()),
            (Some(700007), Some("624ae757067d6af1d845b42bda01f73b"))
        );
    }
}

#[tokio::test]
async fn files_without_a_download_url_are_skipped_and_an_empty_md5_is_none() {
    let env = store_env().await;
    let body = serde_json::json!({"_idRow": 7, "_sName": "x", "_aFiles": [
        {"_idRow": 1, "_sFile": "a.zip", "_nFilesize": 10, "_sDownloadUrl": "", "_sMd5Checksum": "ab"},
        {"_idRow": 2, "_sFile": "b.zip", "_sDownloadUrl": "https://gamebanana.com/dl/2", "_sMd5Checksum": ""}
    ]});
    mount_files(&env, "Mod", 7, serde_json::to_vec(&body).unwrap()).await;
    let files = env
        .client
        .item_files(StoreKind::Crosshairs, &gb(7))
        .await
        .unwrap();
    let [file] = files.as_slice() else {
        panic!("{files:?}")
    };
    assert_eq!(file.file_name(), "b.zip");
    assert_eq!((file.size(), file.md5()), (None, None));
}

#[tokio::test]
async fn a_failed_file_listing_reports_its_status() {
    let env = store_env().await;
    let result = env.client.item_files(StoreKind::Sounds, &gb(1)).await;
    assert!(
        matches!(result, Err(FilesError::Http { status: 404, .. })),
        "{result:?}"
    );
}

#[tokio::test]
async fn curated_files_point_at_the_repo_archive() {
    let env = store_env().await;
    sql(&env.dir)
        .execute_batch(
            "INSERT INTO store_huds (hud_id, name, author, repo, hash, data_json, fetched_at) VALUES
                ('rayshud', 'rayshud', 'raysfire', 'https://github.com/raysfire/rayshud', 'a22e', '{}', 'x'),
                ('budhud', 'budhud', 'Whisker', NULL, NULL, '{}', 'x');",
        )
        .unwrap();
    let id = StoreItemId::Curated("rayshud".to_owned());
    let files = env
        .client
        .item_files(StoreKind::CuratedHuds, &id)
        .await
        .unwrap();
    let [file] = files.as_slice() else {
        panic!("{files:?}")
    };
    assert_eq!((file.item(), file.file_name()), (&id, "rayshud.zip"));
    assert_eq!(
        file.url(),
        "https://github.com/raysfire/rayshud/archive/a22e.zip"
    );
    assert_eq!((file.size(), file.md5()), (None, None));
    assert!(env.server.received_requests().await.unwrap().is_empty());

    for (kind, item) in [
        (
            StoreKind::CuratedHuds,
            StoreItemId::Curated("budhud".to_owned()),
        ),
        (
            StoreKind::CuratedHuds,
            StoreItemId::Curated("nope".to_owned()),
        ),
        (
            StoreKind::Sounds,
            StoreItemId::Curated("rayshud".to_owned()),
        ),
    ] {
        let result = env.client.item_files(kind, &item).await;
        assert!(
            matches!(result, Err(FilesError::NotInStore { .. })),
            "{item:?}: {result:?}"
        );
    }
}

#[tokio::test]
async fn addon_files_are_the_release_asset() {
    let env = store_env().await;
    sql(&env.dir)
        .execute_batch(
            "INSERT INTO store_addons (asset_name, display_name, download_url, file_size, release_tag, fetched_at)
             VALUES ('mastercomfig-addon-flat-mouse.vpk', 'Flat Mouse', 'https://github.com/x/flat.vpk', 299, '9.100.1', 'x');",
        )
        .unwrap();
    let id = StoreItemId::Addon(AddonName::new("mastercomfig-addon-flat-mouse.vpk").unwrap());
    let files = env.client.item_files(StoreKind::Addons, &id).await.unwrap();
    let [file] = files.as_slice() else {
        panic!("{files:?}")
    };
    assert_eq!(file.file_name(), "mastercomfig-addon-flat-mouse.vpk");
    assert_eq!(file.url(), "https://github.com/x/flat.vpk");
    assert_eq!((file.size(), file.md5()), (Some(299), None));
}

fn seed_preview(env: &StoreEnv, preview_url: Option<&str>) {
    sql(&env.dir)
        .execute(
            "INSERT INTO store_sounds (gb_id, name, preview_url, fetched_at) VALUES (1, 'Ding', ?1, 'x')",
            [preview_url],
        )
        .unwrap();
}

#[tokio::test]
async fn preview_audio_fetches_a_trusted_preview() {
    let env = store_env().await;
    seed_preview(
        &env,
        Some(&format!("{}/preview/ding.mp3", env.server.uri())),
    );
    Mock::given(method("GET"))
        .and(path("/preview/ding.mp3"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"ID3 audio".to_vec()))
        .mount(&env.server)
        .await;
    let item = env.client.store_items(StoreKind::Sounds).unwrap().remove(0);
    assert_eq!(env.client.preview_audio(&item).await.unwrap(), b"ID3 audio");
}

#[tokio::test]
async fn preview_audio_refuses_untrusted_urls_without_a_request() {
    let env = store_env().await;
    for url in [
        "https://evil.example/x.mp3".to_owned(),
        format!("{}/previewer/x.mp3", env.server.uri()),
    ] {
        sql(&env.dir)
            .execute("DELETE FROM store_sounds", [])
            .unwrap();
        seed_preview(&env, Some(&url));
        let item = env.client.store_items(StoreKind::Sounds).unwrap().remove(0);
        let result = env.client.preview_audio(&item).await;
        assert!(
            matches!(&result, Err(PreviewError::UntrustedUrl { url: u }) if *u == url),
            "{result:?}"
        );
    }
    assert!(env.server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_prefix_without_a_slash_refuses_a_look_alike_host() {
    let env = store_env().await;
    let endpoints = Endpoints::default().with_preview_prefixes(["https://files.gamebanana.com"]);
    let db = Database::open(&env.dir).unwrap();
    let client = StoreClient::new(db, env.dir.clone(), endpoints).with_config(fast_config());
    let url = "https://files.gamebanana.com.evil.example/x.mp3";
    seed_preview(&env, Some(url));
    let item = client.store_items(StoreKind::Sounds).unwrap().remove(0);
    let result = client.preview_audio(&item).await;
    assert!(
        matches!(&result, Err(PreviewError::UntrustedUrl { url: u }) if u == url),
        "{result:?}"
    );
}

#[tokio::test]
async fn an_item_without_a_preview_is_refused() {
    let env = store_env().await;
    seed_preview(&env, None);
    let item = env.client.store_items(StoreKind::Sounds).unwrap().remove(0);
    let result = env.client.preview_audio(&item).await;
    assert!(matches!(result, Err(PreviewError::NoPreview)), "{result:?}");
}

#[tokio::test]
async fn a_preview_over_the_sound_cap_is_refused() {
    let env = store_env().await;
    let client = env
        .client
        .clone()
        .with_config(fast_config().with_size_cap(StoreKind::Sounds, 4));
    seed_preview(
        &env,
        Some(&format!("{}/preview/long.mp3", env.server.uri())),
    );
    Mock::given(method("GET"))
        .and(path("/preview/long.mp3"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0u8; 16]))
        .mount(&env.server)
        .await;
    let item = client.store_items(StoreKind::Sounds).unwrap().remove(0);
    let result = client.preview_audio(&item).await;
    assert!(
        matches!(result, Err(PreviewError::TooLarge { limit: 4, .. })),
        "{result:?}"
    );
}

#[tokio::test]
async fn an_empty_file_listing_is_retried_then_listed() {
    let env = store_env().await;
    Mock::given(method("GET"))
        .and(path("/apiv11/Sound/53978"))
        .respond_with(ResponseTemplate::new(200).set_body_string(""))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&env.server)
        .await;
    mount_files(&env, "Sound", 53978, store_fixture("gb-sound-files.json")).await;
    let files = env
        .client
        .item_files(StoreKind::Sounds, &gb(53978))
        .await
        .unwrap();
    assert_eq!(files.len(), 7);
    assert_eq!(env.server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn three_empty_file_listings_fail_as_throttled() {
    let env = store_env().await;
    mount_files(&env, "Sound", 53978, Vec::new()).await;
    let result = env.client.item_files(StoreKind::Sounds, &gb(53978)).await;
    assert!(
        matches!(result, Err(FilesError::Throttled { .. })),
        "{result:?}"
    );
    assert_eq!(env.server.received_requests().await.unwrap().len(), 3);
}

#[tokio::test]
async fn clients_sharing_a_pacer_space_their_gamebanana_calls() {
    let env = store_env().await;
    mount_files(&env, "Sound", 53978, store_fixture("gb-sound-files.json")).await;
    let gap = Duration::from_millis(300);
    let first = env
        .client
        .clone()
        .with_config(fast_config().with_request_spacing(gap));
    let db = Database::open(&env.dir).unwrap();
    let endpoints = Endpoints::default().with_gamebanana(format!("{}/apiv11", env.server.uri()));
    let second = StoreClient::new(db, env.dir.clone(), endpoints)
        .with_config(fast_config())
        .with_pacer_of(&first);
    let item = gb(53978);
    let start = Instant::now();
    let (a, b) = tokio::join!(
        first.item_files(StoreKind::Sounds, &item),
        second.item_files(StoreKind::Sounds, &item),
    );
    assert!(a.is_ok() && b.is_ok(), "{a:?} {b:?}");
    assert!(
        start.elapsed() >= gap,
        "the second call waited the shared gap: {:?}",
        start.elapsed()
    );
}
