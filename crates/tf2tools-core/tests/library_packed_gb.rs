//! GameBanana add-on copies packed into `copy.zip`: conversion, reinstall and remove.

use crate::common;
use crate::gb_addon_install::{Setup, download, no, setup, vpk};

use common::packed::{gb_loose, gb_tree, gb_zip};
use tf2tools_core::{GbAddonInstallError, GbId};

fn chosen(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| (*n).to_owned()).collect()
}

/// The installed files under tf/custom with their bytes.
fn custom_files(s: &Setup) -> Vec<(String, Vec<u8>)> {
    let custom = s.tf2.custom_dir();
    common::tree(&custom)
        .into_iter()
        .map(|rel| {
            let bytes = std::fs::read(custom.join(&rel)).unwrap();
            (rel, bytes)
        })
        .collect()
}

fn clear_custom(s: &Setup) {
    let custom = s.tf2.custom_dir();
    std::fs::remove_dir_all(&custom).unwrap();
    std::fs::create_dir_all(&custom).unwrap();
}

/// Installs add-on 7 with a VPK payload and a folder payload, leaves its copy loose, and
/// returns what landed in the game.
async fn two_payloads(s: &Setup) -> Vec<(String, Vec<u8>)> {
    let body = common::zip_bytes(&[
        ("Toon/toon.vpk", &vpk(&[1u8; 4096])),
        ("pack/materials/b.vmt", &[2u8; 4096]),
    ]);
    let d = download(s, 7, "Toon FX", "toon.zip", body, 200).await;
    s.env
        .lib
        .install_gb_addon(d, &chosen(&["Toon/toon.vpk", "pack"]), &s.tf2, &no)
        .unwrap();
    s.env.lib.unpack_gb_addon_for_tests(GbId::new(7)).unwrap();
    custom_files(s)
}

#[tokio::test]
async fn a_loose_copy_is_packed_and_installs_again_the_same() {
    let s = setup().await;
    let installed = two_payloads(&s).await;
    let before = gb_tree(&s.env.dir);
    let loose_bytes: u64 = gb_loose(&s.env.dir, 7)
        .iter()
        .map(|rel| {
            let path = s.env.dir.gb_addon_dir().join("7").join(rel);
            std::fs::metadata(path).unwrap().len()
        })
        .sum();
    let report = s.env.lib.convert_library();
    let gb = report.gb_addons();
    assert_eq!((gb.packed(), gb.failed()), (1, 0));
    assert!(gb_zip(&s.env.dir, 7).is_file());
    assert_eq!(gb_loose(&s.env.dir, 7), Vec::<String>::new());
    assert_eq!(gb_tree(&s.env.dir), before);
    let zip_len = std::fs::metadata(gb_zip(&s.env.dir, 7)).unwrap().len();
    assert_eq!(
        (gb.bytes_before(), gb.bytes_after()),
        (loose_bytes, zip_len)
    );

    clear_custom(&s);
    let entry = s
        .env
        .lib
        .reinstall_gb_addon(GbId::new(7), &s.tf2, &no)
        .unwrap();
    assert!(entry.is_installed());
    assert_eq!(custom_files(&s), installed);
}

#[tokio::test]
async fn a_second_pass_leaves_a_packed_copy_alone() {
    let s = setup().await;
    two_payloads(&s).await;
    s.env.lib.convert_library();
    let stamp = std::fs::metadata(gb_zip(&s.env.dir, 7))
        .unwrap()
        .modified()
        .unwrap();
    let again = s.env.lib.convert_library();
    let gb = again.gb_addons();
    assert_eq!((gb.packed(), gb.failed(), gb.cleaned()), (0, 0, 0));
    let now = std::fs::metadata(gb_zip(&s.env.dir, 7))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(now, stamp);
}

#[tokio::test]
async fn stale_loose_files_beside_a_packed_copy_are_never_installed() {
    let s = setup().await;
    let installed = two_payloads(&s).await;
    s.env.lib.convert_library();
    // A crash after the zip was placed leaves the loose tree; here every file of it is stale.
    let root = s.env.dir.gb_addon_dir();
    for rel in gb_tree(&s.env.dir) {
        common::write_file(&root.join(rel), b"stale");
    }
    clear_custom(&s);
    s.env
        .lib
        .reinstall_gb_addon(GbId::new(7), &s.tf2, &no)
        .unwrap();
    assert_eq!(custom_files(&s), installed);
    let report = s.env.lib.convert_library();
    assert_eq!(report.gb_addons().cleaned(), 1);
    assert_eq!(gb_loose(&s.env.dir, 7), Vec::<String>::new());
}

#[tokio::test]
async fn a_damaged_copy_zip_installs_again_as_not_in_library() {
    let s = setup().await;
    two_payloads(&s).await;
    s.env.lib.convert_library();
    std::fs::write(gb_zip(&s.env.dir, 7), b"not a zip").unwrap();
    clear_custom(&s);
    let err = s
        .env
        .lib
        .reinstall_gb_addon(GbId::new(7), &s.tf2, &no)
        .unwrap_err();
    assert!(
        matches!(err, GbAddonInstallError::NotInLibrary { .. }),
        "{err:?}"
    );
    assert!(custom_files(&s).is_empty());
}

#[tokio::test]
async fn a_copy_that_cannot_be_packed_stays_loose_and_installs() {
    let s = setup().await;
    let installed = two_payloads(&s).await;
    let before = gb_loose(&s.env.dir, 7);
    // A folder where the zip goes makes the read-back refuse it.
    std::fs::create_dir_all(gb_zip(&s.env.dir, 7)).unwrap();
    let report = s.env.lib.convert_library();
    assert_eq!(
        (report.gb_addons().packed(), report.gb_addons().failed()),
        (0, 1)
    );
    assert_eq!(gb_loose(&s.env.dir, 7), before);
    clear_custom(&s);
    s.env
        .lib
        .reinstall_gb_addon(GbId::new(7), &s.tf2, &no)
        .unwrap();
    assert_eq!(custom_files(&s), installed);
}

#[tokio::test]
async fn a_slot_folder_with_no_files_keeps_the_copy_loose() {
    let s = setup().await;
    two_payloads(&s).await;
    // Emptying the folder payload leaves `<n>/<name>/materials/` with nothing a zip keeps.
    let vmt = gb_loose(&s.env.dir, 7)
        .into_iter()
        .find(|rel| rel.ends_with("materials/b.vmt"))
        .unwrap();
    std::fs::remove_file(s.env.dir.gb_addon_dir().join("7").join(vmt)).unwrap();
    let report = s.env.lib.convert_library();
    let gb = report.gb_addons();
    assert_eq!((gb.packed(), gb.failed()), (0, 0));
    assert!(!gb_zip(&s.env.dir, 7).exists());
}

#[tokio::test]
async fn remove_deletes_a_packed_copy() {
    let s = setup().await;
    two_payloads(&s).await;
    s.env.lib.convert_library();
    s.env
        .lib
        .remove_gb_addon(GbId::new(7), &s.tf2, &no)
        .unwrap();
    assert!(!s.env.dir.gb_addon_dir().join("7").exists());
    assert!(custom_files(&s).is_empty());
}

#[tokio::test]
async fn a_row_without_a_copy_and_a_staging_dir_are_left_alone() {
    let s = setup().await;
    common::sql(&s.env.dir)
        .execute(
            "INSERT INTO library_gb_addons (gb_id, section, name, author, file_id, file_date,
             latest_file_date, checked_at, payloads, files, installed, gone)
             VALUES (50, 'effects', 'Gone', 'vex', 1, 1, 1, 0, '[\"a.vpk\"]',
             '[\"custom/tf2tools-gb-50-gone.vpk\"]', 0, 0)",
            [],
        )
        .unwrap();
    let staged = s.env.dir.gb_addon_dir().join(".staging-9/1/x.vpk");
    common::write_file(&staged, b"x");
    let report = s.env.lib.convert_library();
    let gb = report.gb_addons();
    assert_eq!((gb.packed(), gb.failed(), gb.cleaned()), (0, 0, 0));
    assert!(staged.is_file());
    assert!(!s.env.dir.gb_addon_dir().join("50").exists());
}

#[tokio::test]
async fn an_install_stores_the_copy_packed() {
    let s = setup().await;
    let body = common::zip_bytes(&[
        ("Toon/toon.vpk", &vpk(&[1u8; 4096])),
        ("pack/materials/b.vmt", &[2u8; 4096]),
    ]);
    let d = download(&s, 7, "Toon FX", "toon.zip", body, 200).await;
    s.env
        .lib
        .install_gb_addon(d, &chosen(&["Toon/toon.vpk", "pack"]), &s.tf2, &no)
        .unwrap();
    assert!(gb_zip(&s.env.dir, 7).is_file());
    assert_eq!(gb_loose(&s.env.dir, 7), Vec::<String>::new());
    // Each zip entry is `7/<slot>/<installed path>`, and the game holds exactly those paths.
    let mut in_zip: Vec<String> = gb_tree(&s.env.dir)
        .into_iter()
        .map(|rel| rel.splitn(3, '/').nth(2).unwrap().to_owned())
        .collect();
    in_zip.sort();
    let in_game: Vec<String> = custom_files(&s).into_iter().map(|(rel, _)| rel).collect();
    assert_eq!(in_zip, in_game);
}

#[tokio::test]
async fn an_install_whose_pack_fails_keeps_the_loose_copy() {
    let mut s = setup().await;
    s.env
        .lib
        .set_pack_rename(common::packed::rename_then_damage);
    let d = download(&s, 8, "Crit", "crit.vpk", vpk(b"c"), 100).await;
    let entry = s
        .env
        .lib
        .install_gb_addon(d, &chosen(&["crit.vpk"]), &s.tf2, &no)
        .unwrap();
    assert!(entry.is_installed());
    assert!(!gb_zip(&s.env.dir, 8).exists());
    assert_eq!(gb_loose(&s.env.dir, 8), ["1/tf2tools-gb-8-crit.vpk"]);
    clear_custom(&s);
    s.env
        .lib
        .reinstall_gb_addon(GbId::new(8), &s.tf2, &no)
        .unwrap();
    assert_eq!(
        common::tree(&s.tf2.custom_dir()),
        ["tf2tools-gb-8-crit.vpk"]
    );
}

#[tokio::test]
async fn an_update_over_a_packed_copy_replaces_it_whole() {
    let s = setup().await;
    let d = download(&s, 9, "Old", "old.vpk", vpk(b"o"), 100).await;
    s.env
        .lib
        .install_gb_addon(d, &chosen(&["old.vpk"]), &s.tf2, &no)
        .unwrap();
    let d = download(&s, 9, "New", "new.vpk", vpk(b"n"), 200).await;
    s.env
        .lib
        .install_gb_addon(d, &chosen(&["new.vpk"]), &s.tf2, &no)
        .unwrap();
    assert_eq!(gb_tree(&s.env.dir), ["9/1/tf2tools-gb-9-new.vpk"]);
    assert_eq!(gb_loose(&s.env.dir, 9), Vec::<String>::new());
    assert_eq!(common::tree(&s.tf2.custom_dir()), ["tf2tools-gb-9-new.vpk"]);
}

#[tokio::test]
async fn a_failing_addon_copy_is_named_by_its_id() {
    let s = setup().await;
    two_payloads(&s).await;
    std::fs::create_dir_all(gb_zip(&s.env.dir, 7)).unwrap();
    let report = s.env.lib.convert_library();
    assert_eq!(report.new_failures(), ["gb-addons/7"]);
    assert!(s.env.lib.convert_library().new_failures().is_empty());
}

#[tokio::test]
async fn a_kind_that_cannot_be_listed_keeps_its_remembered_failures() {
    let s = setup().await;
    two_payloads(&s).await;
    std::fs::create_dir_all(gb_zip(&s.env.dir, 7)).unwrap();
    assert_eq!(s.env.lib.convert_library().new_failures(), ["gb-addons/7"]);
    // A row with a negative id makes the add-on listing fail for one pass.
    let insert_bad_row = "INSERT INTO library_gb_addons (gb_id, section, name, author, file_id,
        file_date, latest_file_date, checked_at, payloads, files, installed, gone)
        VALUES (-1, 'Skins', 'bad', 'vex', 1, 0, 0, 0, '[]', '[]', 0, 0)";
    common::sql(&s.env.dir).execute(insert_bad_row, []).unwrap();
    let unlisted = s.env.lib.convert_library();
    assert_eq!(unlisted.gb_addons().failed(), 0);
    assert!(unlisted.new_failures().is_empty());
    common::sql(&s.env.dir)
        .execute("DELETE FROM library_gb_addons WHERE gb_id = -1", [])
        .unwrap();
    // The failure was not forgotten while unlisted, so it is still not new.
    let listed = s.env.lib.convert_library();
    assert_eq!(listed.gb_addons().failed(), 1);
    assert!(listed.new_failures().is_empty());
}

// A conversion pass that got in between the row save and the pack would pack first and
// leave the install warning that a packed copy was "left loose".
#[tokio::test]
async fn an_install_packs_where_no_conversion_pass_can_reach_it() {
    let mut s = setup().await;
    s.env
        .lib
        .set_pack_rename(common::packed::rename_noting_copies);
    let d = download(&s, 9, "Raced", "raced.vpk", vpk(b"r"), 100).await;
    s.env
        .lib
        .install_gb_addon(d, &chosen(&["raced.vpk"]), &s.tf2, &no)
        .unwrap();
    assert!(common::packed::packed_under_copies_lock(&gb_zip(
        &s.env.dir, 9
    )));
}
