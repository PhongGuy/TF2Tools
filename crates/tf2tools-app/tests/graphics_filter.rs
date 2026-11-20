//! Finding a setting: search, changed only, the empty state and the category rail.

use crate::support;

use gpui_kit::{ElementId, Entity, TestAppContext, px, size};
use support::graphics::{found, idle, open, reveal, saved, search, wait_for_list};
use tf2tools_app::ids;
use tf2tools_app::pages::graphics::GraphicsPage;
use tf2tools_app::shell::route::Page;

fn full_page(
    cx: &mut TestAppContext,
    env: &support::Env,
    choices: &str,
) -> (support::ShellWindow, Entity<GraphicsPage>) {
    let s = support::shell_window(cx, env, found(env));
    support::seed_managed_mastercomfig(env, "9.100.1", "medium");
    support::seed_full_mastercomfig_catalog(env, "9.100.1");
    support::seed_mastercomfig_choices(env, choices);
    let page = open(cx, &s);
    wait_for_list(cx, &s, &page);
    (s, page)
}

fn at(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    id: impl Into<ElementId>,
) -> gpui_kit::Bounds<gpui_kit::Pixels> {
    let id = id.into();
    support::bounds_of(cx, s.win, id.clone()).unwrap_or_else(|| panic!("{id:?} not drawn"))
}

#[gpui_kit::test]
fn a_wide_page_starts_the_rail_beside_the_list_below_the_strip(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _page) = full_page(cx, &env, r#"{"fpscap":"120"}"#);
    cx.simulate_window_resize(s.win, size(px(1280.), px(900.)));
    support::settle(cx, s.win);
    let status = at(cx, &s, ids::GRAPHICS_STATUS);
    let strip = at(cx, &s, ids::GRAPHICS_CHANGES);
    let rail = at(cx, &s, ids::GRAPHICS_RAIL);
    let search = at(cx, &s, ids::GRAPHICS_SEARCH);
    assert!(
        status.left() <= rail.left() + px(1.),
        "the cards span the rail's column: {status:?} {rail:?}"
    );
    assert!(
        strip.right() >= search.right() - px(1.),
        "{strip:?} {search:?}"
    );
    assert!(
        rail.top() >= strip.bottom(),
        "the rail starts below the strip: {rail:?} {strip:?}"
    );
    assert!(
        (rail.top() - search.top()).abs() <= px(6.),
        "the rail lines up with the search: {rail:?} {search:?}"
    );
    assert!(rail.right() <= search.left(), "{rail:?} {search:?}");
}

#[gpui_kit::test]
fn the_rail_stays_in_view_while_the_list_scrolls(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _page) = full_page(cx, &env, "{}");
    cx.simulate_window_resize(s.win, size(px(1280.), px(900.)));
    support::settle(cx, s.win);
    reveal(cx, &s, ids::graphics_section("networking"));
    let view = at(cx, &s, ids::page(Page::Graphics));
    let rail = at(cx, &s, ids::GRAPHICS_RAIL);
    let status = at(cx, &s, ids::GRAPHICS_STATUS);
    assert!(
        status.bottom() < view.top(),
        "scrolled past the status card: {status:?}"
    );
    assert!(
        rail.top() >= view.top() - px(1.)
            && rail.top() <= view.top() + tf2tools_app::shell::CONTENT_PADDING + px(4.),
        "the rail pins at the top: {rail:?} {view:?}"
    );
    support::click_id(cx, s.win, ids::graphics_rail("graphics"));
    let section = at(cx, &s, ids::graphics_section("graphics"));
    let gap = section.top() - view.top();
    assert!(
        gap >= px(-2.) && gap <= tf2tools_app::shell::CONTENT_PADDING + px(4.),
        "{gap:?}"
    );
}

#[gpui_kit::test]
fn search_filters_rows_and_show_all_brings_them_back(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = full_page(cx, &env, "{}");
    search(cx, &s, &page, "shadows");
    assert!(support::present(cx, s.win, ids::graphics_row("shadows")));
    assert!(!support::present(cx, s.win, ids::graphics_row("lod")));
    assert!(!support::present(cx, s.win, ids::GRAPHICS_EMPTY));
    search(cx, &s, &page, "zzz");
    assert_eq!(
        support::label_of(cx, s.win, ids::GRAPHICS_EMPTY).as_deref(),
        Some("No setting matches \"zzz\".")
    );
    support::click_id(cx, s.win, ids::GRAPHICS_SHOW_ALL);
    assert!(support::present(cx, s.win, ids::graphics_row("lod")));
    let query = cx.update(|cx| page.read(cx).search_input().read(cx).value().to_string());
    assert_eq!(query, "");
}

#[gpui_kit::test]
fn changed_only_shows_the_changed_rows_then_says_nothing_differs(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = full_page(cx, &env, r#"{"fpscap":"120"}"#);
    support::click_id(cx, s.win, ids::GRAPHICS_CHANGED_ONLY);
    assert!(cx.update(|cx| page.read(cx).changed_only()));
    assert!(support::present(cx, s.win, ids::graphics_row("fpscap")));
    assert!(!support::present(cx, s.win, ids::graphics_row("lod")));
    support::click_id(cx, s.win, ids::graphics_change_undo("fpscap"));
    support::wait_real(cx, "the undo", |cx| saved(cx, &page, "fpscap").is_none());
    idle(cx, &page);
    assert_eq!(
        support::label_of(cx, s.win, ids::GRAPHICS_EMPTY).as_deref(),
        Some("Nothing differs from Medium.")
    );
    support::click_id(cx, s.win, ids::GRAPHICS_SHOW_ALL);
    assert!(!cx.update(|cx| page.read(cx).changed_only()));
    assert!(support::present(cx, s.win, ids::graphics_row("lod")));
}

#[gpui_kit::test]
fn a_rail_entry_scrolls_its_section_to_the_top(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _page) = full_page(cx, &env, "{}");
    cx.simulate_window_resize(s.win, size(px(1400.), px(900.)));
    support::settle(cx, s.win);
    let top = |cx: &mut TestAppContext| {
        let view = support::bounds_of(cx, s.win, ids::page(Page::Graphics)).expect("page");
        let hud = support::bounds_of(cx, s.win, ids::graphics_section("hud")).expect("hud");
        hud.top() - view.top()
    };
    assert!(top(cx) > px(900.), "HUD starts below the fold");
    let rail = support::bounds_of(cx, s.win, ids::graphics_rail("hud")).unwrap();
    let section = support::bounds_of(cx, s.win, ids::graphics_section("graphics")).unwrap();
    assert!(
        rail.right() <= section.left(),
        "a wide page keeps the rail beside the list"
    );
    support::click_id(cx, s.win, ids::graphics_rail("hud"));
    let gap = top(cx);
    assert!(
        gap >= px(-2.) && gap <= tf2tools_app::shell::CONTENT_PADDING + px(4.),
        "{gap:?}"
    );
}

#[gpui_kit::test]
fn a_narrow_page_puts_the_rail_above_the_settings(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _page) = full_page(cx, &env, "{}");
    cx.simulate_window_resize(s.win, size(px(1000.), px(900.)));
    support::settle(cx, s.win);
    let rail = support::bounds_of(cx, s.win, ids::graphics_rail("graphics")).unwrap();
    let section = support::bounds_of(cx, s.win, ids::graphics_section("graphics")).unwrap();
    assert!(rail.bottom() <= section.top(), "{rail:?} {section:?}");
    let strip = at(cx, &s, ids::GRAPHICS_NO_CHANGES);
    assert!(rail.top() >= strip.bottom(), "{rail:?} {strip:?}");
}

#[gpui_kit::test]
fn every_settings_list_control_has_a_stable_id(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = full_page(cx, &env, r#"{"fpscap":"120","lod":null}"#);
    let mut drawn: Vec<ElementId> = vec![
        ids::graphics_row("vsync"),
        ids::graphics_switch("vsync"),
        ids::graphics_segments("lod"),
        ids::graphics_segment("lod", "low"),
        ids::graphics_module("lod"),
        ids::graphics_module_description("lod"),
        ids::graphics_module_select("decals"),
        ids::graphics_module_preset("fpscap"),
        ids::graphics_module_reset("fpscap"),
        ids::graphics_category("hud"),
        ids::graphics_change("fpscap"),
        ids::graphics_change_undo("fpscap"),
        ids::graphics_rail_dot("graphics"),
        ids::GRAPHICS_CHANGES.into(),
        ids::GRAPHICS_RESET_ALL.into(),
        ids::GRAPHICS_SEARCH.into(),
        ids::GRAPHICS_CHANGED_ONLY.into(),
        ids::GRAPHICS_RAIL.into(),
        ids::GRAPHICS_SETTINGS.into(),
    ];
    for key in ["graphics", "hud", "sound", "misc", "networking"] {
        drawn.extend([
            ids::graphics_rail(key),
            ids::graphics_rail_count(key),
            ids::graphics_section(key),
        ]);
    }
    let missing: Vec<&ElementId> = drawn
        .iter()
        .filter(|id| !support::present(cx, s.win, (*id).clone()))
        .collect();
    assert!(missing.is_empty(), "not drawn: {missing:?}");
    assert!(!support::present(cx, s.win, ids::graphics_rail_dot("hud")));
    assert_eq!(
        support::label_of(cx, s.win, ids::graphics_rail_count("hud")).as_deref(),
        Some("14")
    );
    search(cx, &s, &page, "zzz");
    assert!(support::present(cx, s.win, ids::GRAPHICS_EMPTY));
    assert!(support::present(cx, s.win, ids::GRAPHICS_SHOW_ALL));
    // The always-drawn status detail line puts the empty state below the fold.
    reveal(cx, &s, ids::GRAPHICS_SHOW_ALL.into());
    support::click_id(cx, s.win, ids::GRAPHICS_SHOW_ALL);
    support::click_id(cx, s.win, ids::GRAPHICS_RESET_ALL);
    support::wait_real(cx, "the reset all", |cx| {
        saved(cx, &page, "fpscap").is_none()
    });
    idle(cx, &page);
    assert!(support::present(cx, s.win, ids::GRAPHICS_NO_CHANGES));
}
