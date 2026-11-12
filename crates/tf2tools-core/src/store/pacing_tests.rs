//! Which priority each GameBanana caller takes through the shared pacer.

use std::time::Duration;

use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::pacer::Priority;
use super::{Endpoints, StoreClient, StoreConfig, StoreItemId, StoreKind};
use crate::GbId;
use crate::db::Database;
use crate::paths::DataDir;

fn fixture(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/store")
        .join(name);
    std::fs::read(&path).unwrap()
}

fn client(server: &MockServer, tmp: &tempfile::TempDir) -> StoreClient {
    let dir = DataDir::at(tmp.path().join("data"));
    let db = Database::open(&dir).unwrap();
    let endpoints = Endpoints::default().with_gamebanana(format!("{}/apiv11", server.uri()));
    let config = StoreConfig::default()
        .with_request_spacing(Duration::ZERO)
        .with_retry_backoff(Duration::from_millis(1));
    StoreClient::new(db, dir, endpoints).with_config(config)
}

#[tokio::test]
async fn a_file_listing_is_interactive() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/apiv11/Sound/53978"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(fixture("gb-sound-files.json")))
        .mount(&server)
        .await;
    let tmp = tempfile::tempdir().unwrap();
    let client = client(&server, &tmp);
    let item = StoreItemId::GameBanana(GbId::new(53978));
    client.item_files(StoreKind::Sounds, &item).await.unwrap();
    let stats = client.pacer.stats();
    assert_eq!((stats.interactive, stats.background), (1, 0));
}

#[tokio::test]
async fn sync_pages_are_background() {
    let server = MockServer::start().await;
    for (page, name) in [("1", "gb-sounds-page1.json"), ("2", "gb-sounds-last.json")] {
        Mock::given(method("GET"))
            .and(path("/apiv11/Sound/Index"))
            .and(query_param("_nPage", page))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(fixture(name)))
            .mount(&server)
            .await;
    }
    let tmp = tempfile::tempdir().unwrap();
    let client = client(&server, &tmp);
    client.sync(StoreKind::Sounds, true, &|_| {}).await.unwrap();
    let stats = client.pacer.stats();
    assert_eq!((stats.interactive, stats.background), (0, 2));
}

#[tokio::test]
async fn a_rate_limit_on_a_gamebanana_call_widens_both_pacer_lanes() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/apiv11/Sound/53978"))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/apiv11/Sound/53978"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(fixture("gb-sound-files.json")))
        .mount(&server)
        .await;
    let tmp = tempfile::tempdir().unwrap();
    let spacing = Duration::from_millis(1);
    let client = client(&server, &tmp).with_config(
        StoreConfig::default()
            .with_request_spacing(spacing)
            .with_retry_backoff(Duration::from_millis(1)),
    );
    let item = StoreItemId::GameBanana(GbId::new(53978));
    client.item_files(StoreKind::Sounds, &item).await.unwrap();
    for priority in [Priority::Interactive, Priority::Background] {
        assert_eq!(client.pacer.gap_of(priority), spacing * 2, "{priority:?}");
    }
}
