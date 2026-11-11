//! Shared logs.tf helpers: a shell over a mock logs.tf, its fixtures and page lookups.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use gpui_kit::{Entity, TestAppContext};
use serde_json::json;
use tf2tools_app::ids;
use tf2tools_app::pages::logs_tf::LogsTfPage;
use tf2tools_app::services::{Services, Tf2State};
use tf2tools_app::shell::route::Page;
use tf2tools_core::{
    LogsCache, LogsClient, LogsEndpoints, STEAM_ID64_BASE, SteamId, StoreConfig, TrackedPlayer,
};

use crate::support;

pub fn sid(account: u64) -> SteamId {
    SteamId::from_id64(STEAM_ID64_BASE + account).unwrap()
}

/// Opens logs.tf through the sidebar and waits for its first load and any sync to finish.
pub fn open_page(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<LogsTfPage> {
    support::click_id(cx, s.win, ids::nav(Page::LogsTf));
    let page = cx
        .update(|cx| s.shell.read(cx).logs_tf_page())
        .expect("a logs.tf page");
    support::wait_real(cx, "the logs.tf page", |cx| {
        cx.update(|cx| {
            let p = page.read(cx);
            p.is_loaded() && !p.is_syncing(cx)
        })
    });
    support::settle(cx, s.win);
    page
}

/// A logs client on `base` with no request spacing, over the services' own database.
pub fn logs_client(services: &Services, base: &str) -> LogsClient {
    LogsClient::new(
        LogsCache::new(services.db.clone()),
        LogsEndpoints::loopback(base),
    )
    .with_config(
        StoreConfig::default()
            .with_request_spacing(Duration::ZERO)
            .with_retry_backoff(Duration::from_millis(1)),
    )
}

/// A shell whose logs client talks to `base`, with `players` (account, name) tracked in order.
pub fn shell(
    cx: &mut TestAppContext,
    env: &support::Env,
    base: &str,
    players: &[(u64, &'static str)],
) -> support::ShellWindow {
    cx.executor().allow_parking();
    let base = base.to_owned();
    let players = players.to_vec();
    support::shell_window_tweaked(
        cx,
        env,
        Tf2State::Missing { saved: None },
        move |services| {
            let logs = logs_client(services, &base);
            for (account, name) in &players {
                logs.cache()
                    .add_player(&TrackedPlayer::new(sid(*account), *name))
                    .unwrap();
            }
            *services = services.clone().with_logs(logs);
        },
    )
}

/// A logs.tf list response holding (id, upload date) pairs.
pub fn list_body(logs: &[(u64, i64)]) -> Vec<u8> {
    let logs: Vec<_> = logs
        .iter()
        .map(|&(id, date)| {
            json!({"id": id, "title": format!("Log {id}"), "map": "cp_process_f12",
                   "date": date, "views": 0, "players": 12})
        })
        .collect();
    json!({"success": true, "results": logs.len(), "total": logs.len(), "parameters": {}, "logs": logs})
        .to_string()
        .into_bytes()
}

pub fn players_of(cx: &mut TestAppContext, page: &Entity<LogsTfPage>) -> Vec<String> {
    cx.update(|cx| {
        page.read(cx)
            .players()
            .iter()
            .map(|p| p.name().to_owned())
            .collect()
    })
}

pub const SIXES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../tf2tools-core/tests/fixtures/logstf/log-4137103-6v6.json"
));
pub const HIGHLANDER: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../tf2tools-core/tests/fixtures/logstf/log-4150405-hl.json"
));

pub const ARIE: u64 = 231_702;

pub fn unix_now() -> i64 {
    chrono::Utc::now().timestamp()
}

pub fn day_ago(days: i64) -> i64 {
    unix_now() - days * 86_400
}

/// Serves Arie's list of logs 3 (the 6v6 fixture) and 2 (the highlander fixture).
pub fn serve_two_logs(server: &support::StoreServer) {
    server.mount_bytes(
        "/api/v1/log",
        list_body(&[(3, day_ago(1)), (2, day_ago(2))]),
    );
    server.mount_bytes("/json/3", SIXES.as_bytes().to_vec());
    server.mount_bytes("/json/2", HIGHLANDER.as_bytes().to_vec());
}

pub fn row_ids(cx: &mut TestAppContext, page: &Entity<LogsTfPage>) -> Vec<u64> {
    cx.update(|cx| page.read(cx).rows().iter().map(|r| r.id()).collect())
}

pub fn expanded(cx: &mut TestAppContext, page: &Entity<LogsTfPage>) -> Option<u64> {
    cx.update(|cx| page.read(cx).expanded())
}

/// Scrolls the page to its last log, since the grid fills most of the window above the list.
pub fn to_end(cx: &mut TestAppContext, page: &Entity<LogsTfPage>) {
    let state = cx.update(|cx| page.read(cx).list_state().clone());
    state.scroll_to_end();
}

/// Opens log `id`'s row and waits for its body to be parsed and drawn.
pub fn open_log(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<LogsTfPage>,
    id: u64,
) {
    to_end(cx, page);
    support::click_id(cx, s.win, ids::logstf_log(id));
    support::wait_real(cx, "the parsed log", |cx| {
        cx.update(|cx| page.read(cx).detail().is_some())
    });
    support::settle(cx, s.win);
}

/// Serves `account`'s list only, holding (id, upload date) pairs, after `delay`.
pub fn mount_list_for(
    server: &super::StoreServer,
    account: u64,
    logs: &[(u64, i64)],
    delay: std::time::Duration,
) {
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, ResponseTemplate};
    let player = sid(account).id64().to_string();
    let reply = ResponseTemplate::new(200)
        .set_body_bytes(list_body(logs))
        .set_delay(delay);
    server.rt.block_on(
        Mock::given(method("GET"))
            .and(path("/api/v1/log"))
            .and(query_param("player", player.as_str()))
            .respond_with(reply)
            .mount(&server.server),
    );
}

/// Holds replies until the test opens it, so a request stays in flight exactly as long as
/// the test needs. Wiremock answers under its own lock, so while a reply is held the server
/// takes no other request and `requests_to` would block; `arrived` counts instead.
#[derive(Clone, Default)]
pub struct Gate(Arc<(Mutex<bool>, Condvar, AtomicUsize)>);

impl Gate {
    /// How many held requests have reached the server.
    pub fn arrived(&self) -> usize {
        self.0.2.load(Ordering::SeqCst)
    }

    /// Lets every held reply, and every later one, through.
    pub fn open(&self) {
        *self.0.0.lock().unwrap() = true;
        self.0.1.notify_all();
    }
}

struct Held {
    gate: Gate,
    reply: wiremock::ResponseTemplate,
}

impl wiremock::Respond for Held {
    // A gate the test never opens gives way after ten seconds, so a failure cannot hang.
    fn respond(&self, _: &wiremock::Request) -> wiremock::ResponseTemplate {
        let (open, wake, arrived) = &*self.gate.0;
        arrived.fetch_add(1, Ordering::SeqCst);
        let shut = open.lock().unwrap();
        let _ = wake
            .wait_timeout_while(shut, Duration::from_secs(10), |open| !*open)
            .unwrap();
        self.reply.clone()
    }
}

/// Serves `body` at `route` behind `gate`, only for `account`'s requests when one is given.
pub fn mount_held(
    server: &super::StoreServer,
    gate: &Gate,
    route: &str,
    account: Option<u64>,
    body: Vec<u8>,
) {
    use wiremock::matchers::{method, path, query_param};
    let mut mock = wiremock::Mock::given(method("GET")).and(path(route));
    if let Some(account) = account {
        mock = mock.and(query_param("player", sid(account).id64().to_string()));
    }
    let held = Held {
        gate: gate.clone(),
        reply: wiremock::ResponseTemplate::new(200).set_body_bytes(body),
    };
    server
        .rt
        .block_on(mock.respond_with(held).mount(&server.server));
}

/// How many requests for `route` the server has had.
pub fn requests_to(server: &super::StoreServer, route: &str) -> usize {
    server
        .rt
        .block_on(server.server.received_requests())
        .unwrap_or_default()
        .iter()
        .filter(|r| r.url.path() == route)
        .count()
}

/// How many list requests (`/api/v1/log`) the server has had for `account`.
pub fn list_requests_for(server: &super::StoreServer, account: u64) -> usize {
    let player = sid(account).id64().to_string();
    server
        .rt
        .block_on(server.server.received_requests())
        .unwrap_or_default()
        .iter()
        .filter(|r| r.url.path() == "/api/v1/log")
        .filter(|r| {
            r.url
                .query_pairs()
                .any(|(k, v)| k == "player" && v == player)
        })
        .count()
}

/// How many list requests (`/api/v1/log`) the server has had.
pub fn list_requests(server: &super::StoreServer) -> usize {
    requests_to(server, "/api/v1/log")
}

/// Downloads log `id` into the shell's cache through a second client on `beside`, as the
/// sync would, while the shell's own server may be held.
pub fn download_beside(cx: &mut TestAppContext, beside: &support::StoreServer, id: u64) {
    let services = cx.update(|cx| cx.global::<Services>().clone());
    let client = logs_client(&services, &beside.uri());
    beside
        .rt
        .block_on(client.fetch_log(id))
        .expect("the log downloads");
}
