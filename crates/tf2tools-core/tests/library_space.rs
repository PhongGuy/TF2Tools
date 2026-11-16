//! What `Library::space` counts: lengths on disk, and packed items at their files' sizes.

use crate::common;
use crate::gb_addon_install::{download, no, setup, vpk};

use common::packed::{gb_zip, loose_hud, packed_files, zip_path};
use common::{library_env, tree, write_file, zip_bytes};
use tf2tools_core::{AddonName, DataDir, Provenance};

/// Every file length under `<root>/library`.
fn lengths(dir: &DataDir) -> u64 {
    let root = dir.root().join("library");
    tree(&root)
        .iter()
        .map(|rel| std::fs::metadata(root.join(rel)).unwrap().len())
        .sum()
}

#[test]
fn an_empty_library_uses_nothing() {
    let env = library_env();
    let space = env.lib.space().unwrap();
    assert_eq!((space.on_disk(), space.unpacked()), (0, 0));
}

#[test]
fn a_packed_hud_counts_its_zip_on_disk_and_its_files_unpacked() {
    let env = library_env();
    let archive = write_file(
        &env.tmp.path().join("in/rayshud.zip"),
        &zip_bytes(&[
            ("rayshud/info.vdf", b"\"rayshud\" {}"),
            ("rayshud/materials/bg.vtf", &[7u8; 65536]),
        ]),
    );
    let hud = env.lib.import_hud(&archive, Provenance::Local).unwrap();
    let vpk_path = write_file(
        &env.tmp.path().join("in/flat.vpk"),
        &common::vpk_bytes(&[("materials/a.vmt", b"x")]),
    );
    let name = AddonName::new("mastercomfig-addon-flat-mouse.vpk").unwrap();
    env.lib.import_addon(&vpk_path, &name, "1").unwrap();
    let zip = std::fs::metadata(zip_path(&env, &hud)).unwrap().len();
    let entries: u64 = packed_files(&env, &hud)
        .iter()
        .map(|(_, b)| b.len() as u64)
        .sum();
    let space = env.lib.space().unwrap();
    assert_eq!(space.on_disk(), lengths(&env.dir));
    assert_eq!(space.unpacked(), lengths(&env.dir) - zip + entries);
    assert!(space.unpacked() > space.on_disk());
}

#[test]
fn a_loose_hud_and_a_damaged_zip_count_the_same_both_ways() {
    let env = library_env();
    let hud = loose_hud(&env, "rayshud", &[("a.res", b"a")], Provenance::Local);
    let space = env.lib.space().unwrap();
    assert_eq!(space.on_disk(), space.unpacked());
    std::fs::write(zip_path(&env, &hud), b"not a zip").unwrap();
    let space = env.lib.space().unwrap();
    assert_eq!(space.on_disk(), lengths(&env.dir));
    assert_eq!(space.unpacked(), space.on_disk());
}

#[tokio::test]
async fn a_packed_addon_copy_counts_its_files_unpacked() {
    let s = setup().await;
    let d = download(&s, 7, "Toon", "toon.vpk", vpk(&[0u8; 65536]), 100).await;
    s.env
        .lib
        .install_gb_addon(d, &["toon.vpk".to_owned()], &s.tf2, &no)
        .unwrap();
    let zip = std::fs::metadata(gb_zip(&s.env.dir, 7)).unwrap().len();
    let space = s.env.lib.space().unwrap();
    assert_eq!(space.on_disk(), lengths(&s.env.dir));
    let vpk_len = vpk(&[0u8; 65536]).len() as u64;
    assert_eq!(space.unpacked(), lengths(&s.env.dir) - zip + vpk_len);
}
