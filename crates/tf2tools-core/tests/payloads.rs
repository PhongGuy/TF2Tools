//! Finding the installable payloads in an extracted GameBanana download.

use crate::common::{self, write_file};

use std::path::Path;
use tempfile::TempDir;
use tf2tools_core::{MAX_PAYLOADS, PayloadKind, find_payloads};

fn tree_of(entries: &[(&str, &[u8])]) -> TempDir {
    let tmp = tempfile::tempdir().unwrap();
    for &(name, bytes) in entries {
        write_file(&tmp.path().join(name), bytes);
    }
    tmp
}

fn names(root: &Path) -> Vec<(String, PayloadKind)> {
    find_payloads(root)
        .unwrap()
        .iter()
        .map(|p| (p.name().to_owned(), p.kind()))
        .collect()
}

#[test]
fn a_named_folder_with_content_dirs_is_one_payload() {
    let t = tree_of(&[
        ("Buckshot Bass/materials/models/a.vmt", b"m"),
        ("Buckshot Bass/models/a.mdl", b"mdl"),
        ("readme.txt", b"r"),
    ]);
    assert_eq!(
        names(t.path()),
        [("Buckshot Bass".to_owned(), PayloadKind::Folder)]
    );
}

#[test]
fn wrappers_down_to_the_content_root_are_stripped() {
    let t = tree_of(&[("team fortress 2/tf/custom/mymod/materials/a.vmt", b"m")]);
    assert_eq!(
        names(t.path()),
        [(
            "team fortress 2/tf/custom/mymod".to_owned(),
            PayloadKind::Folder
        )]
    );
    let t = tree_of(&[("tf/materials/vgui/a.vmt", b"m")]);
    assert_eq!(names(t.path()), [("tf".to_owned(), PayloadKind::Folder)]);
}

#[test]
fn content_at_the_root_is_the_main_files_payload() {
    let t = tree_of(&[("Materials/a.vmt", b"m")]);
    let found = find_payloads(t.path()).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name(), "");
    assert_eq!(found[0].label(), "Main files");
    assert_eq!(found[0].kind(), PayloadKind::Folder);
}

#[test]
fn several_vpks_are_payloads_largest_first() {
    let t = tree_of(&[
        (
            "54 FOV/X.vpk",
            &common::vpk_bytes(&[("models/a.mdl", b"a")]),
        ),
        (
            "70+ FOV/X.vpk",
            &common::vpk_bytes(&[("models/a.mdl", &[1u8; 64])]),
        ),
        ("Read Me!.txt", b"r"),
    ]);
    assert_eq!(
        names(t.path()),
        [
            ("70+ FOV/X.vpk".to_owned(), PayloadKind::Vpk),
            ("54 FOV/X.vpk".to_owned(), PayloadKind::Vpk),
        ]
    );
}

#[test]
fn a_folder_that_duplicates_a_vpk_collapses_to_the_vpk() {
    let crit: &[u8] = b"crit";
    let t = tree_of(&[
        ("Capri CRIT/materials/hud/crit.vmt", crit),
        (
            "Capri CRIT.vpk",
            &common::vpk_bytes(&[("materials/hud/crit.vmt", crit)]),
        ),
    ]);
    assert_eq!(
        names(t.path()),
        [("Capri CRIT.vpk".to_owned(), PayloadKind::Vpk)]
    );
}

#[test]
fn a_folder_with_files_the_vpk_lacks_stays() {
    let t = tree_of(&[
        ("Pack/materials/hud/crit.vmt", b"crit"),
        ("Pack/materials/hud/extra.vmt", &[9u8; 128]),
        (
            "Pack.vpk",
            &common::vpk_bytes(&[("materials/hud/crit.vmt", b"crit")]),
        ),
    ]);
    assert_eq!(
        names(t.path()),
        [
            ("Pack".to_owned(), PayloadKind::Folder),
            ("Pack.vpk".to_owned(), PayloadKind::Vpk),
        ]
    );
}

#[test]
fn an_info_vdf_gui_with_customization_is_one_payload() {
    let t = tree_of(&[
        ("eve hud/info.vdf", b"\"eve\" {}"),
        ("eve hud/resource/ui/a.res", b"a"),
        ("eve hud/#customization/opt1/resource/ui/a.res", b"b"),
        ("eve hud/#customization/opt2/materials/x.vmt", b"c"),
    ]);
    assert_eq!(
        names(t.path()),
        [("eve hud".to_owned(), PayloadKind::Folder)]
    );
}

#[test]
fn a_vpk_inside_a_folder_payload_is_its_own_payload() {
    let t = tree_of(&[
        ("pack/materials/a.vmt", &[1u8; 256]),
        (
            "pack/extra.vpk",
            &common::vpk_bytes(&[("sound/a.wav", b"w")]),
        ),
    ]);
    let found = find_payloads(t.path()).unwrap();
    assert_eq!(
        found
            .iter()
            .map(|p| (p.name(), p.kind()))
            .collect::<Vec<_>>(),
        [
            ("pack", PayloadKind::Folder),
            ("pack/extra.vpk", PayloadKind::Vpk)
        ]
    );
    assert_eq!(found[0].size(), 256, "a folder's size leaves its VPKs out");
}

#[test]
fn a_multi_chunk_vpk_is_one_payload() {
    let t = tree_of(&[
        ("big_dir.vpk", &common::vpk_bytes(&[("models/a.mdl", b"a")])),
        ("big_000.vpk", &[0u8; 32]),
        ("big_001.vpk", &[0u8; 32]),
    ]);
    let found = find_payloads(t.path()).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name(), "big_dir.vpk");
    assert_eq!(found[0].vpk_parts(), 3);
}

#[test]
fn more_than_the_limit_and_none_are_both_reported_as_found() {
    let vpk = common::vpk_bytes(&[("models/a.mdl", b"a")]);
    let many: Vec<(String, Vec<u8>)> = (0..=MAX_PAYLOADS)
        .map(|i| (format!("v{i:02}.vpk"), vpk.clone()))
        .collect();
    let refs: Vec<(&str, &[u8])> = many
        .iter()
        .map(|(n, b)| (n.as_str(), b.as_slice()))
        .collect();
    let t = tree_of(&refs);
    assert_eq!(find_payloads(t.path()).unwrap().len(), MAX_PAYLOADS + 1);
    let t = tree_of(&[("readme.txt", b"r"), ("setup.exe", b"MZ")]);
    assert!(find_payloads(t.path()).unwrap().is_empty());
}

#[test]
fn too_many_vpks_stop_the_walk_before_any_folder() {
    let vpk = common::vpk_bytes(&[("models/a.mdl", b"a")]);
    let mut many: Vec<(String, Vec<u8>)> = (0..=MAX_PAYLOADS)
        .map(|i| (format!("v{i:02}.vpk"), vpk.clone()))
        .collect();
    many.push(("Loose/materials/a.vmt".to_owned(), b"m".to_vec()));
    let refs: Vec<(&str, &[u8])> = many
        .iter()
        .map(|(n, b)| (n.as_str(), b.as_slice()))
        .collect();
    let t = tree_of(&refs);
    let found = names(t.path());
    assert_eq!(found.len(), MAX_PAYLOADS + 1);
    assert!(found.iter().all(|(_, kind)| *kind == PayloadKind::Vpk));
}

#[test]
fn an_unreadable_vpk_is_still_a_payload_and_collapses_nothing() {
    let t = tree_of(&[
        ("Pack/materials/a.vmt", b"a"),
        ("Pack.vpk", b"not a vpk at all"),
    ]);
    assert_eq!(
        names(t.path()),
        [
            ("Pack.vpk".to_owned(), PayloadKind::Vpk),
            ("Pack".to_owned(), PayloadKind::Folder),
        ]
    );
}
