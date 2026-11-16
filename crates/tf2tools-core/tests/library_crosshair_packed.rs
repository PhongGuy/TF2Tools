//! Library crosshairs read the same packed as loose: previews, drawings, apply and the scan.

use crate::common;

use std::fs;
use std::path::{Path, PathBuf};

use common::packed::crosshair_zip_files;
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

/// Unpacks `entry`, which import packed, so a test can read its loose form first.
fn loose(env: &LibraryEnv, entry: &CrosshairEntry) {
    env.lib.unpack_crosshair_for_tests(entry.id()).unwrap();
    assert!(
        !dir_of(env, entry).join("copy.zip").exists(),
        "the copy is loose after unpacking"
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
        loose(&env, &entry);
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
    loose(&env, &brackets);
    loose(&env, &dot);
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
fn the_scan_counts_a_crosshair_with_a_damaged_zip_as_waiting() {
    let env = crosshair_env();
    let builtins = builtin_catalog();
    let dot = import_png_as(&env, "Dot", Provenance::Local);
    env.lib
        .assign("tf_weapon_scattergun", &library(&dot))
        .unwrap();
    pack(&env, &dot);
    env.lib.apply_crosshairs(&env.tf2, &builtins).unwrap();
    assert_eq!(
        env.lib.crosshairs_not_in_game(&env.tf2, &builtins).unwrap(),
        Vec::<String>::new()
    );
    // The game still has the crosshair, but the library copy no longer matches it.
    write_file(&dir_of(&env, &dot).join("copy.zip"), b"not a zip at all");
    assert_eq!(
        env.lib.crosshairs_not_in_game(&env.tf2, &builtins).unwrap(),
        ["tf_weapon_scattergun"]
    );
}

#[test]
fn an_imported_crosshair_is_packed_with_the_files_the_loose_import_wrote() {
    let env = crosshair_env();
    let dot = import_png_as(&env, "Dot", Provenance::Local);
    assert_eq!(tree(&dir_of(&env, &dot)), ["copy.zip"]);
    let packed = crosshair_zip_files(&env.dir, dot.folder_name());
    let folder = dot.folder_name();
    let names: Vec<&str> = packed.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        names,
        [
            format!("{folder}.vmt"),
            format!("{folder}.vtf"),
            s("preview.png")
        ]
    );
    let source = fs::read(env.tmp.path().join("in/Dot.png")).unwrap();
    assert_eq!(packed[2].1, source, "the PNG is kept as the preview");

    // Unpacked, the folder holds exactly the bytes the zip carried.
    env.lib.unpack_crosshair_for_tests(dot.id()).unwrap();
    let loose: Vec<(String, Vec<u8>)> = tree(&dir_of(&env, &dot))
        .into_iter()
        .map(|rel| {
            let bytes = fs::read(dir_of(&env, &dot).join(&rel)).unwrap();
            (rel, bytes)
        })
        .collect();
    assert_eq!(loose, packed);
}

#[test]
fn an_import_whose_pack_fails_stays_loose_and_works() {
    let mut env = crosshair_env();
    env.lib.set_pack_rename(common::packed::rename_then_damage);
    let builtins = builtin_catalog();
    let dot = import_png_as(&env, "Dot", Provenance::Local);
    let folder = dot.folder_name();
    assert_eq!(
        tree(&dir_of(&env, &dot)),
        [
            format!("{folder}.vmt"),
            format!("{folder}.vtf"),
            s("preview.png")
        ]
    );
    assert!(env.lib.crosshair_preview(&library(&dot), &builtins).is_ok());
    env.lib
        .assign("tf_weapon_scattergun", &library(&dot))
        .unwrap();
    let report = env.lib.apply_crosshairs(&env.tf2, &builtins).unwrap();
    assert!(report.skipped().is_empty());
    assert_eq!(report.applied().len(), 1);
}

#[test]
fn deleting_a_packed_crosshair_removes_its_folder() {
    let env = crosshair_env();
    let dot = import_png_as(&env, "Dot", Provenance::Local);
    assert!(dir_of(&env, &dot).join("copy.zip").is_file());
    env.lib.delete_crosshair(dot.id()).unwrap();
    assert!(!dir_of(&env, &dot).exists());
    assert!(env.lib.list_crosshairs().unwrap().is_empty());
}

/// The loose files of `entry`'s folder with their bytes, sorted.
fn loose_files(env: &LibraryEnv, entry: &CrosshairEntry) -> Vec<(String, Vec<u8>)> {
    let dir = dir_of(env, entry);
    tree(&dir)
        .into_iter()
        .map(|rel| {
            let bytes = fs::read(dir.join(&rel)).unwrap();
            (rel, bytes)
        })
        .collect()
}

#[test]
fn a_pass_packs_loose_crosshairs_and_counts_their_bytes() {
    let env = crosshair_env();
    let brackets = import_vtf(&env, "Brackets");
    let dot = import_png_as(&env, "Dot", Provenance::Local);
    loose(&env, &brackets);
    loose(&env, &dot);
    let before = [loose_files(&env, &brackets), loose_files(&env, &dot)];
    let report = env.lib.convert_library();
    let c = report.crosshairs();
    assert_eq!((c.packed(), c.failed(), c.cleaned()), (2, 0, 0));
    let mut zips = 0;
    let mut bytes = 0;
    for (entry, files) in [&brackets, &dot].into_iter().zip(&before) {
        assert_eq!(tree(&dir_of(&env, entry)), ["copy.zip"]);
        assert_eq!(&crosshair_zip_files(&env.dir, entry.folder_name()), files);
        zips += fs::metadata(dir_of(&env, entry).join("copy.zip"))
            .unwrap()
            .len();
        bytes += files.iter().map(|(_, b)| b.len() as u64).sum::<u64>();
    }
    assert_eq!((c.bytes_before(), c.bytes_after()), (bytes, zips));
    let again = env.lib.convert_library();
    assert_eq!(again.crosshairs().packed(), 0);
}

#[test]
fn a_painted_crosshair_stays_loose_through_a_pass_and_resaves() {
    let env = crosshair_env();
    let mine = painted(&env, "Mine");
    // A row whose format is not painted but whose folder holds a drawing stays loose too.
    let drawn = import_png_as(&env, "Drawn", Provenance::Local);
    loose(&env, &drawn);
    write_file(&dir_of(&env, &drawn).join("source.png"), b"drawing");
    let before = (loose_files(&env, &mine), loose_files(&env, &drawn));
    let report = env.lib.convert_library();
    assert_eq!(report.crosshairs().packed(), 0);
    assert_eq!(
        (loose_files(&env, &mine), loose_files(&env, &drawn)),
        before
    );
    let drawing = RgbaImage::from_pixels(32, 32, vec![200; 32 * 32 * 4]).unwrap();
    env.lib
        .save_painted_crosshair(&PaintTarget::Existing(mine.id().clone()), "Mine", &drawing)
        .unwrap();
    assert!(!dir_of(&env, &mine).join("copy.zip").exists());
    assert_eq!(
        env.lib.painted_source(mine.id()).unwrap().drawing(),
        &drawing
    );
}

#[test]
fn a_crosshair_that_fails_to_pack_stays_loose_works_and_warns_once() {
    let mut env = crosshair_env();
    let builtins = builtin_catalog();
    let dot = import_png_as(&env, "Dot", Provenance::Local);
    loose(&env, &dot);
    env.lib.set_pack_rename(common::packed::rename_then_damage);
    let key = format!("crosshairs/{}", dot.folder_name());
    let first = env.lib.convert_library();
    assert_eq!(
        (first.crosshairs().packed(), first.crosshairs().failed()),
        (0, 1)
    );
    assert_eq!(first.new_failures(), std::slice::from_ref(&key));
    assert!(!dir_of(&env, &dot).join("copy.zip").exists());
    assert!(env.lib.crosshair_preview(&library(&dot), &builtins).is_ok());
    env.lib
        .assign("tf_weapon_scattergun", &library(&dot))
        .unwrap();
    let applied = env.lib.apply_crosshairs(&env.tf2, &builtins).unwrap();
    assert_eq!(applied.applied().len(), 1);
    let second = env.lib.convert_library();
    assert_eq!(second.crosshairs().failed(), 1);
    assert!(second.new_failures().is_empty());
}

#[test]
fn leftovers_beside_a_crosshair_zip_are_cleaned() {
    let env = crosshair_env();
    let dot = import_png_as(&env, "Dot", Provenance::Local);
    let packed = crosshair_zip_files(&env.dir, dot.folder_name());
    write_file(&dir_of(&env, &dot).join("preview.png"), b"stale");
    write_file(&dir_of(&env, &dot).join("copy.zip.tmp"), b"half a zip");
    let report = env.lib.convert_library();
    assert_eq!(
        (report.crosshairs().cleaned(), report.crosshairs().packed()),
        (1, 0)
    );
    assert_eq!(tree(&dir_of(&env, &dot)), ["copy.zip"]);
    assert_eq!(crosshair_zip_files(&env.dir, dot.folder_name()), packed);
}

#[test]
fn a_crosshair_row_naming_the_parent_folder_is_not_packed() {
    let env = crosshair_env();
    let dot = import_png_as(&env, "Dot", Provenance::Local);
    let library = env.dir.library_dir(LibraryKind::Crosshairs);
    let before = tree(library.parent().unwrap());
    common::sql(&env.dir)
        .execute(
            "UPDATE library_crosshairs SET folder_name = '..' WHERE id = ?1",
            [dot.id().to_string()],
        )
        .unwrap();
    let report = env.lib.convert_library();
    assert_eq!(
        (report.crosshairs().packed(), report.crosshairs().failed()),
        (0, 0)
    );
    assert_eq!(tree(library.parent().unwrap()), before);
    assert!(!library.join("copy.zip").exists());
}

fn s(text: &str) -> String {
    text.to_owned()
}
