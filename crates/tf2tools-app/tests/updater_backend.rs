use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[cfg(unix)]
use tf2tools_app::updater::AfterInstall;
use tf2tools_app::updater::backend::{Deadlines, core_backend};
use tf2tools_app::updater::{DownloadSink, Failure, Offer, UpdateBackend};
use tf2tools_core::InstallTarget;

mod support;
use support::update_feed::{SignedFeed, fresh_key};

const AGES: Duration = Duration::from_secs(600);
const UNVERIFIED: &str = "Update check failed: the update could not be verified. Try again later.";
const DAMAGED: &str = "The download was damaged. Try again.";
const USE_PAGE: &str = "TF2Tools can't update itself where it's installed. Download the new version from the release page instead.";

fn says(message: &str, use_release_page: bool) -> Failure {
    Failure {
        message: message.to_owned(),
        use_release_page,
    }
}

fn appimage(dir: &Path) -> PathBuf {
    let path = dir.join("tf2tools_6.0.0_x86_64.AppImage");
    std::fs::write(&path, b"old AppImage").unwrap();
    path
}

fn backend(feed: &SignedFeed, target: InstallTarget, deadlines: Deadlines) -> UpdateBackend {
    core_backend(feed.client(), target, "6.0.0", deadlines)
}

fn relaxed() -> Deadlines {
    Deadlines {
        check: AGES,
        download: AGES,
    }
}

fn check(feed: &SignedFeed, backend: &UpdateBackend) -> Result<Option<Offer>, Failure> {
    feed.rt.block_on((backend.check)())
}

fn download(feed: &SignedFeed, backend: &UpdateBackend) -> (Result<(), Failure>, u8) {
    let (sink, percent) = DownloadSink::new();
    let result = feed.rt.block_on((backend.download)(sink));
    (result, *percent.borrow())
}

fn leftovers(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".tf2tools-update"))
        .collect()
}

#[test]
fn a_newer_release_is_offered_to_an_appimage_as_installable() {
    let feed = SignedFeed::start();
    feed.publish("6.0.1", b"new AppImage");
    let dir = tempfile::tempdir().unwrap();
    let backend = backend(
        &feed,
        InstallTarget::AppImage(appimage(dir.path())),
        relaxed(),
    );
    assert_eq!(
        check(&feed, &backend),
        Ok(Some(Offer {
            version: "6.0.1".into(),
            release_page: feed.release_page("6.0.1"),
            installable: true,
        }))
    );
}

#[test]
fn an_unmanaged_install_is_offered_the_release_page_only() {
    let feed = SignedFeed::start();
    feed.publish("6.0.1", b"new AppImage");
    let backend = backend(&feed, InstallTarget::Unmanaged, relaxed());
    let offer = check(&feed, &backend).unwrap().expect("an offer");
    assert!(!offer.installable);
    assert_eq!(offer.release_page, feed.release_page("6.0.1"));
    let (result, _) = download(&feed, &backend);
    assert_eq!(result, Err(says(USE_PAGE, true)));
}

#[test]
fn the_same_version_is_up_to_date() {
    let feed = SignedFeed::start();
    feed.publish("6.0.0", b"same");
    let backend = backend(&feed, InstallTarget::Unmanaged, relaxed());
    assert_eq!(check(&feed, &backend), Ok(None));
}

#[test]
fn a_manifest_signed_by_another_key_is_not_trusted() {
    let feed = SignedFeed::start();
    feed.publish_signed_by("6.0.1", b"evil", &fresh_key());
    let backend = backend(&feed, InstallTarget::Unmanaged, relaxed());
    assert_eq!(check(&feed, &backend), Err(says(UNVERIFIED, false)));
}

#[test]
fn a_slow_server_hits_the_check_deadline() {
    let feed = SignedFeed::start();
    feed.publish_slow("6.0.1", b"new", Duration::from_secs(5), Duration::ZERO);
    let deadlines = Deadlines {
        check: Duration::from_millis(300),
        download: AGES,
    };
    let backend = backend(&feed, InstallTarget::Unmanaged, deadlines);
    let started = Instant::now();
    assert_eq!(
        check(&feed, &backend),
        Err(says(
            "Checking for updates took too long. Check your internet connection and try again.",
            false
        ))
    );
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
}

// The AppImage swap is unix only.
#[cfg(unix)]
#[test]
fn download_then_apply_replaces_the_appimage() {
    let feed = SignedFeed::start();
    feed.publish("6.0.1", b"new AppImage");
    let dir = tempfile::tempdir().unwrap();
    let path = appimage(dir.path());
    let backend = backend(&feed, InstallTarget::AppImage(path.clone()), relaxed());
    check(&feed, &backend).unwrap().expect("an offer");
    let (result, percent) = download(&feed, &backend);
    assert_eq!(result, Ok(()));
    assert_eq!(percent, 100);
    assert_eq!(
        std::fs::read(&path).unwrap(),
        b"old AppImage",
        "nothing swaps before apply"
    );
    assert_eq!((backend.apply)(), Ok(AfterInstall::Relaunch(path.clone())));
    assert_eq!(std::fs::read(&path).unwrap(), b"new AppImage");
    let backup = dir.path().join("tf2tools_6.0.0_x86_64.AppImage.bak");
    assert_eq!(std::fs::read(backup).unwrap(), b"old AppImage");
    assert_eq!(
        (backend.apply)(),
        Err(says("Download the update first.", false)),
        "a staged update applies once"
    );
}

// A shell script stands in for the installer, since only its start is checked here.
#[cfg(unix)]
#[test]
fn download_then_apply_starts_the_installer_and_quits() {
    use std::os::unix::fs::PermissionsExt;
    let feed = SignedFeed::start();
    feed.publish_installer("6.0.1", b"#!/bin/sh\nexit 0\n");
    let staging = tempfile::tempdir().unwrap();
    let client = feed.client().with_staging_dir(staging.path());
    let backend = core_backend(client, InstallTarget::WindowsInstaller, "6.0.0", relaxed());
    check(&feed, &backend).unwrap().expect("an offer");
    assert_eq!(download(&feed, &backend).0, Ok(()));
    let staged = staging.path().join("tf2tools-update-6.0.1-setup.exe");
    std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!((backend.apply)(), Ok(AfterInstall::Quit));
}

#[test]
fn a_damaged_download_leaves_the_appimage_alone() {
    let feed = SignedFeed::start();
    feed.publish_with_hash("6.0.1", b"new AppImage", &"0".repeat(64));
    let dir = tempfile::tempdir().unwrap();
    let path = appimage(dir.path());
    let backend = backend(&feed, InstallTarget::AppImage(path.clone()), relaxed());
    check(&feed, &backend).unwrap().expect("an offer");
    assert_eq!(download(&feed, &backend).0, Err(says(DAMAGED, false)));
    assert_eq!(std::fs::read(&path).unwrap(), b"old AppImage");
    assert!(
        leftovers(dir.path()).is_empty(),
        "{:?}",
        leftovers(dir.path())
    );
}

#[test]
fn a_stalled_download_hits_its_deadline_and_leaves_no_part_file() {
    let feed = SignedFeed::start();
    feed.publish_slow(
        "6.0.1",
        b"new AppImage",
        Duration::ZERO,
        Duration::from_secs(5),
    );
    let dir = tempfile::tempdir().unwrap();
    let path = appimage(dir.path());
    let deadlines = Deadlines {
        check: AGES,
        download: Duration::from_millis(300),
    };
    let backend = backend(&feed, InstallTarget::AppImage(path.clone()), deadlines);
    check(&feed, &backend).unwrap().expect("an offer");
    let started = Instant::now();
    assert_eq!(
        download(&feed, &backend).0,
        Err(says(
            "The download took too long. Check your internet connection and try again.",
            false
        ))
    );
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    assert!(
        leftovers(dir.path()).is_empty(),
        "{:?}",
        leftovers(dir.path())
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"old AppImage");
}

#[test]
fn download_and_apply_out_of_order_fail_in_plain_words() {
    let feed = SignedFeed::start();
    let dir = tempfile::tempdir().unwrap();
    let backend = backend(
        &feed,
        InstallTarget::AppImage(appimage(dir.path())),
        relaxed(),
    );
    assert_eq!(
        download(&feed, &backend).0,
        Err(says("Check for updates first.", false))
    );
    assert_eq!(
        (backend.apply)(),
        Err(says("Download the update first.", false))
    );
}
