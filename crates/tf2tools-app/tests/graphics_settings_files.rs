//! The settings list's files: a player's own modules.cfg, the override VPK, and an update.

use crate::support;

use gpui_kit::TestAppContext;
use support::graphics::{
    HEADER, choose, found, idle, modules_cfg, open, saved, server_shell, toasted, value,
    wait_for_list,
};
use tf2tools_app::ids;

const REPLACE: &str = "TF2 already has a modules.cfg that TF2Tools didn't write. Your file is kept as a backup next to it.";

#[gpui_kit::test]
fn a_foreign_modules_cfg_is_replaced_only_after_asking(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_mastercomfig_catalog(&env, "9.100.1");
    let cfg = env.game.join("tf/cfg/overrides/modules.cfg");
    std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
    std::fs::write(&cfg, "fpscap=60\n").unwrap();
    let page = open(cx, &s);
    wait_for_list(cx, &s, &page);

    choose(cx, &page, "fpscap", Some(value("120")));
    support::wait_real(cx, "the question", |cx| {
        support::present(cx, s.win, ids::CONFIRM_DIALOG)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::CONFIRM_DESCRIPTION).as_deref(),
        Some(REPLACE)
    );
    assert_eq!(
        modules_cfg(&env).as_deref(),
        Some("fpscap=60\n"),
        "nothing changes before the answer"
    );
    support::click_id(cx, s.win, ids::CONFIRM_CANCEL);
    idle(cx, &page);
    assert_eq!(modules_cfg(&env).as_deref(), Some("fpscap=60\n"));
    assert_eq!(saved(cx, &page, "fpscap"), None);

    choose(cx, &page, "fpscap", Some(value("120")));
    support::wait_real(cx, "the question again", |cx| {
        support::present(cx, s.win, ids::CONFIRM_DIALOG)
    });
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    support::wait_real(cx, "the replace", |cx| {
        saved(cx, &page, "fpscap") == Some(value("120"))
    });
    idle(cx, &page);
    assert_eq!(
        modules_cfg(&env).as_deref(),
        Some(format!("{HEADER}fpscap=120\n").as_str())
    );
    assert_eq!(
        std::fs::read_to_string(cfg.with_file_name("modules.cfg.bak")).unwrap(),
        "fpscap=60\n"
    );
}

#[gpui_kit::test]
fn low_sound_downloads_its_override(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let vpk = support::vpk_bytes(&[("cfg/addons/override-1.cfg", b"echo".as_slice())]);
    server.mount_bytes(
        "/gh/mastercomfig/mastercomfig-addon-override-1.vpk",
        vpk.clone(),
    );
    let s = server_shell(cx, &env, &server);
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_mastercomfig_catalog(&env, "9.100.1");
    support::seed_mastercomfig_release(&env, &server.uri(), "9.100.1");
    support::seed_mastercomfig_overrides(&env, &server.uri());
    let page = open(cx, &s);
    wait_for_list(cx, &s, &page);
    choose(cx, &page, "sound", Some(value("low")));
    support::wait_real(cx, "the override", |cx| {
        saved(cx, &page, "sound") == Some(value("low"))
    });
    idle(cx, &page);
    let override_1 = env.game.join("tf/custom/mastercomfig-addon-override-1.vpk");
    assert_eq!(std::fs::read(override_1).unwrap(), vpk);
    assert_eq!(
        modules_cfg(&env).as_deref(),
        Some(format!("{HEADER}sound=low\n").as_str())
    );
}

#[gpui_kit::test]
fn an_update_drops_settings_the_new_version_lacks_and_names_them(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes(
        "/gh/mastercomfig/mastercomfig-base.vpk",
        support::mastercomfig_base("9.101.0"),
    );
    let s = server_shell(cx, &env, &server);
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_mastercomfig_release(&env, &server.uri(), "9.101.0");
    for tag in ["9.100.1", "9.101.0"] {
        support::seed_mastercomfig_catalog(&env, tag);
    }
    support::seed_mastercomfig_choices(&env, r#"{"fpscap":"120","gone":"on"}"#);
    let page = open(cx, &s);
    support::click_id(cx, s.win, ids::GRAPHICS_UPDATE);
    toasted(cx, &s, "went back to the preset: Gone.");
    idle(cx, &page);
    let tag = cx.update(|cx| page.read(cx).catalog().map(|c| c.tag().to_owned()));
    assert_eq!(tag.as_deref(), Some("9.101.0"));
    assert_eq!(saved(cx, &page, "gone"), None);
    assert_eq!(saved(cx, &page, "fpscap"), Some(value("120")));
    assert_eq!(
        modules_cfg(&env).as_deref(),
        Some(format!("{HEADER}fpscap=120\n").as_str())
    );
}

#[gpui_kit::test]
fn an_update_says_the_player_s_own_modules_cfg_was_kept(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes(
        "/gh/mastercomfig/mastercomfig-base.vpk",
        support::mastercomfig_base("9.101.0"),
    );
    let s = server_shell(cx, &env, &server);
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_mastercomfig_release(&env, &server.uri(), "9.101.0");
    for tag in ["9.100.1", "9.101.0"] {
        support::seed_mastercomfig_catalog(&env, tag);
    }
    support::seed_mastercomfig_choices(&env, r#"{"fpscap":"120"}"#);
    let cfg = env.game.join("tf/cfg/overrides/modules.cfg");
    std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
    std::fs::write(&cfg, "fpscap=60\n").unwrap();
    let page = open(cx, &s);
    support::click_id(cx, s.win, ids::GRAPHICS_UPDATE);
    toasted(
        cx,
        &s,
        "Now on 9.101.0. Your own modules.cfg was kept, so the settings below aren't in use. Change one to use them instead.",
    );
    idle(cx, &page);
    assert_eq!(modules_cfg(&env).as_deref(), Some("fpscap=60\n"));
    assert!(!support::present(cx, s.win, ids::CONFIRM_DIALOG));
}

// A fresh install with nothing saved; `own_cfg` seeds a player's own modules.cfg first.
fn fresh_install(cx: &mut TestAppContext, own_cfg: Option<&str>) -> support::Env {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes(
        "/gh/mastercomfig/mastercomfig-base.vpk",
        support::mastercomfig_base("9.100.1"),
    );
    let s = server_shell(cx, &env, &server);
    support::seed_mastercomfig_release(&env, &server.uri(), "9.100.1");
    support::seed_mastercomfig_catalog(&env, "9.100.1");
    if let Some(text) = own_cfg {
        let cfg = env.game.join("tf/cfg/overrides/modules.cfg");
        std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
        std::fs::write(&cfg, text).unwrap();
    }
    let page = open(cx, &s);
    support::click_id(cx, s.win, ids::GRAPHICS_INSTALL);
    toasted(
        cx,
        &s,
        "mastercomfig 9.100.1 is ready. Pick a preset below.",
    );
    idle(cx, &page);
    let labels = support::toast_labels(cx, s.win);
    assert!(
        !labels.iter().any(|t| t.contains("was kept")),
        "no kept note: {labels:?}"
    );
    env
}

#[gpui_kit::test]
fn a_fresh_install_leaves_the_player_s_own_modules_cfg_alone(cx: &mut TestAppContext) {
    let env = fresh_install(cx, Some("fpscap=60\n"));
    assert_eq!(modules_cfg(&env).as_deref(), Some("fpscap=60\n"));
}

#[gpui_kit::test]
fn a_fresh_install_writes_no_modules_cfg(cx: &mut TestAppContext) {
    let env = fresh_install(cx, None);
    assert_eq!(modules_cfg(&env), None);
}
