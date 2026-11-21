//! A log's demo on demos.tf: the lookup when a log opens, the footer buttons and the download.

use std::path::PathBuf;
use std::sync::Arc;

use gpui_kit::{Entity, ListOffset, TestAppContext, px};
use serde_json::json;
use tf2tools_app::ids;
use tf2tools_app::pages::logs_tf::{DemoLookup, LogsTfPage};
use tf2tools_app::picker::ScriptedPicker;

use crate::support;
use crate::support::logstf::{
    ARIE, Gate, expanded, mount_held, open_log, open_page, requests_to, serve_two_logs,
    shell_with_demos,
};

// Log 3 is the 6v6 fixture: cp_gullywash_f9, uploaded 1792201829, 1714 s long. Log 2 is
// koth_proside_v1, so the same list never matches it.
const SIXES_MAP: &str = "cp_gullywash_f9";
const SIXES_DATE: i64 = 1_792_201_829;
const SIXES_LENGTH: u32 = 1714;
const DEMO_ID: u64 = 777;
const DEMO_NAME: &str = "match-20261017-2110-cp_gullywash_f9.dem";
const DEMO_ROUTE: &str = "/files/ab/777.dem";

/// A demos.tf list holding log 3's demo, its file on the demos.tf server at `base`.
fn sixes_list(base: &str) -> Vec<u8> {
    json!([{"id": DEMO_ID, "url": format!("{base}{DEMO_ROUTE}"), "name": DEMO_NAME,
            "server": "serveme.tf #1", "duration": SIXES_LENGTH + 20, "map": SIXES_MAP,
            "time": SIXES_DATE + 60, "playerCount": 12}])
    .to_string()
    .into_bytes()
}

fn empty_list(_: &str) -> Vec<u8> {
    b"[]".to_vec()
}

struct Demos {
    _env: support::Env,
    _logs: support::StoreServer,
    server: support::StoreServer,
    s: support::ShellWindow,
    page: Entity<LogsTfPage>,
    #[expect(dead_code, reason = "the footer's download tests read it")]
    picker: Arc<ScriptedPicker>,
}

/// Arie's two logs synced from one mock server; demos.tf on its own (`server`) answering
/// `list` (built from its address) or HTTP 500 for `None`; `picks` queued for save dialogs.
fn demos(cx: &mut TestAppContext, list: Option<fn(&str) -> Vec<u8>>, picks: Vec<PathBuf>) -> Demos {
    let env = support::env();
    let logs = support::StoreServer::start();
    serve_two_logs(&logs);
    let server = support::StoreServer::start();
    match list {
        Some(list) => server.mount_bytes("/demos/", list(&server.uri())),
        None => server.mount_status("/demos/", 500),
    }
    let picker = Arc::new(ScriptedPicker::new(picks));
    let players = [(ARIE, "Arie")];
    let s = shell_with_demos(
        cx,
        &env,
        &logs.uri(),
        &server.uri(),
        &players,
        Arc::clone(&picker),
    );
    let page = open_page(cx, &s);
    Demos {
        _env: env,
        _logs: logs,
        server,
        s,
        page,
        picker,
    }
}

fn lookup(cx: &mut TestAppContext, page: &Entity<LogsTfPage>, id: u64) -> Option<DemoLookup> {
    cx.update(|cx| page.read(cx).demo_lookup(id).cloned())
}

/// Log `id`'s lookup once it has finished.
fn settled(cx: &mut TestAppContext, page: &Entity<LogsTfPage>, id: u64) -> DemoLookup {
    support::wait_real(cx, "the demo lookup", |cx| {
        matches!(
            lookup(cx, page, id),
            Some(DemoLookup::Found(_) | DemoLookup::Missing)
        )
    });
    lookup(cx, page, id).unwrap()
}

/// Opens log `id`, whose row sits above the open log's panel, so `open_log`'s scroll to the
/// list's end would leave it out of view.
fn reopen_above(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<LogsTfPage>,
    id: u64,
) {
    let item_ix = cx
        .update(|cx| page.read(cx).rows().iter().position(|r| r.id() == id))
        .expect("the log is listed")
        + 1;
    let state = cx.update(|cx| page.read(cx).list_state().clone());
    state.scroll_to(ListOffset {
        item_ix,
        offset_in_item: px(0.),
    });
    support::settle(cx, s.win);
    support::click_id(cx, s.win, ids::logstf_log(id));
    support::wait_real(cx, "the reopened log", |cx| {
        cx.update(|cx| page.read(cx).expanded() == Some(id) && page.read(cx).detail().is_some())
    });
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn an_opened_log_finds_its_demo(cx: &mut TestAppContext) {
    let d = demos(cx, Some(sixes_list), Vec::new());
    open_log(cx, &d.s, &d.page, 3);
    match settled(cx, &d.page, 3) {
        DemoLookup::Found(demo) => assert_eq!(demo.id(), DEMO_ID),
        other => panic!("expected log 3's demo, got {other:?}"),
    }
    assert_eq!(requests_to(&d.server, "/demos/"), 1);
}

#[gpui_kit::test]
fn no_demo_on_demos_tf_is_missing(cx: &mut TestAppContext) {
    let d = demos(cx, Some(empty_list), Vec::new());
    open_log(cx, &d.s, &d.page, 3);
    assert_eq!(settled(cx, &d.page, 3), DemoLookup::Missing);
}

#[gpui_kit::test]
fn a_failing_demos_tf_is_missing(cx: &mut TestAppContext) {
    let d = demos(cx, None, Vec::new());
    open_log(cx, &d.s, &d.page, 3);
    assert_eq!(settled(cx, &d.page, 3), DemoLookup::Missing);
}

#[gpui_kit::test]
fn a_lookup_lands_on_its_own_log_and_is_never_repeated(cx: &mut TestAppContext) {
    let env = support::env();
    let logs = support::StoreServer::start();
    serve_two_logs(&logs);
    // demos.tf has its own server, so its held reply never blocks a logs.tf request.
    let server = support::StoreServer::start();
    let gate = Gate::default();
    mount_held(&server, &gate, "/demos/", None, sixes_list(&server.uri()));
    let picker = Arc::new(ScriptedPicker::empty());
    let s = shell_with_demos(
        cx,
        &env,
        &logs.uri(),
        &server.uri(),
        &[(ARIE, "Arie")],
        picker,
    );
    let page = open_page(cx, &s);
    open_log(cx, &s, &page, 3);
    support::wait_real(cx, "log 3's lookup reaches demos.tf", |_| {
        gate.arrived() == 1
    });
    assert_eq!(lookup(cx, &page, 3), Some(DemoLookup::Looking));
    // Log 2 opens while log 3's answer is held; both answers then arrive. Log 2 (koth_proside_v1,
    // 18 players, 2068 s, a real upload date) yields a query, so it asks demos.tf once too.
    open_log(cx, &s, &page, 2);
    gate.open();
    assert_eq!(settled(cx, &page, 2), DemoLookup::Missing);
    assert!(matches!(settled(cx, &page, 3), DemoLookup::Found(_)));
    assert_eq!(expanded(cx, &page), Some(2), "log 2 stays open");
    reopen_above(cx, &s, &page, 3);
    assert_eq!(
        requests_to(&server, "/demos/"),
        2,
        "reopening log 3 asks demos.tf nothing more"
    );
}
