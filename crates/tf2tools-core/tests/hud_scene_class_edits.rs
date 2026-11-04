//! The class select scene under a #base stub, an edit, and a root block that is gone or zeroed.

mod common;

use common::hud_stock::{class_select, class_stub, stock_files, stock_files_with};
use tf2tools_core::{
    ChangeTarget, EditRefused, HudFiles, HudModel, PartKey, Platform, Scene, Screen,
};

const FILE: &str = "resource/ui/classselection.res";
const ROOT: &str =
    "  class { ControlName Frame fieldName class xpos 0 ypos 0 wide f0 tall 480 visible 1 }\n";
const SD: Screen = Screen::new(640, 480);

fn menu(files: &HudFiles) -> HudModel {
    HudModel::load_scene(files, Platform::Linux, Scene::ClassSelect).unwrap()
}

fn k(text: &str) -> PartKey {
    PartKey::new(text)
}

#[test]
fn a_stub_file_draws_from_the_merged_tree_read_only() {
    let m = menu(&class_stub());
    assert!(m.problems().is_empty());
    assert_eq!(m.stub_files(), [FILE]);
    let part = m.part(&k("class/soldier/SubImage")).expect("drawn");
    assert!(part.is_from_base() && !part.is_readable());
    let stock = menu(&stock_files());
    assert_eq!(m.parts().len(), stock.parts().len());
    assert_eq!(
        m.layout(SD).get(&k("class/soldier")),
        stock.layout(SD).get(&k("class/soldier"))
    );
    assert_eq!(
        m.move_part(&k("class/soldier"), 4, 0, SD, 1),
        Err(EditRefused::ReadOnly)
    );
}

#[test]
fn a_move_edits_classselection_res() {
    let m = menu(&stock_files());
    let changes = m.move_part(&k("class/CancelButton"), 10, 0, SD, 1).unwrap();
    assert!(!changes.is_empty());
    for c in &changes {
        let ChangeTarget::Set(s) = c.target() else {
            panic!("not a set")
        };
        assert_eq!(s.file(), FILE);
    }
}

#[test]
fn a_file_without_the_class_block_still_draws_its_controls() {
    // One corpus HUD (xhud) drops the block; the engine's 64x24 default then stands.
    let text = class_select().replace(ROOT, "");
    assert_ne!(text, class_select());
    let m = menu(&stock_files_with(&[(FILE, Some(&text))]));
    assert!(m.hidden_by_hud().is_none() && m.problems().is_empty());
    assert!(m.part(&k("class")).unwrap().is_readable());
    let l = m.layout(SD);
    let soldier = l.get(&k("class/soldier")).unwrap();
    assert_eq!((soldier.x(), soldier.wide()), (70, 50));
    assert!(soldier.hidden().is_none());
}

#[test]
fn a_root_block_zeroed_by_the_hud_hides_the_scene() {
    let text = class_select().replace(
        "wide f0 tall 480 visible 1 }\n  scout",
        "wide 0 tall 0 visible 1 }\n  scout",
    );
    assert_ne!(text, class_select());
    let m = menu(&stock_files_with(&[(FILE, Some(&text))]));
    assert!(m.hidden_by_hud().is_some());
}
