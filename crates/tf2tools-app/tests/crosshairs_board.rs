//! The Crosshairs page's loadout board: cells per class and slot, bulk assigns from the
//! headers, the per-weapon panel, reset, drag and drop, and the not-in-TF2 bar.

mod support;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::loadout::{BoardTarget, SLOTS};
use tf2tools_app::pages::crosshairs::CrosshairsPage;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{CrosshairRef, Tf2Install, TfClass, WeaponSlot};

fn found(env: &support::Env) -> Tf2State {
    Tf2State::Found(Tf2Install::from_path(&env.game).unwrap())
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

fn toast_text(cx: &mut TestAppContext, s: &support::ShellWindow) -> String {
    support::wait_real(cx, "the toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    support::label_of(cx, s.win, ids::TOAST).unwrap()
}

/// Waits for the board change and the apply it starts to finish, so the table is final.
fn wait_saved(cx: &mut TestAppContext, s: &support::ShellWindow) {
    let sync = cx
        .update(|cx| tf2tools_app::crosshair_sync::CrosshairSync::global(cx))
        .expect("the apply queue");
    support::wait_real(cx, "the save", |cx| {
        cx.update(|cx| {
            let sync = sync.read(cx);
            sync.finished() >= 1 && !sync.is_saving()
        })
    });
    support::settle(cx, s.win);
}

fn sorted_scripts(s: &support::ShellWindow) -> Vec<String> {
    let mut scripts: Vec<String> = s
        .services
        .library
        .assignments()
        .unwrap()
        .iter()
        .map(|a| a.weapon_script().to_owned())
        .collect();
    scripts.sort();
    scripts
}

fn sorted(target: BoardTarget) -> Vec<String> {
    let mut scripts: Vec<String> = target.scripts().iter().map(|s| s.to_string()).collect();
    scripts.sort();
    scripts
}

fn first_builtin(s: &support::ShellWindow) -> (CrosshairRef, String) {
    let first = s.services.builtins.all()[0].clone();
    (
        CrosshairRef::Builtin(first.name().to_owned()),
        first.display_name().to_owned(),
    )
}

fn select_first(cx: &mut TestAppContext, s: &support::ShellWindow) -> String {
    let first = s.services.builtins.all()[0].clone();
    support::click_id(cx, s.win, ids::crosshair_builtin(first.name()));
    first.display_name().to_owned()
}

fn assign(s: &support::ShellWindow, target: BoardTarget, crosshair: &CrosshairRef) {
    s.services
        .library
        .assign_many(&target.scripts(), crosshair)
        .unwrap();
}

fn cell_label(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    class: TfClass,
    slot: WeaponSlot,
) -> Option<String> {
    support::label_of(cx, s.win, ids::loadout_cell(class, slot))
}

#[gpui_kit::test]
fn the_board_has_a_cell_for_every_class_and_slot(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open(cx, &s);
    assert!(support::present(cx, s.win, ids::CROSSHAIRS_BOARD));
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIRS_BOARD_EVERYTHING).as_deref(),
        Some("Everything")
    );
    for class in TfClass::ALL {
        assert_eq!(
            support::label_of(cx, s.win, ids::loadout_class(class)).as_deref(),
            Some(class.name())
        );
        for slot in SLOTS {
            assert_eq!(cell_label(cx, &s, class, slot).as_deref(), Some("Default"));
        }
    }
    for slot in SLOTS {
        assert_eq!(
            support::label_of(cx, s.win, ids::loadout_slot(slot)).as_deref(),
            Some(slot.name())
        );
    }
}

#[gpui_kit::test]
fn a_cell_names_its_crosshair_or_says_mixed(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let (first, name) = first_builtin(&s);
    assign(
        &s,
        BoardTarget::Cell(TfClass::Scout, WeaponSlot::Primary),
        &first,
    );
    let soldier = BoardTarget::Cell(TfClass::Soldier, WeaponSlot::Primary).scripts();
    s.services.library.assign(soldier[0], &first).unwrap();
    let _page = open(cx, &s);
    assert_eq!(
        cell_label(cx, &s, TfClass::Scout, WeaponSlot::Primary),
        Some(name)
    );
    assert_eq!(
        cell_label(cx, &s, TfClass::Soldier, WeaponSlot::Primary).as_deref(),
        Some("Mixed")
    );
    assert_eq!(
        cell_label(cx, &s, TfClass::Scout, WeaponSlot::Melee).as_deref(),
        Some("Default")
    );
}

#[gpui_kit::test]
fn clicking_a_cell_assigns_the_selection_to_its_weapons(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open(cx, &s);
    let name = select_first(cx, &s);
    let target = BoardTarget::Cell(TfClass::Scout, WeaponSlot::Primary);
    support::click_id(
        cx,
        s.win,
        ids::loadout_cell(TfClass::Scout, WeaponSlot::Primary),
    );
    wait_saved(cx, &s);
    assert_eq!(sorted_scripts(&s), sorted(target));
    support::wait_real(cx, "the reload", |cx| {
        cell_label(cx, &s, TfClass::Scout, WeaponSlot::Primary).as_deref() == Some(name.as_str())
    });
}

#[gpui_kit::test]
fn a_class_header_assigns_the_whole_class(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open(cx, &s);
    select_first(cx, &s);
    support::click_id(cx, s.win, ids::loadout_class(TfClass::Soldier));
    wait_saved(cx, &s);
    assert_eq!(
        sorted_scripts(&s),
        sorted(BoardTarget::Class(TfClass::Soldier))
    );
}

#[gpui_kit::test]
fn a_slot_header_assigns_that_slot_for_every_class(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open(cx, &s);
    select_first(cx, &s);
    support::click_id(cx, s.win, ids::loadout_slot(WeaponSlot::Melee));
    wait_saved(cx, &s);
    assert_eq!(
        sorted_scripts(&s),
        sorted(BoardTarget::Slot(WeaponSlot::Melee))
    );
}

#[gpui_kit::test]
fn everything_assigns_every_weapon(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open(cx, &s);
    select_first(cx, &s);
    support::click_id(cx, s.win, ids::CROSSHAIRS_BOARD_EVERYTHING);
    wait_saved(cx, &s);
    assert_eq!(sorted_scripts(&s), sorted(BoardTarget::Everything));
}

#[gpui_kit::test]
fn a_board_click_without_a_selection_asks_for_one(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open(cx, &s);
    support::click_id(
        cx,
        s.win,
        ids::loadout_cell(TfClass::Pyro, WeaponSlot::Primary),
    );
    assert_eq!(toast_text(cx, &s), "Pick a crosshair first");
    assert!(sorted_scripts(&s).is_empty());
}

#[gpui_kit::test]
fn a_failed_board_assign_changes_no_weapon(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open(cx, &s);
    select_first(cx, &s);
    // The first row of any batch lands and every later one fails.
    rusqlite::Connection::open(env.data.db_path())
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER fail_after_first BEFORE INSERT ON crosshair_assignments
             WHEN (SELECT count(*) FROM crosshair_assignments) >= 1
             BEGIN SELECT RAISE(ABORT, 'disk full'); END;",
        )
        .unwrap();
    support::click_id(cx, s.win, ids::loadout_class(TfClass::Medic));
    let toast = toast_text(cx, &s);
    assert!(toast.starts_with("Failed to assign crosshairs"), "{toast}");
    assert!(!toast.contains("disk full"), "{toast}");
    assert!(sorted_scripts(&s).is_empty());
}

#[gpui_kit::test]
fn right_clicking_a_cell_opens_its_weapons(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open(cx, &s);
    assert!(!support::present(cx, s.win, ids::CROSSHAIRS_DETAILS));
    let cell = ids::loadout_cell(TfClass::Scout, WeaponSlot::Primary);
    cx.update_window(s.win, |_, w, cx| w.right_click(cell, cx))
        .unwrap();
    support::settle(cx, s.win);
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIRS_DETAILS).as_deref(),
        Some("Scout primary weapons")
    );
    for script in BoardTarget::Cell(TfClass::Scout, WeaponSlot::Primary).scripts() {
        assert!(
            support::present(cx, s.win, ids::weapon_row(script)),
            "{script}"
        );
    }
    for script in BoardTarget::Cell(TfClass::Scout, WeaponSlot::Melee).scripts() {
        assert!(
            !support::present(cx, s.win, ids::weapon_row(script)),
            "{script}"
        );
    }
    support::click_id(cx, s.win, ids::CROSSHAIRS_DETAILS_CLOSE);
    assert!(!support::present(cx, s.win, ids::CROSSHAIRS_DETAILS));
}

#[gpui_kit::test]
fn the_cell_menu_button_opens_its_weapons(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open(cx, &s);
    support::click_id(
        cx,
        s.win,
        ids::loadout_cell_menu(TfClass::Spy, WeaponSlot::Melee),
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIRS_DETAILS).as_deref(),
        Some("Spy melee weapons")
    );
    assert_eq!(cx.update(|cx| page.read(cx).class()), TfClass::Spy);
}

#[gpui_kit::test]
fn a_weapon_row_assigns_one_weapon(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open(cx, &s);
    let name = select_first(cx, &s);
    support::click_id(
        cx,
        s.win,
        ids::loadout_cell_menu(TfClass::Scout, WeaponSlot::Primary),
    );
    let script = BoardTarget::Cell(TfClass::Scout, WeaponSlot::Primary).scripts()[0];
    support::click_id(cx, s.win, ids::weapon_row(script));
    wait_saved(cx, &s);
    assert_eq!(sorted_scripts(&s), [script]);
    support::wait_real(cx, "the reload", |cx| {
        support::label_of(cx, s.win, ids::weapon_assigned(script)).as_deref() == Some(name.as_str())
    });
}

#[gpui_kit::test]
fn reset_takes_a_cell_back_to_the_game_crosshair(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let (first, _) = first_builtin(&s);
    assign(&s, BoardTarget::Class(TfClass::Scout), &first);
    let _page = open(cx, &s);
    support::click_id(
        cx,
        s.win,
        ids::loadout_cell_menu(TfClass::Scout, WeaponSlot::Primary),
    );
    support::click_id(cx, s.win, ids::CROSSHAIRS_DETAILS_RESET);
    wait_saved(cx, &s);
    let mut expected = sorted(BoardTarget::Class(TfClass::Scout));
    let primaries = sorted(BoardTarget::Cell(TfClass::Scout, WeaponSlot::Primary));
    expected.retain(|s| !primaries.contains(s));
    assert_eq!(sorted_scripts(&s), expected);
    support::wait_real(cx, "the reload", |cx| {
        cell_label(cx, &s, TfClass::Scout, WeaponSlot::Primary).as_deref() == Some("Default")
    });
}

#[gpui_kit::test]
fn dragging_a_crosshair_onto_a_cell_assigns_it(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open(cx, &s);
    let first = s.services.builtins.all()[0].clone();
    let cell = ids::loadout_cell(TfClass::Heavy, WeaponSlot::Melee);
    cx.update_window(s.win, |_, w, cx| {
        w.drag_to(ids::crosshair_builtin(first.name()), cell, cx)
    })
    .unwrap();
    wait_saved(cx, &s);
    assert_eq!(
        sorted_scripts(&s),
        sorted(BoardTarget::Cell(TfClass::Heavy, WeaponSlot::Melee))
    );
}

#[gpui_kit::test]
fn the_bar_says_tf2_uses_its_own_crosshairs_when_none_are_set(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIRS_WAITING).as_deref(),
        Some("TF2 is using its own crosshairs")
    );
}
