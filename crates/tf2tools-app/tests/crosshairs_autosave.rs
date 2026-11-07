//! The Crosshairs page saves into TF2 on its own: every board change applies in the
//! background, rapid changes coalesce, and without TF2 the change waits for the game.

mod support;

use std::path::PathBuf;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext};
use tf2tools_app::crosshair_sync::{CrosshairSync, SyncStatus};
use tf2tools_app::ids;
use tf2tools_app::loadout::BoardTarget;
use tf2tools_app::pages::crosshairs::CrosshairsPage;
use tf2tools_app::pages::profiles::probe::Tf2Probe;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{Tf2Install, TfClass, WeaponSlot};

fn found(env: &support::Env) -> Tf2State {
    Tf2State::Found(Tf2Install::from_path(&env.game).unwrap())
}

fn managed(env: &support::Env) -> PathBuf {
    env.game.join("tf/custom/tf2tools-crosshairs")
}

fn script(env: &support::Env, name: &str) -> PathBuf {
    managed(env).join("scripts").join(format!("{name}.txt"))
}

fn window(cx: &mut TestAppContext, env: &support::Env, state: Tf2State) -> support::ShellWindow {
    let s = support::shell_window(cx, env, state);
    cx.update(|cx| Tf2Probe::install(cx, || false));
    s
}

fn open(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<CrosshairsPage> {
    support::click_id(cx, s.win, ids::nav(Page::Crosshairs));
    let page = cx
        .update(|cx| s.shell.read(cx).crosshairs_page())
        .expect("a Crosshairs page");
    support::wait_real(cx, "Crosshairs to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

fn sync(cx: &mut TestAppContext) -> Entity<CrosshairSync> {
    cx.update(|cx| CrosshairSync::global(cx))
        .expect("the apply queue")
}

/// Waits until `n` applies have finished and none runs.
fn wait_applied(cx: &mut TestAppContext, n: usize) {
    let sync = sync(cx);
    support::wait_real(cx, "the applies", |cx| {
        cx.update(|cx| {
            let sync = sync.read(cx);
            sync.finished() >= n && !sync.is_saving()
        })
    });
}

fn line(cx: &mut TestAppContext, s: &support::ShellWindow) -> Option<String> {
    support::label_of(cx, s.win, ids::CROSSHAIRS_WAITING)
}

fn select_first(cx: &mut TestAppContext, s: &support::ShellWindow) {
    let first = s.services.builtins.all()[0].name().to_owned();
    support::click_id(cx, s.win, ids::crosshair_builtin(&first));
}

#[gpui_kit::test]
fn there_is_no_install_button(cx: &mut TestAppContext) {
    let env = support::env();
    let s = window(cx, &env, found(&env));
    let _page = open(cx, &s);
    assert!(!support::present(cx, s.win, ids::CROSSHAIRS_INSTALL));
    assert_eq!(
        line(cx, &s).as_deref(),
        Some("TF2 is using its own crosshairs")
    );
}

#[gpui_kit::test]
fn a_cell_click_puts_its_weapons_in_tf2(cx: &mut TestAppContext) {
    let env = support::env();
    let s = window(cx, &env, found(&env));
    let _page = open(cx, &s);
    select_first(cx, &s);
    support::click_id(
        cx,
        s.win,
        ids::loadout_cell(TfClass::Scout, WeaponSlot::Primary),
    );
    wait_applied(cx, 1);
    for weapon in BoardTarget::Cell(TfClass::Scout, WeaponSlot::Primary).scripts() {
        assert!(script(&env, weapon).is_file(), "{weapon}");
    }
    support::wait_real(cx, "the line", |cx| {
        line(cx, &s).as_deref() == Some("In TF2")
    });
}

#[gpui_kit::test]
fn every_kind_of_change_applies(cx: &mut TestAppContext) {
    let env = support::env();
    let s = window(cx, &env, found(&env));
    let _page = open(cx, &s);
    select_first(cx, &s);
    let first = s.services.builtins.all()[0].name().to_owned();
    let mut applies = 0;
    let mut expect_apply = |cx: &mut TestAppContext, what: &str| {
        applies += 1;
        wait_applied(cx, applies);
        let queue = sync(cx);
        assert_eq!(cx.update(|cx| queue.read(cx).started()), applies, "{what}");
    };
    support::click_id(cx, s.win, ids::loadout_class(TfClass::Soldier));
    expect_apply(cx, "class");
    support::click_id(cx, s.win, ids::loadout_slot(WeaponSlot::Melee));
    expect_apply(cx, "slot");
    support::click_id(cx, s.win, ids::CROSSHAIRS_BOARD_EVERYTHING);
    expect_apply(cx, "everything");
    cx.update_window(s.win, |_, w, cx| {
        w.drag_to(
            ids::crosshair_builtin(&first),
            ids::loadout_cell(TfClass::Spy, WeaponSlot::Melee),
            cx,
        )
    })
    .unwrap();
    expect_apply(cx, "drag");
    support::click_id(
        cx,
        s.win,
        ids::loadout_cell_menu(TfClass::Pyro, WeaponSlot::Primary),
    );
    let weapon = BoardTarget::Cell(TfClass::Pyro, WeaponSlot::Primary).scripts()[0];
    support::click_id(cx, s.win, ids::weapon_row(weapon));
    expect_apply(cx, "per weapon");
    support::click_id(cx, s.win, ids::CROSSHAIRS_DETAILS_RESET);
    expect_apply(cx, "reset");
}

#[gpui_kit::test]
fn rapid_changes_coalesce_into_one_rerun(cx: &mut TestAppContext) {
    let env = support::env();
    let s = window(cx, &env, found(&env));
    let _page = open(cx, &s);
    let sync = sync(cx);
    // Three asks before the first apply can finish: one runs, one rerun follows.
    cx.update(|cx| {
        sync.update(cx, |sync, cx| {
            sync.request(cx);
            sync.request(cx);
            sync.request(cx);
        })
    });
    wait_applied(cx, 2);
    support::settle(cx, s.win);
    assert_eq!(cx.update(|cx| sync.read(cx).started()), 2);
}

#[gpui_kit::test]
fn reset_takes_the_weapons_out_of_the_game(cx: &mut TestAppContext) {
    let env = support::env();
    let s = window(cx, &env, found(&env));
    let _page = open(cx, &s);
    select_first(cx, &s);
    support::click_id(
        cx,
        s.win,
        ids::loadout_cell(TfClass::Heavy, WeaponSlot::Primary),
    );
    wait_applied(cx, 1);
    support::click_id(
        cx,
        s.win,
        ids::loadout_cell(TfClass::Heavy, WeaponSlot::Melee),
    );
    wait_applied(cx, 2);
    let melee = BoardTarget::Cell(TfClass::Heavy, WeaponSlot::Melee).scripts();
    assert!(script(&env, melee[0]).is_file());

    support::click_id(
        cx,
        s.win,
        ids::loadout_cell_menu(TfClass::Heavy, WeaponSlot::Melee),
    );
    support::click_id(cx, s.win, ids::CROSSHAIRS_DETAILS_RESET);
    wait_applied(cx, 3);
    assert!(!script(&env, melee[0]).exists());
    let primary = BoardTarget::Cell(TfClass::Heavy, WeaponSlot::Primary).scripts();
    assert!(script(&env, primary[0]).is_file());

    // With nothing left on the board, the managed folder goes.
    support::click_id(
        cx,
        s.win,
        ids::loadout_cell_menu(TfClass::Heavy, WeaponSlot::Primary),
    );
    support::click_id(cx, s.win, ids::CROSSHAIRS_DETAILS_RESET);
    wait_applied(cx, 4);
    assert!(!managed(&env).exists());
}

#[gpui_kit::test]
fn without_tf2_a_change_saves_and_applies_once_tf2_is_found(cx: &mut TestAppContext) {
    let env = support::env();
    let s = window(cx, &env, Tf2State::Missing { saved: None });
    let _page = open(cx, &s);
    assert!(support::present(cx, s.win, ids::CROSSHAIRS_NO_TF2));
    select_first(cx, &s);
    support::click_id(
        cx,
        s.win,
        ids::loadout_cell(TfClass::Medic, WeaponSlot::Primary),
    );
    let medic = BoardTarget::Cell(TfClass::Medic, WeaponSlot::Primary).scripts();
    support::wait_real(cx, "the save", |_| {
        s.services.library.assignments().unwrap().len() == medic.len()
    });
    support::wait_real(cx, "the line", |cx| {
        line(cx, &s).as_deref() == Some("Saved. It goes into TF2 once TF2 is found")
    });
    assert!(!managed(&env).exists());

    cx.update(|cx| {
        s.tf2.update(cx, |tf2, cx| {
            *tf2 = found(&env);
            cx.notify();
        })
    });
    wait_applied(cx, 1);
    assert!(script(&env, medic[0]).is_file());
}

#[gpui_kit::test]
fn a_change_while_tf2_runs_says_when_it_takes_effect(cx: &mut TestAppContext) {
    let env = support::env();
    let s = window(cx, &env, found(&env));
    cx.update(|cx| Tf2Probe::install(cx, || true));
    let _page = open(cx, &s);
    select_first(cx, &s);
    support::click_id(
        cx,
        s.win,
        ids::loadout_cell(TfClass::Sniper, WeaponSlot::Primary),
    );
    wait_applied(cx, 1);
    support::wait_real(cx, "the line", |cx| {
        line(cx, &s).as_deref()
            == Some("Saved. TF2 is running, so it takes effect next time TF2 starts")
    });
}

#[gpui_kit::test]
fn skipped_weapons_show_in_the_line(cx: &mut TestAppContext) {
    let env = support::env();
    let s = window(cx, &env, found(&env));
    s.services
        .library
        .assign(
            "tf_weapon_bat",
            &tf2tools_core::CrosshairRef::Builtin("nope".into()),
        )
        .unwrap();
    let _page = open(cx, &s);
    select_first(cx, &s);
    support::click_id(
        cx,
        s.win,
        ids::loadout_cell(TfClass::Spy, WeaponSlot::Primary),
    );
    wait_applied(cx, 1);
    support::wait_real(cx, "the line", |cx| {
        line(cx, &s).as_deref() == Some("In TF2. Skipped 1: tf_weapon_bat (unknown builtin nope)")
    });
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_failed_apply_toasts_once(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt as _;
    let env = support::env();
    let s = window(cx, &env, found(&env));
    let _page = open(cx, &s);
    let custom = env.game.join("tf/custom");
    std::fs::set_permissions(&custom, std::fs::Permissions::from_mode(0o555)).unwrap();
    // Root ignores the mode, so the apply could not fail; nothing to test then.
    if std::fs::write(custom.join("probe"), b"").is_ok() {
        std::fs::set_permissions(&custom, std::fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }
    select_first(cx, &s);
    support::click_id(
        cx,
        s.win,
        ids::loadout_cell(TfClass::Scout, WeaponSlot::Melee),
    );
    wait_applied(cx, 1);
    std::fs::set_permissions(&custom, std::fs::Permissions::from_mode(0o755)).unwrap();
    support::wait_real(cx, "the toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    let toast = support::label_of(cx, s.win, ids::TOAST).unwrap();
    assert!(
        toast.starts_with("Couldn't save your crosshairs to TF2"),
        "{toast}"
    );
    let queue = sync(cx);
    assert_eq!(
        cx.update(|cx| queue.read(cx).status().clone()),
        SyncStatus::Failed
    );
    // No retry loop: the failure ends the work.
    support::settle(cx, s.win);
    assert_eq!(cx.update(|cx| queue.read(cx).started()), 1);
}

#[gpui_kit::test]
fn remove_from_tf2_keeps_the_board_and_the_next_change_reapplies(cx: &mut TestAppContext) {
    let env = support::env();
    let s = window(cx, &env, found(&env));
    let _page = open(cx, &s);
    select_first(cx, &s);
    support::click_id(
        cx,
        s.win,
        ids::loadout_cell(TfClass::Scout, WeaponSlot::Primary),
    );
    wait_applied(cx, 1);
    assert!(managed(&env).exists());
    support::click_id(cx, s.win, ids::CROSSHAIRS_UNINSTALL);
    support::wait_real(cx, "the removal", |_| !managed(&env).exists());
    support::wait_real(cx, "the line", |cx| {
        line(cx, &s).as_deref() == Some("Removed from TF2. Your next change puts them back")
    });
    assert!(!s.services.library.assignments().unwrap().is_empty());
    support::click_id(
        cx,
        s.win,
        ids::loadout_cell(TfClass::Scout, WeaponSlot::Melee),
    );
    wait_applied(cx, 2);
    assert!(managed(&env).exists());
}
