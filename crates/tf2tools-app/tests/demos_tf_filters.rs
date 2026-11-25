//! The Demos.tf toolbar: range, format, map and tracked players.
use std::collections::HashMap;

use gpui_kit::component::slider::{SliderEvent, SliderValue};
use gpui_kit::{AppContext as _, TestAppContext};
use tf2tools_app::ids;
use tf2tools_core::{DemoFormat, DemosRange};

use crate::support;
use crate::support::demostf::{
    Demo, DemosTf, demo, demos_tf_shell, first_queries, ids_down, list_body, list_queries,
    mount_before, mount_first, shown_ids, wait_end, wait_ids,
};
use crate::support::logstf::unix_now;

fn release(cx: &mut TestAppContext, d: &DemosTf, stop: f32, emit: bool) {
    let slider = cx.update(|cx| d.page.read(cx).range_slider().clone());
    cx.update_window(d.s.win, |_, window, cx| {
        slider.update(cx, |st, cx| {
            st.set_value(stop, window, cx);
            if emit {
                cx.emit(SliderEvent::Release(SliderValue::Single(stop)));
            }
        })
    })
    .unwrap();
    support::settle(cx, d.s.win);
}

fn firsts(
    cx: &mut TestAppContext,
    server: &support::StoreServer,
    n: usize,
) -> Vec<HashMap<String, String>> {
    support::wait_real(cx, "a restart", |_| first_queries(server).len() >= n);
    first_queries(server)
}

fn after_is(q: &HashMap<String, String>, days: i64) {
    let after: i64 = q["after"].parse().unwrap();
    assert!((after - (unix_now() - days * 86_400)).abs() < 120, "{q:?}");
}

// demos.tf listing demo 5 for any first page, then nothing older.
fn one_demo(cx: &mut TestAppContext) -> (support::Env, support::StoreServer, DemosTf) {
    let (env, server) = (support::env(), support::StoreServer::start());
    mount_first(&server, list_body(&server.uri(), &[demo(5)]));
    mount_before(&server, 5, 200, b"[]".to_vec());
    let d = demos_tf_shell(cx, &env, &server.uri(), &[]);
    wait_ids(cx, &d.page, &[5]);
    wait_end(cx, &d.page);
    (env, server, d)
}

fn pick_format(cx: &mut TestAppContext, d: &DemosTf, value: &str) {
    support::click_id(cx, d.s.win, ids::DEMOS_TF_FORMAT);
    support::click_id(cx, d.s.win, ids::select_option(ids::DEMOS_TF_FORMAT, value));
}

#[gpui_kit::test]
fn the_range_commits_on_release_and_all_has_no_start(cx: &mut TestAppContext) {
    let (_env, server, d) = one_demo(cx);
    assert_eq!(cx.update(|cx| d.page.read(cx).range()), DemosRange::Week1);
    release(cx, &d, 2., false);
    assert_eq!(first_queries(&server).len(), 1, "a drag alone asks nothing");
    release(cx, &d, 2., true);
    after_is(&firsts(cx, &server, 2)[1], 30);
    assert_eq!(cx.update(|cx| d.page.read(cx).range()), DemosRange::Month1);
    support::click_id(cx, d.s.win, ids::demos_tf_range_stop(DemosRange::All));
    let q = firsts(cx, &server, 3);
    assert!(!q[2].contains_key("after"), "{:?}", q[2]);
}

#[gpui_kit::test]
fn an_empty_range_widens_one_stop(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    mount_first(&server, b"[]".to_vec());
    let d = demos_tf_shell(cx, &env, &server.uri(), &[]);
    wait_end(cx, &d.page);
    support::settle(cx, d.s.win);
    assert_eq!(
        support::label_of(cx, d.s.win, ids::DEMOS_TF_WIDEN).as_deref(),
        Some("Look further back")
    );
    support::click_id(cx, d.s.win, ids::DEMOS_TF_WIDEN);
    after_is(&firsts(cx, &server, 2)[1], 30);
    assert_eq!(cx.update(|cx| d.page.read(cx).range()), DemosRange::Month1);
    support::click_id(cx, d.s.win, ids::demos_tf_range_stop(DemosRange::All));
    firsts(cx, &server, 3);
    wait_end(cx, &d.page);
    support::settle(cx, d.s.win);
    assert!(support::present(cx, d.s.win, ids::DEMOS_TF_EMPTY));
    assert!(
        !support::present(cx, d.s.win, ids::DEMOS_TF_WIDEN),
        "All has nothing wider"
    );
}

#[gpui_kit::test]
fn picking_a_format_asks_demos_tf_for_it(cx: &mut TestAppContext) {
    let (_env, server, d) = one_demo(cx);
    pick_format(cx, &d, "6v6");
    assert_eq!(firsts(cx, &server, 2)[1]["type"], "6v6");
    assert_eq!(
        cx.update(|cx| d.page.read(cx).format()),
        Some(DemoFormat::Sixes)
    );
    wait_ids(cx, &d.page, &[5]);
}

#[gpui_kit::test]
fn a_format_demos_tf_cannot_filter_skips_pages_without_it(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let uri = server.uri();
    mount_first(&server, list_body(&uri, &ids_down(1000, 50)));
    let bball = Demo {
        map: "ctf_bball_alpine_b4",
        players: 2,
        ..demo(900)
    };
    mount_before(&server, 951, 200, list_body(&uri, &[bball, demo(899)]));
    mount_before(&server, 899, 200, b"[]".to_vec());
    let d = demos_tf_shell(cx, &env, &uri, &[]);
    support::wait_real(cx, "fifty rows", |cx| shown_ids(cx, &d.page).len() == 50);
    let before = list_queries(&server).len();
    pick_format(cx, &d, "bball");
    wait_ids(cx, &d.page, &[900]);
    wait_end(cx, &d.page);
    let asked = &list_queries(&server)[before..];
    assert!(asked.iter().all(|q| !q.contains_key("type")), "{asked:?}");
    assert_eq!(asked.len(), 3, "first, before 951, before 899: {asked:?}");
}

#[gpui_kit::test]
fn the_skip_limit_offers_keep_looking_and_retry_resumes_there(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    let uri = server.uri();
    // Ten raw pages of 6v6 demos, 1000 down to 501: the skip limit, with no BBall in them.
    mount_first(&server, list_body(&uri, &ids_down(1000, 50)));
    for k in 1..10u64 {
        mount_before(
            &server,
            1001 - 50 * k,
            200,
            list_body(&uri, &ids_down(1000 - 50 * k, 50)),
        );
    }
    let d = demos_tf_shell(cx, &env, &uri, &[]);
    support::wait_real(cx, "fifty rows", |cx| shown_ids(cx, &d.page).len() == 50);
    let before = list_queries(&server).len();
    pick_format(cx, &d, "bball");
    support::wait_real(cx, "Keep looking", |cx| {
        support::present(cx, d.s.win, ids::DEMOS_TF_KEEP_LOOKING)
    });
    assert_eq!(
        list_queries(&server).len() - before,
        10,
        "ten raw pages, then it waits"
    );
    assert!(shown_ids(cx, &d.page).is_empty());
    assert!(
        !cx.update(|cx| d.page.read(cx).is_end()),
        "the limit is not the end"
    );
    assert!(!support::present(cx, d.s.win, ids::DEMOS_TF_EMPTY));
    support::settle(cx, d.s.win);
    assert_eq!(
        list_queries(&server).len() - before,
        10,
        "Keep looking waits for a click"
    );
    // The next page fails once; Retry asks for it again, not the skipped pages.
    mount_before(&server, 501, 500, Vec::new());
    support::click_id(cx, d.s.win, ids::DEMOS_TF_KEEP_LOOKING);
    support::wait_real(cx, "the error", |cx| {
        support::present(cx, d.s.win, ids::DEMOS_TF_ERROR)
    });
    server.reset();
    let bball = Demo {
        map: "ctf_bball_alpine_b4",
        players: 2,
        ..demo(450)
    };
    mount_before(&server, 501, 200, list_body(&uri, &[bball]));
    mount_before(&server, 450, 200, b"[]".to_vec());
    support::click_id(cx, d.s.win, ids::DEMOS_TF_RETRY);
    wait_ids(cx, &d.page, &[450]);
    assert_eq!(
        list_queries(&server)[0]["before_id"],
        "501",
        "Retry resumes at the failed page"
    );
}
