//! The nine bundled class icons and their text fallback.

use tf2tools_app::assets::AssetRoot;
use tf2tools_app::widgets::class_icon::class_short_name;
use tf2tools_core::{TfClass, leaderboard_icon_stem};

#[test]
fn icon_stems_follow_the_game_files() {
    assert_eq!(
        TfClass::ALL.map(leaderboard_icon_stem),
        [
            "scout", "soldier", "pyro", "demo", "heavy", "engineer", "medic", "sniper", "spy"
        ]
    );
}

#[test]
fn every_class_has_a_bundled_png() {
    let root = AssetRoot::resolve().expect("the repo's assets folder");
    for class in TfClass::ALL {
        let path = root
            .class_icon(class)
            .unwrap_or_else(|| panic!("{class:?} has no icon"));
        let bytes = std::fs::read(&path).unwrap();
        assert!(
            bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
            "{}",
            path.display()
        );
    }
}

#[test]
fn a_root_without_icons_falls_back_to_text() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("crosshairs")).unwrap();
    std::fs::write(tmp.path().join("crosshairs/crosshairs.json"), "[]").unwrap();
    let root = AssetRoot::resolve_from(Some(tmp.path().to_path_buf()), None, None).unwrap();
    assert_eq!(root.class_icon(TfClass::Medic), None);
    assert_eq!(
        TfClass::ALL.map(class_short_name),
        [
            "Scout", "Soldier", "Pyro", "Demo", "Heavy", "Engie", "Medic", "Sniper", "Spy"
        ]
    );
}
