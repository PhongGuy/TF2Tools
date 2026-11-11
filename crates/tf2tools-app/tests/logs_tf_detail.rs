//! The open log's sections, compared with the fixtures' logs.tf pages.

use std::time::Duration;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::logs_tf::LogsTfPage;
use tf2tools_app::pages::logs_tf::sort::{PlayerColumn, PlayerSort, sort_players};
use tf2tools_core::TfClass;

use crate::support;
use crate::support::logstf::{
    ARIE, HIGHLANDER, SIXES, day_ago, list_body, open_log, open_page, scroll_to_in_log,
    serve_two_logs, shell, sid,
};

/// Arie's two logs synced, log `log` open; the env and server live as long as the test.
fn opened(
    cx: &mut TestAppContext,
    log: u64,
) -> (
    support::Env,
    support::StoreServer,
    support::ShellWindow,
    Entity<LogsTfPage>,
) {
    let env = support::env();
    let server = support::StoreServer::start();
    serve_two_logs(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    open_log(cx, &s, &page, log);
    (env, server, s, page)
}

#[gpui_kit::test]
fn clicking_a_header_sorts_the_players_and_again_flips_it(cx: &mut TestAppContext) {
    let (_env, _server, s, page) = opened(cx, 3);
    scroll_to_in_log(cx, &s, &page, 3, ids::logstf_sort("DA"));
    support::click_id(cx, s.win, ids::logstf_sort("DA"));
    assert_eq!(
        cx.update(|cx| page.read(cx).player_sort()),
        PlayerSort {
            column: PlayerColumn::Damage,
            descending: true
        }
    );
    let top = |cx: &mut TestAppContext, account: u64| {
        support::bounds_of(cx, s.win, ids::logstf_player_row(sid(account)))
            .unwrap()
            .top()
    };
    assert!(top(cx, 285_837_700) < top(cx, ARIE), "most damage first");
    support::click_id(cx, s.win, ids::logstf_sort("DA"));
    assert!(
        top(cx, ARIE) < top(cx, 285_837_700),
        "a second click flips it"
    );
    open_log(cx, &s, &page, 2);
    assert_eq!(
        cx.update(|cx| page.read(cx).player_sort()),
        PlayerSort {
            column: PlayerColumn::Damage,
            descending: false
        },
        "the sort is kept while the page lives"
    );
}

#[gpui_kit::test]
fn a_sort_by_a_column_the_next_log_hides_shows_it_in_the_default_order(cx: &mut TestAppContext) {
    // A 6v6 log whose hidden HS column still carries a number, so sorting by it would show.
    let mut sixes: serde_json::Value = serde_json::from_str(SIXES).unwrap();
    sixes["players"]["[U:1:231702]"]["headshots"] = 5.into();
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes(
        "/api/v1/log",
        list_body(&[(3, day_ago(1)), (2, day_ago(2))]),
    );
    server.mount_bytes("/json/3", HIGHLANDER.as_bytes().to_vec());
    server.mount_bytes("/json/2", serde_json::to_vec(&sixes).unwrap());
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    open_log(cx, &s, &page, 3);
    scroll_to_in_log(cx, &s, &page, 3, ids::logstf_sort("HS"));
    support::click_id(cx, s.win, ids::logstf_sort("HS"));
    let hs = PlayerSort {
        column: PlayerColumn::Headshots,
        descending: true,
    };
    assert_eq!(cx.update(|cx| page.read(cx).player_sort()), hs);

    open_log(cx, &s, &page, 2);
    assert_eq!(
        cx.update(|cx| page.read(cx).player_sort()),
        hs,
        "the sort is kept"
    );
    let players = cx.update(|cx| page.read(cx).detail().unwrap().players().to_vec());
    let tops: Vec<_> = sort_players(&players, PlayerSort::default())
        .into_iter()
        .map(|i| {
            let row = ids::logstf_player_row(players[i].steam_id());
            support::bounds_of(cx, s.win, row).unwrap().top()
        })
        .collect();
    assert!(
        tops.windows(2).all(|w| w[0] < w[1]),
        "default order: {tops:?}"
    );
}

#[gpui_kit::test]
fn hovering_a_class_icon_opens_its_card(cx: &mut TestAppContext) {
    let (_env, _server, s, page) = opened(cx, 3);
    let icon = ids::logstf_class_card(sid(ARIE), TfClass::Scout);
    scroll_to_in_log(cx, &s, &page, 3, icon.clone());
    assert!(
        !support::present(
            cx,
            s.win,
            ids::logstf_class_popover(sid(ARIE), TfClass::Scout)
        ),
        "closed until hovered"
    );
    cx.update_window(s.win, |_, w, cx| w.hover(icon.clone(), cx))
        .unwrap();
    support::settle(cx, s.win);
    cx.executor().advance_clock(Duration::from_millis(300));
    support::settle(cx, s.win);
    assert!(
        support::present(
            cx,
            s.win,
            ids::logstf_class_popover(sid(ARIE), TfClass::Scout)
        ),
        "the card opens on hover"
    );
    assert_eq!(
        support::label_of(
            cx,
            s.win,
            ids::logstf_weapon(sid(ARIE), TfClass::Scout, "scattergun")
        )
        .as_deref(),
        Some("scattergun 7 (100%) 3334 (86%) 56%")
    );
}

#[gpui_kit::test]
fn rounds_open_any_number_and_show_players_and_events(cx: &mut TestAppContext) {
    let (_env, _server, s, page) = opened(cx, 3);
    scroll_to_in_log(cx, &s, &page, 3, ids::LOGSTF_ROUNDS);
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_round_score(3, 0)).as_deref(),
        Some("0 - 1")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_round_midfight(3, 0)).as_deref(),
        Some("BLU")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_round_midfight(3, 5)).as_deref(),
        Some("-")
    );
    support::click_id(cx, s.win, ids::logstf_round(3, 0));
    // Round 0's panel pushes round 1's header down, possibly out of view.
    scroll_to_in_log(cx, &s, &page, 3, ids::logstf_round(3, 1));
    support::click_id(cx, s.win, ids::logstf_round(3, 1));
    assert_eq!(cx.update(|cx| page.read(cx).open_rounds()), [0, 1]);
    assert!(support::present(cx, s.win, ids::logstf_round_panel(3, 0)));
    assert!(support::present(cx, s.win, ids::logstf_round_panel(3, 1)));
    assert!(support::present(
        cx,
        s.win,
        ids::logstf_round_player(3, 0, sid(ARIE))
    ));
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_event(3, 0, 0)).as_deref(),
        Some("0:23 Medic Death Meta Knightynski killed by DrHappiness")
    );
    scroll_to_in_log(cx, &s, &page, 3, ids::logstf_round(3, 0));
    support::click_id(cx, s.win, ids::logstf_round(3, 0));
    assert_eq!(cx.update(|cx| page.read(cx).open_rounds()), [1]);
    assert!(!support::present(cx, s.win, ids::logstf_round_panel(3, 0)));
}

#[gpui_kit::test]
fn a_medic_block_reads_like_logs_tf(cx: &mut TestAppContext) {
    let (_env, _server, s, page) = opened(cx, 3);
    let rahmed = sid(175_790_976);
    scroll_to_in_log(cx, &s, &page, 3, ids::logstf_medic(rahmed));
    let facts = [
        ("Healing", "28170 (986/m)"),
        ("Charges:medigun", "Medigun: 12 (100%)"),
        ("Drops", "0"),
        ("Avg time to build", "63 s"),
        ("Avg time before using", "37 s"),
        ("Near full charge deaths", "0"),
        ("Avg uber length", "6.5 s"),
        ("Deaths after charge", "4"),
        ("Major advantages lost", "1"),
        ("Biggest advantage lost", "21 s"),
    ];
    for (label, value) in facts {
        assert_eq!(
            support::label_of(cx, s.win, ids::logstf_medic_fact(rahmed, label)).as_deref(),
            Some(value),
            "{label}"
        );
    }
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_heal_target(rahmed, sid(251_597_547))).as_deref(),
        Some("30%")
    );
    assert!(
        support::present(cx, s.win, ids::logstf_medic(sid(98_265_577))),
        "both medics"
    );
}

#[gpui_kit::test]
fn the_matrix_tabs_switch_and_list_every_player(cx: &mut TestAppContext) {
    use tf2tools_core::MatrixKind;
    let (_env, _server, s, page) = opened(cx, 3);
    scroll_to_in_log(cx, &s, &page, 3, ids::LOGSTF_MATRIX);
    assert_eq!(
        cx.update(|cx| page.read(cx).matrix_kind()),
        MatrixKind::Kills
    );
    assert_eq!(
        support::selected_of(cx, s.win, ids::logstf_matrix_tab(MatrixKind::Kills)),
        Some(true)
    );
    for account in [ARIE, 98_265_577, 285_837_700] {
        assert!(
            support::present(cx, s.win, ids::logstf_matrix_row(sid(account))),
            "{account}"
        );
    }
    support::click_id(cx, s.win, ids::logstf_matrix_tab(MatrixKind::Deaths));
    assert_eq!(
        cx.update(|cx| page.read(cx).matrix_kind()),
        MatrixKind::Deaths
    );
    assert_eq!(
        support::selected_of(cx, s.win, ids::logstf_matrix_tab(MatrixKind::Deaths)),
        Some(true)
    );
    assert_eq!(
        support::selected_of(cx, s.win, ids::logstf_matrix_tab(MatrixKind::Kills)),
        Some(false)
    );
}

#[gpui_kit::test]
fn chat_and_killstreaks_open_in_dialogs(cx: &mut TestAppContext) {
    let (_env, _server, s, page) = opened(cx, 3);
    scroll_to_in_log(cx, &s, &page, 3, ids::LOGSTF_CHAT_TOGGLE);
    assert_eq!(
        support::label_of(cx, s.win, ids::LOGSTF_STREAKS_TOGGLE).as_deref(),
        Some("Show Killstreaks (13)")
    );
    support::click_id(cx, s.win, ids::LOGSTF_CHAT_TOGGLE);
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_chat_line(0)).as_deref(),
        Some("line between insane and genius: DRHAPPYWIPE")
    );
    // All 69 lines fit an 800x600 window only by scrolling inside the dialog.
    cx.simulate_window_resize(
        s.win,
        gpui_kit::size(gpui_kit::px(800.), gpui_kit::px(600.)),
    );
    let list = support::bounds_of(cx, s.win, ids::LOGSTF_CHAT).expect("the chat list");
    let last = support::bounds_of(cx, s.win, ids::logstf_chat_line(68)).expect("the last line");
    assert!(list.bottom() <= gpui_kit::px(600.), "{list:?}");
    assert!(
        last.bottom() > list.bottom(),
        "the chat scrolls: {last:?} {list:?}"
    );
    support::click_dialog_x(cx, s.win);
    scroll_to_in_log(cx, &s, &page, 3, ids::LOGSTF_CHAT_TOGGLE);
    support::click_id(cx, s.win, ids::LOGSTF_STREAKS_TOGGLE);
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_streak(0)).as_deref(),
        Some("0:39 ORANGEjuice 3")
    );
}

#[gpui_kit::test]
fn a_log_with_only_the_required_keys_opens_with_its_sections_left_out(cx: &mut TestAppContext) {
    const MINIMAL: &str = r#"{"length": 60, "teams": {"Red": {}, "Blue": {}},
        "players": {"[U:1:231702]": {"team": "Red",
            "class_stats": [{"type": "scout", "total_time": 60}]}}}"#;
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes("/api/v1/log", list_body(&[(9, day_ago(1))]));
    server.mount_bytes("/json/9", MINIMAL.as_bytes().to_vec());
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    open_log(cx, &s, &page, 9);
    assert!(
        support::present(cx, s.win, ids::logstf_banner(9)),
        "the log opens"
    );
    assert!(
        !support::present(cx, s.win, ids::LOGSTF_ROUNDS),
        "no rounds section"
    );
    assert!(
        !support::present(cx, s.win, ids::LOGSTF_MEDICS),
        "no medic section"
    );
    assert!(
        support::present(cx, s.win, ids::logstf_matrix_row(sid(ARIE))),
        "matrix rows read as zeros"
    );
    assert!(
        support::disabled_of(cx, s.win, ids::LOGSTF_STREAKS_TOGGLE),
        "nothing to show"
    );
}
