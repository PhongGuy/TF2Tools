//! The bundled setting pictures: which values have one, and what a row shows of them.

use crate::support;

use std::path::PathBuf;

use tf2tools_app::pages::graphics::model::preset_label;
use tf2tools_app::pages::graphics::pictures::{Compare, PictureIndex, RowPictures, row_pictures};
use tf2tools_app::pages::graphics::view::row;
use tf2tools_app::remote_images::local_image_url;
use tf2tools_core::{Choices, ModuleChoice, Preset};

const MANIFEST: &str = r#"{
  "version": 3, "mastercomfig": "9.100.1", "size": [960, 540], "map": "itemtest", "base": {},
  "settings": [
    {"module": "lod", "crop": {"x": 0, "y": 0, "w": 480, "h": 270},
     "values": [{"value": "low"}, {"value": "medium"}, {"value": "high", "same_as": "medium"}, {"value": "ultra"}]},
    {"module": "decals", "values": [{"value": "off"}, {"value": "low"}]},
    {"module": "vsync", "no_picture": "performance-only", "values": [{"value": "off"}, {"value": "on"}]}
  ]
}"#;

/// A pictures folder for MANIFEST; lod/ultra and decals/low have no file, so they are placeholders.
fn folder() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("manifest.json"), MANIFEST).unwrap();
    for file in [
        "lod/low.webp",
        "lod/low.zoom.webp",
        "lod/medium.webp",
        "lod/medium.zoom.webp",
        "decals/off.webp",
        "placeholder.webp",
    ] {
        let path = tmp.path().join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"x").unwrap();
    }
    tmp
}

fn value(v: &str) -> ModuleChoice {
    ModuleChoice::Value(v.to_owned())
}

fn bundled() -> PathBuf {
    support::repo_asset_root().setting_pictures_dir()
}

fn pictures_of(
    preset: Preset,
    saved: &[(&str, ModuleChoice)],
    module: &str,
    zoom: bool,
) -> Option<RowPictures> {
    let catalog = support::full_module_catalog();
    let mut choices = Choices::new(preset);
    for (name, choice) in saved {
        choices.set(&catalog, name, choice.clone());
    }
    let row = row(&catalog, &choices, catalog.module(module).unwrap());
    row_pictures(
        &PictureIndex::open(&bundled()),
        &row,
        &preset_label(preset),
        zoom,
    )
}

#[test]
fn only_real_pictures_count_and_a_shared_look_borrows_the_other_file() {
    let tmp = folder();
    let dir = tmp.path();
    let index = PictureIndex::open(dir);
    let low = index.get("lod", "low").unwrap();
    assert_eq!(low.full, dir.join("lod/low.webp"));
    assert_eq!(low.zoom, Some(dir.join("lod/low.zoom.webp")));
    let high = index.get("lod", "high").unwrap();
    assert_eq!(high.full, dir.join("lod/medium.webp"));
    assert_eq!(high.zoom, Some(dir.join("lod/medium.zoom.webp")));
    assert_eq!(
        index.get("lod", "ultra"),
        None,
        "a placeholder is no picture"
    );
    assert_eq!(
        index.get("decals", "off").unwrap().zoom,
        None,
        "no crop, no zoom"
    );
    assert_eq!(index.get("decals", "low"), None);
    assert_eq!(index.get("vsync", "on"), None);
    assert_eq!(index.get("shadows", "low"), None, "outside the manifest");
    assert!(index.has_pictures("lod") && index.has_pictures("decals"));
    assert!(!index.has_pictures("vsync") && !index.has_pictures("shadows"));
    assert!(index.has_zoom("lod") && !index.has_zoom("decals"));
    let path = dir.join("lod/low.webp");
    assert_eq!(index.url(&path), local_image_url(&path, "3"));
}

#[test]
fn a_folder_that_cannot_be_read_has_no_pictures() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(!PictureIndex::open(&tmp.path().join("missing")).has_pictures("lod"));
    std::fs::write(tmp.path().join("manifest.json"), "{").unwrap();
    let broken = PictureIndex::open(tmp.path());
    assert!(!broken.has_pictures("lod"));
    assert_eq!(broken.get("lod", "low"), None);
}

#[test]
fn the_preset_s_own_value_shows_one_picture_and_the_strip_tags_it() {
    let dir = bundled();
    let lod = pictures_of(Preset::Medium, &[], "lod", false).unwrap();
    assert_eq!(lod.thumb, Some(dir.join("lod/medium.webp")));
    assert_eq!(
        lod.compare,
        Compare::Same {
            picture: dir.join("lod/medium.webp"),
            pill: "Medium, as Medium: Medium".into()
        }
    );
    let strip: Vec<(&str, &str, bool, bool)> = lod
        .strip
        .iter()
        .map(|p| (p.value.as_str(), p.label.as_ref(), p.preset, p.chosen))
        .collect();
    assert_eq!(
        strip,
        [
            ("low", "Low", false, false),
            ("medium", "Medium", true, true),
            ("high", "High", false, false),
            ("ultra", "Ultra", false, false)
        ]
    );
    assert_eq!(lod.strip[0].picture, dir.join("lod/low.webp"));
    assert!(lod.has_zoom);
}

#[test]
fn a_changed_value_wipes_between_the_preset_s_picture_and_yours() {
    let dir = bundled();
    let lod = pictures_of(Preset::Medium, &[("lod", value("high"))], "lod", false).unwrap();
    assert_eq!(lod.thumb, Some(dir.join("lod/high.webp")));
    let wipe = |preset: &str, yours: &str| Compare::Wipe {
        preset: dir.join(preset),
        yours: dir.join(yours),
        preset_pill: "Medium: Medium".into(),
        yours_pill: "Yours: High".into(),
    };
    assert_eq!(lod.compare, wipe("lod/medium.webp", "lod/high.webp"));
    let chosen: Vec<&str> = lod
        .strip
        .iter()
        .filter(|p| p.chosen)
        .map(|p| p.value.as_str())
        .collect();
    assert_eq!(chosen, ["high"]);
    let zoomed = pictures_of(Preset::Medium, &[("lod", value("high"))], "lod", true).unwrap();
    assert_eq!(
        zoomed.compare,
        wipe("lod/medium.zoom.webp", "lod/high.zoom.webp")
    );
    assert_eq!(zoomed.thumb, lod.thumb, "thumbnails stay whole");
    assert_eq!(zoomed.strip, lod.strip, "the strip stays whole");
    let decals = pictures_of(Preset::Medium, &[], "decals", true).unwrap();
    assert!(!decals.has_zoom);
    assert_eq!(
        decals.compare,
        Compare::Same {
            picture: dir.join("decals/low.webp"),
            pill: "Low, as Medium: Low".into()
        },
        "a module without crops stays whole under zoom"
    );
}

#[test]
fn a_side_without_a_picture_leaves_the_other_alone() {
    let dir = bundled();
    // Custom leaves lod to TF2, so only the player's pick has a picture.
    let custom = pictures_of(Preset::Custom, &[("lod", value("high"))], "lod", false).unwrap();
    assert_eq!(
        custom.compare,
        Compare::Single {
            picture: dir.join("lod/high.webp"),
            pill: "Yours: High".into(),
            yours: true
        }
    );
    assert!(custom.strip.iter().all(|p| !p.preset));
    // A saved "Let TF2 decide" leaves only the preset's side.
    let unset = pictures_of(
        Preset::Medium,
        &[("lod", ModuleChoice::NotSet)],
        "lod",
        false,
    )
    .unwrap();
    assert_eq!(unset.thumb, None);
    assert_eq!(
        unset.compare,
        Compare::Single {
            picture: dir.join("lod/medium.webp"),
            pill: "Medium: Medium".into(),
            yours: false
        }
    );
}

#[test]
fn under_custom_nothing_carries_the_preset_tag_and_unpictured_modules_have_none() {
    let lod = pictures_of(Preset::Custom, &[], "lod", false).unwrap();
    assert_eq!(
        (lod.thumb, lod.compare, lod.strip.len()),
        (None, Compare::None, 4)
    );
    assert!(lod.strip.iter().all(|p| !p.preset && !p.chosen));
    for module in ["vsync", "motion_blur", "fpscap", "hud_achievement"] {
        assert_eq!(
            pictures_of(Preset::Medium, &[], module, false),
            None,
            "{module}"
        );
    }
}
