use gpui_kit::{AppContext as _, TestAppContext, px};
use tf2tools_app::ids;
use tf2tools_app::services::Tf2State;
use tf2tools_core::Tf2Install;

use crate::support;

#[gpui_kit::test]
fn the_badge_hides_while_detecting_and_follows_the_state(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Detecting);
    assert!(!support::present(cx, s.win, ids::TF2_BADGE));
    cx.update(|cx| {
        s.tf2.update(cx, |t, cx| {
            *t = Tf2State::Missing { saved: None };
            cx.notify();
        })
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::TF2_BADGE).as_deref(),
        Some("TF2 Not Found")
    );
    let found = Tf2Install::from_path(&env.game).unwrap();
    cx.update(|cx| {
        s.tf2.update(cx, |t, cx| {
            *t = Tf2State::Found(found);
            cx.notify();
        })
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::TF2_BADGE).as_deref(),
        Some("TF2 Connected")
    );
}

#[gpui_kit::test]
fn the_badge_sits_right_of_the_title_in_the_header(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let header = support::bounds_of(cx, s.win, ids::HEADER).unwrap();
    let row = support::bounds_of(cx, s.win, ids::HEADER_ROW).unwrap();
    let badge = support::bounds_of(cx, s.win, ids::TF2_BADGE).unwrap();
    let title = support::bounds_of(cx, s.win, ids::HEADER_TITLE).unwrap();
    assert!(badge.origin.x > title.origin.x + title.size.width);
    // Measured against our own row, since Windows draws its window buttons right of it.
    assert!(row.origin.x + row.size.width <= header.origin.x + header.size.width);
    assert!(
        row.origin.x + row.size.width - (badge.origin.x + badge.size.width) <= gpui_kit::px(32.)
    );
    assert!(
        badge.origin.y >= header.origin.y
            && badge.origin.y + badge.size.height <= header.origin.y + header.size.height
    );
}

#[gpui_kit::test]
fn the_badge_is_as_tall_as_v5s(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let badge = support::bounds_of(cx, s.win, ids::TF2_BADGE).unwrap();
    // Measured on v5's HUDs page: 21 px, though shadcn's classes (border, py-0.5, text-xs) sum to 22.
    assert_eq!(badge.size.height, px(21.));
}

#[gpui_kit::test]
fn the_connected_badge_is_as_wide_as_v5s(cx: &mut TestAppContext) {
    let env = support::env();
    let found = Tf2Install::from_path(&env.game).unwrap();
    let s = support::shell_window(cx, &env, Tf2State::Found(found));
    let badge = support::bounds_of(cx, s.win, ids::TF2_BADGE).unwrap();
    // Measured on v5's HUDs page, row 28: the pill spans x 1065..=1175.
    assert!(
        (badge.size.width - px(111.)).abs() <= px(1.),
        "{:?}",
        badge.size.width
    );
    assert_eq!(badge.size.height, px(21.));
}

// GPUI's Inter shapes "TF2 Connected" 4 px narrower than WebKit, so the pill
// keeps v5's 111 px however narrow its text. A 12 px theme font (the window rem) shrinks the
// text and the rem padding; the shorter badge proves it took effect.
#[gpui_kit::test]
fn the_connected_badge_keeps_v5s_width_when_its_text_shapes_narrower(cx: &mut TestAppContext) {
    let env = support::env();
    let found = Tf2Install::from_path(&env.game).unwrap();
    let s = support::shell_window(cx, &env, Tf2State::Found(found));
    cx.update(|cx| gpui_kit::component::Theme::global_mut(cx).font_size = px(12.));
    cx.update_window(s.win, |_, w, _| w.refresh()).unwrap();
    let badge = support::bounds_of(cx, s.win, ids::TF2_BADGE).unwrap();
    assert!(
        badge.size.height < px(21.),
        "the rem did not apply: {badge:?}"
    );
    assert_eq!(badge.size.width, px(111.), "{badge:?}");
}
