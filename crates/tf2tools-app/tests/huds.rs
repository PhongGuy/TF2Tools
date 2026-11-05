mod support;

use std::cell::Cell;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::library_events::{LibraryChanged, LibraryEvents};
use tf2tools_app::pages::huds::HudsPage;
use tf2tools_app::picker::{PickKind, ScriptedPicker};
use tf2tools_app::services::{Services, Tf2State};
use tf2tools_app::shell::route::Page;
use tf2tools_core::{GbId, HudEntry, LibraryKind, LogEntry, Provenance, Tf2Install};

fn found(env: &support::Env) -> Tf2State {
    Tf2State::Found(Tf2Install::from_path(&env.game).unwrap())
}

/// The HUDs page, the shell's start page, once its first load is done.
fn open_huds(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<HudsPage> {
    let page = cx
        .update(|cx| s.shell.read(cx).huds_page())
        .expect("a HUDs page");
    support::wait_real(cx, "HUDs to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

#[gpui_kit::test]
fn huds_is_a_real_page(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_huds(cx, &s);
    assert!(support::present(cx, s.win, ids::page(Page::Huds)));
    assert!(!support::present(cx, s.win, ids::PLACEHOLDER_MESSAGE));
}

/// A minimal HUD archive, `<name>/info.vdf` plus a layout file, written into `dir`.
fn hud_zip(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(format!("{name}.zip"));
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file(format!("{name}/info.vdf"), options).unwrap();
    write!(zip, "\"{name}\"\n{{\n\t\"ui_version\"\t\"3\"\n}}\n").unwrap();
    zip.start_file(format!("{name}/resource/ui/hudlayout.res"), options)
        .unwrap();
    write!(zip, "\"Resource/HudLayout.res\"\n{{\n}}\n").unwrap();
    zip.finish().unwrap();
    path
}

fn seed(services: &Services, env: &support::Env, name: &str, provenance: Provenance) -> HudEntry {
    let archive = hud_zip(env.tmp.path(), name);
    services.library.import_hud(&archive, provenance).unwrap()
}

/// A HUD folder placed in tf/custom by hand, outside the library.
fn place_external(env: &support::Env, folder: &str) {
    let dir = env.game.join("tf/custom").join(folder);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("info.vdf"), format!("\"{folder}\"\n{{\n}}\n")).unwrap();
}

/// Leaves for Settings and comes back, which builds a fresh page.
fn revisit(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<HudsPage> {
    support::click_id(cx, s.win, ids::nav(Page::Settings));
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    open_huds(cx, s)
}

#[gpui_kit::test]
fn no_installed_hud_shows_the_empty_card(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_huds(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUDS_INSTALLED_EMPTY).as_deref(),
        Some("No HUD installed")
    );
    assert!(!support::present(cx, s.win, ids::HUDS_UNINSTALL));
}

#[gpui_kit::test]
fn a_library_install_is_managed(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let hud = seed(&s.services, &env, "rayshud", Provenance::Local);
    let tf2 = Tf2Install::from_path(&env.game).unwrap();
    s.services.library.install_hud(hud.id(), &tf2).unwrap();
    let _page = revisit(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUDS_INSTALLED_NAME).as_deref(),
        Some("rayshud")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::HUDS_INSTALLED_BADGE).as_deref(),
        Some("Managed")
    );
    let path = env
        .game
        .join("tf")
        .join("custom")
        .join("rayshud")
        .display()
        .to_string();
    assert_eq!(
        support::label_of(cx, s.win, ids::HUDS_INSTALLED_PATH),
        Some(path)
    );
    assert!(support::present(cx, s.win, ids::HUDS_UNINSTALL));
}

#[gpui_kit::test]
fn a_hand_placed_hud_is_external(cx: &mut TestAppContext) {
    let env = support::env();
    place_external(&env, "myhud");
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_huds(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUDS_INSTALLED_NAME).as_deref(),
        Some("myhud")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::HUDS_INSTALLED_BADGE).as_deref(),
        Some("External")
    );
}

#[gpui_kit::test]
fn two_installed_huds_show_the_conflict_box(cx: &mut TestAppContext) {
    let env = support::env();
    place_external(&env, "budhud");
    place_external(&env, "rayshud");
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_huds(cx, &s);
    let label = support::label_of(cx, s.win, ids::HUDS_CONFLICTS).unwrap();
    assert_eq!(
        label,
        "HUD Conflicts Detected. TF2 can only use one HUD at a time. Uninstall the ones you don't want until one is left."
    );
    assert!(!support::present(cx, s.win, ids::HUDS_INSTALLED_EMPTY));
}

#[gpui_kit::test]
fn the_installed_card_follows_tf2_state(cx: &mut TestAppContext) {
    let env = support::env();
    place_external(&env, "myhud");
    let s = support::shell_window(cx, &env, Tf2State::Detecting);
    let _page = open_huds(cx, &s);
    assert!(support::present(cx, s.win, ids::HUDS_INSTALLED_EMPTY));
    let state = found(&env);
    cx.update(|cx| {
        s.tf2.update(cx, |tf2, cx| {
            *tf2 = state;
            cx.notify();
        })
    });
    support::wait_real(cx, "the installed card", |cx| {
        support::label_of(cx, s.win, ids::HUDS_INSTALLED_NAME).as_deref() == Some("myhud")
    });
}

fn search(cx: &mut TestAppContext, s: &support::ShellWindow, page: &Entity<HudsPage>, text: &str) {
    let input = cx.update(|cx| page.read(cx).search_input().clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.set_value(text, window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn the_library_shows_a_card_per_hud(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    seed(&s.services, &env, "budhud", Provenance::Local);
    seed(&s.services, &env, "flawhud", Provenance::Local);
    let _page = revisit(cx, &s);
    for folder in ["budhud", "flawhud"] {
        assert_eq!(
            support::label_of(cx, s.win, ids::hud_card(folder)).as_deref(),
            Some(folder)
        );
        assert_eq!(
            support::label_of(cx, s.win, ids::hud_badge(folder)).as_deref(),
            Some("Local")
        );
        assert!(!support::present(cx, s.win, ids::hud_author(folder)));
        assert!(support::present(cx, s.win, ids::hud_install(folder)));
        assert!(support::present(cx, s.win, ids::hud_delete(folder)));
    }
    assert!(!support::present(cx, s.win, ids::HUDS_EMPTY));
}

#[gpui_kit::test]
fn a_store_hud_shows_store_and_its_author(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let gb = Provenance::GameBanana {
        gb_id: GbId::new(7),
        author: "Ray".into(),
    };
    seed(&s.services, &env, "rayshud", gb);
    let _page = revisit(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_badge("rayshud")).as_deref(),
        Some("Store")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_author("rayshud")).as_deref(),
        Some("by Ray")
    );
}

#[gpui_kit::test]
fn an_empty_library_says_so(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_huds(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUDS_EMPTY).as_deref(),
        Some("Your library is empty")
    );
    assert!(support::present(cx, s.win, ids::HUDS_ADD));
}

#[gpui_kit::test]
fn search_filters_by_name_ignoring_case(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    seed(&s.services, &env, "budhud", Provenance::Local);
    seed(&s.services, &env, "flawhud", Provenance::Local);
    let page = revisit(cx, &s);
    search(cx, &s, &page, "FLAW");
    assert!(support::present(cx, s.win, ids::hud_card("flawhud")));
    assert!(!support::present(cx, s.win, ids::hud_card("budhud")));
    search(cx, &s, &page, "zzz");
    assert_eq!(
        support::label_of(cx, s.win, ids::HUDS_NO_MATCH).as_deref(),
        Some("No HUDs found")
    );
    assert!(!support::present(cx, s.win, ids::HUDS_EMPTY));
    search(cx, &s, &page, "");
    assert!(support::present(cx, s.win, ids::hud_card("budhud")));
}

#[gpui_kit::test]
fn a_revisit_reloads_the_library(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_huds(cx, &s);
    assert!(support::present(cx, s.win, ids::HUDS_EMPTY));
    seed(&s.services, &env, "budhud", Provenance::Local);
    let _page = revisit(cx, &s);
    assert!(support::present(cx, s.win, ids::hud_card("budhud")));
}

#[gpui_kit::test]
fn add_hud_imports_the_picked_archive(cx: &mut TestAppContext) {
    let env = support::env();
    let archive = hud_zip(env.tmp.path(), "budhud");
    let picker = Arc::new(ScriptedPicker::new([archive]));
    let s = support::shell_window_with(cx, &env, found(&env), picker.clone(), support::no_steam());
    let _page = open_huds(cx, &s);
    support::click_id(cx, s.win, ids::HUDS_ADD);
    support::wait_real(cx, "the import toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::TOAST).as_deref(),
        Some("HUD imported. budhud added to your library")
    );
    assert_eq!(picker.asked(), vec![PickKind::HudArchive]);
    assert!(support::present(cx, s.win, ids::hud_card("budhud")));
}

#[gpui_kit::test]
fn an_import_whose_name_is_taken_keeps_both_and_names_the_new_one(cx: &mut TestAppContext) {
    let env = support::env();
    let archive = hud_zip(env.tmp.path(), "budhud");
    let picker = Arc::new(ScriptedPicker::new([archive]));
    let s = support::shell_window_with(cx, &env, found(&env), picker, support::no_steam());
    seed(&s.services, &env, "budhud", Provenance::Local);
    let _page = revisit(cx, &s);
    support::click_id(cx, s.win, ids::HUDS_ADD);
    support::wait_real(cx, "the import toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::TOAST).as_deref(),
        Some("HUD imported. Added to your library as budhud (2)")
    );
    assert_eq!(s.services.library.list_huds().unwrap().len(), 2);
}

#[gpui_kit::test]
fn a_cancelled_pick_changes_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window_with(
        cx,
        &env,
        found(&env),
        Arc::new(ScriptedPicker::empty()),
        support::no_steam(),
    );
    let _page = open_huds(cx, &s);
    support::click_id(cx, s.win, ids::HUDS_ADD);
    support::settle(cx, s.win);
    assert!(!support::present(cx, s.win, ids::TOAST));
    assert!(support::present(cx, s.win, ids::HUDS_EMPTY));
}

#[gpui_kit::test]
fn a_non_hud_archive_is_toasted(cx: &mut TestAppContext) {
    let env = support::env();
    let archive = env.tmp.path().join("notes.zip");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
    zip.start_file("readme.txt", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"not a hud").unwrap();
    zip.finish().unwrap();
    let picker = Arc::new(ScriptedPicker::new([archive]));
    let s = support::shell_window_with(cx, &env, found(&env), picker, support::no_steam());
    let _page = open_huds(cx, &s);
    support::click_id(cx, s.win, ids::HUDS_ADD);
    support::wait_real(cx, "the error toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    let toast = support::label_of(cx, s.win, ids::TOAST).unwrap();
    assert!(toast.starts_with("Failed to import HUD. "), "{toast}");
}

#[gpui_kit::test]
fn install_makes_a_hud_active(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    seed(&s.services, &env, "budhud", Provenance::Local);
    let _page = revisit(cx, &s);
    support::click_id(cx, s.win, ids::hud_install("budhud"));
    support::wait_real(cx, "the install toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::TOAST).as_deref(),
        Some("HUD installed. budhud is now active")
    );
    assert!(env.game.join("tf/custom/budhud/info.vdf").is_file());
    assert_eq!(
        support::label_of(cx, s.win, ids::HUDS_INSTALLED_BADGE).as_deref(),
        Some("Managed")
    );
}

// Sets the shared TF2 state, as Settings or a recheck would.
fn set_tf2(cx: &mut TestAppContext, s: &support::ShellWindow, state: Tf2State) {
    cx.update(|cx| {
        s.tf2.update(cx, |tf2, cx| {
            *tf2 = state;
            cx.notify();
        })
    });
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn without_tf2_the_page_warns_and_shuts_installs(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    seed(&s.services, &env, "budhud", Provenance::Local);
    let _page = revisit(cx, &s);
    assert!(support::present(cx, s.win, ids::HUDS_NO_TF2));
    assert!(support::disabled_of(cx, s.win, ids::hud_install("budhud")));
    assert!(!support::disabled_of(cx, s.win, ids::HUDS_ADD));
    assert!(!support::disabled_of(cx, s.win, ids::hud_delete("budhud")));
    support::click_id(cx, s.win, ids::HUDS_NO_TF2_SETTINGS);
    assert!(support::present(cx, s.win, ids::page(Page::Settings)));
    assert!(!env.game.join("tf/custom/budhud").exists());
}

#[gpui_kit::test]
fn a_moved_tf2_folder_is_titled_not_found(cx: &mut TestAppContext) {
    // A saved folder that moved away is "set", so the title must not claim it isn't.
    let env = support::env();
    let saved = Some(env.game.join("gone"));
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved });
    let _page = revisit(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUDS_NO_TF2).as_deref(),
        Some("TF2 not found")
    );
}

#[gpui_kit::test]
fn finding_tf2_clears_the_warning_and_opens_installs(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    seed(&s.services, &env, "budhud", Provenance::Local);
    let _page = revisit(cx, &s);
    assert!(support::present(cx, s.win, ids::HUDS_NO_TF2));
    set_tf2(cx, &s, found(&env));
    assert!(!support::present(cx, s.win, ids::HUDS_NO_TF2));
    assert!(!support::disabled_of(cx, s.win, ids::hud_install("budhud")));
}

#[gpui_kit::test]
fn uninstall_removes_the_installed_hud(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let hud = seed(&s.services, &env, "rayshud", Provenance::Local);
    s.services
        .library
        .install_hud(hud.id(), &Tf2Install::from_path(&env.game).unwrap())
        .unwrap();
    let _page = revisit(cx, &s);
    support::click_id(cx, s.win, ids::HUDS_UNINSTALL);
    support::wait_real(cx, "the uninstall toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::TOAST).as_deref(),
        Some("HUD uninstalled. rayshud has been removed from TF2")
    );
    assert!(!env.game.join("tf/custom/rayshud").exists());
    assert!(support::present(cx, s.win, ids::HUDS_INSTALLED_EMPTY));
}

#[gpui_kit::test]
fn an_install_survives_leaving_the_page(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    seed(&s.services, &env, "budhud", Provenance::Local);
    let page = revisit(cx, &s);
    // Start the install and leave in one update, so the page is gone when the task resumes.
    cx.update_window(s.win, |_, window, cx| {
        window.click(ids::hud_install("budhud"), cx);
        s.shell
            .update(cx, |shell, cx| shell.navigate(Page::Settings, cx));
    })
    .unwrap();
    drop(page);
    support::wait_real(cx, "the install toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    assert!(env.game.join("tf/custom/budhud/info.vdf").is_file());
}

#[gpui_kit::test]
fn delete_asks_then_removes(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    seed(&s.services, &env, "budhud", Provenance::Local);
    let _page = revisit(cx, &s);
    support::click_id(cx, s.win, ids::hud_delete("budhud"));
    assert_eq!(
        support::label_of(cx, s.win, ids::CONFIRM_DESCRIPTION).as_deref(),
        Some("Are you sure you want to delete budhud from your library? This cannot be undone.")
    );
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    support::wait_real(cx, "the delete toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::TOAST).as_deref(),
        Some("HUD removed. budhud has been deleted from your library")
    );
    assert!(s.services.library.list_huds().unwrap().is_empty());
    assert!(!support::present(cx, s.win, ids::hud_card("budhud")));
}

#[gpui_kit::test]
fn cancelling_delete_keeps_the_hud(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    seed(&s.services, &env, "budhud", Provenance::Local);
    let _page = revisit(cx, &s);
    support::click_id(cx, s.win, ids::hud_delete("budhud"));
    support::click_id(cx, s.win, ids::CONFIRM_CANCEL);
    support::settle(cx, s.win);
    assert_eq!(s.services.library.list_huds().unwrap().len(), 1);
    assert!(support::present(cx, s.win, ids::hud_card("budhud")));
    assert!(!support::present(cx, s.win, ids::TOAST));
}

#[gpui_kit::test]
fn search_finds_a_hud_added_while_the_page_is_open(cx: &mut TestAppContext) {
    let env = support::env();
    let archive = hud_zip(env.tmp.path(), "flawhud");
    let picker = Arc::new(ScriptedPicker::new([archive]));
    let s = support::shell_window_with(cx, &env, found(&env), picker, support::no_steam());
    seed(&s.services, &env, "budhud", Provenance::Local);
    let page = revisit(cx, &s);
    search(cx, &s, &page, "FLAW");
    assert!(support::present(cx, s.win, ids::HUDS_NO_MATCH));
    support::click_id(cx, s.win, ids::HUDS_ADD);
    support::wait_real(cx, "the import", |cx| {
        support::present(cx, s.win, ids::hud_card("flawhud"))
    });
    assert!(!support::present(cx, s.win, ids::hud_card("budhud")));
}

fn row(e: &LogEntry) -> (&str, &str, &str) {
    (e.category(), e.action(), e.message())
}

#[gpui_kit::test]
fn hud_operations_write_v5s_rows(cx: &mut TestAppContext) {
    let env = support::env();
    let archive = hud_zip(env.tmp.path(), "budhud");
    let picker = Arc::new(ScriptedPicker::new([archive.clone()]));
    let s = support::shell_window_with(cx, &env, found(&env), picker, support::no_steam());
    let _page = open_huds(cx, &s);
    // Each step waits for its row, then revisits, so the next click lands on a fresh, idle page.
    let rows_then_revisit = |cx: &mut TestAppContext, n: usize| {
        support::wait_real(cx, "the row", |_| support::log_rows(&s.services).len() == n);
        revisit(cx, &s);
    };
    support::click_id(cx, s.win, ids::HUDS_ADD);
    rows_then_revisit(cx, 1);
    support::click_id(cx, s.win, ids::hud_install("budhud"));
    rows_then_revisit(cx, 2);
    support::click_id(cx, s.win, ids::HUDS_UNINSTALL);
    rows_then_revisit(cx, 3);
    support::click_id(cx, s.win, ids::hud_delete("budhud"));
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    support::wait_real(cx, "the row", |_| support::log_rows(&s.services).len() == 4);
    let rows = support::log_rows(&s.services);
    let got: Vec<_> = rows.iter().rev().map(row).collect();
    assert_eq!(
        got,
        [
            ("hud", "import", "Imported HUD: budhud"),
            ("hud", "install", "Installed HUD: budhud"),
            ("hud", "uninstall", "Uninstalled HUD(s): budhud"),
            ("hud", "delete", "Deleted HUD from library: budhud"),
        ]
    );
    assert_eq!(
        rows[3].details().map(str::to_owned),
        Some(format!("Source: {}", archive.display()))
    );
    assert_eq!(
        rows[2].details().map(str::to_owned),
        Some(format!(
            "Installed to: {}",
            env.game.join("tf").join("custom").join("budhud").display()
        ))
    );
}

#[gpui_kit::test]
fn a_failed_install_writes_no_row(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let hud = seed(&s.services, &env, "budhud", Provenance::Local);
    let _page = revisit(cx, &s);
    let folder = s
        .services
        .data
        .library_dir(tf2tools_core::LibraryKind::Huds)
        .join(hud.folder_name());
    std::fs::remove_dir_all(folder).unwrap();
    support::click_id(cx, s.win, ids::hud_install("budhud"));
    support::wait_real(cx, "the toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    let toast = support::label_of(cx, s.win, ids::TOAST).unwrap();
    assert!(toast.starts_with("Failed to install HUD. "), "{toast}");
    support::assert_plain(&toast);
    assert!(support::log_rows(&s.services).is_empty());
}

#[gpui_kit::test]
fn a_library_change_reloads_the_open_page(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_huds(cx, &s);
    assert!(support::present(cx, s.win, ids::HUDS_EMPTY));
    seed(&s.services, &env, "budhud", Provenance::Local);
    cx.update(LibraryEvents::changed);
    support::wait_real(cx, "the reload", |cx| {
        support::present(cx, s.win, ids::hud_card("budhud"))
    });
}

#[gpui_kit::test]
fn a_successful_install_emits_once_and_a_failed_one_not_at_all(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    seed(&s.services, &env, "budhud", Provenance::Local);
    let _page = revisit(cx, &s);
    let count = Rc::new(Cell::new(0));
    let seen = count.clone();
    let _sub = cx.update(|cx| {
        let events = LibraryEvents::global(cx);
        cx.subscribe(&events, move |_, _: &LibraryChanged, _| {
            seen.set(seen.get() + 1)
        })
    });
    support::click_id(cx, s.win, ids::hud_install("budhud"));
    support::wait_real(cx, "the install toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    assert_eq!(count.get(), 1);
    // The page still shows the install, reloaded through the event alone.
    assert_eq!(
        support::label_of(cx, s.win, ids::HUDS_INSTALLED_BADGE).as_deref(),
        Some("Managed")
    );
    // Deleting the library folder makes the next install fail; that emits nothing.
    support::click_id(cx, s.win, ids::HUDS_UNINSTALL);
    support::wait_real(cx, "the uninstall", |_| count.get() == 2);
    std::fs::remove_dir_all(
        env.data
            .library_dir(tf2tools_core::LibraryKind::Huds)
            .join("budhud"),
    )
    .unwrap();
    support::click_id(cx, s.win, ids::hud_install("budhud"));
    support::wait_real(cx, "the failure toast", |cx| {
        support::toast_labels(cx, s.win)
            .iter()
            .any(|t| t.starts_with("Failed to install HUD"))
    });
    assert_eq!(count.get(), 2);
}

#[gpui_kit::test]
fn a_repeated_load_failure_toasts_once(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let page = cx
        .update(|cx| s.shell.read(cx).huds_page())
        .expect("a HUDs page");
    let reload = |cx: &mut TestAppContext| {
        cx.update(LibraryEvents::changed);
        support::wait_real(cx, "the reload", |cx| {
            cx.update(|cx| page.read(cx).is_idle())
        });
    };
    let failures = |cx: &mut TestAppContext| {
        support::toast_labels(cx, s.win)
            .iter()
            .filter(|t| t.starts_with("Failed to load HUDs"))
            .count()
    };
    support::wait_real(cx, "the first load", |cx| {
        cx.update(|cx| page.read(cx).is_idle())
    });
    support::rename_table(&env, "library_huds", "library_huds_away");
    reload(cx);
    reload(cx);
    assert_eq!(failures(cx), 1);
    let shown = support::toast_labels(cx, s.win);
    assert!(
        shown.iter().any(|t| t
            == "Failed to load HUDs. TF2Tools couldn't open your library. Restart TF2Tools and try again."),
        "{shown:?}"
    );
    // A good load re-arms the toast. Clear the shown toast first so the count does not
    // depend on the kit's 5 s autohide.
    cx.update_window(s.win, |_, window, cx| {
        tf2tools_app::shell::toast::clear_toasts(window, cx)
    })
    .unwrap();
    // The kit animates the close on the fake clock; one second of ticks covers the exit and
    // stays far inside the 5 s autohide, so only the clear can empty the stack.
    for _ in 0..20 {
        cx.executor().advance_clock(Duration::from_millis(50));
        if failures(cx) == 0 {
            break;
        }
    }
    assert_eq!(failures(cx), 0, "the clear left the toast up");
    support::rename_table(&env, "library_huds_away", "library_huds");
    reload(cx);
    support::rename_table(&env, "library_huds", "library_huds_away");
    reload(cx);
    assert_eq!(failures(cx), 1);
}

#[gpui_kit::test]
fn an_install_that_ends_after_leaving_still_announces_the_change(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    seed(&s.services, &env, "budhud", Provenance::Local);
    let page = revisit(cx, &s);
    let (count, _sub) = support::library_changes(cx);
    // Start the install and leave in one update, so the page is gone when the task resumes.
    cx.update_window(s.win, |_, window, cx| {
        window.click(ids::hud_install("budhud"), cx);
        s.shell
            .update(cx, |shell, cx| shell.navigate(Page::Settings, cx));
    })
    .unwrap();
    drop(page);
    support::wait_real(cx, "the install toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    assert!(cx.update(|cx| s.shell.read(cx).huds_page()).is_none());
    assert_eq!(count.get(), 1);
}

#[gpui_kit::test]
fn an_install_logs_the_folder_it_lands_in(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    seed(&s.services, &env, "m0rehud-abc1234", Provenance::Local);
    std::fs::write(
        env.data
            .library_dir(LibraryKind::Huds)
            .join("m0rehud-abc1234/resource/ui/extra.res"),
        "#base \"../../custom/m0rehud/resource/ui/base.res\"\n",
    )
    .unwrap();
    let _page = revisit(cx, &s);
    support::click_id(cx, s.win, ids::hud_install("m0rehud-abc1234"));
    support::wait_real(cx, "the install row", |_| {
        support::log_rows(&s.services)
            .iter()
            .any(|r| r.action() == "install")
    });
    let rows = support::log_rows(&s.services);
    let install = rows.iter().find(|r| r.action() == "install").unwrap();
    assert_eq!(install.message(), "Installed HUD: m0rehud");
    assert_eq!(
        install.details().map(str::to_owned),
        Some(format!(
            "Installed to: {}",
            env.game.join("tf").join("custom").join("m0rehud").display()
        ))
    );
    assert!(env.game.join("tf/custom/m0rehud/info.vdf").is_file());
    revisit(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUDS_INSTALLED_BADGE).as_deref(),
        Some("Managed")
    );
}

#[gpui_kit::test]
fn an_install_blocked_by_a_players_folder_says_so(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    seed(&s.services, &env, "m0rehud-abc1234", Provenance::Local);
    std::fs::write(
        env.data
            .library_dir(LibraryKind::Huds)
            .join("m0rehud-abc1234/resource/ui/extra.res"),
        "#base \"../../custom/m0rehud/resource/ui/base.res\"\n",
    )
    .unwrap();
    let mine = env.game.join("tf/custom/m0rehud");
    std::fs::create_dir_all(&mine).unwrap();
    std::fs::write(mine.join("notes.txt"), "mine").unwrap();
    let _page = revisit(cx, &s);
    support::click_id(cx, s.win, ids::hud_install("m0rehud-abc1234"));
    support::wait_real(cx, "the install toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    let warning = "m0rehud-abc1234 installed. A folder named m0rehud is already in your \
                   custom folder, so parts of this HUD may not show. Move or rename that \
                   folder and install again.";
    assert_eq!(support::toast_labels(cx, s.win), [warning]);
    assert!(
        env.game
            .join("tf/custom/m0rehud-abc1234/info.vdf")
            .is_file()
    );
    assert_eq!(std::fs::read(mine.join("notes.txt")).unwrap(), b"mine");
}

// At the 800x600 minimum with the sidebar open, the grid keeps room for a row of 136 px local
// cards (the page scrolls to show it whole) and Add HUD keeps the page's margin.
#[gpui_kit::test]
fn at_800x600_the_grid_has_room_and_add_hud_keeps_the_margin(cx: &mut TestAppContext) {
    use gpui_kit::{px, size};
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    seed(&s.services, &env, "budhud", Provenance::Local);
    cx.simulate_window_resize(s.win, size(px(800.), px(600.)));
    let _page = revisit(cx, &s);
    let bounds = |cx: &mut TestAppContext, id: gpui_kit::ElementId| {
        support::bounds_of(cx, s.win, id.clone()).unwrap_or_else(|| panic!("no {id:?}"))
    };
    let page = bounds(cx, ids::page(Page::Huds));
    let library = bounds(cx, ids::HUDS_LIBRARY.into());
    assert!(
        library.size.height >= px(136.),
        "the grid is a {library:?} strip"
    );
    let (add, search) = (
        bounds(cx, ids::HUDS_ADD.into()),
        bounds(cx, ids::HUDS_SEARCH.into()),
    );
    let margin = tf2tools_app::shell::CONTENT_PADDING;
    assert!(
        add.right() <= page.right() - margin + px(1.),
        "{add:?} past the margin of {page:?}"
    );
    assert!(add.left() >= search.right(), "{add:?} over {search:?}");
    // Scrolling the page to its end shows the whole grid.
    cx.update_window(s.win, |_, w, cx| {
        let wheel = gpui_kit::ScrollWheelEvent {
            position: gpui_kit::point(page.center().x, page.top() + px(80.)),
            delta: gpui_kit::ScrollDelta::Lines(gpui_kit::point(0., -40.)),
            modifiers: gpui_kit::Modifiers::default(),
            touch_phase: gpui_kit::TouchPhase::Moved,
        };
        let _ = w.dispatch_event(gpui_kit::InputEvent::to_platform_input(wheel), cx);
    })
    .unwrap();
    support::settle(cx, s.win);
    let library = bounds(cx, ids::HUDS_LIBRARY.into());
    assert!(
        library.bottom() <= page.bottom() + px(1.),
        "{library:?} still runs past {page:?}"
    );
}
