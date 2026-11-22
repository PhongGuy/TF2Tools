//! Settings rows: a switch, a segmented control or a select, with the preset shown when changed.

use crate::support;

use gpui_kit::{Entity, TestAppContext, px, size};
use support::graphics::{
    HEADER, found, idle, modules_cfg, open, reveal, row_value, saved, shown, value, wait_for_list,
};
use tf2tools_app::ids;
use tf2tools_app::pages::graphics::GraphicsPage;
use tf2tools_app::pages::graphics::view::{ControlKind, control_kind};
use tf2tools_core::{ModuleChoice, Preset};

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
    // The first change grows the changes strip above the list, pushing the row down, so each
    // later click reveals its row again.
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
        Some("Medium: Off")
    );
    // Medium sets vsync off, so flipping back to off clears the saved choice.
    reveal(cx, &s, ids::graphics_switch("vsync"));
    support::click_id(cx, s.win, ids::graphics_switch("vsync"));
    support::wait_real(cx, "vsync off", |cx| saved(cx, &page, "vsync").is_none());
    idle(cx, &page);
    assert_eq!(modules_cfg(&env).as_deref(), Some(HEADER));
    reveal(cx, &s, ids::graphics_switch("vsync"));
    support::click_id(cx, s.win, ids::graphics_switch("vsync"));
    support::wait_real(cx, "vsync on again", |cx| {
        saved(cx, &page, "vsync") == Some(value("on"))
    });
    idle(cx, &page);
    reveal(cx, &s, ids::graphics_module_reset("vsync"));
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
        cx.update(|cx| page.read(cx).module_select("fpscap"))
            .is_some()
    );
    assert!(
        cx.update(|cx| page.read(cx).module_select("decals"))
            .is_none()
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

// Under `preset`, every row with nothing saved shows what mastercomfig's real preset data sets:
// that segment or switch state, or that Select entry, and "Let TF2 decide" only when it sets nothing.
fn every_row_shows_the_preset_s_value(cx: &mut TestAppContext, preset: Preset) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", preset.name());
    support::seed_full_mastercomfig_catalog(&env, "9.100.1");
    let page = open(cx, &s);
    wait_for_list(cx, &s, &page);
    let catalog = support::full_module_catalog();
    for module in catalog.categories().iter().flat_map(|c| c.modules()) {
        let name = module.name();
        let resolved = catalog.resolve(name, preset);
        let expected = resolved.map_or(ModuleChoice::NotSet, value);
        assert_eq!(row_value(cx, &page, name), Some(expected.clone()), "{name}");
        match control_kind(module) {
            ControlKind::Switch => assert_eq!(
                support::checked_of(cx, s.win, ids::graphics_switch(name)),
                Some(resolved == Some("on")),
                "{name}"
            ),
            ControlKind::Segmented => {
                for v in module.values() {
                    assert_eq!(
                        checked(cx, &s, name, v.value()),
                        Some(resolved == Some(v.value())),
                        "{name}={}",
                        v.value()
                    );
                }
            }
            ControlKind::Select => {
                assert_eq!(shown(cx, &page, name), Some(expected), "{name}")
            }
        }
    }
}

#[test]
fn the_full_catalogue_resolves_settings_outside_the_trimmed_seven() {
    let catalog = support::full_module_catalog();
    for module in ["shading", "anti_aliasing", "lighting"] {
        assert!(catalog.resolve(module, Preset::High).is_some(), "{module}");
    }
}

#[gpui_kit::test]
fn every_row_shows_the_low_preset_s_value(cx: &mut TestAppContext) {
    every_row_shows_the_preset_s_value(cx, Preset::Low);
}

#[gpui_kit::test]
fn every_row_shows_the_medium_preset_s_value(cx: &mut TestAppContext) {
    every_row_shows_the_preset_s_value(cx, Preset::Medium);
}

#[gpui_kit::test]
fn every_row_shows_the_high_preset_s_value(cx: &mut TestAppContext) {
    every_row_shows_the_preset_s_value(cx, Preset::High);
}

#[gpui_kit::test]
fn every_row_shows_the_ultra_preset_s_value(cx: &mut TestAppContext) {
    every_row_shows_the_preset_s_value(cx, Preset::Ultra);
}

#[gpui_kit::test]
fn every_row_shows_the_custom_preset_s_value(cx: &mut TestAppContext) {
    every_row_shows_the_preset_s_value(cx, Preset::Custom);
}

#[gpui_kit::test]
fn every_control_fits_its_row_at_800_and_1280_px(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _page) = full_page(cx, &env, "{}");
    support::graphics::assert_controls_fit(cx, &s, 800.);
    support::graphics::assert_controls_fit(cx, &s, 1280.);
}

#[gpui_kit::test]
fn a_saved_choice_the_new_preset_matches_still_offers_reset(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = full_page(cx, &env, r#"{"lod":"high"}"#);
    support::click_id(cx, s.win, ids::graphics_preset(Preset::High));
    support::wait_real(cx, "the pick", |cx| {
        cx.update(|cx| page.read(cx).preset()) == Some(Preset::High)
    });
    idle(cx, &page);
    support::settle(cx, s.win);
    // High gives lod=high too, yet the saved choice still pins it across later switches.
    assert_eq!(saved(cx, &page, "lod"), Some(value("high")));
    assert_eq!(row_value(cx, &page, "lod"), Some(value("high")));
    assert!(support::present(
        cx,
        s.win,
        ids::graphics_module_reset("lod")
    ));
    assert_eq!(
        support::label_of(cx, s.win, ids::graphics_module_preset("lod")).as_deref(),
        Some("High: High")
    );
    // The strip names the kept value once, and its undo is a plain Reset like the row's.
    assert_eq!(
        support::label_of(cx, s.win, ids::graphics_change("lod")).as_deref(),
        Some("Model quality: High")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::graphics_change_undo("lod")).as_deref(),
        Some("Reset")
    );
}

/// Changed five-value rows, for the fit checks.
const CHANGED_FIVE: &str = r#"{"shadows":"high","texture_quality":"very_high","color_filter":"grayscale","water":"ultra"}"#;
const FIVE: [&str; 4] = ["shadows", "texture_quality", "color_filter", "water"];

#[gpui_kit::test]
fn a_changed_row_puts_its_preset_note_and_reset_inline_before_the_control(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _page) = full_page(cx, &env, r#"{"shadows":"high"}"#);
    cx.simulate_window_resize(s.win, size(px(1280.), px(900.)));
    support::settle(cx, s.win);
    reveal(cx, &s, ids::graphics_segments("shadows"));
    let b = |cx: &mut TestAppContext, id| support::bounds_of(cx, s.win, id).unwrap();
    let more = b(cx, ids::graphics_learn_more("shadows"));
    let note = b(cx, ids::graphics_module_preset("shadows"));
    let reset = b(cx, ids::graphics_module_reset("shadows"));
    let control = b(cx, ids::graphics_segments("shadows"));
    assert!(more.right() <= note.left(), "{more:?} {note:?}");
    assert!(
        note.right() <= reset.left()
            && reset.left() - note.right() <= px(12.)
            && reset.right() <= control.left(),
        "note, Reset, then the control, side by side: {note:?} {reset:?} {control:?}"
    );
    for (name, el) in [("note", note), ("Reset", reset)] {
        assert!(
            (el.center().y - control.center().y).abs() <= px(2.),
            "the {name} {el:?} sits level with the control {control:?}"
        );
    }
    assert!(
        note.size.height <= px(18.),
        "the note keeps to one line: {note:?}"
    );
    assert!(reset.size.height <= px(28.), "{reset:?}");
}

#[gpui_kit::test]
fn a_row_s_reset_is_a_ghost_button(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _page) = full_page(cx, &env, r#"{"lod":"high"}"#);
    reveal(cx, &s, ids::graphics_module_reset("lod"));
    assert_eq!(
        support::graphics::borders_at(cx, &s, ids::graphics_module_reset("lod")),
        [],
        "no outline round Reset"
    );
}

#[gpui_kit::test]
fn changed_five_value_rows_fit_at_800_px(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _page) = full_page(cx, &env, CHANGED_FIVE);
    support::graphics::assert_controls_fit(cx, &s, 800.);
    support::graphics::assert_changed_rows_fit(cx, &s, &FIVE, 800.);
}

#[gpui_kit::test]
fn changed_rows_keep_their_text_readable_on_a_wide_page(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _page) = full_page(cx, &env, support::graphics::CHANGED_WIDE);
    for width in [1252., 1280.] {
        support::graphics::assert_rows_readable(
            cx,
            &s,
            ids::IdScope::PAGE,
            &["shadows", "texture_quality"],
            width,
        );
    }
}

// Only the link's own words open the docs, not the blank space beside it.
#[gpui_kit::test]
fn learn_more_hugs_its_label(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _page) = full_page(cx, &env, r#"{"shadows":"high"}"#);
    cx.simulate_window_resize(s.win, size(px(1280.), px(900.)));
    support::settle(cx, s.win);
    let more = support::bounds_of(cx, s.win, ids::graphics_learn_more("shadows")).unwrap();
    assert!(
        (more.size.width - px(72.)).abs() <= px(8.),
        "Learn more is {more:?}, not about 72 px wide"
    );
}
