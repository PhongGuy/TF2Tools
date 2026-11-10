use crate::support;

use std::path::PathBuf;

use tf2tools_app::services::OpError;
use tf2tools_app::store_messages::{
    StoreFailure, addon_stopped, addons_stopped, describe_browse, describe_download,
    describe_files, describe_install, describe_preview, describe_store_read, download_stopped,
    listing_stopped, no_tf2, preview_gone, preview_unavailable, release_has_no_file,
};
use tf2tools_core::{
    ArchiveError, BrowseError, DownloadError, FilesError, GbId, ImportError, InstallError,
    LibraryError, PreviewError, StoreKind,
};

const GB: StoreKind = StoreKind::GameBananaHuds;
const GB_URL: &str = "https://files.gamebanana.com/mods/flawhud.zip";
const GH_URL: &str = "https://github.com/mastercomfig/mastercomfig/releases/download/9.9.9/x.vpk";

const DL_DISK: &str = "Couldn't save the download. Make sure your disk has free space and TF2Tools can write to its folder, then try again.";
const DL_LIBRARY: &str = "TF2Tools couldn't update your library. Restart TF2Tools and try again.";
const DL_STOPPED: &str = "The download stopped before it finished. Try again.";
const DL_UNUSABLE: &str = "This download has nothing TF2Tools can use. Pick another file.";
const DL_LOCKED: &str = "This download is locked with a password, so TF2Tools can't open it.";
const DL_UNPACK: &str = "The download couldn't be unpacked. Try again, or pick another file.";

fn io(text: &str) -> std::io::Error {
    std::io::Error::other(text.to_owned())
}

// Every DownloadError variant once, with the store it is reported for and the words the player reads.
fn download_cases() -> Vec<(StoreKind, DownloadError, &'static str)> {
    vec![
        (
            GB,
            DownloadError::AlreadyInLibrary {
                what: "gamebanana 7".into(),
            },
            "This item is already in your library.",
        ),
        (
            GB,
            DownloadError::WrongKind {
                file_name: "hitsound.wav".into(),
            },
            "This file doesn't belong in this store. Pick another file.",
        ),
        (
            GB,
            DownloadError::RateLimited { url: GB_URL.into() },
            "GameBanana is busy right now. Wait a few minutes, then try again.",
        ),
        (
            GB,
            DownloadError::Http {
                url: GB_URL.into(),
                status: 502,
            },
            "GameBanana is having problems right now. Try again later.",
        ),
        (
            GB,
            DownloadError::Http {
                url: GB_URL.into(),
                status: 404,
            },
            "Couldn't get this file from GameBanana. Try again later.",
        ),
        (
            StoreKind::Addons,
            DownloadError::Http {
                url: GH_URL.into(),
                status: 404,
            },
            "Couldn't get this file from GitHub. Try again later.",
        ),
        (
            StoreKind::CuratedHuds,
            DownloadError::Network {
                url: GH_URL.into(),
                message: "error sending request: connection refused".into(),
            },
            "Couldn't reach GitHub. Check your internet connection and try again.",
        ),
        (
            StoreKind::Sounds,
            DownloadError::Network {
                url: GB_URL.into(),
                message: "dns error".into(),
            },
            "Couldn't reach GameBanana. Check your internet connection and try again.",
        ),
        (
            GB,
            DownloadError::TooLarge {
                url: GB_URL.into(),
                limit: 536_870_912,
            },
            "This file is too big for TF2Tools. Pick another file.",
        ),
        (
            GB,
            DownloadError::ChecksumMismatch {
                file_name: "flawhud.zip".into(),
                expected: "abc123".into(),
                found: "def456".into(),
            },
            "The download was damaged. Try again.",
        ),
        (
            GB,
            DownloadError::NoUsableFiles {
                reason: "no info.vdf in /tmp/x".into(),
            },
            DL_UNUSABLE,
        ),
        (
            GB,
            DownloadError::Interrupted {
                message: "task 4 cancelled".into(),
            },
            DL_STOPPED,
        ),
        (
            GB,
            DownloadError::Io {
                path: PathBuf::from("/home/p/.local/share/x/huds"),
                source: io("No space left on device (os error 28)"),
            },
            DL_DISK,
        ),
        (
            GB,
            DownloadError::Archive(ArchiveError::Encrypted {
                path: PathBuf::from("/tmp/hud.rar"),
            }),
            DL_LOCKED,
        ),
        (
            GB,
            DownloadError::Archive(ArchiveError::Malformed {
                path: PathBuf::from("/tmp/hud.zip"),
                message: "invalid Zip archive: bad magic".into(),
            }),
            DL_UNPACK,
        ),
        (
            GB,
            DownloadError::Import(ImportError::Archive(ArchiveError::Encrypted {
                path: PathBuf::from("/tmp/hud.rar"),
            })),
            DL_LOCKED,
        ),
        (
            GB,
            DownloadError::Import(ImportError::NotAHud {
                path: PathBuf::from("/tmp/x"),
            }),
            DL_UNUSABLE,
        ),
        (
            StoreKind::Sounds,
            DownloadError::Import(ImportError::NotAVpk {
                path: PathBuf::from("/tmp/x.vpk"),
            }),
            DL_UNUSABLE,
        ),
        (
            GB,
            DownloadError::Import(ImportError::Io {
                path: PathBuf::from("/tmp/x"),
                source: io("Permission denied (os error 13)"),
            }),
            DL_DISK,
        ),
        (
            GB,
            DownloadError::Import(ImportError::Database {
                message: "database is locked".into(),
            }),
            DL_LIBRARY,
        ),
        (
            GB,
            DownloadError::Database {
                message: "no such table: store_huds".into(),
            },
            DL_LIBRARY,
        ),
        (
            GB,
            DownloadError::RollbackFailed {
                imported: Vec::new(),
                cause: Box::new(DownloadError::Network {
                    url: GB_URL.into(),
                    message: "connection reset".into(),
                }),
                source: LibraryError::Database {
                    message: "disk I/O error".into(),
                },
            },
            "Couldn't reach GameBanana. Check your internet connection and try again.",
        ),
    ]
}

#[test]
fn download_errors_read_as_player_advice() {
    for (kind, err, want) in download_cases() {
        let raw = err.to_string();
        assert_eq!(
            describe_download(kind, &err),
            StoreFailure::new(want, raw),
            "{kind:?} {err:?}"
        );
    }
}

#[test]
fn no_download_message_carries_a_url_status_or_error_text() {
    for (kind, err, _) in download_cases() {
        support::assert_plain(&describe_download(kind, &err).message);
    }
    support::assert_plain(&download_stopped("task 3 panicked at src/x.rs:9").message);
}

#[test]
fn a_stopped_download_keeps_its_detail() {
    assert_eq!(
        download_stopped("task 3 panicked at src/x.rs:9"),
        StoreFailure::new(DL_STOPPED, "task 3 panicked at src/x.rs:9")
    );
}

fn files_cases() -> Vec<(StoreKind, FilesError, &'static str)> {
    vec![
        (
            GB,
            FilesError::NotInStore {
                what: "gamebanana 9".into(),
            },
            "This item is no longer in the store.",
        ),
        (
            GB,
            FilesError::RateLimited { url: GB_URL.into() },
            "GameBanana is busy right now. Wait a few minutes, then try again.",
        ),
        (
            GB,
            FilesError::Http {
                url: GB_URL.into(),
                status: 503,
            },
            "GameBanana is having problems right now. Try again later.",
        ),
        (
            GB,
            FilesError::Http {
                url: GB_URL.into(),
                status: 404,
            },
            "GameBanana sent something TF2Tools couldn't read. Try again later.",
        ),
        (
            StoreKind::Addons,
            FilesError::Network {
                url: GH_URL.into(),
                message: "timed out".into(),
            },
            "Couldn't reach GitHub. Check your internet connection and try again.",
        ),
        (
            GB,
            FilesError::TooLarge {
                url: GB_URL.into(),
                limit: 1_048_576,
            },
            "GameBanana sent something TF2Tools couldn't read. Try again later.",
        ),
        (
            StoreKind::Sounds,
            FilesError::Malformed {
                url: GB_URL.into(),
                message: "missing field `_aFiles`".into(),
            },
            "GameBanana sent something TF2Tools couldn't read. Try again later.",
        ),
        (
            GB,
            FilesError::Database {
                message: "database is locked".into(),
            },
            "TF2Tools couldn't read the store. Restart TF2Tools and try again.",
        ),
    ]
}

#[test]
fn listing_errors_read_as_player_advice() {
    for (kind, err, want) in files_cases() {
        let raw = err.to_string();
        assert_eq!(
            describe_files(kind, &err),
            StoreFailure::new(want, raw),
            "{kind:?} {err:?}"
        );
    }
    assert_eq!(
        listing_stopped("join error".to_owned()),
        StoreFailure::new(
            "Loading the files stopped before it finished. Try again.",
            "join error"
        )
    );
}

#[test]
fn no_listing_message_carries_a_url_status_or_error_text() {
    for (kind, err, _) in files_cases() {
        support::assert_plain(&describe_files(kind, &err).message);
    }
    support::assert_plain(&listing_stopped("join error".to_owned()).message);
}

const PV_URL: &str = "https://sounds.gamebanana.com/preview/7.mp3";

fn preview_cases() -> Vec<(PreviewError, &'static str)> {
    vec![
        (PreviewError::NoPreview, "This sound has no preview."),
        (
            PreviewError::UntrustedUrl {
                url: "https://evil.example/4.mp3".into(),
            },
            "This preview isn't hosted on GameBanana, so TF2Tools won't play it.",
        ),
        (
            PreviewError::RateLimited { url: PV_URL.into() },
            "GameBanana is busy right now. Wait a few minutes, then try again.",
        ),
        (
            PreviewError::Http {
                url: PV_URL.into(),
                status: 500,
            },
            "GameBanana is having problems right now. Try again later.",
        ),
        (
            PreviewError::Http {
                url: PV_URL.into(),
                status: 404,
            },
            "Couldn't get this preview from GameBanana. Try again later.",
        ),
        (
            PreviewError::Network {
                url: PV_URL.into(),
                message: "connection refused".into(),
            },
            "Couldn't reach GameBanana. Check your internet connection and try again.",
        ),
        (
            PreviewError::TooLarge {
                url: PV_URL.into(),
                limit: 8_388_608,
            },
            "Couldn't get this preview from GameBanana. Try again later.",
        ),
    ]
}

#[test]
fn preview_errors_read_as_player_advice() {
    for (err, want) in preview_cases() {
        let raw = err.to_string();
        assert_eq!(
            describe_preview(&err),
            StoreFailure::new(want, raw),
            "{err:?}"
        );
    }
    assert_eq!(
        preview_gone(GbId::new(99)),
        StoreFailure::new(
            "This sound is no longer in the store.",
            "sound 99 is no longer in the store"
        )
    );
    assert_eq!(
        preview_unavailable("store database error: locked".to_owned()),
        StoreFailure::new(
            "Couldn't play this preview. Try again later.",
            "store database error: locked"
        )
    );
}

#[test]
fn no_preview_message_carries_a_url_status_or_error_text() {
    for (err, _) in preview_cases() {
        support::assert_plain(&describe_preview(&err).message);
    }
    support::assert_plain(&preview_gone(GbId::new(99)).message);
    support::assert_plain(&preview_unavailable("x".to_owned()).message);
}

const READ: &str = "TF2Tools couldn't read the store. Restart TF2Tools and try again.";

#[test]
fn a_failed_store_read_reads_as_player_advice() {
    for err in [
        OpError::core(
            "load the store",
            "store database error: no such table: store_crosshairs",
        ),
        OpError::core(
            "Failed to load the store",
            "library database error: disk I/O error",
        ),
        OpError::Internal("index out of bounds at src/store_grid/mod.rs:3".into()),
    ] {
        let failure = describe_store_read(&err);
        assert_eq!(failure, StoreFailure::new(READ, err.to_string()));
        support::assert_plain(&failure.message);
    }
}

fn install_cases() -> Vec<(InstallError, &'static str)> {
    vec![
        (
            InstallError::Io {
                path: PathBuf::from(
                    "/home/p/.steam/steam/steamapps/common/Team Fortress 2/tf/custom/x.vpk",
                ),
                source: io("Permission denied (os error 13)"),
            },
            "Couldn't copy the addon into TF2. Make sure TF2Tools can write to your TF2 folder, then try again.",
        ),
        (
            InstallError::NotInLibrary {
                what: "addon x.vpk".into(),
            },
            "This addon is no longer in your library. Download it again.",
        ),
        (
            InstallError::Database {
                message: "database is locked".into(),
            },
            "TF2Tools couldn't update your library. Restart TF2Tools and try again.",
        ),
        (
            InstallError::GameMissing {
                path: PathBuf::from("/home/p/Games/Team Fortress 2/tf"),
            },
            "TF2 wasn't found where it used to be. Check the TF2 folder in Settings.",
        ),
    ]
}

#[test]
fn addon_update_errors_read_as_player_advice() {
    for (err, want) in install_cases() {
        let raw = err.to_string();
        assert_eq!(
            describe_install(&err),
            StoreFailure::new(want, raw),
            "{err:?}"
        );
    }
    assert_eq!(
        no_tf2(),
        StoreFailure::new(
            "Set your TF2 folder in Settings, then try again.",
            "TF2 not found: Set the TF2 folder in Settings, then try again."
        )
    );
    assert_eq!(
        release_has_no_file(),
        StoreFailure::new(
            "This mastercomfig release has nothing to download yet. Try again later.",
            "the release has no file"
        )
    );
}

#[test]
fn no_addon_update_message_carries_a_path_or_error_text() {
    for (err, _) in install_cases() {
        support::assert_plain(&describe_install(&err).message);
    }
    support::assert_plain(&no_tf2().message);
    support::assert_plain(&release_has_no_file().message);
}

#[test]
fn a_throttled_listing_says_gamebanana_is_busy() {
    let err = FilesError::Throttled { url: GB_URL.into() };
    let failure = describe_files(StoreKind::GameBananaHuds, &err);
    assert_eq!(
        failure,
        StoreFailure::new(
            "GameBanana is busy, try again in a minute.",
            err.to_string()
        )
    );
    support::assert_plain(&failure.message);
}

#[test]
fn browse_failures_speak_of_gamebanana() {
    let cases = [
        (
            BrowseError::Network {
                url: GB_URL.into(),
                message: "refused".into(),
            },
            "Couldn't reach GameBanana. Check your internet connection and try again.",
        ),
        (
            BrowseError::Throttled { url: GB_URL.into() },
            "GameBanana is busy, try again in a minute.",
        ),
        (
            BrowseError::RateLimited { url: GB_URL.into() },
            "GameBanana is busy right now. Wait a few minutes, then try again.",
        ),
        (
            BrowseError::Http {
                url: GB_URL.into(),
                status: 404,
            },
            "GameBanana couldn't find this. Try again later.",
        ),
        (
            BrowseError::Gone {
                gb_id: GbId::new(5),
            },
            "This item is no longer on GameBanana.",
        ),
        (
            BrowseError::Http {
                url: GB_URL.into(),
                status: 503,
            },
            "GameBanana is having problems right now. Try again later.",
        ),
        (
            BrowseError::Http {
                url: GB_URL.into(),
                status: 403,
            },
            "GameBanana sent something TF2Tools couldn't read. Try again later.",
        ),
        (
            BrowseError::Malformed {
                url: GB_URL.into(),
                message: "x".into(),
            },
            "GameBanana sent something TF2Tools couldn't read. Try again later.",
        ),
    ];
    for (err, message) in cases {
        let failure = describe_browse(&err);
        assert_eq!(failure, StoreFailure::new(message, err.to_string()));
        support::assert_plain(&failure.message);
    }
}

#[test]
fn a_stopped_add_on_task_names_add_ons_not_files() {
    assert_eq!(
        addons_stopped("join error".to_owned()),
        StoreFailure::new(
            "Loading add-ons stopped before it finished. Try again.",
            "join error"
        )
    );
    assert_eq!(
        addon_stopped("join error".to_owned()),
        StoreFailure::new(
            "Loading this add-on stopped before it finished. Try again.",
            "join error"
        )
    );
    support::assert_plain(&addons_stopped("join error".to_owned()).message);
    support::assert_plain(&addon_stopped("join error".to_owned()).message);
}

#[test]
fn addon_fetch_failures_read_as_plain_words() {
    use tf2tools_app::store_messages::describe_gb_fetch;
    use tf2tools_core::{ArchiveError, GbAddonFetchError, GbId};
    let p = || PathBuf::from("/tmp/x.rar");
    let cases: Vec<(GbAddonFetchError, &str)> = vec![
        (
            GbAddonFetchError::NoFiles {
                gb_id: GbId::new(1),
            },
            "This add-on has no files to install. Pick another add-on.",
        ),
        (
            GbAddonFetchError::VirusScan {
                file_name: "a.zip".into(),
                result: "infected".into(),
            },
            "GameBanana's virus scan didn't pass this file, so it wasn't installed.",
        ),
        (
            GbAddonFetchError::TooLarge {
                url: GB_URL.into(),
                limit: 1,
            },
            "This download is too big for TF2Tools to install.",
        ),
        (
            GbAddonFetchError::OnlyExe {
                file_name: "a.zip".into(),
            },
            "This download only holds a program, which TF2Tools won't run. It wasn't installed.",
        ),
        (
            GbAddonFetchError::UnknownLayout {
                file_name: "a.zip".into(),
                payloads: 0,
            },
            "TF2Tools found nothing in this download it knows how to install, so it wasn't installed.",
        ),
        (
            GbAddonFetchError::UnknownLayout {
                file_name: "a.zip".into(),
                payloads: 40,
            },
            "This download holds too many parts for TF2Tools to tell which to install, so it wasn't installed.",
        ),
        (
            GbAddonFetchError::Archive(ArchiveError::UnsafePath {
                path: p(),
                entry: "../x".into(),
            }),
            "This download looks unsafe and was not installed.",
        ),
        (
            GbAddonFetchError::Archive(ArchiveError::Encrypted { path: p() }),
            DL_LOCKED,
        ),
        (
            GbAddonFetchError::Archive(ArchiveError::Malformed {
                path: p(),
                message: "bad".into(),
            }),
            "This download can't be opened. Pick another add-on.",
        ),
        (
            GbAddonFetchError::ChecksumMismatch {
                file_name: "a.zip".into(),
                expected: "a".into(),
                found: "b".into(),
            },
            "The download was damaged on the way. Try again.",
        ),
        (
            GbAddonFetchError::Network {
                url: GB_URL.into(),
                message: "reset".into(),
            },
            "Couldn't reach GameBanana. Check your internet connection and try again.",
        ),
        (
            GbAddonFetchError::RateLimited { url: GB_URL.into() },
            "GameBanana is busy right now. Wait a few minutes, then try again.",
        ),
        (
            GbAddonFetchError::Http {
                url: GB_URL.into(),
                status: 503,
            },
            "GameBanana is having problems right now. Try again later.",
        ),
        (
            GbAddonFetchError::Http {
                url: GB_URL.into(),
                status: 404,
            },
            "GameBanana couldn't find this file. Try again later.",
        ),
        (
            GbAddonFetchError::Io {
                path: p(),
                source: io("No space left on device (os error 28)"),
            },
            DL_DISK,
        ),
        (
            GbAddonFetchError::Archive(ArchiveError::Io {
                path: p(),
                source: io("No space left on device (os error 28)"),
            }),
            DL_DISK,
        ),
        (
            GbAddonFetchError::Interrupted {
                message: "cancelled".into(),
            },
            "The download stopped before it finished. Try again.",
        ),
    ];
    for (err, want) in cases {
        let failure = describe_gb_fetch(&err);
        assert_eq!(failure.message, want, "{err:?}");
        assert_eq!(failure.detail, err.to_string());
        support::assert_plain(&failure.message);
    }
    // The log names the entry that made an archive unsafe.
    let unsafe_path = GbAddonFetchError::Archive(ArchiveError::UnsafePath {
        path: p(),
        entry: "../x".into(),
    });
    assert!(describe_gb_fetch(&unsafe_path).detail.contains("../x"));
}
