use crate::support;

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, px};
use tf2tools_app::ids;
use tf2tools_app::library_events::{LibraryChanged, LibraryEvents};
use tf2tools_app::pages::addons::AddonsPage;
use tf2tools_app::pages::profiles::probe::Tf2Probe;
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
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    let _page = open_addons(cx, &s);
    let count = Rc::new(Cell::new(0));
    let seen = Rc::clone(&count);
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
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
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
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
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
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
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

const BASE: &str = "tf/custom/mastercomfig-base.vpk";

fn shell_with(
    cx: &mut TestAppContext,
    env: &support::Env,
    server: &support::StoreServer,
) -> support::ShellWindow {
    cx.executor().allow_parking();
    let endpoints = server.endpoints();
    support::shell_window_tweaked(cx, env, found(env), |services| {
        *services = services.clone().with_endpoints(endpoints);
    })
}

#[gpui_kit::test]
fn an_install_without_mastercomfig_asks_then_installs_both(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes(
        "/gh/mastercomfig/mastercomfig-base.vpk",
        support::mastercomfig_base("9.100.1"),
    );
    let s = shell_with(cx, &env, &server);
    support::seed_mastercomfig_release(&env, &server.uri(), "9.100.1");
    support::seed_library_addon(&s.services, &env, PYRO, "9.100.1", &vpk(), false);
    let _page = open_addons(cx, &s);
    support::click_id(cx, s.win, ids::addon_install(&name(PYRO)));
    support::wait_real(cx, "the question", |cx| {
        support::present(cx, s.win, ids::CONFIRM_DIALOG)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::CONFIRM_DESCRIPTION).as_deref(),
        Some("This addon needs mastercomfig. Install both?")
    );
    assert!(!env.game.join(BASE).exists(), "nothing before the answer");
    assert!(!env.game.join("tf/custom").join(PYRO).exists());
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    toast_is(cx, &s, "Addon installed. No Pyroland is now active");
    assert_eq!(
        std::fs::read(env.game.join(BASE)).unwrap(),
        support::mastercomfig_base("9.100.1")
    );
    assert!(env.game.join("tf/custom").join(PYRO).is_file());
}

#[gpui_kit::test]
fn cancelling_the_question_installs_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_library_addon(&s.services, &env, PYRO, "9.100.1", &vpk(), false);
    let page = open_addons(cx, &s);
    support::click_id(cx, s.win, ids::addon_install(&name(PYRO)));
    support::wait_real(cx, "the question", |cx| {
        support::present(cx, s.win, ids::CONFIRM_DIALOG)
    });
    support::click_id(cx, s.win, ids::CONFIRM_CANCEL);
    support::wait_real(cx, "idle", |cx| {
        cx.update(|cx| !page.read(cx).is_any_busy(cx))
    });
    assert!(!env.game.join(BASE).exists());
    assert!(!env.game.join("tf/custom").join(PYRO).exists());
    assert!(support::log_rows(&s.services).is_empty());
}

#[gpui_kit::test]
fn the_page_stays_busy_while_the_question_is_open(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_library_addon(&s.services, &env, PYRO, "9.100.1", &vpk(), false);
    let page = open_addons(cx, &s);
    support::click_id(cx, s.win, ids::addon_install(&name(PYRO)));
    support::wait_real(cx, "the question", |cx| {
        support::present(cx, s.win, ids::CONFIRM_DIALOG)
    });
    assert!(cx.update(|cx| page.read(cx).is_any_busy(cx)));
}

#[gpui_kit::test]
fn an_install_left_before_the_check_asks_nothing_and_installs_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_library_addon(&s.services, &env, PYRO, "9.100.1", &vpk(), false);
    let page = open_addons(cx, &s);
    let (count, _sub) = support::library_changes(cx);
    // Start the install and leave in one update, so the page is gone when the check lands.
    cx.update_window(s.win, |_, window, cx| {
        window.click(ids::addon_install(&name(PYRO)), cx);
        s.shell
            .update(cx, |shell, cx| shell.navigate(Page::Huds, cx));
    })
    .unwrap();
    drop(page);
    support::settle(cx, s.win);
    assert!(cx.update(|cx| s.shell.read(cx).addons_page()).is_none());
    assert!(!support::present(cx, s.win, ids::CONFIRM_DIALOG));
    assert!(!support::present(cx, s.win, ids::TOAST));
    assert!(!env.game.join(BASE).exists());
    assert!(!env.game.join("tf/custom").join(PYRO).exists());
    assert_eq!(count.get(), 0);
}

#[gpui_kit::test]
fn installing_both_refuses_while_tf2_runs(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes(
        "/gh/mastercomfig/mastercomfig-base.vpk",
        support::mastercomfig_base("9.100.1"),
    );
    let s = shell_with(cx, &env, &server);
    support::seed_mastercomfig_release(&env, &server.uri(), "9.100.1");
    support::seed_library_addon(&s.services, &env, PYRO, "9.100.1", &vpk(), false);
    cx.update(|cx| Tf2Probe::install(cx, || true));
    let page = open_addons(cx, &s);
    support::click_id(cx, s.win, ids::addon_install(&name(PYRO)));
    support::wait_real(cx, "the question", |cx| {
        support::present(cx, s.win, ids::CONFIRM_DIALOG)
    });
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    support::wait_real(cx, "the refusal", |cx| {
        support::toast_labels(cx, s.win)
            .iter()
            .any(|t| t.contains("Close TF2 first"))
    });
    support::wait_real(cx, "idle", |cx| {
        cx.update(|cx| !page.read(cx).is_any_busy(cx))
    });
    assert!(!env.game.join(BASE).exists());
    assert!(!env.game.join("tf/custom").join(PYRO).exists());
}

#[gpui_kit::test]
fn install_both_left_during_the_base_download_still_copies_the_addon(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes(
        "/gh/mastercomfig/mastercomfig-base.vpk",
        support::mastercomfig_base("9.100.1"),
    );
    let s = shell_with(cx, &env, &server);
    support::seed_mastercomfig_release(&env, &server.uri(), "9.100.1");
    support::seed_library_addon(&s.services, &env, PYRO, "9.100.1", &vpk(), false);
    let page = open_addons(cx, &s);
    let (count, _sub) = support::library_changes(cx);
    support::click_id(cx, s.win, ids::addon_install(&name(PYRO)));
    support::wait_real(cx, "the question", |cx| {
        support::present(cx, s.win, ids::CONFIRM_DIALOG)
    });
    // Answer and leave in one update, so the page is gone while the base downloads.
    cx.update_window(s.win, |_, window, cx| {
        window.click(ids::CONFIRM_OK, cx);
        s.shell
            .update(cx, |shell, cx| shell.navigate(Page::Huds, cx));
    })
    .unwrap();
    drop(page);
    toast_is(cx, &s, "Addon installed. No Pyroland is now active");
    assert!(cx.update(|cx| s.shell.read(cx).addons_page()).is_none());
    assert!(env.game.join(BASE).is_file());
    assert!(env.game.join("tf/custom").join(PYRO).is_file());
    assert_eq!(count.get(), 1);
}

fn store_shell(
    cx: &mut TestAppContext,
    env: &support::Env,
    server: &support::StoreServer,
) -> support::ShellWindow {
    let endpoints = server.endpoints();
    let s = support::shell_window_tweaked(cx, env, found(env), move |services| {
        *services = services.clone().with_endpoints(endpoints);
    });
    support::store_ready(cx, server);
    cx.update(|cx| Tf2Probe::install(cx, || false));
    s
}

fn gb_vpk() -> Vec<u8> {
    support::vpk_bytes(&[("materials/a.vmt", b"v")])
}

/// Installs add-on `id` through the flow and waits for it to finish.
fn install_gb(cx: &mut TestAppContext, s: &support::ShellWindow, id: u64, section: &str) {
    let rt = cx.update(|cx| tf2tools_app::tokio_bridge::handle(cx));
    let profile = rt
        .block_on(
            s.services
                .store
                .addon_catalog()
                .profile(tf2tools_core::GbId::new(id)),
        )
        .unwrap();
    let installs = cx.update(tf2tools_app::gb_addon_installs::GbAddonInstalls::global);
    let win = s.win;
    let section = tf2tools_core::AddonSection::by_key(section).unwrap();
    let started = cx.update(|cx| installs.update(cx, |i, cx| i.install(section, profile, win, cx)));
    assert!(started, "the install starts");
    support::wait_real(cx, "the install", |cx| {
        cx.update(|cx| !installs.read(cx).is_busy())
    });
}

#[gpui_kit::test]
fn installed_gamebanana_addons_list_by_section(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(1, "Skin One", &[("a.vpk", gb_vpk(), 100)]);
    server.mount_gb_addon(2, "Effect Two", &[("b.vpk", gb_vpk(), 100)]);
    let s = store_shell(cx, &env, &server);
    install_gb(cx, &s, 1, "skins");
    install_gb(cx, &s, 2, "effects");
    let _page = open_addons(cx, &s);
    assert!(support::present(cx, s.win, ids::ADDONS_GB));
    assert!(support::present(cx, s.win, ids::addons_gb_group("skins")));
    assert!(support::present(cx, s.win, ids::addons_gb_group("effects")));
    let one = tf2tools_core::GbId::new(1);
    assert!(support::present(cx, s.win, ids::addons_gb_remove(one)));
    assert!(support::present(cx, s.win, ids::addons_gb_open(one)));
    assert!(!support::present(cx, s.win, ids::addons_gb_update(one)));
    assert!(!support::present(cx, s.win, ids::addons_gb_tag(one)));
}

#[gpui_kit::test]
fn gamebanana_addons_alone_do_not_show_the_empty_library(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(3, "Only", &[("a.vpk", gb_vpk(), 100)]);
    let s = store_shell(cx, &env, &server);
    install_gb(cx, &s, 3, "skins");
    let _page = open_addons(cx, &s);
    assert!(!support::present(cx, s.win, ids::ADDONS_EMPTY));
    assert!(support::present(
        cx,
        s.win,
        ids::addons_gb_card(tf2tools_core::GbId::new(3))
    ));
}

#[gpui_kit::test]
fn remove_asks_then_deletes_the_files(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(4, "Gone Soon", &[("a.vpk", gb_vpk(), 100)]);
    let s = store_shell(cx, &env, &server);
    install_gb(cx, &s, 4, "skins");
    let _page = open_addons(cx, &s);
    let id = tf2tools_core::GbId::new(4);
    support::click_id(cx, s.win, ids::addons_gb_remove(id));
    let asked = support::label_of(cx, s.win, ids::CONFIRM_DESCRIPTION).unwrap();
    assert_eq!(
        asked,
        "Are you sure you want to remove Gone Soon? It will be removed from TF2 and from your library."
    );
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    crate::gb_addon_installs::toast_is(cx, s.win, "Add-on removed. Gone Soon");
    assert!(
        !env.game
            .join("tf/custom/tf2tools-gb-4-gone-soon.vpk")
            .exists()
    );
    support::wait_real(cx, "the card gone", |cx| {
        !support::present(cx, s.win, ids::addons_gb_card(id))
    });
}

#[gpui_kit::test]
fn remove_while_tf2_runs_says_close_tf2_first_without_asking(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(40, "Kept", &[("a.vpk", gb_vpk(), 100)]);
    let s = store_shell(cx, &env, &server);
    install_gb(cx, &s, 40, "skins");
    let _page = open_addons(cx, &s);
    cx.update(|cx| Tf2Probe::install(cx, || true));
    let id = tf2tools_core::GbId::new(40);
    support::click_id(cx, s.win, ids::addons_gb_remove(id));
    crate::gb_addon_installs::toast_is(
        cx,
        s.win,
        "Close TF2 first. TF2 is running. Close the game, then try again.",
    );
    assert!(!support::present(cx, s.win, ids::CONFIRM_DESCRIPTION));
    assert!(env.game.join("tf/custom/tf2tools-gb-40-kept.vpk").is_file());
}

#[gpui_kit::test]
fn a_hand_deleted_addon_offers_install_again(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(5, "Back", &[("a.vpk", gb_vpk(), 100)]);
    let s = store_shell(cx, &env, &server);
    install_gb(cx, &s, 5, "skins");
    let _page = open_addons(cx, &s);
    let id = tf2tools_core::GbId::new(5);
    assert!(!support::present(cx, s.win, ids::addons_gb_tag(id)));
    // Leave, delete by hand, and come back: the page must list it again.
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    std::fs::remove_file(env.game.join("tf/custom/tf2tools-gb-5-back.vpk")).unwrap();
    let _page = open_addons(cx, &s);
    support::wait_real(cx, "the not installed tag", |cx| {
        support::label_of(cx, s.win, ids::addons_gb_tag(id)).as_deref() == Some("Not installed")
    });
    support::click_id(cx, s.win, ids::addons_gb_reinstall(id));
    crate::gb_addon_installs::toast_is(cx, s.win, "Add-on installed. Back");
    assert!(env.game.join("tf/custom/tf2tools-gb-5-back.vpk").is_file());
}

#[gpui_kit::test]
fn install_again_without_a_library_copy_downloads_it(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(14, "Lost Copy", &[("a.vpk", gb_vpk(), 100)]);
    let s = store_shell(cx, &env, &server);
    install_gb(cx, &s, 14, "skins");
    let file = env.game.join("tf/custom/tf2tools-gb-14-lost-copy.vpk");
    std::fs::remove_file(&file).unwrap();
    std::fs::remove_dir_all(env.data.gb_addon_dir().join("14")).unwrap();
    let _page = open_addons(cx, &s);
    let id = tf2tools_core::GbId::new(14);
    support::wait_real(cx, "the not installed tag", |cx| {
        support::label_of(cx, s.win, ids::addons_gb_tag(id)).as_deref() == Some("Not installed")
    });
    support::click_id(cx, s.win, ids::addons_gb_reinstall(id));
    support::wait_real(cx, "the installed card", |cx| {
        !support::present(cx, s.win, ids::addons_gb_tag(id))
    });
    assert!(file.is_file());
    assert!(env.data.gb_addon_dir().join("14").is_dir());
}

#[gpui_kit::test]
fn opening_the_page_checks_for_updates_and_update_installs_the_newer_file(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(6, "Fresh", &[("a.vpk", gb_vpk(), 100)]);
    let s = store_shell(cx, &env, &server);
    install_gb(cx, &s, 6, "skins");
    support::sql(&env)
        .execute("UPDATE library_gb_addons SET checked_at = 0", [])
        .unwrap();
    server.reset();
    server.mount_gb_addon(6, "Fresh", &[("a2.vpk", gb_vpk(), 300)]);
    let _page = open_addons(cx, &s);
    let id = tf2tools_core::GbId::new(6);
    support::wait_real(cx, "the update", |cx| {
        support::present(cx, s.win, ids::addons_gb_update(id))
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::addons_gb_tag(id)).as_deref(),
        Some("Update available")
    );
    support::click_id(cx, s.win, ids::addons_gb_update(id));
    crate::gb_addon_installs::toast_is(cx, s.win, "Add-on updated. Fresh");
    support::wait_real(cx, "the update gone", |cx| {
        !support::present(cx, s.win, ids::addons_gb_update(id))
    });
}

#[gpui_kit::test]
fn an_item_gone_from_gamebanana_keeps_remove(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(7, "Trashed", &[("a.vpk", gb_vpk(), 100)]);
    let s = store_shell(cx, &env, &server);
    install_gb(cx, &s, 7, "skins");
    support::sql(&env)
        .execute("UPDATE library_gb_addons SET checked_at = 0", [])
        .unwrap();
    server.reset();
    server.mount_gb_profile(
        7,
        serde_json::json!({"_idRow": 7, "_sName": "Trashed", "_bIsTrashed": true}),
    );
    let _page = open_addons(cx, &s);
    let id = tf2tools_core::GbId::new(7);
    support::wait_real(cx, "the gone tag", |cx| {
        support::label_of(cx, s.win, ids::addons_gb_tag(id)).as_deref()
            == Some("No longer on GameBanana")
    });
    assert!(support::present(cx, s.win, ids::addons_gb_remove(id)));
    assert!(!support::present(cx, s.win, ids::addons_gb_update(id)));
}

#[gpui_kit::test]
fn a_gone_addon_whose_files_were_deleted_offers_install_again(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(15, "Trashed", &[("a.vpk", gb_vpk(), 100)]);
    let s = store_shell(cx, &env, &server);
    install_gb(cx, &s, 15, "skins");
    support::sql(&env)
        .execute("UPDATE library_gb_addons SET gone = 1", [])
        .unwrap();
    let file = env.game.join("tf/custom/tf2tools-gb-15-trashed.vpk");
    std::fs::remove_file(&file).unwrap();
    let _page = open_addons(cx, &s);
    let id = tf2tools_core::GbId::new(15);
    // The library copy needs nothing from GameBanana, so the store detail's offer holds here.
    support::wait_real(cx, "install again", |cx| {
        support::present(cx, s.win, ids::addons_gb_reinstall(id))
    });
    assert_eq!(
        tf2tools_app::gb_addon_installs::GbAddonOffer::of(env_entry(cx, 15).as_ref()),
        tf2tools_app::gb_addon_installs::GbAddonOffer::InstallAgain
    );
    support::click_id(cx, s.win, ids::addons_gb_reinstall(id));
    support::wait_real(cx, "the files back", |_| file.is_file());
}

fn env_entry(cx: &mut TestAppContext, id: u64) -> Option<tf2tools_core::GbAddonEntry> {
    let installs = cx.update(tf2tools_app::gb_addon_installs::GbAddonInstalls::global);
    cx.update(|cx| {
        installs
            .read(cx)
            .entry(tf2tools_core::GbId::new(id))
            .cloned()
    })
}

#[gpui_kit::test]
fn at_800_by_600_a_gamebanana_card_keeps_its_tag_and_buttons_inside(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(8, "A Rather Long Add-on Name", &[("a.vpk", gb_vpk(), 100)]);
    let s = store_shell(cx, &env, &server);
    install_gb(cx, &s, 8, "skins");
    support::sql(&env)
        .execute("UPDATE library_gb_addons SET latest_file_date = 999", [])
        .unwrap();
    cx.simulate_window_resize(s.win, gpui_kit::size(px(800.), px(600.)));
    let _page = open_addons(cx, &s);
    let id = tf2tools_core::GbId::new(8);
    let card = support::bounds_of(cx, s.win, ids::addons_gb_card(id)).expect("a card");
    for part in [
        ids::addons_gb_tag(id),
        ids::addons_gb_update(id),
        ids::addons_gb_remove(id),
        ids::addons_gb_open(id),
    ] {
        let b = support::bounds_of(cx, s.win, part.clone()).expect("drawn");
        assert!(b.right() <= card.right(), "{part:?} runs past the card");
        assert!(b.size.width > px(0.), "{part:?} is squeezed away");
    }
}

#[gpui_kit::test]
fn a_gamebanana_row_with_an_unknown_section_is_not_counted(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(9, "Known", &[("a.vpk", gb_vpk(), 100)]);
    server.mount_gb_addon(10, "Stray", &[("b.vpk", gb_vpk(), 100)]);
    let s = store_shell(cx, &env, &server);
    install_gb(cx, &s, 9, "skins");
    install_gb(cx, &s, 10, "skins");
    support::sql(&env)
        .execute(
            "UPDATE library_gb_addons SET section = 'bogus' WHERE gb_id = 10",
            [],
        )
        .unwrap();
    let _page = open_addons(cx, &s);
    support::wait_real(cx, "the stray row gone", |cx| {
        !support::present(cx, s.win, ids::addons_gb_card(tf2tools_core::GbId::new(10)))
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::ADDONS_COUNT).as_deref(),
        Some("1 in library, 1 installed")
    );
}

/// The ids of add-on `id`'s Update, Remove and link buttons.
fn gb_buttons(id: u64) -> [gpui_kit::ElementId; 3] {
    let id = tf2tools_core::GbId::new(id);
    [
        ids::addons_gb_update(id),
        ids::addons_gb_remove(id),
        ids::addons_gb_open(id),
    ]
}

/// Installs add-on `id` and records a newer file, so its card offers Update.
fn install_gb_with_update(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    env: &support::Env,
    id: u64,
) {
    install_gb(cx, s, id, "skins");
    support::sql(env)
        .execute("UPDATE library_gb_addons SET latest_file_date = 999", [])
        .unwrap();
}

#[gpui_kit::test]
fn without_tf2_gamebanana_buttons_rest_but_the_link(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(11, "Rests", &[("a.vpk", gb_vpk(), 100)]);
    let s = store_shell(cx, &env, &server);
    install_gb_with_update(cx, &s, &env, 11);
    cx.update(|cx| {
        tf2tools_app::services::tf2_state(cx).update(cx, |state, cx| {
            *state = Tf2State::Missing { saved: None };
            cx.notify();
        })
    });
    let _page = open_addons(cx, &s);
    let [update, remove, open] = gb_buttons(11);
    assert_eq!(
        support::disabled_ids(cx, s.win, &[update.clone(), remove.clone(), open]),
        vec![update, remove]
    );
}

#[gpui_kit::test]
fn while_another_addon_works_gamebanana_buttons_rest_but_the_link(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(12, "Waits", &[("a.vpk", gb_vpk(), 100)]);
    let pack = support::zip_of(&[("A.vpk", &gb_vpk()), ("B.vpk", &gb_vpk())]);
    server.mount_gb_addon(13, "Pack", &[("pack.zip", pack, 100)]);
    let s = store_shell(cx, &env, &server);
    install_gb_with_update(cx, &s, &env, 12);
    let _page = open_addons(cx, &s);
    let [update, remove, open] = gb_buttons(12);
    let all = [update.clone(), remove.clone(), open];
    assert!(support::disabled_ids(cx, s.win, &all).is_empty());
    // The pack waits on its checklist, so its install stays running.
    let rt = cx.update(|cx| tf2tools_app::tokio_bridge::handle(cx));
    let profile = rt
        .block_on(
            s.services
                .store
                .addon_catalog()
                .profile(tf2tools_core::GbId::new(13)),
        )
        .unwrap();
    let installs = cx.update(tf2tools_app::gb_addon_installs::GbAddonInstalls::global);
    let (win, section) = (s.win, tf2tools_core::AddonSection::by_key("skins").unwrap());
    assert!(cx.update(|cx| installs.update(cx, |i, cx| i.install(section, profile, win, cx))));
    support::wait_real(cx, "the checklist", |cx| {
        support::present(cx, s.win, ids::PAYLOAD_CHECKLIST)
    });
    assert_eq!(support::disabled_ids(cx, s.win, &all), vec![update, remove]);
}

#[gpui_kit::test]
fn a_recorded_gamebanana_map_is_neither_counted_nor_listed(cx: &mut TestAppContext) {
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_addon(4, "koth_x", &[("a.vpk", gb_vpk(), 100)]);
    let s = store_shell(cx, &env, &server);
    install_gb(cx, &s, 4, "maps");
    let _page = open_addons(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::ADDONS_COUNT).as_deref(),
        Some("No addons in library")
    );
    assert!(support::present(cx, s.win, ids::ADDONS_EMPTY));
    assert!(!support::present(cx, s.win, ids::ADDONS_GB));
}
