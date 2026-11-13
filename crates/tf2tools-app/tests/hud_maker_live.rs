//! The HUD Maker's live link: Save also updates the installed copy of the same HUD.

use crate::support;

use std::path::PathBuf;

use gpui_kit::{AppContext as _, Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_app::pages::hud_maker::live::{LIVE_NOTE, live_failure};
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{LibraryKind, PartKey, ReinstallError, Tf2Install};

const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";
const HEALTH: &str = "resource/ui/hudplayerhealth.res";

fn stock_shell(cx: &mut TestAppContext, env: &support::Env) -> support::ShellWindow {
    support::seed_stock_game(&env.game);
    support::shell_window(
        cx,
        env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    )
}

fn open_maker(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<HudMakerPage> {
    support::click_nav(cx, s.win, Page::HudMaker);
    let page = cx.update(|cx| s.shell.read(cx).hud_maker_page()).unwrap();
    support::wait_real(cx, "the HUD to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

fn edit(cx: &mut TestAppContext, page: &Entity<HudMakerPage>, colour: &str) {
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            let e = p.editor_mut().unwrap();
            e.select(Some(PartKey::new(HV)));
            e.recolour_selected(colour).unwrap();
            cx.notify();
        })
    });
}

fn set_name(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    text: &str,
) {
    let input = cx.update(|cx| page.read(cx).name_input().clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.set_value(text, window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
}

fn save(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    button: &'static str,
) {
    // Save as new sits in the toolbar's overflow menu.
    if button == ids::HUD_MAKER_SAVE_AS_NEW {
        support::click_id(cx, s.win, ids::HUD_MAKER_MORE);
    }
    support::click_id(cx, s.win, button);
    support::wait_real(cx, "the save", |cx| {
        cx.update(|cx| !page.read(cx).is_saving())
    });
    support::settle(cx, s.win);
}

// Saves a red edit as "My HUD", installs it and returns its folder in tf/custom.
fn saved_and_installed(
    cx: &mut TestAppContext,
    env: &support::Env,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
) -> PathBuf {
    edit(cx, page, "255 0 0 255");
    set_name(cx, s, page, "My HUD");
    save(cx, s, page, ids::HUD_MAKER_SAVE);
    support::click_id(cx, s.win, ids::HUD_MAKER_INSTALL);
    let folder = s.services.library.list_huds().unwrap()[0]
        .folder_name()
        .to_owned();
    let target = env.game.join("tf/custom").join(folder);
    support::wait_real(cx, "the install", |_| target.join("info.vdf").is_file());
    support::wait_real(cx, "the install to finish", |cx| {
        !support::disabled_of(cx, s.win, ids::HUD_MAKER_SAVE)
    });
    support::settle(cx, s.win);
    target
}

fn library_health(env: &support::Env, s: &support::ShellWindow) -> Vec<u8> {
    let folder = s.services.library.list_huds().unwrap()[0]
        .folder_name()
        .to_owned();
    std::fs::read(
        env.data
            .library_dir(LibraryKind::Huds)
            .join(folder)
            .join(HEALTH),
    )
    .unwrap()
}

/// The library file as an install writes it: on Linux its own file path is lowercased.
fn installed_form(library: Vec<u8>) -> Vec<u8> {
    if cfg!(target_os = "linux") {
        String::from_utf8(library)
            .unwrap()
            .replacen(
                "\"Resource/UI/HudPlayerHealth.res\"",
                "\"resource/ui/hudplayerhealth.res\"",
                1,
            )
            .into_bytes()
    } else {
        library
    }
}

fn staging(env: &support::Env) -> PathBuf {
    env.game.join("tf/custom/.tf2tools-reinstall")
}

#[gpui_kit::test]
fn saving_the_installed_hud_updates_it_and_says_how_to_see_it(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    let target = saved_and_installed(cx, &env, &s, &page);
    let before = std::fs::read(target.join(HEALTH)).unwrap();
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_LIVE_NOTE));

    edit(cx, &page, "0 255 0 255");
    save(cx, &s, &page, ids::HUD_MAKER_SAVE);
    let after = std::fs::read(target.join(HEALTH)).unwrap();
    assert_ne!(after, before);
    assert_eq!(after, installed_form(library_health(&env, &s)));
    assert!(!target.join(".tf2tools").exists());
    assert!(!staging(&env).exists());
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_LIVE_NOTE).as_deref(),
        Some(LIVE_NOTE)
    );
    assert!(cx.update(|cx| page.read(cx).live_note_shown()));
    // The player's log names the HUD; where it landed is developer detail.
    let rows = support::log_rows(&s.services);
    let updated = rows
        .iter()
        .find(|r| r.message() == "Updated installed HUD: My HUD")
        .unwrap();
    assert_eq!(updated.details(), None);
}

#[gpui_kit::test]
fn copy_puts_the_console_command_on_the_clipboard(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    saved_and_installed(cx, &env, &s, &page);
    edit(cx, &page, "0 255 0 255");
    save(cx, &s, &page, ids::HUD_MAKER_SAVE);
    support::click_id(cx, s.win, ids::HUD_MAKER_LIVE_COPY);
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some("hud_reloadscheme")
    );
    assert!(
        support::toast_labels(cx, s.win)
            .contains(&"Copied. Paste it into TF2's console.".to_owned())
    );
    support::click_id(cx, s.win, ids::HUD_MAKER_LIVE_CLOSE);
    support::settle(cx, s.win);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_LIVE_NOTE));
}

#[gpui_kit::test]
fn saving_a_hud_that_is_not_installed_leaves_tf_custom_alone(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    let target = saved_and_installed(cx, &env, &s, &page);
    let before = std::fs::read(target.join(HEALTH)).unwrap();

    edit(cx, &page, "0 255 0 255");
    set_name(cx, &s, &page, "My other HUD");
    save(cx, &s, &page, ids::HUD_MAKER_SAVE_AS_NEW);
    assert_eq!(s.services.library.list_huds().unwrap().len(), 2);
    assert_eq!(std::fs::read(target.join(HEALTH)).unwrap(), before);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_LIVE_NOTE));
    let custom = env.game.join("tf/custom");
    let folders = std::fs::read_dir(&custom).unwrap().count();
    assert_eq!(folders, 1, "only the installed HUD");
}

// Unix permissions stand in for a folder TF2 holds open on Windows; given back before the
// temp dir is deleted.
#[cfg(unix)]
fn set_mode(path: &std::path::Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

// The toasts a failed live update shows, by their labels.
#[cfg(unix)]
fn not_updated(cx: &mut TestAppContext, s: &support::ShellWindow, line: &str) {
    let label = format!("Installed copy not updated. {line}");
    assert!(
        support::toast_labels(cx, s.win).contains(&label),
        "{:?}",
        support::toast_labels(cx, s.win)
    );
    assert!(
        !support::present(cx, s.win, ids::HUD_MAKER_LIVE_NOTE),
        "the live note should go once the toast says the copy was not updated"
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_hud_tf2_holds_open_keeps_its_installed_copy_and_says_close_tf2(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    let target = saved_and_installed(cx, &env, &s, &page);
    let before = std::fs::read(target.join(HEALTH)).unwrap();

    // A folder that cannot move to another parent stands in for files TF2 holds open.
    edit(cx, &page, "0 255 0 255");
    set_mode(&target, 0o555);
    save(cx, &s, &page, ids::HUD_MAKER_SAVE);
    set_mode(&target, 0o755);

    assert_eq!(std::fs::read(target.join(HEALTH)).unwrap(), before);
    assert!(!staging(&env).exists());
    assert!(
        cx.update(|cx| page.read(cx).can_leave()),
        "the save itself landed"
    );
    not_updated(
        cx,
        &s,
        "TF2 is using this HUD's files. Close TF2, then Save again to update it.",
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_custom_folder_that_cannot_be_written_says_install_again(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    let target = saved_and_installed(cx, &env, &s, &page);
    let before = std::fs::read(target.join(HEALTH)).unwrap();

    let custom = env.game.join("tf/custom");
    edit(cx, &page, "0 255 0 255");
    set_mode(&custom, 0o555);
    save(cx, &s, &page, ids::HUD_MAKER_SAVE);
    set_mode(&custom, 0o755);

    assert_eq!(std::fs::read(target.join(HEALTH)).unwrap(), before);
    assert!(!staging(&env).exists());
    not_updated(
        cx,
        &s,
        "Saved, but couldn't update the installed copy. Close TF2 and install it again from \
         HUDs.",
    );
}

#[test]
fn a_folder_that_would_not_move_says_tf2_is_using_it() {
    let in_use = "TF2 is using this HUD's files. Close TF2, then Save again to update it.";
    let held = || std::io::Error::other("held open");
    let moved = ReinstallError::InUse {
        path: PathBuf::from("hud"),
        source: held(),
    };
    assert_eq!(live_failure(&moved), in_use);
    let aside = ReinstallError::SetAside {
        path: PathBuf::from("aside"),
        source: held(),
    };
    assert_eq!(live_failure(&aside), in_use);
    let copy = ReinstallError::Unchanged {
        path: PathBuf::from("hud"),
        source: held(),
    };
    assert_eq!(
        live_failure(&copy),
        "Saved, but couldn't update the installed copy. Close TF2 and install it again from \
         HUDs."
    );
}

#[test]
fn a_moved_game_folder_points_to_settings() {
    let gone = ReinstallError::GameMissing {
        path: PathBuf::from("/home/p/Games/Team Fortress 2/tf"),
    };
    assert_eq!(
        live_failure(&gone),
        "Saved, but TF2 wasn't found where it used to be. Check the TF2 folder in \
         Settings."
    );
}

#[gpui_kit::test]
fn a_later_save_as_new_takes_the_note_away(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    saved_and_installed(cx, &env, &s, &page);
    edit(cx, &page, "0 255 0 255");
    save(cx, &s, &page, ids::HUD_MAKER_SAVE);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_LIVE_NOTE));

    set_name(cx, &s, &page, "My other HUD");
    save(cx, &s, &page, ids::HUD_MAKER_SAVE_AS_NEW);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_LIVE_NOTE));
    assert!(!cx.update(|cx| page.read(cx).live_note_shown()));
}

#[gpui_kit::test]
fn opening_another_hud_takes_the_note_away(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    saved_and_installed(cx, &env, &s, &page);
    let installed = s.services.library.list_huds().unwrap()[0].id().clone();
    // A second library HUD, then back to the installed one for a live update.
    edit(cx, &page, "0 0 255 255");
    set_name(cx, &s, &page, "My other HUD");
    save(cx, &s, &page, ids::HUD_MAKER_SAVE_AS_NEW);
    let other = (s.services.library.list_huds().unwrap().into_iter())
        .find(|h| h.id() != &installed)
        .unwrap()
        .id()
        .clone();
    let reopen = |cx: &mut TestAppContext, id: tf2tools_core::HudId| {
        cx.update(|cx| page.update(cx, |p, cx| p.open(id, cx)));
        support::wait_real(cx, "the HUD to open", |cx| {
            cx.update(|cx| page.read(cx).is_loaded())
        });
        support::settle(cx, s.win);
    };
    reopen(cx, installed);
    edit(cx, &page, "0 255 0 255");
    save(cx, &s, &page, ids::HUD_MAKER_SAVE);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_LIVE_NOTE));

    reopen(cx, other);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_LIVE_NOTE));
}
