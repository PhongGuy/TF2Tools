//! HUD install into tf/custom, one HUD at a time, and the installed-HUD scan.

mod common;

use std::fs;

use common::{LibraryEnv, library_env, set_setting, setting, tree, write_file, zip_bytes};
use tf2tools_core::{HudEntry, HudId, HudStatus, InstallError, LibraryKind, Provenance};

fn import(env: &LibraryEnv, folder: &str) -> HudEntry {
    let vdf = format!("{folder}/info.vdf");
    let res = format!("{folder}/resource/clientscheme.res");
    let bytes = zip_bytes(&[
        (vdf.as_str(), b"hud".as_slice()),
        (res.as_str(), b"scheme".as_slice()),
    ]);
    let archive = write_file(&env.tmp.path().join(format!("{folder}.zip")), &bytes);
    env.lib.import_hud(&archive, Provenance::Local).unwrap()
}

#[test]
fn install_copies_the_hud_without_metadata_and_records_it() {
    let env = library_env();
    let hud = import(&env, "rayshud");
    write_file(
        &env.dir
            .library_dir(LibraryKind::Huds)
            .join("rayshud/.tf2tools/thumbnail.webp"),
        b"thumb",
    );
    set_setting(&env.dir, "active_profile_id", "p1");
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    assert_eq!(
        tree(&env.tf2.custom_dir()),
        ["rayshud/info.vdf", "rayshud/resource/clientscheme.res"]
    );
    assert_eq!(
        setting(&env.dir, "installed_hud_folder").as_deref(),
        Some("rayshud")
    );
    assert_eq!(setting(&env.dir, "active_profile_id").as_deref(), Some(""));
}

#[test]
fn install_removes_every_other_hud_but_nothing_else() {
    let env = library_env();
    let hud = import(&env, "rayshud");
    let custom = env.tf2.custom_dir();
    write_file(&custom.join("otherhud/info.vdf"), b"x");
    write_file(&custom.join("thirdhud/info.vdf"), b"x");
    write_file(&custom.join("tf2tools-sounds/sound/ui/hitsound.wav"), b"w");
    write_file(&custom.join("readme.txt"), b"r");
    let named = env.lib.external_huds(&env.tf2).unwrap();
    env.lib
        .install_hud_discarding(hud.id(), &env.tf2, &named)
        .unwrap();
    assert_eq!(
        tree(&custom),
        [
            "rayshud/info.vdf",
            "rayshud/resource/clientscheme.res",
            "readme.txt",
            "tf2tools-sounds/sound/ui/hitsound.wav",
        ]
    );
}

#[test]
fn an_unknown_or_vanished_hud_leaves_the_installed_one() {
    let env = library_env();
    let custom = env.tf2.custom_dir();
    write_file(&custom.join("otherhud/info.vdf"), b"x");

    let err = env
        .lib
        .install_hud(&HudId::new("missing"), &env.tf2)
        .unwrap_err();
    assert!(matches!(err, InstallError::NotInLibrary { .. }), "{err:?}");

    let hud = import(&env, "gone");
    fs::remove_dir_all(env.dir.library_dir(LibraryKind::Huds).join("gone")).unwrap();
    let err = env.lib.install_hud(hud.id(), &env.tf2).unwrap_err();
    assert!(matches!(err, InstallError::NotInLibrary { .. }), "{err:?}");

    common::sql(&env.dir)
        .execute(
            "INSERT INTO library_huds (id, folder_name, name, created_at, updated_at)
             VALUES ('bad', '../../../Team Fortress 2/tf/custom/otherhud', 'bad', 't', 't')",
            [],
        )
        .unwrap();
    let err = env
        .lib
        .install_hud(&HudId::new("bad"), &env.tf2)
        .unwrap_err();
    assert!(matches!(err, InstallError::NotInLibrary { .. }), "{err:?}");

    assert!(custom.join("otherhud/info.vdf").is_file());
    assert_eq!(setting(&env.dir, "installed_hud_folder"), None);
}

#[test]
fn uninstall_removes_every_hud_and_clears_the_settings() {
    let env = library_env();
    let hud = import(&env, "rayshud");
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    let custom = env.tf2.custom_dir();
    write_file(&custom.join("otherhud/info.vdf"), b"x");
    write_file(&custom.join("keep/readme.txt"), b"k");
    set_setting(&env.dir, "active_profile_id", "p1");
    let named = env.lib.external_huds(&env.tf2).unwrap();
    env.lib.uninstall_hud_discarding(&env.tf2, &named).unwrap();
    assert_eq!(tree(&custom), ["keep/readme.txt"]);
    assert_eq!(
        setting(&env.dir, "installed_hud_folder").as_deref(),
        Some("")
    );
    assert_eq!(setting(&env.dir, "active_profile_id").as_deref(), Some(""));
}

#[cfg(unix)]
#[test]
fn uninstall_removes_a_hud_folder_whose_name_is_not_utf8() {
    use std::os::unix::ffi::OsStrExt;
    let env = library_env();
    let custom = env.tf2.custom_dir();
    let odd = custom.join(std::ffi::OsStr::from_bytes(b"hud\xff"));
    write_file(&odd.join("info.vdf"), b"x");
    let named = env.lib.external_huds(&env.tf2).unwrap();
    env.lib.uninstall_hud_discarding(&env.tf2, &named).unwrap();
    assert!(!odd.exists());
}

#[test]
fn status_tells_managed_unmanaged_and_multiple_apart() {
    let env = library_env();
    assert_eq!(
        env.lib.hud_status(&env.tf2).unwrap(),
        HudStatus::NotInstalled
    );

    let hud = import(&env, "rayshud");
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    assert_eq!(
        env.lib.hud_status(&env.tf2).unwrap(),
        HudStatus::Managed {
            folder_name: "rayshud".into(),
            id: hud.id().clone()
        }
    );

    env.lib.uninstall_hud(&env.tf2).unwrap();
    let custom = env.tf2.custom_dir();
    write_file(&custom.join("manual/info.vdf"), b"x");
    write_file(&custom.join("notahud/readme.txt"), b"x");
    assert_eq!(
        env.lib.hud_status(&env.tf2).unwrap(),
        HudStatus::Unmanaged {
            folder_name: "manual".into()
        }
    );

    write_file(&custom.join("zeta/info.vdf"), b"x");
    write_file(&custom.join("alpha/info.vdf"), b"x");
    assert_eq!(
        env.lib.hud_status(&env.tf2).unwrap(),
        HudStatus::Multiple {
            folder_names: vec!["alpha".into(), "manual".into(), "zeta".into()]
        }
    );
}

#[cfg(unix)]
#[test]
fn an_unwritable_custom_dir_is_an_io_error_and_settings_stay() {
    use std::os::unix::fs::PermissionsExt;
    let env = library_env();
    let hud = import(&env, "rayshud");
    let custom = env.tf2.custom_dir();
    fs::set_permissions(&custom, fs::Permissions::from_mode(0o555)).unwrap();
    let result = env.lib.install_hud(hud.id(), &env.tf2);
    fs::set_permissions(&custom, fs::Permissions::from_mode(0o755)).unwrap();
    let err = result.unwrap_err();
    assert!(matches!(err, InstallError::Io { .. }), "{err:?}");
    assert_eq!(setting(&env.dir, "installed_hud_folder"), None);
}
