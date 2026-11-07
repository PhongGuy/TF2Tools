//! Downloading an offered AppImage beside the running one, checked by size and sha256.

use crate::common;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use common::update::{
    appimage_offer, asset_path, asset_url, feed, installer_offer, manifest_json, publish,
    publish_installer, publish_manifest, serve_asset, serve_feed, sha256_hex,
};
use tf2tools_core::{InstallTarget, StagedUpdate, UpdateDownloadError};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

fn appimage_in(dir: &Path) -> PathBuf {
    let app = dir.join("tf2tools.AppImage");
    std::fs::write(&app, b"old").unwrap();
    app
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

#[test]
fn a_staged_update_is_send_sync_debug() {
    fn assert_send_sync<T: Send + Sync + std::fmt::Debug + 'static>() {}
    assert_send_sync::<StagedUpdate>();
    assert_send_sync::<UpdateDownloadError>();
}

#[tokio::test]
async fn a_good_download_is_staged_beside_the_appimage() {
    let tmp = tempfile::tempdir().unwrap();
    let app = appimage_in(tmp.path());
    let feed = feed().await;
    let payload = vec![7u8; 300_000];
    publish(&feed, "6.0.1", &payload).await;
    let offer = appimage_offer(&feed, &app).await;
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let target = InstallTarget::AppImage(app.clone());
    let staged = feed
        .client
        .download(&offer, &target, move |done, total| {
            sink.lock().unwrap().push((done, total))
        })
        .await
        .unwrap();
    assert_eq!(staged.path(), tmp.path().join(".tf2tools-update-6.0.1"));
    assert_eq!(staged.path().parent(), app.parent());
    assert_eq!(staged.version(), "6.0.1");
    assert_eq!(staged.target(), &target);
    assert_eq!(std::fs::read(staged.path()).unwrap(), payload);
    assert_eq!(std::fs::read(&app).unwrap(), b"old");
    assert_eq!(
        names(tmp.path()),
        [".tf2tools-update-6.0.1", "tf2tools.AppImage"]
    );
    let seen = seen.lock().unwrap();
    assert_eq!(seen.last(), Some(&(300_000, 300_000)));
    assert!(seen.windows(2).all(|pair| pair[0].0 <= pair[1].0));
    assert!(seen.iter().all(|&(_, total)| total == 300_000));
}

#[tokio::test]
async fn the_download_future_is_send_with_a_non_sync_callback() {
    fn assert_send<T: Send>(_: &T) {}
    let tmp = tempfile::tempdir().unwrap();
    let app = appimage_in(tmp.path());
    let feed = feed().await;
    publish(&feed, "6.0.1", b"payload").await;
    let offer = appimage_offer(&feed, &app).await;
    let target = InstallTarget::AppImage(app);
    let last = std::cell::Cell::new(0u64);
    let download = feed
        .client
        .download(&offer, &target, move |done, _| last.set(done));
    assert_send(&download);
    download.await.unwrap();
}

#[tokio::test]
async fn a_redirected_asset_downloads() {
    let tmp = tempfile::tempdir().unwrap();
    let app = appimage_in(tmp.path());
    let feed = feed().await;
    publish_manifest(&feed, "6.0.1", b"payload").await;
    Mock::given(method("GET"))
        .and(path(asset_path("6.0.1")))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("location", format!("{}/cdn/blob", feed.server.uri())),
        )
        .mount(&feed.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/cdn/blob"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"payload".to_vec()))
        .mount(&feed.server)
        .await;
    let offer = appimage_offer(&feed, &app).await;
    let staged = feed
        .client
        .download(&offer, &InstallTarget::AppImage(app), |_, _| {})
        .await
        .unwrap();
    assert_eq!(std::fs::read(staged.path()).unwrap(), b"payload");
}

#[tokio::test]
async fn a_short_body_is_a_size_mismatch_and_leaves_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let app = appimage_in(tmp.path());
    let feed = feed().await;
    publish_manifest(&feed, "6.0.1", &[7u8; 1000]).await;
    serve_asset(&feed.server, "6.0.1", &[7u8; 999]).await;
    let offer = appimage_offer(&feed, &app).await;
    let err = feed
        .client
        .download(&offer, &InstallTarget::AppImage(app), |_, _| {})
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            UpdateDownloadError::SizeMismatch {
                expected: 1000,
                received: 999
            }
        ),
        "{err:?}"
    );
    assert_eq!(names(tmp.path()), ["tf2tools.AppImage"]);
}

#[tokio::test]
async fn a_long_body_is_a_size_mismatch_and_leaves_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let app = appimage_in(tmp.path());
    let feed = feed().await;
    publish_manifest(&feed, "6.0.1", &[7u8; 1000]).await;
    serve_asset(&feed.server, "6.0.1", &[7u8; 1001]).await;
    let offer = appimage_offer(&feed, &app).await;
    let err = feed
        .client
        .download(&offer, &InstallTarget::AppImage(app), |_, _| {})
        .await
        .unwrap_err();
    match err {
        UpdateDownloadError::SizeMismatch { expected, received } => {
            assert_eq!(expected, 1000);
            assert!(received > 1000, "{received}");
        }
        other => panic!("expected SizeMismatch, got {other:?}"),
    }
    assert_eq!(names(tmp.path()), ["tf2tools.AppImage"]);
}

#[tokio::test]
async fn a_wrong_hash_is_a_hash_mismatch_and_leaves_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let app = appimage_in(tmp.path());
    let feed = feed().await;
    publish_manifest(&feed, "6.0.1", &[7u8; 1000]).await;
    serve_asset(&feed.server, "6.0.1", &[8u8; 1000]).await;
    let offer = appimage_offer(&feed, &app).await;
    let err = feed
        .client
        .download(&offer, &InstallTarget::AppImage(app), |_, _| {})
        .await
        .unwrap_err();
    assert!(
        matches!(err, UpdateDownloadError::HashMismatch { .. }),
        "{err:?}"
    );
    assert_eq!(names(tmp.path()), ["tf2tools.AppImage"]);
}

#[tokio::test]
async fn a_missing_asset_is_status_404() {
    let tmp = tempfile::tempdir().unwrap();
    let app = appimage_in(tmp.path());
    let feed = feed().await;
    publish_manifest(&feed, "6.0.1", b"payload").await;
    let offer = appimage_offer(&feed, &app).await;
    let err = feed
        .client
        .download(&offer, &InstallTarget::AppImage(app), |_, _| {})
        .await
        .unwrap_err();
    assert!(
        matches!(err, UpdateDownloadError::Status { status: 404, .. }),
        "{err:?}"
    );
    assert_eq!(names(tmp.path()), ["tf2tools.AppImage"]);
}

#[tokio::test]
async fn a_part_file_left_by_a_crash_is_overwritten() {
    let tmp = tempfile::tempdir().unwrap();
    let app = appimage_in(tmp.path());
    std::fs::write(
        tmp.path().join(".tf2tools-update-6.0.1.part"),
        vec![1u8; 5000],
    )
    .unwrap();
    let feed = feed().await;
    publish(&feed, "6.0.1", b"payload").await;
    let offer = appimage_offer(&feed, &app).await;
    let staged = feed
        .client
        .download(&offer, &InstallTarget::AppImage(app), |_, _| {})
        .await
        .unwrap();
    assert_eq!(std::fs::read(staged.path()).unwrap(), b"payload");
    assert_eq!(
        names(tmp.path()),
        [".tf2tools-update-6.0.1", "tf2tools.AppImage"]
    );
}

#[tokio::test]
async fn an_offer_without_an_asset_is_no_asset() {
    let tmp = tempfile::tempdir().unwrap();
    let app = appimage_in(tmp.path());
    let feed = feed().await;
    publish(&feed, "6.0.1", b"payload").await;
    let offer = feed
        .client
        .check("6.0.0", &InstallTarget::Unmanaged)
        .await
        .unwrap()
        .unwrap();
    let err = feed
        .client
        .download(&offer, &InstallTarget::AppImage(app), |_, _| {})
        .await
        .unwrap_err();
    assert!(matches!(err, UpdateDownloadError::NoAsset), "{err:?}");
}

#[tokio::test]
async fn an_unmanaged_target_is_unsupported_and_fetches_nothing() {
    let feed = feed().await;
    publish(&feed, "6.0.1", b"payload").await;
    let tmp = tempfile::tempdir().unwrap();
    let offer = appimage_offer(&feed, &appimage_in(tmp.path())).await;
    let err = feed
        .client
        .download(&offer, &InstallTarget::Unmanaged, |_, _| {})
        .await
        .unwrap_err();
    assert!(matches!(err, UpdateDownloadError::Unsupported), "{err:?}");
    assert_eq!(feed.server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn an_installer_is_staged_as_an_exe_in_the_staging_dir() {
    let feed = feed().await;
    publish_installer(&feed, "6.0.1", b"installer", b"installer").await;
    let offer = installer_offer(&feed).await;
    let staging = tempfile::tempdir().unwrap();
    let client = feed.client.clone().with_staging_dir(staging.path());
    let staged = client
        .download(&offer, &InstallTarget::WindowsInstaller, |_, _| {})
        .await
        .unwrap();
    let expected = staging.path().join("tf2tools-update-6.0.1-setup.exe");
    assert_eq!(staged.path(), expected);
    assert_eq!(staged.version(), "6.0.1");
    assert_eq!(staged.target(), &InstallTarget::WindowsInstaller);
    assert_eq!(std::fs::read(&expected).unwrap(), b"installer");
    assert_eq!(names(staging.path()), ["tf2tools-update-6.0.1-setup.exe"]);
}

#[tokio::test]
async fn a_damaged_installer_leaves_nothing_in_the_staging_dir() {
    let feed = feed().await;
    publish_installer(&feed, "6.0.1", b"installer", b"tampered").await;
    let offer = installer_offer(&feed).await;
    let staging = tempfile::tempdir().unwrap();
    let client = feed.client.clone().with_staging_dir(staging.path());
    let err = client
        .download(&offer, &InstallTarget::WindowsInstaller, |_, _| {})
        .await
        .unwrap_err();
    assert!(
        matches!(err, UpdateDownloadError::SizeMismatch { .. }),
        "{err:?}"
    );
    assert!(names(staging.path()).is_empty());
}

#[tokio::test]
async fn an_appimage_offer_is_unsupported_for_the_installer_and_fetches_nothing() {
    let feed = feed().await;
    publish(&feed, "6.0.1", b"payload").await;
    let tmp = tempfile::tempdir().unwrap();
    let offer = appimage_offer(&feed, &appimage_in(tmp.path())).await;
    let staging = tempfile::tempdir().unwrap();
    let client = feed.client.clone().with_staging_dir(staging.path());
    let err = client
        .download(&offer, &InstallTarget::WindowsInstaller, |_, _| {})
        .await
        .unwrap_err();
    assert!(matches!(err, UpdateDownloadError::Unsupported), "{err:?}");
    assert_eq!(feed.server.received_requests().await.unwrap().len(), 2);
    assert!(names(staging.path()).is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn a_read_only_directory_is_not_writable() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let app = appimage_in(tmp.path());
    let feed = feed().await;
    publish(&feed, "6.0.1", b"payload").await;
    let offer = appimage_offer(&feed, &app).await;
    std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
    let result = feed
        .client
        .download(&offer, &InstallTarget::AppImage(app), |_, _| {})
        .await;
    std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    match result {
        Err(UpdateDownloadError::NotWritable { dir }) => assert_eq!(dir, tmp.path()),
        other => panic!("expected NotWritable, got {other:?}"),
    }
    assert_eq!(names(tmp.path()), ["tf2tools.AppImage"]);
}

#[tokio::test]
async fn an_installer_offer_is_unsupported_for_an_appimage_and_fetches_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let app = appimage_in(tmp.path());
    let feed = feed().await;
    let key = format!("{}-{}-nsis", std::env::consts::OS, std::env::consts::ARCH);
    let url = asset_url(&feed.server.uri(), "6.0.1");
    let sha = sha256_hex(b"payload");
    let manifest = manifest_json("6.0.1", &[(key.as_str(), url.as_str(), sha.as_str(), 7)]);
    serve_feed(&feed.server, &manifest, &feed.key.sign(manifest.as_bytes())).await;
    serve_asset(&feed.server, "6.0.1", b"payload").await;
    let offer = feed
        .client
        .check("6.0.0", &InstallTarget::WindowsInstaller)
        .await
        .unwrap()
        .unwrap();
    let err = feed
        .client
        .download(&offer, &InstallTarget::AppImage(app), |_, _| {})
        .await
        .unwrap_err();
    assert!(matches!(err, UpdateDownloadError::Unsupported), "{err:?}");
    assert_eq!(feed.server.received_requests().await.unwrap().len(), 2);
    assert_eq!(names(tmp.path()), ["tf2tools.AppImage"]);
}

#[tokio::test]
async fn a_dropped_download_leaves_no_part_file() {
    let tmp = tempfile::tempdir().unwrap();
    let app = appimage_in(tmp.path());
    let feed = feed().await;
    publish_manifest(&feed, "6.0.1", b"payload").await;
    Mock::given(method("GET"))
        .and(path(asset_path("6.0.1")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(b"payload".to_vec())
                .set_delay(std::time::Duration::from_secs(30)),
        )
        .mount(&feed.server)
        .await;
    let offer = appimage_offer(&feed, &app).await;
    let target = InstallTarget::AppImage(app);
    let part = tmp.path().join(".tf2tools-update-6.0.1.part");
    let mut download = Box::pin(feed.client.download(&offer, &target, |_, _| {}));
    // Drive the download until the part file exists, then drop it mid-request.
    let started = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            tokio::select! {
                result = &mut download => panic!("finished early: {result:?}"),
                () = tokio::time::sleep(std::time::Duration::from_millis(20)) => {
                    if part.exists() {
                        break;
                    }
                }
            }
        }
    })
    .await;
    assert!(started.is_ok(), "the part file never appeared");
    drop(download);
    assert_eq!(names(tmp.path()), ["tf2tools.AppImage"]);
}

#[cfg(unix)]
#[tokio::test]
async fn a_symlink_at_the_part_path_is_not_written_through() {
    let tmp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let victim = outside.path().join("victim");
    std::fs::write(&victim, b"untouched").unwrap();
    let app = appimage_in(tmp.path());
    std::os::unix::fs::symlink(&victim, tmp.path().join(".tf2tools-update-6.0.1.part")).unwrap();
    let feed = feed().await;
    publish(&feed, "6.0.1", b"payload").await;
    let offer = appimage_offer(&feed, &app).await;
    let staged = feed
        .client
        .download(&offer, &InstallTarget::AppImage(app), |_, _| {})
        .await
        .unwrap();
    assert_eq!(std::fs::read(&victim).unwrap(), b"untouched");
    assert_eq!(std::fs::read(staged.path()).unwrap(), b"payload");
    assert!(!staged.path().is_symlink());
    assert_eq!(
        names(tmp.path()),
        [".tf2tools-update-6.0.1", "tf2tools.AppImage"]
    );
}

#[tokio::test]
async fn downloading_the_same_version_twice_restages_it() {
    let tmp = tempfile::tempdir().unwrap();
    let app = appimage_in(tmp.path());
    let feed = feed().await;
    publish(&feed, "6.0.1", b"payload").await;
    let offer = appimage_offer(&feed, &app).await;
    let target = InstallTarget::AppImage(app);
    for _ in 0..2 {
        let staged = feed
            .client
            .download(&offer, &target, |_, _| {})
            .await
            .unwrap();
        assert_eq!(std::fs::read(staged.path()).unwrap(), b"payload");
    }
    assert_eq!(
        names(tmp.path()),
        [".tf2tools-update-6.0.1", "tf2tools.AppImage"]
    );
}
