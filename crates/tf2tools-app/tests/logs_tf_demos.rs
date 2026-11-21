//! A log's demo on demos.tf: the lookup when a log opens, the footer buttons and the download.

use std::path::PathBuf;
use std::sync::Arc;

use gpui_kit::{Entity, ListOffset, TestAppContext, px};
use serde_json::json;
use tf2tools_app::ids;
use tf2tools_app::pages::logs_tf::{DemoLookup, LogsTfPage};
use tf2tools_app::picker::{PickKind, ScriptedPicker};

use crate::support;
use crate::support::logstf::{
    ARIE, Gate, expanded, mount_held, open_log, open_page, requests_to, scroll_to_in_log,
    serve_two_logs, shell_with_demos,
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
    assert!(
        matches!(lookup(cx, &page, 3), Some(DemoLookup::Found(_))),
        "reopening keeps the found demo rather than looking again"
    );
    assert_eq!(
        requests_to(&server, "/demos/"),
        2,
        "reopening log 3 asks demos.tf nothing more"
    );
}

/// `demos` with log 3 open, its lookup finished, scrolled to its footer.
fn at_footer(
    cx: &mut TestAppContext,
    list: Option<fn(&str) -> Vec<u8>>,
    picks: Vec<PathBuf>,
) -> Demos {
    let d = demos(cx, list, picks);
    open_log(cx, &d.s, &d.page, 3);
    settled(cx, &d.page, 3);
    scroll_to_in_log(cx, &d.s, &d.page, 3, ids::logstf_download(3));
    d
}

fn toast(cx: &mut TestAppContext, d: &Demos) -> String {
    support::wait_real(cx, "the toast", |cx| {
        support::present(cx, d.s.win, ids::TOAST)
    });
    support::label_of(cx, d.s.win, ids::TOAST).unwrap()
}

#[gpui_kit::test]
fn a_found_demo_offers_its_download_and_its_page(cx: &mut TestAppContext) {
    let d = at_footer(cx, Some(sixes_list), Vec::new());
    let download = support::label_of(cx, d.s.win, ids::logstf_demo_download(3));
    assert_eq!(download.as_deref(), Some("Download demo"));
    assert!(!support::disabled_of(
        cx,
        d.s.win,
        ids::logstf_demo_download(3)
    ));
    let link = support::label_of(cx, d.s.win, ids::logstf_demo_link(3));
    assert_eq!(link.as_deref(), Some("demos.tf"));
}

#[gpui_kit::test]
fn no_demo_shows_neither_button(cx: &mut TestAppContext) {
    let d = at_footer(cx, Some(empty_list), Vec::new());
    assert!(
        support::present(cx, d.s.win, ids::logstf_download(3)),
        "Download log stays"
    );
    assert!(!support::present(cx, d.s.win, ids::logstf_demo_download(3)));
    assert!(!support::present(cx, d.s.win, ids::logstf_demo_link(3)));
}

#[gpui_kit::test]
fn another_logs_demo_never_shows_on_this_one(cx: &mut TestAppContext) {
    let d = demos(cx, Some(sixes_list), Vec::new());
    open_log(cx, &d.s, &d.page, 3);
    assert!(matches!(settled(cx, &d.page, 3), DemoLookup::Found(_)));
    open_log(cx, &d.s, &d.page, 2);
    assert_eq!(settled(cx, &d.page, 2), DemoLookup::Missing);
    scroll_to_in_log(cx, &d.s, &d.page, 2, ids::logstf_download(2));
    assert!(!support::present(cx, d.s.win, ids::logstf_demo_download(2)));
    assert!(
        !support::present(cx, d.s.win, ids::logstf_demo_download(3)),
        "log 3 is closed"
    );
}

#[gpui_kit::test]
fn download_demo_saves_it_where_the_player_chose(cx: &mut TestAppContext) {
    let target = tempfile::tempdir().unwrap();
    let path = target.path().join("my demo.dem");
    let d = at_footer(cx, Some(sixes_list), vec![path.clone()]);
    let demo: Vec<u8> = (0..4096u32).map(|i| (i % 251) as u8).collect();
    d.server.mount_bytes(DEMO_ROUTE, demo.clone());
    support::click_id(cx, d.s.win, ids::logstf_demo_download(3));
    let said = toast(cx, &d);
    assert!(said.starts_with("Demo downloaded"), "{said}");
    assert_eq!(std::fs::read(&path).unwrap(), demo);
    assert_eq!(d.picker.suggested(), [DEMO_NAME]);
    assert_eq!(d.picker.asked(), [PickKind::Demo]);
    support::wait_real(cx, "the download to end", |cx| {
        cx.update(|cx| d.page.read(cx).demo_progress(3).is_none())
    });
    let label = support::label_of(cx, d.s.win, ids::logstf_demo_download(3));
    assert_eq!(label.as_deref(), Some("Download demo"));
}

#[gpui_kit::test]
fn a_running_download_disables_the_button_and_says_so(cx: &mut TestAppContext) {
    let target = tempfile::tempdir().unwrap();
    let path = target.path().join("m.dem");
    let d = at_footer(cx, Some(sixes_list), vec![path.clone()]);
    let gate = Gate::default();
    mount_held(&d.server, &gate, DEMO_ROUTE, None, b"demo".to_vec());
    support::click_id(cx, d.s.win, ids::logstf_demo_download(3));
    support::wait_real(cx, "the demo request", |_| gate.arrived() == 1);
    let label = support::label_of(cx, d.s.win, ids::logstf_demo_download(3));
    assert_eq!(
        label.as_deref(),
        Some("Downloading demo"),
        "no size known yet, so no percent"
    );
    assert!(support::disabled_of(
        cx,
        d.s.win,
        ids::logstf_demo_download(3)
    ));
    gate.open();
    let said = toast(cx, &d);
    assert!(said.starts_with("Demo downloaded"), "{said}");
    assert_eq!(std::fs::read(&path).unwrap(), b"demo");
}

#[gpui_kit::test]
fn a_failed_download_says_why_and_writes_nothing(cx: &mut TestAppContext) {
    let target = tempfile::tempdir().unwrap();
    let path = target.path().join("m.dem");
    let d = at_footer(cx, Some(sixes_list), vec![path.clone()]);
    d.server.mount_status(DEMO_ROUTE, 404);
    support::click_id(cx, d.s.win, ids::logstf_demo_download(3));
    let said = toast(cx, &d);
    assert!(said.starts_with("Couldn't download this demo."), "{said}");
    assert!(said.contains("demos.tf no longer has this demo."), "{said}");
    support::assert_plain(&said);
    assert!(!path.exists(), "nothing is written");
    assert_eq!(cx.update(|cx| d.page.read(cx).demo_progress(3)), None);
}

#[gpui_kit::test]
fn a_cancelled_save_downloads_nothing(cx: &mut TestAppContext) {
    let d = at_footer(cx, Some(sixes_list), Vec::new());
    d.server.mount_bytes(DEMO_ROUTE, b"demo".to_vec());
    support::click_id(cx, d.s.win, ids::logstf_demo_download(3));
    support::wait_real(cx, "the save picker is asked", |_| {
        !d.picker.suggested().is_empty()
    });
    support::settle(cx, d.s.win);
    assert!(!support::present(cx, d.s.win, ids::TOAST), "no toast");
    assert_eq!(requests_to(&d.server, DEMO_ROUTE), 0, "nothing downloaded");
}
