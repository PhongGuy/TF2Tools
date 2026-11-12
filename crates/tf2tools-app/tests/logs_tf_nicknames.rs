//! Nicknames for tracked logs.tf players: set when adding, renamed from the chip, and shown in
//! an opened log's tables with a marker while Settings keeps them on.

use std::time::Duration;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::log_nicknames::LogNicknames;
use tf2tools_app::pages::logs_tf::LogsTfPage;
use tf2tools_app::pages::logs_tf::text::nickname_note;
use tf2tools_core::SteamId;

use crate::support;
use crate::support::logstf::{
    ARIE, SIXES, list_body, open_log, open_page, scroll_to_in_log, serve_two_logs, shell, sid,
};

const FOUND_XML: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
    <profile><steamID64>76561197960287930</steamID64><steamID><![CDATA[Rabscuttle]]></steamID></profile>";
const GABE: u64 = 76_561_197_960_287_930;

fn set_input(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    input: &Entity<gpui_kit::component::input::InputState>,
    text: &str,
) {
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.set_value(text, window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
}

fn nickname_of(s: &support::ShellWindow, id: SteamId) -> Option<String> {
    let players = s.services.logs.cache().players().unwrap();
    let player = players.iter().find(|p| p.steam_id() == id).unwrap();
    player.nickname().map(str::to_owned)
}

#[gpui_kit::test]
fn adding_with_a_nickname_names_the_chip_by_it(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes("/steam/id/gabelogannewell/", FOUND_XML.as_bytes().to_vec());
    server.mount_bytes("/api/v1/log", list_body(&[]));
    let s = shell(cx, &env, &server.uri(), &[]);
    let page = open_page(cx, &s);

    support::click_id(cx, s.win, ids::LOGSTF_EMPTY_ADD);
    assert!(support::present(cx, s.win, ids::LOGSTF_ADD_NICKNAME));
    let link = cx.update(|cx| page.read(cx).add_input().clone());
    set_input(
        cx,
        &s,
        &link,
        "https://steamcommunity.com/id/gabelogannewell/",
    );
    let nickname = cx.update(|cx| page.read(cx).add_nickname_input().clone());
    set_input(cx, &s, &nickname, "Gaben");
    support::click_id(cx, s.win, ids::LOGSTF_ADD_CONFIRM);
    support::wait_real(cx, "the new chip", |cx| {
        cx.update(|cx| page.read(cx).players().len()) == 1
    });
    let gabe = SteamId::from_id64(GABE).unwrap();
    assert_eq!(nickname_of(&s, gabe).as_deref(), Some("Gaben"));
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_chip(gabe)).as_deref(),
        Some("Gaben")
    );
    let left = cx.update(|cx| {
        page.read(cx)
            .add_nickname_input()
            .read(cx)
            .value()
            .to_string()
    });
    assert_eq!(left, "", "the nickname field empties after an add");
}

#[gpui_kit::test]
fn renaming_a_chip_saves_the_nickname_and_a_blank_one_clears_it(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes("/api/v1/log", list_body(&[]));
    let s = shell(cx, &env, &server.uri(), &[(1, "a"), (2, "b")]);
    let page = open_page(cx, &s);

    support::click_id(cx, s.win, ids::logstf_chip_rename(sid(1)));
    assert!(support::present(cx, s.win, ids::LOGSTF_RENAME_INPUT));
    let input = cx.update(|cx| page.read(cx).rename_input().clone());
    set_input(cx, &s, &input, "Ace");
    support::click_id(cx, s.win, ids::LOGSTF_RENAME_SAVE);
    support::wait_real(cx, "the rename", |cx| {
        support::label_of(cx, s.win, ids::logstf_chip(sid(1))).as_deref() == Some("Ace")
    });
    assert_eq!(nickname_of(&s, sid(1)).as_deref(), Some("Ace"));
    assert!(!support::present(cx, s.win, ids::LOGSTF_RENAME_INPUT));

    // The field opens on the current nickname; emptying it goes back to the Steam name.
    support::click_id(cx, s.win, ids::logstf_chip_rename(sid(1)));
    let shown = cx.update(|cx| page.read(cx).rename_input().read(cx).value().to_string());
    assert_eq!(shown, "Ace");
    set_input(cx, &s, &input, "  ");
    support::click_id(cx, s.win, ids::LOGSTF_RENAME_SAVE);
    support::wait_real(cx, "the clear", |cx| {
        support::label_of(cx, s.win, ids::logstf_chip(sid(1))).as_deref() == Some("a")
    });
    assert_eq!(nickname_of(&s, sid(1)), None);
    assert_eq!(nickname_of(&s, sid(2)), None, "only that player changed");
}

#[gpui_kit::test]
fn cancelling_a_rename_keeps_the_name(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes("/api/v1/log", list_body(&[]));
    let s = shell(cx, &env, &server.uri(), &[(1, "a")]);
    let page = open_page(cx, &s);
    support::click_id(cx, s.win, ids::logstf_chip_rename(sid(1)));
    let input = cx.update(|cx| page.read(cx).rename_input().clone());
    set_input(cx, &s, &input, "Ace");
    support::click_id(cx, s.win, ids::LOGSTF_RENAME_CANCEL);
    support::settle(cx, s.win);
    assert!(!support::present(cx, s.win, ids::LOGSTF_RENAME_INPUT));
    assert_eq!(nickname_of(&s, sid(1)), None);
}

// Arie tracked with the nickname "Me", the 6v6 log open; also returns Arie's name in that log.
fn opened_with_nickname(
    cx: &mut TestAppContext,
) -> (
    support::Env,
    support::StoreServer,
    support::ShellWindow,
    Entity<LogsTfPage>,
    String,
) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_two_logs(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    s.services
        .logs
        .cache()
        .set_nickname(sid(ARIE), Some("Me"))
        .unwrap();
    let page = open_page(cx, &s);
    open_log(cx, &s, &page, 3);
    let log = tf2tools_core::parse_log(SIXES).unwrap();
    let in_log = log
        .players()
        .iter()
        .find(|p| p.steam_id() == sid(ARIE))
        .unwrap()
        .name()
        .to_owned();
    (env, server, s, page, in_log)
}

#[gpui_kit::test]
fn an_open_log_names_a_nicknamed_player_with_a_marker_that_explains(cx: &mut TestAppContext) {
    let (_env, _server, s, page, in_log) = opened_with_nickname(cx);
    assert_ne!(in_log, "Me");
    let arie = sid(ARIE);
    scroll_to_in_log(cx, &s, &page, 3, ids::logstf_player_name(arie));
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_player_name(arie)).as_deref(),
        Some("Me")
    );
    let marker = ids::logstf_nick("player", arie);
    assert!(support::present(cx, s.win, marker.clone()));
    cx.update_window(s.win, |_, w, cx| w.hover(marker, cx))
        .unwrap();
    support::settle(cx, s.win);
    cx.executor().advance_clock(Duration::from_millis(800));
    support::settle(cx, s.win);
    let note = support::label_of(cx, s.win, ids::LOGSTF_NICK_TIP);
    assert_eq!(note, Some(nickname_note(&in_log)));
    assert_eq!(
        nickname_note("Arie"),
        "This is your nickname for this player. Their name in this log is Arie. \
         You can turn this off in Settings."
    );

    scroll_to_in_log(cx, &s, &page, 3, ids::logstf_matrix_name(arie));
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_matrix_name(arie)).as_deref(),
        Some("Me")
    );
    assert!(support::present(
        cx,
        s.win,
        ids::logstf_nick("matrix", arie)
    ));
}

#[gpui_kit::test]
fn turning_nicknames_off_shows_the_logs_own_names(cx: &mut TestAppContext) {
    let (_env, _server, s, page, in_log) = opened_with_nickname(cx);
    let arie = sid(ARIE);
    scroll_to_in_log(cx, &s, &page, 3, ids::logstf_player_name(arie));
    assert!(support::present(
        cx,
        s.win,
        ids::logstf_nick("player", arie)
    ));
    cx.update(|cx| {
        let nicknames = LogNicknames::global(cx);
        nicknames.update(cx, |n, cx| n.set(false, cx));
    });
    support::settle(cx, s.win);
    assert!(!support::present(
        cx,
        s.win,
        ids::logstf_nick("player", arie)
    ));
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_player_name(arie)),
        Some(in_log)
    );
    // The chip keeps the nickname: the setting is about the match tables only.
    let list = cx.update(|cx| page.read(cx).list_state().clone());
    list.scroll_to(gpui_kit::ListOffset {
        item_ix: 0,
        offset_in_item: gpui_kit::px(0.),
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_chip(arie)).as_deref(),
        Some("Me")
    );
}

// deshio chats in the 6v6 log.
const DESHIO: u64 = 251_597_547;

#[gpui_kit::test]
fn the_chat_names_a_nicknamed_player_by_the_nickname(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_two_logs(&server);
    let s = shell(cx, &env, &server.uri(), &[(DESHIO, "deshio")]);
    let deshio = sid(DESHIO);
    s.services
        .logs
        .cache()
        .set_nickname(deshio, Some("Desh"))
        .unwrap();
    let page = open_page(cx, &s);
    open_log(cx, &s, &page, 3);
    let log = tf2tools_core::parse_log(SIXES).unwrap();
    let said = log
        .chat()
        .iter()
        .position(|c| c.steam_id() == Some(deshio))
        .unwrap();
    scroll_to_in_log(cx, &s, &page, 3, ids::LOGSTF_CHAT_TOGGLE);
    support::click_id(cx, s.win, ids::LOGSTF_CHAT_TOGGLE);
    support::settle(cx, s.win);
    assert!(support::present(
        cx,
        s.win,
        ids::logstf_nick(&format!("chat:{said}"), deshio)
    ));
    let line = support::label_of(cx, s.win, ids::logstf_chat_line(said)).unwrap();
    assert!(line.starts_with("Desh: "), "{line}");
}

#[gpui_kit::test]
fn a_nickname_matching_the_log_name_shows_no_marker(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_two_logs(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let arie = sid(ARIE);
    let log = tf2tools_core::parse_log(SIXES).unwrap();
    let in_log = log
        .players()
        .iter()
        .find(|p| p.steam_id() == arie)
        .unwrap()
        .name()
        .to_owned();
    s.services
        .logs
        .cache()
        .set_nickname(arie, Some(&in_log))
        .unwrap();
    let page = open_page(cx, &s);
    open_log(cx, &s, &page, 3);
    scroll_to_in_log(cx, &s, &page, 3, ids::logstf_player_name(arie));
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_player_name(arie)),
        Some(in_log)
    );
    assert!(!support::present(
        cx,
        s.win,
        ids::logstf_nick("player", arie)
    ));
}
