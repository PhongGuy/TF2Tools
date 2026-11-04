use std::time::Duration;

use tf2tools_app::updater::backend::{
    Deadlines, describe_apply, describe_check, describe_download,
};
use tf2tools_app::updater::{DownloadSink, Failure};
use tf2tools_core::{UpdateApplyError, UpdateCheckError, UpdateDownloadError};

const OFFLINE: &str =
    "Couldn't reach the update server. Check your internet connection and try again.";
const UNREADABLE: &str = "The update information couldn't be read. Try again later.";
const UNVERIFIED: &str = "Update check failed: the update could not be verified. Try again later.";
const UNAVAILABLE: &str =
    "Automatic updates aren't available in this version. Get new versions from the release page.";
const STOPPED: &str = "The download stopped. Check your internet connection and try again.";
const UNSERVED: &str = "The update couldn't be downloaded right now. Try again later, or get it from the release page.";
const BUSY: &str = "The update server is busy right now. Try again in a few minutes.";
const DAMAGED: &str = "The download was damaged. Try again.";
const DISK: &str = "Couldn't save the update. Make sure your disk has free space, then try again.";
const USE_PAGE: &str = "TF2Tools can't update itself where it's installed. Download the new version from the release page instead.";
const INSTALL_IO: &str =
    "Couldn't install the update. Try again, or download the new version from the release page.";
const INSTALLER: &str =
    "Couldn't start the installer. Try again, or download the new version from the release page.";
const CHANGED: &str = "The update changed before it could be installed. Try again.";

fn says(message: &str, use_release_page: bool) -> Failure {
    Failure {
        message: message.to_owned(),
        use_release_page,
    }
}

#[test]
fn default_deadlines_are_thirty_seconds_and_twenty_minutes() {
    assert_eq!(
        Deadlines::default(),
        Deadlines {
            check: Duration::from_secs(30),
            download: Duration::from_secs(20 * 60)
        }
    );
}

#[test]
fn download_sink_reports_whole_percents() {
    let (sink, percent) = DownloadSink::new();
    sink.report(1, 3);
    assert_eq!(*percent.borrow(), 33);
    sink.report(10, 5);
    assert_eq!(*percent.borrow(), 100);
    sink.report(5, 0);
    assert_eq!(*percent.borrow(), 0);
}

#[test]
fn check_errors_read_as_player_advice() {
    let url =
        "http://127.0.0.1:9/gh/PhongGuy/TF2Tools/releases/latest/download/tf2tools-update.json"
            .to_owned();
    let cases = [
        (
            UpdateCheckError::Network {
                url: url.clone(),
                message: "connection refused".into(),
            },
            says(OFFLINE, false),
        ),
        (
            UpdateCheckError::Status {
                url: url.clone(),
                status: 404,
            },
            says(UNREADABLE, false),
        ),
        (
            UpdateCheckError::Status {
                url: url.clone(),
                status: 502,
            },
            says(UNREADABLE, false),
        ),
        (
            UpdateCheckError::TooLarge {
                url: url.clone(),
                limit: 65536,
            },
            says(UNREADABLE, false),
        ),
        (
            UpdateCheckError::Malformed {
                message: "version".into(),
            },
            says(UNREADABLE, false),
        ),
        (UpdateCheckError::BadSignature, says(UNVERIFIED, false)),
        (
            UpdateCheckError::BadKey {
                message: "no public key is configured".into(),
            },
            says(UNAVAILABLE, true),
        ),
    ];
    for (err, expected) in cases {
        let got = describe_check(&err);
        assert_eq!(got, expected, "{err}");
        assert!(
            !got.message.contains("127.0.0.1") && !got.message.contains("http"),
            "{got:?}"
        );
    }
}

#[test]
fn download_errors_read_as_player_advice() {
    let io = || std::io::Error::other("disk full");
    let cases = [
        (
            UpdateDownloadError::Network {
                url: "u".into(),
                message: "reset".into(),
            },
            says(STOPPED, false),
        ),
        (
            UpdateDownloadError::Status {
                url: "u".into(),
                status: 500,
            },
            says(BUSY, false),
        ),
        (
            UpdateDownloadError::Status {
                url: "u".into(),
                status: 503,
            },
            says(BUSY, false),
        ),
        (
            UpdateDownloadError::Status {
                url: "u".into(),
                status: 404,
            },
            says(UNSERVED, true),
        ),
        (
            UpdateDownloadError::SizeMismatch {
                expected: 10,
                received: 9,
            },
            says(DAMAGED, false),
        ),
        (
            UpdateDownloadError::HashMismatch {
                expected: "a".into(),
                actual: "b".into(),
            },
            says(DAMAGED, false),
        ),
        (
            UpdateDownloadError::Io {
                path: "/x".into(),
                source: io(),
            },
            says(DISK, false),
        ),
        (
            UpdateDownloadError::NotWritable { dir: "/opt".into() },
            says(USE_PAGE, true),
        ),
        (UpdateDownloadError::NoAsset, says(USE_PAGE, true)),
        (UpdateDownloadError::Unsupported, says(USE_PAGE, true)),
    ];
    for (err, expected) in cases {
        assert_eq!(describe_download(&err), expected, "{err}");
    }
}

#[test]
fn apply_errors_read_as_player_advice() {
    let cases = [
        (
            UpdateApplyError::NotWritable {
                path: "/opt/x".into(),
            },
            says(USE_PAGE, true),
        ),
        (
            UpdateApplyError::Unsupported { path: None },
            says(USE_PAGE, true),
        ),
        (
            UpdateApplyError::Io {
                path: "/x".into(),
                source: std::io::Error::other("busy"),
            },
            says(INSTALL_IO, false),
        ),
        (
            UpdateApplyError::Spawn {
                path: "/tmp/setup.exe".into(),
                source: std::io::Error::other("refused"),
            },
            says(INSTALLER, false),
        ),
        (
            UpdateApplyError::Changed {
                path: "/tmp/setup.exe".into(),
            },
            says(CHANGED, false),
        ),
    ];
    for (err, expected) in cases {
        assert_eq!(describe_apply(&err), expected, "{err}");
    }
}
