//! The logs.tf page: chips, the add dialog, sync, the class grid and the log list.

use std::time::Duration;

use crate::support;
use crate::support::logstf::{
    ARIE, Gate, HIGHLANDER, SIXES, day_ago, expanded, list_body, list_requests, list_requests_for,
    mount_held, mount_list_for, open_log, open_page, players_of, requests_to, row_ids,
    serve_two_logs, shell, sid, to_end, unix_now,
};

use gpui_kit::{AppContext as _, Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::logs_tf::LogsTfPage;
use tf2tools_app::pages::logs_tf::text::{LOOKING_UP, log_url, profile_url};
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{SteamId, TfClass};

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
        cx.update(|cx| page.read(cx).error(cx).map(str::to_owned)),
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
        cx.update(|cx| page.read(cx).error(cx).is_some() && !page.read(cx).is_syncing(cx))
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
fn switching_chips_mid_sync_cancels_the_first_and_syncs_the_second(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    // The list answers slowly, so Arie's sync is still listing when the chip is clicked.
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
    support::wait_real(cx, "Arie's sync", |cx| {
        cx.update(|cx| page.read(cx).is_loaded() && page.read(cx).is_syncing(cx))
    });
    support::click_id(cx, s.win, ids::logstf_chip(sid(1)));
    support::wait_real(cx, "the switch", |cx| {
        cx.update(|cx| {
            let p = page.read(cx);
            p.active() == Some(sid(1)) && !p.is_syncing(cx)
        })
    });
    let synced = |id: SteamId| {
        s.services
            .logs
            .cache()
            .players()
            .unwrap()
            .into_iter()
            .find(|p| p.steam_id() == id)
            .and_then(|p| p.last_synced())
    };
    assert_eq!(
        synced(sid(ARIE)),
        None,
        "Arie's sync was cancelled, so it is not stamped"
    );
    assert!(synced(sid(1)).is_some(), "the chosen player synced");
    assert_eq!(row_ids(cx, &page), [3, 2]);

    // Choosing Arie again resumes their sync.
    support::click_id(cx, s.win, ids::logstf_chip(sid(ARIE)));
    support::wait_real(cx, "Arie's resumed sync", |cx| {
        cx.update(|cx| !page.read(cx).is_syncing(cx)) && synced(sid(ARIE)).is_some()
    });
}

#[gpui_kit::test]
fn removing_a_player_mid_sync_stops_their_sync_before_their_logs_go(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    // Arie's list answers slowly, so the removal lands while it is in flight. The other
    // player's list is empty; their sync still lists (no listed range is stamped, so it is
    // not skipped), finds nothing and downloads nothing.
    mount_list_for(
        &server,
        ARIE,
        &[(3, day_ago(1))],
        Duration::from_millis(500),
    );
    mount_list_for(&server, 1, &[], Duration::ZERO);
    server.mount_bytes("/json/3", SIXES.as_bytes().to_vec());
    let s = shell(cx, &env, &server.uri(), &[(1, "other"), (ARIE, "Arie")]);
    let other = i64::try_from(sid(1).id64()).unwrap();
    support::sql(&env)
        .execute(
            "UPDATE logstf_players SET last_synced = ?1 WHERE steam_id = ?2",
            [unix_now() - 60, other],
        )
        .unwrap();
    support::click_id(cx, s.win, ids::nav(Page::LogsTf));
    let page = cx.update(|cx| s.shell.read(cx).logs_tf_page()).unwrap();
    // Waiting for the request itself means the removal meets a list that is on the wire.
    support::wait_real(cx, "Arie's list request", |cx| {
        cx.update(|cx| page.read(cx).is_loaded() && page.read(cx).is_syncing(cx))
            && list_requests(&server) == 1
    });
    support::click_id(cx, s.win, ids::logstf_chip_remove(sid(ARIE)));
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    support::wait_real(cx, "the removal and the next sync", |cx| {
        cx.update(|cx| {
            let p = page.read(cx);
            p.players().len() == 1 && !p.is_syncing(cx)
        })
    });
    let conn = support::sql(&env);
    let count = |sql: &str| conn.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap();
    assert_eq!(
        count("SELECT COUNT(*) FROM logstf_player_logs"),
        0,
        "no membership outlives Arie's removal"
    );
    assert_eq!(
        count("SELECT COUNT(*) FROM logstf_logs"),
        0,
        "Arie's unshared log went with them"
    );
    assert_eq!(
        requests_to(&server, "/json/3"),
        0,
        "Arie's stopped sync downloaded nothing"
    );
    assert_eq!(cx.update(|cx| page.read(cx).error(cx)), None);
    assert!(
        support::toast_labels(cx, s.win).is_empty(),
        "nothing failed"
    );
}

/// Opens logs.tf with "other" (stamped as synced) and Arie tracked, Arie active.
fn removal_page(
    cx: &mut TestAppContext,
    env: &support::Env,
    server: &support::StoreServer,
) -> (support::ShellWindow, Entity<LogsTfPage>) {
    let s = shell(cx, env, &server.uri(), &[(1, "other"), (ARIE, "Arie")]);
    let other = i64::try_from(sid(1).id64()).unwrap();
    support::sql(env)
        .execute(
            "UPDATE logstf_players SET last_synced = ?1 WHERE steam_id = ?2",
            [unix_now() - 60, other],
        )
        .unwrap();
    support::click_id(cx, s.win, ids::nav(Page::LogsTf));
    let page = cx.update(|cx| s.shell.read(cx).logs_tf_page()).unwrap();
    (s, page)
}

/// Memberships and stored logs left in the database.
fn stored_rows(env: &support::Env) -> (i64, i64) {
    let conn = support::sql(env);
    let count = |sql: &str| conn.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap();
    (
        count("SELECT COUNT(*) FROM logstf_player_logs"),
        count("SELECT COUNT(*) FROM logstf_logs"),
    )
}

#[gpui_kit::test]
fn removing_a_player_mid_download_waits_for_the_download(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    // Arie's list answers at once and both bodies wait on the gate, so the removal meets a
    // download on the wire.
    mount_list_for(
        &server,
        ARIE,
        &[(3, day_ago(1)), (2, day_ago(2))],
        Duration::ZERO,
    );
    mount_list_for(&server, 1, &[], Duration::ZERO);
    let downloads = Gate::default();
    mount_held(
        &server,
        &downloads,
        "/json/3",
        None,
        SIXES.as_bytes().to_vec(),
    );
    mount_held(
        &server,
        &downloads,
        "/json/2",
        None,
        HIGHLANDER.as_bytes().to_vec(),
    );
    let (s, page) = removal_page(cx, &env, &server);
    support::wait_real(cx, "Arie's first download", |_| downloads.arrived() == 1);
    support::click_id(cx, s.win, ids::logstf_chip_remove(sid(ARIE)));
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_chip(sid(ARIE))).as_deref(),
        Some("Removing...")
    );
    let tracked = s.services.logs.cache().players().unwrap();
    assert!(
        tracked.iter().any(|p| p.steam_id() == sid(ARIE)),
        "Arie stays tracked until their download ends"
    );
    support::click_id(cx, s.win, ids::logstf_chip_remove(sid(ARIE)));
    assert!(
        !support::present(cx, s.win, ids::CONFIRM_OK),
        "a player being removed cannot be asked about again"
    );
    downloads.open();
    support::wait_real(cx, "the removal and the next sync", |cx| {
        cx.update(|cx| {
            let p = page.read(cx);
            p.players().len() == 1 && !p.is_syncing(cx)
        })
    });
    assert_eq!(
        downloads.arrived(),
        1,
        "no download started after the removal"
    );
    assert_eq!(
        stored_rows(&env),
        (0, 0),
        "nothing of Arie's outlives the removal"
    );
    assert!(
        support::toast_labels(cx, s.win).is_empty(),
        "nothing failed"
    );
}

#[gpui_kit::test]
fn choosing_a_player_being_removed_starts_no_sync_for_them(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    // Arie's list waits on the gate, so the chip click lands while the removal waits on it.
    let list = Gate::default();
    mount_held(
        &server,
        &list,
        "/api/v1/log",
        Some(ARIE),
        list_body(&[(3, day_ago(1))]),
    );
    mount_list_for(&server, 1, &[], Duration::ZERO);
    server.mount_bytes("/json/3", SIXES.as_bytes().to_vec());
    let (s, page) = removal_page(cx, &env, &server);
    support::wait_real(cx, "Arie's list request", |_| list.arrived() == 1);
    support::click_id(cx, s.win, ids::logstf_chip_remove(sid(ARIE)));
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    support::click_id(cx, s.win, ids::logstf_chip(sid(ARIE)));
    list.open();
    support::wait_real(cx, "the removal and the next sync", |cx| {
        cx.update(|cx| {
            let p = page.read(cx);
            p.players().len() == 1 && !p.is_syncing(cx)
        })
    });
    assert_eq!(
        list_requests_for(&server, ARIE),
        1,
        "no second sync for Arie started"
    );
    assert_eq!(
        stored_rows(&env),
        (0, 0),
        "nothing of Arie's outlives the removal"
    );
}

#[gpui_kit::test]
fn removing_two_players_back_to_back_syncs_neither(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    // Arie's list waits on the gate, so the other player's removal starts and ends while
    // Arie's removal still waits for Arie's sync.
    let list = Gate::default();
    mount_held(
        &server,
        &list,
        "/api/v1/log",
        Some(ARIE),
        list_body(&[(3, day_ago(1))]),
    );
    mount_list_for(&server, 1, &[], Duration::ZERO);
    server.mount_bytes("/json/3", SIXES.as_bytes().to_vec());
    let (s, page) = removal_page(cx, &env, &server);
    support::wait_real(cx, "Arie's list request", |_| list.arrived() == 1);
    support::click_id(cx, s.win, ids::logstf_chip_remove(sid(ARIE)));
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    support::click_id(cx, s.win, ids::logstf_chip_remove(sid(1)));
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    // The other player has no sync to wait for, so their chip may already be gone.
    let other = support::label_of(cx, s.win, ids::logstf_chip(sid(1)));
    assert!(
        matches!(other.as_deref(), None | Some("Removing...")),
        "{other:?}"
    );
    support::wait_real(cx, "the other player's removal", |cx| {
        cx.update(|cx| page.read(cx).players().len() == 1)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_chip(sid(ARIE))).as_deref(),
        Some("Removing..."),
        "Arie is still being removed after the other removal ended"
    );
    support::click_id(cx, s.win, ids::logstf_chip(sid(ARIE)));
    support::click_id(cx, s.win, ids::logstf_chip_remove(sid(ARIE)));
    assert!(!support::present(cx, s.win, ids::CONFIRM_OK));
    list.open();
    support::wait_real(cx, "both removals", |cx| {
        cx.update(|cx| {
            let p = page.read(cx);
            p.players().is_empty() && !p.is_syncing(cx)
        })
    });
    assert_eq!(
        list_requests_for(&server, ARIE),
        1,
        "no second sync for Arie"
    );
    assert_eq!(
        list_requests_for(&server, 1),
        0,
        "no sync for the other player"
    );
    assert_eq!(
        stored_rows(&env),
        (0, 0),
        "nothing of either player is left"
    );
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
        cx.update(|cx| page.read(cx).is_syncing(cx)),
        "Arie's sync is open"
    );
    assert!(!support::present(cx, s.win, ids::LOGSTF_PROGRESS));
    support::wait_real(cx, "both syncs", |cx| {
        cx.update(|cx| !page.read(cx).is_syncing(cx))
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
        Some("Last 3 months, 2 logs")
    );
    let scout = cx.update(|cx| page.read(cx).averages().unwrap()[0].clone());
    assert_eq!(scout.seconds(), 1559);
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
        Some("No logs on logs.tf for this player yet.")
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

#[gpui_kit::test]
fn an_open_6v6_log_hides_columns_logs_tf_hides(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_two_logs(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    open_log(cx, &s, &page, 3);

    for shown in [
        "K", "A", "D", "DA", "DA/M", "KA/D", "K/D", "DT", "DT/M", "HP", "AS", "CAP",
    ] {
        assert!(
            support::present(cx, s.win, ids::logstf_sort(shown)),
            "{shown}"
        );
    }
    for hidden in ["HS", "BS"] {
        assert!(
            !support::present(cx, s.win, ids::logstf_sort(hidden)),
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
    assert!(support::present(cx, s.win, ids::logstf_log_open(3)));
}

#[gpui_kit::test]
fn the_banner_and_team_table_read_like_logs_tf(cx: &mut TestAppContext) {
    use tf2tools_core::Team;
    let env = support::env();
    let server = support::StoreServer::start();
    serve_two_logs(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    open_log(cx, &s, &page, 3);
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_banner(3)).as_deref(),
        Some("BLU 2, RED 3")
    );
    let cells = [
        (Team::Blue, "Kills", "117"),
        (Team::Red, "Damage", "37145"),
        (Team::Blue, "Charges", "14"),
        (Team::Red, "Drops", "0"),
        (Team::Red, "Caps", "17"),
        (Team::Blue, "Midfights", "4"),
        (Team::Red, "Midfights", "1"),
    ];
    for (team, column, value) in cells {
        assert_eq!(
            support::label_of(cx, s.win, ids::logstf_team_cell(team, column)).as_deref(),
            Some(value),
            "{team:?} {column}"
        );
    }
    let red = support::bounds_of(cx, s.win, ids::logstf_team_cell(Team::Red, "Kills")).unwrap();
    let blu = support::bounds_of(cx, s.win, ids::logstf_team_cell(Team::Blue, "Kills")).unwrap();
    assert!(
        red.top() < blu.top(),
        "RED's row comes first, as on logs.tf"
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
            support::present(cx, s.win, ids::logstf_sort(shown)),
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
fn the_map_sits_under_the_title_in_the_smallest_window(cx: &mut TestAppContext) {
    use gpui_kit::{px, size};
    let env = support::env();
    let server = support::StoreServer::start();
    serve_two_logs(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    cx.simulate_window_resize(s.win, size(px(800.), px(600.)));
    let page = open_page(cx, &s);
    to_end(cx, &page);
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_log_map(3)).as_deref(),
        Some("cp_process_f12")
    );
    let title = support::bounds_of(cx, s.win, ids::logstf_log_title(3)).unwrap();
    let map = support::bounds_of(cx, s.win, ids::logstf_log_map(3)).unwrap();
    let list = support::bounds_of(cx, s.win, ids::LOGSTF_LIST).unwrap();
    assert!(
        map.top() >= title.bottom() - px(1.),
        "map {map:?} under title {title:?}"
    );
    assert!(
        map.left() >= title.left() - px(1.),
        "map {map:?} in the title cell"
    );
    assert!(map.right() <= list.right(), "map {map:?} inside {list:?}");
}

#[gpui_kit::test]
fn a_wide_window_gives_the_map_its_own_column(cx: &mut TestAppContext) {
    use gpui_kit::px;
    let env = support::env();
    let server = support::StoreServer::start();
    serve_two_logs(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    to_end(cx, &page);
    let title = support::bounds_of(cx, s.win, ids::logstf_log_title(3)).unwrap();
    let map = support::bounds_of(cx, s.win, ids::logstf_log_map(3)).unwrap();
    assert!(
        map.left() >= title.right(),
        "map {map:?} right of title {title:?}"
    );
    assert!(
        map.size.width <= px(161.),
        "map {map:?} is one 160 px column"
    );
}

#[gpui_kit::test]
fn shrinking_across_the_breakpoint_makes_room_for_the_map_line(cx: &mut TestAppContext) {
    use gpui_kit::{px, size};
    let env = support::env();
    let server = support::StoreServer::start();
    serve_two_logs(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    to_end(cx, &page);
    let wide = support::bounds_of(cx, s.win, ids::logstf_log(3)).unwrap();

    // The rows were measured one line high; the narrow window must remeasure them.
    cx.simulate_window_resize(s.win, size(px(800.), px(600.)));
    to_end(cx, &page);
    let row = support::bounds_of(cx, s.win, ids::logstf_log(3)).unwrap();
    let title = support::bounds_of(cx, s.win, ids::logstf_log_title(3)).unwrap();
    let map = support::bounds_of(cx, s.win, ids::logstf_log_map(3)).unwrap();
    assert!(
        row.size.height >= wide.size.height + map.size.height - px(1.),
        "row {row:?} was {wide:?}"
    );
    assert!(
        row.size.height >= title.size.height + map.size.height,
        "row {row:?} holds title {title:?} and map {map:?}"
    );
    assert!(map.bottom() <= row.bottom(), "map {map:?} inside {row:?}");
}

#[gpui_kit::test]
fn a_long_title_and_map_stay_inside_their_row_in_the_smallest_window(cx: &mut TestAppContext) {
    use gpui_kit::{px, size};
    let env = support::env();
    let server = support::StoreServer::start();
    let title = "Grand Finals ".repeat(12);
    let map = format!("cp_{}", "very_long_map_name_".repeat(8));
    let body = format!(
        r#"{{"success": true, "results": 1, "total": 1, "parameters": {{}}, "logs": [
            {{"id": 3, "title": "{title}", "map": "{map}", "date": {}, "views": 0, "players": 12}}]}}"#,
        day_ago(1)
    );
    server.mount_bytes("/api/v1/log", body.into_bytes());
    server.mount_bytes("/json/3", SIXES.as_bytes().to_vec());
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    cx.simulate_window_resize(s.win, size(px(800.), px(600.)));
    let page = open_page(cx, &s);
    to_end(cx, &page);
    let row = support::bounds_of(cx, s.win, ids::logstf_log(3)).unwrap();
    let list = support::bounds_of(cx, s.win, ids::LOGSTF_LIST).unwrap();
    let dpm = support::bounds_of(cx, s.win, ids::logstf_log_dpm(3)).unwrap();
    for cell in [ids::logstf_log_title(3), ids::logstf_log_map(3)] {
        let b = support::bounds_of(cx, s.win, cell.clone()).unwrap();
        assert!(
            b.right() <= dpm.left(),
            "{cell:?} {b:?} runs into DPM {dpm:?}"
        );
        assert!(b.bottom() <= row.bottom(), "{cell:?} {b:?} below {row:?}");
    }
    assert!(dpm.right() <= list.right(), "DPM {dpm:?} past {list:?}");
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
    let header = support::bounds_of(cx, s.win, ids::logstf_sort("Name")).unwrap();
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
    let cap = support::bounds_of(cx, s.win, ids::logstf_sort("CAP")).unwrap();
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
    let cap = support::bounds_of(cx, s.win, ids::logstf_sort("CAP")).unwrap();
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
        cx.update(|cx| page.read(cx).is_syncing(cx)),
        "Arie's sync is open"
    );
    assert!(!support::present(cx, s.win, ids::LOGSTF_NO_LOGS));
    support::wait_real(cx, "both syncs", |cx| {
        cx.update(|cx| !page.read(cx).is_syncing(cx))
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
    let cap = support::bounds_of(cx, s.win, ids::logstf_sort("CAP")).unwrap();
    let before = page_offset(cx, &page);
    let at = point(table.center().x, table.top() + px(10.));
    wheel(cx, &s, at, ScrollDelta::Lines(point(0., -3.)));
    let after = page_offset(cx, &page);
    let moved = support::bounds_of(cx, s.win, ids::logstf_sort("CAP")).unwrap();
    assert!(
        after > before,
        "the page scrolls down: {before:?} then {after:?}"
    );
    assert_eq!(moved.left(), cap.left(), "the table stays put sideways");

    // A sideways wheel, as shift+wheel or a trackpad sends, still scrolls the table.
    let table = support::bounds_of(cx, s.win, ids::LOGSTF_PLAYERS_SCROLL).unwrap();
    let at = point(table.center().x, table.top() + px(10.));
    let before = page_offset(cx, &page);
    let cap = support::bounds_of(cx, s.win, ids::logstf_sort("CAP")).unwrap();
    wheel(cx, &s, at, ScrollDelta::Lines(point(-3., 0.)));
    let moved = support::bounds_of(cx, s.win, ids::logstf_sort("CAP")).unwrap();
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
        cx.update(|cx| !page.read(cx).is_syncing(cx))
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
        cx.update(|cx| !page.read(cx).is_syncing(cx) && page.read(cx).rows().len() == 3)
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

#[gpui_kit::test]
fn the_banner_halves_are_solid_with_blu_on_the_left(cx: &mut TestAppContext) {
    use gpui_kit::component::ActiveTheme as _;
    use tf2tools_core::Team;
    let env = support::env();
    let server = support::StoreServer::start();
    serve_two_logs(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    open_log(cx, &s, &page, 3);
    let half = |cx: &mut TestAppContext, team| {
        support::bounds_of(cx, s.win, ids::logstf_banner_team(3, team)).expect("a banner half")
    };
    let (blu, red) = (half(cx, Team::Blue), half(cx, Team::Red));
    assert!(blu.origin.x < red.origin.x, "BLU sits left of RED");
    let (theme_red, theme_blue) = cx.update(|cx| (cx.theme().red, cx.theme().blue));
    for color in [theme_red, theme_blue] {
        let ratio = contrast_with_white(tf2tools_app::theme::team_fill(color));
        assert!(ratio >= 4.5, "white on {color:?} is {ratio:.2}:1");
    }
}

// WCAG's contrast ratio of white text on `fill`.
fn contrast_with_white(fill: gpui_kit::Hsla) -> f32 {
    let rgb = fill.to_rgb();
    let linear = |c: f32| {
        if c <= 0.039_28 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let lum = 0.2126 * linear(rgb.r) + 0.7152 * linear(rgb.g) + 0.0722 * linear(rgb.b);
    1.05 / (lum + 0.05)
}

// Serves one 6v6 log for a player who plays seven classes in it, and opens the page on them.
fn seven_class_page(
    cx: &mut TestAppContext,
    env: &support::Env,
    server: &support::StoreServer,
) -> (support::ShellWindow, Entity<LogsTfPage>) {
    server.mount_bytes("/api/v1/log", list_body(&[(3, day_ago(1))]));
    server.mount_bytes("/json/3", SIXES.as_bytes().to_vec());
    let s = shell(cx, env, &server.uri(), &[(SEVEN_CLASSES, "Seven")]);
    let page = open_page(cx, &s);
    to_end(cx, &page);
    (s, page)
}

const SEVEN_CLASSES: u64 = 170_965_550;

// Every class icon of row 3 stays inside the class column, clear of K/D/A and in the row.
fn assert_icons_clear_of_kda(cx: &mut TestAppContext, s: &support::ShellWindow) {
    use gpui_kit::px;
    let row = support::bounds_of(cx, s.win, ids::logstf_log(3)).unwrap();
    let kda = support::bounds_of(cx, s.win, ids::logstf_log_kda(3)).unwrap();
    let mut shown = 0;
    for class in TfClass::ALL {
        let Some(icon) = support::bounds_of(cx, s.win, ids::logstf_log_class(3, class)) else {
            continue;
        };
        shown += 1;
        assert!(
            icon.right() + px(8.) <= kda.left(),
            "{class:?} {icon:?} touches K/D/A {kda:?}"
        );
        assert!(
            icon.bottom() <= row.bottom(),
            "{class:?} {icon:?} below {row:?}"
        );
    }
    assert_eq!(shown, 7, "every class the player played has an icon");
}

#[gpui_kit::test]
fn many_class_icons_stay_clear_of_kda(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (s, _page) = seven_class_page(cx, &env, &server);
    assert_icons_clear_of_kda(cx, &s);
}

#[gpui_kit::test]
fn many_class_icons_stay_clear_of_kda_in_the_smallest_window(cx: &mut TestAppContext) {
    use gpui_kit::{px, size};
    let env = support::env();
    let server = support::StoreServer::start();
    let (s, page) = seven_class_page(cx, &env, &server);
    cx.simulate_window_resize(s.win, size(px(800.), px(600.)));
    to_end(cx, &page);
    assert_icons_clear_of_kda(cx, &s);
}

// `text` shaped in the theme font at `rems` times the root size.
fn shaped(cx: &mut TestAppContext, win: gpui_kit::AnyWindowHandle, text: &str, rems: f32) -> f32 {
    use gpui_kit::component::ActiveTheme as _;
    cx.update_window(win, |_, w, cx| {
        let run = gpui_kit::TextRun {
            len: text.len(),
            font: gpui_kit::font(cx.theme().font_family.clone()),
            ..Default::default()
        };
        let size = gpui_kit::rems(rems).to_pixels(w.rem_size());
        f32::from(
            w.text_system()
                .shape_line(text.to_owned().into(), size, &[run], None)
                .width,
        )
    })
    .unwrap()
}

#[gpui_kit::test]
fn a_typical_title_and_map_read_whole_in_the_smallest_window(cx: &mut TestAppContext) {
    use gpui_kit::{px, size};
    let env = support::env();
    let server = support::StoreServer::start();
    let (title, map) = ("serveme.tf #1234567", "cp_process_final");
    let body = format!(
        r#"{{"success": true, "results": 1, "total": 1, "parameters": {{}}, "logs": [
            {{"id": 3, "title": "{title}", "map": "{map}", "date": {}, "views": 0, "players": 12}}]}}"#,
        day_ago(1)
    );
    server.mount_bytes("/api/v1/log", body.into_bytes());
    server.mount_bytes("/json/3", SIXES.as_bytes().to_vec());
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    cx.simulate_window_resize(s.win, size(px(800.), px(600.)));
    let page = open_page(cx, &s);
    to_end(cx, &page);
    let cell = support::bounds_of(cx, s.win, ids::logstf_log_title(3)).unwrap();
    let need = shaped(cx, s.win, title, 0.875);
    assert!(
        f32::from(cell.size.width) >= need,
        "title {cell:?} under {need} px"
    );
    let cell = support::bounds_of(cx, s.win, ids::logstf_log_map(3)).unwrap();
    let need = shaped(cx, s.win, map, 0.75);
    assert!(
        f32::from(cell.size.width) >= need,
        "map {cell:?} under {need} px"
    );
}
