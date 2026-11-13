//! Finding the maps in an extracted GameBanana download and the files that go with them.

use crate::common::{self, write_file};

use std::io::Write;
use std::path::Path;

use tf2tools_core::{MapPayload, find_maps};

const CAP: u64 = 1 << 20;

type Row = (String, String, String, Vec<(String, String)>, usize);

fn bsp(tag: &[u8]) -> Vec<u8> {
    let mut body = b"VBSP".to_vec();
    body.extend_from_slice(tag);
    body
}

fn bz2(bytes: &[u8]) -> Vec<u8> {
    let mut enc = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::default());
    enc.write_all(bytes).unwrap();
    enc.finish().unwrap()
}

fn rel(tree: &Path, path: &Path) -> String {
    path.strip_prefix(tree)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/")
}

/// Each payload as (name, bsp, root, companions as (source, destination), skipped), paths
/// relative to `tree`.
fn rows(tree: &Path, maps: &[MapPayload]) -> Vec<Row> {
    maps.iter()
        .map(|m| {
            let companions = m
                .companions()
                .iter()
                .map(|(src, dest)| (rel(tree, src), dest.clone()))
                .collect();
            (
                m.name().to_owned(),
                rel(tree, m.bsp()),
                rel(tree, m.root()),
                companions,
                m.skipped(),
            )
        })
        .collect()
}

fn row(name: &str, bsp: &str, root: &str, companions: &[(&str, &str)], skipped: usize) -> Row {
    let companions = companions
        .iter()
        .map(|(s, d)| ((*s).to_owned(), (*d).to_owned()))
        .collect();
    (
        name.to_owned(),
        bsp.to_owned(),
        root.to_owned(),
        companions,
        skipped,
    )
}

fn names(maps: &[MapPayload]) -> Vec<&str> {
    maps.iter().map(MapPayload::name).collect()
}

#[test]
fn a_loose_map_at_the_root_is_found_and_other_files_are_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    let t = tmp.path();
    write_file(&t.join("cp_loose.bsp"), &bsp(b"a"));
    write_file(&t.join("readme.txt"), b"hi");
    let maps = find_maps(t, CAP).unwrap();
    assert_eq!(
        rows(t, &maps),
        [row("cp_loose", "cp_loose.bsp", "", &[], 1)]
    );
}

#[test]
fn a_wrapper_folder_is_the_root_and_its_nav_goes_to_maps() {
    let tmp = tempfile::tempdir().unwrap();
    let t = tmp.path();
    write_file(&t.join("Wrapper/cp_a.bsp"), &bsp(b"a"));
    write_file(&t.join("Wrapper/CP_A.NAV"), b"nav");
    let maps = find_maps(t, CAP).unwrap();
    assert_eq!(
        rows(t, &maps),
        [row(
            "cp_a",
            "Wrapper/cp_a.bsp",
            "Wrapper",
            &[("Wrapper/CP_A.NAV", "maps/cp_a.nav")],
            0
        )]
    );
}

#[test]
fn a_game_tree_layout_keeps_only_the_allow_listed_files() {
    let tmp = tempfile::tempdir().unwrap();
    let t = tmp.path();
    let tf = "team fortress 2/tf";
    write_file(&t.join(format!("{tf}/maps/mvm_x.bsp")), &bsp(b"x"));
    write_file(&t.join(format!("{tf}/maps/mvm_x_level_sounds.txt")), b"s");
    write_file(
        &t.join(format!("{tf}/scripts/population/mvm_x_adv.pop")),
        b"pop",
    );
    write_file(&t.join(format!("{tf}/scripts/items/x.txt")), b"items");
    write_file(
        &t.join(format!("{tf}/materials/vgui/maps/menu_photos_mvm_x.vmt")),
        b"vmt",
    );
    write_file(&t.join(format!("{tf}/resource/ui/x.res")), b"res");
    let maps = find_maps(t, CAP).unwrap();
    assert_eq!(
        rows(t, &maps),
        [row(
            "mvm_x",
            &format!("{tf}/maps/mvm_x.bsp"),
            tf,
            &[
                (
                    &format!("{tf}/materials/vgui/maps/menu_photos_mvm_x.vmt"),
                    "materials/vgui/maps/menu_photos_mvm_x.vmt"
                ),
                (
                    &format!("{tf}/scripts/population/mvm_x_adv.pop"),
                    "scripts/population/mvm_x_adv.pop"
                ),
            ],
            3
        )]
    );
}

#[test]
fn folder_names_match_in_any_case_and_file_names_keep_theirs() {
    let tmp = tempfile::tempdir().unwrap();
    let t = tmp.path();
    write_file(&t.join("MyMap/Maps/Koth_A.BSP"), &bsp(b"a"));
    write_file(&t.join("MyMap/Maps/KOTH_A.NAV"), b"nav");
    write_file(&t.join("MyMap/Materials/VGUI/Maps/Sub/Koth_A.vtf"), b"vtf");
    write_file(&t.join("MyMap/Scripts/Population/koth_a.pop"), b"pop");
    let maps = find_maps(t, CAP).unwrap();
    assert_eq!(
        rows(t, &maps),
        [row(
            "Koth_A",
            "MyMap/Maps/Koth_A.BSP",
            "MyMap",
            &[
                ("MyMap/Maps/KOTH_A.NAV", "maps/Koth_A.nav"),
                (
                    "MyMap/Materials/VGUI/Maps/Sub/Koth_A.vtf",
                    "materials/vgui/maps/Sub/Koth_A.vtf"
                ),
                (
                    "MyMap/Scripts/Population/koth_a.pop",
                    "scripts/population/koth_a.pop"
                ),
            ],
            0
        )]
    );
}

#[test]
fn an_eleven_map_pack_lists_every_map_in_order() {
    let tmp = tempfile::tempdir().unwrap();
    let t = tmp.path();
    for i in (1..=11).rev() {
        write_file(&t.join(format!("maps/pack_{i:02}.bsp")), &bsp(b"p"));
    }
    let maps = find_maps(t, CAP).unwrap();
    let expected: Vec<String> = (1..=11).map(|i| format!("pack_{i:02}")).collect();
    assert_eq!(names(&maps), expected);
}

#[test]
fn a_bz2_map_is_decompressed_in_place() {
    let tmp = tempfile::tempdir().unwrap();
    let t = tmp.path();
    write_file(&t.join("maps/cp_z.bsp.bz2"), &bz2(&bsp(b"zz")));
    let maps = find_maps(t, CAP).unwrap();
    assert_eq!(rows(t, &maps), [row("cp_z", "maps/cp_z.bsp", "", &[], 0)]);
    assert_eq!(std::fs::read(maps[0].bsp()).unwrap(), bsp(b"zz"));
    assert!(!t.join("maps/cp_z.bsp.bz2").exists());
}

#[test]
fn a_bz2_that_is_corrupt_or_too_large_is_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    let t = tmp.path();
    write_file(&t.join("maps/cp_big.bsp.bz2"), &bz2(&bsp(&[0u8; 64])));
    write_file(&t.join("maps/cp_bad.bsp.bz2"), b"not bzip2 at all");
    assert!(find_maps(t, 16).unwrap().is_empty());
    assert_eq!(
        common::tree(t),
        ["maps/cp_bad.bsp.bz2", "maps/cp_big.bsp.bz2"],
        "no partial map is left behind"
    );
}

#[test]
fn many_bz2_maps_share_one_budget() {
    let tmp = tempfile::tempdir().unwrap();
    let t = tmp.path();
    for i in 1..=3 {
        write_file(
            &t.join(format!("maps/cp_{i}.bsp.bz2")),
            &bz2(&bsp(&[0u8; 36])),
        );
    }
    write_file(&t.join("maps/bad name.bsp.bz2"), &bz2(&bsp(b"b")));
    // Each map is 40 bytes, so a 100-byte budget holds two of them.
    assert_eq!(names(&find_maps(t, 100).unwrap()), ["cp_1", "cp_2"]);
    assert_eq!(
        common::tree(t),
        [
            "maps/bad name.bsp.bz2",
            "maps/cp_1.bsp",
            "maps/cp_2.bsp",
            "maps/cp_3.bsp.bz2"
        ]
    );
}

#[test]
fn nested_archives_bad_names_and_non_maps_are_not_maps() {
    let tmp = tempfile::tempdir().unwrap();
    let t = tmp.path();
    write_file(
        &t.join("pack.zip"),
        &common::zip_bytes(&[("maps/cp_n.bsp", &bsp(b"n"))]),
    );
    write_file(&t.join("maps/bad name.bsp"), &bsp(b"b"));
    write_file(&t.join("maps/cp_fake.bsp"), b"PK\x03\x04 not a map");
    assert!(find_maps(t, CAP).unwrap().is_empty());
}

#[test]
fn a_name_repeated_in_another_case_keeps_the_first() {
    let tmp = tempfile::tempdir().unwrap();
    let t = tmp.path();
    write_file(&t.join("a/maps/CP_X.bsp"), &bsp(b"1"));
    write_file(&t.join("b/maps/cp_x.bsp"), &bsp(b"2"));
    let maps = find_maps(t, CAP).unwrap();
    assert_eq!(
        rows(t, &maps),
        [row("CP_X", "a/maps/CP_X.bsp", "a", &[], 0)]
    );
}

#[test]
fn a_dotted_name_is_a_map() {
    let tmp = tempfile::tempdir().unwrap();
    let t = tmp.path();
    write_file(&t.join("maps/pl_x.1.bsp"), &bsp(b"d"));
    assert_eq!(names(&find_maps(t, CAP).unwrap()), ["pl_x.1"]);
}
