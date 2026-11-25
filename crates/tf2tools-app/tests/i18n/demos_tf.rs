//! The Demos.tf toolbar and empty footer in German fit the narrowest window.
use gpui_kit::{TestAppContext, px, size};
use tf2tools_app::ids;
use tf2tools_app::shell::route::Page;

use crate::LocaleGuard;
use crate::support;
use support::demo_viewer::inside;
use support::demostf::{demos_tf_shell, mount_first, wait_end};

#[gpui_kit::test]
fn the_german_toolbar_fits_an_800_px_window(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("de");
    let (env, server) = (support::env(), support::StoreServer::start());
    mount_first(&server, b"[]".to_vec());
    let d = demos_tf_shell(cx, &env, &server.uri(), &[]);
    wait_end(cx, &d.page);
    cx.simulate_window_resize(d.s.win, size(px(800.), px(600.)));
    support::settle(cx, d.s.win);
    let empty = support::label_of(cx, d.s.win, ids::DEMOS_TF_EMPTY).unwrap();
    assert_ne!(empty, "No demos in this range", "not translated");
    let page = support::bounds_of(cx, d.s.win, ids::page(Page::DemosTf)).unwrap();
    for id in [
        ids::DEMOS_TF_OPEN,
        ids::DEMOS_TF_RANGE,
        ids::DEMOS_TF_FORMAT,
        ids::DEMOS_TF_WIDEN,
    ] {
        let part = support::bounds_of(cx, d.s.win, id).unwrap();
        assert!(inside(part, page), "{id} at {part:?} leaves {page:?}");
    }
}
