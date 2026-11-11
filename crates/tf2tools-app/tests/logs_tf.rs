//! The logs.tf page: chips, the add dialog, sync, the class grid and the log list.

use std::time::Duration;

use crate::support;

use gpui_kit::{AppContext as _, Entity, TestAppContext};
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

#[gpui_kit::test]
fn a_failed_removal_toasts_and_keeps_the_chip(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes("/api/v1/log", list_body(&[]));
    let s = shell(cx, &env, &server.uri(), &[(1, "a"), (2, "b")]);
    let page = open_page(cx, &s);
    rusqlite::Connection::open(env.data.db_path())
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER keep_players BEFORE DELETE ON logstf_players
             BEGIN SELECT RAISE(ABORT, 'disk full'); END;",
        )
        .unwrap();

    support::click_id(cx, s.win, ids::logstf_chip_remove(sid(2)));
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    support::wait_real(cx, "the toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    let toast = support::label_of(cx, s.win, ids::TOAST).unwrap();
    assert!(toast.starts_with("Failed to stop tracking"), "{toast}");
    assert!(!toast.contains("disk full"), "{toast}");
    support::assert_plain(&toast);
    assert_eq!(players_of(cx, &page), ["a", "b"]);
    assert!(support::present(cx, s.win, ids::logstf_chip(sid(2))));
}

const FOUND_XML: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
    <profile><steamID64>76561197960287930</steamID64><steamID><![CDATA[Rabscuttle]]></steamID></profile>";
const NOT_FOUND_XML: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
    <response><error><![CDATA[The specified profile could not be found.]]></error></response>";
const GABE: u64 = 76_561_197_960_287_930;

fn type_into_add(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<LogsTfPage>,
    text: &str,
) {
    let input = cx.update(|cx| page.read(cx).add_input().clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.set_value(text, window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn adding_by_profile_link_tracks_and_activates_the_player(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes("/steam/id/gabelogannewell/", FOUND_XML.as_bytes().to_vec());
    server.mount_bytes("/api/v1/log", list_body(&[]));
    let s = shell(cx, &env, &server.uri(), &[]);
    let page = open_page(cx, &s);

    support::click_id(cx, s.win, ids::LOGSTF_EMPTY_ADD);
    assert!(support::disabled_of(cx, s.win, ids::LOGSTF_ADD_CONFIRM));
    type_into_add(
        cx,
        &s,
        &page,
        "https://steamcommunity.com/id/gabelogannewell/",
    );
    assert!(!support::disabled_of(cx, s.win, ids::LOGSTF_ADD_CONFIRM));
    support::click_id(cx, s.win, ids::LOGSTF_ADD_CONFIRM);
    support::wait_real(cx, "the new chip", |cx| {
        cx.update(|cx| page.read(cx).players().len()) == 1
    });
    let gabe = SteamId::from_id64(GABE).unwrap();
    assert_eq!(players_of(cx, &page), ["Rabscuttle"]);
    assert_eq!(cx.update(|cx| page.read(cx).active()), Some(gabe));
    assert!(
        !support::present(cx, s.win, ids::LOGSTF_ADD_INPUT),
        "the dialog closed"
    );
}

#[gpui_kit::test]
fn a_bad_input_or_unknown_profile_keeps_the_dialog_with_a_message(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes("/steam/id/nobody9z8y7x/", NOT_FOUND_XML.as_bytes().to_vec());
    let s = shell(cx, &env, &server.uri(), &[]);
    let page = open_page(cx, &s);
    support::click_id(cx, s.win, ids::LOGSTF_EMPTY_ADD);

    type_into_add(cx, &s, &page, "has space");
    support::click_id(cx, s.win, ids::LOGSTF_ADD_CONFIRM);
    assert_eq!(
        support::label_of(cx, s.win, ids::LOGSTF_ADD_ERROR).as_deref(),
        Some("That doesn't look like a Steam profile link or Steam ID.")
    );

    type_into_add(cx, &s, &page, "nobody9z8y7x");
    support::click_id(cx, s.win, ids::LOGSTF_ADD_CONFIRM);
    support::wait_real(cx, "the lookup", |cx| {
        support::label_of(cx, s.win, ids::LOGSTF_ADD_ERROR).as_deref()
            == Some("We couldn't find that Steam profile.")
    });
    assert!(support::present(cx, s.win, ids::LOGSTF_ADD_INPUT));
    let typed = cx.update(|cx| page.read(cx).add_input().read(cx).value().to_string());
    assert_eq!(typed, "nobody9z8y7x");
    assert!(players_of(cx, &page).is_empty());
}

#[gpui_kit::test]
fn adding_a_tracked_player_just_activates_them(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes("/api/v1/log", list_body(&[]));
    // No Steam route: the lookup fails and an Id input falls back to its placeholder.
    let s = shell(cx, &env, &server.uri(), &[(1, "a"), (2, "b")]);
    let page = open_page(cx, &s);
    support::click_id(cx, s.win, ids::LOGSTF_ADD);
    type_into_add(cx, &s, &page, "[U:1:1]");
    support::click_id(cx, s.win, ids::LOGSTF_ADD_CONFIRM);
    support::wait_real(cx, "the switch", |cx| {
        cx.update(|cx| page.read(cx).active()) == Some(sid(1))
    });
    assert_eq!(players_of(cx, &page), ["a", "b"]);
}

#[gpui_kit::test]
fn a_lookup_landing_after_the_dialog_closed_leaves_other_dialogs_alone(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes_after(
        "/steam/id/gabelogannewell/",
        FOUND_XML.as_bytes().to_vec(),
        Duration::from_millis(500),
    );
    server.mount_bytes("/api/v1/log", list_body(&[]));
    let s = shell(cx, &env, &server.uri(), &[(1, "a")]);
    let page = open_page(cx, &s);

    support::click_id(cx, s.win, ids::LOGSTF_ADD);
    type_into_add(cx, &s, &page, "gabelogannewell");
    support::press(cx, s.win, "enter");
    assert!(
        support::disabled_of(cx, s.win, ids::LOGSTF_ADD_CONFIRM),
        "Enter started the lookup, which holds Add"
    );
    support::click_dialog_x(cx, s.win);
    assert!(!support::present(cx, s.win, ids::LOGSTF_ADD_INPUT));
    support::click_id(cx, s.win, ids::logstf_chip_remove(sid(1)));
    support::wait_real(cx, "the late add", |cx| {
        cx.update(|cx| page.read(cx).players().len()) == 2
    });
    assert!(
        support::present(cx, s.win, ids::CONFIRM_TITLE),
        "the stop-tracking confirm is still open"
    );
}

#[gpui_kit::test]
fn the_add_dialog_cancels_without_adding(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let s = shell(cx, &env, &server.uri(), &[]);
    let page = open_page(cx, &s);
    support::click_id(cx, s.win, ids::LOGSTF_EMPTY_ADD);
    type_into_add(cx, &s, &page, "gabelogannewell");
    support::click_id(cx, s.win, ids::LOGSTF_ADD_CANCEL);
    assert!(!support::present(cx, s.win, ids::LOGSTF_ADD_INPUT));
    assert!(players_of(cx, &page).is_empty());
}
