//! Installing and updating GameBanana maps as one tf/custom folder per item.

use crate::common::{self, gb_addon_file, gb_addon_profile};
use crate::gb_addon_install::{Setup, no, setup, yes};

use tf2tools_core::{GbAddonEntry, GbId, GbMapDownload, GbMapInstallError, GbMapRemoveError};

fn bsp(tag: &[u8]) -> Vec<u8> {
    let mut body = b"VBSP".to_vec();
    body.extend_from_slice(tag);
    body
}

/// Map item `id` named `name` whose only file is a zip of `entries`, dated `date`, downloaded.
async fn download(
    s: &Setup,
    id: u64,
    name: &str,
    entries: &[(&str, &[u8])],
    date: i64,
) -> GbMapDownload {
    s.env.server.reset().await;
    let body = common::zip_bytes(entries);
    let f = gb_addon_file(&s.env, id, 0, "pack.zip", &body, date).await;
    let profile = gb_addon_profile(&s.env, id, name, vec![f]).await;
    s.env
        .client
        .download_gb_map(&profile, &|_| {})
        .await
        .unwrap()
}

fn chosen(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| (*n).to_owned()).collect()
}

fn custom(s: &Setup) -> Vec<String> {
    common::tree(&s.tf2.custom_dir())
}

fn row(s: &Setup, id: u64) -> Option<GbAddonEntry> {
    s.env
        .lib
        .list_gb_addons(Some(&s.tf2))
        .unwrap()
        .into_iter()
        .find(|e| e.gb_id() == GbId::new(id))
}

#[tokio::test]
async fn a_chosen_subset_installs_into_one_folder_even_while_tf2_runs() {
    let s = setup().await;
    let d = download(
        &s,
        5,
        "City Pack",
        &[
            ("maps/cp_a.bsp", &bsp(b"a")),
            ("maps/cp_a.nav", b"nav"),
            ("maps/cp_b.bsp", &bsp(b"b")),
            ("maps/cp_c.bsp", &bsp(b"c")),
            ("scripts/population/cp_a_adv.pop", b"pop"),
            ("resource/ui/x.res", b"res"),
        ],
        100,
    )
    .await;
    let entry = s
        .env
        .lib
        .install_gb_map(d, &chosen(&["cp_a", "cp_c"]), &s.tf2, &yes)
        .unwrap();
    let f = "tf2tools-gb-5-city-pack";
    assert_eq!(
        custom(&s),
        [
            format!("{f}/maps/cp_a.bsp"),
            format!("{f}/maps/cp_a.nav"),
            format!("{f}/maps/cp_c.bsp"),
            format!("{f}/scripts/population/cp_a_adv.pop"),
        ]
    );
    assert_eq!(
        std::fs::read(s.tf2.custom_dir().join(f).join("maps/cp_c.bsp")).unwrap(),
        bsp(b"c")
    );
    assert_eq!(entry.section(), "maps");
    assert_eq!(entry.payloads(), ["cp_a", "cp_c"]);
    assert_eq!(entry.files(), [format!("custom/{f}")]);
    assert!(row(&s, 5).unwrap().is_installed());
}

#[tokio::test]
async fn maps_the_game_has_are_dropped_and_all_present_is_refused() {
    let s = setup().await;
    common::write_file(&s.tf2.tf_dir().join("maps/cp_b.bsp"), b"VBSP stock");
    let d = download(
        &s,
        5,
        "P",
        &[("maps/cp_a.bsp", &bsp(b"a")), ("maps/cp_b.bsp", &bsp(b"b"))],
        100,
    )
    .await;
    let entry = s
        .env
        .lib
        .install_gb_map(d, &chosen(&["cp_a", "cp_b"]), &s.tf2, &no)
        .unwrap();
    assert_eq!(entry.payloads(), ["cp_a"]);
    assert_eq!(custom(&s), ["tf2tools-gb-5-p/maps/cp_a.bsp"]);

    // Another item shipping cp_a now finds it in item 5's folder.
    let d = download(
        &s,
        6,
        "Q",
        &[("maps/CP_A.bsp", &bsp(b"q")), ("maps/cp_b.bsp", &bsp(b"q"))],
        100,
    )
    .await;
    let err = s
        .env
        .lib
        .install_gb_map(d, &chosen(&["CP_A", "cp_b"]), &s.tf2, &no)
        .unwrap_err();
    assert!(
        matches!(&err, GbMapInstallError::AlreadyPresent { names } if names == &["CP_A", "cp_b"]),
        "{err:?}"
    );
    assert_eq!(custom(&s), ["tf2tools-gb-5-p/maps/cp_a.bsp"]);
    assert!(row(&s, 6).is_none());
}

#[tokio::test]
async fn nothing_or_an_unknown_name_is_refused() {
    let s = setup().await;
    let d = download(&s, 5, "P", &[("maps/cp_a.bsp", &bsp(b"a"))], 100).await;
    let err = s.env.lib.install_gb_map(d, &[], &s.tf2, &no).unwrap_err();
    assert!(matches!(err, GbMapInstallError::NothingChosen), "{err:?}");
    let d = download(&s, 5, "P", &[("maps/cp_a.bsp", &bsp(b"a"))], 100).await;
    let err = s
        .env
        .lib
        .install_gb_map(d, &chosen(&["cp_zz"]), &s.tf2, &no)
        .unwrap_err();
    assert!(
        matches!(&err, GbMapInstallError::UnknownMap { name } if name == "cp_zz"),
        "{err:?}"
    );
    assert!(custom(&s).is_empty());
}

#[tokio::test]
async fn installing_again_over_its_own_folder_is_not_already_present() {
    let s = setup().await;
    let d = download(&s, 5, "P", &[("maps/cp_a.bsp", &bsp(b"1"))], 100).await;
    s.env
        .lib
        .install_gb_map(d, &chosen(&["cp_a"]), &s.tf2, &no)
        .unwrap();
    let d = download(&s, 5, "P", &[("maps/cp_a.bsp", &bsp(b"2"))], 100).await;
    s.env
        .lib
        .install_gb_map(d, &chosen(&["cp_a"]), &s.tf2, &no)
        .unwrap();
    let path = s.tf2.custom_dir().join("tf2tools-gb-5-p/maps/cp_a.bsp");
    assert_eq!(std::fs::read(path).unwrap(), bsp(b"2"));
}

#[tokio::test]
async fn an_update_replaces_renamed_maps_and_refuses_while_tf2_runs() {
    let s = setup().await;
    let d = download(&s, 7, "X", &[("maps/cp_x_rc2.bsp", &bsp(b"2"))], 100).await;
    s.env
        .lib
        .install_gb_map(d, &chosen(&["cp_x_rc2"]), &s.tf2, &no)
        .unwrap();

    let d = download(&s, 7, "X", &[("maps/cp_x_rc3.bsp", &bsp(b"3"))], 200).await;
    let err = s
        .env
        .lib
        .update_gb_map(d, &chosen(&["cp_x_rc3"]), &s.tf2, &yes)
        .unwrap_err();
    assert!(matches!(err, GbMapInstallError::Tf2Running), "{err:?}");
    assert_eq!(custom(&s), ["tf2tools-gb-7-x/maps/cp_x_rc2.bsp"]);
    assert_eq!(row(&s, 7).unwrap().file_date(), 100);

    let d = download(&s, 7, "X", &[("maps/cp_x_rc3.bsp", &bsp(b"3"))], 200).await;
    let entry = s
        .env
        .lib
        .update_gb_map(d, &chosen(&["cp_x_rc3"]), &s.tf2, &no)
        .unwrap();
    assert_eq!(custom(&s), ["tf2tools-gb-7-x/maps/cp_x_rc3.bsp"]);
    assert_eq!(entry.payloads(), ["cp_x_rc3"]);
    assert_eq!(entry.file_date(), 200);
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn on_linux_the_folder_installs_in_lowercase() {
    let s = setup().await;
    let d = download(
        &s,
        8,
        "Mixed",
        &[
            ("Maps/Koth_Mixed.bsp", &bsp(b"m")),
            ("Maps/Koth_Mixed.nav", b"nav"),
            ("Materials/VGUI/Maps/Menu_Koth.vtf", b"vtf"),
        ],
        100,
    )
    .await;
    let entry = s
        .env
        .lib
        .install_gb_map(d, &chosen(&["Koth_Mixed"]), &s.tf2, &no)
        .unwrap();
    assert_eq!(
        custom(&s),
        [
            "tf2tools-gb-8-mixed/maps/koth_mixed.bsp",
            "tf2tools-gb-8-mixed/maps/koth_mixed.nav",
            "tf2tools-gb-8-mixed/materials/vgui/maps/menu_koth.vtf",
        ]
    );
    assert_eq!(entry.payloads(), ["koth_mixed"]);
}

#[cfg(not(target_os = "linux"))]
#[tokio::test]
async fn elsewhere_file_names_keep_their_case() {
    let s = setup().await;
    let d = download(&s, 8, "Mixed", &[("Maps/Koth_Mixed.bsp", &bsp(b"m"))], 100).await;
    let entry = s
        .env
        .lib
        .install_gb_map(d, &chosen(&["Koth_Mixed"]), &s.tf2, &no)
        .unwrap();
    assert_eq!(custom(&s), ["tf2tools-gb-8-mixed/maps/Koth_Mixed.bsp"]);
    assert_eq!(entry.payloads(), ["Koth_Mixed"]);
}

#[cfg(unix)]
#[tokio::test]
async fn a_write_failure_leaves_no_folder_and_the_row_not_installed() {
    use std::os::unix::fs::PermissionsExt;
    let s = setup().await;
    let d = download(&s, 9, "P", &[("maps/cp_a.bsp", &bsp(b"a"))], 100).await;
    let dir = s.tf2.custom_dir();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    // Root writes through any mode, so there is nothing to test there.
    if std::fs::write(dir.join(".probe"), b"").is_ok() {
        std::fs::remove_file(dir.join(".probe")).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }
    let result = s.env.lib.install_gb_map(d, &chosen(&["cp_a"]), &s.tf2, &no);
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        matches!(result, Err(GbMapInstallError::Io { .. })),
        "{result:?}"
    );
    assert!(custom(&s).is_empty());
    assert!(!row(&s, 9).unwrap().is_installed());
}

#[cfg(unix)]
#[tokio::test]
async fn an_update_whose_copy_fails_keeps_the_installed_map() {
    use std::os::unix::fs::PermissionsExt;
    let s = setup().await;
    let d = download(&s, 7, "X", &[("maps/cp_x_rc2.bsp", &bsp(b"2"))], 100).await;
    s.env
        .lib
        .install_gb_map(d, &chosen(&["cp_x_rc2"]), &s.tf2, &no)
        .unwrap();

    // An unreadable new map makes the copy into the game fail part-way.
    let d = download(&s, 7, "X", &[("maps/cp_x_rc3.bsp", &bsp(b"3"))], 200).await;
    let new_map = d.maps()[0].bsp().to_path_buf();
    std::fs::set_permissions(&new_map, std::fs::Permissions::from_mode(0o000)).unwrap();
    // Root reads through any mode, so there is nothing to test there.
    if std::fs::read(&new_map).is_ok() {
        return;
    }
    let result = s
        .env
        .lib
        .update_gb_map(d, &chosen(&["cp_x_rc3"]), &s.tf2, &no);
    assert!(
        matches!(result, Err(GbMapInstallError::Io { .. })),
        "{result:?}"
    );
    assert_eq!(custom(&s), ["tf2tools-gb-7-x/maps/cp_x_rc2.bsp"]);
    let old = s.tf2.custom_dir().join("tf2tools-gb-7-x/maps/cp_x_rc2.bsp");
    assert_eq!(std::fs::read(old).unwrap(), bsp(b"2"));
}

#[cfg(unix)]
#[tokio::test]
async fn an_update_installs_even_when_the_old_folder_cannot_be_fully_deleted() {
    use std::os::unix::fs::PermissionsExt;
    let s = setup().await;
    let d = download(
        &s,
        7,
        "X",
        &[
            ("maps/cp_x_rc2.bsp", &bsp(b"2")),
            ("scripts/population/cp_x_rc2.pop", b"pop"),
        ],
        100,
    )
    .await;
    s.env
        .lib
        .install_gb_map(d, &chosen(&["cp_x_rc2"]), &s.tf2, &no)
        .unwrap();

    // A folder of the old install whose files cannot be deleted, as a held file would be.
    let held = s
        .tf2
        .custom_dir()
        .join("tf2tools-gb-7-x/scripts/population");
    std::fs::set_permissions(&held, std::fs::Permissions::from_mode(0o555)).unwrap();
    // Root deletes through any mode, so there is nothing to test there.
    if std::fs::write(held.join(".probe"), b"").is_ok() {
        std::fs::remove_file(held.join(".probe")).unwrap();
        return;
    }
    let d = download(&s, 7, "X", &[("maps/cp_x_rc3.bsp", &bsp(b"3"))], 200).await;
    let result = s
        .env
        .lib
        .update_gb_map(d, &chosen(&["cp_x_rc3"]), &s.tf2, &no);
    for dir in common::tree(&s.tf2.custom_dir()) {
        if let Some(rel) = dir.strip_suffix("/cp_x_rc2.pop") {
            let path = s.tf2.custom_dir().join(rel);
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    let entry = result.unwrap();
    assert_eq!(entry.payloads(), ["cp_x_rc3"]);
    let installed: Vec<String> = custom(&s)
        .into_iter()
        .filter(|p| p.starts_with("tf2tools-gb-7-x/"))
        .collect();
    assert_eq!(installed, ["tf2tools-gb-7-x/maps/cp_x_rc3.bsp"]);
}

#[tokio::test]
async fn a_map_folder_left_aside_by_an_interrupted_update_is_put_back() {
    let s = setup().await;
    let d = download(&s, 7, "X", &[("maps/cp_x.bsp", &bsp(b"x"))], 100).await;
    s.env
        .lib
        .install_gb_map(d, &chosen(&["cp_x"]), &s.tf2, &no)
        .unwrap();
    // A crash between the two renames of a swap leaves the old folder only in staging.
    let custom_dir = s.tf2.custom_dir();
    let aside = custom_dir.join(".tf2tools-map-staging/old");
    std::fs::create_dir_all(&aside).unwrap();
    std::fs::rename(
        custom_dir.join("tf2tools-gb-7-x"),
        aside.join("tf2tools-gb-7-x"),
    )
    .unwrap();

    let d = download(&s, 8, "Y", &[("maps/cp_y.bsp", &bsp(b"y"))], 100).await;
    s.env
        .lib
        .install_gb_map(d, &chosen(&["cp_y"]), &s.tf2, &no)
        .unwrap();
    assert_eq!(
        custom(&s),
        [
            "tf2tools-gb-7-x/maps/cp_x.bsp",
            "tf2tools-gb-8-y/maps/cp_y.bsp",
        ]
    );
}

#[tokio::test]
async fn remove_deletes_only_the_maps_folder_and_its_row() {
    let s = setup().await;
    common::write_file(&s.tf2.tf_dir().join("maps/itemtest.bsp"), b"VBSP stock");
    common::write_file(&s.tf2.custom_dir().join("myhud/maps/x.bsp"), b"VBSP hud");
    let d = download(&s, 9, "P", &[("maps/cp_a.bsp", &bsp(b"a"))], 100).await;
    s.env
        .lib
        .install_gb_map(d, &chosen(&["cp_a"]), &s.tf2, &no)
        .unwrap();

    let err = s
        .env
        .lib
        .remove_gb_map(GbId::new(9), &s.tf2, &yes)
        .unwrap_err();
    assert!(matches!(err, GbMapRemoveError::Tf2Running), "{err:?}");
    assert!(custom(&s).contains(&"tf2tools-gb-9-p/maps/cp_a.bsp".to_owned()));

    s.env.lib.remove_gb_map(GbId::new(9), &s.tf2, &no).unwrap();
    assert_eq!(custom(&s), ["myhud/maps/x.bsp"]);
    assert!(s.tf2.tf_dir().join("maps/itemtest.bsp").exists());
    assert!(row(&s, 9).is_none());

    let err = s
        .env
        .lib
        .remove_gb_map(GbId::new(9), &s.tf2, &no)
        .unwrap_err();
    assert!(
        matches!(err, GbMapRemoveError::NotInLibrary { gb_id } if gb_id == GbId::new(9)),
        "{err:?}"
    );
}

#[tokio::test]
async fn a_map_row_is_due_for_the_daily_update_check() {
    let s = setup().await;
    let d = download(&s, 10, "P", &[("maps/cp_a.bsp", &bsp(b"a"))], 100).await;
    s.env
        .lib
        .install_gb_map(d, &chosen(&["cp_a"]), &s.tf2, &no)
        .unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let later = i64::try_from(now).unwrap() + 2 * 86_400;
    assert!(
        s.env
            .lib
            .gb_addons_due_for_check(later)
            .unwrap()
            .contains(&GbId::new(10))
    );
}

#[tokio::test]
async fn remove_clears_its_own_set_aside_folder_but_not_another_items() {
    let s = setup().await;
    for (id, name, map) in [(7, "X", "cp_x"), (77, "Y", "cp_y")] {
        let entry = format!("maps/{map}.bsp");
        let d = download(&s, id, name, &[(entry.as_str(), &bsp(b"m"))], 100).await;
        s.env
            .lib
            .install_gb_map(d, &chosen(&[map]), &s.tf2, &no)
            .unwrap();
    }
    // Interrupted updates left both items' old folders only in staging.
    let custom_dir = s.tf2.custom_dir();
    let aside = custom_dir.join(".tf2tools-map-staging/old");
    std::fs::create_dir_all(&aside).unwrap();
    for folder in ["tf2tools-gb-7-x", "tf2tools-gb-77-y"] {
        std::fs::rename(custom_dir.join(folder), aside.join(folder)).unwrap();
    }

    s.env.lib.remove_gb_map(GbId::new(7), &s.tf2, &no).unwrap();
    assert!(!aside.join("tf2tools-gb-7-x").exists());
    assert!(aside.join("tf2tools-gb-77-y/maps/cp_y.bsp").exists());

    // The next install puts back the other item's folder, never the removed one.
    let d = download(&s, 8, "Z", &[("maps/cp_z.bsp", &bsp(b"z"))], 100).await;
    s.env
        .lib
        .install_gb_map(d, &chosen(&["cp_z"]), &s.tf2, &no)
        .unwrap();
    assert_eq!(
        custom(&s),
        [
            "tf2tools-gb-77-y/maps/cp_y.bsp",
            "tf2tools-gb-8-z/maps/cp_z.bsp",
        ]
    );
}
