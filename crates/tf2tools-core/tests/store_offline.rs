//! A loopback-only store client: nothing leaves the machine, and a warm disk cache still serves.

mod common;

use common::{StoreEnv, fast_config, sql, store_env, write_file};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tf2tools_core::{
    Database, DownloadError, Endpoints, FilesError, GbId, ImageError, PreviewError, StoreClient,
    StoreConfig, StoreItemId, StoreKind, SyncError,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

const REMOTE_IMAGE: &str = "https://images.gamebanana.com/img/ss/mods/offline.jpg";

fn offline_client(env: &StoreEnv, endpoints: Endpoints) -> StoreClient {
    let db = Database::open(&env.dir).unwrap();
    StoreClient::new(db, env.dir.clone(), endpoints)
        .with_config(fast_config().with_loopback_only(true))
}

fn cache_file(env: &StoreEnv, url: &str) -> PathBuf {
    use sha2::{Digest, Sha256};
    let name: String = Sha256::digest(url.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    env.dir.image_cache_dir().join(name)
}

#[test]
fn loopback_only_is_off_by_default() {
    assert!(!StoreConfig::default().loopback_only());
    assert!(
        StoreConfig::default()
            .with_loopback_only(true)
            .loopback_only()
    );
}

#[test]
fn loopback_endpoints_put_every_base_under_one_address() {
    let e = Endpoints::loopback("http://127.0.0.1:9");
    assert_eq!(e.gamebanana(), "http://127.0.0.1:9/apiv11");
    assert_eq!(e.github_api(), "http://127.0.0.1:9/gh-api");
    assert_eq!(e.github(), "http://127.0.0.1:9/gh");
    assert_eq!(e.preview_prefixes(), ["http://127.0.0.1:9/preview/"]);
    assert_eq!(e.image_prefixes(), ["http://127.0.0.1:9/img/"]);
}

#[tokio::test]
async fn a_remote_image_is_refused_offline() {
    let env = store_env().await;
    let client = offline_client(&env, Endpoints::default());
    assert!(client.is_loopback_only());
    let result = client.fetch_image(REMOTE_IMAGE).await;
    assert!(
        matches!(&result, Err(ImageError::Network { message, .. }) if message.starts_with("offline")),
        "{result:?}"
    );
}

#[tokio::test]
async fn a_warm_cache_serves_a_remote_image_offline() {
    let env = store_env().await;
    write_file(&cache_file(&env, REMOTE_IMAGE), b"cached jpeg");
    let client = offline_client(&env, Endpoints::default());
    assert_eq!(
        client.fetch_image(REMOTE_IMAGE).await.unwrap(),
        b"cached jpeg"
    );
}

#[tokio::test]
async fn a_loopback_server_is_still_reached_offline() {
    let env = store_env().await;
    Mock::given(method("GET"))
        .and(path("/img/a.jpg"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"jpeg".to_vec()))
        .mount(&env.server)
        .await;
    let client = offline_client(&env, Endpoints::loopback(&env.server.uri()));
    let url = format!("{}/img/a.jpg", env.server.uri());
    assert_eq!(client.fetch_image(&url).await.unwrap(), b"jpeg");
}

#[tokio::test]
async fn a_sync_against_remote_endpoints_fails_offline() {
    let env = store_env().await;
    let client = offline_client(&env, Endpoints::default());
    let result = client.sync(StoreKind::Sounds, true, &|_| {}).await;
    assert!(
        matches!(&result, Err(SyncError::Network { message, .. }) if message.starts_with("offline")),
        "{result:?}"
    );
}

fn is_offline(message: &str) -> bool {
    message.starts_with("offline")
}

#[tokio::test]
async fn a_redirect_off_loopback_is_refused_offline_without_connecting() {
    // 192.0.2.1 is TEST-NET-1: an unguarded client would sit in its 10 s connect timeout.
    let env = store_env().await;
    Mock::given(method("GET"))
        .and(path("/apiv11/Sound/Index"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("Location", "http://192.0.2.1/apiv11/Sound/Index"),
        )
        .mount(&env.server)
        .await;
    let client = offline_client(&env, Endpoints::loopback(&env.server.uri()));
    let started = Instant::now();
    let result = client.sync(StoreKind::Sounds, true, &|_| {}).await;
    assert!(
        matches!(&result, Err(SyncError::Network { message, .. }) if is_offline(message)),
        "{result:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[tokio::test]
async fn a_remote_preview_is_refused_offline() {
    let env = store_env().await;
    sql(&env.dir)
        .execute(
            "INSERT INTO store_sounds (gb_id, name, preview_url, fetched_at) \
             VALUES (1, 'Ding', 'https://files.gamebanana.com/a.mp3', 'x')",
            [],
        )
        .unwrap();
    let client = offline_client(&env, Endpoints::default());
    let item = client.store_items(StoreKind::Sounds).unwrap().remove(0);
    let result = client.preview_audio(&item).await;
    assert!(
        matches!(&result, Err(PreviewError::Network { message, .. }) if is_offline(message)),
        "{result:?}"
    );
}

#[tokio::test]
async fn a_remote_file_listing_is_refused_offline() {
    let env = store_env().await;
    let client = offline_client(&env, Endpoints::default());
    let id = StoreItemId::GameBanana(GbId::new(7));
    let result = client.item_files(StoreKind::Sounds, &id).await;
    assert!(
        matches!(&result, Err(FilesError::Network { message, .. }) if is_offline(message)),
        "{result:?}"
    );
}

#[tokio::test]
async fn a_remote_download_is_refused_offline() {
    // The listing comes from the loopback server; its download URL points off the machine.
    let env = store_env().await;
    sql(&env.dir)
        .execute(
            "INSERT INTO store_sounds (gb_id, name, fetched_at) VALUES (7, 'x', 'x')",
            [],
        )
        .unwrap();
    let listing = serde_json::json!({"_idRow": 7, "_sName": "x", "_aFiles": [{
        "_sFile": "a.wav", "_nFilesize": 1, "_sDownloadUrl": "https://files.gamebanana.com/dl/1"
    }]});
    Mock::given(method("GET"))
        .and(path("/apiv11/Sound/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(listing))
        .mount(&env.server)
        .await;
    let client = offline_client(&env, Endpoints::loopback(&env.server.uri()));
    let id = StoreItemId::GameBanana(GbId::new(7));
    let file = client
        .item_files(StoreKind::Sounds, &id)
        .await
        .unwrap()
        .remove(0);
    let result = client.download(StoreKind::Sounds, &file, &|_| {}).await;
    assert!(
        matches!(&result, Err(DownloadError::Network { message, .. }) if is_offline(message)),
        "{result:?}"
    );
}

#[test]
fn loopback_endpoints_put_hud_db_screenshots_under_the_image_prefix() {
    assert_eq!(
        Endpoints::default().hud_resources(),
        "https://raw.githubusercontent.com/mastercomfig/hud-db/main/hud-resources"
    );
    let e = Endpoints::loopback("http://127.0.0.1:9");
    assert_eq!(e.hud_resources(), "http://127.0.0.1:9/img/hud-resources");
}

#[tokio::test]
async fn a_warm_cache_serves_a_loopback_curated_image_with_no_server() {
    // Port 9 (discard) is closed, so only the cache can answer.
    let env = store_env().await;
    sql(&env.dir)
        .execute(
            "INSERT INTO store_huds (hud_id, name, author, data_json, fetched_at) \
             VALUES ('rayshud', 'rayshud', 'raysfire', '{\"resources\":[\"r1\"]}', 'x')",
            [],
        )
        .unwrap();
    let client = offline_client(&env, Endpoints::loopback("http://127.0.0.1:9"));
    let item = client
        .store_items(StoreKind::CuratedHuds)
        .unwrap()
        .remove(0);
    let url = item.images()[0].clone();
    assert_eq!(url, "http://127.0.0.1:9/img/hud-resources/rayshud/r1.webp");
    write_file(&cache_file(&env, &url), b"cached webp");
    assert_eq!(client.fetch_image(&url).await.unwrap(), b"cached webp");
}
