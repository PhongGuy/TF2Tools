//! The changes strip: one chip per changed setting, each with an undo, and Reset all.

use crate::support;

use gpui_kit::TestAppContext;
use support::graphics::{
    HEADER, found, idle, modules_cfg, open, saved, server_shell, wait_for_list,
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
