use crate::support;

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, px};
use tf2tools_app::ids;
use tf2tools_app::library_events::{LibraryChanged, LibraryEvents};
use tf2tools_app::pages::addons::AddonsPage;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_app::widgets::card_grid::{GRID_GAP, SHELL_CHROME_WIDTH, columns_for};
use tf2tools_core::{AddonName, LogEntry, Tf2Install};

const FOOT: &str = "mastercomfig-addon-no-footsteps.vpk";
const PYRO: &str = "mastercomfig-addon-no-pyroland.vpk";

fn found(env: &support::Env) -> Tf2State {
    Tf2State::Found(Tf2Install::from_path(&env.game).unwrap())
}

fn name(asset: &str) -> AddonName {
    AddonName::new(asset).unwrap()
}

fn vpk() -> Vec<u8> {
    support::vpk_bytes(&[("cfg/addons/x.cfg", b"x".as_slice())])
}

/// Opens the Addons page from the sidebar and waits for its first load.
fn open_addons(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<AddonsPage> {
    support::click_id(cx, s.win, ids::nav(Page::Addons));
    let page = cx
        .update(|cx| s.shell.read(cx).addons_page())
        .expect("an Addons page");
    support::wait_real(cx, "Addons to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

fn y(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    id: gpui_kit::ElementId,
) -> gpui_kit::Pixels {
    support::bounds_of(cx, s.win, id).expect("drawn").origin.y
}

#[gpui_kit::test]
fn addons_is_a_real_page(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    assert!(!Page::Addons.is_placeholder());
    let _page = open_addons(cx, &s);
    assert!(support::present(cx, s.win, ids::page(Page::Addons)));
    assert!(!support::present(cx, s.win, ids::PLACEHOLDER_MESSAGE));
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    assert!(cx.update(|cx| s.shell.read(cx).addons_page()).is_none());
}

#[gpui_kit::test]
fn an_empty_library_shows_v5_s_empty_state(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_addons(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::ADDONS_COUNT).as_deref(),
        Some("No addons in library")
    );
    assert!(support::present(cx, s.win, ids::ADDONS_EMPTY));
    assert!(support::present(cx, s.win, ids::ADDONS_EMPTY_BROWSE));
    assert!(!support::present(cx, s.win, ids::ADDONS_INSTALLED));
    assert!(!support::present(cx, s.win, ids::ADDONS_NO_TF2));
}

#[gpui_kit::test]
fn installed_and_library_addons_go_to_their_sections(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_library_addon(&s.services, &env, FOOT, "9.100.1", &vpk(), true);
    support::seed_library_addon(&s.services, &env, PYRO, "9.0.0", &vpk(), false);
    let page = open_addons(cx, &s);
    assert_eq!(cx.update(|cx| page.read(cx).addons().len()), 2);
    assert_eq!(
        support::label_of(cx, s.win, ids::ADDONS_COUNT).as_deref(),
        Some("2 in library, 1 installed")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::addon_card(&name(FOOT))).as_deref(),
        Some("No Footsteps")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::addon_tag(&name(FOOT))).as_deref(),
        Some("9.100.1")
    );
    assert!(support::present(cx, s.win, ids::addon_check(&name(FOOT))));
    assert!(!support::present(cx, s.win, ids::addon_check(&name(PYRO))));
    assert!(support::present(
        cx,
        s.win,
        ids::addon_uninstall(&name(FOOT))
    ));
    assert!(support::present(cx, s.win, ids::addon_install(&name(PYRO))));
    assert!(support::present(cx, s.win, ids::addon_delete(&name(PYRO))));
    assert!(!support::present(cx, s.win, ids::ADDONS_INSTALLED_EMPTY));
    let installed = y(cx, &s, ids::ADDONS_INSTALLED.into());
    let foot = y(cx, &s, ids::addon_card(&name(FOOT)));
    let library = y(cx, &s, ids::ADDONS_LIBRARY.into());
    let pyro = y(cx, &s, ids::addon_card(&name(PYRO)));
    assert!(installed < foot && foot < library && library < pyro);
}

#[gpui_kit::test]
fn every_card_keeps_the_column_width_however_full_its_row(cx: &mut TestAppContext) {
    const SOUND: &str = "mastercomfig-addon-no-soundscapes.vpk";
    const TUTORIAL: &str = "mastercomfig-addon-no-tutorial.vpk";
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    // A lone installed card, and a library row filled to v5's three columns at 1200 px.
    support::seed_library_addon(&s.services, &env, FOOT, "9.100.1", &vpk(), true);
    for asset in [PYRO, SOUND, TUTORIAL] {
        support::seed_library_addon(&s.services, &env, asset, "9.0.0", &vpk(), false);
    }
    let _page = open_addons(cx, &s);
    let content = px(1200.) - SHELL_CHROME_WIDTH;
    let columns = columns_for(content);
    assert_eq!(columns, 3);
    let column = (content - GRID_GAP * (columns - 1) as f32) / columns as f32;
    let bounds = |cx: &mut TestAppContext, asset: &str| {
        support::bounds_of(cx, s.win, ids::addon_card(&name(asset))).expect("drawn")
    };
    let row = bounds(cx, PYRO).origin.y;
    for asset in [FOOT, PYRO, SOUND, TUTORIAL] {
        let card = bounds(cx, asset);
        if asset != FOOT {
            assert_eq!(card.origin.y, row, "{asset} shares the full library row");
        }
        assert!(
            (card.size.width - column).abs() < px(0.5),
            "{asset} is {:?} wide, the column is {column:?}",
            card.size.width
        );
    }
}

#[gpui_kit::test]
fn with_nothing_installed_the_installed_section_says_so(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_library_addon(&s.services, &env, PYRO, "9.0.0", &vpk(), false);
    let _page = open_addons(cx, &s);
    assert!(support::present(cx, s.win, ids::ADDONS_INSTALLED_EMPTY));
    assert!(support::present(cx, s.win, ids::ADDONS_LIBRARY));
}

#[gpui_kit::test]
fn without_tf2_the_page_warns_and_lists_everything_as_library(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    support::seed_library_addon(&s.services, &env, FOOT, "9.100.1", &vpk(), true);
    // A stray VPK in tf/custom is not adopted without a TF2 install.
    std::fs::write(env.game.join("tf/custom").join(PYRO), vpk()).unwrap();
    let _page = open_addons(cx, &s);
    assert!(support::present(cx, s.win, ids::ADDONS_NO_TF2));
    assert_eq!(
        support::label_of(cx, s.win, ids::ADDONS_COUNT).as_deref(),
        Some("1 in library, 0 installed")
    );
    assert!(support::present(cx, s.win, ids::ADDONS_INSTALLED_EMPTY));
    assert!(support::present(cx, s.win, ids::addon_card(&name(FOOT))));
    assert!(!support::present(cx, s.win, ids::addon_card(&name(PYRO))));
}

#[gpui_kit::test]
fn the_first_load_adopts_addons_found_in_tf_custom(cx: &mut TestAppContext) {
    let env = support::env();
    std::fs::write(env.game.join("tf/custom").join(FOOT), vpk()).unwrap();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_addons(cx, &s);
    assert!(support::present(cx, s.win, ids::addon_check(&name(FOOT))));
    assert_eq!(s.services.library.list_addons(None).unwrap().len(), 1);
}

#[gpui_kit::test]
fn browse_opens_the_addon_store(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_addons(cx, &s);
    support::click_id(cx, s.win, ids::ADDONS_BROWSE);
    assert_eq!(cx.update(|cx| s.shell.read(cx).page()), Page::AddonStore);
    let _page = open_addons(cx, &s);
    support::click_id(cx, s.win, ids::ADDONS_EMPTY_BROWSE);
    assert_eq!(cx.update(|cx| s.shell.read(cx).page()), Page::AddonStore);
}

#[gpui_kit::test]
fn a_library_change_reloads_the_page(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_addons(cx, &s);
    assert!(support::present(cx, s.win, ids::ADDONS_EMPTY));
    support::seed_library_addon(&s.services, &env, PYRO, "9.0.0", &vpk(), false);
    cx.update(LibraryEvents::changed);
    support::wait_real(cx, "the reload", |cx| {
        support::present(cx, s.win, ids::addon_card(&name(PYRO)))
    });
}

// Core adopts only into an empty library, so the library is left empty: without the page's
// own first-load guard, the reload would adopt the VPK still in tf/custom again.
#[gpui_kit::test]
fn adoption_runs_only_on_the_first_load(cx: &mut TestAppContext) {
    let env = support::env();
    std::fs::write(env.game.join("tf/custom").join(FOOT), vpk()).unwrap();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_addons(cx, &s);
    assert!(support::present(cx, s.win, ids::addon_card(&name(FOOT))));
    rusqlite::Connection::open(env.data.db_path())
        .unwrap()
        .execute("DELETE FROM library_addons", [])
        .unwrap();
    cx.update(LibraryEvents::changed);
    support::wait_real(cx, "the reload to show an empty library", |cx| {
        support::present(cx, s.win, ids::ADDONS_EMPTY)
    });
    assert!(!support::present(cx, s.win, ids::addon_card(&name(FOOT))));
    assert!(s.services.library.list_addons(None).unwrap().is_empty());
}

fn row(e: &LogEntry) -> (&str, &str, &str) {
    (e.category(), e.action(), e.message())
}

fn toast_is(cx: &mut TestAppContext, s: &support::ShellWindow, text: &str) {
    support::wait_real(cx, text, |cx| {
        support::label_of(cx, s.win, ids::TOAST).as_deref() == Some(text)
    });
}

#[gpui_kit::test]
fn install_copies_the_addon_writes_a_row_and_emits(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_library_addon(&s.services, &env, PYRO, "9.100.1", &vpk(), false);
    let _page = open_addons(cx, &s);
    let count = Rc::new(Cell::new(0));
    let seen = count.clone();
    let _sub = cx.update(|cx| {
        let events = LibraryEvents::global(cx);
        cx.subscribe(&events, move |_, _: &LibraryChanged, _| {
            seen.set(seen.get() + 1)
        })
    });
    support::click_id(cx, s.win, ids::addon_install(&name(PYRO)));
    toast_is(cx, &s, "Addon installed. No Pyroland is now active");
    assert!(env.game.join("tf/custom").join(PYRO).is_file());
    assert_eq!(count.get(), 1);
    support::wait_real(cx, "the reload", |cx| {
        support::present(cx, s.win, ids::addon_check(&name(PYRO)))
    });
    let rows = support::log_rows(&s.services);
    assert_eq!(rows.len(), 1);
    assert_eq!(
        row(&rows[0]),
        ("addon_store", "install", "Installed addon: No Pyroland")
    );
}

#[gpui_kit::test]
fn uninstall_removes_it_and_writes_a_row(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_library_addon(&s.services, &env, FOOT, "9.100.1", &vpk(), true);
    let _page = open_addons(cx, &s);
    support::click_id(cx, s.win, ids::addon_uninstall(&name(FOOT)));
    toast_is(cx, &s, "Addon uninstalled. No Footsteps has been removed");
    assert!(!env.game.join("tf/custom").join(FOOT).exists());
    support::wait_real(cx, "the reload", |cx| {
        support::present(cx, s.win, ids::addon_install(&name(FOOT)))
    });
    let rows = support::log_rows(&s.services);
    assert_eq!(
        row(&rows[0]),
        (
            "addon_store",
            "uninstall",
            "Uninstalled addon: No Footsteps"
        )
    );
}

#[gpui_kit::test]
fn delete_asks_then_removes_and_writes_a_row(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_library_addon(&s.services, &env, FOOT, "9.100.1", &vpk(), true);
    let _page = open_addons(cx, &s);
    support::click_id(cx, s.win, ids::addon_delete(&name(FOOT)));
    let asked = support::label_of(cx, s.win, ids::CONFIRM_DESCRIPTION).unwrap();
    assert_eq!(
        asked,
        "Are you sure you want to remove this addon from your library? If installed, it will also be removed from TF2."
    );
    support::assert_plain(&asked);
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    toast_is(cx, &s, "Addon removed. No Footsteps removed from library");
    assert!(s.services.library.list_addons(None).unwrap().is_empty());
    assert!(!env.game.join("tf/custom").join(FOOT).exists());
    support::wait_real(cx, "the reload", |cx| {
        support::present(cx, s.win, ids::ADDONS_EMPTY)
    });
    let rows = support::log_rows(&s.services);
    assert_eq!(
        row(&rows[0]),
        (
            "addon",
            "delete",
            "Deleted addon from library: No Footsteps"
        )
    );
}

#[gpui_kit::test]
fn cancelling_delete_keeps_the_addon(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_library_addon(&s.services, &env, FOOT, "9.100.1", &vpk(), true);
    let _page = open_addons(cx, &s);
    support::click_id(cx, s.win, ids::addon_delete(&name(FOOT)));
    support::click_id(cx, s.win, ids::CONFIRM_CANCEL);
    assert_eq!(s.services.library.list_addons(None).unwrap().len(), 1);
    assert!(env.game.join("tf/custom").join(FOOT).is_file());
    assert!(support::log_rows(&s.services).is_empty());
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_failed_install_toasts_and_writes_no_row(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt as _;
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_library_addon(&s.services, &env, PYRO, "9.100.1", &vpk(), false);
    let page = open_addons(cx, &s);
    let (count, _sub) = support::library_changes(cx);
    let custom = env.game.join("tf/custom");
    std::fs::set_permissions(&custom, std::fs::Permissions::from_mode(0o555)).unwrap();
    // Root ignores the mode, so the install could not fail; nothing to test then.
    if std::fs::write(custom.join("probe"), b"").is_ok() {
        std::fs::set_permissions(&custom, std::fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }
    support::click_id(cx, s.win, ids::addon_install(&name(PYRO)));
    support::wait_real(cx, "the failure toast", |cx| {
        support::label_of(cx, s.win, ids::TOAST)
            .is_some_and(|t| t.starts_with("Failed to install addon"))
    });
    let toast = support::label_of(cx, s.win, ids::TOAST).unwrap();
    support::assert_plain(&toast);
    std::fs::set_permissions(&custom, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(support::log_rows(&s.services).is_empty());
    support::wait_real(cx, "idle", |cx| {
        cx.update(|cx| !page.read(cx).is_any_busy(cx))
    });
    assert_eq!(count.get(), 0);
}

#[gpui_kit::test]
fn without_tf2_install_rests_but_delete_works(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    support::seed_library_addon(&s.services, &env, PYRO, "9.100.1", &vpk(), false);
    let _page = open_addons(cx, &s);
    support::click_id(cx, s.win, ids::addon_install(&name(PYRO)));
    support::settle(cx, s.win);
    assert!(!support::present(cx, s.win, ids::TOAST));
    assert!(!env.game.join("tf/custom").join(PYRO).exists());
    support::click_id(cx, s.win, ids::addon_delete(&name(PYRO)));
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    toast_is(cx, &s, "Addon removed. No Pyroland removed from library");
    assert!(s.services.library.list_addons(None).unwrap().is_empty());
}

#[gpui_kit::test]
fn the_page_s_own_install_shows_one_installing_footer(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_library_addon(&s.services, &env, PYRO, "9.100.1", &vpk(), false);
    let page = open_addons(cx, &s);
    let pyro = name(PYRO);
    // Click and draw before the executor runs the install job, so the busy card is on screen.
    let (busy, install, delete) = cx
        .update_window(s.win, |_, w, cx| {
            w.click(ids::addon_install(&pyro), cx);
            w.render_frame(cx);
            w.simulate_next_frame(cx);
            let busy = w
                .try_find(ids::addon_busy(&pyro))
                .and_then(|e| e.label().map(str::to_owned));
            let install = w.try_find(ids::addon_install(&pyro)).is_some();
            let delete = w.try_find(ids::addon_delete(&pyro)).is_some();
            (busy, install, delete)
        })
        .unwrap();
    assert_eq!(busy.as_deref(), Some("Installing..."));
    assert!(
        !install && !delete,
        "the busy footer replaces the card's actions"
    );
    assert!(cx.update(|cx| page.read(cx).is_any_busy(cx)));
    support::wait_real(cx, "the install", |cx| {
        support::present(cx, s.win, ids::addon_check(&pyro))
    });
    assert!(!support::present(cx, s.win, ids::addon_busy(&pyro)));
}

#[gpui_kit::test]
fn the_addon_buttons_are_v5_s_32_px(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_library_addon(&s.services, &env, FOOT, "9.100.1", &vpk(), true);
    support::seed_library_addon(&s.services, &env, PYRO, "9.0.0", &vpk(), false);
    let _page = open_addons(cx, &s);
    let targets: [gpui_kit::ElementId; 4] = [
        ids::ADDONS_BROWSE.into(),
        ids::addon_uninstall(&name(FOOT)),
        ids::addon_install(&name(PYRO)),
        ids::addon_delete(&name(PYRO)),
    ];
    for id in targets {
        let b = support::bounds_of(cx, s.win, id.clone()).expect("drawn");
        assert_eq!(b.size.height, px(32.), "{id:?}");
    }
}

#[gpui_kit::test]
fn the_release_tag_is_a_v5_badge(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_library_addon(&s.services, &env, PYRO, "9.0.0", &vpk(), false);
    let _page = open_addons(cx, &s);
    let tag = support::bounds_of(cx, s.win, ids::addon_tag(&name(PYRO))).unwrap();
    // shadcn's Badge, as tests/badge.rs pins the TF2 badge: 21 px with text-xs.
    assert_eq!(tag.size.height, px(21.));
}

// v5's AddonCard check is `CheckCircle class="size-4"`.
#[gpui_kit::test]
fn the_in_library_check_is_v5_s_16_px(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_library_addon(&s.services, &env, FOOT, "9.100.1", &vpk(), true);
    let _page = open_addons(cx, &s);
    let check = support::bounds_of(cx, s.win, ids::addon_check(&name(FOOT))).expect("installed");
    assert_eq!((check.size.width, check.size.height), (px(16.), px(16.)));
}

#[gpui_kit::test]
fn an_addon_card_has_v5_s_card_spacing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_library_addon(&s.services, &env, PYRO, "9.0.0", &vpk(), false);
    let _page = open_addons(cx, &s);
    let card = support::bounds_of(cx, s.win, ids::addon_card(&name(PYRO))).unwrap();
    let tag = support::bounds_of(cx, s.win, ids::addon_tag(&name(PYRO))).unwrap();
    let desc = support::bounds_of(cx, s.win, ids::addon_description(&name(PYRO))).unwrap();
    let install = support::bounds_of(cx, s.win, ids::addon_install(&name(PYRO))).unwrap();
    // The 1 px border plus py-6.
    assert_eq!(tag.origin.y - card.origin.y, px(25.));
    assert_eq!(
        card.origin.y + card.size.height - (install.origin.y + install.size.height),
        px(25.)
    );
    // Header pb-2 then gap-6; content pb-2 then gap-6.
    assert_eq!(desc.origin.y - (tag.origin.y + tag.size.height), px(32.));
    assert_eq!(
        install.origin.y - (desc.origin.y + desc.size.height),
        px(32.)
    );
}

#[gpui_kit::test]
fn an_install_that_ends_after_leaving_still_announces_the_change(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_library_addon(&s.services, &env, PYRO, "9.100.1", &vpk(), false);
    let page = open_addons(cx, &s);
    let (count, _sub) = support::library_changes(cx);
    // Start the install and leave in one update, so the page is gone when the task resumes.
    cx.update_window(s.win, |_, window, cx| {
        window.click(ids::addon_install(&name(PYRO)), cx);
        s.shell
            .update(cx, |shell, cx| shell.navigate(Page::Huds, cx));
    })
    .unwrap();
    drop(page);
    support::wait_real(cx, "the install toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    assert!(cx.update(|cx| s.shell.read(cx).addons_page()).is_none());
    assert!(env.game.join("tf/custom").join(PYRO).is_file());
    assert_eq!(count.get(), 1);
}
