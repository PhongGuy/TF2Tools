//! The win panel scene under a #base stub, an edit, and a hudlayout box that hides it.

mod common;

use common::hud_stock::{HUDLAYOUT, stock_files, stock_files_with, win_stub};
use tf2tools_core::{
    ChangeTarget, EditRefused, HudFiles, HudModel, PartKey, Platform, Scene, Screen,
};

const WIN: &str = "resource/ui/winpanel.res";
const SD: Screen = Screen::new(640, 480);

fn win(files: &HudFiles) -> HudModel {
    HudModel::load_scene(files, Platform::Linux, Scene::WinPanel).unwrap()
}

fn k(text: &str) -> PartKey {
    PartKey::new(text)
}

#[test]
fn a_stub_file_draws_from_the_merged_tree_read_only() {
    let m = win(&win_stub());
    assert!(m.problems().is_empty());
    assert_eq!(m.stub_files(), [WIN]);
    let part = m.part(&k("WinPanel/WinningTeamLabel")).expect("drawn");
    assert!(part.is_from_base() && !part.is_readable());
    let stock = win(&stock_files());
    assert_eq!(m.parts().len(), stock.parts().len());
    assert_eq!(
        m.layout(SD).get(&k("WinPanel/WinningTeamLabel")),
        stock.layout(SD).get(&k("WinPanel/WinningTeamLabel"))
    );
    let key = k("WinPanel/WinningTeamLabel");
    assert_eq!(m.move_part(&key, 4, 0, SD, 1), Err(EditRefused::ReadOnly));
}

#[test]
fn a_move_edits_winpanel_res_and_a_zeroed_layout_box_hides_the_scene() {
    let m = win(&stock_files());
    let changes = m
        .move_part(&k("WinPanel/DetailsLabel"), 10, 0, SD, 1)
        .unwrap();
    assert!(!changes.is_empty());
    for c in &changes {
        let ChangeTarget::Set(s) = c.target() else {
            panic!("not a set")
        };
        assert_eq!(s.file(), WIN);
    }
    let layout = HUDLAYOUT.replace(
        "WinPanel { visible 1 xpos c-150 ypos 215 wide 300 tall 300 }",
        "WinPanel { visible 1 xpos 0 ypos 0 wide 0 tall 0 }",
    );
    assert_ne!(layout, HUDLAYOUT);
    let files = stock_files_with(&[("scripts/hudlayout.res", Some(&layout))]);
    assert!(win(&files).hidden_by_hud().is_some());
}
