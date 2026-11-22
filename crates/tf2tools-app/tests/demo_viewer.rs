//! The Demo Viewer page: where it sits, its Preview badge, its empty state and its Open button.

use std::sync::Arc;

use gpui_kit::TestAppContext;
use gpui_kit::assets::IconName;
use tf2tools_app::ids;
use tf2tools_app::picker::{PickKind, ScriptedPicker};
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::nav_icon;
use tf2tools_app::shell::route::Page;

use crate::support;

#[test]
fn the_demo_viewer_follows_logs_tf_with_its_own_icon() {
    let at = Page::ALL
        .iter()
        .position(|p| *p == Page::DemoViewer)
        .unwrap();
    assert_eq!(Page::ALL[at - 1], Page::LogsTf);
    assert_eq!(Page::ALL[at + 1], Page::Settings);
    let page = Page::DemoViewer;
    assert_eq!(
        (page.slug(), page.label().as_ref(), page.title().as_ref()),
        ("demo-viewer", "Demo Viewer", "Demo Viewer")
    );
    assert_eq!(nav_icon(page), IconName::Clapperboard);
    assert!(!page.is_placeholder());
}

#[gpui_kit::test]
fn the_page_opens_empty_with_a_preview_badge_beside_its_title(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    support::click_nav(cx, s.win, Page::DemoViewer);
    assert_eq!(
        support::label_of(cx, s.win, ids::HEADER_TITLE).as_deref(),
        Some("Demo Viewer")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::DEMO_VIEWER_PREVIEW_BADGE).as_deref(),
        Some("Preview")
    );
    let title = support::bounds_of(cx, s.win, ids::HEADER_TITLE).unwrap();
    let badge = support::bounds_of(cx, s.win, ids::DEMO_VIEWER_PREVIEW_BADGE).unwrap();
    assert!(badge.origin.x > title.origin.x + title.size.width);
    assert_eq!(
        support::label_of(cx, s.win, ids::DEMO_VIEWER_STATUS).as_deref(),
        Some("Open a TF2 demo to see its map in 3D.")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::DEMO_VIEWER_OPEN).as_deref(),
        Some("Open demo")
    );
    assert!(!support::present(cx, s.win, ids::DEMO_VIEWER_VIEWPORT));
}

#[gpui_kit::test]
fn the_badge_shows_only_on_the_demo_viewer(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    assert!(!support::present(cx, s.win, ids::DEMO_VIEWER_PREVIEW_BADGE));
}

#[gpui_kit::test]
fn open_asks_for_a_demo_and_a_cancel_changes_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let picker = Arc::new(ScriptedPicker::empty());
    let s = support::shell_window_with(
        cx,
        &env,
        Tf2State::Missing { saved: None },
        Arc::clone(&picker),
        support::no_steam(),
    );
    support::click_nav(cx, s.win, Page::DemoViewer);
    support::click_id(cx, s.win, ids::DEMO_VIEWER_OPEN);
    assert_eq!(picker.asked(), [PickKind::OpenDemo]);
    assert_eq!(
        support::label_of(cx, s.win, ids::DEMO_VIEWER_STATUS).as_deref(),
        Some("Open a TF2 demo to see its map in 3D.")
    );
    assert!(!support::present(cx, s.win, ids::DEMO_VIEWER_VIEWPORT));
}
