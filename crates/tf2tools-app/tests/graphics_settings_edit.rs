//! Changing the settings list: a pick, Game default, Reset, Reset all and the TF2 guard.

use crate::support;

use gpui_kit::{Entity, TestAppContext};
use support::graphics::{
    HEADER, choose, click_segment, found, idle, modules_cfg, open, pick, reveal, row_value, saved,
    scroll_to_top, segment_checked, shown, toasted, value, wait_for_list,
};
use tf2tools_app::ids;
use tf2tools_app::pages::graphics::GraphicsPage;
use tf2tools_app::pages::profiles::probe::Tf2Probe;
use tf2tools_core::ModuleChoice;

fn managed_page(
    cx: &mut TestAppContext,
    env: &support::Env,
    choices: &str,
) -> (support::ShellWindow, Entity<GraphicsPage>) {
    let s = support::shell_window(cx, env, found(env));
    support::seed_managed_mastercomfig(env, "9.100.1", "medium");
    support::seed_mastercomfig_catalog(env, "9.100.1");
    support::seed_mastercomfig_choices(env, choices);
    let page = open(cx, &s);
    wait_for_list(cx, &s, &page);
    (s, page)
}

#[gpui_kit::test]
fn a_pick_writes_modules_cfg_and_the_preset_s_value_clears_it(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = managed_page(cx, &env, "{}");
    let row = cx
        .update(|cx| page.read(cx).module_select("fpscap"))
        .unwrap()
        .entity_id();
    choose(cx, &page, "fpscap", Some(value("120")));
    support::wait_real(cx, "the pick", |cx| {
        saved(cx, &page, "fpscap") == Some(value("120"))
    });
    idle(cx, &page);
    assert_eq!(
        modules_cfg(&env).as_deref(),
        Some(format!("{HEADER}fpscap=120\n").as_str())
    );
    assert_eq!(shown(cx, &page, "fpscap"), Some(value("120")));
    assert!(support::present(
        cx,
        s.win,
        ids::graphics_module_reset("fpscap")
    ));
    let after = cx
        .update(|cx| page.read(cx).module_select("fpscap"))
        .unwrap()
        .entity_id();
    assert_eq!(after, row, "the row keeps its Select");

    choose(cx, &page, "fpscap", Some(value("400")));
    support::wait_real(cx, "the preset's value", |cx| {
        saved(cx, &page, "fpscap").is_none()
    });
    idle(cx, &page);
    assert_eq!(modules_cfg(&env).as_deref(), Some(HEADER));
    assert!(!support::present(
        cx,
        s.win,
        ids::graphics_module_reset("fpscap")
    ));

    choose(cx, &page, "fpscap", Some(ModuleChoice::NotSet));
    support::wait_real(cx, "Game default", |cx| {
        saved(cx, &page, "fpscap") == Some(ModuleChoice::NotSet)
    });
    idle(cx, &page);
    assert_eq!(
        modules_cfg(&env).as_deref(),
        Some(format!("{HEADER}alias fpscap\n").as_str())
    );
    assert_eq!(shown(cx, &page, "fpscap"), Some(ModuleChoice::NotSet));

    choose(cx, &page, "fpscap", None);
    idle(cx, &page);
    assert_eq!(
        saved(cx, &page, "fpscap"),
        Some(ModuleChoice::NotSet),
        "a cleared Select changes nothing"
    );
}

#[gpui_kit::test]
fn reset_and_reset_all_go_back_to_the_preset(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = managed_page(
        cx,
        &env,
        r#"{"fpscap":"120","hud_avatars":"off","lod":null}"#,
    );
    reveal(cx, &s, ids::graphics_module_reset("fpscap"));
    support::click_id(cx, s.win, ids::graphics_module_reset("fpscap"));
    support::wait_real(cx, "the reset", |cx| saved(cx, &page, "fpscap").is_none());
    idle(cx, &page);
    assert_eq!(
        modules_cfg(&env).as_deref(),
        Some(format!("{HEADER}alias lod\nhud_avatars=off\n").as_str())
    );
    assert_eq!(shown(cx, &page, "fpscap"), Some(value("400")));
    scroll_to_top(cx, &s);
    support::click_id(cx, s.win, ids::GRAPHICS_RESET_ALL);
    support::wait_real(cx, "the reset all", |cx| {
        cx.update(|cx| {
            page.read(cx)
                .choices()
                .is_some_and(|c| c.modules().is_empty())
        })
    });
    idle(cx, &page);
    assert_eq!(modules_cfg(&env).as_deref(), Some(HEADER));
    assert_eq!(row_value(cx, &page, "lod"), Some(value("medium")));
    assert!(!support::present(cx, s.win, ids::GRAPHICS_RESET_ALL));
}

#[gpui_kit::test]
fn low_sound_waits_for_tf2_to_close_but_other_settings_do_not(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = managed_page(cx, &env, "{}");
    cx.update(|cx| Tf2Probe::install(cx, || true));
    click_segment(cx, &s, "sound", "low");
    toasted(cx, &s, "Close TF2 first");
    idle(cx, &page);
    assert_eq!(saved(cx, &page, "sound"), None);
    assert_eq!(
        (
            segment_checked(cx, &s, "sound", "high"),
            segment_checked(cx, &s, "sound", "low")
        ),
        (Some(true), Some(false)),
        "the row still shows the saved choice"
    );
    assert_eq!(modules_cfg(&env), None);
    assert!(
        !env.game
            .join("tf/custom/mastercomfig-addon-override-1.vpk")
            .exists()
    );
    choose(cx, &page, "fpscap", Some(value("120")));
    support::wait_real(cx, "the pick", |cx| {
        saved(cx, &page, "fpscap") == Some(value("120"))
    });
    idle(cx, &page);
    assert_eq!(
        modules_cfg(&env).as_deref(),
        Some(format!("{HEADER}fpscap=120\n").as_str())
    );
}

#[gpui_kit::test]
fn a_refused_pick_goes_back_even_when_the_reload_fails(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = managed_page(cx, &env, "{}");
    cx.update(|cx| Tf2Probe::install(cx, || true));
    support::rename_table(&env, "mastercomfig_release", "gone_release");
    click_segment(cx, &s, "sound", "low");
    toasted(cx, &s, "Close TF2 first");
    toasted(cx, &s, "Failed to load mastercomfig");
    idle(cx, &page);
    assert_eq!(
        (
            segment_checked(cx, &s, "sound", "high"),
            segment_checked(cx, &s, "sound", "low")
        ),
        (Some(true), Some(false))
    );
}

// With six values the guarded sound row is a Select, which shows a pick before the page saves
// it; a refused pick must move it back once the page settles.
#[gpui_kit::test]
fn a_refused_select_pick_moves_the_select_back(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::graphics::seed_six_value_sound_catalog(&env, "9.100.1");
    let page = open(cx, &s);
    wait_for_list(cx, &s, &page);
    assert_eq!(
        shown(cx, &page, "sound"),
        Some(value("high")),
        "a Select row"
    );
    cx.update(|cx| Tf2Probe::install(cx, || true));
    pick(cx, &s, &page, "sound", value("low"));
    assert_eq!(
        shown(cx, &page, "sound"),
        Some(value("low")),
        "the click shows"
    );
    toasted(cx, &s, "Close TF2 first");
    idle(cx, &page);
    assert_eq!(saved(cx, &page, "sound"), None);
    assert_eq!(
        shown(cx, &page, "sound"),
        Some(value("high")),
        "the Select goes back to the saved choice"
    );
    assert_eq!(modules_cfg(&env), None);
    assert!(
        !env.game
            .join("tf/custom/mastercomfig-addon-override-1.vpk")
            .exists()
    );
}

#[gpui_kit::test]
fn a_segmented_row_shows_and_offers_let_tf2_decide(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = managed_page(cx, &env, r#"{"decals":null}"#);
    let values = ["off", "low", "medium", "high", "ultra"];
    assert_eq!(segment_checked(cx, &s, "decals", ""), Some(true));
    // The segment reads "Auto", but its tooltip and accessible name keep the full wording.
    assert_eq!(
        support::label_of(cx, s.win, ids::graphics_segment("decals", "")).as_deref(),
        Some("Let TF2 decide")
    );
    for v in values {
        assert_eq!(segment_checked(cx, &s, "decals", v), Some(false), "{v}");
    }

    click_segment(cx, &s, "decals", "high");
    support::wait_real(cx, "the pick", |cx| {
        saved(cx, &page, "decals") == Some(value("high"))
    });
    idle(cx, &page);
    assert_eq!(
        modules_cfg(&env).as_deref(),
        Some(format!("{HEADER}decals=high\n").as_str())
    );
    assert_eq!(segment_checked(cx, &s, "decals", ""), Some(false));

    click_segment(cx, &s, "decals", "");
    support::wait_real(cx, "Let TF2 decide", |cx| {
        saved(cx, &page, "decals") == Some(ModuleChoice::NotSet)
    });
    idle(cx, &page);
    assert_eq!(
        modules_cfg(&env).as_deref(),
        Some(format!("{HEADER}alias decals\n").as_str())
    );
    assert_eq!(row_value(cx, &page, "decals"), Some(ModuleChoice::NotSet));
    assert_eq!(segment_checked(cx, &s, "decals", ""), Some(true));
    assert_eq!(segment_checked(cx, &s, "decals", "high"), Some(false));
}
