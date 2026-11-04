//! Install detection and the AppImage swap, on temp dirs only.

mod common;

use std::path::{Path, PathBuf};

#[cfg(unix)]
use common::update::{appimage_offer, publish};
use common::update::{feed, installer_offer, publish_installer};
#[cfg(unix)]
use tf2tools_core::Applied;
use tf2tools_core::{InstallTarget, StagedUpdate, UpdateApplyError, apply};

fn p(s: &str) -> PathBuf {
    PathBuf::from(s)
}

// The AppImage swap tests that use these helpers run on unix only.
#[cfg(unix)]
fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

/// Downloads `payload` as 6.0.1 for the AppImage at `app`, which must exist.
#[cfg(unix)]
async fn stage(app: &Path, payload: &[u8]) -> StagedUpdate {
    let feed = feed().await;
    publish(&feed, "6.0.1", payload).await;
    let offer = appimage_offer(&feed, app).await;
    feed.client
        .download(
            &offer,
            &InstallTarget::AppImage(app.to_path_buf()),
            |_, _| {},
        )
        .await
        .unwrap()
}

// `/home/...` is not an absolute path on Windows.
#[cfg(unix)]
#[test]
fn an_appimage_run_from_its_mount_is_an_appimage() {
    let target = InstallTarget::detect_from(
        Some(p("/home/u/Apps/tf2tools_6.0.0_x86_64.AppImage")),
        Some(p("/tmp/.mount_tf2abc")),
        &p("/tmp/.mount_tf2abc/usr/bin/tf2tools"),
    );
    assert_eq!(
        target,
        InstallTarget::AppImage(p("/home/u/Apps/tf2tools_6.0.0_x86_64.AppImage"))
    );
}

#[test]
fn a_deb_install_is_unmanaged_even_with_an_inherited_appimage_env() {
    let target = InstallTarget::detect_from(
        Some(p("/home/u/Apps/other.AppImage")),
        Some(p("/tmp/.mount_other")),
        &p("/usr/bin/tf2tools"),
    );
    assert_eq!(target, InstallTarget::Unmanaged);
}

#[test]
fn a_make_install_is_unmanaged() {
    let target = InstallTarget::detect_from(None, None, &p("/home/u/.local/lib/tf2tools/tf2tools"));
    assert_eq!(target, InstallTarget::Unmanaged);
}

#[test]
fn a_sibling_mount_sharing_a_name_prefix_is_not_inside_appdir() {
    let target = InstallTarget::detect_from(
        Some(p("/home/u/Apps/tf2tools.AppImage")),
        Some(p("/tmp/.mount_tf2")),
        &p("/tmp/.mount_tf2x/usr/bin/tf2tools"),
    );
    assert_eq!(target, InstallTarget::Unmanaged);
}

#[test]
fn half_an_appimage_environment_is_unmanaged() {
    let exe = p("/tmp/.mount_tf2abc/usr/bin/tf2tools");
    let only_appimage =
        InstallTarget::detect_from(Some(p("/home/u/tf2tools.AppImage")), None, &exe);
    let only_appdir = InstallTarget::detect_from(None, Some(p("/tmp/.mount_tf2abc")), &exe);
    assert_eq!(only_appimage, InstallTarget::Unmanaged);
    assert_eq!(only_appdir, InstallTarget::Unmanaged);
}

#[test]
fn a_relative_or_empty_appimage_path_is_unmanaged() {
    let exe = p("/tmp/.mount_tf2abc/usr/bin/tf2tools");
    for appimage in ["tf2tools.AppImage", ""] {
        let target =
            InstallTarget::detect_from(Some(p(appimage)), Some(p("/tmp/.mount_tf2abc")), &exe);
        assert_eq!(target, InstallTarget::Unmanaged, "{appimage:?}");
    }
}

#[test]
fn a_relative_or_empty_appdir_is_unmanaged() {
    // Every path starts with the empty path, so an empty $APPDIR would claim any binary.
    let exe = p("usr/bin/tf2tools");
    for appdir in ["usr", ""] {
        let target =
            InstallTarget::detect_from(Some(p("/home/u/tf2tools.AppImage")), Some(p(appdir)), &exe);
        assert_eq!(target, InstallTarget::Unmanaged, "{appdir:?}");
    }
}

#[test]
fn the_test_binary_is_unmanaged() {
    // The host's own $APPIMAGE is never read; an injected one cannot claim this binary.
    let exe = std::env::current_exe().unwrap();
    let target = InstallTarget::detect_from(
        Some(p("/home/u/Apps/tf2tools.AppImage")),
        Some(p("/tmp/.mount_tf2abc")),
        &exe,
    );
    assert_eq!(target, InstallTarget::Unmanaged);
}

#[test]
fn an_exe_beside_the_uninstaller_is_the_windows_installer() {
    let tmp = tempfile::tempdir().unwrap();
    let exe = tmp.path().join("tf2tools.exe");
    std::fs::write(&exe, b"exe").unwrap();
    std::fs::write(tmp.path().join("uninstall.exe"), b"nsis").unwrap();
    assert_eq!(
        InstallTarget::detect_installer_from(&exe),
        InstallTarget::WindowsInstaller
    );
}

#[test]
fn an_exe_without_the_uninstaller_is_unmanaged() {
    // A dev build or a copied exe has no uninstaller and must not run installers.
    let tmp = tempfile::tempdir().unwrap();
    let exe = tmp.path().join("tf2tools.exe");
    std::fs::write(&exe, b"exe").unwrap();
    assert_eq!(
        InstallTarget::detect_installer_from(&exe),
        InstallTarget::Unmanaged
    );
}

#[test]
fn a_directory_named_like_the_uninstaller_is_not_one() {
    let tmp = tempfile::tempdir().unwrap();
    let exe = tmp.path().join("tf2tools.exe");
    std::fs::write(&exe, b"exe").unwrap();
    std::fs::create_dir(tmp.path().join("uninstall.exe")).unwrap();
    assert_eq!(
        InstallTarget::detect_installer_from(&exe),
        InstallTarget::Unmanaged
    );
}

#[test]
fn the_test_binary_is_not_an_installed_copy() {
    let exe = std::env::current_exe().unwrap();
    assert_eq!(
        InstallTarget::detect_installer_from(&exe),
        InstallTarget::Unmanaged
    );
}

#[cfg(unix)]
#[tokio::test]
async fn apply_replaces_the_appimage_and_keeps_a_backup() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let app = tmp.path().join("tf2tools.AppImage");
    std::fs::write(&app, b"old").unwrap();
    let staged = stage(&app, b"new payload").await;
    assert_eq!(staged.path().parent(), app.parent());
    assert_eq!(
        apply(staged).unwrap(),
        Applied::Replaced {
            relaunch: app.clone()
        }
    );
    assert_eq!(std::fs::read(&app).unwrap(), b"new payload");
    let mode = std::fs::metadata(&app).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o755);
    assert_eq!(
        std::fs::read(tmp.path().join("tf2tools.AppImage.bak")).unwrap(),
        b"old"
    );
    assert_eq!(
        names(tmp.path()),
        ["tf2tools.AppImage", "tf2tools.AppImage.bak"]
    );
}

#[cfg(unix)]
#[tokio::test]
async fn an_older_backup_is_replaced() {
    let tmp = tempfile::tempdir().unwrap();
    let app = tmp.path().join("tf2tools.AppImage");
    std::fs::write(&app, b"old").unwrap();
    std::fs::write(tmp.path().join("tf2tools.AppImage.bak"), b"older").unwrap();
    let staged = stage(&app, b"new payload").await;
    apply(staged).unwrap();
    assert_eq!(
        std::fs::read(tmp.path().join("tf2tools.AppImage.bak")).unwrap(),
        b"old"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_planted_backup_link_is_replaced_not_followed() {
    let tmp = tempfile::tempdir().unwrap();
    let app = tmp.path().join("tf2tools.AppImage");
    std::fs::write(&app, b"old").unwrap();
    let victim = tmp.path().join("victim");
    std::fs::write(&victim, b"victim").unwrap();
    let bak = tmp.path().join("tf2tools.AppImage.bak");
    std::os::unix::fs::symlink(&victim, &bak).unwrap();
    let staged = stage(&app, b"new payload").await;
    apply(staged).unwrap();
    assert_eq!(std::fs::read(&victim).unwrap(), b"victim");
    assert!(std::fs::symlink_metadata(&bak).unwrap().is_file());
    assert_eq!(std::fs::read(&bak).unwrap(), b"old");
}

#[cfg(unix)]
#[tokio::test]
async fn a_staged_file_swapped_for_a_link_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let app = tmp.path().join("tf2tools.AppImage");
    std::fs::write(&app, b"old").unwrap();
    let victim = tmp.path().join("victim");
    std::fs::write(&victim, b"victim").unwrap();
    std::fs::set_permissions(&victim, std::fs::Permissions::from_mode(0o600)).unwrap();
    let staged = stage(&app, b"new payload").await;
    std::fs::remove_file(staged.path()).unwrap();
    std::os::unix::fs::symlink(&victim, staged.path()).unwrap();
    let result = apply(staged);
    assert!(
        matches!(result, Err(UpdateApplyError::Unsupported { .. })),
        "{result:?}"
    );
    assert_eq!(std::fs::read(&app).unwrap(), b"old");
    let mode = std::fs::metadata(&victim).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
    assert_eq!(names(tmp.path()), ["tf2tools.AppImage", "victim"]);
}

#[cfg(unix)]
#[tokio::test]
async fn a_read_only_directory_is_not_writable_and_changes_nothing() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let app = tmp.path().join("tf2tools.AppImage");
    std::fs::write(&app, b"old").unwrap();
    let staged = stage(&app, b"new payload").await;
    std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
    let result = apply(staged);
    std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        matches!(result, Err(UpdateApplyError::NotWritable { .. })),
        "{result:?}"
    );
    assert_eq!(std::fs::read(&app).unwrap(), b"old");
    assert!(!tmp.path().join("tf2tools.AppImage.bak").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn an_old_backup_in_a_read_only_directory_is_not_writable() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let app = tmp.path().join("tf2tools.AppImage");
    std::fs::write(&app, b"old").unwrap();
    let bak = tmp.path().join("tf2tools.AppImage.bak");
    std::fs::write(&bak, b"older").unwrap();
    let staged = stage(&app, b"new payload").await;
    std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
    let result = apply(staged);
    std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        matches!(result, Err(UpdateApplyError::NotWritable { .. })),
        "{result:?}"
    );
    assert_eq!(std::fs::read(&app).unwrap(), b"old");
    assert_eq!(std::fs::read(&bak).unwrap(), b"older");
}

#[cfg(unix)]
#[tokio::test]
async fn a_symlinked_appimage_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let real_dir = tmp.path().join("real");
    let link_dir = tmp.path().join("apps");
    std::fs::create_dir_all(&real_dir).unwrap();
    std::fs::create_dir_all(&link_dir).unwrap();
    let real = real_dir.join("tf2tools.AppImage");
    std::fs::write(&real, b"old").unwrap();
    let link = link_dir.join("tf2tools.AppImage");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let staged = stage(&link, b"new payload").await;
    let result = apply(staged);
    assert!(
        matches!(result, Err(UpdateApplyError::Unsupported { .. })),
        "{result:?}"
    );
    assert!(
        std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(std::fs::read(&real).unwrap(), b"old");
    assert_eq!(names(&link_dir), ["tf2tools.AppImage"]);
}

#[cfg(unix)]
#[tokio::test]
async fn a_vanished_appimage_is_an_io_error() {
    let tmp = tempfile::tempdir().unwrap();
    let app = tmp.path().join("tf2tools.AppImage");
    std::fs::write(&app, b"old").unwrap();
    let staged = stage(&app, b"new payload").await;
    std::fs::remove_file(&app).unwrap();
    let result = apply(staged);
    assert!(
        matches!(result, Err(UpdateApplyError::Io { .. })),
        "{result:?}"
    );
    assert_eq!(names(tmp.path()), Vec::<String>::new());
}

/// Downloads `payload` as the 6.0.1 Windows installer, staged in `staging`.
async fn stage_installer(staging: &Path, payload: &[u8]) -> StagedUpdate {
    let feed = feed().await;
    publish_installer(&feed, "6.0.1", payload, payload).await;
    let offer = installer_offer(&feed).await;
    feed.client
        .clone()
        .with_staging_dir(staging)
        .download(&offer, &InstallTarget::WindowsInstaller, |_, _| {})
        .await
        .unwrap()
}

#[tokio::test]
async fn an_installer_that_cannot_run_is_a_spawn_error_and_is_removed() {
    // Not an executable anywhere: Windows refuses the image, unix the missing exec bit.
    let staging = tempfile::tempdir().unwrap();
    let staged = stage_installer(staging.path(), b"not an installer").await;
    let path = staged.path().to_path_buf();
    match apply(staged) {
        Err(UpdateApplyError::Spawn { path: failed, .. }) => assert_eq!(failed, path),
        other => panic!("expected Spawn, got {other:?}"),
    }
    assert!(!path.exists());
}

#[tokio::test]
async fn a_missing_installer_is_not_started() {
    let staging = tempfile::tempdir().unwrap();
    let staged = stage_installer(staging.path(), b"installer").await;
    std::fs::remove_file(staged.path()).unwrap();
    let err = apply(staged).unwrap_err();
    assert!(
        matches!(
            err,
            UpdateApplyError::Io { .. } | UpdateApplyError::Unsupported { .. }
        ),
        "{err:?}"
    );
}

/// A harmless stand-in installer that records its arguments in `<itself>.args`.
#[cfg(unix)]
const RECORDER: &[u8] = b"#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$0.args\"\n";

#[cfg(unix)]
#[tokio::test]
async fn the_installer_starts_passive_and_relaunches_the_app() {
    use std::os::unix::fs::PermissionsExt;
    let staging = tempfile::tempdir().unwrap();
    let staged = stage_installer(staging.path(), RECORDER).await;
    let path = staged.path().to_path_buf();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(apply(staged).unwrap(), Applied::InstallerStarted);
    let args = PathBuf::from(format!("{}.args", path.display()));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::fs::read_to_string(&args).map_or(true, |a| !a.ends_with('\n')) {
        assert!(
            std::time::Instant::now() < deadline,
            "the installer never ran"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(std::fs::read_to_string(&args).unwrap(), "/P\n/R\n");
    assert!(path.exists(), "a started installer is left in place");
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn an_installer_still_open_for_writing_elsewhere_starts_once_it_is_closed() {
    use std::os::unix::fs::PermissionsExt;
    // Any process that forked while the file was open for writing holds that handle until
    // it execs, and Linux refuses to run the file meanwhile. Holding the handle here for a
    // moment stands in for that.
    let staging = tempfile::tempdir().unwrap();
    let staged = stage_installer(staging.path(), RECORDER).await;
    let path = staged.path().to_path_buf();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    let writer = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    let release = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(100));
        drop(writer);
    });
    assert_eq!(apply(staged).unwrap(), Applied::InstallerStarted);
    release.join().unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn a_symlink_in_place_of_the_installer_is_unsupported() {
    let staging = tempfile::tempdir().unwrap();
    let staged = stage_installer(staging.path(), b"installer").await;
    std::fs::remove_file(staged.path()).unwrap();
    std::os::unix::fs::symlink("/bin/true", staged.path()).unwrap();
    let err = apply(staged).unwrap_err();
    assert!(
        matches!(err, UpdateApplyError::Unsupported { .. }),
        "{err:?}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn an_installer_changed_after_download_is_refused_and_not_started() {
    use std::os::unix::fs::PermissionsExt;
    // The verified download is overwritten by a runnable stand-in before apply.
    let staging = tempfile::tempdir().unwrap();
    let staged = stage_installer(staging.path(), b"the signed installer").await;
    let path = staged.path().to_path_buf();
    std::fs::write(&path, RECORDER).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    match apply(staged) {
        Err(UpdateApplyError::Changed { path: changed }) => assert_eq!(changed, path),
        other => panic!("expected Changed, got {other:?}"),
    }
    assert!(!path.exists(), "a changed installer is removed");
    // A spawned stand-in would have written its arguments by now.
    std::thread::sleep(std::time::Duration::from_millis(500));
    let args = PathBuf::from(format!("{}.args", path.display()));
    assert!(!args.exists(), "the changed installer was started");
}
