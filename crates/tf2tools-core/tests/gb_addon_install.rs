//! Installing a GameBanana add-on's chosen payloads into the library and tf/custom.

use crate::common::{
    self, StoreEnv, gb_addon_download, gb_addon_file, gb_addon_profile, store_env,
};

use tempfile::TempDir;
use tf2tools_core::{GbAddonDownload, GbAddonInstallError, GbId, Tf2Install};

pub struct Setup {
    pub env: StoreEnv,
    pub _game: TempDir,
    pub tf2: Tf2Install,
}

pub async fn setup() -> Setup {
    let env = store_env().await;
    let (game, path) = common::temp_tf2_tree();
    let tf2 = Tf2Install::from_path(&path).unwrap();
    Setup {
        env,
        _game: game,
        tf2,
    }
}

/// Add-on `id` named `name` whose only file is `body`, dated `date`, downloaded as an
/// Effects add-on. Clears earlier mocks so an id can be served again with a newer file.
pub async fn download(
    s: &Setup,
    id: u64,
    name: &str,
    file: &str,
    body: Vec<u8>,
    date: i64,
) -> GbAddonDownload {
    s.env.server.reset().await;
    let f = gb_addon_file(&s.env, id, 0, file, &body, date).await;
    let profile = gb_addon_profile(&s.env, id, name, vec![f]).await;
    gb_addon_download(&s.env.client, "effects", &profile)
        .await
        .0
        .unwrap()
}

pub fn vpk(tag: &[u8]) -> Vec<u8> {
    common::vpk_bytes(&[("materials/a.vmt", tag)])
}

pub fn no() -> bool {
    false
}

pub fn yes() -> bool {
    true
}

fn chosen(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| (*n).to_owned()).collect()
}

fn custom(s: &Setup) -> Vec<String> {
    common::tree(&s.tf2.custom_dir())
}

#[tokio::test]
async fn a_vpk_payload_installs_under_its_tf2tools_name_and_is_recorded() {
    let s = setup().await;
    let body = common::zip_bytes(&[
        ("Toon/toon.vpk", &vpk(&[1u8; 64])),
        ("Toon/extra.vpk", &vpk(b"x")),
    ]);
    let d = download(&s, 7, "Toon FX", "toon.zip", body, 200).await;
    let entry = s
        .env
        .lib
        .install_gb_addon(d, &chosen(&["Toon/toon.vpk"]), &s.tf2, &no)
        .unwrap();
    assert_eq!(custom(&s), ["tf2tools-gb-7-toon-fx.vpk"]);
    assert_eq!(entry.files(), ["custom/tf2tools-gb-7-toon-fx.vpk"]);
    assert_eq!(entry.payloads(), ["Toon/toon.vpk"]);
    assert_eq!(entry.section(), "effects");
    assert_eq!((entry.file_date(), entry.latest_file_date()), (200, 200));
    assert!(entry.is_installed() && !entry.has_update() && !entry.is_gone());
    assert_eq!(
        common::tree(&s.env.dir.gb_addon_dir()),
        ["7/1/tf2tools-gb-7-toon-fx.vpk"]
    );
    assert!(
        common::tmp_is_empty(&s.env.dir),
        "the consumed download's scratch dir is gone"
    );
}

#[tokio::test]
async fn several_chosen_payloads_are_numbered_in_payload_order() {
    let s = setup().await;
    let body = common::zip_bytes(&[
        ("Toon/toon.vpk", &vpk(&[1u8; 64])),
        ("Toon/extra.vpk", &vpk(b"x")),
    ]);
    let d = download(&s, 7, "Toon FX", "toon.zip", body, 200).await;
    let entry = s
        .env
        .lib
        .install_gb_addon(
            d,
            &chosen(&["Toon/extra.vpk", "Toon/toon.vpk"]),
            &s.tf2,
            &no,
        )
        .unwrap();
    assert_eq!(
        custom(&s),
        ["tf2tools-gb-7-toon-fx-2.vpk", "tf2tools-gb-7-toon-fx.vpk"]
    );
    assert_eq!(entry.payloads(), ["Toon/toon.vpk", "Toon/extra.vpk"]);
}

#[tokio::test]
async fn a_folder_payload_installs_without_its_nested_vpks() {
    let s = setup().await;
    // The nested VPK packs other files, so the folder is not a duplicate of it.
    let nested = common::vpk_bytes(&[("materials/b.vmt", b"x")]);
    let body = common::zip_bytes(&[
        ("pack/materials/a.vmt", &[1u8; 256]),
        ("pack/x.vpk", &nested),
    ]);
    let d = download(&s, 8, "Pink Pack", "pack.zip", body, 100).await;
    s.env
        .lib
        .install_gb_addon(d, &chosen(&["pack"]), &s.tf2, &no)
        .unwrap();
    assert_eq!(custom(&s), ["tf2tools-gb-8-pink-pack/materials/a.vmt"]);
}

#[tokio::test]
async fn a_folder_payload_leaves_out_a_variant_nested_in_it() {
    let s = setup().await;
    let body = common::zip_bytes(&[
        ("mod/models/a.mdl", b"m"),
        ("mod/Optional/Red/materials/a.vmt", b"r"),
    ]);
    let d = download(&s, 8, "Pink Pack", "pack.zip", body, 100).await;
    s.env
        .lib
        .install_gb_addon(d, &chosen(&["mod"]), &s.tf2, &no)
        .unwrap();
    assert_eq!(custom(&s), ["tf2tools-gb-8-pink-pack/models/a.mdl"]);
}

#[tokio::test]
async fn a_multi_chunk_vpk_keeps_its_chunk_numbers() {
    let s = setup().await;
    let body = common::zip_bytes(&[("big_dir.vpk", &vpk(b"d")), ("big_000.vpk", &[0u8; 32])]);
    let d = download(&s, 9, "Big", "big.zip", body, 100).await;
    let entry = s
        .env
        .lib
        .install_gb_addon(d, &chosen(&["big_dir.vpk"]), &s.tf2, &no)
        .unwrap();
    assert_eq!(
        custom(&s),
        ["tf2tools-gb-9-big_000.vpk", "tf2tools-gb-9-big_dir.vpk"]
    );
    assert_eq!(entry.files().len(), 2);
}

#[tokio::test]
async fn a_name_cannot_form_a_path() {
    let s = setup().await;
    let d = download(&s, 10, "a/b\\c..", "x.vpk", vpk(b"v"), 100).await;
    s.env
        .lib
        .install_gb_addon(d, &chosen(&["x.vpk"]), &s.tf2, &no)
        .unwrap();
    assert_eq!(custom(&s), ["tf2tools-gb-10-a-b-c.vpk"]);
}

#[tokio::test]
async fn while_tf2_runs_a_vpk_is_refused_and_a_folder_is_allowed() {
    let s = setup().await;
    let d = download(&s, 11, "Crit", "crit.vpk", vpk(b"v"), 100).await;
    let err = s
        .env
        .lib
        .install_gb_addon(d, &chosen(&["crit.vpk"]), &s.tf2, &yes)
        .unwrap_err();
    assert!(matches!(err, GbAddonInstallError::Tf2Running), "{err:?}");
    assert!(custom(&s).is_empty());
    assert!(s.env.lib.list_gb_addons(Some(&s.tf2)).unwrap().is_empty());

    let body = common::zip_bytes(&[("pack/materials/a.vmt", b"a")]);
    let d = download(&s, 12, "Pack", "pack.zip", body, 100).await;
    s.env
        .lib
        .install_gb_addon(d, &chosen(&["pack"]), &s.tf2, &yes)
        .unwrap();
    assert_eq!(custom(&s), ["tf2tools-gb-12-pack/materials/a.vmt"]);
}

#[tokio::test]
async fn unknown_or_no_payloads_are_refused_before_anything_changes() {
    let s = setup().await;
    let d = download(&s, 14, "A", "a.vpk", vpk(b"v"), 100).await;
    let err = s.env.lib.install_gb_addon(d, &[], &s.tf2, &no).unwrap_err();
    assert!(matches!(err, GbAddonInstallError::NothingChosen), "{err:?}");
    let d = download(&s, 14, "A", "a.vpk", vpk(b"v"), 100).await;
    let err = s
        .env
        .lib
        .install_gb_addon(d, &chosen(&["nope.vpk"]), &s.tf2, &no)
        .unwrap_err();
    assert!(
        matches!(err, GbAddonInstallError::UnknownPayload { ref name } if name == "nope.vpk"),
        "{err:?}"
    );
    assert!(custom(&s).is_empty());
    assert!(common::tree(&s.env.dir.gb_addon_dir()).is_empty());
}

#[tokio::test]
async fn a_missing_game_folder_is_refused() {
    let s = setup().await;
    let d = download(&s, 16, "A", "a.vpk", vpk(b"v"), 100).await;
    std::fs::remove_dir_all(s.tf2.tf_dir()).unwrap();
    let err = s
        .env
        .lib
        .install_gb_addon(d, &chosen(&["a.vpk"]), &s.tf2, &no)
        .unwrap_err();
    assert!(
        matches!(err, GbAddonInstallError::GameMissing { .. }),
        "{err:?}"
    );
    assert!(!s.tf2.tf_dir().exists(), "no writer recreates a moved game");
}

#[tokio::test]
async fn an_install_over_a_tampered_row_never_follows_paths_out_of_custom() {
    let s = setup().await;
    let game = s.tf2.tf_dir().parent().unwrap().to_path_buf();
    common::write_file(&game.join("outside"), b"keep");
    let absolute = game.join("absolute");
    common::write_file(&absolute, b"keep");
    std::fs::create_dir_all(s.tf2.custom_dir().join("sub")).unwrap();
    common::write_file(&s.tf2.custom_dir().join("sub/nested"), b"keep");
    common::write_file(&s.tf2.custom_dir().join("somehud/info.vdf"), b"keep");
    common::write_file(&s.tf2.custom_dir().join("tf2tools-gb-2-other.vpk"), b"keep");
    let files = serde_json::json!([
        "../outside",
        absolute.to_str().unwrap(),
        "custom/sub/nested",
        "custom/..",
        "custom/somehud",
        "custom/tf2tools-gb-2-other.vpk",
        "custom/tf2tools-gb-20-C:x",
    ]);
    common::sql(&s.env.dir)
        .execute(
            "INSERT INTO library_gb_addons (gb_id, section, name, author, file_id, file_date,
             latest_file_date, checked_at, payloads, files, installed, gone)
             VALUES (20, 'effects', 'Bad', 'vex', 1, 1, 1, 0, '[\"pack\"]', ?1, 1, 0)",
            [files.to_string()],
        )
        .unwrap();
    let listed = s.env.lib.list_gb_addons(Some(&s.tf2)).unwrap();
    assert!(
        !listed[0].is_installed(),
        "paths outside custom never count as present"
    );

    let body = common::zip_bytes(&[("pack/materials/a.vmt", b"a")]);
    let d = download(&s, 20, "Pack", "pack.zip", body, 100).await;
    s.env
        .lib
        .install_gb_addon(d, &chosen(&["pack"]), &s.tf2, &no)
        .unwrap();
    assert!(game.join("outside").exists() && absolute.exists());
    assert!(s.tf2.custom_dir().join("sub/nested").exists());
    assert!(
        s.tf2.custom_dir().join("somehud/info.vdf").exists(),
        "another add-on's folder"
    );
    assert!(
        s.tf2.custom_dir().join("tf2tools-gb-2-other.vpk").exists(),
        "another id's file"
    );
}

#[tokio::test]
async fn a_failed_clear_keeps_the_library_copy_matching_the_row() {
    let s = setup().await;
    let old_copy = s.env.dir.gb_addon_dir().join("30/1/tf2tools-gb-30-old.vpk");
    common::write_file(&old_copy, b"old");
    common::sql(&s.env.dir)
        .execute(
            "INSERT INTO library_gb_addons (gb_id, section, name, author, file_id, file_date,
             latest_file_date, checked_at, payloads, files, installed, gone)
             VALUES (30, 'effects', 'Old', 'vex', 1, 1, 1, 0, '[\"old.vpk\"]',
             '[\"custom/tf2tools-gb-30-old.vpk\"]', 1, 0)",
            [],
        )
        .unwrap();
    // A file where tf/custom should be makes every old path unreadable, so the clear fails.
    let custom_dir = s.tf2.custom_dir();
    std::fs::remove_dir_all(&custom_dir).unwrap();
    common::write_file(&custom_dir, b"not a folder");

    let d = download(&s, 30, "New", "new.vpk", vpk(b"n"), 300).await;
    let err = s
        .env
        .lib
        .install_gb_addon(d, &chosen(&["new.vpk"]), &s.tf2, &no)
        .unwrap_err();
    assert!(matches!(err, GbAddonInstallError::Io { .. }), "{err:?}");
    assert_eq!(
        common::tree(&s.env.dir.gb_addon_dir()),
        ["30/1/tf2tools-gb-30-new.vpk"],
        "the library copy holds the new files the row names"
    );
    let row = &s.env.lib.list_gb_addons(Some(&s.tf2)).unwrap()[0];
    assert_eq!(row.files(), ["custom/tf2tools-gb-30-new.vpk"]);
    assert_eq!(row.file_date(), 300);
    assert!(!row.is_installed());

    std::fs::remove_file(&custom_dir).unwrap();
    std::fs::create_dir_all(&custom_dir).unwrap();
    let d = download(&s, 30, "New", "new.vpk", vpk(b"n"), 300).await;
    let entry = s
        .env
        .lib
        .install_gb_addon(d, &chosen(&["new.vpk"]), &s.tf2, &no)
        .unwrap();
    assert!(entry.is_installed());
    assert_eq!(custom(&s), ["tf2tools-gb-30-new.vpk"]);
}

#[tokio::test]
async fn while_tf2_runs_an_update_that_removes_a_vpk_is_refused() {
    let s = setup().await;
    let d = download(&s, 13, "Crit", "crit.vpk", vpk(b"v"), 100).await;
    s.env
        .lib
        .install_gb_addon(d, &chosen(&["crit.vpk"]), &s.tf2, &no)
        .unwrap();
    let library = s.env.dir.gb_addon_dir();
    let copy_before = common::tree(&library);
    let bytes_before: Vec<Vec<u8>> = copy_before
        .iter()
        .map(|f| std::fs::read(library.join(f)).unwrap())
        .collect();
    let body = common::zip_bytes(&[("crit/materials/a.vmt", b"a")]);
    let d = download(&s, 13, "Crit", "crit.zip", body, 200).await;
    let err = s
        .env
        .lib
        .install_gb_addon(d, &chosen(&["crit"]), &s.tf2, &yes)
        .unwrap_err();
    assert!(matches!(err, GbAddonInstallError::Tf2Running), "{err:?}");
    assert_eq!(custom(&s), ["tf2tools-gb-13-crit.vpk"]);
    let row = &s.env.lib.list_gb_addons(Some(&s.tf2)).unwrap()[0];
    assert!(row.is_installed());
    assert_eq!(row.file_date(), 100);
    assert_eq!(
        common::tree(&library),
        copy_before,
        "the library copy is untouched"
    );
    let bytes_after: Vec<Vec<u8>> = copy_before
        .iter()
        .map(|f| std::fs::read(library.join(f)).unwrap())
        .collect();
    assert_eq!(bytes_after, bytes_before);
}

#[tokio::test]
async fn an_update_replaces_the_old_files_and_keeps_one_row() {
    let s = setup().await;
    let v1 = common::zip_bytes(&[("pack/materials/a.vmt", b"a")]);
    let d = download(&s, 15, "Pack", "pack.zip", v1, 100).await;
    s.env
        .lib
        .install_gb_addon(d, &chosen(&["pack"]), &s.tf2, &no)
        .unwrap();
    let d = download(&s, 15, "Pack", "pack.vpk", vpk(b"v2"), 200).await;
    let entry = s
        .env
        .lib
        .install_gb_addon(d, &chosen(&["pack.vpk"]), &s.tf2, &no)
        .unwrap();
    assert_eq!(custom(&s), ["tf2tools-gb-15-pack.vpk"]);
    assert_eq!(
        common::tree(&s.env.dir.gb_addon_dir()),
        ["15/1/tf2tools-gb-15-pack.vpk"]
    );
    assert_eq!(entry.file_date(), 200);
    assert_eq!(s.env.lib.list_gb_addons(Some(&s.tf2)).unwrap().len(), 1);
}

#[tokio::test]
async fn a_failed_copy_into_the_game_leaves_a_row_to_install_again() {
    let s = setup().await;
    // A file where tf/custom should be makes every copy into the game fail.
    std::fs::remove_dir_all(s.tf2.custom_dir()).unwrap();
    std::fs::write(s.tf2.custom_dir(), b"not a folder").unwrap();
    let d = download(&s, 17, "A", "a.vpk", vpk(b"v"), 100).await;
    let err = s
        .env
        .lib
        .install_gb_addon(d, &chosen(&["a.vpk"]), &s.tf2, &no)
        .unwrap_err();
    assert!(matches!(err, GbAddonInstallError::Io { .. }), "{err:?}");
    let list = s.env.lib.list_gb_addons(Some(&s.tf2)).unwrap();
    assert_eq!(list.len(), 1);
    assert!(!list[0].is_installed());
    assert_eq!(
        common::tree(&s.env.dir.gb_addon_dir()),
        ["17/1/tf2tools-gb-17-a.vpk"]
    );
}

#[tokio::test]
async fn a_stale_staging_dir_is_cleared_first() {
    let s = setup().await;
    common::write_file(
        &s.env.dir.gb_addon_dir().join(".staging-18/9/junk.txt"),
        b"junk",
    );
    let d = download(&s, 18, "A", "a.vpk", vpk(b"v"), 100).await;
    s.env
        .lib
        .install_gb_addon(d, &chosen(&["a.vpk"]), &s.tf2, &no)
        .unwrap();
    assert_eq!(
        common::tree(&s.env.dir.gb_addon_dir()),
        ["18/1/tf2tools-gb-18-a.vpk"]
    );
}

#[tokio::test]
async fn a_stale_staging_file_is_cleared_first() {
    let s = setup().await;
    common::write_file(&s.env.dir.gb_addon_dir().join(".staging-27"), b"junk");
    let d = download(&s, 27, "A", "a.vpk", vpk(b"v"), 100).await;
    s.env
        .lib
        .install_gb_addon(d, &chosen(&["a.vpk"]), &s.tf2, &no)
        .unwrap();
    assert_eq!(
        common::tree(&s.env.dir.gb_addon_dir()),
        ["27/1/tf2tools-gb-27-a.vpk"]
    );
}

#[tokio::test]
async fn a_failed_staging_copy_changes_nothing() {
    let s = setup().await;
    let d = download(&s, 19, "A", "a.vpk", vpk(b"v1"), 100).await;
    s.env
        .lib
        .install_gb_addon(d, &chosen(&["a.vpk"]), &s.tf2, &no)
        .unwrap();
    let d = download(&s, 19, "A", "a.vpk", vpk(b"v2"), 200).await;
    // The download's extracted files vanishing makes the staging copy fail part-way.
    std::fs::remove_dir_all(s.env.dir.tmp_dir()).unwrap();
    let root = s.env.dir.gb_addon_dir();
    let result = s
        .env
        .lib
        .install_gb_addon(d, &chosen(&["a.vpk"]), &s.tf2, &no);
    assert!(
        matches!(result, Err(GbAddonInstallError::Io { .. })),
        "{result:?}"
    );
    let list = s.env.lib.list_gb_addons(Some(&s.tf2)).unwrap();
    assert!(list[0].is_installed());
    assert_eq!(list[0].file_date(), 100);
    assert_eq!(custom(&s), ["tf2tools-gb-19-a.vpk"]);
    assert_eq!(common::tree(&root), ["19/1/tf2tools-gb-19-a.vpk"]);
}

#[tokio::test]
async fn remove_deletes_exactly_its_files_and_forgets_it() {
    let s = setup().await;
    let body = common::zip_bytes(&[
        ("Toon/toon.vpk", &vpk(&[1u8; 64])),
        ("pack/materials/b.vmt", b"a"),
    ]);
    let d = download(&s, 20, "Toon", "toon.zip", body, 100).await;
    s.env
        .lib
        .install_gb_addon(d, &chosen(&["Toon/toon.vpk", "pack"]), &s.tf2, &no)
        .unwrap();
    common::write_file(&s.tf2.custom_dir().join("foreign.vpk"), b"f");
    common::write_file(&s.tf2.custom_dir().join("foreign/x.txt"), b"f");
    s.env
        .lib
        .remove_gb_addon(GbId::new(20), &s.tf2, &no)
        .unwrap();
    assert_eq!(custom(&s), ["foreign.vpk", "foreign/x.txt"]);
    assert!(s.env.lib.list_gb_addons(Some(&s.tf2)).unwrap().is_empty());
    assert!(!s.env.dir.gb_addon_dir().join("20").exists());
}

#[tokio::test]
async fn remove_refuses_while_tf2_runs_when_a_vpk_is_installed() {
    let s = setup().await;
    let d = download(&s, 21, "Crit", "crit.vpk", vpk(b"v"), 100).await;
    s.env
        .lib
        .install_gb_addon(d, &chosen(&["crit.vpk"]), &s.tf2, &no)
        .unwrap();
    let err = s
        .env
        .lib
        .remove_gb_addon(GbId::new(21), &s.tf2, &yes)
        .unwrap_err();
    assert!(matches!(err, GbAddonInstallError::Tf2Running), "{err:?}");
    assert_eq!(custom(&s), ["tf2tools-gb-21-crit.vpk"]);
}

#[tokio::test]
async fn removing_an_unknown_addon_is_not_in_library() {
    let s = setup().await;
    let err = s
        .env
        .lib
        .remove_gb_addon(GbId::new(99), &s.tf2, &no)
        .unwrap_err();
    assert!(
        matches!(err, GbAddonInstallError::NotInLibrary { gb_id } if gb_id == GbId::new(99)),
        "{err:?}"
    );
}

#[tokio::test]
async fn remove_skips_recorded_paths_that_leave_custom() {
    let s = setup().await;
    let d = download(&s, 22, "A", "a.vpk", vpk(b"v"), 100).await;
    s.env
        .lib
        .install_gb_addon(d, &chosen(&["a.vpk"]), &s.tf2, &no)
        .unwrap();
    let outside = s.tf2.tf_dir().join("outside.txt");
    common::write_file(&outside, b"keep");
    common::sql(&s.env.dir)
        .execute(
            "UPDATE library_gb_addons SET files = ?1 WHERE gb_id = 22",
            [r#"["custom/../outside.txt","custom/tf2tools-gb-22-a.vpk","cfg/autoexec.cfg"]"#],
        )
        .unwrap();
    s.env
        .lib
        .remove_gb_addon(GbId::new(22), &s.tf2, &no)
        .unwrap();
    assert!(outside.exists());
    assert!(custom(&s).is_empty());
}

#[tokio::test]
async fn a_hand_deleted_addon_lists_as_not_installed_and_installs_again() {
    let s = setup().await;
    let body = common::zip_bytes(&[("pack/materials/a.vmt", b"a")]);
    let d = download(&s, 23, "Pack", "pack.zip", body, 100).await;
    s.env
        .lib
        .install_gb_addon(d, &chosen(&["pack"]), &s.tf2, &no)
        .unwrap();
    std::fs::remove_dir_all(s.tf2.custom_dir().join("tf2tools-gb-23-pack")).unwrap();
    assert!(!s.env.lib.list_gb_addons(Some(&s.tf2)).unwrap()[0].is_installed());
    let entry = s
        .env
        .lib
        .reinstall_gb_addon(GbId::new(23), &s.tf2, &yes)
        .unwrap();
    assert!(entry.is_installed());
    assert_eq!(custom(&s), ["tf2tools-gb-23-pack/materials/a.vmt"]);
}

#[tokio::test]
async fn install_again_refuses_while_tf2_runs_for_a_vpk() {
    let s = setup().await;
    let d = download(&s, 25, "Crit", "crit.vpk", vpk(b"v"), 100).await;
    s.env
        .lib
        .install_gb_addon(d, &chosen(&["crit.vpk"]), &s.tf2, &no)
        .unwrap();
    std::fs::remove_file(s.tf2.custom_dir().join("tf2tools-gb-25-crit.vpk")).unwrap();
    let err = s
        .env
        .lib
        .reinstall_gb_addon(GbId::new(25), &s.tf2, &yes)
        .unwrap_err();
    assert!(matches!(err, GbAddonInstallError::Tf2Running), "{err:?}");
    assert!(custom(&s).is_empty());
}

#[tokio::test]
async fn install_again_without_a_library_copy_is_not_in_library() {
    let s = setup().await;
    let d = download(&s, 24, "A", "a.vpk", vpk(b"v"), 100).await;
    s.env
        .lib
        .install_gb_addon(d, &chosen(&["a.vpk"]), &s.tf2, &no)
        .unwrap();
    std::fs::remove_dir_all(s.env.dir.gb_addon_dir().join("24")).unwrap();
    let err = s
        .env
        .lib
        .reinstall_gb_addon(GbId::new(24), &s.tf2, &no)
        .unwrap_err();
    assert!(
        matches!(err, GbAddonInstallError::NotInLibrary { .. }),
        "{err:?}"
    );
}

#[tokio::test]
async fn install_again_from_a_partial_library_copy_is_not_in_library() {
    let s = setup().await;
    let body = common::zip_bytes(&[
        ("Toon/toon.vpk", &vpk(&[1u8; 64])),
        ("pack/materials/b.vmt", b"a"),
    ]);
    let d = download(&s, 26, "Toon", "toon.zip", body, 100).await;
    s.env
        .lib
        .install_gb_addon(d, &chosen(&["Toon/toon.vpk", "pack"]), &s.tf2, &no)
        .unwrap();
    for name in custom(&s) {
        let top = name.split('/').next().unwrap().to_owned();
        let _ = std::fs::remove_dir_all(s.tf2.custom_dir().join(&top));
        let _ = std::fs::remove_file(s.tf2.custom_dir().join(&top));
    }
    // A copy that lost one slot, as a half-replaced copy would, must not install partly.
    std::fs::remove_dir_all(s.env.dir.gb_addon_dir().join("26/2")).unwrap();
    let err = s
        .env
        .lib
        .reinstall_gb_addon(GbId::new(26), &s.tf2, &no)
        .unwrap_err();
    assert!(
        matches!(err, GbAddonInstallError::NotInLibrary { .. }),
        "{err:?}"
    );
    assert!(custom(&s).is_empty());
}

#[tokio::test]
async fn a_first_install_cut_short_after_its_files_land_leaves_a_row_to_remove() {
    let s = setup().await;
    // Refusing the final installed row stands in for the app dying right after the copy.
    common::sql(&s.env.dir)
        .execute_batch(
            "CREATE TRIGGER cut_short BEFORE INSERT ON library_gb_addons
             WHEN NEW.installed = 1 BEGIN SELECT RAISE(ABORT, 'cut short'); END",
        )
        .unwrap();
    let d = download(&s, 32, "A", "a.vpk", vpk(b"v"), 100).await;
    let err = s
        .env
        .lib
        .install_gb_addon(d, &chosen(&["a.vpk"]), &s.tf2, &no)
        .unwrap_err();
    assert!(
        matches!(err, GbAddonInstallError::Database { .. }),
        "{err:?}"
    );
    assert_eq!(custom(&s), ["tf2tools-gb-32-a.vpk"]);
    let list = s.env.lib.list_gb_addons(Some(&s.tf2)).unwrap();
    assert_eq!(list.len(), 1, "the files in the game have a row");
    assert!(!list[0].is_installed());
    assert_eq!(list[0].files(), ["custom/tf2tools-gb-32-a.vpk"]);

    s.env
        .lib
        .remove_gb_addon(GbId::new(32), &s.tf2, &no)
        .unwrap();
    assert!(custom(&s).is_empty());
    assert!(s.env.lib.list_gb_addons(Some(&s.tf2)).unwrap().is_empty());
}

#[tokio::test]
async fn an_update_cut_short_owns_the_old_and_new_files() {
    let s = setup().await;
    let d = download(&s, 33, "Old", "a.vpk", vpk(b"v1"), 100).await;
    s.env
        .lib
        .install_gb_addon(d, &chosen(&["a.vpk"]), &s.tf2, &no)
        .unwrap();
    common::sql(&s.env.dir)
        .execute_batch(
            "CREATE TRIGGER cut_short BEFORE INSERT ON library_gb_addons
             WHEN NEW.installed = 1 BEGIN SELECT RAISE(ABORT, 'cut short'); END",
        )
        .unwrap();
    // A renamed item installs under a new name, so a cut-short update must own both names.
    let d = download(&s, 33, "New", "a.vpk", vpk(b"v2"), 200).await;
    s.env
        .lib
        .install_gb_addon(d, &chosen(&["a.vpk"]), &s.tf2, &no)
        .unwrap_err();
    let row = &s.env.lib.list_gb_addons(Some(&s.tf2)).unwrap()[0];
    assert!(!row.is_installed());
    assert_eq!(
        row.files(),
        [
            "custom/tf2tools-gb-33-new.vpk",
            "custom/tf2tools-gb-33-old.vpk"
        ]
    );
    s.env
        .lib
        .remove_gb_addon(GbId::new(33), &s.tf2, &no)
        .unwrap();
    assert!(custom(&s).is_empty());
}

#[tokio::test]
async fn remove_also_deletes_a_leftover_staging_dir() {
    let s = setup().await;
    let d = download(&s, 34, "A", "a.vpk", vpk(b"v"), 100).await;
    s.env
        .lib
        .install_gb_addon(d, &chosen(&["a.vpk"]), &s.tf2, &no)
        .unwrap();
    common::write_file(
        &s.env.dir.gb_addon_dir().join(".staging-34/1/junk.vpk"),
        b"junk",
    );
    common::write_file(&s.env.dir.gb_addon_dir().join(".staging-340/1/x"), b"x");
    s.env
        .lib
        .remove_gb_addon(GbId::new(34), &s.tf2, &no)
        .unwrap();
    assert_eq!(
        common::tree(&s.env.dir.gb_addon_dir()),
        [".staging-340/1/x"],
        "only this add-on's staging goes"
    );
}
