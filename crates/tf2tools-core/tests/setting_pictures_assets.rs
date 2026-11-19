//! Every committed setting picture: present, one size, and named by the manifest.

use std::collections::BTreeSet;
use std::path::PathBuf;

use tf2tools_core::SettingPictures;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/setting-pictures")
}

/// Every file the manifest names: each shot value's picture, and its zoom when the setting has a crop.
fn named(pics: &SettingPictures) -> BTreeSet<PathBuf> {
    let mut named = BTreeSet::new();
    for s in pics
        .manifest()
        .settings
        .iter()
        .filter(|s| s.no_picture.is_none())
    {
        for v in s.values.iter().filter(|v| v.same_as.is_none()) {
            named.insert(pics.file_for(&s.module, &v.value));
            if s.crop.is_some() {
                named.insert(pics.zoom_file_for(&s.module, &v.value));
            }
        }
    }
    named
}

#[test]
fn every_shot_value_has_a_picture_of_the_manifest_size() {
    let pics = SettingPictures::open(&dir()).unwrap();
    let [w, h] = pics.manifest().size;
    for setting in pics
        .manifest()
        .settings
        .iter()
        .filter(|s| s.no_picture.is_none())
    {
        for value in setting.values.iter().filter(|v| v.same_as.is_none()) {
            let path = pics.file_for(&setting.module, &value.value);
            let img = image::open(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert_eq!((img.width(), img.height()), (w, h), "{}", path.display());
            // A cropped setting also carries a zoom at twice the crop's size.
            if let Some(crop) = setting.crop {
                let zoom = pics.zoom_file_for(&setting.module, &value.value);
                let img = image::open(&zoom).unwrap_or_else(|e| panic!("{}: {e}", zoom.display()));
                assert_eq!(
                    (img.width(), img.height()),
                    (2 * crop.w, 2 * crop.h),
                    "{}",
                    zoom.display()
                );
            }
        }
    }
}

#[test]
fn the_folder_holds_only_pictures_the_manifest_names() {
    let pics = SettingPictures::open(&dir()).unwrap();
    let named = named(&pics);
    for entry in walkdir::WalkDir::new(dir()).min_depth(2) {
        let path = entry.unwrap().into_path();
        if path.is_file() {
            assert!(named.contains(&path), "unexpected {}", path.display());
        }
    }
}
