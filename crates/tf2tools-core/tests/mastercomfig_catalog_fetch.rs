//! mastercomfig's module catalog fetched from GitHub once per release and kept on disk.

use crate::common;

use common::{MC_MODULES, MC_PRESET_MODULES, StoreEnv, fast_config, store_env};
use std::path::Path;
use tf2tools_core::{DataDir, Database, Endpoints, ModuleCatalogError, Preset, StoreClient};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

const DATA: &str = "/gh-raw/mastercomfig";

async fn serve(env: &StoreEnv, tag: &str, modules: &[u8], presets: &[u8]) {
    for (file, body) in [("modules.json", modules), ("preset_modules.json", presets)] {
        Mock::given(method("GET"))
            .and(path(format!("{DATA}/{tag}/data/{file}")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body.to_vec()))
            .mount(&env.server)
            .await;
    }
}

async fn requests(env: &StoreEnv) -> usize {
    env.server.received_requests().await.unwrap().len()
}

#[test]
fn the_data_base_and_cache_folder_have_fixed_homes() {
    assert_eq!(
        Endpoints::default().mastercomfig_data(),
        "https://raw.githubusercontent.com/mastercomfig/mastercomfig"
    );
    assert_eq!(
        Endpoints::loopback("http://x").mastercomfig_data(),
        "http://x/gh-raw/mastercomfig"
    );
    assert_eq!(
        DataDir::at("/r").mastercomfig_dir(),
        Path::new("/r/mastercomfig")
    );
}

#[tokio::test]
async fn the_catalog_is_fetched_once_per_tag_and_then_read_from_disk() {
    let env = store_env().await;
    serve(&env, "9.100.1", MC_MODULES, MC_PRESET_MODULES).await;
    assert!(env.client.cached_mastercomfig_catalog("9.100.1").is_none());
    let catalog = env.client.mastercomfig_catalog("9.100.1").await.unwrap();
    assert_eq!(catalog.resolve("lod", Preset::High), Some("high"));
    assert_eq!(requests(&env).await, 2);
    let again = env.client.mastercomfig_catalog("9.100.1").await.unwrap();
    assert_eq!(again, catalog);
    assert_eq!(requests(&env).await, 2, "the second load reads the disk");
    assert_eq!(
        env.client.cached_mastercomfig_catalog("9.100.1"),
        Some(catalog)
    );
    let saved = env.dir.mastercomfig_dir().join("9.100.1");
    assert_eq!(
        std::fs::read(saved.join("modules.json")).unwrap(),
        MC_MODULES
    );
    assert!(
        env.client.cached_mastercomfig_catalog("9.101.0").is_none(),
        "per tag"
    );
}

#[tokio::test]
async fn a_corrupt_cached_catalog_is_fetched_again_and_overwritten() {
    let env = store_env().await;
    serve(&env, "9.100.1", MC_MODULES, MC_PRESET_MODULES).await;
    let saved = env.dir.mastercomfig_dir().join("9.100.1");
    std::fs::create_dir_all(&saved).unwrap();
    std::fs::write(saved.join("modules.json"), b"{ not json").unwrap();
    std::fs::write(saved.join("preset_modules.json"), MC_PRESET_MODULES).unwrap();
    assert!(env.client.cached_mastercomfig_catalog("9.100.1").is_none());
    let catalog = env.client.mastercomfig_catalog("9.100.1").await.unwrap();
    assert_eq!(catalog.resolve("lod", Preset::High), Some("high"));
    assert_eq!(requests(&env).await, 2);
    assert_eq!(
        std::fs::read(saved.join("modules.json")).unwrap(),
        MC_MODULES
    );
}

#[tokio::test]
async fn an_unreadable_catalog_is_a_format_error_and_is_not_saved() {
    let env = store_env().await;
    serve(&env, "9.100.1", br#"{"graphics":{}}"#, MC_PRESET_MODULES).await;
    let err = env
        .client
        .mastercomfig_catalog("9.100.1")
        .await
        .unwrap_err();
    assert!(matches!(err, ModuleCatalogError::Format { .. }), "{err:?}");
    assert!(!env.dir.mastercomfig_dir().join("9.100.1").exists());
}

#[tokio::test]
async fn a_tag_github_does_not_have_is_an_http_error() {
    let env = store_env().await;
    let err = env
        .client
        .mastercomfig_catalog("unknown")
        .await
        .unwrap_err();
    assert!(
        matches!(err, ModuleCatalogError::Http { status: 404, .. }),
        "{err:?}"
    );
}

#[tokio::test]
async fn a_tag_that_is_not_a_plain_version_is_refused_without_a_request() {
    let env = store_env().await;
    for tag in ["../9.100.1", "", "9.100.1/x", ".hidden"] {
        let err = env.client.mastercomfig_catalog(tag).await.unwrap_err();
        assert!(
            matches!(err, ModuleCatalogError::UnsupportedTag { .. }),
            "{tag}: {err:?}"
        );
        assert!(env.client.cached_mastercomfig_catalog(tag).is_none());
    }
    assert_eq!(requests(&env).await, 0);
}

#[tokio::test]
async fn an_unreachable_server_is_a_network_error() {
    let env = store_env().await;
    let db = Database::open(&env.dir).unwrap();
    let endpoints = Endpoints::default().with_mastercomfig_data("http://127.0.0.1:9");
    let client = StoreClient::new(db, env.dir.clone(), endpoints).with_config(fast_config());
    let err = client.mastercomfig_catalog("9.100.1").await.unwrap_err();
    assert!(matches!(err, ModuleCatalogError::Network { .. }), "{err:?}");
}
