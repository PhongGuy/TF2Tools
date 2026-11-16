//! Library crosshairs read the same packed as loose: previews, drawings, apply and the scan.

use crate::common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{
    LibraryEnv, builtin_catalog, crosshair_env, import_png_as, tree, write_file, zip_bytes,
};
use tf2tools_core::{
    CrosshairEntry, CrosshairRef, InstallError, LibraryError, LibraryKind, PaintTarget, Provenance,
    RgbaImage, SkipReason,
};

fn bundled_vtf() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/crosshairs/vtf/Brackets_TobYZ.vtf")
}

fn import_vtf(env: &LibraryEnv, name: &str) -> CrosshairEntry {
    let src = write_file(
        &env.tmp.path().join(format!("in/{name}.vtf")),
        &fs::read(bundled_vtf()).unwrap(),
    );
    env.lib
        .import_crosshair(&src, name, Provenance::Local)
        .unwrap()
}

fn painted(env: &LibraryEnv, name: &str) -> CrosshairEntry {
    let pixels = (0..32 * 32)
        .flat_map(|i: u32| {
            [
                i as u8,
                (i / 3) as u8,
                9,
                if i.is_multiple_of(4) { 0 } else { 255 },
            ]
        })
        .collect();
    let drawing = RgbaImage::from_pixels(32, 32, pixels).unwrap();
    env.lib
        .save_painted_crosshair(&PaintTarget::New, name, &drawing)
        .unwrap()
}

fn dir_of(env: &LibraryEnv, entry: &CrosshairEntry) -> PathBuf {
    env.dir
        .library_dir(LibraryKind::Crosshairs)
        .join(entry.folder_name())
}

/// Packs `entry` with the seam and checks only the zip is left in its folder.
fn pack(env: &LibraryEnv, entry: &CrosshairEntry) {
    env.lib.pack_crosshair_for_tests(entry.id()).unwrap();
    assert_eq!(
        tree(&dir_of(env, entry)),
        ["copy.zip"],
        "only the zip stays in the folder"
    );
}

fn library(entry: &CrosshairEntry) -> CrosshairRef {
    CrosshairRef::Library(entry.id().clone())
}

/// Every file of the managed folder with its bytes.
fn managed(env: &LibraryEnv) -> Vec<(String, Vec<u8>)> {
    let root = env.tf2.custom_dir().join("tf2tools-crosshairs");
    tree(&root)
        .into_iter()
        .map(|rel| {
            let bytes = fs::read(root.join(&rel)).unwrap();
            (rel, bytes)
        })
        .collect()
}

#[test]
fn a_packed_crosshair_previews_and_draws_as_the_loose_one() {
    let env = crosshair_env();
    let builtins = builtin_catalog();
    for entry in [
        import_vtf(&env, "Brackets"),
        import_png_as(&env, "Dot", Provenance::Local),
    ] {
        let preview = env.lib.crosshair_preview(&library(&entry), &builtins);
        let drawing = env.lib.crosshair_drawing(&library(&entry), &builtins);
        pack(&env, &entry);
        assert_eq!(
            env.lib
                .crosshair_preview(&library(&entry), &builtins)
                .unwrap(),
            preview.unwrap()
        );
        assert_eq!(
            env.lib
                .crosshair_drawing(&library(&entry), &builtins)
                .unwrap(),
            drawing.unwrap()
        );
    }
}

#[test]
fn a_packed_library_applies_the_same_files_and_is_then_in_game() {
    let env = crosshair_env();
    let builtins = builtin_catalog();
    let brackets = import_vtf(&env, "Brackets");
    let dot = import_png_as(&env, "Dot", Provenance::Local);
    env.lib
        .assign("tf_weapon_rocketlauncher", &library(&brackets))
        .unwrap();
    env.lib
        .assign("tf_weapon_scattergun", &library(&dot))
        .unwrap();
    env.lib
        .assign("tf_weapon_shotgun_primary", &library(&dot))
        .unwrap();

    let loose = env.lib.apply_crosshairs(&env.tf2, &builtins).unwrap();
    let written = managed(&env);
    assert_eq!(written.len(), 7, "two VTFs, two VMTs, three scripts");
    pack(&env, &brackets);
    pack(&env, &dot);
    let packed = env.lib.apply_crosshairs(&env.tf2, &builtins).unwrap();
    assert_eq!(packed, loose);
    assert!(packed.skipped().is_empty());
    assert_eq!(managed(&env), written);
    assert_eq!(
        env.lib.crosshairs_not_in_game(&env.tf2, &builtins).unwrap(),
        Vec::<String>::new()
    );
}

#[test]
fn a_packed_crosshair_without_its_vtf_is_skipped_as_missing() {
    let env = crosshair_env();
    let builtins = builtin_catalog();
    let dot = import_png_as(&env, "Dot", Provenance::Local);
    env.lib
        .assign("tf_weapon_scattergun", &library(&dot))
        .unwrap();
    pack(&env, &dot);
    // The zip keeps only the VMT, so the copy is packed but has no texture.
    let vmt = format!("{}.vmt", dot.folder_name());
    write_file(
        &dir_of(&env, &dot).join("copy.zip"),
        &zip_bytes(&[(vmt.as_str(), b"\"UnlitGeneric\" { }")]),
    );

    let report = env.lib.apply_crosshairs(&env.tf2, &builtins).unwrap();
    assert!(report.applied().is_empty());
    let reasons: Vec<SkipReason> = report.skipped().iter().map(|s| s.reason()).collect();
    assert_eq!(reasons, [SkipReason::MissingLibraryCrosshair]);
    assert!(
        env.lib
            .crosshair_preview(&library(&dot), &builtins)
            .is_err_and(|e| matches!(e, LibraryError::NotFound { .. }))
    );
}

#[test]
fn a_hand_packed_painted_crosshair_still_opens_in_the_maker() {
    let env = crosshair_env();
    let entry = painted(&env, "Mine");
    let loose = env.lib.painted_source(entry.id()).unwrap();
    pack(&env, &entry);
    let packed = env.lib.painted_source(entry.id()).unwrap();
    assert_eq!(packed, loose);
    assert!(!packed.recovered());
}

/// A packed crosshair assigned to the scattergun whose `copy.zip` is then overwritten with junk.
fn damaged(env: &LibraryEnv) -> PathBuf {
    let dot = import_png_as(env, "Dot", Provenance::Local);
    env.lib
        .assign("tf_weapon_scattergun", &library(&dot))
        .unwrap();
    pack(env, &dot);
    write_file(&dir_of(env, &dot).join("copy.zip"), b"not a zip at all")
}

#[test]
fn applying_a_damaged_zip_is_an_io_error_naming_it() {
    let env = crosshair_env();
    let zip = damaged(&env);
    let result = env.lib.apply_crosshairs(&env.tf2, &builtin_catalog());
    assert!(
        matches!(&result, Err(InstallError::Io { path, .. }) if *path == zip),
        "{result:?}"
    );
}

#[test]
fn the_scan_reports_a_damaged_zip_as_an_io_error() {
    let env = crosshair_env();
    let zip = damaged(&env);
    let result = env.lib.crosshairs_not_in_game(&env.tf2, &builtin_catalog());
    assert!(
        matches!(&result, Err(LibraryError::Io { path, .. }) if *path == zip),
        "{result:?}"
    );
}
