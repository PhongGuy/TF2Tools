//! The pilot's logs.tf stand-in, read through core's own clients.

use std::io::{Read as _, Write as _};
use std::net::TcpStream;
use std::time::Duration;

use tf2tools_core::{
    DataDir, Database, DemoQuery, DemosClient, DemosEndpoints, LogsCache, LogsCancel, LogsClient,
    LogsEndpoints, LogsRange, PlayerInput, SteamId, TrackedPlayer,
};
use tf2tools_pilot::demos_fake::{DemosFake, SEEDED_MAP};
use tf2tools_pilot::logs_fake::{LogsFake, NEWEST_LOG, PLAYERS, UNMATCHED_MAP};

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn pilot_a() -> SteamId {
    SteamId::from_steam3(PLAYERS[0].0).unwrap()
}

// A client on a fresh database, tracking Pilot A, with no request spacing.
fn client(tmp: &tempfile::TempDir) -> (LogsClient, LogsCache) {
    let fake = LogsFake::start().unwrap();
    let db = Database::open(&DataDir::at(tmp.path().join("data"))).unwrap();
    let cache = LogsCache::new(db);
    cache
        .add_player(&TrackedPlayer::new(pilot_a(), "Pilot A"))
        .unwrap();
    let client = LogsClient::new(cache.clone(), LogsEndpoints::loopback(fake.base())).with_config(
        LogsClient::default_config()
            .with_request_spacing(Duration::ZERO)
            .with_loopback_only(true),
    );
    (client, cache)
}

fn demos() -> DemosClient {
    let fake = DemosFake::start().unwrap();
    DemosClient::new(DemosEndpoints::loopback(fake.base())).with_config(
        DemosClient::default_config()
            .with_request_spacing(Duration::ZERO)
            .with_loopback_only(true),
    )
}

#[test]
fn a_sync_lists_and_downloads_the_three_fake_logs() {
    let tmp = tempfile::tempdir().unwrap();
    let (client, cache) = client(&tmp);
    let summary = runtime()
        .block_on(client.sync_player(pilot_a(), LogsRange::All, true, &LogsCancel::new(), &|_| {}))
        .unwrap();
    assert_eq!(summary.listed(), 3);
    assert_eq!(summary.downloaded(), 3);
    let ids: Vec<u64> = cache
        .logs(pilot_a(), LogsRange::All)
        .unwrap()
        .iter()
        .map(|row| row.id())
        .collect();
    assert_eq!(ids, [7003, 7002, 7001]);
}

#[test]
fn a_fetched_log_has_its_map_and_every_player() {
    let tmp = tempfile::tempdir().unwrap();
    let (client, _) = client(&tmp);
    let log = runtime().block_on(client.fetch_log(NEWEST_LOG)).unwrap();
    assert_eq!(log.map(), SEEDED_MAP);
    assert_eq!(log.length(), 1800);
    assert_eq!(log.players().len(), 4);
    for player in log.players() {
        assert!(
            PLAYERS.iter().any(|(_, name)| *name == player.name()),
            "{}",
            player.name()
        );
        assert_eq!(player.classes().len(), 2, "{}", player.name());
    }
}

#[test]
fn the_steam_lookup_returns_the_fake_persona() {
    let tmp = tempfile::tempdir().unwrap();
    let (client, _) = client(&tmp);
    let player = runtime()
        .block_on(client.resolve_player(&PlayerInput::Id(pilot_a())))
        .unwrap();
    assert_eq!(player.steam_id(), pilot_a());
    assert_eq!(player.name(), "Pilot A");
}

#[test]
fn anything_else_is_not_found() {
    let fake = LogsFake::start().unwrap();
    let addr = fake
        .base()
        .strip_prefix("http://")
        .and_then(|rest| rest.strip_suffix("/logstf"))
        .unwrap()
        .to_owned();
    for path in [
        "/logstf/logs/log_7003.log.zip",
        "/logstf/json/9999",
        "/logstf/nope",
        "/nope",
    ] {
        let mut stream = TcpStream::connect(&addr).unwrap();
        write!(stream, "GET {path} HTTP/1.1\r\nHost: {addr}\r\n\r\n").unwrap();
        let mut reply = String::new();
        stream.read_to_string(&mut reply).unwrap();
        assert!(reply.starts_with("HTTP/1.1 404"), "{path}: {reply}");
    }
}

#[test]
fn the_newest_log_finds_its_demo_and_the_unmatched_one_does_not() {
    let tmp = tempfile::tempdir().unwrap();
    let (client, _) = client(&tmp);
    let demos = demos();
    let rt = runtime();
    let newest = rt.block_on(client.fetch_log(NEWEST_LOG)).unwrap();
    let query = DemoQuery::from_log(&newest).expect("the newest log is searchable");
    let found = rt.block_on(demos.find_demo(&query)).unwrap();
    assert_eq!(found.map(|d| d.id()), Some(9006));

    let unmatched = rt.block_on(client.fetch_log(7002)).unwrap();
    assert_eq!(unmatched.map(), UNMATCHED_MAP);
    let query = DemoQuery::from_log(&unmatched).unwrap();
    assert_eq!(rt.block_on(demos.find_demo(&query)).unwrap(), None);

    let old = rt.block_on(client.fetch_log(7001)).unwrap();
    let query = DemoQuery::from_log(&old).unwrap();
    assert_eq!(rt.block_on(demos.find_demo(&query)).unwrap(), None);
}
