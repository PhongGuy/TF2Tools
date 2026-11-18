//! Map list and map download failures in player words. The messages never carry URLs,
//! paths or codes; the raw error rides along as `detail` for the developer log.

use tf2tools_core::{MapDownloadError, MapRemoveError, SyncError};

use gpui_kit::SharedString;

use crate::i18n::{Phrase, tr};
use crate::store_messages::StoreFailure;

// The site the map list and the maps come from, as players know it.
const SERVEME: &str = "serveme.tf";

fn database() -> SharedString {
    tr!("messages.map.database")
}

fn stopped() -> SharedString {
    tr!("messages.shared.download_stopped")
}

/// A failed fetch of serveme.tf's map list.
pub fn describe_map_sync(err: &SyncError) -> StoreFailure {
    StoreFailure::new(map_sync_phrase(err).text(), err.to_string())
}

/// The player line for a failed fetch of serveme.tf's map list, worded each time it is read.
pub fn map_sync_phrase(err: &SyncError) -> Phrase {
    match err {
        SyncError::Network { .. } => {
            Phrase::new(|| tr!("messages.shared.source_unreachable", source = SERVEME))
        }
        SyncError::RateLimited { .. } => {
            Phrase::new(|| tr!("messages.shared.source_busy", source = SERVEME))
        }
        SyncError::Http { status, .. } if *status >= 500 => {
            Phrase::new(|| tr!("messages.shared.source_down", source = SERVEME))
        }
        SyncError::Io { .. } | SyncError::Database { .. } => Phrase::new(database),
        SyncError::Http { .. }
        | SyncError::TooLarge { .. }
        | SyncError::Malformed { .. }
        | SyncError::Archive(_)
        | SyncError::Interrupted { .. }
        | _ => Phrase::new(|| tr!("messages.shared.source_unreadable", source = SERVEME)),
    }
}

/// A failed map download.
pub fn describe_map_download(err: &MapDownloadError) -> StoreFailure {
    let message = match err {
        MapDownloadError::InGame { .. } => tr!("messages.map.in_game"),
        MapDownloadError::AlreadyDownloaded { .. } => tr!("messages.map.already_downloaded"),
        MapDownloadError::BadName { .. } | MapDownloadError::NotAMap { .. } => {
            tr!("messages.map.not_a_map")
        }
        MapDownloadError::RateLimited { .. } => {
            tr!("messages.shared.source_busy", source = SERVEME)
        }
        MapDownloadError::Http { status, .. } if *status >= 500 => {
            tr!("messages.shared.source_down", source = SERVEME)
        }
        MapDownloadError::Http { .. } => tr!("messages.map.http"),
        MapDownloadError::Network { .. } => {
            tr!("messages.shared.source_unreachable", source = SERVEME)
        }
        MapDownloadError::TooLarge { .. } => tr!("messages.map.too_large"),
        MapDownloadError::Io { .. } => tr!("messages.map.disk"),
        MapDownloadError::Database { .. } => database(),
        _ => stopped(),
    };
    StoreFailure::new(message, err.to_string())
}

/// A failed map removal.
pub fn describe_map_remove(err: &MapRemoveError) -> StoreFailure {
    let message = match err {
        MapRemoveError::NotDownloaded { .. } => tr!("messages.map.not_downloaded"),
        MapRemoveError::Io { .. } => tr!("messages.map.remove_io"),
        MapRemoveError::Database { .. } | _ => database(),
    };
    StoreFailure::new(message, err.to_string())
}

/// A map task that ended without an outcome, keeping `detail` for the log.
pub fn map_stopped(detail: String) -> StoreFailure {
    StoreFailure::new(map_stopped_phrase().text(), detail)
}

/// The player line for a map task that ended without an outcome.
pub fn map_stopped_phrase() -> Phrase {
    Phrase::new(stopped)
}
