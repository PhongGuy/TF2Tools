//! The HUD Maker's default panel layout: docked beside the screen, slim rails in narrow windows.

mod support;

use gpui_kit::{Bounds, ElementId, Entity, Pixels, TestAppContext, px, size};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_app::pages::hud_maker::view::{Panel, PanelMode, Places};
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::Tf2Install;

// A rail is a slim strip at the area's edge, far narrower than either open panel.
const RAIL_MAX: f32 = 48.;

fn stock_shell(cx: &mut TestAppContext, env: &support::Env) -> support::ShellWindow {
    support::seed_stock_game(&env.game);
    let found = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    support::shell_window(cx, env, found)
}

fn open_maker(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<HudMakerPage> {
    support::click_nav(cx, s.win, Page::HudMaker);
    let page = cx
        .update(|cx| s.shell.read(cx).hud_maker_page())
        .expect("a HUD Maker page");
    support::wait_real(cx, "the HUD to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

fn resize(cx: &mut TestAppContext, s: &support::ShellWindow, w: f32, h: f32) {
    cx.simulate_window_resize(s.win, size(px(w), px(h)));
    support::settle(cx, s.win);
}

fn bounds(cx: &mut TestAppContext, s: &support::ShellWindow, id: &'static str) -> Bounds<Pixels> {
    support::bounds_of(cx, s.win, ElementId::from(id)).unwrap_or_else(|| panic!("no {id}"))
}

fn saved_places(s: &support::ShellWindow) -> Option<Places> {
    let settings = s.services.db.settings().unwrap();
    settings.hud_maker_panel().and_then(Places::from_setting)
}

// Neither panel, open or a rail, may lie over the screen.
fn assert_uncovered(cx: &mut TestAppContext, s: &support::ShellWindow, at: (f32, f32)) {
    let canvas = bounds(cx, s, ids::HUD_MAKER_CANVAS);
    for id in [ids::HUD_MAKER_PANEL, ids::HUD_MAKER_PARTS_PANEL] {
        let panel = bounds(cx, s, id);
        assert!(
            !panel.intersects(&canvas),
            "{at:?}: {id} {panel:?} covers the canvas {canvas:?}"
        );
    }
}

#[gpui_kit::test]
fn the_default_panels_dock_beside_the_canvas_in_roomy_windows(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    for (w, h) in [(1280., 800.), (1440., 900.)] {
        resize(cx, &s, w, h);
        assert_uncovered(cx, &s, (w, h));
        let shown = cx.update(|cx| page.read(cx).mode_shown(Panel::Options));
        assert_eq!(shown, PanelMode::DockLeft, "{w}x{h}");
        // The part settings stand open on the left; the part list waits as a rail on the right.
        assert!(support::present(cx, s.win, ids::HUD_MAKER_PANEL_BODY));
        let area = bounds(cx, &s, ids::HUD_MAKER_CANVAS_AREA);
        let parts = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL);
        assert!(parts.size.width <= px(RAIL_MAX), "{w}x{h}: {parts:?}");
        assert!((parts.right() - area.right()).abs() <= px(1.), "{parts:?}");
    }
}

#[gpui_kit::test]
fn the_open_part_list_docks_beside_the_canvas_too(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let _page = open_maker(cx, &s);
    resize(cx, &s, 1440., 900.);
    support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_COLLAPSE);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_PARTS));
    assert_uncovered(cx, &s, (1440., 900.));
}

#[gpui_kit::test]
fn narrow_windows_fold_the_panels_to_rails(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let _page = open_maker(cx, &s);
    resize(cx, &s, 800., 600.);
    assert_uncovered(cx, &s, (800., 600.));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_PANEL_BODY));
    for id in [ids::HUD_MAKER_PANEL, ids::HUD_MAKER_PARTS_PANEL] {
        let rail = bounds(cx, &s, id);
        assert!(rail.size.width <= px(RAIL_MAX), "{id} {rail:?}");
    }
    // Growing the window docks the side panel open again.
    resize(cx, &s, 1280., 800.);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_PANEL_BODY));
}

#[gpui_kit::test]
fn a_rail_opens_its_panel_on_demand_without_saving_it(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    resize(cx, &s, 800., 600.);
    let before = cx.update(|cx| page.read(cx).place_of(Panel::Options));
    support::click_id(cx, s.win, ids::HUD_MAKER_PANEL_COLLAPSE);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_PANEL_BODY));
    assert_eq!(
        cx.update(|cx| page.read(cx).place_of(Panel::Options)),
        before
    );
    support::click_id(cx, s.win, ids::HUD_MAKER_PANEL_COLLAPSE);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_PANEL_BODY));
    assert_uncovered(cx, &s, (800., 600.));
    assert_eq!(saved_places(&s), None);
}

#[gpui_kit::test]
fn a_collapsed_docked_panel_gives_its_column_to_the_canvas(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let _page = open_maker(cx, &s);
    resize(cx, &s, 1440., 900.);
    let open = bounds(cx, &s, ids::HUD_MAKER_CANVAS);
    support::click_id(cx, s.win, ids::HUD_MAKER_PANEL_COLLAPSE);
    let railed = bounds(cx, &s, ids::HUD_MAKER_CANVAS);
    assert!(
        railed.size.width > open.size.width,
        "{railed:?} vs {open:?}"
    );
    assert_uncovered(cx, &s, (1440., 900.));
    support::wait_real(cx, "the setting", |_| {
        saved_places(&s).is_some_and(|p| p.options.collapsed)
    });
}

#[gpui_kit::test]
fn the_players_float_choice_is_remembered(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let _page = open_maker(cx, &s);
    support::click_id(cx, s.win, ids::HUD_MAKER_PANEL_FLOAT);
    support::wait_real(cx, "the setting", |_| {
        saved_places(&s).is_some_and(|p| p.options.mode == PanelMode::Floating)
    });
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    let page = open_maker(cx, &s);
    let shown = cx.update(|cx| page.read(cx).mode_shown(Panel::Options));
    assert_eq!(shown, PanelMode::Floating);
}
