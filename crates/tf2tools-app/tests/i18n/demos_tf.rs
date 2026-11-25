//! The Demos.tf toolbar and empty footer in German fit the narrowest window, and the kept
//! page's format Select, map placeholder and list error follow a language switch.
use gpui_kit::{TestAppContext, px, size};
use tf2tools_app::ids;
use tf2tools_app::shell::route::Page;

use crate::LocaleGuard;
use crate::library::shown_text;
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

// The page is kept across a switch, so its format Select's "any" row retitles in place.
#[gpui_kit::test]
fn the_kept_format_select_follows_a_language_switch(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
    let (env, server) = (support::env(), support::StoreServer::start());
    mount_first(&server, b"[]".to_vec());
    let d = demos_tf_shell(cx, &env, &server.uri(), &[]);
    wait_end(cx, &d.page);
    assert_eq!(
        shown_text(cx, d.s.win, ids::DEMOS_TF_FORMAT).as_deref(),
        Some("Any format")
    );
    tf2tools_app::i18n::apply("ru");
    cx.update(|cx| cx.refresh_windows());
    assert_eq!(
        shown_text(cx, d.s.win, ids::DEMOS_TF_FORMAT).as_deref(),
        Some("Любой формат")
    );
    assert_eq!(
        shown_text(cx, d.s.win, ids::DEMOS_TF_MAP).as_deref(),
        Some("Карта")
    );
}

// A failed list's footer is worded at draw time, so a switch retitles the error in place.
#[gpui_kit::test]
fn the_list_error_follows_a_language_switch(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
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
    tf2tools_app::i18n::apply("ru");
    cx.update(|cx| cx.refresh_windows());
    support::settle(cx, d.s.win);
    assert_eq!(
        support::label_of(cx, d.s.win, ids::DEMOS_TF_ERROR).as_deref(),
        Some("demos.tf сейчас не отвечает. Попробуйте позже.")
    );
}
