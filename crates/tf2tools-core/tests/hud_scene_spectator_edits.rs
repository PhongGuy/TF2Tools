//! The spectator scene under a #base stub, an edit, and a root block that is gone or zeroed.

use crate::common;

use common::hud_stock::{SPECTATOR, spectator_stub, stock_files, stock_files_with};
use tf2tools_core::{
    ChangeTarget, EditRefused, HudFiles, HudModel, PartKey, Platform, Scene, Screen,
};

const FILE: &str = "resource/ui/spectator.res";
const ROOT: &str =
    "  Spectator { ControlName Frame fieldName Spectator tall 480 visible 1 enabled 1 }\n";
const SD: Screen = Screen::new(640, 480);

fn bar(files: &HudFiles) -> HudModel {
    HudModel::load_scene(files, Platform::Linux, Scene::Spectator).unwrap()
}

fn k(text: &str) -> PartKey {
    PartKey::new(text)
}

#[test]
fn a_stub_file_draws_from_the_merged_tree_read_only() {
    let m = bar(&spectator_stub());
    assert!(m.problems().is_empty());
    assert_eq!(m.stub_files(), [FILE]);
    let part = m.part(&k("Spectator/TipLabel")).expect("drawn");
    assert!(part.is_from_base() && !part.is_readable());
    let stock = bar(&stock_files());
    assert_eq!(m.parts().len(), stock.parts().len());
    assert_eq!(
        m.layout(SD).get(&k("Spectator/TipLabel")),
        stock.layout(SD).get(&k("Spectator/TipLabel"))
    );
    assert_eq!(
        m.move_part(&k("Spectator/TipLabel"), 4, 0, SD, 1),
        Err(EditRefused::ReadOnly)
    );
}

#[test]
fn a_move_edits_spectator_res() {
    let m = bar(&stock_files());
    let changes = m.move_part(&k("Spectator/TipLabel"), 10, 0, SD, 1).unwrap();
    assert!(!changes.is_empty());
    for c in &changes {
        let ChangeTarget::Set(s) = c.target() else {
            panic!("not a set")
        };
        assert_eq!(s.file(), FILE);
    }
}

#[test]
fn a_file_without_the_root_block_still_draws_and_fills_the_screen() {
    // Four corpus HUDs (two of them stubs) have no Spectator block; the game still sizes it.
    let text = SPECTATOR.replace(ROOT, "");
    assert_ne!(text, SPECTATOR);
    let m = bar(&stock_files_with(&[(FILE, Some(&text))]));
    assert!(m.hidden_by_hud().is_none() && m.problems().is_empty());
    assert!(m.part(&k("Spectator")).unwrap().is_readable());
    let l = m.layout(SD);
    let root = l.get(&k("Spectator")).unwrap();
    assert_eq!((root.wide(), root.tall()), (640, 480));
    let tip = l.get(&k("Spectator/TipLabel")).unwrap();
    assert_eq!((tip.x(), tip.y(), tip.wide()), (15, 413, 145));
    assert!(tip.hidden().is_none());
}

#[test]
fn a_root_block_the_hud_zeroes_cannot_hide_the_scene() {
    // The game sets the panel's bounds and keeps its visibility, so these keys do nothing.
    let zeroed =
        "  Spectator { ControlName Frame fieldName Spectator wide 0 tall 0 visible 0 enabled 1 }\n";
    let text = SPECTATOR.replace(ROOT, zeroed);
    assert_ne!(text, SPECTATOR);
    let m = bar(&stock_files_with(&[(FILE, Some(&text))]));
    assert!(m.hidden_by_hud().is_none());
    let l = m.layout(SD);
    let root = l.get(&k("Spectator")).unwrap();
    assert_eq!((root.wide(), root.tall()), (640, 480));
    assert!(l.get(&k("Spectator/topbar")).unwrap().hidden().is_none());
}
