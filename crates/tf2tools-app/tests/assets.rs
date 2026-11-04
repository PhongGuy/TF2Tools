use std::fs;
use std::path::{Path, PathBuf};

use tf2tools_app::assets::{AssetRoot, BACKGROUNDS};
use tf2tools_core::WeaponSlot;

mod support;

/// Makes `root` a valid asset root and returns it.
fn valid(root: &Path) -> PathBuf {
    let dir = root.join("crosshairs");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("crosshairs.json"), "[]").unwrap();
    root.to_path_buf()
}

#[test]
fn the_env_override_wins() {
    let tmp = tempfile::tempdir().unwrap();
    let env = valid(&tmp.path().join("env"));
    let exe = tmp.path().join("bin");
    valid(&exe.join("assets"));
    let dev = valid(&tmp.path().join("dev"));
    let root = AssetRoot::resolve_from(Some(env.clone()), Some(exe), Some(dev)).unwrap();
    assert_eq!(root.path(), env.as_path());
}

#[test]
fn assets_beside_the_binary_beat_the_dev_tree() {
    let tmp = tempfile::tempdir().unwrap();
    let exe = tmp.path().join("bin");
    let beside = valid(&exe.join("assets"));
    let dev = valid(&tmp.path().join("dev"));
    let root = AssetRoot::resolve_from(None, Some(exe), Some(dev)).unwrap();
    assert_eq!(root.path(), beside.as_path());
}

#[test]
fn the_dev_tree_is_the_last_resort() {
    let tmp = tempfile::tempdir().unwrap();
    let dev = valid(&tmp.path().join("dev"));
    let root =
        AssetRoot::resolve_from(None, Some(tmp.path().join("bin")), Some(dev.clone())).unwrap();
    assert_eq!(root.path(), dev.as_path());
}

#[test]
fn a_candidate_without_the_manifest_is_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    let env = tmp.path().join("env");
    fs::create_dir_all(env.join("crosshairs")).unwrap();
    let dev = valid(&tmp.path().join("dev"));
    let root = AssetRoot::resolve_from(Some(env), None, Some(dev.clone())).unwrap();
    assert_eq!(root.path(), dev.as_path());
}

#[test]
fn a_miss_names_every_place_searched() {
    let tmp = tempfile::tempdir().unwrap();
    let env = tmp.path().join("env");
    let exe = tmp.path().join("bin");
    let dev = tmp.path().join("dev");
    let err = AssetRoot::resolve_from(Some(env.clone()), Some(exe.clone()), Some(dev.clone()))
        .unwrap_err();
    let lib = tmp.path().join("lib/tf2tools/assets");
    assert_eq!(
        err.searched,
        vec![env.clone(), exe.join("assets"), lib.clone(), dev.clone()]
    );
    let text = err.to_string();
    for place in [&env, &exe.join("assets"), &lib, &dev] {
        assert!(text.contains(&place.display().to_string()), "{text}");
    }
}

#[test]
fn the_repo_assets_resolve_and_hold_the_crosshair_manifest() {
    let root = AssetRoot::resolve_from(None, None, Some(support::repo_assets())).unwrap();
    assert!(root.crosshairs_dir().join("crosshairs.json").is_file());
}

#[test]
fn the_repo_assets_hold_the_backgrounds_and_slot_icons() {
    let root = support::repo_asset_root();
    assert_eq!(BACKGROUNDS, ["2fort", "badlands", "process"]);
    for name in BACKGROUNDS {
        let path = root.background(name);
        assert_eq!(
            path,
            root.path().join("backgrounds").join(format!("{name}.png"))
        );
        assert!(path.is_file(), "{}", path.display());
    }
    for (slot, file) in [
        (WeaponSlot::Primary, "primary.png"),
        (WeaponSlot::Secondary, "secondary.png"),
        (WeaponSlot::Melee, "melee.png"),
    ] {
        let path = root.slot_icon(slot);
        assert_eq!(path, root.path().join("weapon-icons").join(file));
        assert!(path.is_file(), "{}", path.display());
    }
}

#[test]
fn the_packaged_lib_dir_is_found_from_usr_bin() {
    let tmp = tempfile::tempdir().unwrap();
    let usr = tmp.path().join("usr");
    let lib = valid(&usr.join("lib/tf2tools/assets"));
    let dev = valid(&tmp.path().join("dev"));
    let root = AssetRoot::resolve_from(None, Some(usr.join("bin")), Some(dev)).unwrap();
    assert_eq!(root.path(), lib.as_path());
}

#[test]
fn assets_beside_the_binary_beat_the_lib_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let usr = tmp.path().join("usr");
    let beside = valid(&usr.join("bin/assets"));
    valid(&usr.join("lib/tf2tools/assets"));
    let root = AssetRoot::resolve_from(None, Some(usr.join("bin")), None).unwrap();
    assert_eq!(root.path(), beside.as_path());
}

#[test]
fn an_exe_dir_without_a_parent_adds_no_lib_candidate() {
    let err = AssetRoot::resolve_from(None, Some(PathBuf::from("/")), None).unwrap_err();
    assert_eq!(err.searched, vec![PathBuf::from("/assets")]);
}
