//! Apply with the Graphics and Add-ons slots: the module choices finished in the app, the
//! reasons a step is left as it is, and the movement keys line.

use crate::support;

use gpui_kit::AppContext as _;
use support::graphics::{HEADER, found, modules_cfg};
use support::profiles::{NULL, addon, low_fps60};
use tf2tools_app::ids;
use tf2tools_app::pages::profiles::probe::Tf2Probe;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{DraftItem, LogLevel, ProfileDraft, ProfileId, SlotMode};

fn graphics_only(s: &support::ShellWindow, name: &str) -> ProfileId {
    let draft = ProfileDraft::new(name)
        .with_hud(SlotMode::Leave)
        .with_sounds(SlotMode::Leave)
        .with_crosshairs(SlotMode::Leave)
        .with_graphics(SlotMode::Set(low_fps60()));
    s.services
        .library
        .create_profile(&draft)
        .unwrap()
        .id()
        .clone()
}

fn open(cx: &mut gpui_kit::TestAppContext, s: &support::ShellWindow) {
    cx.executor().allow_parking();
    cx.update(|cx| Tf2Probe::install(cx, || false));
    support::click_id(cx, s.win, ids::nav(Page::Profiles));
    let page = cx.update(|cx| s.shell.read(cx).profiles_page()).unwrap();
    support::wait_real(cx, "Profiles to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
}

fn toast(cx: &mut gpui_kit::TestAppContext, s: &support::ShellWindow) -> String {
    support::wait_real(cx, "the apply toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    support::label_of(cx, s.win, ids::TOAST).unwrap()
}

#[gpui_kit::test]
fn graphics_set_finishes_its_module_choices_then_marks_active(cx: &mut gpui_kit::TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_mastercomfig_catalog(&env, "9.100.1");
    let potato = graphics_only(&s, "Potato");
    open(cx, &s);
    support::click_apply(cx, s.win, &potato);
    assert_eq!(
        toast(cx, &s),
        "Potato applied. TF2 picks it up the next time it starts."
    );
    assert_eq!(
        modules_cfg(&env).as_deref(),
        Some(format!("{HEADER}fpscap=60\n").as_str())
    );
    assert_eq!(s.services.library.active_profile().unwrap(), Some(potato));
    let rows = support::log_rows(&s.services);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].message(), "Activated profile: Potato");
}

#[gpui_kit::test]
fn without_a_cached_catalog_graphics_fails_and_the_profile_stays_inactive(
    cx: &mut gpui_kit::TestAppContext,
) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    let potato = graphics_only(&s, "Potato");
    open(cx, &s);
    support::click_apply(cx, s.win, &potato);
    assert_eq!(
        toast(cx, &s),
        "Couldn't apply Potato. Graphics: Open the Graphics page once so TF2Tools can read your mastercomfig's settings."
    );
    assert_eq!(s.services.library.active_profile().unwrap(), None);
    let rows = support::log_rows(&s.services);
    assert_eq!(rows[0].level(), LogLevel::Error);
    assert!(
        rows[0].details().unwrap().contains("Failed: Graphics"),
        "{:?}",
        rows[0].details()
    );
}

#[gpui_kit::test]
fn a_foreign_modules_cfg_fails_graphics_with_its_reason(cx: &mut gpui_kit::TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_mastercomfig_catalog(&env, "9.100.1");
    let cfg = env.game.join("tf/cfg/overrides/modules.cfg");
    std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
    std::fs::write(&cfg, "lod=low\n").unwrap();
    let potato = graphics_only(&s, "Potato");
    open(cx, &s);
    support::click_apply(cx, s.win, &potato);
    assert_eq!(
        toast(cx, &s),
        "Couldn't apply Potato. Graphics: Your modules.cfg was not written by TF2Tools; open the Graphics page to replace it."
    );
    assert_eq!(std::fs::read_to_string(&cfg).unwrap(), "lod=low\n");
    assert_eq!(s.services.library.active_profile().unwrap(), None);
}

#[gpui_kit::test]
fn graphics_over_no_mastercomfig_is_left_as_it_is(cx: &mut gpui_kit::TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let potato = graphics_only(&s, "Potato");
    open(cx, &s);
    support::click_apply(cx, s.win, &potato);
    assert_eq!(
        toast(cx, &s),
        "Potato applied. TF2 picks it up the next time it starts. Graphics was left as it is: mastercomfig isn't installed, so install it on the Graphics page first."
    );
    assert_eq!(s.services.library.active_profile().unwrap(), None);
    assert!(!env.game.join("tf/custom/mastercomfig-base.vpk").exists());
}

#[gpui_kit::test]
fn a_skipped_item_keeps_the_profile_inactive_after_the_modules_phase(
    cx: &mut gpui_kit::TestAppContext,
) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_mastercomfig_catalog(&env, "9.100.1");
    let path = env.tmp.path().join("gone.zip");
    std::fs::write(&path, support::hud_zip_bytes("gone")).unwrap();
    let hud = s
        .services
        .library
        .import_hud(&path, tf2tools_core::Provenance::Local)
        .unwrap();
    let draft = ProfileDraft::new("Potato")
        .with_hud(SlotMode::Set(DraftItem::Pick(hud.id().clone())))
        .with_graphics(SlotMode::Set(low_fps60()));
    let potato = s
        .services
        .library
        .create_profile(&draft)
        .unwrap()
        .id()
        .clone();
    s.services.library.delete_hud(hud.id()).unwrap();
    open(cx, &s);
    support::click_apply(cx, s.win, &potato);
    support::wait_real(cx, "Missing Items", |cx| {
        support::present(cx, s.win, ids::PROFILES_MISSING_DIALOG)
    });
    support::click_id(cx, s.win, ids::PROFILES_MISSING_INSTALL);
    let text = toast(cx, &s);
    assert!(text.starts_with("Potato applied."), "{text}");
    assert!(text.contains("Kept what you had for: HUD"), "{text}");
    assert_eq!(
        modules_cfg(&env).as_deref(),
        Some(format!("{HEADER}fpscap=60\n").as_str())
    );
    assert_eq!(s.services.library.active_profile().unwrap(), None);
}

#[gpui_kit::test]
fn turning_null_movement_on_says_what_the_keys_do(cx: &mut gpui_kit::TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    let vpk = support::vpk_bytes(&[("cfg/addon.cfg", b"echo addon\n")]);
    support::seed_library_addon(&s.services, &env, NULL, "9.100.1", &vpk, false);
    let draft = ProfileDraft::new("Movement")
        .with_hud(SlotMode::Leave)
        .with_sounds(SlotMode::Leave)
        .with_crosshairs(SlotMode::Leave)
        .with_addons(SlotMode::Set((vec![DraftItem::Pick(addon(NULL))], false)));
    let id = s
        .services
        .library
        .create_profile(&draft)
        .unwrap()
        .id()
        .clone();
    open(cx, &s);
    support::click_apply(cx, s.win, &id);
    let text = toast(cx, &s);
    assert!(
        text.ends_with(
            "No movement keys were found in your TF2 settings, so it has nothing to use yet."
        ),
        "{text}"
    );
    assert!(env.game.join("tf/custom").join(NULL).is_file());
}

#[gpui_kit::test]
fn turning_null_movement_off_says_the_keys_are_back(cx: &mut gpui_kit::TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    let vpk = support::vpk_bytes(&[("cfg/addon.cfg", b"echo addon\n")]);
    support::seed_library_addon(&s.services, &env, NULL, "9.100.1", &vpk, true);
    let draft = ProfileDraft::new("Plain")
        .with_hud(SlotMode::Leave)
        .with_sounds(SlotMode::Leave)
        .with_crosshairs(SlotMode::Leave)
        .with_addons(SlotMode::None);
    let id = s
        .services
        .library
        .create_profile(&draft)
        .unwrap()
        .id()
        .clone();
    open(cx, &s);
    support::click_apply(cx, s.win, &id);
    let text = toast(cx, &s);
    assert_eq!(
        text,
        "Plain applied. TF2 picks it up the next time it starts. Your movement keys are back to normal."
    );
    assert!(!env.game.join("tf/custom").join(NULL).exists());
}

#[gpui_kit::test]
fn removing_mastercomfig_that_restores_binds_says_the_keys_are_back(
    cx: &mut gpui_kit::TestAppContext,
) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    let config = env.game.join("tf/cfg/config.cfg");
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(&config, "bind \"w\" \"+mf\"\r\nbind \"s\" \"+mb\"\r\n").unwrap();
    let draft = ProfileDraft::new("Stock")
        .with_hud(SlotMode::Leave)
        .with_sounds(SlotMode::Leave)
        .with_crosshairs(SlotMode::Leave)
        .with_graphics(SlotMode::None);
    let id = s
        .services
        .library
        .create_profile(&draft)
        .unwrap()
        .id()
        .clone();
    open(cx, &s);
    support::click_apply(cx, s.win, &id);
    assert_eq!(
        toast(cx, &s),
        "Stock applied. TF2 picks it up the next time it starts. Your movement keys are back to normal."
    );
    assert!(!env.game.join("tf/custom/mastercomfig-base.vpk").exists());
}

#[gpui_kit::test]
fn a_second_apply_during_the_modules_phase_does_nothing(cx: &mut gpui_kit::TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_mastercomfig_catalog(&env, "9.100.1");
    let potato = graphics_only(&s, "Potato");
    let other = graphics_only(&s, "Other");
    open(cx, &s);
    let page = cx.update(|cx| s.shell.read(cx).profiles_page()).unwrap();
    support::click_apply(cx, s.win, &potato);
    // Every tick until the toast, try to start Other; a gap in `activating` would open its summary.
    support::wait_real(cx, "the apply toast", |cx| {
        if support::present(cx, s.win, ids::TOAST) {
            return true;
        }
        cx.update_window(s.win, |_, window, cx| {
            page.update(cx, |p, cx| p.activate(other.clone(), window, cx))
        })
        .unwrap();
        false
    });
    support::wait_real(cx, "idle", |cx| {
        cx.update(|cx| page.read(cx).activating().is_none())
    });
    assert!(!support::present(cx, s.win, ids::PROFILE_APPLY_OK));
    assert_eq!(s.services.library.active_profile().unwrap(), Some(potato));
    assert_eq!(support::log_rows(&s.services).len(), 1);
}
