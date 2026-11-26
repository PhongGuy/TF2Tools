//! The pilot's demos.tf stand-in, read through core's own client.

use std::io::{Read as _, Write as _};
use std::net::TcpStream;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tf2tools_app::app::CLOSED_BASE;
use tf2tools_core::{
    DemoCursor, DemoListQuery, DemoSummary, DemosClient, DemosEndpoints, DemosError, DemosRange,
    Endpoints,
};
use tf2tools_pilot::demos_fake::{DemosFake, MISSING_MAP, SEEDED_MAP, seed_map};

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn client_on(endpoints: DemosEndpoints) -> DemosClient {
    DemosClient::new(endpoints).with_config(
        DemosClient::default_config()
            .with_request_spacing(Duration::ZERO)
            .with_loopback_only(true),
    )
}

fn client(fake: &DemosFake) -> DemosClient {
    client_on(DemosEndpoints::loopback(fake.base()))
}

fn query() -> DemoListQuery {
    DemoListQuery::new(DemosRange::Week1, now())
}

fn first_page(client: &DemosClient) -> Vec<DemoSummary> {
    runtime()
        .block_on(client.list(&query(), DemoCursor::First))
        .unwrap()
        .demos()
        .to_vec()
}

#[test]
fn the_list_parses_through_cores_client_with_every_case_the_page_shows() {
    let fake = DemosFake::start().unwrap();
    let demos = first_page(&client(&fake));
    let ids: Vec<u64> = demos.iter().map(DemoSummary::id).collect();
    assert_eq!(ids, [9006, 9005, 9004, 9003, 9002, 9001]);

    let freezer = format!("{}/freezer/", fake.base());
    assert_eq!(
        demos
            .iter()
            .filter(|d| d.url().starts_with(&freezer))
            .count(),
        1
    );
    assert_eq!(
        demos.iter().filter(|d| d.url().is_empty()).count(),
        1,
        "one private row"
    );
    assert_eq!(demos.iter().filter(|d| d.map() == MISSING_MAP).count(), 1);
    assert!(demos.iter().any(|d| d.map() == SEEDED_MAP));

    let reach = now() - DemosRange::Week1.reach_secs().unwrap();
    for d in &demos {
        assert!(
            d.time() > reach && d.time() <= now(),
            "{}: {}",
            d.id(),
            d.time()
        );
        assert!(d.duration() > 0 && d.player_count() > 0, "{}", d.id());
    }
}

#[test]
fn every_public_url_downloads_a_header_naming_its_map() {
    let fake = DemosFake::start().unwrap();
    let client = client(&fake);
    let dir = tempfile::tempdir().unwrap();
    let rt = runtime();
    let public: Vec<DemoSummary> = first_page(&client)
        .into_iter()
        .filter(|d| !d.url().is_empty())
        .collect();
    assert_eq!(public.len(), 5);
    for demo in &public {
        let dest = dir.path().join(format!("{}.dem", demo.id()));
        rt.block_on(client.download_demo(demo, &dest, &|_, _| {}))
            .unwrap_or_else(|e| panic!("{}: {e}", demo.url()));
        let bytes = std::fs::read(&dest).unwrap();
        assert_eq!(bytes.len(), 1072, "{}", demo.url());
        assert!(bytes.starts_with(b"HL2DEMO\0"));
        // demo_header lays out stamp (8), two i32s (8), then 260-byte server and player fields.
        assert_eq!(&bytes[536..536 + demo.map().len()], demo.map().as_bytes());
    }
}

#[test]
fn paging_past_the_last_row_ends_the_list() {
    let fake = DemosFake::start().unwrap();
    let client = client(&fake);
    let rt = runtime();
    let first = rt
        .block_on(client.list(&query(), DemoCursor::First))
        .unwrap();
    assert_eq!(first.next(), Some(DemoCursor::BeforeId(9001)));

    let tail = rt
        .block_on(client.list(&query(), DemoCursor::BeforeId(9004)))
        .unwrap();
    let ids: Vec<u64> = tail.demos().iter().map(DemoSummary::id).collect();
    assert_eq!(ids, [9003, 9002, 9001]);

    let rest = rt
        .block_on(client.list(&query(), DemoCursor::BeforeId(9001)))
        .unwrap();
    assert!(rest.demos().is_empty());
    assert_eq!(rest.next(), None);
}

#[test]
fn anything_else_is_not_found() {
    let fake = DemosFake::start().unwrap();
    let rt = runtime();
    assert!(matches!(
        rt.block_on(client(&fake).demo(9006)),
        Err(DemosError::Gone)
    ));

    let addr = fake
        .base()
        .trim_start_matches("http://")
        .trim_end_matches("/demostf")
        .to_owned();
    for path in ["/nope", "/demostf/files/1.dem", "/demostf/files/9003.dem"] {
        let mut stream = TcpStream::connect(&addr).unwrap();
        write!(stream, "GET {path} HTTP/1.1\r\nHost: {addr}\r\n\r\n").unwrap();
        let mut reply = String::new();
        stream.read_to_string(&mut reply).unwrap();
        assert!(reply.starts_with("HTTP/1.1 404"), "{path}: {reply}");
    }
}

#[test]
fn seed_map_writes_the_seeded_bsp_only() {
    let game = tempfile::tempdir().unwrap();
    seed_map(game.path()).unwrap();
    let maps = game.path().join("tf/maps");
    let bsp = std::fs::read(maps.join(format!("{SEEDED_MAP}.bsp"))).unwrap();
    assert!(bsp.starts_with(b"VBSP"));
    assert!(!maps.join(format!("{MISSING_MAP}.bsp")).exists());
}

#[test]
fn the_seeded_boot_lists_demos_from_the_fake() {
    let tmp = tempfile::tempdir().unwrap();
    let boot = tf2tools_pilot::seeded_boot(&tmp.path().join("com.tf2tools.app")).unwrap();
    assert!(boot.offline);
    let closed = Endpoints::loopback(CLOSED_BASE);
    assert_eq!(boot.endpoints.gamebanana(), closed.gamebanana());
    let api = boot.endpoints.demos().api();
    assert!(
        api.starts_with("http://127.0.0.1:") && api.ends_with("/demostf"),
        "{api}"
    );
    assert_ne!(boot.endpoints.demos(), closed.demos());
    assert_eq!(
        first_page(&client_on(boot.endpoints.demos().clone())).len(),
        6
    );
}

#[test]
fn every_start_shares_one_listener_and_one_clock() {
    let a = DemosFake::start().unwrap();
    let b = DemosFake::start().unwrap();
    assert_eq!(a.base(), b.base());
    assert_eq!(
        a.base(),
        format!("{}/demostf", tf2tools_pilot::fake_http::root().unwrap())
    );

    let start = tf2tools_pilot::fake_http::start_time();
    assert_eq!(start, tf2tools_pilot::fake_http::start_time());
    assert!(start <= now() && start > now() - 3600, "{start}");

    // The newest row is anchored to the shared clock, not to the request's own now.
    let demos = first_page(&client(&a));
    assert_eq!(demos[0].id(), 9006);
    assert_eq!(demos[0].time(), start - 3600);
}
