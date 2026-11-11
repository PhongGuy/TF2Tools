//! The logs.tf page: chips, the add dialog, sync, the class grid and the log list.

use std::time::Duration;

use crate::support;

use gpui_kit::{AppContext as _, Entity, TestAppContext};
use serde_json::json;
use tf2tools_app::ids;
use tf2tools_app::pages::logs_tf::LogsTfPage;
use tf2tools_app::pages::logs_tf::text::{LOOKING_UP, log_url, profile_url};
use tf2tools_app::services::{Services, Tf2State};
use tf2tools_app::shell::route::Page;
use tf2tools_core::{
    LogsCache, LogsClient, LogsEndpoints, STEAM_ID64_BASE, SteamId, StoreConfig, TfClass,
    TrackedPlayer,
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

const SIXES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../tf2tools-core/tests/fixtures/logstf/log-4137103-6v6.json"
));
const HIGHLANDER: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../tf2tools-core/tests/fixtures/logstf/log-4150405-hl.json"
));

const ARIE: u64 = 231_702;

fn unix_now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn day_ago(days: i64) -> i64 {
    unix_now() - days * 86_400
}

/// Serves Arie's list of logs 3 (the 6v6 fixture) and 2 (the highlander fixture).
fn serve_two_logs(server: &support::StoreServer) {
    server.mount_bytes(
        "/api/v1/log",
        list_body(&[(3, day_ago(1)), (2, day_ago(2))]),
    );
    server.mount_bytes("/json/3", SIXES.as_bytes().to_vec());
    server.mount_bytes("/json/2", HIGHLANDER.as_bytes().to_vec());
}

fn row_ids(cx: &mut TestAppContext, page: &Entity<LogsTfPage>) -> Vec<u64> {
    cx.update(|cx| page.read(cx).rows().iter().map(|r| r.id()).collect())
}

#[gpui_kit::test]
fn opening_the_page_syncs_the_active_player(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_two_logs(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    assert_eq!(row_ids(cx, &page), [3, 2]);
    assert!(cx.update(|cx| page.read(cx).averages().is_some()));
    assert_eq!(
        cx.update(|cx| page.read(cx).error().map(str::to_owned)),
        None
    );
    assert!(!support::present(cx, s.win, ids::LOGSTF_PROGRESS));
    assert!(support::present(cx, s.win, ids::LOGSTF_PROFILE));
}

#[gpui_kit::test]
fn a_failed_refresh_keeps_the_saved_logs_and_says_why(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_two_logs(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);

    server.reset();
    server.mount_status("/api/v1/log", 503);
    support::click_id(cx, s.win, ids::LOGSTF_REFRESH);
    support::wait_real(cx, "the failed refresh", |cx| {
        cx.update(|cx| page.read(cx).error().is_some() && !page.read(cx).is_syncing())
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::LOGSTF_ERROR).as_deref(),
        Some("logs.tf isn't responding right now. Try again later.")
    );
    assert_eq!(row_ids(cx, &page), [3, 2]);
}

#[gpui_kit::test]
fn offline_the_page_says_so(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env, "http://127.0.0.1:9", &[(ARIE, "Arie")]);
    let _page = open_page(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::LOGSTF_ERROR).as_deref(),
        Some("You're offline. Showing saved logs.")
    );
    assert!(
        !support::present(cx, s.win, ids::LOGSTF_NO_LOGS),
        "a player never synced is not a player without logs"
    );
}

#[gpui_kit::test]
fn switching_chips_mid_sync_finishes_the_first_and_shows_the_second(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    // The list answers slowly, so Arie's sync is still open when the chip is clicked.
    server.mount_bytes_after(
        "/api/v1/log",
        list_body(&[(3, day_ago(1)), (2, day_ago(2))]),
        Duration::from_millis(500),
    );
    server.mount_bytes("/json/3", SIXES.as_bytes().to_vec());
    server.mount_bytes("/json/2", HIGHLANDER.as_bytes().to_vec());
    let s = shell(cx, &env, &server.uri(), &[(1, "other"), (ARIE, "Arie")]);
    support::click_id(cx, s.win, ids::nav(Page::LogsTf));
    let page = cx.update(|cx| s.shell.read(cx).logs_tf_page()).unwrap();
    support::wait_real(cx, "the chips", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    assert!(
        cx.update(|cx| page.read(cx).is_syncing()),
        "Arie's sync is open"
    );
    assert_eq!(cx.update(|cx| page.read(cx).active()), Some(sid(ARIE)));
    support::click_id(cx, s.win, ids::logstf_chip(sid(1)));
    support::wait_real(cx, "both syncs", |cx| {
        cx.update(|cx| {
            let p = page.read(cx);
            p.active() == Some(sid(1)) && !p.is_syncing()
        }) && s
            .services
            .logs
            .cache()
            .players()
            .unwrap()
            .iter()
            .all(|p| p.last_synced().is_some())
    });
    assert_eq!(s.services.logs.cache().logs(sid(ARIE)).unwrap().len(), 2);
    // "other" shares the same served list, so it shows the same two logs, from its own load.
    assert_eq!(row_ids(cx, &page), [3, 2]);
}

#[gpui_kit::test]
fn the_progress_line_belongs_to_the_active_player(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes(
        "/api/v1/log",
        list_body(&[(3, day_ago(1)), (2, day_ago(2))]),
    );
    // The first download answers slowly, so Arie's sync is mid-download at the switch.
    server.mount_bytes_after("/json/3", SIXES.as_bytes().to_vec(), Duration::from_secs(1));
    server.mount_bytes("/json/2", HIGHLANDER.as_bytes().to_vec());
    let s = shell(cx, &env, &server.uri(), &[(1, "other"), (ARIE, "Arie")]);
    support::click_id(cx, s.win, ids::nav(Page::LogsTf));
    let page = cx.update(|cx| s.shell.read(cx).logs_tf_page()).unwrap();
    support::wait_real(cx, "Arie's progress", |cx| {
        support::label_of(cx, s.win, ids::LOGSTF_PROGRESS).as_deref()
            == Some("Downloading logs: 0 of 2")
    });

    support::click_id(cx, s.win, ids::logstf_chip(sid(1)));
    support::wait_real(cx, "the switch", |cx| {
        cx.update(|cx| page.read(cx).active()) == Some(sid(1))
    });
    support::settle(cx, s.win);
    assert!(
        cx.update(|cx| page.read(cx).is_syncing()),
        "Arie's sync is open"
    );
    assert!(!support::present(cx, s.win, ids::LOGSTF_PROGRESS));
    support::wait_real(cx, "both syncs", |cx| {
        cx.update(|cx| !page.read(cx).is_syncing())
    });
}

#[gpui_kit::test]
fn the_grid_shows_nine_classes_and_dims_the_unplayed(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_two_logs(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    for class in TfClass::ALL {
        assert!(
            support::present(cx, s.win, ids::logstf_class(class)),
            "{class:?}"
        );
    }
    // Arie played Scout, Pyro and Heavy in the 6v6 log and is absent from the highlander one.
    for class in [TfClass::Scout, TfClass::Pyro, TfClass::Heavy] {
        assert!(
            !support::present(cx, s.win, ids::logstf_class_unplayed(class)),
            "{class:?}"
        );
    }
    for class in [TfClass::Soldier, TfClass::Medic, TfClass::Spy] {
        assert!(
            support::present(cx, s.win, ids::logstf_class_unplayed(class)),
            "{class:?}"
        );
    }
    assert_eq!(
        support::label_of(cx, s.win, ids::LOGSTF_SUMMARY).as_deref(),
        Some("Last 90 days, 2 logs")
    );
    let scout = cx.update(|cx| page.read(cx).averages().unwrap()[0].clone());
    assert_eq!(scout.seconds(), 1559);
}

fn expanded(cx: &mut TestAppContext, page: &Entity<LogsTfPage>) -> Option<u64> {
    cx.update(|cx| page.read(cx).expanded())
}

/// Scrolls the page to its last log, since the grid fills most of the window above the list.
fn to_end(cx: &mut TestAppContext, page: &Entity<LogsTfPage>) {
    let state = cx.update(|cx| page.read(cx).list_state().clone());
    state.scroll_to_end();
}

#[gpui_kit::test]
fn one_row_opens_at_a_time(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_two_logs(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    to_end(cx, &page);

    support::click_id(cx, s.win, ids::logstf_log(3));
    assert_eq!(expanded(cx, &page), Some(3));
    assert!(support::present(cx, s.win, ids::logstf_log_panel(3)));
    assert!(support::present(cx, s.win, ids::logstf_log_open(3)));

    to_end(cx, &page);
    support::click_id(cx, s.win, ids::logstf_log(2));
    assert_eq!(expanded(cx, &page), Some(2));
    assert!(!support::present(cx, s.win, ids::logstf_log_panel(3)));
    assert!(support::present(cx, s.win, ids::logstf_log_panel(2)));

    support::click_id(cx, s.win, ids::logstf_log(2));
    assert_eq!(expanded(cx, &page), None);
}

#[gpui_kit::test]
fn gone_and_unreadable_rows_explain_and_do_not_open(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes(
        "/api/v1/log",
        list_body(&[(5, day_ago(1)), (6, day_ago(2))]),
    );
    server.mount_status("/json/5", 404);
    server.mount_bytes("/json/6", b"{}".to_vec());
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    to_end(cx, &page);

    support::click_id(cx, s.win, ids::logstf_log(5));
    assert_eq!(expanded(cx, &page), None);
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_log_note(5)).as_deref(),
        Some("This log was removed from logs.tf")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_log_note(6)).as_deref(),
        Some("This log couldn't be read")
    );
    assert!(support::present(cx, s.win, ids::logstf_log_open(6)));
}

#[gpui_kit::test]
fn a_player_without_logs_gets_a_profile_link(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes("/api/v1/log", list_body(&[]));
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let _page = open_page(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::LOGSTF_NO_LOGS).as_deref(),
        Some("No logs in the last 90 days.")
    );
    assert!(support::present(cx, s.win, ids::LOGSTF_NO_LOGS_PROFILE));
    for class in TfClass::ALL {
        assert!(support::present(
            cx,
            s.win,
            ids::logstf_class_unplayed(class)
        ));
    }
}

#[gpui_kit::test]
fn the_grid_scrolls_away_with_the_logs(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_two_logs(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    let scout = ids::logstf_class(TfClass::Scout);
    let before = support::bounds_of(cx, s.win, scout.clone()).unwrap();
    to_end(cx, &page);
    let after = support::bounds_of(cx, s.win, scout).unwrap();
    assert!(after.top() < before.top(), "{before:?} then {after:?}");
}

/// Opens log `id`'s row and waits for its body to be parsed and drawn.
fn open_log(cx: &mut TestAppContext, s: &support::ShellWindow, page: &Entity<LogsTfPage>, id: u64) {
    to_end(cx, page);
    support::click_id(cx, s.win, ids::logstf_log(id));
    support::wait_real(cx, "the parsed log", |cx| {
        cx.update(|cx| page.read(cx).detail().is_some())
    });
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn an_open_6v6_log_hides_columns_logs_tf_hides(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_two_logs(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    open_log(cx, &s, &page, 3);

    assert_eq!(
        support::label_of(cx, s.win, ids::LOGSTF_TEAM_SCORE).as_deref(),
        Some("3 : 2")
    );
    for shown in [
        "K", "A", "D", "DA", "DA/M", "KA/D", "K/D", "DT", "DT/M", "HP", "AS", "CAP",
    ] {
        assert!(
            support::present(cx, s.win, ids::logstf_col(shown)),
            "{shown}"
        );
    }
    for hidden in ["HS", "BS"] {
        assert!(
            !support::present(cx, s.win, ids::logstf_col(hidden)),
            "{hidden}"
        );
    }
    assert!(support::present(
        cx,
        s.win,
        ids::logstf_player_row(sid(ARIE))
    ));
    assert!(support::present(
        cx,
        s.win,
        ids::logstf_player_row(sid(103_548_491))
    ));
    assert!(support::present(
        cx,
        s.win,
        ids::logstf_medic(sid(175_790_976))
    ));
    assert!(support::present(
        cx,
        s.win,
        ids::logstf_medic(sid(98_265_577))
    ));
    assert!(support::present(
        cx,
        s.win,
        ids::logstf_class_kills(sid(ARIE))
    ));
    assert!(
        !support::present(cx, s.win, ids::logstf_class_kills(sid(103_548_491))),
        "class kills only for tracked players"
    );
    assert!(support::present(cx, s.win, ids::logstf_log_open(3)));
}

#[gpui_kit::test]
fn a_highlander_log_shows_every_column_and_closing_drops_the_detail(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_two_logs(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    open_log(cx, &s, &page, 2);
    for shown in ["HS", "BS", "HP", "DT", "AS", "CAP"] {
        assert!(
            support::present(cx, s.win, ids::logstf_col(shown)),
            "{shown}"
        );
    }
    assert_eq!(
        cx.update(|cx| page.read(cx).detail().map(|d| d.players().len())),
        Some(18)
    );
    support::click_id(cx, s.win, ids::logstf_log(2));
    assert!(cx.update(|cx| page.read(cx).detail().is_none()));
}

#[gpui_kit::test]
fn an_open_log_fits_the_smallest_window(cx: &mut TestAppContext) {
    use gpui_kit::{InputEvent as _, ListOffset, ScrollDelta, ScrollEvent, point, px, size};

    let env = support::env();
    let server = support::StoreServer::start();
    serve_two_logs(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    cx.simulate_window_resize(s.win, size(px(800.), px(600.)));
    let page = open_page(cx, &s);
    open_log(cx, &s, &page, 2);

    // The row keeps a readable title by dropping its lower-priority columns.
    let title = support::bounds_of(cx, s.win, ids::logstf_log_title(2)).unwrap();
    assert!(title.size.width >= px(72.), "title {title:?}");

    // Every core column stays whole: DPM, the last, ends inside the list.
    let list = support::bounds_of(cx, s.win, ids::LOGSTF_LIST).unwrap();
    for dpm in [ids::LOGSTF_HEAD_DPM.into(), ids::logstf_log_dpm(2)] {
        let cell = support::bounds_of(cx, s.win, dpm).unwrap();
        assert!(cell.right() <= list.right(), "DPM {cell:?} past {list:?}");
    }

    // The players table keeps every column, so its name column never collapses.
    let first = cx.update(|cx| page.read(cx).detail().unwrap().players()[0].steam_id());
    let header = support::bounds_of(cx, s.win, ids::logstf_col("Name")).unwrap();
    let name = support::bounds_of(cx, s.win, ids::logstf_player_name(first)).unwrap();
    assert!(header.size.width >= px(100.), "header {header:?}");
    assert!(name.size.width >= px(100.), "name {name:?}");

    // The table scrolls sideways to its last column: bring it into view, then wheel right.
    let row = support::bounds_of(cx, s.win, ids::logstf_log(2)).unwrap();
    let table = support::bounds_of(cx, s.win, ids::LOGSTF_PLAYERS_SCROLL).unwrap();
    let state = cx.update(|cx| page.read(cx).list_state().clone());
    state.scroll_to(ListOffset {
        item_ix: 2,
        offset_in_item: table.top() - row.top(),
    });
    support::settle(cx, s.win);
    let table = support::bounds_of(cx, s.win, ids::LOGSTF_PLAYERS_SCROLL).unwrap();
    let cap = support::bounds_of(cx, s.win, ids::logstf_col("CAP")).unwrap();
    assert!(cap.right() > table.right(), "the table overflows at 800 px");
    let wheel = ScrollEvent {
        position: point(table.center().x, table.top() + px(10.)),
        delta: ScrollDelta::Pixels(point(px(-2000.), px(0.))),
        ..Default::default()
    };
    cx.update_window(s.win, |_, w, cx| {
        let _ = w.dispatch_event(wheel.to_platform_input(), cx);
    })
    .unwrap();
    support::settle(cx, s.win);
    let table = support::bounds_of(cx, s.win, ids::LOGSTF_PLAYERS_SCROLL).unwrap();
    let cap = support::bounds_of(cx, s.win, ids::logstf_col("CAP")).unwrap();
    assert!(
        cap.right() <= table.right() + px(1.) && cap.left() >= table.left(),
        "CAP {cap:?} outside {table:?}"
    );
}

#[gpui_kit::test]
fn a_player_waiting_for_their_first_sync_is_not_shown_as_without_logs(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    // Arie's list answers slowly, so the other player's sync waits its turn behind it.
    server.mount_bytes_after(
        "/api/v1/log",
        list_body(&[(3, day_ago(1))]),
        Duration::from_millis(500),
    );
    server.mount_bytes("/json/3", SIXES.as_bytes().to_vec());
    let s = shell(cx, &env, &server.uri(), &[(1, "other"), (ARIE, "Arie")]);
    support::click_id(cx, s.win, ids::nav(Page::LogsTf));
    let page = cx.update(|cx| s.shell.read(cx).logs_tf_page()).unwrap();
    support::wait_real(cx, "the chips", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::click_id(cx, s.win, ids::logstf_chip(sid(1)));
    support::wait_real(cx, "the switch", |cx| {
        cx.update(|cx| page.read(cx).active()) == Some(sid(1))
    });
    assert!(
        cx.update(|cx| page.read(cx).is_syncing()),
        "Arie's sync is open"
    );
    assert!(!support::present(cx, s.win, ids::LOGSTF_NO_LOGS));
    support::wait_real(cx, "both syncs", |cx| {
        cx.update(|cx| !page.read(cx).is_syncing())
    });
}

#[gpui_kit::test]
fn a_row_still_downloading_does_not_open(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes("/api/v1/log", list_body(&[(3, day_ago(1))]));
    server.mount_bytes_after("/json/3", SIXES.as_bytes().to_vec(), Duration::from_secs(1));
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    support::click_id(cx, s.win, ids::nav(Page::LogsTf));
    let page = cx.update(|cx| s.shell.read(cx).logs_tf_page()).unwrap();
    support::wait_real(cx, "the pending row", |cx| {
        cx.update(|cx| {
            page.read(cx)
                .rows()
                .iter()
                .any(|r| r.id() == 3 && r.state() == tf2tools_core::LogState::Pending)
        })
    });
    to_end(cx, &page);
    support::click_id(cx, s.win, ids::logstf_log(3));
    assert_eq!(expanded(cx, &page), None);
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_log_note(3)).as_deref(),
        Some("Downloading...")
    );
    support::wait_real(cx, "the sync", |cx| {
        cx.update(|cx| !page.read(cx).is_syncing())
    });
}

/// Opens log 2 at 800 by 600 and scrolls the page so the players table sits mid-window.
fn players_table_in_view(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<LogsTfPage>,
) {
    use gpui_kit::{ListOffset, px, size};

    cx.simulate_window_resize(s.win, size(px(800.), px(600.)));
    support::settle(cx, s.win);
    open_log(cx, s, page, 2);
    let row = support::bounds_of(cx, s.win, ids::logstf_log(2)).unwrap();
    let table = support::bounds_of(cx, s.win, ids::LOGSTF_PLAYERS_SCROLL).unwrap();
    let state = cx.update(|cx| page.read(cx).list_state().clone());
    state.scroll_to(ListOffset {
        item_ix: 2,
        offset_in_item: table.top() - row.top() - px(100.),
    });
    support::settle(cx, s.win);
}

/// Sends one wheel event at `at` and lets the window redraw.
fn wheel(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    at: gpui_kit::Point<gpui_kit::Pixels>,
    delta: gpui_kit::ScrollDelta,
) {
    use gpui_kit::{InputEvent as _, ScrollEvent};

    let event = ScrollEvent {
        position: at,
        delta,
        ..Default::default()
    };
    cx.update_window(s.win, |_, w, cx| {
        let _ = w.dispatch_event(event.to_platform_input(), cx);
    })
    .unwrap();
    support::settle(cx, s.win);
}

fn page_offset(cx: &mut TestAppContext, page: &Entity<LogsTfPage>) -> gpui_kit::Pixels {
    let state = cx.update(|cx| page.read(cx).list_state().clone());
    -state.scroll_px_offset_for_scrollbar().y
}

#[gpui_kit::test]
fn a_mouse_wheel_over_a_wide_table_scrolls_the_page(cx: &mut TestAppContext) {
    use gpui_kit::{ScrollDelta, point, px};

    let env = support::env();
    let server = support::StoreServer::start();
    serve_two_logs(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    players_table_in_view(cx, &s, &page);

    let table = support::bounds_of(cx, s.win, ids::LOGSTF_PLAYERS_SCROLL).unwrap();
    let cap = support::bounds_of(cx, s.win, ids::logstf_col("CAP")).unwrap();
    let before = page_offset(cx, &page);
    let at = point(table.center().x, table.top() + px(10.));
    wheel(cx, &s, at, ScrollDelta::Lines(point(0., -3.)));
    let after = page_offset(cx, &page);
    let moved = support::bounds_of(cx, s.win, ids::logstf_col("CAP")).unwrap();
    assert!(
        after > before,
        "the page scrolls down: {before:?} then {after:?}"
    );
    assert_eq!(moved.left(), cap.left(), "the table stays put sideways");

    // A sideways wheel, as shift+wheel or a trackpad sends, still scrolls the table.
    let table = support::bounds_of(cx, s.win, ids::LOGSTF_PLAYERS_SCROLL).unwrap();
    let at = point(table.center().x, table.top() + px(10.));
    let before = page_offset(cx, &page);
    let cap = support::bounds_of(cx, s.win, ids::logstf_col("CAP")).unwrap();
    wheel(cx, &s, at, ScrollDelta::Lines(point(-3., 0.)));
    let moved = support::bounds_of(cx, s.win, ids::logstf_col("CAP")).unwrap();
    assert!(moved.left() < cap.left(), "{cap:?} then {moved:?}");
    assert_eq!(page_offset(cx, &page), before, "the page stays put");
}

#[gpui_kit::test]
fn the_page_keeps_its_scroll_while_a_sync_lands(cx: &mut TestAppContext) {
    use gpui_kit::{ScrollDelta, point, px, size};

    let env = support::env();
    let server = support::StoreServer::start();
    // Enough slow downloads that the sync reports progress all through the scrolling.
    let logs: Vec<_> = (100..140).map(|id| (id, day_ago(1))).collect();
    server.mount_bytes("/api/v1/log", list_body(&logs));
    for (id, _) in &logs {
        server.mount_bytes_after(
            &format!("/json/{id}"),
            SIXES.as_bytes().to_vec(),
            Duration::from_millis(150),
        );
    }
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    cx.simulate_window_resize(s.win, size(px(800.), px(600.)));
    support::click_id(cx, s.win, ids::nav(Page::LogsTf));
    let page = cx.update(|cx| s.shell.read(cx).logs_tf_page()).unwrap();
    support::wait_real(cx, "the progress line", |cx| {
        support::label_of(cx, s.win, ids::LOGSTF_PROGRESS).is_some()
    });

    // Over the grid, each wheel step moves the page by its own distance and each progress
    // event after it leaves the page where it was.
    let mut expected = page_offset(cx, &page);
    for step in 0..6 {
        wheel(
            cx,
            &s,
            point(px(350.), px(400.)),
            ScrollDelta::Pixels(point(px(0.), px(-100.))),
        );
        expected += px(100.);
        assert_eq!(page_offset(cx, &page), expected, "after wheel step {step}");
        let shown = support::label_of(cx, s.win, ids::LOGSTF_PROGRESS);
        support::wait_real(cx, "the next progress event", |cx| {
            support::label_of(cx, s.win, ids::LOGSTF_PROGRESS) != shown
        });
        support::settle(cx, s.win);
        assert_eq!(page_offset(cx, &page), expected, "after progress {step}");
    }
    support::wait_real(cx, "the sync", |cx| {
        cx.update(|cx| !page.read(cx).is_syncing())
    });
}

#[gpui_kit::test]
fn a_new_log_keeps_the_open_log_where_it_was(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_two_logs(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    players_table_in_view(cx, &s, &page);
    let before = support::bounds_of(cx, s.win, ids::LOGSTF_PLAYERS_SCROLL).unwrap();

    // A refresh finds a newer log, which lands above the open one.
    server.reset();
    server.mount_bytes(
        "/api/v1/log",
        list_body(&[(4, unix_now()), (3, day_ago(1)), (2, day_ago(2))]),
    );
    server.mount_bytes("/json/4", SIXES.as_bytes().to_vec());
    cx.update(|cx| page.update(cx, |page, cx| page.refresh(cx)));
    support::wait_real(cx, "the refresh", |cx| {
        cx.update(|cx| !page.read(cx).is_syncing() && page.read(cx).rows().len() == 3)
    });
    support::settle(cx, s.win);
    let after = support::bounds_of(cx, s.win, ids::LOGSTF_PLAYERS_SCROLL);
    assert_eq!(after, Some(before), "the open log stays put");
}

#[gpui_kit::test]
fn the_add_button_says_it_is_looking_up(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes_after(
        "/steam/id/gabelogannewell/",
        FOUND_XML.as_bytes().to_vec(),
        Duration::from_millis(500),
    );
    server.mount_bytes("/api/v1/log", list_body(&[]));
    let s = shell(cx, &env, &server.uri(), &[]);
    let page = open_page(cx, &s);
    support::click_id(cx, s.win, ids::LOGSTF_EMPTY_ADD);
    assert_eq!(
        support::label_of(cx, s.win, ids::LOGSTF_ADD_CONFIRM).as_deref(),
        Some("Add")
    );
    type_into_add(cx, &s, &page, "gabelogannewell");
    support::click_id(cx, s.win, ids::LOGSTF_ADD_CONFIRM);
    assert_eq!(
        support::label_of(cx, s.win, ids::LOGSTF_ADD_CONFIRM).as_deref(),
        Some(LOOKING_UP)
    );
    support::wait_real(cx, "the new chip", |cx| {
        cx.update(|cx| page.read(cx).players().len()) == 1
    });
}
