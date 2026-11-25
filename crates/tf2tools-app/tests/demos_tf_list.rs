//! The Demos.tf list: its first page, rows, states and paging.
use gpui_kit::TestAppContext;
use tf2tools_app::ids;

use crate::support;
use crate::support::demostf::{
    Demo, demo, demos_tf_shell, list_body, list_queries, mount_before, mount_first, wait_end,
    wait_ids, wait_quiet,
};
use crate::support::logstf::{Gate, mount_held, unix_now};

#[gpui_kit::test]
fn the_list_shows_the_last_week_newest_first(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let hl = Demo {
        map: "koth_proot_b5b",
        players: 18,
        ..demo(11)
    };
    mount_first(&server, list_body(&server.uri(), &[demo(12), hl]));
    // Two rows leave the footer in view; once it pages it asks for the page before 11.
    mount_before(&server, 11, 200, b"[]".to_vec());
    let d = demos_tf_shell(cx, &env, &server.uri(), &[]);
    wait_ids(cx, &d.page, &[12, 11]);
    wait_quiet(cx, &d);
    let label = |cx: &mut TestAppContext, id| support::label_of(cx, d.s.win, id);
    assert_eq!(
        label(cx, ids::demos_tf_row_title(12)).as_deref(),
        Some("serveme.tf #7 - RED vs BLU")
    );
    assert_eq!(
        label(cx, ids::demos_tf_row_format(12)).as_deref(),
        Some("6v6")
    );
    assert_eq!(
        label(cx, ids::demos_tf_row_format(11)).as_deref(),
        Some("Highlander")
    );
    assert_eq!(
        label(cx, ids::demos_tf_row_map(11)).as_deref(),
        Some("koth_proot_b5b")
    );
    assert_eq!(
        label(cx, ids::demos_tf_row_duration(12)).as_deref(),
        Some("28:34")
    );
    assert!(support::present(cx, d.s.win, ids::demos_tf_row_date(12)));
    assert!(support::present(cx, d.s.win, ids::DEMOS_TF_OPEN));
    assert!(!support::present(cx, d.s.win, ids::DEMOS_TF_LOADING));
    let q = &list_queries(&server)[0];
    let after: i64 = q["after"].parse().unwrap();
    assert!((after - (unix_now() - 7 * 86_400)).abs() < 120, "{q:?}");
    assert!(
        !q.contains_key("before_id") && !q.contains_key("page") && !q.contains_key("players"),
        "{q:?}"
    );
}

#[gpui_kit::test]
fn the_list_says_it_is_loading_until_the_first_page_lands(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let gate = Gate::default();
    mount_held(
        &server,
        &gate,
        "/demos/",
        None,
        list_body(&server.uri(), &[demo(5)]),
    );
    let d = demos_tf_shell(cx, &env, &server.uri(), &[]);
    support::wait_real(cx, "the list request", |_| gate.arrived() >= 1);
    support::settle(cx, d.s.win);
    assert_eq!(
        support::label_of(cx, d.s.win, ids::DEMOS_TF_LOADING).as_deref(),
        Some("Loading demos...")
    );
    gate.open();
    wait_ids(cx, &d.page, &[5]);
}

#[gpui_kit::test]
fn an_empty_range_says_so_and_asks_once(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    mount_first(&server, b"[]".to_vec());
    let d = demos_tf_shell(cx, &env, &server.uri(), &[]);
    wait_end(cx, &d.page);
    support::settle(cx, d.s.win);
    assert_eq!(
        support::label_of(cx, d.s.win, ids::DEMOS_TF_EMPTY).as_deref(),
        Some("No demos in this range")
    );
    assert!(!support::present(cx, d.s.win, ids::DEMOS_TF_LOADING));
    assert_eq!(list_queries(&server).len(), 1);
}

#[gpui_kit::test]
fn a_failed_list_waits_for_retry(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_status("/demos/", 500);
    let d = demos_tf_shell(cx, &env, &server.uri(), &[]);
    support::wait_real(cx, "the error", |cx| {
        support::present(cx, d.s.win, ids::DEMOS_TF_ERROR)
    });
    assert_eq!(
        support::label_of(cx, d.s.win, ids::DEMOS_TF_ERROR).as_deref(),
        Some("demos.tf isn't responding right now. Try again later.")
    );
    let asked = list_queries(&server).len();
    support::settle(cx, d.s.win);
    support::settle(cx, d.s.win);
    assert_eq!(
        list_queries(&server).len(),
        asked,
        "an error waits for Retry"
    );
    server.reset();
    mount_first(&server, list_body(&server.uri(), &[demo(9)]));
    // One row leaves the footer in view; once it pages it asks for the page before 9.
    mount_before(&server, 9, 200, b"[]".to_vec());
    support::click_id(cx, d.s.win, ids::DEMOS_TF_RETRY);
    wait_ids(cx, &d.page, &[9]);
    wait_quiet(cx, &d);
    assert!(!support::present(cx, d.s.win, ids::DEMOS_TF_ERROR));
}
