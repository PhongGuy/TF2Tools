//! The loadout board's model: what each class and slot cell shows, which weapons a board
//! click targets, and how many weapons wait for an Install.

use tf2tools_app::loadout::{BoardTarget, CellState, Loadout, SLOTS};
use tf2tools_core::{CrosshairRef, TfClass, WeaponSlot, weapons};

fn builtin(name: &str) -> CrosshairRef {
    CrosshairRef::Builtin(name.into())
}

fn scripts_of(class: TfClass, slot: Option<WeaponSlot>) -> Vec<&'static str> {
    weapons()
        .iter()
        .filter(|w| w.class() == class && slot.is_none_or(|s| w.slot() == s))
        .map(|w| w.script_name())
        .collect()
}

fn loadout(assigned: &[(&str, CrosshairRef)]) -> Loadout {
    Loadout::new(
        assigned.iter().map(|(s, c)| ((*s).to_owned(), c.clone())),
        Vec::<String>::new(),
    )
}

#[test]
fn every_class_has_the_three_board_slots() {
    assert_eq!(
        SLOTS,
        [
            WeaponSlot::Primary,
            WeaponSlot::Secondary,
            WeaponSlot::Melee
        ]
    );
    for class in TfClass::ALL {
        for slot in SLOTS {
            assert!(
                !scripts_of(class, Some(slot)).is_empty(),
                "{class:?} {slot:?}"
            );
        }
    }
}

#[test]
fn a_cell_with_nothing_assigned_is_default() {
    let board = loadout(&[]);
    assert_eq!(
        board.cell(TfClass::Scout, WeaponSlot::Primary),
        CellState::Default
    );
}

#[test]
fn a_cell_whose_weapons_all_share_a_crosshair_shows_it() {
    let dot = builtin("dot");
    let assigned: Vec<_> = scripts_of(TfClass::Scout, Some(WeaponSlot::Primary))
        .into_iter()
        .map(|s| (s, dot.clone()))
        .collect();
    let board = loadout(&assigned);
    assert_eq!(
        board.cell(TfClass::Scout, WeaponSlot::Primary),
        CellState::Single(dot)
    );
    assert_eq!(
        board.cell(TfClass::Scout, WeaponSlot::Melee),
        CellState::Default
    );
}

#[test]
fn a_cell_with_different_crosshairs_is_mixed() {
    let primaries = scripts_of(TfClass::Soldier, Some(WeaponSlot::Primary));
    assert!(primaries.len() > 1);
    let board = loadout(&[(primaries[0], builtin("a")), (primaries[1], builtin("b"))]);
    assert_eq!(
        board.cell(TfClass::Soldier, WeaponSlot::Primary),
        CellState::Mixed
    );
}

#[test]
fn a_cell_with_only_some_weapons_assigned_is_mixed() {
    let primaries = scripts_of(TfClass::Soldier, Some(WeaponSlot::Primary));
    let board = loadout(&[(primaries[0], builtin("a"))]);
    assert_eq!(
        board.cell(TfClass::Soldier, WeaponSlot::Primary),
        CellState::Mixed
    );
}

#[test]
fn a_cell_targets_the_class_weapons_in_its_slot() {
    let target = BoardTarget::Cell(TfClass::Heavy, WeaponSlot::Secondary);
    assert_eq!(
        target.scripts(),
        scripts_of(TfClass::Heavy, Some(WeaponSlot::Secondary))
    );
}

#[test]
fn a_class_header_targets_every_weapon_of_the_class() {
    assert_eq!(
        BoardTarget::Class(TfClass::Spy).scripts(),
        scripts_of(TfClass::Spy, None)
    );
}

#[test]
fn a_slot_header_targets_that_slot_for_every_class() {
    let expected: Vec<&str> = weapons()
        .iter()
        .filter(|w| w.slot() == WeaponSlot::Melee)
        .map(|w| w.script_name())
        .collect();
    assert_eq!(BoardTarget::Slot(WeaponSlot::Melee).scripts(), expected);
    assert!(
        expected
            .iter()
            .any(|s| scripts_of(TfClass::Scout, None).contains(s))
    );
    assert!(
        expected
            .iter()
            .any(|s| scripts_of(TfClass::Spy, None).contains(s))
    );
}

#[test]
fn everything_targets_every_weapon() {
    let all: Vec<&str> = weapons().iter().map(|w| w.script_name()).collect();
    assert_eq!(BoardTarget::Everything.scripts(), all);
}

#[test]
fn targets_say_what_they_cover_in_player_words() {
    assert_eq!(
        BoardTarget::Cell(TfClass::Scout, WeaponSlot::Primary).describe(),
        "all Scout primary weapons"
    );
    assert_eq!(
        BoardTarget::Class(TfClass::Soldier).describe(),
        "all Soldier weapons"
    );
    assert_eq!(
        BoardTarget::Slot(WeaponSlot::Melee).describe(),
        "every class's melee weapons"
    );
    assert_eq!(BoardTarget::Everything.describe(), "every weapon");
}

#[test]
fn clearing_a_cell_returns_it_to_default_and_leaves_the_rest() {
    let dot = builtin("dot");
    let all: Vec<_> = BoardTarget::Class(TfClass::Pyro)
        .scripts()
        .into_iter()
        .map(|s| (s, dot.clone()))
        .collect();
    let mut board = loadout(&all);
    board.clear(BoardTarget::Cell(TfClass::Pyro, WeaponSlot::Melee));
    assert_eq!(
        board.cell(TfClass::Pyro, WeaponSlot::Melee),
        CellState::Default
    );
    assert_eq!(
        board.cell(TfClass::Pyro, WeaponSlot::Primary),
        CellState::Single(dot)
    );
}

#[test]
fn assigning_a_target_fills_every_cell_it_covers() {
    let mut board = loadout(&[]);
    board.assign(BoardTarget::Slot(WeaponSlot::Secondary), &builtin("x"));
    for class in TfClass::ALL {
        assert_eq!(
            board.cell(class, WeaponSlot::Secondary),
            CellState::Single(builtin("x"))
        );
        assert_eq!(board.cell(class, WeaponSlot::Primary), CellState::Default);
    }
}

#[test]
fn the_waiting_count_is_the_weapons_an_install_would_change() {
    let primaries = scripts_of(TfClass::Scout, Some(WeaponSlot::Primary));
    let board = Loadout::new(
        Vec::new(),
        vec![primaries[0].to_owned(), "tf_weapon_minigun".to_owned()],
    );
    assert_eq!(board.waiting_count(), 2);
    assert!(board.cell_waiting(TfClass::Scout, WeaponSlot::Primary));
    assert!(board.cell_waiting(TfClass::Heavy, WeaponSlot::Primary));
    assert!(!board.cell_waiting(TfClass::Scout, WeaponSlot::Melee));
    assert_eq!(loadout(&[]).waiting_count(), 0);
}

#[test]
fn the_waiting_line_names_the_count() {
    assert_eq!(Loadout::waiting_line(1), "1 change not in TF2 yet");
    assert_eq!(Loadout::waiting_line(5), "5 changes not in TF2 yet");
    assert_eq!(Loadout::waiting_line(0), "Your crosshairs are in TF2");
}
