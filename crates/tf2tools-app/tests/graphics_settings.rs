//! The Graphics page's settings list: one row per mastercomfig module, read from its catalog.

use crate::support;

use gpui_kit::TestAppContext;
use support::graphics::{found, open, server_shell, shown, value, wait_for_list};
use tf2tools_app::ids;
use tf2tools_app::pages::graphics::model::{SettingsNote, settings_note_text};
use tf2tools_core::{ModuleChoice, Preset};

fn note(cx: &mut TestAppContext, s: &support::ShellWindow) -> Option<String> {
    support::label_of(cx, s.win, ids::GRAPHICS_SETTINGS_NOTE)
}

#[gpui_kit::test]
fn a_saved_catalog_lists_every_module_with_the_preset_s_value(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_mastercomfig_catalog(&env, "9.100.1");
    let page = open(cx, &s);
    wait_for_list(cx, &s, &page);
    assert!(support::present(cx, s.win, ids::GRAPHICS_SETTINGS));
    assert_eq!(
        support::label_of(cx, s.win, ids::graphics_category("hud")).as_deref(),
        Some("HUD")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::graphics_module("motion_blur")).as_deref(),
        Some("Motion blur")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::graphics_module_description("lod")).as_deref(),
        Some("Controls how detailed models are.")
    );
    assert_eq!(shown(cx, &page, "lod"), Some(value("medium")));
    assert_eq!(shown(cx, &page, "fpscap"), Some(value("400")));
    assert_eq!(shown(cx, &page, "decals"), Some(value("low")));
    assert!(!support::present(
        cx,
        s.win,
        ids::graphics_module_reset("lod")
    ));
    assert!(support::disabled_of(cx, s.win, ids::GRAPHICS_RESET_ALL));
    assert!(!support::present(cx, s.win, ids::GRAPHICS_SETTINGS_NOTE));
    assert_eq!(
        support::label_of(cx, s.win, ids::GRAPHICS_FOOTER).as_deref(),
        Some("Changes apply the next time TF2 starts.")
    );
}

#[gpui_kit::test]
fn saved_choices_show_with_a_reset_on_their_rows(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_mastercomfig_catalog(&env, "9.100.1");
    support::seed_mastercomfig_choices(&env, r#"{"fpscap":"120","lod":null}"#);
    let page = open(cx, &s);
    wait_for_list(cx, &s, &page);
    assert_eq!(shown(cx, &page, "fpscap"), Some(value("120")));
    assert_eq!(shown(cx, &page, "lod"), Some(ModuleChoice::NotSet));
    assert!(support::present(
        cx,
        s.win,
        ids::graphics_module_reset("fpscap")
    ));
    assert!(support::present(
        cx,
        s.win,
        ids::graphics_module_reset("lod")
    ));
    assert!(!support::present(
        cx,
        s.win,
        ids::graphics_module_reset("decals")
    ));
    assert!(!support::disabled_of(cx, s.win, ids::GRAPHICS_RESET_ALL));
}

#[gpui_kit::test]
fn the_catalog_is_fetched_once_and_saved_for_next_time(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_mastercomfig_catalog("9.100.1");
    let s = server_shell(cx, &env, &server);
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    let page = open(cx, &s);
    wait_for_list(cx, &s, &page);
    assert_eq!(shown(cx, &page, "sound"), Some(value("high")));
    assert!(
        env.data
            .mastercomfig_dir()
            .join("9.100.1/modules.json")
            .is_file()
    );
}

#[gpui_kit::test]
fn offline_the_list_says_it_needs_the_internet_once(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    let page = open(cx, &s);
    let offline = settings_note_text(SettingsNote::Offline);
    support::wait_real(cx, "the offline note", |cx| {
        note(cx, &s).as_deref() == Some(offline)
    });
    assert!(cx.update(|cx| page.read(cx).catalog().is_none()));
    assert!(
        support::present(cx, s.win, ids::graphics_preset(Preset::High)),
        "presets still work"
    );
}

#[gpui_kit::test]
fn a_catalog_it_cannot_read_says_the_version_is_not_supported(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes(
        "/gh-raw/mastercomfig/9.100.1/data/modules.json",
        b"not json".to_vec(),
    );
    server.mount_bytes(
        "/gh-raw/mastercomfig/9.100.1/data/preset_modules.json",
        b"{}".to_vec(),
    );
    let s = server_shell(cx, &env, &server);
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    let _page = open(cx, &s);
    let unsupported = settings_note_text(SettingsNote::Unsupported);
    support::wait_real(cx, "the unsupported note", |cx| {
        note(cx, &s).as_deref() == Some(unsupported)
    });
    assert!(support::present(
        cx,
        s.win,
        ids::graphics_preset(Preset::Medium)
    ));
}

#[gpui_kit::test]
fn a_hand_install_has_no_settings_list(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    std::fs::write(
        env.game.join("tf/custom/mastercomfig-base.vpk"),
        support::mastercomfig_base("9.100.1"),
    )
    .unwrap();
    support::seed_mastercomfig_catalog(&env, "9.100.1");
    let _page = open(cx, &s);
    assert!(support::present(cx, s.win, ids::GRAPHICS_TAKE_OVER));
    assert!(!support::present(cx, s.win, ids::GRAPHICS_SETTINGS));
}
