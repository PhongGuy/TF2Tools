//! Settings rows: a switch, a segmented control or a select, with the preset shown when changed.

use crate::support;

use gpui_kit::{Entity, TestAppContext};
use support::graphics::{
    HEADER, found, idle, modules_cfg, open, reveal, row_value, saved, value, wait_for_list,
};
use tf2tools_app::ids;
use tf2tools_app::pages::graphics::GraphicsPage;
use tf2tools_core::ModuleChoice;

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

fn checked(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    module: &str,
    v: &str,
) -> Option<bool> {
    support::checked_of(cx, s.win, ids::graphics_segment(module, v))
}

#[gpui_kit::test]
fn a_switch_row_flips_between_on_and_off(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = full_page(cx, &env, "{}");
    assert_eq!(
        support::checked_of(cx, s.win, ids::graphics_switch("vsync")),
        Some(false)
    );
    assert!(
        cx.update(|cx| page.read(cx).module_select("vsync"))
            .is_none()
    );
    reveal(cx, &s, ids::graphics_switch("vsync"));
    support::click_id(cx, s.win, ids::graphics_switch("vsync"));
    support::wait_real(cx, "vsync on", |cx| {
        saved(cx, &page, "vsync") == Some(value("on"))
    });
    idle(cx, &page);
    assert_eq!(
        modules_cfg(&env).as_deref(),
        Some(format!("{HEADER}vsync=on\n").as_str())
    );
    assert_eq!(
        support::checked_of(cx, s.win, ids::graphics_switch("vsync")),
        Some(true)
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::graphics_module_preset("vsync")).as_deref(),
        Some("Medium: Let TF2 decide")
    );
    // The first change grows the changes strip above the list, pushing the row down.
    reveal(cx, &s, ids::graphics_switch("vsync"));
    support::click_id(cx, s.win, ids::graphics_switch("vsync"));
    support::wait_real(cx, "vsync off", |cx| {
        saved(cx, &page, "vsync") == Some(value("off"))
    });
    idle(cx, &page);
    support::click_id(cx, s.win, ids::graphics_module_reset("vsync"));
    support::wait_real(cx, "the reset", |cx| saved(cx, &page, "vsync").is_none());
    idle(cx, &page);
    assert_eq!(modules_cfg(&env).as_deref(), Some(HEADER));
    assert!(!support::present(
        cx,
        s.win,
        ids::graphics_module_preset("vsync")
    ));
}

#[gpui_kit::test]
fn a_segment_picks_its_value_and_a_second_click_changes_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = full_page(cx, &env, "{}");
    assert!(cx.update(|cx| page.read(cx).module_select("lod")).is_none());
    assert_eq!(checked(cx, &s, "lod", "medium"), Some(true));
    reveal(cx, &s, ids::graphics_segment("lod", "high"));
    support::click_id(cx, s.win, ids::graphics_segment("lod", "high"));
    support::wait_real(cx, "lod high", |cx| {
        saved(cx, &page, "lod") == Some(value("high"))
    });
    idle(cx, &page);
    assert_eq!(
        modules_cfg(&env).as_deref(),
        Some(format!("{HEADER}lod=high\n").as_str())
    );
    assert_eq!(checked(cx, &s, "lod", "high"), Some(true));
    assert_eq!(checked(cx, &s, "lod", "medium"), Some(false));
    reveal(cx, &s, ids::graphics_segment("lod", "high"));
    support::click_id(cx, s.win, ids::graphics_segment("lod", "high"));
    idle(cx, &page);
    assert_eq!(
        saved(cx, &page, "lod"),
        Some(value("high")),
        "no segment means no NotSet"
    );
    assert_eq!(row_value(cx, &page, "lod"), Some(value("high")));
}

#[gpui_kit::test]
fn a_saved_let_tf2_decide_shows_no_segment_and_offers_reset(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = full_page(cx, &env, r#"{"lod":null}"#);
    for v in ["low", "medium", "high", "ultra"] {
        assert_eq!(checked(cx, &s, "lod", v), Some(false), "{v}");
    }
    assert_eq!(row_value(cx, &page, "lod"), Some(ModuleChoice::NotSet));
    assert_eq!(
        support::label_of(cx, s.win, ids::graphics_module_preset("lod")).as_deref(),
        Some("Medium: Medium")
    );
    assert!(support::present(
        cx,
        s.win,
        ids::graphics_module_reset("lod")
    ));
    assert_eq!(
        support::label_of(cx, s.win, ids::graphics_module("lod")).as_deref(),
        Some("Model quality")
    );
    assert!(
        cx.update(|cx| page.read(cx).module_select("decals"))
            .is_some()
    );
}

#[gpui_kit::test]
fn a_segment_is_picked_with_the_keyboard(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = full_page(cx, &env, "{}");
    support::tab_to(cx, s.win, ids::graphics_segment("lod", "ultra"), 64);
    support::press(cx, s.win, "enter");
    support::wait_real(cx, "the Enter pick", |cx| {
        saved(cx, &page, "lod") == Some(value("ultra"))
    });
    idle(cx, &page);
    support::tab_to(cx, s.win, ids::graphics_segment("phong", "off"), 64);
    support::press(cx, s.win, "space");
    support::wait_real(cx, "the Space pick", |cx| {
        saved(cx, &page, "phong") == Some(value("off"))
    });
}
