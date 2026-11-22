//! Finding a demo's map where the game loads maps, in MapIndex's order and ignoring case.

use std::path::PathBuf;

use tempfile::TempDir;
use tf2tools_core::{Tf2Install, find_installed_map};

fn game() -> (TempDir, Tf2Install) {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("tf")).unwrap();
    let install = Tf2Install::from_path(tmp.path()).unwrap();
    (tmp, install)
}

fn put(install: &Tf2Install, rel: &str) -> PathBuf {
    let path = install.tf_dir().join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"VBSP").unwrap();
    path
}

#[test]
fn tf_maps_comes_first() {
    let (_tmp, tf2) = game();
    let own = put(&tf2, "maps/cp_a.bsp");
    put(&tf2, "download/maps/cp_a.bsp");
    put(&tf2, "custom/aaa/maps/cp_a.bsp");
    assert_eq!(find_installed_map(&tf2, "cp_a"), Some(own));
}

#[test]
fn downloads_come_before_custom_folders() {
    let (_tmp, tf2) = game();
    let server = put(&tf2, "download/maps/cp_a.bsp");
    put(&tf2, "custom/aaa/maps/cp_a.bsp");
    assert_eq!(find_installed_map(&tf2, "cp_a"), Some(server));
}

#[test]
fn custom_folders_go_by_name_and_their_maps_dir_by_any_case() {
    let (_tmp, tf2) = game();
    put(&tf2, "custom/zeta/maps/cp_b.bsp");
    let alpha = put(&tf2, "custom/alpha/Maps/cp_b.bsp");
    assert_eq!(find_installed_map(&tf2, "cp_b"), Some(alpha));
}

#[test]
fn the_stem_matches_in_any_case_and_the_real_path_comes_back() {
    let (_tmp, tf2) = game();
    let real = put(&tf2, "maps/CP_Mixed.BSP");
    assert_eq!(find_installed_map(&tf2, "cp_mixed"), Some(real));
}

#[test]
fn a_missing_map_or_another_kind_of_file_is_none() {
    let (_tmp, tf2) = game();
    put(&tf2, "maps/cp_c.bsp.bz2");
    put(&tf2, "maps/cp_c.nav");
    std::fs::create_dir_all(tf2.tf_dir().join("maps/cp_d.bsp")).unwrap();
    assert_eq!(find_installed_map(&tf2, "cp_c"), None);
    assert_eq!(find_installed_map(&tf2, "cp_d"), None);
    assert_eq!(find_installed_map(&tf2, "cp_none"), None);
}

#[test]
fn an_unsafe_name_is_never_looked_up() {
    let (_tmp, tf2) = game();
    put(&tf2, "maps/x.bsp");
    put(&tf2, "x.bsp");
    for name in ["../x", "maps/x", "", ".x", "../maps/x"] {
        assert_eq!(find_installed_map(&tf2, name), None, "{name:?}");
    }
}
