//! The open log's sections, compared with the fixtures' logs.tf pages.

use std::time::Duration;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::logs_tf::LogsTfPage;
use tf2tools_app::pages::logs_tf::sort::{PlayerColumn, PlayerSort};
use tf2tools_core::TfClass;

use crate::support;
use crate::support::logstf::{
    ARIE, open_log, open_page, scroll_to_in_log, serve_two_logs, shell, sid,
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
