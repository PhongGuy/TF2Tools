//! View demo from a logs.tf log: the Demos.tf viewer it opens and the way back to the log.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use gpui_kit::{AppContext as _, Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::logs_tf::{DemoLookup, LogsTfPage};
use tf2tools_app::shell::route::Page;
use tf2tools_core::DemoCache;

use crate::support;
use crate::support::demo_view_fixtures::demo_header;
use crate::support::demo_viewer::{StubHandle, demo_file, install_map};
use crate::support::demostf::{DemosTf, demos_tf_shell, mount_first};
use crate::support::logstf::{
    ARIE, DEMO_ID, DEMO_ROUTE, Gate, mount_held, open_log, open_page, scroll_to_in_log,
    serve_two_logs, shell_with_demo_viewer, sixes_list,
};

fn file() -> Vec<u8> {
    demo_header("cp_synthetic", 60.0)
}

// The Demos.tf page over an empty demos.tf list, and a cp_synthetic demo on disk whose map is
// installed.
fn demos_page(cx: &mut TestAppContext) -> (support::Env, support::StoreServer, DemosTf, PathBuf) {
    let (env, server) = (support::env(), support::StoreServer::start());
    install_map(&env, "cp_synthetic");
    mount_first(&server, b"[]".to_vec());
    let d = demos_tf_shell(cx, &env, &server.uri(), &[]);
    let path = demo_file(&env, "logs.dem", &file());
    (env, server, d, path)
}

// Opens `path` the way logs.tf's View demo does, then waits for the viewport.
fn open_from_logs(cx: &mut TestAppContext, d: &DemosTf, path: PathBuf) {
    let page = d.page.clone();
    cx.update_window(d.s.win, |_, window, cx| {
        page.update(cx, |page, cx| {
            page.open_from(path, Page::LogsTf, window, cx)
        });
    })
    .unwrap();
    support::wait_real(cx, "the viewport", |cx| {
        support::present(cx, d.s.win, ids::DEMO_VIEWER_VIEWPORT)
    });
}

#[gpui_kit::test]
fn a_demo_opened_from_logs_tf_offers_the_way_back_to_the_log(cx: &mut TestAppContext) {
    let (_env, _server, d, path) = demos_page(cx);
    open_from_logs(cx, &d, path);
    assert_eq!(
        support::label_of(cx, d.s.win, ids::DEMOS_TF_CRUMB).as_deref(),
        Some("Back to the log")
    );
    assert_eq!(
        support::label_of(cx, d.s.win, ids::DEMOS_TF_CRUMB_NAME).as_deref(),
        Some("cp_synthetic")
    );
    assert_eq!(cx.update(|cx| d.page.read(cx).origin()), Some(Page::LogsTf));
}

#[gpui_kit::test]
fn back_from_a_logs_tf_demo_returns_to_logs_tf_and_frees_the_viewer(cx: &mut TestAppContext) {
    let (_env, _server, d, path) = demos_page(cx);
    open_from_logs(cx, &d, path);
    support::click_id(cx, d.s.win, ids::DEMOS_TF_CRUMB);
    support::settle(cx, d.s.win);
    assert_eq!(cx.update(|cx| d.s.shell.read(cx).page()), Page::LogsTf);
    assert!(cx.update(|cx| d.page.read(cx).viewer().is_none()));
    assert_eq!(cx.update(|cx| d.page.read(cx).origin()), None);
    let stubs = d.stubs.lock().unwrap();
    assert_eq!(stubs.len(), 1);
    assert!(!stubs[0].is_alive(), "the viewer's renderer is freed");
}

#[gpui_kit::test]
fn leaving_a_logs_tf_demo_by_the_sidebar_goes_where_the_player_clicked(cx: &mut TestAppContext) {
    let (_env, _server, d, path) = demos_page(cx);
    open_from_logs(cx, &d, path);
    support::click_nav(cx, d.s.win, Page::Settings);
    support::settle(cx, d.s.win);
    assert_eq!(cx.update(|cx| d.s.shell.read(cx).page()), Page::Settings);
    assert!(cx.update(|cx| d.page.read(cx).viewer().is_none()));
    assert_eq!(cx.update(|cx| d.page.read(cx).origin()), None);
    support::click_nav(cx, d.s.win, Page::DemosTf);
    support::settle(cx, d.s.win);
    assert!(support::present(cx, d.s.win, ids::DEMOS_TF_LIST));
    assert!(!support::present(cx, d.s.win, ids::DEMOS_TF_CRUMB));
}

struct Viewing {
    env: support::Env,
    _logs: support::StoreServer,
    server: support::StoreServer,
    s: support::ShellWindow,
    page: Entity<LogsTfPage>,
    stubs: Arc<Mutex<Vec<StubHandle>>>,
}

// Arie's logs on a found TF2 with stub renderers, demos.tf holding log 3's demo and the
// cp_synthetic map installed; log 3 open, its lookup found, scrolled to its footer.
fn viewing(cx: &mut TestAppContext) -> Viewing {
    let env = support::env();
    install_map(&env, "cp_synthetic");
    let logs = support::StoreServer::start();
    serve_two_logs(&logs);
    let server = support::StoreServer::start();
    server.mount_bytes("/demos/", sixes_list(&server.uri()));
    let (s, stubs) =
        shell_with_demo_viewer(cx, &env, &logs.uri(), &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    open_log(cx, &s, &page, 3);
    support::wait_real(cx, "log 3's demo lookup", |cx| {
        cx.update(|cx| matches!(page.read(cx).demo_lookup(3), Some(DemoLookup::Found(_))))
    });
    scroll_to_in_log(cx, &s, &page, 3, ids::logstf_download(3));
    Viewing {
        env,
        _logs: logs,
        server,
        s,
        page,
        stubs,
    }
}

fn view_progress(cx: &mut TestAppContext, v: &Viewing) -> Option<Option<u8>> {
    cx.update(|cx| v.page.read(cx).view_progress(3))
}

#[gpui_kit::test]
fn a_found_demo_offers_view_demo_before_its_download(cx: &mut TestAppContext) {
    let v = viewing(cx);
    assert_eq!(
        support::label_of(cx, v.s.win, ids::logstf_demo_view(3)).as_deref(),
        Some("View demo")
    );
    assert!(!support::disabled_of(cx, v.s.win, ids::logstf_demo_view(3)));
    let view = support::bounds_of(cx, v.s.win, ids::logstf_demo_view(3)).unwrap();
    let download = support::bounds_of(cx, v.s.win, ids::logstf_demo_download(3)).unwrap();
    assert!(
        view.top() < download.top()
            || (view.top() == download.top() && view.left() < download.left()),
        "View demo {view:?} comes before Download demo {download:?}"
    );
}

#[gpui_kit::test]
fn an_opening_demo_says_so_and_lands_in_the_cache(cx: &mut TestAppContext) {
    let v = viewing(cx);
    let gate = Gate::default();
    mount_held(&v.server, &gate, DEMO_ROUTE, None, file());
    support::click_id(cx, v.s.win, ids::logstf_demo_view(3));
    support::wait_real(cx, "the demo request", |_| gate.arrived() == 1);
    assert_eq!(
        support::label_of(cx, v.s.win, ids::logstf_demo_view(3)).as_deref(),
        Some("Opening demo"),
        "no size known yet, so no percent"
    );
    assert!(support::disabled_of(cx, v.s.win, ids::logstf_demo_view(3)));
    assert_eq!(view_progress(cx, &v), Some(None));
    gate.open();
    support::wait_real(cx, "the open to end", |cx| view_progress(cx, &v).is_none());
    assert_eq!(gate.arrived(), 1);
    let cached = DemoCache::new(v.env.data.demo_cache_dir()).path_for(DEMO_ID);
    assert_eq!(std::fs::read(cached).unwrap(), file());
}

#[gpui_kit::test]
fn a_failed_open_says_why_and_stays_on_the_log(cx: &mut TestAppContext) {
    let v = viewing(cx);
    v.server.mount_status(DEMO_ROUTE, 404);
    support::click_id(cx, v.s.win, ids::logstf_demo_view(3));
    support::wait_real(cx, "the toast", |cx| {
        support::present(cx, v.s.win, ids::TOAST)
    });
    let said = support::label_of(cx, v.s.win, ids::TOAST).unwrap();
    assert!(said.starts_with("Couldn't download this demo."), "{said}");
    assert!(said.contains("demos.tf no longer has this demo."), "{said}");
    support::wait_real(cx, "the open to end", |cx| view_progress(cx, &v).is_none());
    assert_eq!(cx.update(|cx| v.s.shell.read(cx).page()), Page::LogsTf);
    assert!(v.stubs.lock().unwrap().is_empty(), "no viewer was built");
}
