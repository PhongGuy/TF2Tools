//! HUD folders in tf/custom that the library cannot put back: they are never removed until
//! the caller names them, and keeping a copy goes first.

mod common;

use std::fs;
use std::path::Path;

use common::{
    LibraryEnv, builtin_catalog, import_hud_as, library_env, set_setting, setting, tree, write_file,
};
use tf2tools_core::{
    ApplyProfileError, ApplyStep, DraftSlot, ExternalHud, HudEntry, HudId, HudStatus, InstallError,
    LibraryKind, Profile, ProfileDraft, Provenance, Reinstall, ReinstallError,
};
// Only the Unix permission and link tests check why keeping a HUD failed.
#[cfg(unix)]
use tf2tools_core::KeepHudError;

fn place(env: &LibraryEnv, folder: &str) {
    let root = env.tf2.custom_dir().join(folder);
    write_file(&root.join("info.vdf"), folder.as_bytes());
    write_file(&root.join("resource/ui/hudlayout.res"), b"hand made");
    write_file(&root.join("materials/vgui/empty.vtf"), &[0, 159, 146, 150]);
}

/// Every file under `root` with its bytes, for byte-for-byte comparisons.
fn snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
    tree(root)
        .into_iter()
        .map(|rel| {
            let bytes = fs::read(root.join(&rel)).unwrap_or_default();
            (rel, bytes)
        })
        .collect()
}

fn hud(env: &LibraryEnv, folder: &str) -> HudEntry {
    import_hud_as(env, folder, Provenance::Local)
}

fn names(huds: &[ExternalHud]) -> Vec<&str> {
    huds.iter().map(ExternalHud::folder_name).collect()
}

fn library_folders(env: &LibraryEnv) -> Vec<String> {
    let huds = env.lib.list_huds().unwrap();
    huds.iter().map(|h| h.folder_name().to_owned()).collect()
}

fn hud_profile(env: &LibraryEnv, slot: DraftSlot<HudId>) -> Profile {
    let draft = ProfileDraft::new("Comp").with_hud(slot);
    env.lib.create_profile(&draft).unwrap()
}

#[test]
fn only_folders_the_library_cannot_put_back_are_external() {
    let env = library_env();
    let installed = hud(&env, "rayshud");
    env.lib.install_hud(installed.id(), &env.tf2).unwrap();
    place(&env, "manual");
    place(&env, "another");
    write_file(&env.tf2.custom_dir().join("notahud/readme.txt"), b"x");
    assert_eq!(
        names(&env.lib.external_huds(&env.tf2).unwrap()),
        ["another", "manual"]
    );
}

#[test]
fn a_hud_installed_under_another_name_is_not_external() {
    let env = library_env();
    let installed = hud(&env, "rayshud");
    let folder = env.dir.library_dir(LibraryKind::Huds).join("rayshud");
    write_file(
        &folder.join("resource/clientscheme.res"),
        b"#base \"../../custom/rays/scheme.res\"",
    );
    let done = env.lib.install_hud(installed.id(), &env.tf2).unwrap();
    assert_eq!(done.folder(), "rays");
    assert!(env.lib.external_huds(&env.tf2).unwrap().is_empty());
}

#[test]
fn a_library_row_with_no_files_cannot_put_its_hud_back() {
    let env = library_env();
    let gone = hud(&env, "rayshud");
    env.lib.install_hud(gone.id(), &env.tf2).unwrap();
    fs::remove_dir_all(env.dir.library_dir(LibraryKind::Huds).join("rayshud")).unwrap();
    assert_eq!(
        names(&env.lib.external_huds(&env.tf2).unwrap()),
        ["rayshud"]
    );
}

#[test]
fn a_refused_install_changes_nothing() {
    let env = library_env();
    let previous = hud(&env, "rayshud");
    env.lib.install_hud(previous.id(), &env.tf2).unwrap();
    let next = hud(&env, "budhud");
    place(&env, "manual");
    set_setting(&env.dir, "active_profile_id", "p1");
    let custom = env.tf2.custom_dir();
    let before = snapshot(&custom);

    let err = env.lib.install_hud(next.id(), &env.tf2).unwrap_err();

    match err {
        InstallError::ExternalHuds(huds) => assert_eq!(names(&huds), ["manual"]),
        other => panic!("{other:?}"),
    }
    assert_eq!(snapshot(&custom), before);
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
fn a_refused_uninstall_changes_nothing() {
    let env = library_env();
    place(&env, "manual");
    let custom = env.tf2.custom_dir();
    let before = snapshot(&custom);
    let err = env.lib.uninstall_hud(&env.tf2).unwrap_err();
    assert!(matches!(err, InstallError::ExternalHuds(_)), "{err:?}");
    assert_eq!(snapshot(&custom), before);
}

#[test]
fn keeping_a_copy_adds_it_and_then_the_install_goes_through() {
    let env = library_env();
    let next = hud(&env, "budhud");
    place(&env, "manual");
    let custom = env.tf2.custom_dir();
    let original = snapshot(&custom.join("manual"));

    let found = env.lib.external_huds(&env.tf2).unwrap();
    let kept = env.lib.keep_external_huds(&found, &env.tf2).unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].name(), "manual");
    assert_eq!(*kept[0].provenance(), Provenance::Local);
    assert!(kept[0].folder_name() == "manual");
    assert!(library_folders(&env).contains(&"manual".to_owned()));

    env.lib.install_hud(next.id(), &env.tf2).unwrap();
    assert!(!custom.join("manual").exists());
    let library = env.dir.library_dir(LibraryKind::Huds).join("manual");
    assert_eq!(snapshot(&library), original);

    // The kept HUD installs like any other, and nothing has to be asked again.
    env.lib.install_hud(kept[0].id(), &env.tf2).unwrap();
    assert_eq!(snapshot(&custom.join("manual")), original);
}

#[test]
fn keeping_never_overwrites_a_stray_library_folder() {
    let env = library_env();
    place(&env, "manual");
    let stray = env.dir.library_dir(LibraryKind::Huds).join("manual");
    write_file(&stray.join("stray.txt"), b"mine");

    let found = env.lib.external_huds(&env.tf2).unwrap();
    let kept = env.lib.keep_external_huds(&found, &env.tf2).unwrap();

    assert_eq!(kept[0].folder_name(), "manual-2");
    assert!(stray.join("stray.txt").is_file());
}

#[cfg(unix)]
#[test]
fn a_failed_copy_removes_nothing_and_leaves_the_previous_hud() {
    use std::os::unix::fs::PermissionsExt;
    let env = library_env();
    let previous = hud(&env, "rayshud");
    env.lib.install_hud(previous.id(), &env.tf2).unwrap();
    place(&env, "manual");
    let custom = env.tf2.custom_dir();
    let before = snapshot(&custom);
    let found = env.lib.external_huds(&env.tf2).unwrap();

    let huds = env.dir.library_dir(LibraryKind::Huds);
    fs::set_permissions(&huds, fs::Permissions::from_mode(0o555)).unwrap();
    let result = env.lib.keep_external_huds(&found, &env.tf2);
    fs::set_permissions(&huds, fs::Permissions::from_mode(0o755)).unwrap();

    let err = result.unwrap_err();
    assert!(matches!(err, KeepHudError::Io { .. }), "{err:?}");
    assert_eq!(snapshot(&custom), before);
    assert_eq!(library_folders(&env), ["rayshud"]);
    assert_eq!(
        setting(&env.dir, "installed_hud_folder").as_deref(),
        Some("rayshud")
    );
}

#[cfg(unix)]
#[test]
fn a_link_the_copy_would_lose_is_refused_rather_than_dropped() {
    let env = library_env();
    place(&env, "manual");
    let root = env.tf2.custom_dir().join("manual");
    std::os::unix::fs::symlink("resource", root.join("link")).unwrap();
    let before = snapshot(&root);

    let found = env.lib.external_huds(&env.tf2).unwrap();
    let err = env.lib.keep_external_huds(&found, &env.tf2).unwrap_err();

    assert!(matches!(err, KeepHudError::NotCopyable { .. }), "{err:?}");
    assert_eq!(snapshot(&root), before);
    assert!(library_folders(&env).is_empty());
    assert!(
        !env.dir
            .library_dir(LibraryKind::Huds)
            .join("manual")
            .exists()
    );
}

#[test]
fn replacing_removes_only_with_that_explicit_choice() {
    let env = library_env();
    let next = hud(&env, "budhud");
    place(&env, "manual");
    let custom = env.tf2.custom_dir();
    let found = env.lib.external_huds(&env.tf2).unwrap();
    env.lib
        .install_hud_discarding(next.id(), &env.tf2, &found)
        .unwrap();
    assert!(!custom.join("manual").exists());
    assert!(custom.join("budhud/info.vdf").is_file());
    assert!(!library_folders(&env).contains(&"manual".to_owned()));
}

#[test]
fn a_choice_never_covers_a_hud_that_appeared_after_it() {
    let env = library_env();
    let next = hud(&env, "budhud");
    place(&env, "manual");
    let found = env.lib.external_huds(&env.tf2).unwrap();
    place(&env, "newer");
    let custom = env.tf2.custom_dir();
    let before = snapshot(&custom);

    let err = env
        .lib
        .install_hud_discarding(next.id(), &env.tf2, &found)
        .unwrap_err();

    match err {
        InstallError::ExternalHuds(huds) => assert_eq!(names(&huds), ["newer"]),
        other => panic!("{other:?}"),
    }
    assert_eq!(snapshot(&custom), before);
}

#[test]
fn uninstalling_without_keeping_removes_what_was_named() {
    let env = library_env();
    place(&env, "manual");
    let found = env.lib.external_huds(&env.tf2).unwrap();
    env.lib.uninstall_hud_discarding(&env.tf2, &found).unwrap();
    assert_eq!(
        env.lib.hud_status(&env.tf2).unwrap(),
        HudStatus::NotInstalled
    );
}

#[test]
fn non_hud_folders_are_untouched() {
    let env = library_env();
    let next = hud(&env, "budhud");
    place(&env, "manual");
    let custom = env.tf2.custom_dir();
    write_file(&custom.join("sounds/sound/ui/hitsound.wav"), b"wav");
    write_file(&custom.join("loose.vpk"), b"vpk");
    let found = env.lib.external_huds(&env.tf2).unwrap();
    env.lib
        .install_hud_discarding(next.id(), &env.tf2, &found)
        .unwrap();
    env.lib.uninstall_hud_discarding(&env.tf2, &[]).unwrap();
    assert_eq!(fs::read(custom.join("loose.vpk")).unwrap(), b"vpk");
    assert_eq!(
        fs::read(custom.join("sounds/sound/ui/hitsound.wav")).unwrap(),
        b"wav"
    );
}

#[test]
fn a_non_hud_folder_holding_the_huds_name_is_never_merged_into() {
    let env = library_env();
    let next = hud(&env, "budhud");
    let custom = env.tf2.custom_dir();
    write_file(&custom.join("budhud/notes.txt"), b"mine");
    write_file(
        &custom.join("budhud/resource/clientscheme.res"),
        b"mine too",
    );
    let before = snapshot(&custom.join("budhud"));

    let done = env.lib.install_hud(next.id(), &env.tf2).unwrap();

    assert_eq!(done.folder(), "budhud-2");
    assert_eq!(done.blocked_name(), Some("budhud"));
    assert!(custom.join("budhud-2/info.vdf").is_file());
    assert_eq!(snapshot(&custom.join("budhud")), before);
    env.lib.uninstall_hud(&env.tf2).unwrap();
    assert_eq!(snapshot(&custom.join("budhud")), before);
    assert!(!custom.join("budhud-2").exists());
}

#[test]
fn an_edited_install_is_refused_as_changes_that_would_be_lost() {
    let env = library_env();
    let installed = hud(&env, "rayshud");
    let next = hud(&env, "budhud");
    env.lib.install_hud(installed.id(), &env.tf2).unwrap();
    let custom = env.tf2.custom_dir();
    write_file(
        &custom.join("rayshud/resource/clientscheme.res"),
        b"tweaked",
    );
    let before = snapshot(&custom);

    let err = env.lib.install_hud(next.id(), &env.tf2).unwrap_err();

    match err {
        InstallError::ExternalHuds(huds) => {
            assert_eq!(names(&huds), ["rayshud"]);
            assert!(huds[0].has_changes());
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(snapshot(&custom), before);
}

#[test]
fn an_edit_of_the_same_size_counts_as_a_change() {
    let env = library_env();
    let installed = hud(&env, "rayshud");
    env.lib.install_hud(installed.id(), &env.tf2).unwrap();
    let file = env
        .tf2
        .custom_dir()
        .join("rayshud/resource/clientscheme.res");
    fs::write(&file, b"SCHEME").unwrap();
    // Set by hand, since a write right after the install can share its clock tick.
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(60);
    fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(later)
        .unwrap();
    assert_eq!(
        names(&env.lib.external_huds(&env.tf2).unwrap()),
        ["rayshud"]
    );
}

#[test]
fn an_install_whose_times_moved_but_bytes_did_not_is_no_change() {
    let env = library_env();
    let installed = hud(&env, "rayshud");
    let next = hud(&env, "budhud");
    env.lib.install_hud(installed.id(), &env.tf2).unwrap();
    let custom = env.tf2.custom_dir();
    let file = custom.join("rayshud/resource/clientscheme.res");
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(60);
    fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(later)
        .unwrap();

    assert!(env.lib.external_huds(&env.tf2).unwrap().is_empty());
    change_library_file(&env, "rayshud", b"saved");
    assert_eq!(
        names(&env.lib.external_huds(&env.tf2).unwrap()),
        ["rayshud"],
        "a save moved the library on, so nothing proves it any more: ask"
    );
    change_library_file(&env, "rayshud", b"scheme");
    let done = env.lib.reinstall_hud(installed.id(), &env.tf2).unwrap();
    assert!(matches!(done, Reinstall::Updated { .. }), "{done:?}");
    env.lib.install_hud(next.id(), &env.tf2).unwrap();
    assert!(!custom.join("rayshud").exists());
}

#[test]
fn a_file_added_to_an_install_counts_as_a_change() {
    let env = library_env();
    let installed = hud(&env, "rayshud");
    env.lib.install_hud(installed.id(), &env.tf2).unwrap();
    write_file(&env.tf2.custom_dir().join("rayshud/cfg/extra.cfg"), b"mine");
    let found = env.lib.external_huds(&env.tf2).unwrap();
    assert_eq!(names(&found), ["rayshud"]);
}

#[test]
fn a_file_removed_from_an_install_counts_as_a_change() {
    let env = library_env();
    let installed = hud(&env, "rayshud");
    env.lib.install_hud(installed.id(), &env.tf2).unwrap();
    fs::remove_file(
        env.tf2
            .custom_dir()
            .join("rayshud/resource/clientscheme.res"),
    )
    .unwrap();
    assert_eq!(
        names(&env.lib.external_huds(&env.tf2).unwrap()),
        ["rayshud"]
    );
}

#[test]
fn a_cache_the_game_writes_is_not_a_change() {
    let env = library_env();
    let installed = hud(&env, "rayshud");
    env.lib.install_hud(installed.id(), &env.tf2).unwrap();
    let custom = env.tf2.custom_dir();
    write_file(&custom.join("rayshud/sound/sound.cache"), b"game");
    assert!(env.lib.external_huds(&env.tf2).unwrap().is_empty());
    env.lib.uninstall_hud(&env.tf2).unwrap();
    assert!(!custom.join("rayshud").exists());
}

#[test]
fn a_library_newer_than_the_install_is_not_a_change() {
    let env = library_env();
    let installed = hud(&env, "rayshud");
    env.lib.install_hud(installed.id(), &env.tf2).unwrap();
    write_file(
        &env.dir
            .library_dir(LibraryKind::Huds)
            .join("rayshud/resource/clientscheme.res"),
        b"saved later",
    );
    assert!(env.lib.external_huds(&env.tf2).unwrap().is_empty());
}

#[test]
fn a_hand_placed_folder_named_like_a_library_hud_is_asked_about() {
    let env = library_env();
    hud(&env, "rayshud");
    place(&env, "rayshud");
    let found = env.lib.external_huds(&env.tf2).unwrap();
    assert_eq!(names(&found), ["rayshud"]);
    assert!(found[0].has_changes());
}

#[test]
fn a_hand_placed_copy_equal_to_the_library_can_be_put_back() {
    let env = library_env();
    hud(&env, "rayshud");
    let root = env.tf2.custom_dir().join("rayshud");
    write_file(&root.join("info.vdf"), b"hud");
    write_file(&root.join("resource/clientscheme.res"), b"scheme");
    assert!(env.lib.external_huds(&env.tf2).unwrap().is_empty());
}

#[test]
fn a_hand_placed_copy_of_the_same_sizes_is_compared_by_its_bytes() {
    let env = library_env();
    hud(&env, "rayshud");
    let root = env.tf2.custom_dir().join("rayshud");
    write_file(&root.join("info.vdf"), b"hud");
    write_file(&root.join("resource/clientscheme.res"), b"SCHEME");
    assert_eq!(
        names(&env.lib.external_huds(&env.tf2).unwrap()),
        ["rayshud"]
    );
}

/// An install made before fingerprints: the record names the folder and nothing else.
fn older_install(env: &LibraryEnv, id: &HudEntry) {
    env.lib.install_hud(id.id(), &env.tf2).unwrap();
    common::sql(&env.dir)
        .execute(
            "DELETE FROM settings WHERE key = 'installed_hud_fingerprint'",
            [],
        )
        .unwrap();
}

#[test]
fn an_older_install_matching_the_library_is_not_asked_about() {
    let env = library_env();
    let installed = hud(&env, "rayshud");
    older_install(&env, &installed);
    assert!(env.lib.external_huds(&env.tf2).unwrap().is_empty());
}

#[test]
fn an_older_install_that_differs_from_the_library_is_asked_about() {
    let env = library_env();
    let installed = hud(&env, "rayshud");
    older_install(&env, &installed);
    write_file(
        &env.tf2
            .custom_dir()
            .join("rayshud/resource/clientscheme.res"),
        b"tweaked",
    );
    let found = env.lib.external_huds(&env.tf2).unwrap();
    assert_eq!(names(&found), ["rayshud"]);
    assert!(found[0].has_changes());
}

#[test]
fn keeping_changes_takes_a_new_name_and_leaves_the_original() {
    let env = library_env();
    let installed = hud(&env, "rayshud");
    let next = hud(&env, "budhud");
    env.lib.install_hud(installed.id(), &env.tf2).unwrap();
    let custom = env.tf2.custom_dir();
    write_file(
        &custom.join("rayshud/resource/clientscheme.res"),
        b"tweaked",
    );
    let library = env.dir.library_dir(LibraryKind::Huds);
    let original = snapshot(&library.join("rayshud"));

    let found = env.lib.external_huds(&env.tf2).unwrap();
    let kept = env.lib.keep_external_huds(&found, &env.tf2).unwrap();
    assert_eq!(kept[0].folder_name(), "rayshud-2");
    env.lib
        .install_hud_discarding(next.id(), &env.tf2, &found)
        .unwrap();

    assert_eq!(snapshot(&library.join("rayshud")), original);
    assert_eq!(
        fs::read(library.join("rayshud-2/resource/clientscheme.res")).unwrap(),
        b"tweaked"
    );
    assert!(!custom.join("rayshud").exists());
}

#[test]
fn keep_skips_a_hud_that_left_since_it_was_listed() {
    let env = library_env();
    place(&env, "manual");
    let found = env.lib.external_huds(&env.tf2).unwrap();
    fs::remove_dir_all(env.tf2.custom_dir().join("manual")).unwrap();
    let kept = env.lib.keep_external_huds(&found, &env.tf2).unwrap();
    assert!(kept.is_empty());
    assert!(library_folders(&env).is_empty());
}

#[cfg(unix)]
#[test]
fn a_failed_install_copy_leaves_nothing_for_the_retry_to_trip_on() {
    use std::os::unix::fs::PermissionsExt;
    let env = library_env();
    let next = hud(&env, "rayshud");
    // A texture, since the name scan reads text files and would fail before the copy starts.
    let file = write_file(
        &env.dir
            .library_dir(LibraryKind::Huds)
            .join("rayshud/materials/vgui/a.vtf"),
        b"vtf",
    );
    fs::set_permissions(&file, fs::Permissions::from_mode(0o000)).unwrap();
    let failed = env.lib.install_hud(next.id(), &env.tf2);
    fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(matches!(failed, Err(InstallError::Io { .. })), "{failed:?}");
    let custom = env.tf2.custom_dir();
    assert!(!custom.join("rayshud").exists());

    let done = env.lib.install_hud(next.id(), &env.tf2).unwrap();
    assert_eq!(done.folder(), "rayshud");
    assert_eq!(done.blocked_name(), None);
    env.lib.uninstall_hud(&env.tf2).unwrap();
    assert!(tree(&custom).is_empty());
}

#[test]
fn library_huds_are_replaced_without_being_asked() {
    let env = library_env();
    let first = hud(&env, "rayshud");
    let second = hud(&env, "budhud");
    env.lib.install_hud(first.id(), &env.tf2).unwrap();
    env.lib.install_hud(second.id(), &env.tf2).unwrap();
    assert!(!env.tf2.custom_dir().join("rayshud").exists());
    env.lib.uninstall_hud(&env.tf2).unwrap();
    assert!(!env.tf2.custom_dir().join("budhud").exists());
}

#[test]
fn a_profile_with_a_hud_refuses_before_any_step_runs() {
    let env = library_env();
    let next = hud(&env, "budhud");
    let profile = hud_profile(&env, DraftSlot::Item(next.id().clone()));
    place(&env, "manual");
    set_setting(&env.dir, "active_profile_id", "p1");
    let custom = env.tf2.custom_dir();
    let before = snapshot(&custom);

    let err = env
        .lib
        .apply_profile(profile.id(), &env.tf2, &builtin_catalog(), false)
        .unwrap_err();

    match err {
        ApplyProfileError::ExternalHuds(huds) => assert_eq!(names(&huds), ["manual"]),
        other => panic!("{other:?}"),
    }
    assert_eq!(snapshot(&custom), before);
    assert_eq!(
        setting(&env.dir, "active_profile_id").as_deref(),
        Some("p1")
    );
}

#[test]
fn a_profile_without_a_hud_refuses_too() {
    let env = library_env();
    let profile = hud_profile(&env, DraftSlot::Empty);
    place(&env, "manual");
    let err = env
        .lib
        .apply_profile(profile.id(), &env.tf2, &builtin_catalog(), false)
        .unwrap_err();
    assert!(matches!(err, ApplyProfileError::ExternalHuds(_)), "{err:?}");
    assert!(env.tf2.custom_dir().join("manual/info.vdf").is_file());
}

#[test]
fn install_anyway_with_the_hud_missing_refuses_too() {
    let env = library_env();
    let gone = hud(&env, "budhud");
    let profile = hud_profile(&env, DraftSlot::Item(gone.id().clone()));
    env.lib.delete_hud(gone.id()).unwrap();
    place(&env, "manual");
    let err = env
        .lib
        .apply_profile(profile.id(), &env.tf2, &builtin_catalog(), true)
        .unwrap_err();
    assert!(matches!(err, ApplyProfileError::ExternalHuds(_)), "{err:?}");
    assert!(env.tf2.custom_dir().join("manual/info.vdf").is_file());
}

#[test]
fn a_profile_goes_through_once_the_huds_are_named() {
    let env = library_env();
    let next = hud(&env, "budhud");
    let profile = hud_profile(&env, DraftSlot::Item(next.id().clone()));
    place(&env, "manual");
    let found = env.lib.external_huds(&env.tf2).unwrap();
    let report = env
        .lib
        .apply_profile_discarding(profile.id(), &env.tf2, &builtin_catalog(), false, &found)
        .unwrap();
    assert!(report.failed().is_empty(), "{:?}", report.failed());
    assert!(report.applied().contains(&ApplyStep::Hud));
    assert!(!env.tf2.custom_dir().join("manual").exists());
    assert!(env.tf2.custom_dir().join("budhud/info.vdf").is_file());
}

#[cfg(unix)]
#[test]
fn a_folder_name_that_is_not_utf8_can_be_kept_and_removed() {
    use std::os::unix::ffi::OsStrExt;
    let env = library_env();
    let next = hud(&env, "budhud");
    let custom = env.tf2.custom_dir();
    let odd = custom.join(std::ffi::OsStr::from_bytes(b"hud\xff"));
    write_file(&odd.join("info.vdf"), b"x");

    let found = env.lib.external_huds(&env.tf2).unwrap();
    assert_eq!(found.len(), 1);
    env.lib.keep_external_huds(&found, &env.tf2).unwrap();
    env.lib
        .install_hud_discarding(next.id(), &env.tf2, &found)
        .unwrap();
    assert!(!odd.exists());
    assert_eq!(library_folders(&env).len(), 2);
}

fn change_library_file(env: &LibraryEnv, folder: &str, bytes: &[u8]) {
    write_file(
        &env.dir
            .library_dir(LibraryKind::Huds)
            .join(folder)
            .join("resource/clientscheme.res"),
        bytes,
    );
}

#[test]
fn reinstall_updates_its_own_install_again_and_again() {
    let env = library_env();
    let installed = hud(&env, "rayshud");
    env.lib.install_hud(installed.id(), &env.tf2).unwrap();
    let file = env
        .tf2
        .custom_dir()
        .join("rayshud/resource/clientscheme.res");
    for text in [&b"one"[..], b"two"] {
        change_library_file(&env, "rayshud", text);
        let done = env.lib.reinstall_hud(installed.id(), &env.tf2).unwrap();
        assert!(matches!(done, Reinstall::Updated { .. }), "{done:?}");
        assert_eq!(fs::read(&file).unwrap(), text);
    }
    assert!(env.lib.external_huds(&env.tf2).unwrap().is_empty());
}

#[test]
fn reinstall_leaves_an_edited_install_alone() {
    let env = library_env();
    let installed = hud(&env, "rayshud");
    env.lib.install_hud(installed.id(), &env.tf2).unwrap();
    let custom = env.tf2.custom_dir();
    write_file(
        &custom.join("rayshud/resource/clientscheme.res"),
        b"tweaked",
    );
    change_library_file(&env, "rayshud", b"saved");
    let before = snapshot(&custom);

    let err = env.lib.reinstall_hud(installed.id(), &env.tf2).unwrap_err();

    assert!(matches!(err, ReinstallError::Changed { .. }), "{err:?}");
    assert_eq!(snapshot(&custom), before);
}

#[test]
fn reinstall_leaves_a_hand_placed_folder_of_the_same_name_alone() {
    let env = library_env();
    let hud_entry = hud(&env, "rayshud");
    place(&env, "rayshud");
    let custom = env.tf2.custom_dir();
    let before = snapshot(&custom);
    let err = env.lib.reinstall_hud(hud_entry.id(), &env.tf2).unwrap_err();
    assert!(matches!(err, ReinstallError::Changed { .. }), "{err:?}");
    assert_eq!(snapshot(&custom), before);
}

fn record(env: &LibraryEnv) -> [Option<String>; 4] {
    [
        "installed_hud_folder",
        "installed_hud_id",
        "installed_hud_fingerprint",
        "active_profile_id",
    ]
    .map(|key| setting(&env.dir, key))
}

fn named(env: &LibraryEnv, folder: &str) -> Vec<ExternalHud> {
    let mut found = env.lib.external_huds(&env.tf2).unwrap();
    found.retain(|hud| hud.folder_name() == folder);
    found
}

/// A crash's leftovers: a marked swap that set `folder` aside.
fn set_aside(env: &LibraryEnv, folder: &str) {
    let staging = env.tf2.custom_dir().join(".tf2tools-reinstall");
    write_file(&staging.join("swapping"), folder.as_bytes());
    write_file(&staging.join("old").join(folder).join("info.vdf"), b"x");
}

#[test]
fn removing_one_hud_leaves_the_others_as_they_were() {
    let env = library_env();
    let installed = hud(&env, "rayshud");
    env.lib.install_hud(installed.id(), &env.tf2).unwrap();
    place(&env, "manual");
    place(&env, "another");
    let custom = env.tf2.custom_dir();
    let (ours, theirs) = (
        snapshot(&custom.join("rayshud")),
        snapshot(&custom.join("manual")),
    );

    let discard = named(&env, "another");
    env.lib
        .uninstall_one_hud(&env.tf2, "another", &discard)
        .unwrap();

    assert!(!custom.join("another").exists());
    assert_eq!(snapshot(&custom.join("rayshud")), ours);
    assert_eq!(snapshot(&custom.join("manual")), theirs);
}

#[test]
fn removing_one_external_hud_is_refused_until_it_is_named() {
    let env = library_env();
    place(&env, "manual");
    place(&env, "another");
    let custom = env.tf2.custom_dir();
    let before = snapshot(&custom);

    let refused = env.lib.uninstall_one_hud(&env.tf2, "manual", &[]);
    let Err(InstallError::ExternalHuds(found)) = refused else {
        panic!("removed without being named: {refused:?}");
    };

    assert_eq!(names(&found), ["manual"]);
    assert_eq!(snapshot(&custom), before);
}

#[test]
fn naming_another_hud_does_not_cover_the_one_being_removed() {
    let env = library_env();
    place(&env, "manual");
    place(&env, "another");
    let discard = named(&env, "another");
    let refused = env.lib.uninstall_one_hud(&env.tf2, "manual", &discard);
    assert!(matches!(refused, Err(InstallError::ExternalHuds(_))));
    assert!(env.tf2.custom_dir().join("manual/info.vdf").is_file());
}

#[test]
fn an_edited_install_is_refused_when_it_alone_is_removed() {
    let env = library_env();
    let installed = hud(&env, "rayshud");
    env.lib.install_hud(installed.id(), &env.tf2).unwrap();
    place(&env, "manual");
    let edited = env.tf2.custom_dir().join("rayshud/resource/mine.res");
    write_file(&edited, b"edited");

    let refused = env.lib.uninstall_one_hud(&env.tf2, "rayshud", &[]);
    assert!(matches!(refused, Err(InstallError::ExternalHuds(_))));
    assert!(edited.is_file());
}

#[test]
fn removing_the_recorded_hud_clears_the_record() {
    let env = library_env();
    let installed = hud(&env, "rayshud");
    env.lib.install_hud(installed.id(), &env.tf2).unwrap();
    set_setting(&env.dir, "active_profile_id", "p1");
    place(&env, "manual");

    env.lib.uninstall_one_hud(&env.tf2, "rayshud", &[]).unwrap();

    assert_eq!(record(&env), [const { Some(String::new()) }; 4]);
    assert_eq!(
        env.lib.hud_status(&env.tf2).unwrap(),
        HudStatus::Unmanaged {
            folder_name: "manual".to_owned()
        }
    );
}

#[test]
fn removing_another_hud_keeps_the_record_of_the_install() {
    let env = library_env();
    let installed = hud(&env, "rayshud");
    env.lib.install_hud(installed.id(), &env.tf2).unwrap();
    set_setting(&env.dir, "active_profile_id", "p1");
    let before = record(&env);
    place(&env, "manual");

    let discard = named(&env, "manual");
    env.lib
        .uninstall_one_hud(&env.tf2, "manual", &discard)
        .unwrap();

    assert_eq!(record(&env), before);
    assert_eq!(
        env.lib.hud_status(&env.tf2).unwrap(),
        HudStatus::Managed {
            folder_name: "rayshud".to_owned(),
            id: installed.id().clone()
        }
    );
    assert!(env.lib.external_huds(&env.tf2).unwrap().is_empty());
}

#[test]
fn a_removed_hud_does_not_come_back_from_a_crash_leftover() {
    let env = library_env();
    let installed = hud(&env, "rayshud");
    env.lib.install_hud(installed.id(), &env.tf2).unwrap();
    place(&env, "manual");
    set_aside(&env, "rayshud");

    env.lib.uninstall_one_hud(&env.tf2, "rayshud", &[]).unwrap();
    env.lib.recover_reinstall(&env.tf2).unwrap();

    assert!(!env.tf2.custom_dir().join("rayshud").exists());
}

#[test]
fn removing_another_hud_leaves_a_crash_leftover_to_recover() {
    let env = library_env();
    let installed = hud(&env, "rayshud");
    env.lib.install_hud(installed.id(), &env.tf2).unwrap();
    place(&env, "manual");
    set_aside(&env, "rayshud");

    let discard = named(&env, "manual");
    env.lib
        .uninstall_one_hud(&env.tf2, "manual", &discard)
        .unwrap();

    assert!(env.tf2.custom_dir().join(".tf2tools-reinstall").exists());
}

#[test]
fn only_a_hud_folder_can_be_removed_by_name() {
    let env = library_env();
    let custom = env.tf2.custom_dir();
    place(&env, "manual");
    write_file(&custom.join("notahud/readme.txt"), b"x");
    write_file(&custom.parent().unwrap().join("outside/info.vdf"), b"x");
    let before = snapshot(&custom);

    for folder in ["notahud", "gone", "../outside", "..", ""] {
        env.lib.uninstall_one_hud(&env.tf2, folder, &[]).unwrap();
    }

    assert_eq!(snapshot(&custom), before);
    assert!(custom.parent().unwrap().join("outside/info.vdf").is_file());
}

#[cfg(unix)]
#[test]
fn one_folder_whose_name_is_not_utf8_can_be_removed() {
    use std::os::unix::ffi::OsStrExt;
    let env = library_env();
    let custom = env.tf2.custom_dir();
    let odd = custom.join(std::ffi::OsStr::from_bytes(b"hud\xff"));
    write_file(&odd.join("info.vdf"), b"x");
    place(&env, "manual");

    let found = env.lib.external_huds(&env.tf2).unwrap();
    let name = "hud\u{fffd}";
    let discard: Vec<ExternalHud> = found
        .into_iter()
        .filter(|hud| hud.folder_name() == name)
        .collect();
    env.lib.uninstall_one_hud(&env.tf2, name, &discard).unwrap();

    assert!(!odd.exists());
    assert!(custom.join("manual/info.vdf").is_file());
}

#[cfg(unix)]
#[test]
fn removing_a_linked_hud_takes_the_link_and_leaves_its_target() {
    let env = library_env();
    let custom = env.tf2.custom_dir();
    let target = env.tmp.path().join("elsewhere/linked");
    write_file(&target.join("info.vdf"), b"x");
    write_file(&target.join("resource/mine.res"), b"hand made");
    std::os::unix::fs::symlink(&target, custom.join("linked")).unwrap();
    place(&env, "manual");
    let before = snapshot(&target);

    let refused = env.lib.uninstall_one_hud(&env.tf2, "linked", &[]);
    let Err(InstallError::ExternalHuds(found)) = refused else {
        panic!("removed without being named: {refused:?}");
    };
    assert_eq!(names(&found), ["linked"]);

    env.lib
        .uninstall_one_hud(&env.tf2, "linked", &found)
        .unwrap();

    assert!(custom.join("linked").symlink_metadata().is_err());
    assert_eq!(snapshot(&target), before);
    assert!(custom.join("manual/info.vdf").is_file());
}
