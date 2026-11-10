//! The 56 px header is the kit's title bar, which brings the window controls, around v5's
//! title and TF2 badge; and the window options suit a client-drawn bar with the app's icon.

use std::sync::Arc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext, WindowDecorations, point, px};
use tf2tools_app::app::{window_icon, window_options};
use tf2tools_app::ids;
use tf2tools_app::services::Tf2State;
use tf2tools_core::Tf2Install;

use crate::support;

#[gpui_kit::test]
fn window_options_suit_a_client_drawn_title_bar(cx: &mut TestAppContext) {
    let opts = cx.update(|cx| window_options(cx));
    let expected = cfg!(target_os = "linux").then_some(WindowDecorations::Client);
    assert_eq!(opts.window_decorations, expected);
    assert!(opts.app_owns_titlebar_drag);
    let bar = opts.titlebar.expect("titlebar options");
    assert_eq!(bar.title, Some("TF2Tools".into()));
    assert!(bar.appears_transparent);
    assert!(opts.icon.is_some(), "the window carries the app icon");
}

#[test]
fn the_window_icon_is_the_packaged_256_px_png() {
    let icon = window_icon().expect("the packaged PNG decodes");
    assert_eq!(icon.dimensions(), (256, 256));
    assert!(Arc::ptr_eq(&icon, &window_icon().unwrap()), "decoded once");
}

#[gpui_kit::test]
fn one_title_bar_scope_holds_the_header_and_the_window_controls(cx: &mut TestAppContext) {
    let env = support::env();
    let found = Tf2Install::from_path(&env.game).unwrap();
    let s = support::shell_window(cx, &env, Tf2State::Found(found));
    cx.update_window(s.win, |_, w, cx| {
        w.render_frame(cx);
        // `within` panics unless exactly one `title-bar` scope exists, and it is the header's.
        // The kit's controls are not observable (and the test platform's server decorations
        // hide them); the kit bar always carries them, so finding the bar is the evidence.
        let mut header = w.within(ids::HEADER);
        let bar = header.within("title-bar");
        for id in [ids::HEADER_TITLE, ids::SIDEBAR_TOGGLE, ids::TF2_BADGE] {
            assert!(bar.try_find(id).is_some(), "{id} is in the title bar");
        }
        assert_eq!(bar.find(ids::HEADER_TITLE).label(), Some("HUDs"));
        assert_eq!(bar.find(ids::TF2_BADGE).label(), Some("TF2 Connected"));
    })
    .unwrap();
}

// A press on the sidebar toggle belongs to the button: dragging off it must not reach the bar's
// window move, which the test platform answers with a panic.
#[gpui_kit::test]
fn a_drag_started_on_the_sidebar_toggle_does_not_move_the_window(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let centre = support::bounds_of(cx, s.win, ids::SIDEBAR_TOGGLE)
        .unwrap()
        .center();
    let drag = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        cx.update_window(s.win, |_, w, cx| {
            w.drag(centre, point(centre.x + px(40.), centre.y), cx)
        })
        .unwrap();
    }));
    assert!(drag.is_ok(), "the drag reached the window move");
}

// The page title is no control, so a drag from it reaches the window move: the test platform's
// "not implemented" panic is the move starting.
#[gpui_kit::test]
fn a_drag_on_the_header_title_moves_the_window(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let centre = support::bounds_of(cx, s.win, ids::HEADER_TITLE)
        .unwrap()
        .center();
    let drag = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        cx.update_window(s.win, |_, w, cx| {
            w.drag(centre, point(centre.x + px(40.), centre.y), cx)
        })
        .unwrap();
    }));
    let message = drag
        .err()
        .and_then(|p| {
            p.downcast_ref::<String>()
                .cloned()
                .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
        })
        .unwrap_or_default();
    assert!(message.contains("not implemented"), "no move: {message:?}");
}

// At 800 px Linux draws three 34 px window buttons right of the header row; the test platform
// draws none, so a 698 px window leaves the row the same room. With every status item showing,
// the row must fit that room, so the buttons beyond it stay whole.
#[gpui_kit::test]
fn a_full_header_fits_beside_the_window_buttons_at_800_px(cx: &mut TestAppContext) {
    use tf2tools_app::shell::route::Page;
    use tf2tools_core::StoreKind;

    let env = support::env();
    let (hub, runs) = support::fake_hub(cx);
    let found = Tf2Install::from_path(&env.game).unwrap();
    let s = support::shell_window(cx, &env, Tf2State::Found(found));
    cx.update(|cx| {
        s.shell
            .update(cx, |shell, cx| shell.navigate(Page::CrosshairStore, cx))
    });
    let kinds = [
        StoreKind::Sounds,
        StoreKind::Crosshairs,
        StoreKind::GameBananaHuds,
        StoreKind::CuratedHuds,
    ];
    let mut held = Vec::new();
    for kind in kinds {
        assert!(cx.update(|cx| hub.update(cx, |h, cx| h.start(kind, false, cx))));
        let run = runs
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        run.report(3450, 3450);
        held.push(run);
    }
    let width = px(698.);
    cx.simulate_window_resize(s.win, gpui_kit::size(width, px(600.)));
    support::next_repaint(cx);
    for kind in kinds {
        assert!(
            support::present(cx, s.win, ids::sync_ring(kind)),
            "{kind:?}"
        );
    }
    let header = support::bounds_of(cx, s.win, ids::HEADER).unwrap();
    let row = support::bounds_of(cx, s.win, ids::HEADER_ROW).unwrap();
    let title = support::bounds_of(cx, s.win, ids::HEADER_TITLE).unwrap();
    let ring = support::bounds_of(cx, s.win, ids::sync_ring(StoreKind::Sounds)).unwrap();
    let badge = support::bounds_of(cx, s.win, ids::TF2_BADGE).unwrap();
    assert!(header.right() <= width, "{header:?}");
    assert!(row.right() <= header.right(), "{row:?} {header:?}");
    assert!(badge.right() <= row.right(), "{badge:?} {row:?}");
    assert!(title.right() <= ring.left(), "{title:?} {ring:?}");
}
