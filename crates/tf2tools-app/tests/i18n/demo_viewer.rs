//! The Demo Viewer's bar, with a map loaded, in the languages with its longest hint, on the
//! narrowest page (an 800x600 window) and the default 944 px one.

use gpui_kit::{TestAppContext, px, size};
use tf2tools_app::ids;
use tf2tools_app::shell::route::Page;

use crate::LocaleGuard;
use crate::support;
use support::demo_view_fixtures::demo_header;
use support::demo_viewer::{StubAnswer, demo_file, demo_viewer_shell, inside, install_map};
use support::graphics::found;

const ENGLISH_HINT: &str = "Drag to look, WASD to move, Shift to slow down.";

fn bar_fits(lang: &str, width: f32, height: f32, cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch(lang);
    let env = support::env();
    install_map(&env, "cp_synthetic");
    let demo = demo_file(&env, "match.dem", &demo_header("cp_synthetic", 60.0));
    let w = demo_viewer_shell(cx, &env, found(&env), vec![demo], StubAnswer::Frames);
    support::click_id(cx, w.s.win, ids::DEMOS_TF_OPEN);
    support::wait_real(cx, "the viewport", |cx| {
        support::present(cx, w.s.win, ids::DEMO_VIEWER_VIEWPORT)
    });
    cx.simulate_window_resize(w.s.win, size(px(width), px(height)));
    support::settle(cx, w.s.win);
    // The hint must be the translated one, or the widths below prove nothing about it.
    let hint = support::label_of(cx, w.s.win, ids::DEMO_VIEWER_HINT);
    assert!(
        hint.is_some_and(|h| h != ENGLISH_HINT),
        "{lang}: the hint is not translated"
    );
    let page = support::bounds_of(cx, w.s.win, ids::page(Page::DemosTf)).unwrap();
    let bar = support::bounds_of(cx, w.s.win, ids::DEMO_VIEWER_BAR).unwrap();
    assert!(inside(bar, page), "{lang} {width}: {bar:?} leaves {page:?}");
    for id in [
        ids::DEMO_VIEWER_HINT,
        ids::DEMO_VIEWER_PLAYERS_NOTE,
        ids::DEMO_VIEWER_OPEN,
        ids::DEMO_VIEWER_BAR_COLLAPSE,
    ] {
        let part = support::bounds_of(cx, w.s.win, id).unwrap();
        assert!(
            inside(part, bar) && inside(part, page),
            "{lang} {width}: {id} at {part:?} leaves the bar {bar:?} or the page {page:?}"
        );
    }
}

#[gpui_kit::test]
fn the_loaded_bar_fits_the_narrowest_page_in_german(cx: &mut TestAppContext) {
    bar_fits("de", 800., 600., cx);
}

#[gpui_kit::test]
fn the_loaded_bar_fits_the_narrowest_page_in_russian(cx: &mut TestAppContext) {
    bar_fits("ru", 800., 600., cx);
}

#[gpui_kit::test]
fn the_loaded_bar_fits_the_default_page_in_german(cx: &mut TestAppContext) {
    bar_fits("de", 1200., 900., cx);
}

#[gpui_kit::test]
fn the_loaded_bar_fits_the_default_page_in_russian(cx: &mut TestAppContext) {
    bar_fits("ru", 1200., 900., cx);
}
