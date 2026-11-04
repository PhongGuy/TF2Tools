use std::path::PathBuf;

use tf2tools_app::sync_messages::{SyncFailure, describe_sync, sync_stopped};
use tf2tools_core::{ArchiveError, StoreKind, SyncError};

const GB_OFFLINE: &str = "Couldn't reach GameBanana. Check your internet connection and try again.";
const GH_OFFLINE: &str = "Couldn't reach GitHub. Check your internet connection and try again.";
const GB_BUSY: &str = "GameBanana is busy right now. Wait a few minutes, then try again.";
const GB_DOWN: &str = "GameBanana is having problems right now. Try again later.";
const GB_UNREADABLE: &str = "GameBanana sent something TF2Tools couldn't read. Try again later.";
const GH_UNREADABLE: &str = "GitHub sent something TF2Tools couldn't read. Try again later.";
const DISK: &str = "Couldn't save the store data. Make sure your disk has free space and TF2Tools can write to its folder, then try again.";
const LIBRARY: &str = "TF2Tools couldn't update its store data. Restart TF2Tools and try again.";
const STOPPED: &str = "The sync stopped before it finished. Try again.";

const GB_URL: &str = "https://gamebanana.com/apiv11/Util/Search/Results?_sModelName=Sound";
const GH_URL: &str = "https://api.github.com/repos/mastercomfig/mastercomfig/releases/latest";

// Every variant once, with the store kind it is reported for and the words the player reads.
fn cases() -> Vec<(StoreKind, SyncError, &'static str)> {
    vec![
        (
            StoreKind::Sounds,
            SyncError::Network {
                url: GB_URL.into(),
                message: "error sending request: connection refused".into(),
            },
            GB_OFFLINE,
        ),
        (
            StoreKind::Addons,
            SyncError::Network {
                url: GH_URL.into(),
                message: "dns error".into(),
            },
            GH_OFFLINE,
        ),
        (
            StoreKind::CuratedHuds,
            SyncError::Network {
                url: GH_URL.into(),
                message: "timed out".into(),
            },
            GH_OFFLINE,
        ),
        (
            StoreKind::Crosshairs,
            SyncError::RateLimited { url: GB_URL.into() },
            GB_BUSY,
        ),
        (
            StoreKind::GameBananaHuds,
            SyncError::Http {
                url: GB_URL.into(),
                status: 503,
            },
            GB_DOWN,
        ),
        (
            StoreKind::Sounds,
            SyncError::Http {
                url: GB_URL.into(),
                status: 404,
            },
            GB_UNREADABLE,
        ),
        (
            StoreKind::Addons,
            SyncError::Malformed {
                url: GH_URL.into(),
                message: "missing field `assets`".into(),
            },
            GH_UNREADABLE,
        ),
        (
            StoreKind::Sounds,
            SyncError::TooLarge {
                url: GB_URL.into(),
                limit: 1_048_576,
            },
            GB_UNREADABLE,
        ),
        (
            StoreKind::CuratedHuds,
            SyncError::Archive(ArchiveError::Rar {
                path: PathBuf::from("/tmp/hud-db.rar"),
            }),
            GH_UNREADABLE,
        ),
        (
            StoreKind::Sounds,
            SyncError::Io {
                path: PathBuf::from("/home/p/.local/share/x/store"),
                source: std::io::Error::other("No space left on device (os error 28)"),
            },
            DISK,
        ),
        (
            StoreKind::Sounds,
            SyncError::Database {
                message: "database is locked".into(),
            },
            LIBRARY,
        ),
        (
            StoreKind::Sounds,
            SyncError::Interrupted {
                message: "task cancelled".into(),
            },
            STOPPED,
        ),
    ]
}

#[test]
fn sync_errors_read_as_player_advice() {
    for (kind, err, want) in cases() {
        let raw = err.to_string();
        assert_eq!(
            describe_sync(kind, &err),
            SyncFailure::new(want, raw),
            "{kind:?} {err:?}"
        );
    }
}

#[test]
fn no_message_carries_a_url_status_or_error_text() {
    let mut messages: Vec<String> = cases()
        .iter()
        .map(|(k, e, _)| describe_sync(*k, e).message)
        .collect();
    messages.push(sync_stopped("the sync stopped: task 7 panicked").message);
    for message in messages {
        let lower = message.to_lowercase();
        for banned in [
            "http", "://", ".com", "error", "refused", "panic", "os ", "sqlite",
        ] {
            assert!(!lower.contains(banned), "{message:?} contains {banned:?}");
        }
        // The app's own name is the one number a player may read.
        let words = message.replace("TF2Tools", "");
        assert!(
            !words.chars().any(|c| c.is_ascii_digit()),
            "{message:?} has a number"
        );
        assert!(message.ends_with('.'), "{message:?} is not a sentence");
    }
}

#[test]
fn a_stopped_sync_keeps_its_detail() {
    assert_eq!(
        sync_stopped("the sync stopped: task 7 panicked"),
        SyncFailure::new(STOPPED, "the sync stopped: task 7 panicked")
    );
}
