//! The Graphics page: mastercomfig's status card and preset picker.

use crate::support;

use gpui_kit::assets::IconName;
use gpui_kit::{Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::graphics::GraphicsPage;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::nav_icon;
use tf2tools_app::shell::route::Page;
use tf2tools_core::Tf2Install;

fn found(env: &support::Env) -> Tf2State {
    Tf2State::Found(Tf2Install::from_path(&env.game).unwrap())
}

/// Opens Graphics from the sidebar and waits for its first load.
fn open_graphics(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<GraphicsPage> {
    support::click_nav(cx, s.win, Page::Graphics);
    let page = cx
        .update(|cx| s.shell.read(cx).graphics_page())
        .expect("a Graphics page");
    support::wait_real(cx, "Graphics to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

#[test]
fn graphics_follows_addons_in_the_tools_group_with_the_gauge_icon() {
    let at = Page::ALL.iter().position(|p| *p == Page::Graphics).unwrap();
    assert_eq!(Page::ALL.len(), 15);
    assert_eq!(Page::ALL[at - 1], Page::Addons);
    assert_eq!(Page::ALL[at + 1], Page::HudStore);
    let page = Page::Graphics;
    assert_eq!(
        (page.slug(), page.label(), page.title()),
        ("graphics", "Graphics", "Graphics")
    );
    assert_eq!(nav_icon(page), IconName::Gauge);
    assert!(!page.is_placeholder());
}

#[gpui_kit::test]
fn graphics_is_a_real_page_rebuilt_on_each_visit(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_graphics(cx, &s);
    assert!(support::present(cx, s.win, ids::page(Page::Graphics)));
    assert!(!support::present(cx, s.win, ids::PLACEHOLDER_MESSAGE));
    assert_eq!(
        support::label_of(cx, s.win, ids::GRAPHICS_SUBTITLE).as_deref(),
        Some("Graphics and game settings from mastercomfig")
    );
    assert!(!support::present(cx, s.win, ids::GRAPHICS_NO_TF2));
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    assert!(cx.update(|cx| s.shell.read(cx).graphics_page()).is_none());
}

#[gpui_kit::test]
fn without_tf2_the_page_asks_for_the_tf2_folder(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let _page = open_graphics(cx, &s);
    assert!(support::present(cx, s.win, ids::page(Page::Graphics)));
    assert!(support::present(cx, s.win, ids::GRAPHICS_NO_TF2));
}
