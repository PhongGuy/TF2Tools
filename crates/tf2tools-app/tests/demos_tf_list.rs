//! The Demos.tf list: its first page, rows, states and paging.
use gpui_kit::TestAppContext;
use tf2tools_app::ids;

use crate::support;
use crate::support::demostf::{
    Demo, demo, demos_tf_shell, ids_down, list_body, list_queries, mount_before, mount_first,
    shown_ids, to_end, wait_end, wait_ids, wait_quiet,
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

#[gpui_kit::test]
fn scrolling_to_the_end_loads_older_demos_by_id_without_repeats(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let uri = server.uri();
    mount_first(&server, list_body(&uri, &ids_down(1000, 50)));
    // Two new demos shifted the list, so the next page repeats 952 and 951.
    mount_before(&server, 951, 200, list_body(&uri, &ids_down(952, 5)));
    mount_before(&server, 948, 200, b"[]".to_vec());
    let d = demos_tf_shell(cx, &env, &uri, &[]);
    let first: Vec<u64> = (951..=1000).rev().collect();
    wait_ids(cx, &d.page, &first);
    support::settle(cx, d.s.win);
    assert_eq!(list_queries(&server).len(), 1, "fifty rows fill the window");
    to_end(cx, &d);
    let mut all = first.clone();
    all.extend([950, 949, 948]);
    wait_ids(cx, &d.page, &all);
    to_end(cx, &d);
    wait_end(cx, &d.page);
    let q = list_queries(&server);
    assert_eq!(q.len(), 3, "{q:?}");
    assert_eq!(q[1]["before_id"], "951");
    assert_eq!(q[2]["before_id"], "948");
    assert_eq!(
        q[1]["after"], q[0]["after"],
        "paging keeps the range's start"
    );
    to_end(cx, &d);
    assert_eq!(list_queries(&server).len(), 3, "the end asks no more");
}

#[gpui_kit::test]
fn a_failed_next_page_keeps_the_rows_and_retries_that_page(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let uri = server.uri();
    mount_first(&server, list_body(&uri, &ids_down(1000, 50)));
    mount_before(&server, 951, 500, Vec::new());
    let d = demos_tf_shell(cx, &env, &uri, &[]);
    wait_ids(cx, &d.page, &(951..=1000).rev().collect::<Vec<_>>());
    to_end(cx, &d);
    support::wait_real(cx, "the error", |cx| {
        support::present(cx, d.s.win, ids::DEMOS_TF_ERROR)
    });
    assert_eq!(shown_ids(cx, &d.page).len(), 50);
    let asked = list_queries(&server).len();
    to_end(cx, &d);
    support::settle(cx, d.s.win);
    assert_eq!(
        list_queries(&server).len(),
        asked,
        "an error waits for Retry"
    );
    server.reset();
    mount_before(&server, 951, 200, list_body(&uri, &ids_down(950, 2)));
    mount_before(&server, 949, 200, b"[]".to_vec());
    support::click_id(cx, d.s.win, ids::DEMOS_TF_RETRY);
    support::wait_real(cx, "the retried page", |cx| {
        shown_ids(cx, &d.page).len() == 52
    });
    assert_eq!(
        list_queries(&server)[0]["before_id"],
        "951",
        "Retry asks for the failed page"
    );
}
