use crate::support;

use std::path::PathBuf;
use std::sync::Arc;

use gpui_kit::{Entity, TestAppContext};
use tf2tools_app::font_check::FontCheckSession;
use tf2tools_app::ids;
use tf2tools_app::pages::huds::HudsPage;
use tf2tools_app::pages::profiles::probe::Tf2Probe;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::font_check::ask_about_fonts;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{DraftSlot, HudEntry, LibraryKind, ProfileDraft, Provenance, Tf2Install};

use support::fonts::{FONTS, FakeFontHost, font_hud_zip_bytes, write_font_hud};

const FONTS_TOAST: &str =
    "Fonts installed. Installed 2 fonts for fonthud. Restart TF2 to use them.";

fn found(env: &support::Env) -> Tf2State {
    Tf2State::Found(Tf2Install::from_path(&env.game).unwrap())
}

fn fonts_dir(env: &support::Env) -> PathBuf {
    env.tmp.path().join("user-fonts")
}

fn host(env: &support::Env, installed: &[&str]) -> Arc<FakeFontHost> {
    Arc::new(FakeFontHost::new(installed, &fonts_dir(env)))
}

/// Imports a library HUD called `name` that ships `FONTS`.
fn seed(s: &support::ShellWindow, env: &support::Env, name: &str) -> HudEntry {
    let path = env.tmp.path().join(format!("{name}.zip"));
    std::fs::write(&path, font_hud_zip_bytes(name, FONTS)).unwrap();
    s.services
        .library
        .import_hud(&path, Provenance::Local)
        .unwrap()
}

/// The HUDs page once its loads, the tf/custom scan included, have landed.
fn open_huds(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<HudsPage> {
    let page = cx
        .update(|cx| s.shell.read(cx).huds_page())
        .expect("a HUDs page");
    support::wait_real(cx, "HUDs to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded() && page.read(cx).is_idle())
    });
    support::settle(cx, s.win);
    page
}

/// Leaves for Settings and comes back, which builds a fresh page.
fn revisit(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<HudsPage> {
    support::click_id(cx, s.win, ids::nav(Page::Settings));
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    open_huds(cx, s)
}

fn started(cx: &mut TestAppContext) -> usize {
    cx.update(|cx| FontCheckSession::started(cx))
}

fn wait_finished(cx: &mut TestAppContext, n: usize) {
    support::wait_real(cx, "the font check", |cx| {
        cx.update(|cx| FontCheckSession::finished(cx)) >= n
    });
}

fn wait_dialog(cx: &mut TestAppContext, s: &support::ShellWindow) {
    support::wait_real(cx, "the font dialog", |cx| {
        support::present(cx, s.win, ids::HUD_FONTS_DIALOG)
    });
}

fn install_fonthud(cx: &mut TestAppContext, s: &support::ShellWindow, env: &support::Env) {
    seed(s, env, "fonthud");
    revisit(cx, s);
    support::click_id(cx, s.win, ids::hud_install("fonthud"));
}

#[gpui_kit::test]
fn each_folder_is_claimed_once_per_session(cx: &mut TestAppContext) {
    cx.update(|cx| {
        assert!(FontCheckSession::claim(cx, "budhud"));
        assert!(!FontCheckSession::claim(cx, "budhud"));
        assert!(FontCheckSession::claim(cx, "rayshud"));
        assert_eq!(FontCheckSession::started(cx), 0);
    });
}

#[gpui_kit::test]
fn an_install_asks_and_installs_the_missing_fonts(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window_fonts(cx, &env, found(&env), host(&env, &[]));
    install_fonthud(cx, &s, &env);
    wait_dialog(cx, &s);
    support::click_id(cx, s.win, ids::HUD_FONTS_INSTALL);
    support::wait_real(cx, "the fonts toast", |cx| {
        support::toast_labels(cx, s.win)
            .iter()
            .any(|l| l == FONTS_TOAST)
    });
    assert!(fonts_dir(&env).join("surface.ttf").is_file());
    assert!(fonts_dir(&env).join("codepro.ttf").is_file());
    assert!(!fonts_dir(&env).join("marlett.ttf").exists());
    let rows = support::log_rows(&s.services);
    assert!(
        rows.iter()
            .any(|r| r.category() == "hud" && r.action() == "install_fonts")
    );
}

#[gpui_kit::test]
fn skip_installs_nothing_and_remembers_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window_fonts(cx, &env, found(&env), host(&env, &[]));
    install_fonthud(cx, &s, &env);
    wait_dialog(cx, &s);
    support::click_id(cx, s.win, ids::HUD_FONTS_SKIP);
    assert!(!support::present(cx, s.win, ids::HUD_FONTS_DIALOG));
    assert!(!fonts_dir(&env).exists());
    assert!(
        s.services
            .db
            .font_check_dismissed("fonthud")
            .unwrap()
            .is_empty()
    );
    let rows = support::log_rows(&s.services);
    assert!(!rows.iter().any(|r| r.action() == "install_fonts"));
    // The answer frees the dialog slot, so a reinstall asks again.
    revisit(cx, &s);
    support::click_id(cx, s.win, ids::HUDS_UNINSTALL);
    support::wait_real(cx, "the uninstall", |_| {
        !env.game.join("tf/custom/fonthud").exists()
    });
    revisit(cx, &s);
    support::click_id(cx, s.win, ids::hud_install("fonthud"));
    wait_dialog(cx, &s);
}

#[gpui_kit::test]
fn dont_ask_again_holds_across_a_reinstall(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window_fonts(cx, &env, found(&env), host(&env, &[]));
    install_fonthud(cx, &s, &env);
    wait_dialog(cx, &s);
    support::click_id(cx, s.win, ids::HUD_FONTS_DONT_ASK);
    support::wait_real(cx, "the stored choice", |_| {
        !s.services
            .db
            .font_check_dismissed("fonthud")
            .unwrap()
            .is_empty()
    });
    assert_eq!(
        s.services.db.font_check_dismissed("fonthud").unwrap(),
        ["font:code pro", "font:surface"]
    );
    revisit(cx, &s);
    support::click_id(cx, s.win, ids::HUDS_UNINSTALL);
    // The uninstall runs off the UI thread; leaving the page before it lands would race it.
    support::wait_real(cx, "the uninstall", |_| {
        !env.game.join("tf/custom/fonthud").exists()
    });
    revisit(cx, &s);
    support::click_id(cx, s.win, ids::hud_install("fonthud"));
    wait_finished(cx, 2);
    assert!(!support::present(cx, s.win, ids::HUD_FONTS_DIALOG));
}

#[gpui_kit::test]
fn installed_fonts_ask_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let installed = host(&env, &["Surface", "Code Pro"]);
    let s = support::shell_window_fonts(cx, &env, found(&env), installed);
    install_fonthud(cx, &s, &env);
    wait_finished(cx, 1);
    assert!(!support::present(cx, s.win, ids::HUD_FONTS_DIALOG));
}

#[gpui_kit::test]
fn without_a_font_host_an_install_asks_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    install_fonthud(cx, &s, &env);
    wait_finished(cx, 1);
    assert!(!support::present(cx, s.win, ids::HUD_FONTS_DIALOG));
    assert_eq!(started(cx), 1);
}

#[gpui_kit::test]
fn the_active_hud_is_checked_once_when_the_page_loads(cx: &mut TestAppContext) {
    let env = support::env();
    write_font_hud(&env.game.join("tf/custom/fonthud"), "fonthud", FONTS);
    let s = support::shell_window_fonts(cx, &env, found(&env), host(&env, &[]));
    wait_dialog(cx, &s);
    support::click_id(cx, s.win, ids::HUD_FONTS_SKIP);
    revisit(cx, &s);
    assert_eq!(started(cx), 1);
    assert!(!support::present(cx, s.win, ids::HUD_FONTS_DIALOG));
}

#[gpui_kit::test]
fn an_install_is_not_checked_again_by_its_reload(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window_fonts(cx, &env, found(&env), host(&env, &[]));
    install_fonthud(cx, &s, &env);
    wait_dialog(cx, &s);
    support::click_id(cx, s.win, ids::HUD_FONTS_SKIP);
    open_huds(cx, &s);
    assert_eq!(started(cx), 1);
}

#[gpui_kit::test]
fn conflicting_huds_are_not_checked(cx: &mut TestAppContext) {
    let env = support::env();
    write_font_hud(&env.game.join("tf/custom/fonthud"), "fonthud", FONTS);
    write_font_hud(&env.game.join("tf/custom/otherhud"), "otherhud", FONTS);
    let s = support::shell_window_fonts(cx, &env, found(&env), host(&env, &[]));
    open_huds(cx, &s);
    assert_eq!(started(cx), 0);
}

fn open_profiles(cx: &mut TestAppContext, s: &support::ShellWindow) {
    cx.update(|cx| Tf2Probe::install(cx, || false));
    support::click_id(cx, s.win, ids::nav(Page::Profiles));
    let page = cx
        .update(|cx| s.shell.read(cx).profiles_page())
        .expect("a Profiles page");
    support::wait_real(cx, "Profiles to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn a_profile_with_a_hud_checks_its_fonts(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window_fonts(cx, &env, found(&env), host(&env, &[]));
    let hud = seed(&s, &env, "fonthud");
    let draft = ProfileDraft::new("Comp").with_hud(DraftSlot::Item(hud.id().clone()));
    let comp = s.services.library.create_profile(&draft).unwrap();
    open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::profile_activate(comp.id()));
    wait_dialog(cx, &s);
    let description = support::label_of(cx, s.win, ids::HUD_FONTS_DESCRIPTION).unwrap();
    assert!(
        description.starts_with("fonthud uses fonts"),
        "{description}"
    );
    support::click_id(cx, s.win, ids::HUD_FONTS_SKIP);
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    open_huds(cx, &s);
    assert_eq!(started(cx), 1);
}

#[gpui_kit::test]
fn a_profile_without_a_hud_checks_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window_fonts(cx, &env, found(&env), host(&env, &[]));
    let vanilla = s
        .services
        .library
        .create_profile(&ProfileDraft::new("Vanilla"))
        .unwrap();
    open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::profile_activate(vanilla.id()));
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    support::wait_real(cx, "the activate toast", |cx| {
        support::toast_labels(cx, s.win)
            .iter()
            .any(|l| l.starts_with("Profile \"Vanilla\""))
    });
    assert_eq!(started(cx), 0);
}

#[gpui_kit::test]
fn a_failed_install_leaves_its_folder_to_the_page_load_check(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window_fonts(cx, &env, found(&env), host(&env, &[]));
    seed(&s, &env, "fonthud");
    // Without its library copy the install fails before touching tf/custom.
    std::fs::remove_dir_all(
        env.data
            .library_dir(tf2tools_core::LibraryKind::Huds)
            .join("fonthud"),
    )
    .unwrap();
    revisit(cx, &s);
    support::click_id(cx, s.win, ids::hud_install("fonthud"));
    support::wait_real(cx, "the failure toast", |cx| {
        support::toast_labels(cx, s.win)
            .iter()
            .any(|t| t.starts_with("Failed to install HUD"))
    });
    write_font_hud(&env.game.join("tf/custom/fonthud"), "fonthud", FONTS);
    revisit(cx, &s);
    wait_dialog(cx, &s);
}

#[gpui_kit::test]
fn a_check_claims_its_folder_before_it_runs(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window_fonts(cx, &env, found(&env), host(&env, &[]));
    open_huds(cx, &s);
    write_font_hud(&env.game.join("tf/custom/fonthud"), "fonthud", FONTS);
    let tf2 = Tf2Install::from_path(&env.game).unwrap();
    // A page load landing while this check runs must find the folder taken.
    cx.update(|cx| {
        ask_about_fonts(s.win, tf2, "fonthud".into(), cx);
        assert!(!FontCheckSession::claim(cx, "fonthud"));
    });
    wait_dialog(cx, &s);
}

// A check that lands while another HUD's dialog shows is dropped; it must free its folder so the
// next page load asks, while a duplicate check of the showing HUD keeps that HUD claimed.
#[gpui_kit::test]
fn a_check_dropped_behind_another_dialog_asks_again_later(cx: &mut TestAppContext) {
    let env = support::env();
    write_font_hud(&env.game.join("tf/custom/fonthud"), "fonthud", FONTS);
    let s = support::shell_window_fonts(cx, &env, found(&env), host(&env, &[]));
    wait_dialog(cx, &s);
    write_font_hud(&env.game.join("tf/custom/otherhud"), "otherhud", FONTS);
    let tf2 = Tf2Install::from_path(&env.game).unwrap();
    cx.update(|cx| {
        ask_about_fonts(s.win, tf2.clone(), "otherhud".into(), cx);
        ask_about_fonts(s.win, tf2, "fonthud".into(), cx);
    });
    wait_finished(cx, 3);
    let description = support::label_of(cx, s.win, ids::HUD_FONTS_DESCRIPTION).unwrap();
    assert!(
        description.starts_with("fonthud uses fonts"),
        "{description}"
    );
    assert!(
        !cx.update(|cx| FontCheckSession::claim(cx, "fonthud")),
        "the showing HUD keeps its claim"
    );
    support::click_id(cx, s.win, ids::HUD_FONTS_SKIP);
    std::fs::remove_dir_all(env.game.join("tf/custom/fonthud")).unwrap();
    revisit(cx, &s);
    assert_eq!(started(cx), 4, "the page load asks about otherhud");
    wait_dialog(cx, &s);
    let description = support::label_of(cx, s.win, ids::HUD_FONTS_DESCRIPTION).unwrap();
    assert!(
        description.starts_with("otherhud uses fonts"),
        "{description}"
    );
}

#[gpui_kit::test]
fn an_install_under_its_expected_folder_checks_that_folder(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window_fonts(cx, &env, found(&env), host(&env, &[]));
    seed(&s, &env, "fonthud-abc1234");
    let extra = env
        .data
        .library_dir(LibraryKind::Huds)
        .join("fonthud-abc1234/resource/ui/extra.res");
    std::fs::create_dir_all(extra.parent().unwrap()).unwrap();
    std::fs::write(
        &extra,
        "#base \"../../custom/fonthud/resource/ui/base.res\"\n",
    )
    .unwrap();
    revisit(cx, &s);
    support::click_id(cx, s.win, ids::hud_install("fonthud-abc1234"));
    wait_dialog(cx, &s);
    assert!(env.game.join("tf/custom/fonthud/info.vdf").is_file());
    support::click_id(cx, s.win, ids::HUD_FONTS_SKIP);
    open_huds(cx, &s);
    // The install's own check found the HUD, so the reload has nothing left to check.
    assert_eq!(started(cx), 1);
}
