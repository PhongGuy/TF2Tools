//! Resolving a Steam profile through its public XML, online and offline.

use tf2tools_core::{
    Database, LogsCache, LogsClient, LogsEndpoints, PlayerInput, ResolveError, SteamId,
    parse_player_input,
};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

use crate::common;
use crate::common::logstf::{FOUND_XML, NOT_FOUND_XML, logs_env};

const GABE: u64 = 76_561_197_960_287_930;

async fn serve_xml(server: &wiremock::MockServer, route: &str, body: &str) {
    Mock::given(method("GET"))
        .and(path(route))
        .and(query_param("xml", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(server)
        .await;
}

#[tokio::test]
async fn a_vanity_resolves_to_its_id_and_persona_name() {
    let env = logs_env().await;
    serve_xml(&env.server, "/steam/id/gabelogannewell/", FOUND_XML).await;
    let input = parse_player_input("https://steamcommunity.com/id/gabelogannewell").unwrap();
    let player = env.client.resolve_player(&input).await.unwrap();
    assert_eq!(player.steam_id().id64(), GABE);
    assert_eq!(player.name(), "Rabscuttle");
}

#[tokio::test]
async fn an_id_resolves_through_its_profiles_page() {
    let env = logs_env().await;
    serve_xml(&env.server, "/steam/profiles/76561197960287930/", FOUND_XML).await;
    let input = PlayerInput::Id(SteamId::from_id64(GABE).unwrap());
    let player = env.client.resolve_player(&input).await.unwrap();
    assert_eq!(player.name(), "Rabscuttle");
}

#[tokio::test]
async fn an_error_body_is_not_found() {
    let env = logs_env().await;
    serve_xml(&env.server, "/steam/id/nobody9z8y7x/", NOT_FOUND_XML).await;
    let input = PlayerInput::Vanity("nobody9z8y7x".into());
    let err = env.client.resolve_player(&input).await.unwrap_err();
    assert!(matches!(err, ResolveError::NotFound), "{err:?}");
    assert_eq!(err.to_string(), "We couldn't find that Steam profile.");
}

#[tokio::test]
async fn a_body_with_neither_id_nor_error_is_malformed() {
    let env = logs_env().await;
    serve_xml(&env.server, "/steam/id/weird/", "<profile></profile>").await;
    let err = env
        .client
        .resolve_player(&PlayerInput::Vanity("weird".into()))
        .await
        .unwrap_err();
    assert!(matches!(err, ResolveError::Malformed { .. }), "{err:?}");
}

/// A client whose every request goes to a closed loopback port.
fn unreachable_client() -> (tempfile::TempDir, LogsClient) {
    let (tmp, dir) = common::temp_data_dir();
    let db = Database::open(&dir).unwrap();
    let client = LogsClient::new(
        LogsCache::new(db),
        LogsEndpoints::loopback("http://127.0.0.1:9"),
    )
    .with_config(common::fast_config());
    (tmp, client)
}

#[tokio::test]
async fn offline_an_id_is_added_under_its_id64() {
    let (_tmp, client) = unreachable_client();
    let input = PlayerInput::Id(SteamId::from_id64(GABE).unwrap());
    let player = client.resolve_player(&input).await.unwrap();
    assert_eq!(player.steam_id().id64(), GABE);
    assert_eq!(player.name(), "76561197960287930");
}

#[tokio::test]
async fn offline_a_vanity_cannot_be_added() {
    let (_tmp, client) = unreachable_client();
    let err = client
        .resolve_player(&PlayerInput::Vanity("gabelogannewell".into()))
        .await
        .unwrap_err();
    assert!(matches!(err, ResolveError::Network { .. }), "{err:?}");
}
