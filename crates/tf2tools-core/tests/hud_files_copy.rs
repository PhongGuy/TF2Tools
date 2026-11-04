//! A HUD copy's read layers, and the read-only tf/cfg index beside them.

mod common;

use common::hud_copy::{empty_game, folder_with};
use tf2tools_core::{CopyView, HudFiles, HudFilesError};

fn text(file: Option<tf2tools_core::HudFile>) -> Option<String> {
    file.map(|f| String::from_utf8(f.into_bytes()).unwrap())
}

#[test]
fn a_copy_reads_originals_over_its_files_and_keeps_tf_cfg_apart() {
    let (game, tf) = empty_game();
    folder_with(&tf, &[("cfg/menu.txt", "from tf")]);
    let hud = folder_with(
        &game.path().join("lib/copy"),
        &[
            ("resource/ui/a.res", "edited"),
            ("cfg/menu.txt", "from the hud"),
            (".tf2tools/original/resource/ui/a.res", "pristine"),
            (".tf2tools/maker.json", "{}"),
        ],
    );
    let files = HudFiles::open_copy(&tf, &hud, &CopyView::new("myhud", [])).unwrap();
    assert_eq!(files.install_folder(), Some("myhud"));
    assert_eq!(
        text(files.read("Resource/UI/A.res").unwrap()).as_deref(),
        Some("pristine")
    );
    assert_eq!(
        text(files.read("cfg/menu.txt").unwrap()).as_deref(),
        Some("from the hud")
    );
    assert_eq!(
        text(files.read_tf_cfg("menu.txt").unwrap()).as_deref(),
        Some("from tf")
    );
    assert!(!files.contains("maker.json") && !files.contains(".tf2tools/maker.json"));
    assert!(files.in_copy("resource/ui/a.res") && files.in_copy("cfg/menu.txt"));
    assert!(!files.in_copy("resource/ui/none.res"));
}

#[test]
fn hidden_paths_leave_the_copy_layer() {
    let (game, tf) = empty_game();
    let hud = folder_with(
        &game.path().join("lib/copy"),
        &[
            ("scripts/hudanimations_tf2tools.txt", "ours"),
            ("resource/ui/kept.res", "theirs"),
        ],
    );
    let view = CopyView::new("myhud", ["Scripts/HudAnimations_TF2Tools.txt".to_owned()]);
    let files = HudFiles::open_copy(&tf, &hud, &view).unwrap();
    assert!(!files.contains("scripts/hudanimations_tf2tools.txt"));
    assert!(files.contains("resource/ui/kept.res"));
}

#[test]
fn the_cfg_index_skips_big_files_and_never_shows_other_tf_files() {
    let (game, tf) = empty_game();
    folder_with(
        &tf,
        &[
            ("resource/ui/loose.res", "x"),
            ("cfg/sub/deep.txt", "y"),
            ("cfg/big.txt", ""),
        ],
    );
    std::fs::File::options()
        .write(true)
        .open(tf.join("cfg/big.txt"))
        .unwrap()
        .set_len(8 * 1024 * 1024 + 1)
        .unwrap();
    let hud = folder_with(&game.path().join("lib/copy"), &[("info.vdf", "hud")]);
    let files = HudFiles::open_copy(&tf, &hud, &CopyView::new("myhud", [])).unwrap();
    assert!(!files.contains("resource/ui/loose.res"));
    assert_eq!(
        text(files.read_tf_cfg("sub/deep.txt").unwrap()).as_deref(),
        Some("y")
    );
    assert!(files.read_tf_cfg("big.txt").unwrap().is_none());
}

#[cfg(unix)]
#[test]
fn a_linked_cfg_folder_or_file_is_never_read() {
    let (game, tf) = empty_game();
    let outside = folder_with(&game.path().join("outside"), &[("secret.txt", "no")]);
    std::os::unix::fs::symlink(&outside, tf.join("cfg")).unwrap();
    let hud = folder_with(&game.path().join("lib/copy"), &[("info.vdf", "hud")]);
    let files = HudFiles::open_copy(&tf, &hud, &CopyView::new("myhud", [])).unwrap();
    assert!(files.read_tf_cfg("secret.txt").unwrap().is_none());

    let (game2, tf2) = empty_game();
    folder_with(&tf2, &[("cfg/real.txt", "ok")]);
    let target = folder_with(&game2.path().join("outside"), &[("secret.txt", "no")]);
    std::os::unix::fs::symlink(target.join("secret.txt"), tf2.join("cfg/link.txt")).unwrap();
    let hud2 = folder_with(&game2.path().join("lib/copy"), &[("info.vdf", "hud")]);
    let files = HudFiles::open_copy(&tf2, &hud2, &CopyView::new("myhud", [])).unwrap();
    assert!(files.read_tf_cfg("real.txt").unwrap().is_some());
    assert!(files.read_tf_cfg("link.txt").unwrap().is_none());
}

#[cfg(unix)]
#[test]
fn a_linked_copy_or_originals_folder_is_refused() {
    let (game, tf) = empty_game();
    let real = folder_with(&game.path().join("real"), &[("info.vdf", "hud")]);
    let link = game.path().join("lib/linked");
    std::fs::create_dir_all(link.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let err = HudFiles::open_copy(&tf, &link, &CopyView::new("myhud", [])).unwrap_err();
    assert!(matches!(err, HudFilesError::LinkedFolder { .. }));
    let hud = folder_with(&game.path().join("lib/copy"), &[("info.vdf", "hud")]);
    std::fs::create_dir_all(hud.join(".tf2tools")).unwrap();
    std::os::unix::fs::symlink(&real, hud.join(".tf2tools/original")).unwrap();
    let err = HudFiles::open_copy(&tf, &hud, &CopyView::new("myhud", [])).unwrap_err();
    assert!(matches!(err, HudFilesError::LinkedFolder { .. }));
}

#[test]
fn stock_files_are_no_copy() {
    let files = HudFiles::from_memory(vec![vec![("resource/ui/a.res".to_owned(), b"x".to_vec())]]);
    assert_eq!(files.install_folder(), None);
    assert!(!files.in_copy("resource/ui/a.res"));
    let marked = HudFiles::from_memory(vec![vec![("resource/ui/a.res".to_owned(), b"x".to_vec())]])
        .with_install_folder("myhud");
    assert_eq!(marked.install_folder(), Some("myhud"));
    assert!(marked.in_copy("resource/ui/a.res"));
}

#[cfg(unix)]
#[test]
fn a_linked_metadata_folder_is_refused() {
    let (game, tf) = empty_game();
    let real = folder_with(
        &game.path().join("real"),
        &[("original/resource/ui/a.res", "x")],
    );
    let hud = folder_with(&game.path().join("lib/copy"), &[("info.vdf", "hud")]);
    std::os::unix::fs::symlink(&real, hud.join(".tf2tools")).unwrap();
    let err = HudFiles::open_copy(&tf, &hud, &CopyView::new("myhud", [])).unwrap_err();
    assert!(matches!(err, HudFilesError::LinkedFolder { .. }));
}

#[cfg(unix)]
#[test]
fn a_file_swapped_for_a_link_after_indexing_is_not_read() {
    let (game, tf) = empty_game();
    let secret =
        folder_with(&game.path().join("outside"), &[("secret.res", "no")]).join("secret.res");
    folder_with(&tf, &[("cfg/menu.txt", "ok")]);
    let hud = folder_with(
        &game.path().join("lib/copy"),
        &[("resource/ui/a.res", "ok")],
    );
    let files = HudFiles::open_copy(&tf, &hud, &CopyView::new("myhud", [])).unwrap();
    std::fs::remove_file(hud.join("resource/ui/a.res")).unwrap();
    std::os::unix::fs::symlink(&secret, hud.join("resource/ui/a.res")).unwrap();
    std::fs::remove_file(tf.join("cfg/menu.txt")).unwrap();
    std::os::unix::fs::symlink(&secret, tf.join("cfg/menu.txt")).unwrap();
    assert!(files.read("resource/ui/a.res").is_err());
    assert!(files.read_tf_cfg("menu.txt").is_err());
}

#[cfg(unix)]
#[test]
fn a_metadata_folder_swapped_for_a_link_after_opening_is_not_read() {
    for swapped in [".tf2tools", ".tf2tools/original"] {
        let (game, tf) = empty_game();
        let hud = folder_with(
            &game.path().join("lib/copy"),
            &[
                ("resource/ui/a.res", "edited"),
                (".tf2tools/original/resource/ui/a.res", "pristine"),
            ],
        );
        let files = HudFiles::open_copy(&tf, &hud, &CopyView::new("myhud", [])).unwrap();
        let moved = game.path().join("moved");
        std::fs::rename(hud.join(swapped), &moved).unwrap();
        std::os::unix::fs::symlink(&moved, hud.join(swapped)).unwrap();
        assert!(files.read("resource/ui/a.res").is_err(), "{swapped}");
    }
}
