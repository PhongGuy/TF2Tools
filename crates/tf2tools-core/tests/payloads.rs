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

/// The shape of GameBanana item 360861: a sound pack wrapped as `Sound/tf/sound` beside
/// a `Stock Choices` folder of model variants, under each of two weapon variants.
const CARBINE: [(&str, &[u8]); 6] = [
    (
        "cv/clencarbin/Unsilenced/Sound/tf/sound/weapons/smg.wav",
        b"wav",
    ),
    (
        "cv/clencarbin/Unsilenced/Stock Choices/Folded stock/models/workshop/a.mdl",
        b"m1",
    ),
    (
        "cv/clencarbin/Unsilenced/Stock Choices/Stockless/models/workshop/a.mdl",
        b"m2",
    ),
    (
        "cv/clencarbin/Unsilenced/Stock Choices/Unfolded stock/models/workshop/a.mdl",
        b"m3",
    ),
    (
        "cv/clencarbin/Silenced/Stock choices/Stockless/models/workshop/a.mdl",
        b"m4",
    ),
    (
        "cv/clencarbin/Silenced/Stock choices/Unfolded/models/workshop/a.mdl",
        b"m5",
    ),
];

#[test]
fn a_content_dir_wrapping_a_tf_folder_leads_to_the_real_root() {
    let t = tree_of(&CARBINE);
    let mut found: Vec<String> = names(t.path()).into_iter().map(|(n, _)| n).collect();
    found.sort();
    assert_eq!(
        found,
        [
            "cv/clencarbin/Silenced/Stock choices/Stockless",
            "cv/clencarbin/Silenced/Stock choices/Unfolded",
            "cv/clencarbin/Unsilenced/Sound/tf",
            "cv/clencarbin/Unsilenced/Stock Choices/Folded stock",
            "cv/clencarbin/Unsilenced/Stock Choices/Stockless",
            "cv/clencarbin/Unsilenced/Stock Choices/Unfolded stock",
        ]
    );
}

#[test]
fn a_content_root_nested_in_another_is_its_own_payload() {
    let t = tree_of(&[
        ("Mod/models/a.mdl", &[1u8; 64]),
        ("Mod/Optional/Red/materials/a.vmt", &[2u8; 8]),
    ]);
    let found = find_payloads(t.path()).unwrap();
    assert_eq!(
        found
            .iter()
            .map(|p| (p.name(), p.size()))
            .collect::<Vec<_>>(),
        [("Mod", 64), ("Mod/Optional/Red", 8)],
        "the outer payload's size leaves the nested one out"
    );
}

#[test]
fn an_info_vdf_folder_keeps_its_nested_content_roots() {
    let t = tree_of(&[
        ("hud/info.vdf", b"\"hud\" {}"),
        ("hud/resource/ui/a.res", b"a"),
        ("hud/extras/alt/materials/x.vmt", b"c"),
    ]);
    assert_eq!(names(t.path()), [("hud".to_owned(), PayloadKind::Folder)]);
}

#[test]
fn the_folder_search_stops_one_past_the_limit() {
    let many: Vec<String> = (0..MAX_PAYLOADS * 4)
        .map(|i| format!("v{i:03}/materials/a.vmt"))
        .collect();
    let refs: Vec<(&str, &[u8])> = many.iter().map(|n| (n.as_str(), &b"m"[..])).collect();
    let t = tree_of(&refs);
    assert_eq!(find_payloads(t.path()).unwrap().len(), MAX_PAYLOADS + 1);
}

#[test]
fn labels_keep_only_the_path_parts_that_tell_payloads_apart() {
    let t = tree_of(&CARBINE);
    let mut labels: Vec<(String, String)> = find_payloads(t.path())
        .unwrap()
        .iter()
        .map(|p| (p.name().to_owned(), p.label().to_owned()))
        .collect();
    labels.sort();
    let labels: Vec<&str> = labels.iter().map(|(_, l)| l.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Silenced / Stockless",
            "Unfolded",
            "Sound",
            "Folded stock",
            "Unsilenced / Stockless",
            "Unfolded stock",
        ]
    );

    let t = tree_of(&[
        (
            "54 FOV/X.vpk",
            &common::vpk_bytes(&[("models/a.mdl", b"a")]),
        ),
        (
            "70+ FOV/X.vpk",
            &common::vpk_bytes(&[("models/b.mdl", b"b")]),
        ),
    ]);
    let labels: Vec<String> = find_payloads(t.path())
        .unwrap()
        .iter()
        .map(|p| p.label().to_owned())
        .collect();
    assert_eq!(labels, ["54 FOV / X.vpk", "70+ FOV / X.vpk"]);

    let t = tree_of(&[("team fortress 2/tf/custom/mymod/materials/a.vmt", b"m")]);
    assert_eq!(find_payloads(t.path()).unwrap()[0].label(), "mymod");
    let t = tree_of(&[("tf/materials/vgui/a.vmt", b"m")]);
    assert_eq!(find_payloads(t.path()).unwrap()[0].label(), "Main files");
}

#[test]
fn a_gui_folder_with_resource_keeps_its_customizations_whole() {
    let t = tree_of(&[
        ("gui/resource/ui/a.res", b"a"),
        ("gui/customizations/alt/resource/ui/a.res", b"b"),
        ("gui/customizations/alt/materials/x.vmt", b"c"),
    ]);
    assert_eq!(names(t.path()), [("gui".to_owned(), PayloadKind::Folder)]);
}

#[test]
fn roots_past_the_stop_are_never_dropped_silently() {
    let t = tree_of(&[("r1/materials/a.vmt", b"a"), ("r2/materials/b.vmt", b"b")]);
    for i in 0..MAX_PAYLOADS {
        std::fs::create_dir_all(t.path().join(format!("e{i:02}/materials"))).unwrap();
    }
    assert!(
        find_payloads(t.path()).unwrap().len() > MAX_PAYLOADS,
        "a walk that stopped early must be refused as too many"
    );
}

#[test]
fn a_content_folder_whose_tf_holds_no_content_is_still_content() {
    let t = tree_of(&[("pack/sound/tf/a.wav", b"w")]);
    assert_eq!(names(t.path()), [("pack".to_owned(), PayloadKind::Folder)]);
}

// Windows folders ignore case, so "Pack" and "pack" can't sit side by side there.
#[test]
#[cfg(not(windows))]
fn labels_differing_only_in_case_are_made_unique() {
    let t = tree_of(&[
        ("Pack/materials/a.vmt", &[1u8; 3]),
        ("pack/models/a.mdl", &[2u8; 2]),
        ("pack (2)/sound/a.wav", &[3u8; 1]),
    ]);
    let labels: Vec<(String, String)> = find_payloads(t.path())
        .unwrap()
        .iter()
        .map(|p| (p.name().to_owned(), p.label().to_owned()))
        .collect();
    assert_eq!(
        labels,
        [
            ("Pack".to_owned(), "Pack".to_owned()),
            ("pack".to_owned(), "pack (3)".to_owned()),
            ("pack (2)".to_owned(), "pack (2)".to_owned()),
        ]
    );
}

#[test]
fn a_gui_folder_still_splits_off_a_nested_root_that_is_not_hud_like() {
    let t = tree_of(&[
        ("Mod/resource/ui/a.res", &[1u8; 64]),
        ("Mod/customizations/alt/resource/ui/a.res", &[2u8; 8]),
        ("Mod/Optional/Red/materials/a.vmt", &[3u8; 4]),
    ]);
    let found = find_payloads(t.path()).unwrap();
    assert_eq!(
        found
            .iter()
            .map(|p| (p.name(), p.size()))
            .collect::<Vec<_>>(),
        [("Mod", 72), ("Mod/Optional/Red", 4)],
        "the HUD-like customization stays inside, the plain root is its own payload"
    );
}
