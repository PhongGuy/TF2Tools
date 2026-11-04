//! Install and delete of a made HUD whose re-save a crash interrupted, leaving `<folder>.old`.

mod common;

use common::hud_stock::stock_files;
use common::{LibraryEnv, import_hud_as, library_env, tree, write_file};
use std::path::{Path, PathBuf};
use tf2tools_core::{
    Change, InstallError, LibraryKind, MakeTarget, Platform, Provenance, SavedHud, SetKey,
};

fn red_health() -> Change {
    Change::set(
        1,
        SetKey::new(
            "resource/ui/hudplayerhealth.res",
            &["Resource/UI/HudPlayerHealth.res", "PlayerStatusHealthValue"],
            "fgcolor",
            None,
            Some("TanDark"),
            "255 80 80 255",
        ),
    )
}

fn old_of(dir: &Path) -> PathBuf {
    dir.with_file_name(format!(
        "{}.old",
        dir.file_name().unwrap().to_string_lossy()
    ))
}

/// A saved made HUD and its live folder.
fn made(env: &LibraryEnv) -> (SavedHud, PathBuf) {
    let saved = env
        .lib
        .save_made_hud(
            &MakeTarget::New,
            "Mine",
            &stock_files(),
            Platform::Linux,
            &[red_health()],
        )
        .unwrap();
    let live = env
        .dir
        .library_dir(LibraryKind::Huds)
        .join(saved.entry().folder_name());
    (saved, live)
}

/// A made HUD caught between a re-save's renames: only `<folder>.old` is on disk.
fn stranded(env: &LibraryEnv) -> (SavedHud, PathBuf) {
    let (saved, live) = made(env);
    std::fs::rename(&live, old_of(&live)).unwrap();
    (saved, live)
}

#[test]
fn install_copies_from_a_stranded_previous_save() {
    let env = library_env();
    let (saved, live) = stranded(&env);
    let installed = env.lib.install_hud(saved.entry().id(), &env.tf2).unwrap();
    assert_eq!(installed.folder(), saved.entry().folder_name());
    assert_eq!(
        tree(&env.tf2.custom_dir().join(installed.folder())),
        ["info.vdf", "resource/ui/hudplayerhealth.res"]
    );
    assert!(!live.exists(), "install renamed nothing");
    assert!(old_of(&live).join(".tf2tools/maker.json").is_file());
}

#[test]
fn delete_removes_a_stranded_previous_save() {
    let env = library_env();
    let (saved, live) = stranded(&env);
    env.lib.delete_hud(saved.entry().id()).unwrap();
    assert!(!old_of(&live).exists());
    assert!(env.lib.list_huds().unwrap().is_empty());
}

#[test]
fn delete_removes_both_folders_after_a_crash_after_the_swap() {
    let env = library_env();
    let (saved, live) = made(&env);
    write_file(&old_of(&live).join(".tf2tools/maker.json"), b"{}");
    env.lib.delete_hud(saved.entry().id()).unwrap();
    assert!(!live.exists());
    assert!(!old_of(&live).exists());
}

#[test]
fn a_plain_folder_named_like_an_old_save_is_never_touched() {
    let env = library_env();
    let huds = env.dir.library_dir(LibraryKind::Huds);
    let rays = import_hud_as(&env, "rayshud", Provenance::Local);
    let other = import_hud_as(&env, "rayshud.old", Provenance::Local);
    assert_eq!(other.folder_name(), "rayshud.old");
    std::fs::remove_dir_all(huds.join("rayshud")).unwrap();
    let install = env.lib.install_hud(rays.id(), &env.tf2);
    assert!(
        matches!(install, Err(InstallError::NotInLibrary { .. })),
        "{install:?}"
    );
    env.lib.delete_hud(rays.id()).unwrap();
    assert!(huds.join("rayshud.old/info.vdf").is_file());
}

#[cfg(unix)]
#[test]
fn a_symlink_named_like_an_old_save_is_never_followed() {
    let env = library_env();
    let (saved, live) = made(&env);
    let elsewhere = env
        .dir
        .library_dir(LibraryKind::Huds)
        .with_file_name("elsewhere");
    std::fs::rename(&live, &elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, old_of(&live)).unwrap();
    let install = env.lib.install_hud(saved.entry().id(), &env.tf2);
    assert!(
        matches!(install, Err(InstallError::NotInLibrary { .. })),
        "{install:?}"
    );
    assert!(
        !env.tf2
            .custom_dir()
            .join(saved.entry().folder_name())
            .exists()
    );
    assert!(elsewhere.join(".tf2tools/maker.json").is_file());
}
