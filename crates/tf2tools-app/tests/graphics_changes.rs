//! The changes strip: one chip per changed setting, each with an undo, and Reset all.

use crate::support;

use gpui_kit::{Bounds, Pixels, TestAppContext, px, size};
use support::graphics::{
    HEADER, found, idle, modules_cfg, open, pick, saved, server_shell, value, wait_for_list,
};
use tf2tools_app::ids;
use tf2tools_app::pages::graphics::model::SettingsNote;

#[gpui_kit::test]
fn the_strip_lists_each_change_and_undoes_them(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_mastercomfig_catalog(&env, "9.100.1");
    support::seed_mastercomfig_choices(&env, r#"{"fpscap":"120","hud_avatars":"off","lod":null}"#);
    let page = open(cx, &s);
    wait_for_list(cx, &s, &page);
    assert_eq!(
        support::label_of(cx, s.win, ids::GRAPHICS_CHANGES).as_deref(),
        Some("3 changes from Medium")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::graphics_change("lod")).as_deref(),
        Some("Model quality: Medium → Let TF2 decide")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::graphics_change("fpscap")).as_deref(),
        Some("Frame rate cap: 400 → 120")
    );
    assert!(!support::present(cx, s.win, ids::GRAPHICS_NO_CHANGES));
    support::click_id(cx, s.win, ids::graphics_change_undo("fpscap"));
    support::wait_real(cx, "the undo", |cx| saved(cx, &page, "fpscap").is_none());
    idle(cx, &page);
    assert_eq!(
        modules_cfg(&env).as_deref(),
        Some(format!("{HEADER}alias lod\nhud_avatars=off\n").as_str())
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::GRAPHICS_CHANGES).as_deref(),
        Some("2 changes from Medium")
    );
    assert!(!support::present(cx, s.win, ids::graphics_change("fpscap")));
    support::click_id(cx, s.win, ids::graphics_change_undo("hud_avatars"));
    support::wait_real(cx, "the second undo", |cx| {
        saved(cx, &page, "hud_avatars").is_none()
    });
    idle(cx, &page);
    assert_eq!(
        support::label_of(cx, s.win, ids::GRAPHICS_CHANGES).as_deref(),
        Some("1 change from Medium")
    );
    support::click_id(cx, s.win, ids::GRAPHICS_RESET_ALL);
    support::wait_real(cx, "the reset all", |cx| saved(cx, &page, "lod").is_none());
    idle(cx, &page);
    assert_eq!(modules_cfg(&env).as_deref(), Some(HEADER));
    assert_eq!(
        support::label_of(cx, s.win, ids::GRAPHICS_NO_CHANGES).as_deref(),
        Some("Using Medium as is. Change any setting below and it shows up here.")
    );
    assert!(!support::present(cx, s.win, ids::GRAPHICS_CHANGES));
    assert!(!support::present(cx, s.win, ids::GRAPHICS_RESET_ALL));
}

#[gpui_kit::test]
fn offline_the_strip_still_lists_saved_choices(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_status("/gh-raw/mastercomfig/9.100.1/data/modules.json", 500);
    let s = server_shell(cx, &env, &server);
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_mastercomfig_choices(&env, r#"{"fpscap":"120"}"#);
    let page = open(cx, &s);
    support::wait_real(cx, "the offline note", |cx| {
        cx.update(|cx| page.read(cx).settings_note()) == Some(SettingsNote::Offline)
    });
    support::settle(cx, s.win);
    assert_eq!(
        support::label_of(cx, s.win, ids::GRAPHICS_CHANGES).as_deref(),
        Some("1 change from Medium")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::graphics_change("fpscap")).as_deref(),
        Some("Frame rate cap: 120")
    );
    assert!(!support::present(
        cx,
        s.win,
        ids::graphics_change_undo("fpscap")
    ));
    assert!(!support::present(cx, s.win, ids::GRAPHICS_RESET_ALL));
}

#[gpui_kit::test]
fn the_strip_keeps_its_count_chips_and_reset_all_on_one_row(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_mastercomfig_catalog(&env, "9.100.1");
    support::seed_mastercomfig_choices(&env, r#"{"fpscap":"120"}"#);
    let page = open(cx, &s);
    wait_for_list(cx, &s, &page);
    for width in [1280., 800.] {
        cx.simulate_window_resize(s.win, size(px(width), px(900.)));
        support::settle(cx, s.win);
        let at = |cx: &mut TestAppContext, id: gpui_kit::ElementId| {
            support::bounds_of(cx, s.win, id).unwrap()
        };
        let strip = at(cx, ids::GRAPHICS_CHANGES.into());
        let chip = at(cx, ids::graphics_change("fpscap"));
        let reset = at(cx, ids::GRAPHICS_RESET_ALL.into());
        for b in [chip, reset] {
            assert!(
                b.left() >= strip.left()
                    && b.right() <= strip.right()
                    && b.bottom() <= strip.bottom(),
                "{width}: {b:?} spills out of {strip:?}"
            );
        }
        if width > 1000. {
            let mid = |b: Bounds<Pixels>| b.top() + b.size.height / 2.;
            assert!(
                (mid(chip) - mid(reset)).abs() <= px(4.),
                "one row: {chip:?} {reset:?}"
            );
            assert!(
                strip.right() - reset.right() <= px(20.),
                "Reset all closes the row: {reset:?}"
            );
            assert!(strip.size.height <= px(60.), "a single row: {strip:?}");
        }
    }
}

// A setting a newer mastercomfig dropped keeps its saved choice until an apply prunes it, so
// the strip must show it with a way to remove it.
#[gpui_kit::test]
fn a_saved_choice_for_a_dropped_setting_shows_and_can_be_removed(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_mastercomfig_catalog(&env, "9.100.1");
    support::seed_mastercomfig_choices(&env, r#"{"old_bloom":"on"}"#);
    let page = open(cx, &s);
    wait_for_list(cx, &s, &page);
    assert_eq!(
        support::label_of(cx, s.win, ids::GRAPHICS_CHANGES).as_deref(),
        Some("1 change from Medium")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::graphics_change("old_bloom")).as_deref(),
        Some("Old bloom: On (no longer in mastercomfig)")
    );
    assert!(support::present(cx, s.win, ids::GRAPHICS_RESET_ALL));
    support::click_id(cx, s.win, ids::graphics_change_undo("old_bloom"));
    support::wait_real(cx, "the removal", |cx| {
        saved(cx, &page, "old_bloom").is_none()
    });
    idle(cx, &page);
    assert_eq!(modules_cfg(&env).as_deref(), Some(HEADER));
    assert!(support::present(cx, s.win, ids::GRAPHICS_NO_CHANGES));
}

// The first change swaps the dashed "as is" strip for the chip strip; the rows below must not
// move, or the list shifts under the pointer that made the change.
#[gpui_kit::test]
fn the_first_change_keeps_the_rows_where_they_were(cx: &mut TestAppContext) {
    first_change_holds_still(cx, px(1280.), "fpscap", "120");
}

// Narrow, the title and one chip no longer fit one line, so the strip stacks them.
#[gpui_kit::test]
fn the_first_change_keeps_the_rows_still_at_the_narrowest_window(cx: &mut TestAppContext) {
    first_change_holds_still(cx, px(800.), "fpscap", "120");
}

// Just narrow, one line would still fit; the stacked strip keeps the same height there too.
#[gpui_kit::test]
fn the_first_change_keeps_the_rows_still_when_just_narrow(cx: &mut TestAppContext) {
    first_change_holds_still(cx, px(1100.), "fpscap", "120");
}

// Makes `module`'s first change at `width` and asserts the strip and the row keep their place.
fn first_change_holds_still(cx: &mut TestAppContext, width: Pixels, module: &str, to: &str) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_mastercomfig_catalog(&env, "9.100.1");
    support::seed_mastercomfig_choices(&env, "{}");
    let page = open(cx, &s);
    wait_for_list(cx, &s, &page);
    cx.simulate_window_resize(s.win, size(width, px(900.)));
    support::settle(cx, s.win);
    let empty = support::bounds_of(cx, s.win, ids::GRAPHICS_NO_CHANGES).unwrap();
    let row_before = support::bounds_of(cx, s.win, ids::graphics_row(module)).unwrap();
    pick(cx, &s, &page, module, value(to));
    support::wait_real(cx, "the first change", |cx| {
        saved(cx, &page, module).is_some()
    });
    idle(cx, &page);
    support::settle(cx, s.win);
    let strip = support::bounds_of(cx, s.win, ids::GRAPHICS_CHANGES).unwrap();
    let row_after = support::bounds_of(cx, s.win, ids::graphics_row(module)).unwrap();
    assert_eq!(
        empty.size.height, strip.size.height,
        "the strip keeps its height: {empty:?} {strip:?}"
    );
    assert_eq!(row_before.top(), row_after.top(), "the row stays put");
}
