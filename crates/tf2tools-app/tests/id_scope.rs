//! IdScope: the page keeps its ids, a hosted copy swaps the first word for its prefix.

use gpui_kit::ElementId;
use tf2tools_app::ids::{self, IdScope};
use tf2tools_core::{TfClass, WeaponSlot};

fn name(id: ElementId) -> String {
    match id {
        ElementId::Name(n) => n.to_string(),
        other => panic!("expected a named id, got {other:?}"),
    }
}

#[test]
fn the_page_scope_keeps_every_id() {
    assert_eq!(
        name(IdScope::PAGE.id(ids::CROSSHAIRS_BOARD)),
        "crosshairs-board"
    );
    assert_eq!(
        name(IdScope::PAGE.id(ids::graphics_row("lod"))),
        "graphics-row:lod"
    );
    assert_eq!(IdScope::default(), IdScope::PAGE);
}

#[test]
fn a_hosted_scope_swaps_the_first_word() {
    let xh = IdScope::PROFILE_CROSSHAIRS;
    assert_eq!(name(xh.id(ids::CROSSHAIRS_BOARD)), "profile-xh-board");
    assert_eq!(
        name(xh.id(ids::loadout_cell(TfClass::Heavy, WeaponSlot::Primary))),
        "profile-xh-cell:heavy:primary"
    );
    assert_eq!(
        name(xh.id(ids::crosshair_builtin("dot"))),
        "profile-xh-option:builtin:dot"
    );
    let gfx = IdScope::PROFILE_GRAPHICS;
    assert_eq!(
        name(gfx.id(ids::graphics_row("lod"))),
        "profile-gfx-row:lod"
    );
    assert_eq!(
        name(gfx.id(ids::graphics_segment("phong", "off"))),
        "profile-gfx-segment:phong=off"
    );
}

#[test]
fn a_name_without_a_dash_gets_the_prefix_in_front() {
    assert_eq!(
        name(IdScope::PROFILE_CROSSHAIRS.id("close")),
        "profile-xh-close"
    );
}

#[test]
fn scoped_board_and_picker_ids_stay_distinct() {
    let page: Vec<ElementId> = vec![
        ids::CROSSHAIRS_BOARD.into(),
        ids::CROSSHAIRS_BOARD_EVERYTHING.into(),
        ids::CROSSHAIRS_DETAILS.into(),
        ids::CROSSHAIRS_DETAILS_RESET.into(),
        ids::CROSSHAIRS_DETAILS_CLOSE.into(),
        ids::CROSSHAIRS_SEARCH.into(),
        ids::CROSSHAIRS_LIST.into(),
        ids::CROSSHAIRS_LIST_SCROLLBAR.into(),
        ids::CROSSHAIRS_GROUP_ALL.into(),
        ids::CROSSHAIRS_GROUP_BUILTIN.into(),
        ids::CROSSHAIRS_GROUP_LIBRARY.into(),
        ids::CROSSHAIRS_NO_MATCH.into(),
        ids::CROSSHAIRS_IMPORT.into(),
        ids::loadout_cell(TfClass::Heavy, WeaponSlot::Primary),
        ids::loadout_cell_menu(TfClass::Heavy, WeaponSlot::Primary),
        ids::loadout_class(TfClass::Heavy),
        ids::loadout_slot(WeaponSlot::Primary),
    ];
    let scoped: std::collections::BTreeSet<String> = page
        .iter()
        .map(|id| name(IdScope::PROFILE_CROSSHAIRS.id(id.clone())))
        .collect();
    assert_eq!(scoped.len(), page.len(), "{scoped:?}");
    for id in page {
        assert!(
            !scoped.contains(&name(id.clone())),
            "{id:?} kept its page name"
        );
    }
}

#[test]
fn ids_that_are_not_names_pass_through() {
    let row = ElementId::NamedInteger("crosshair-tile-row".into(), 3);
    assert_eq!(IdScope::PROFILE_CROSSHAIRS.id(row.clone()), row);
}
