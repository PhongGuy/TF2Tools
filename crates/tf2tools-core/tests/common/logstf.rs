//! A logs.tf client against wiremock, and the recorded logs.tf bodies.

use std::path::Path;

use serde_json::json;
use tempfile::TempDir;
use tf2tools_core::{DataDir, Database, LogsCache, LogsClient, LogsEndpoints};
use wiremock::matchers::{method, path, path_regex, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub struct LogsEnv {
    pub tmp: TempDir,
    pub dir: DataDir,
    pub server: MockServer,
    pub client: LogsClient,
}

/// A fresh database and a client pointed at a new wiremock server, without spacing.
pub async fn logs_env() -> LogsEnv {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = DataDir::at(tmp.path().join("com.tf2tools.app"));
    let db = Database::open(&dir).expect("open database");
    let server = MockServer::start().await;
    let client = LogsClient::new(LogsCache::new(db), LogsEndpoints::loopback(&server.uri()))
        .with_config(super::fast_config());
    LogsEnv {
        tmp,
        dir,
        server,
        client,
    }
}

/// A recorded body from `tests/fixtures/logstf/`.
pub fn logstf_fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/logstf")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

pub const FOUND_XML: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
    <profile><steamID64>76561197960287930</steamID64><steamID><![CDATA[Rabscuttle]]></steamID>\
    <onlineState>offline</onlineState></profile>";
pub const NOT_FOUND_XML: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
    <response><error><![CDATA[The specified profile could not be found.]]></error></response>";

/// One class_stats entry: (type, seconds, kills, deaths, assists, dmg).
pub type ClassSpec = (&'static str, u32, u32, u32, u32, u32);

/// One player of a synthetic log.
pub struct PlayerSpec {
    pub account: u64,
    pub name: &'static str,
    pub team: &'static str,
    pub heal: u32,
    pub ubers: u32,
    pub drops: u32,
    pub classes: Vec<ClassSpec>,
}

/// A minimal log body `parse_log` accepts, `length` seconds long.
pub fn log_body(length: u32, players: &[PlayerSpec]) -> String {
    let mut map = serde_json::Map::new();
    let mut names = serde_json::Map::new();
    for p in players {
        let key = format!("[U:1:{}]", p.account);
        let (k, d, a, dmg) = p.classes.iter().fold((0, 0, 0, 0), |t, c| {
            (t.0 + c.2, t.1 + c.3, t.2 + c.4, t.3 + c.5)
        });
        let stats: Vec<_> = p
            .classes
            .iter()
            .map(|&(ty, secs, k, d, a, dmg)| {
                json!({"type": ty, "total_time": secs, "kills": k, "deaths": d,
                       "assists": a, "dmg": dmg})
            })
            .collect();
        map.insert(
            key.clone(),
            json!({"team": p.team, "kills": k, "deaths": d, "assists": a, "dmg": dmg,
                   "heal": p.heal, "ubers": p.ubers, "drops": p.drops, "class_stats": stats}),
        );
        names.insert(key, json!(p.name));
    }
    json!({"version": 3, "length": length, "teams": {"Red": {}, "Blue": {}},
           "players": map, "names": names, "info": {}})
    .to_string()
}

/// A one-class player on Red.
pub fn solo(account: u64, class: ClassSpec) -> PlayerSpec {
    PlayerSpec {
        account,
        name: "solo",
        team: "Red",
        heal: 0,
        ubers: 0,
        drops: 0,
        classes: vec![class],
    }
}

pub const ARIE: u64 = 76_561_197_960_497_430;

pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

pub fn days_ago(days: i64) -> i64 {
    now() - days * 86_400
}

/// A list response holding `logs` as (id, upload date), in the order given.
pub fn list_body(logs: &[(u64, i64)]) -> String {
    let logs: Vec<_> = logs
        .iter()
        .map(|&(id, date)| {
            json!({"id": id, "title": format!("Log {id}"), "map": "cp_process_f12",
                   "date": date, "views": 0, "players": 12})
        })
        .collect();
    json!({"success": true, "results": logs.len(), "total": logs.len(),
           "parameters": {}, "logs": logs})
    .to_string()
}

pub async fn mount_list(server: &wiremock::MockServer, player: u64, body: String) {
    Mock::given(method("GET"))
        .and(path("/api/v1/log"))
        .and(query_param("player", player.to_string()))
        .and(query_param("limit", "1000"))
        .and(query_param("offset", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(server)
        .await;
}

/// Answers every `/json/<id>` with the same one-scout log.
pub async fn mount_any_log(server: &wiremock::MockServer) {
    Mock::given(method("GET"))
        .and(path_regex(r"^/json/\d+$"))
        .respond_with(ResponseTemplate::new(200).set_body_string(log_body(
            600,
            &[solo(231_702, ("scout", 600, 1, 1, 1, 100))],
        )))
        .mount(server)
        .await;
}

/// The paths the server was asked for, in order.
pub async fn paths(server: &wiremock::MockServer) -> Vec<String> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .map(|r| r.url.path().to_owned())
        .collect()
}
