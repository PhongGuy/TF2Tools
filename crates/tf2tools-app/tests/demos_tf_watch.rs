//! Watch on a Demos.tf row: the cache, progress, the viewer and the way back.
use gpui_kit::{ListOffset, TestAppContext, px, size};
use tf2tools_app::ids;
use tf2tools_app::shell::route::Page;
use tf2tools_core::DemoCache;

use crate::support;
use crate::support::demo_view_fixtures::demo_header;
use crate::support::demo_viewer::{inside, install_map};
use crate::support::demostf::{
    Demo, DemosTf, demo, demos_tf_shell, ids_down, list_body, list_queries, mount_before,
    mount_first, shown_ids, wait_end, wait_ids,
};
use crate::support::logstf::{Gate, mount_held, requests_to};

fn synthetic(id: u64) -> Demo {
    Demo {
        map: "cp_synthetic",
        ..demo(id)
    }
}

fn file() -> Vec<u8> {
    demo_header("cp_synthetic", 60.0)
}

// demos.tf listing `demos`, then nothing older; the shell waits at the list's end.
fn served(
    cx: &mut TestAppContext,
    demos: &[Demo],
) -> (support::Env, support::StoreServer, DemosTf) {
    let (env, server) = (support::env(), support::StoreServer::start());
    install_map(&env, "cp_synthetic");
    mount_first(&server, list_body(&server.uri(), demos));
    let lowest = demos.iter().map(|d| d.id).min().unwrap();
    mount_before(&server, lowest, 200, b"[]".to_vec());
    let d = demos_tf_shell(cx, &env, &server.uri(), &[]);
    wait_ids(cx, &d.page, &demos.iter().map(|d| d.id).collect::<Vec<_>>());
    wait_end(cx, &d.page);
    support::settle(cx, d.s.win);
    (env, server, d)
}

fn viewport(cx: &mut TestAppContext, d: &DemosTf) {
    support::wait_real(cx, "the viewport", |cx| {
        support::present(cx, d.s.win, ids::DEMO_VIEWER_VIEWPORT)
    });
}

#[gpui_kit::test]
fn watch_downloads_the_demo_and_opens_it(cx: &mut TestAppContext) {
    let (env, server, d) = served(cx, &[synthetic(7)]);
    server.mount_bytes("/files/7.dem", file());
    assert_eq!(
        support::label_of(cx, d.s.win, ids::demos_tf_watch(7)).as_deref(),
        Some("Watch")
    );
    support::click_id(cx, d.s.win, ids::demos_tf_watch(7));
    viewport(cx, &d);
    assert!(!support::present(cx, d.s.win, ids::DEMOS_TF_LIST));
    assert_eq!(
        support::label_of(cx, d.s.win, ids::DEMOS_TF_CRUMB_NAME).as_deref(),
        Some("cp_synthetic")
    );
    let cached = DemoCache::new(env.data.demo_cache_dir()).path_for(7);
    assert_eq!(std::fs::read(cached).unwrap(), file());
    assert_eq!(cx.update(|cx| d.page.read(cx).download_progress(7)), None);
}

#[gpui_kit::test]
fn a_cached_demo_opens_without_asking_demos_tf(cx: &mut TestAppContext) {
    let (env, server, d) = served(cx, &[synthetic(7)]);
    let cached = DemoCache::new(env.data.demo_cache_dir()).path_for(7);
    std::fs::create_dir_all(cached.parent().unwrap()).unwrap();
    std::fs::write(&cached, file()).unwrap();
    support::click_id(cx, d.s.win, ids::demos_tf_watch(7));
    viewport(cx, &d);
    assert_eq!(requests_to(&server, "/files/7.dem"), 0);
}

#[gpui_kit::test]
fn a_private_demo_cannot_be_watched(cx: &mut TestAppContext) {
    let (_env, _server, d) = served(
        cx,
        &[Demo {
            public: false,
            ..synthetic(7)
        }],
    );
    assert_eq!(
        support::label_of(cx, d.s.win, ids::demos_tf_watch(7)).as_deref(),
        Some("Private")
    );
    assert!(support::disabled_of(cx, d.s.win, ids::demos_tf_watch(7)));
}

#[gpui_kit::test]
fn a_row_fits_an_800_px_window(cx: &mut TestAppContext) {
    let (_env, _server, d) = served(cx, &[synthetic(7)]);
    cx.simulate_window_resize(d.s.win, size(px(800.), px(600.)));
    support::settle(cx, d.s.win);
    let list = support::bounds_of(cx, d.s.win, ids::DEMOS_TF_LIST).unwrap();
    for id in [
        ids::demos_tf_row_title(7),
        ids::demos_tf_row_map(7),
        ids::demos_tf_watch(7),
    ] {
        let part = support::bounds_of(cx, d.s.win, id.clone()).unwrap();
        assert!(inside(part, list), "{id:?} at {part:?} leaves {list:?}");
    }
}

#[gpui_kit::test]
fn a_running_download_says_so_and_takes_no_second_click(cx: &mut TestAppContext) {
    let (_env, server, d) = served(cx, &[synthetic(7)]);
    let gate = Gate::default();
    mount_held(&server, &gate, "/files/7.dem", None, file());
    support::click_id(cx, d.s.win, ids::demos_tf_watch(7));
    support::wait_real(cx, "the file request", |_| gate.arrived() == 1);
    support::settle(cx, d.s.win);
    assert_eq!(
        support::label_of(cx, d.s.win, ids::demos_tf_watch(7)).as_deref(),
        Some("Downloading demo")
    );
    assert!(support::disabled_of(cx, d.s.win, ids::demos_tf_watch(7)));
    gate.open();
    viewport(cx, &d);
    assert_eq!(gate.arrived(), 1);
}

#[gpui_kit::test]
fn a_running_download_keeps_watch_inside_its_column(cx: &mut TestAppContext) {
    let (_env, server, d) = served(cx, &[synthetic(7)]);
    cx.simulate_window_resize(d.s.win, size(px(1280.), px(800.)));
    let gate = Gate::default();
    mount_held(&server, &gate, "/files/7.dem", None, file());
    support::click_id(cx, d.s.win, ids::demos_tf_watch(7));
    support::wait_real(cx, "the file request", |_| gate.arrived() == 1);
    support::settle(cx, d.s.win);
    let watch = support::bounds_of(cx, d.s.win, ids::demos_tf_watch(7)).unwrap();
    let length = support::bounds_of(cx, d.s.win, ids::demos_tf_row_duration(7)).unwrap();
    assert!(watch.size.width <= px(96.), "Watch is {watch:?}");
    assert!(
        watch.left() >= length.right(),
        "Watch {watch:?} covers Length {length:?}"
    );
    gate.open();
    viewport(cx, &d);
}

#[gpui_kit::test]
fn a_failed_download_says_why_and_stays_on_the_list(cx: &mut TestAppContext) {
    let (_env, server, d) = served(cx, &[synthetic(7)]);
    server.mount_status("/files/7.dem", 404);
    support::click_id(cx, d.s.win, ids::demos_tf_watch(7));
    support::wait_real(cx, "the toast", |cx| {
        support::present(cx, d.s.win, ids::TOAST)
    });
    let said = support::label_of(cx, d.s.win, ids::TOAST).unwrap();
    assert!(said.starts_with("Couldn't download this demo."), "{said}");
    assert!(said.contains("demos.tf no longer has this demo."), "{said}");
    support::wait_real(cx, "the download to end", |cx| {
        cx.update(|cx| d.page.read(cx).download_progress(7).is_none())
    });
    assert!(support::present(cx, d.s.win, ids::DEMOS_TF_LIST));
    assert!(!support::present(cx, d.s.win, ids::DEMO_VIEWER));
}

#[gpui_kit::test]
fn a_download_landing_after_leaving_the_page_only_caches(cx: &mut TestAppContext) {
    let (env, server, d) = served(cx, &[synthetic(7)]);
    let gate = Gate::default();
    mount_held(&server, &gate, "/files/7.dem", None, file());
    support::click_id(cx, d.s.win, ids::demos_tf_watch(7));
    support::wait_real(cx, "the file request", |_| gate.arrived() == 1);
    support::click_nav(cx, d.s.win, Page::Settings);
    gate.open();
    support::wait_real(cx, "the download to end", |cx| {
        cx.update(|cx| d.page.read(cx).download_progress(7).is_none())
    });
    support::click_nav(cx, d.s.win, Page::DemosTf);
    assert!(support::present(cx, d.s.win, ids::DEMOS_TF_LIST));
    assert!(!support::present(cx, d.s.win, ids::DEMO_VIEWER));
    assert!(
        DemoCache::new(env.data.demo_cache_dir())
            .path_for(7)
            .exists()
    );
}

#[gpui_kit::test]
fn back_returns_to_the_same_place_in_the_list(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    install_map(&env, "cp_synthetic");
    let demos: Vec<Demo> = ids_down(1000, 50)
        .into_iter()
        .map(|d| Demo {
            map: "cp_synthetic",
            ..d
        })
        .collect();
    mount_first(&server, list_body(&server.uri(), &demos));
    mount_before(&server, 951, 200, b"[]".to_vec());
    let d = demos_tf_shell(cx, &env, &server.uri(), &[]);
    support::wait_real(cx, "fifty rows", |cx| shown_ids(cx, &d.page).len() == 50);
    let state = cx.update(|cx| d.page.read(cx).list_state().clone());
    state.scroll_to(ListOffset {
        item_ix: 10,
        offset_in_item: px(0.),
    });
    support::settle(cx, d.s.win);
    let before = state.logical_scroll_top();
    // Item 10 is row 9.
    let id = shown_ids(cx, &d.page)[before.item_ix - 1];
    server.mount_bytes(&format!("/files/{id}.dem"), file());
    support::click_id(cx, d.s.win, ids::demos_tf_watch(id));
    viewport(cx, &d);
    let asked = list_queries(&server).len();
    support::click_id(cx, d.s.win, ids::DEMOS_TF_CRUMB);
    support::settle(cx, d.s.win);
    assert!(support::present(cx, d.s.win, ids::DEMOS_TF_LIST));
    let after = state.logical_scroll_top();
    assert_eq!(
        (after.item_ix, after.offset_in_item),
        (before.item_ix, before.offset_in_item)
    );
    assert_eq!(shown_ids(cx, &d.page).len(), 50);
    assert_eq!(list_queries(&server).len(), asked, "Back reloads nothing");
}
