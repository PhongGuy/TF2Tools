//! The live link's staged reinstall of the installed HUD.

mod common;

use std::fs;

use common::{LibraryEnv, library_env, set_setting, setting, tree, write_file, zip_bytes};
use tf2tools_core::{HudEntry, HudId, LibraryKind, Provenance, Reinstall, ReinstallError};

const STAGING: &str = ".tf2tools-reinstall";

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

fn library_file(env: &LibraryEnv, folder: &str, rel: &str) -> std::path::PathBuf {
    env.dir
        .library_dir(LibraryKind::Huds)
        .join(folder)
        .join(rel)
}

fn scheme(env: &LibraryEnv, folder: &str) -> String {
    fs::read_to_string(
        env.tf2
            .custom_dir()
            .join(folder)
            .join("resource/clientscheme.res"),
    )
    .unwrap()
}

#[test]
fn a_reinstall_updates_the_installed_copy_and_nothing_else() {
    let env = library_env();
    let hud = import(&env, "rayshud");
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    set_setting(&env.dir, "active_profile_id", "p1");
    let custom = env.tf2.custom_dir();
    write_file(&custom.join("readme.txt"), b"r");
    fs::write(
        library_file(&env, "rayshud", "resource/clientscheme.res"),
        "edited",
    )
    .unwrap();
    write_file(
        &library_file(&env, "rayshud", "resource/ui/added.res"),
        b"added",
    );
    write_file(
        &library_file(&env, "rayshud", ".tf2tools/maker.json"),
        b"{}",
    );

    let done = env.lib.reinstall_hud(hud.id(), &env.tf2).unwrap();
    assert_eq!(
        done,
        Reinstall::Updated {
            folder: "rayshud".to_owned()
        }
    );
    assert_eq!(
        tree(&custom),
        [
            "rayshud/info.vdf",
            "rayshud/resource/clientscheme.res",
            "rayshud/resource/ui/added.res",
            "readme.txt",
        ]
    );
    assert_eq!(scheme(&env, "rayshud"), "edited");
    assert_eq!(
        setting(&env.dir, "installed_hud_folder").as_deref(),
        Some("rayshud")
    );
    assert_eq!(
        setting(&env.dir, "active_profile_id").as_deref(),
        Some("p1")
    );
}

#[test]
fn a_file_removed_from_the_library_goes_from_the_installed_copy() {
    let env = library_env();
    let hud = import(&env, "rayshud");
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    fs::remove_file(library_file(&env, "rayshud", "resource/clientscheme.res")).unwrap();
    env.lib.reinstall_hud(hud.id(), &env.tf2).unwrap();
    assert_eq!(tree(&env.tf2.custom_dir()), ["rayshud/info.vdf"]);
}

#[test]
fn a_hud_that_is_not_the_installed_one_is_never_reinstalled() {
    let env = library_env();
    let installed = import(&env, "rayshud");
    let other = import(&env, "budhud");
    let custom = env.tf2.custom_dir();

    assert_eq!(
        env.lib.reinstall_hud(installed.id(), &env.tf2).unwrap(),
        Reinstall::NotInstalled,
        "nothing installed"
    );
    env.lib.install_hud(installed.id(), &env.tf2).unwrap();
    let before = tree(&custom);
    assert_eq!(
        env.lib.reinstall_hud(other.id(), &env.tf2).unwrap(),
        Reinstall::NotInstalled
    );
    assert_eq!(
        env.lib
            .reinstall_hud(&HudId::new("missing"), &env.tf2)
            .unwrap(),
        Reinstall::NotInstalled
    );
    write_file(&custom.join("otherhud/info.vdf"), b"x");
    assert_eq!(
        env.lib.reinstall_hud(installed.id(), &env.tf2).unwrap(),
        Reinstall::NotInstalled,
        "two HUDs installed: neither is reliably active"
    );
    let mut expected = before;
    expected.push("otherhud/info.vdf".to_owned());
    expected.sort();
    assert_eq!(tree(&custom), expected);
}

#[test]
fn an_installed_hud_gone_from_the_library_is_left_in_place() {
    let env = library_env();
    let hud = import(&env, "rayshud");
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    fs::remove_dir_all(env.dir.library_dir(LibraryKind::Huds).join("rayshud")).unwrap();
    let err = env.lib.reinstall_hud(hud.id(), &env.tf2).unwrap_err();
    assert!(
        matches!(err, ReinstallError::NotInLibrary { .. }),
        "{err:?}"
    );
    assert_eq!(scheme(&env, "rayshud"), "scheme");
    assert!(!env.tf2.custom_dir().join(STAGING).exists());
}

// Unix permissions stand in for a file TF2 holds open; each test gives them back before the
// temp dir is deleted.
#[cfg(unix)]
fn set_mode(path: &std::path::Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

#[cfg(unix)]
#[test]
fn a_copy_that_fails_leaves_the_installed_copy() {
    let env = library_env();
    let hud = import(&env, "rayshud");
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    let unreadable = library_file(&env, "rayshud", "resource/clientscheme.res");
    fs::write(&unreadable, "edited").unwrap();
    set_mode(&unreadable, 0o000);
    let result = env.lib.reinstall_hud(hud.id(), &env.tf2);
    set_mode(&unreadable, 0o644);

    let err = result.unwrap_err();
    assert!(matches!(err, ReinstallError::Unchanged { .. }), "{err:?}");
    assert_eq!(scheme(&env, "rayshud"), "scheme");
    assert!(!env.tf2.custom_dir().join(STAGING).exists());
}

#[cfg(unix)]
#[test]
fn an_installed_folder_that_cannot_move_is_left_in_place() {
    let env = library_env();
    let hud = import(&env, "rayshud");
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    fs::write(
        library_file(&env, "rayshud", "resource/clientscheme.res"),
        "edited",
    )
    .unwrap();
    // A directory without write permission cannot move to another parent: it holds `..`.
    let installed = env.tf2.custom_dir().join("rayshud");
    set_mode(&installed, 0o555);
    let result = env.lib.reinstall_hud(hud.id(), &env.tf2);
    set_mode(&installed, 0o755);

    let err = result.unwrap_err();
    let ReinstallError::InUse { path, .. } = &err else {
        panic!("{err:?}");
    };
    assert_eq!(path, &installed);
    assert_eq!(scheme(&env, "rayshud"), "scheme");
    assert!(!env.tf2.custom_dir().join(STAGING).exists());
    assert_eq!(
        env.lib.hud_status(&env.tf2).unwrap(),
        tf2tools_core::HudStatus::Managed {
            folder_name: "rayshud".to_owned(),
            id: hud.id().clone()
        }
    );
}

#[test]
fn a_reinstall_first_puts_back_what_a_crash_left() {
    let env = library_env();
    let hud = import(&env, "rayshud");
    env.lib.install_hud(hud.id(), &env.tf2).unwrap();
    let custom = env.tf2.custom_dir();
    // A crash between the two renames: the installed copy sits aside, nothing in its place.
    fs::create_dir_all(custom.join(STAGING).join("old")).unwrap();
    fs::write(custom.join(STAGING).join("swapping"), "rayshud").unwrap();
    fs::rename(
        custom.join("rayshud"),
        custom.join(STAGING).join("old/rayshud"),
    )
    .unwrap();
    fs::write(
        library_file(&env, "rayshud", "resource/clientscheme.res"),
        "edited",
    )
    .unwrap();
    assert_eq!(
        env.lib.reinstall_hud(hud.id(), &env.tf2).unwrap(),
        Reinstall::Updated {
            folder: "rayshud".to_owned()
        }
    );
    assert_eq!(scheme(&env, "rayshud"), "edited");
    assert!(!custom.join(STAGING).exists());
}
