//! The demos.tf client against a mock server: what it asks and how answers map.

use std::time::Duration;

use serde_json::json;
use wiremock::matchers::{method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::{DemosClient, DemosEndpoints};
use crate::demostf::{DemoCursor, DemoFormat, DemoListQuery, DemoQuery, DemosRange};
use crate::error::DemosError;
use crate::logstf::{LOGS_SPACING, STEAM_ID64_BASE, SteamId};
use crate::store::StoreConfig;

const MAP: &str = "cp_process_f12";
const DATE: i64 = 1_795_296_722;

fn query() -> DemoQuery {
    let sid = |a: u64| SteamId::from_id64(STEAM_ID64_BASE + a).unwrap();
    DemoQuery {
        map: MAP.to_owned(),
        date: DATE,
        length: 1800,
        players: vec![sid(1), sid(2)],
    }
}

fn demo_json(id: u64, base: &str) -> serde_json::Value {
    json!({"id": id, "url": format!("{base}/files/aa/{id}.dem"), "name": format!("match-{id}.dem"),
           "server": "serveme.tf #1", "duration": 1790, "map": MAP, "time": DATE + 60,
           "playerCount": 12})
}

async fn env() -> (MockServer, DemosClient) {
    let server = MockServer::start().await;
    let config = StoreConfig::default()
        .with_request_spacing(Duration::ZERO)
        .with_retry_backoff(Duration::from_millis(1));
    let client = DemosClient::new(DemosEndpoints::loopback(&server.uri())).with_config(config);
    (server, client)
}

#[test]
fn the_endpoints_name_demos_tf_or_one_loopback_base() {
    let live = DemosEndpoints::default();
    assert_eq!(live.api(), "https://api.demos.tf");
    assert_eq!(live.site(), "https://demos.tf");
    assert_eq!(
        live.files(),
        ["https://static.demos.tf", "https://freezer.demos.tf"]
    );
    assert_eq!(live.demo_page(1_536_965), "https://demos.tf/1536965");
    let local = DemosEndpoints::loopback("http://127.0.0.1:5/");
    assert_eq!(local.api(), "http://127.0.0.1:5");
    assert_eq!(local.site(), "http://127.0.0.1:5/site");
    assert_eq!(
        local.files(),
        ["http://127.0.0.1:5/files", "http://127.0.0.1:5/freezer"]
    );
    assert_eq!(
        DemosClient::default_config().request_spacing(),
        LOGS_SPACING
    );
}

#[tokio::test]
async fn find_demo_asks_for_the_map_both_players_and_two_hours_around_the_log() {
    let (server, client) = env().await;
    let players = format!("{},{}", STEAM_ID64_BASE + 1, STEAM_ID64_BASE + 2);
    Mock::given(method("GET"))
        .and(path("/demos/"))
        .and(query_param("map", MAP))
        .and(query_param("players", players.as_str()))
        .and(query_param("after", (DATE - 3600).to_string().as_str()))
        .and(query_param("before", (DATE + 3600).to_string().as_str()))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!([demo_json(7, &server.uri())])),
        )
        .expect(1)
        .mount(&server)
        .await;
    let found = client.find_demo(&query()).await.unwrap().unwrap();
    assert_eq!(found.id(), 7);
    assert_eq!(found.url(), format!("{}/files/aa/7.dem", server.uri()));
}

#[tokio::test]
async fn an_empty_answer_is_no_demo() {
    let (server, client) = env().await;
    Mock::given(method("GET"))
        .and(path("/demos/"))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .mount(&server)
        .await;
    assert_eq!(client.find_demo(&query()).await.unwrap(), None);
}

#[tokio::test]
async fn lookup_failures_keep_their_cause() {
    for (status, body) in [(200, "<html>"), (500, ""), (429, "")] {
        let (server, client) = env().await;
        Mock::given(method("GET"))
            .and(path("/demos/"))
            .respond_with(ResponseTemplate::new(status).set_body_string(body))
            .mount(&server)
            .await;
        let failed = client.find_demo(&query()).await.unwrap_err();
        match status {
            200 => assert!(matches!(failed, DemosError::Unreadable), "{failed:?}"),
            500 => assert!(
                matches!(failed, DemosError::Http { status: 500, .. }),
                "{failed:?}"
            ),
            _ => assert!(
                matches!(failed, DemosError::RateLimited { .. }),
                "{failed:?}"
            ),
        }
    }
}

#[tokio::test]
async fn an_offline_client_never_leaves_loopback() {
    let client = DemosClient::new(DemosEndpoints::default())
        .with_config(DemosClient::default_config().with_loopback_only(true));
    assert!(client.is_loopback_only());
    assert!(!DemosClient::new(DemosEndpoints::default()).is_loopback_only());
    let failed = client.find_demo(&query()).await.unwrap_err();
    assert!(
        matches!(&failed, DemosError::Network { message, .. } if message.starts_with("offline")),
        "{failed:?}"
    );
}

#[tokio::test]
async fn demo_reads_one_demo_with_its_players() {
    let (server, client) = env().await;
    let mut body = demo_json(7, &server.uri());
    body["players"] = json!([
        {"steamid": (STEAM_ID64_BASE + 1).to_string(), "name": "a", "team": "red", "class": "medic"},
        {"steamid": (STEAM_ID64_BASE + 2).to_string(), "name": "b", "team": "blue", "class": "scout"}
    ]);
    Mock::given(method("GET"))
        .and(path("/demos/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;
    let detail = client.demo(7).await.unwrap();
    assert_eq!(detail.summary().id(), 7);
    let names: Vec<&str> = detail.players().iter().map(|p| p.name()).collect();
    assert_eq!(names, ["a", "b"]);
}

#[tokio::test]
async fn a_missing_demo_is_gone_and_a_strange_one_unreadable() {
    let (server, client) = env().await;
    Mock::given(method("GET"))
        .and(path("/demos/8"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/demos/9"))
        .respond_with(ResponseTemplate::new(200).set_body_string("nope"))
        .mount(&server)
        .await;
    assert!(matches!(client.demo(8).await, Err(DemosError::Gone)));
    assert!(matches!(client.demo(9).await, Err(DemosError::Unreadable)));
}

#[tokio::test]
async fn a_list_asks_with_its_filters_and_returns_the_next_cursor() {
    let (server, client) = env().await;
    let base = server.uri();
    Mock::given(method("GET"))
        .and(path("/demos/"))
        .and(query_param("map", MAP))
        .and(query_param("type", "6v6"))
        .and(query_param("before_id", "900"))
        .and(query_param_is_missing("page"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!([demo_json(899, &base), demo_json(850, &base)])),
        )
        .expect(1)
        .mount(&server)
        .await;
    let q = DemoListQuery::new(DemosRange::All, DATE)
        .with_map(MAP)
        .with_format(Some(DemoFormat::Sixes));
    let page = client.list(&q, DemoCursor::BeforeId(900)).await.unwrap();
    assert_eq!(
        page.demos().iter().map(|d| d.id()).collect::<Vec<_>>(),
        [899, 850]
    );
    assert_eq!(page.next(), Some(DemoCursor::BeforeId(850)));
}

#[tokio::test]
async fn a_format_filtered_page_keeps_paging_when_nothing_matches() {
    let (server, client) = env().await;
    let base = server.uri();
    Mock::given(method("GET"))
        .and(path("/demos/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([demo_json(40, &base)])))
        .mount(&server)
        .await;
    let q = DemoListQuery::new(DemosRange::All, DATE).with_format(Some(DemoFormat::BBall));
    let page = client.list(&q, DemoCursor::First).await.unwrap();
    assert!(page.demos().is_empty());
    assert_eq!(page.next(), Some(DemoCursor::BeforeId(40)));
}

#[tokio::test]
async fn a_players_list_pages_by_number_and_never_by_id() {
    let (server, client) = env().await;
    let base = server.uri();
    Mock::given(method("GET"))
        .and(path("/demos/"))
        .and(query_param("page", "2"))
        .and(query_param_is_missing("before_id"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([demo_json(7, &base)])))
        .expect(1)
        .mount(&server)
        .await;
    let q = DemoListQuery::new(DemosRange::All, DATE).with_players(&query().players);
    assert_eq!(
        client.list(&q, DemoCursor::Page(2)).await.unwrap().next(),
        Some(DemoCursor::Page(3))
    );
    assert!(matches!(
        client.list(&q, DemoCursor::BeforeId(7)).await,
        Err(DemosError::Cursor)
    ));
}

#[tokio::test]
async fn an_unreadable_list_is_unreadable() {
    let (server, client) = env().await;
    Mock::given(method("GET"))
        .and(path("/demos/"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string("{\"error\":\"column id is ambiguous\"}"),
        )
        .mount(&server)
        .await;
    let q = DemoListQuery::new(DemosRange::All, DATE);
    assert!(matches!(
        client.list(&q, DemoCursor::First).await,
        Err(DemosError::Unreadable)
    ));
}
