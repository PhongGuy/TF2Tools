//! The question before a HUD action removes HUDs the library cannot put back.

mod support;

use std::path::PathBuf;

use gpui_kit::{Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::live::live_failure;
use tf2tools_app::pages::huds::HudsPage;
use tf2tools_app::pages::profiles::ProfilesPage;
use tf2tools_app::pages::profiles::probe::Tf2Probe;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::external_huds::{HudAction, dialog_text, names_text};
use tf2tools_app::shell::route::Page;
use tf2tools_core::{
    DraftSlot, ExternalHud, HudEntry, LibraryKind, Profile, ProfileDraft, Provenance,
    ReinstallError, Tf2Install,
};

fn tf2(env: &support::Env) -> Tf2Install {
    Tf2Install::from_path(&env.game).unwrap()
}

fn found(env: &support::Env) -> Tf2State {
    Tf2State::Found(tf2(env))
}

fn custom(env: &support::Env) -> PathBuf {
    env.game.join("tf/custom")
}

fn seed(s: &support::ShellWindow, env: &support::Env, name: &str) -> HudEntry {
    let path = env.tmp.path().join(format!("{name}.zip"));
    std::fs::write(&path, support::hud_zip_bytes(name)).unwrap();
    s.services
        .library
        .import_hud(&path, Provenance::Local)
        .unwrap()
}

fn place(env: &support::Env, folder: &str) {
    let dir = custom(env).join(folder);
    std::fs::create_dir_all(dir.join("resource")).unwrap();
    std::fs::write(dir.join("info.vdf"), format!("\"{folder}\"\n{{\n}}\n")).unwrap();
    std::fs::write(dir.join("resource/mine.res"), "hand made").unwrap();
}

fn intact(env: &support::Env, folder: &str) -> bool {
    std::fs::read_to_string(custom(env).join(folder).join("resource/mine.res"))
        .is_ok_and(|text| text == "hand made")
}

fn in_library(s: &support::ShellWindow, folder: &str) -> bool {
    let huds = s.services.library.list_huds().unwrap();
    huds.iter().any(|h| h.folder_name() == folder)
}

fn open_huds(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<HudsPage> {
    support::click_id(cx, s.win, ids::nav(Page::Settings));
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    let page = cx
        .update(|cx| s.shell.read(cx).huds_page())
        .expect("a HUDs page");
    support::wait_real(cx, "HUDs to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded() && page.read(cx).is_idle())
    });
    support::settle(cx, s.win);
    page
}

fn open_profiles(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<ProfilesPage> {
    cx.update(|cx| Tf2Probe::install(cx, || false));
    support::click_id(cx, s.win, ids::nav(Page::Profiles));
    let page = cx
        .update(|cx| s.shell.read(cx).profiles_page())
        .expect("a Profiles page");
    support::wait_real(cx, "Profiles to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

fn wait_dialog(cx: &mut TestAppContext, s: &support::ShellWindow) {
    support::wait_real(cx, "the question", |cx| {
        support::present(cx, s.win, ids::EXTERNAL_HUDS_DIALOG)
    });
}

fn wait_toast(cx: &mut TestAppContext, s: &support::ShellWindow) -> String {
    support::wait_real(cx, "the toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    support::label_of(cx, s.win, ids::TOAST).unwrap_or_default()
}

/// Waits until the page has dropped the action; a cancelled action leaves no toast to wait on.
fn wait_install_button(cx: &mut TestAppContext, s: &support::ShellWindow, folder: &str) {
    support::wait_real(cx, "the install button", |cx| {
        !support::disabled_of(cx, s.win, ids::hud_install(folder))
    });
}

#[gpui_kit::test]
fn install_asks_and_names_the_hud(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    seed(&s, &env, "budhud");
    place(&env, "manual");
    let _page = open_huds(cx, &s);
    support::click_id(cx, s.win, ids::hud_install("budhud"));
    wait_dialog(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::EXTERNAL_HUDS_TITLE).as_deref(),
        Some("Keep manual?")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::EXTERNAL_HUDS_DESCRIPTION).as_deref(),
        Some(
            "Installing this HUD removes manual from TF2. TF2Tools has no copy of it, so it \
             can't come back unless you keep one."
        )
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::EXTERNAL_HUDS_KEEP).as_deref(),
        Some("Keep a copy in my library")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::EXTERNAL_HUDS_DISCARD).as_deref(),
        Some("Replace without keeping")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::EXTERNAL_HUDS_CANCEL).as_deref(),
        Some("Cancel")
    );
    assert!(intact(&env, "manual"), "asking changes nothing");
}

#[gpui_kit::test]
fn the_buttons_fit_inside_the_dialog(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    seed(&s, &env, "budhud");
    place(&env, "manual");
    let _page = open_huds(cx, &s);
    support::click_id(cx, s.win, ids::hud_install("budhud"));
    wait_dialog(cx, &s);
    support::settle(cx, s.win);
    let dialog = support::bounds_of(cx, s.win, ids::EXTERNAL_HUDS_DIALOG).unwrap();
    for id in [
        ids::EXTERNAL_HUDS_KEEP,
        ids::EXTERNAL_HUDS_DISCARD,
        ids::EXTERNAL_HUDS_CANCEL,
    ] {
        let button = support::bounds_of(cx, s.win, id).unwrap();
        assert!(button.left() >= dialog.left(), "{id} {button:?} {dialog:?}");
        assert!(
            button.right() <= dialog.right(),
            "{id} {button:?} {dialog:?}"
        );
        assert!(
            button.bottom() <= dialog.bottom(),
            "{id} {button:?} {dialog:?}"
        );
    }
}

#[gpui_kit::test]
fn cancel_changes_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    seed(&s, &env, "budhud");
    place(&env, "manual");
    let _page = open_huds(cx, &s);
    support::click_id(cx, s.win, ids::hud_install("budhud"));
    wait_dialog(cx, &s);
    support::click_id(cx, s.win, ids::EXTERNAL_HUDS_CANCEL);
    wait_install_button(cx, &s, "budhud");
    assert!(!support::present(cx, s.win, ids::EXTERNAL_HUDS_DIALOG));
    assert!(!support::present(cx, s.win, ids::TOAST));
    assert!(intact(&env, "manual"));
    assert!(!custom(&env).join("budhud").exists());
    assert!(!in_library(&s, "manual"));
}

#[gpui_kit::test]
fn escape_cancels_too(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    seed(&s, &env, "budhud");
    place(&env, "manual");
    let _page = open_huds(cx, &s);
    support::click_id(cx, s.win, ids::hud_install("budhud"));
    wait_dialog(cx, &s);
    support::press(cx, s.win, "escape");
    wait_install_button(cx, &s, "budhud");
    assert!(intact(&env, "manual"));
    assert!(!custom(&env).join("budhud").exists());
}

#[gpui_kit::test]
fn enter_takes_the_choice_that_loses_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    seed(&s, &env, "budhud");
    place(&env, "manual");
    let _page = open_huds(cx, &s);
    support::click_id(cx, s.win, ids::hud_install("budhud"));
    wait_dialog(cx, &s);
    support::press(cx, s.win, "enter");
    wait_toast(cx, &s);
    assert!(in_library(&s, "manual"));
    assert!(custom(&env).join("budhud/info.vdf").is_file());
}

#[gpui_kit::test]
fn an_edited_install_asks_about_the_changes(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let installed = seed(&s, &env, "rayshud");
    let library = s.services.library.clone();
    library.install_hud(installed.id(), &tf2(&env)).unwrap();
    seed(&s, &env, "budhud");
    std::fs::write(custom(&env).join("rayshud/info.vdf"), "tweaked").unwrap();
    let _page = open_huds(cx, &s);
    support::click_id(cx, s.win, ids::hud_install("budhud"));
    wait_dialog(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::EXTERNAL_HUDS_TITLE).as_deref(),
        Some("Keep your changes to rayshud?")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::EXTERNAL_HUDS_DESCRIPTION).as_deref(),
        Some(
            "Installing this HUD removes rayshud from TF2. Your changes can't be brought back \
             unless you keep a copy."
        )
    );
    support::click_id(cx, s.win, ids::EXTERNAL_HUDS_KEEP);
    wait_toast(cx, &s);
    assert!(in_library(&s, "rayshud-2"));
    assert!(custom(&env).join("budhud/info.vdf").is_file());
}

#[gpui_kit::test]
fn keeping_a_copy_adds_it_then_installs(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    seed(&s, &env, "budhud");
    place(&env, "manual");
    let _page = open_huds(cx, &s);
    support::click_id(cx, s.win, ids::hud_install("budhud"));
    wait_dialog(cx, &s);
    support::click_id(cx, s.win, ids::EXTERNAL_HUDS_KEEP);
    assert_eq!(
        wait_toast(cx, &s),
        "HUD installed. budhud is now active. A copy of manual is in your library."
    );
    assert!(in_library(&s, "manual"));
    let library = env.data.library_dir(LibraryKind::Huds);
    assert!(library.join("manual/resource/mine.res").is_file());
    assert!(!custom(&env).join("manual").exists());
    assert!(custom(&env).join("budhud/info.vdf").is_file());
}

#[gpui_kit::test]
fn replacing_without_keeping_says_so(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    seed(&s, &env, "budhud");
    place(&env, "manual");
    let _page = open_huds(cx, &s);
    support::click_id(cx, s.win, ids::hud_install("budhud"));
    wait_dialog(cx, &s);
    support::click_id(cx, s.win, ids::EXTERNAL_HUDS_DISCARD);
    assert_eq!(
        wait_toast(cx, &s),
        "HUD installed. budhud is now active. Removed without a copy: manual."
    );
    assert!(!in_library(&s, "manual"));
    assert!(!custom(&env).join("manual").exists());
    assert!(custom(&env).join("budhud/info.vdf").is_file());
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_failed_copy_removes_nothing(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt;
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let previous = seed(&s, &env, "rayshud");
    s.services
        .library
        .install_hud(previous.id(), &tf2(&env))
        .unwrap();
    seed(&s, &env, "budhud");
    place(&env, "manual");
    let _page = open_huds(cx, &s);
    support::click_id(cx, s.win, ids::hud_install("budhud"));
    wait_dialog(cx, &s);
    let library = env.data.library_dir(LibraryKind::Huds);
    std::fs::set_permissions(&library, PermissionsExt::from_mode(0o555)).unwrap();
    support::click_id(cx, s.win, ids::EXTERNAL_HUDS_KEEP);
    let toast = wait_toast(cx, &s);
    std::fs::set_permissions(&library, PermissionsExt::from_mode(0o755)).unwrap();
    assert!(toast.contains("nothing was removed"), "{toast}");
    assert!(intact(&env, "manual"));
    assert!(custom(&env).join("rayshud/info.vdf").is_file());
    assert!(!custom(&env).join("budhud").exists());
}

#[gpui_kit::test]
fn uninstall_asks_with_its_own_words(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    place(&env, "manual");
    let _page = open_huds(cx, &s);
    support::click_id(cx, s.win, ids::HUDS_UNINSTALL);
    wait_dialog(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::EXTERNAL_HUDS_DISCARD).as_deref(),
        Some("Remove without keeping")
    );
    support::click_id(cx, s.win, ids::EXTERNAL_HUDS_CANCEL);
    support::wait_real(cx, "the dialog to go", |cx| {
        !support::present(cx, s.win, ids::EXTERNAL_HUDS_DIALOG)
    });
    support::settle(cx, s.win);
    assert!(intact(&env, "manual"));

    support::click_id(cx, s.win, ids::HUDS_UNINSTALL);
    wait_dialog(cx, &s);
    support::click_id(cx, s.win, ids::EXTERNAL_HUDS_DISCARD);
    let toast = wait_toast(cx, &s);
    assert!(toast.starts_with("HUD uninstalled."), "{toast}");
    assert!(!custom(&env).join("manual").exists());
}

#[gpui_kit::test]
fn library_huds_are_never_asked_about(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let installed = seed(&s, &env, "rayshud");
    s.services
        .library
        .install_hud(installed.id(), &tf2(&env))
        .unwrap();
    seed(&s, &env, "budhud");
    let _page = open_huds(cx, &s);
    support::click_id(cx, s.win, ids::hud_install("budhud"));
    assert_eq!(wait_toast(cx, &s), "HUD installed. budhud is now active");
    assert!(!custom(&env).join("rayshud").exists());
}

fn hud_profile(s: &support::ShellWindow, hud: &HudEntry) -> Profile {
    let draft = ProfileDraft::new("Comp").with_hud(DraftSlot::Item(hud.id().clone()));
    s.services.library.create_profile(&draft).unwrap()
}

#[gpui_kit::test]
fn a_profile_asks_and_cancel_changes_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let hud = seed(&s, &env, "budhud");
    let comp = hud_profile(&s, &hud);
    place(&env, "manual");
    let page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::profile_activate(comp.id()));
    wait_dialog(cx, &s);
    assert!(
        support::label_of(cx, s.win, ids::EXTERNAL_HUDS_DESCRIPTION)
            .is_some_and(|text| text.starts_with("Activating this profile removes manual"))
    );
    support::click_id(cx, s.win, ids::EXTERNAL_HUDS_CANCEL);
    support::wait_real(cx, "the flow to end", |cx| {
        cx.update(|cx| page.read(cx).activating().is_none())
    });
    assert!(intact(&env, "manual"));
    assert!(!custom(&env).join("budhud").exists());
    assert_eq!(s.services.library.active_profile().unwrap(), None);
    assert!(support::log_rows(&s.services).is_empty());
}

#[gpui_kit::test]
fn a_profile_keeps_a_copy_then_applies(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let hud = seed(&s, &env, "budhud");
    let comp = hud_profile(&s, &hud);
    place(&env, "manual");
    let _page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::profile_activate(comp.id()));
    wait_dialog(cx, &s);
    support::click_id(cx, s.win, ids::EXTERNAL_HUDS_KEEP);
    let toast = wait_toast(cx, &s);
    assert!(
        toast.ends_with("A copy of manual is in your library."),
        "{toast}"
    );
    assert!(in_library(&s, "manual"));
    assert!(custom(&env).join("budhud/info.vdf").is_file());
    assert_eq!(
        s.services.library.active_profile().unwrap(),
        Some(comp.id().clone())
    );
}

#[gpui_kit::test]
fn install_anyway_still_asks_before_removing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let gone = seed(&s, &env, "budhud");
    let comp = hud_profile(&s, &gone);
    s.services.library.delete_hud(gone.id()).unwrap();
    place(&env, "manual");
    let _page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::profile_activate(comp.id()));
    support::wait_real(cx, "the Missing Items dialog", |cx| {
        support::present(cx, s.win, ids::PROFILES_MISSING_INSTALL)
    });
    support::click_id(cx, s.win, ids::PROFILES_MISSING_INSTALL);
    wait_dialog(cx, &s);
    assert!(intact(&env, "manual"));
    support::click_id(cx, s.win, ids::EXTERNAL_HUDS_DISCARD);
    wait_toast(cx, &s);
    assert!(!custom(&env).join("manual").exists());
}

/// Real `ExternalHud`s can only come from a scan, so the words are checked on a scratch tree.
fn scanned(s: &support::ShellWindow, env: &support::Env, folders: &[String]) -> Vec<ExternalHud> {
    for folder in folders {
        place(env, folder);
    }
    s.services.library.external_huds(&tf2(env)).unwrap()
}

#[test]
fn a_live_update_blocked_by_changes_says_what_to_do() {
    let err = ReinstallError::Changed {
        folder: "rayshud".into(),
    };
    assert_eq!(
        live_failure(&err),
        "Saved, but your installed HUD has changes of its own, so it wasn't updated. Install \
         this HUD again to choose what to do."
    );
}

#[gpui_kit::test]
fn the_words_follow_the_count(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let names = |count: usize| (0..count).map(|n| format!("h{n:02}")).collect::<Vec<_>>();
    let three = scanned(&s, &env, &names(3));
    assert_eq!(names_text(&three), "h00, h01 and h02");
    let (title, description, label) = dialog_text(HudAction::Uninstall, &three);
    assert_eq!(title, "Keep these HUDs?");
    assert_eq!(
        description,
        "Uninstalling removes h00, h01 and h02 from TF2. TF2Tools has no copy of them, so \
         they can't come back unless you keep one."
    );
    assert_eq!(label, "Remove without keeping");
    let many = scanned(&s, &env, &names(13));
    assert_eq!(
        names_text(&many),
        "h00, h01, h02, h03, h04, h05, h06, h07, h08, h09 and 3 more"
    );
}
