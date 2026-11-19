//! The setting pictures manifest and the lookup the Graphics page uses.

use std::fs;
use std::path::Path;

use tf2tools_core::{
    PictureManifest, Reload, SettingPicture, SettingPictures, SettingPicturesError,
};

const MANIFEST: &str = r#"{
  "version": 1,
  "mastercomfig": "9.100.1",
  "size": [960, 540],
  "map": "ctf_2fort",
  "base": { "shadows": "high", "gibs": "low" },
  "settings": [
    { "module": "shadows", "setpos": [-1888.0, -1280.0, 321.0], "setang": [0.0, -30.0, 0.0],
      "setup": ["thirdperson"], "values": [{ "value": "off" }, { "value": "low" }, { "value": "high" }] },
    { "module": "gibs", "reload": "map", "pause": false, "steps": [{ "run": "kill", "wait_ms": 400 }],
      "values": [{ "value": "off" }, { "value": "low" }, { "value": "high", "same_as": "low", "reason": "same-in-still" }] },
    { "module": "ragdolls", "values": [{ "value": "medium", "same_as": "high" }, { "value": "high", "same_as": "off" }, { "value": "off" }] },
    { "module": "vsync", "no_picture": "performance-only", "values": [{ "value": "off" }, { "value": "on" }] }
  ]
}"#;

/// A pictures folder holding the manifest above and the given picture files.
fn folder(files: &[&str]) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("manifest.json"), MANIFEST).unwrap();
    for file in files {
        let path = tmp.path().join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"webp").unwrap();
    }
    tmp
}

fn open(dir: &Path) -> SettingPictures {
    SettingPictures::open(dir).unwrap()
}

#[test]
fn the_manifest_parses_with_its_defaults() {
    let m = PictureManifest::parse(MANIFEST.as_bytes()).unwrap();
    assert_eq!(m.mastercomfig, "9.100.1");
    assert_eq!(m.size, [960, 540]);
    let shadows = m.setting("shadows").unwrap();
    assert_eq!(shadows.reload, Reload::None);
    assert!(shadows.pause);
    assert_eq!(shadows.setpos, Some([-1888.0, -1280.0, 321.0]));
    let gibs = m.setting("gibs").unwrap();
    assert_eq!(gibs.reload, Reload::Map);
    assert!(!gibs.pause);
    assert_eq!(gibs.steps[0].run, "kill");
    assert_eq!(gibs.steps[0].wait_ms, 400);
    assert!(m.setting("lod").is_none());
}

#[test]
fn an_unknown_field_is_a_format_error() {
    let bad = MANIFEST.replace("\"setup\"", "\"set_up\"");
    let err = PictureManifest::parse(bad.as_bytes()).unwrap_err();
    assert!(
        matches!(err, SettingPicturesError::Format { .. }),
        "{err:?}"
    );
}

#[test]
fn a_missing_manifest_is_a_read_error() {
    let tmp = tempfile::tempdir().unwrap();
    let err = SettingPictures::open(tmp.path()).err().unwrap();
    assert!(matches!(err, SettingPicturesError::Read { .. }), "{err:?}");
}

#[test]
fn a_shot_value_gives_its_picture() {
    let tmp = folder(&["shadows/low.webp"]);
    let pics = open(tmp.path());
    assert_eq!(
        pics.picture("shadows", "low"),
        SettingPicture::Picture {
            path: tmp.path().join("shadows").join("low.webp")
        }
    );
}

#[test]
fn a_missing_file_falls_back_to_the_placeholder() {
    let tmp = folder(&[]);
    let pics = open(tmp.path());
    assert_eq!(
        pics.picture("shadows", "low"),
        SettingPicture::Placeholder {
            path: tmp.path().join("placeholder.webp")
        }
    );
}

#[test]
fn an_unknown_module_or_value_gives_the_placeholder() {
    let tmp = folder(&["shadows/low.webp"]);
    let pics = open(tmp.path());
    let placeholder = SettingPicture::Placeholder {
        path: pics.placeholder(),
    };
    assert_eq!(pics.picture("lod", "low"), placeholder);
    assert_eq!(pics.picture("shadows", "ultra"), placeholder);
}

#[test]
fn a_same_look_value_points_at_the_other_value() {
    let tmp = folder(&["gibs/low.webp"]);
    let pics = open(tmp.path());
    assert_eq!(
        pics.picture("gibs", "high"),
        SettingPicture::SameAs {
            path: tmp.path().join("gibs").join("low.webp"),
            value: "low".to_owned(),
            reason: "same-in-still".to_owned(),
        }
    );
}

#[test]
fn a_same_look_value_without_a_reason_reads_as_same_in_still() {
    let tmp = folder(&["ragdolls/high.webp"]);
    let pics = open(tmp.path());
    assert_eq!(
        pics.picture("ragdolls", "medium"),
        SettingPicture::SameAs {
            path: tmp.path().join("ragdolls").join("high.webp"),
            value: "high".to_owned(),
            reason: "same-in-still".to_owned(),
        }
    );
}

#[test]
fn a_same_look_value_whose_target_has_no_file_gives_the_placeholder() {
    let tmp = folder(&[]);
    let pics = open(tmp.path());
    assert_eq!(
        pics.picture("gibs", "high"),
        SettingPicture::Placeholder {
            path: pics.placeholder()
        }
    );
}

#[test]
fn a_setting_without_pictures_says_why() {
    let tmp = folder(&[]);
    let pics = open(tmp.path());
    assert_eq!(
        pics.picture("vsync", "on"),
        SettingPicture::NoPicture {
            reason: "performance-only".to_owned()
        }
    );
}

#[test]
fn file_for_names_the_module_folder_and_value() {
    let tmp = folder(&[]);
    let pics = open(tmp.path());
    assert_eq!(
        pics.file_for("3dsky", "on"),
        tmp.path().join("3dsky").join("on.webp")
    );
    assert_eq!(pics.manifest().map, "ctf_2fort");
}
