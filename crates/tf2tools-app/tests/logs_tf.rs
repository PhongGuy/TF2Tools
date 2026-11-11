//! The logs.tf page: chips, the add dialog, sync, the class grid and the log list.

use std::time::Duration;

use crate::support;

use gpui_kit::{Entity, TestAppContext};
use serde_json::json;
use tf2tools_app::ids;
use tf2tools_app::pages::logs_tf::LogsTfPage;
use tf2tools_app::pages::logs_tf::text::{log_url, profile_url};
use tf2tools_app::services::{Services, Tf2State};
use tf2tools_app::shell::route::Page;
use tf2tools_core::{
    LogsCache, LogsClient, LogsEndpoints, STEAM_ID64_BASE, SteamId, StoreConfig, TrackedPlayer,
};

fn sid(account: u64) -> SteamId {
    SteamId::from_id64(STEAM_ID64_BASE + account).unwrap()
}

/// Opens logs.tf through the sidebar and waits for its first load and any sync to finish.
fn open_page(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<LogsTfPage> {
    support::click_id(cx, s.win, ids::nav(Page::LogsTf));
    let page = cx
        .update(|cx| s.shell.read(cx).logs_tf_page())
        .expect("a logs.tf page");
    support::wait_real(cx, "the logs.tf page", |cx| {
        cx.update(|cx| {
            let p = page.read(cx);
            p.is_loaded() && !p.is_syncing()
        })
    });
    support::settle(cx, s.win);
    page
}

#[gpui_kit::test]
fn logs_tf_sits_under_maps_and_starts_empty(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let at = Page::ALL.iter().position(|p| *p == Page::LogsTf).unwrap();
    assert_eq!(Page::ALL[at - 1], Page::Maps);
    assert_eq!(
        (Page::LogsTf.slug(), Page::LogsTf.label()),
        ("logs-tf", "logs.tf")
    );

    let page = open_page(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::HEADER_TITLE).as_deref(),
        Some("logs.tf")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::LOGSTF_CREDIT).as_deref(),
        Some("Stats from logs.tf")
    );
    assert!(support::present(cx, s.win, ids::LOGSTF_EMPTY));
    assert!(support::present(cx, s.win, ids::LOGSTF_EMPTY_ADD));
    assert!(cx.update(|cx| page.read(cx).players().is_empty()));
}

#[test]
fn links_point_at_logs_tf() {
    assert_eq!(log_url(4_137_103), "https://logs.tf/4137103");
    assert_eq!(
        profile_url(sid(231_702)),
        "https://logs.tf/profile/76561197960497430"
    );
}

/// A logs client on `base` with no request spacing, over the services' own database.
fn logs_client(services: &Services, base: &str) -> LogsClient {
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
fn shell(
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
fn list_body(logs: &[(u64, i64)]) -> Vec<u8> {
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

fn players_of(cx: &mut TestAppContext, page: &Entity<LogsTfPage>) -> Vec<String> {
    cx.update(|cx| {
        page.read(cx)
            .players()
            .iter()
            .map(|p| p.name().to_owned())
            .collect()
    })
}

#[gpui_kit::test]
fn chips_keep_order_and_the_active_one_is_selected(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes("/api/v1/log", list_body(&[]));
    let s = shell(cx, &env, &server.uri(), &[(1, "a"), (2, "b")]);
    let page = open_page(cx, &s);
    assert_eq!(players_of(cx, &page), ["a", "b"]);
    assert_eq!(
        support::selected_of(cx, s.win, ids::logstf_chip(sid(2))),
        Some(true)
    );
    assert_eq!(
        support::selected_of(cx, s.win, ids::logstf_chip(sid(1))),
        Some(false)
    );
    assert!(support::present(cx, s.win, ids::LOGSTF_ADD));
    assert!(!support::present(cx, s.win, ids::LOGSTF_EMPTY));

    support::click_id(cx, s.win, ids::logstf_chip(sid(1)));
    support::wait_real(cx, "the switch", |cx| {
        cx.update(|cx| page.read(cx).active()) == Some(sid(1))
    });
    assert_eq!(
        support::selected_of(cx, s.win, ids::logstf_chip(sid(1))),
        Some(true)
    );
    assert_eq!(
        s.services.logs.cache().active_player().unwrap(),
        Some(sid(1))
    );
}

#[gpui_kit::test]
fn removing_a_chip_asks_first(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes("/api/v1/log", list_body(&[]));
    let s = shell(cx, &env, &server.uri(), &[(1, "a"), (2, "b")]);
    let page = open_page(cx, &s);

    support::click_id(cx, s.win, ids::logstf_chip_remove(sid(2)));
    assert_eq!(
        support::label_of(cx, s.win, ids::CONFIRM_TITLE).as_deref(),
        Some("Stop tracking b?")
    );
    support::click_id(cx, s.win, ids::CONFIRM_CANCEL);
    support::settle(cx, s.win);
    assert_eq!(players_of(cx, &page), ["a", "b"]);

    support::click_id(cx, s.win, ids::logstf_chip_remove(sid(2)));
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    support::wait_real(cx, "the removal", |cx| {
        cx.update(|cx| page.read(cx).players().len()) == 1
    });
    assert!(!support::present(cx, s.win, ids::logstf_chip(sid(2))));
    assert_eq!(s.services.logs.cache().players().unwrap().len(), 1);
    assert_eq!(cx.update(|cx| page.read(cx).active()), Some(sid(1)));
}
