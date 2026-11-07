//! Map list and map download failures in player words. The messages never carry URLs,
//! paths or codes; the raw error rides along as `detail` for the developer log.

use tf2tools_core::{MapDownloadError, MapRemoveError, SyncError};

use crate::store_messages::StoreFailure;

const DISK: &str = "Couldn't save the map. Make sure your disk has free space and TF2Tools can write to the TF2 folder, then try again.";
const DATABASE: &str = "TF2Tools couldn't update its map list. Restart TF2Tools and try again.";
const STOPPED: &str = "The download stopped before it finished. Try again.";

/// A failed fetch of serveme.tf's map list.
pub fn describe_map_sync(err: &SyncError) -> StoreFailure {
    let message = match err {
        SyncError::Network { .. } => {
            "Couldn't reach serveme.tf. Check your internet connection and try again."
        }
        SyncError::RateLimited { .. } => {
            "serveme.tf is busy right now. Wait a few minutes, then try again."
        }
        SyncError::Http { status, .. } if *status >= 500 => {
            "serveme.tf is having problems right now. Try again later."
        }
        SyncError::Io { .. } | SyncError::Database { .. } => DATABASE,
        SyncError::Http { .. }
        | SyncError::TooLarge { .. }
        | SyncError::Malformed { .. }
        | SyncError::Archive(_)
        | SyncError::Interrupted { .. }
        | _ => "serveme.tf sent something TF2Tools couldn't read. Try again later.",
    };
    StoreFailure::new(message, err.to_string())
}

/// A failed map download.
pub fn describe_map_download(err: &MapDownloadError) -> StoreFailure {
    let message = match err {
        MapDownloadError::InGame { .. } => "TF2 already has this map.",
        MapDownloadError::AlreadyDownloaded { .. } => "This map is already downloaded.",
        MapDownloadError::BadName { .. } | MapDownloadError::NotAMap { .. } => {
            "serveme.tf didn't send a map file. Try again later."
        }
        MapDownloadError::RateLimited { .. } => {
            "serveme.tf is busy right now. Wait a few minutes, then try again."
        }
        MapDownloadError::Http { status, .. } if *status >= 500 => {
            "serveme.tf is having problems right now. Try again later."
        }
        MapDownloadError::Http { .. } => "Couldn't get this map from serveme.tf. Try again later.",
        MapDownloadError::Network { .. } => {
            "Couldn't reach serveme.tf. Check your internet connection and try again."
        }
        MapDownloadError::TooLarge { .. } => "This map is too big for TF2Tools.",
        MapDownloadError::Io { .. } => DISK,
        MapDownloadError::Database { .. } => DATABASE,
        _ => STOPPED,
    };
    StoreFailure::new(message, err.to_string())
}

/// A failed map removal.
pub fn describe_map_remove(err: &MapRemoveError) -> StoreFailure {
    let message = match err {
        MapRemoveError::NotDownloaded { .. } => {
            "TF2Tools only removes maps it downloaded, and this one came from somewhere else."
        }
        MapRemoveError::Io { .. } => {
            "Couldn't delete the map. Close TF2 if it is running, then try again."
        }
        MapRemoveError::Database { .. } | _ => DATABASE,
    };
    StoreFailure::new(message, err.to_string())
}

/// A map task that ended without an outcome, keeping `detail` for the log.
pub fn map_stopped(detail: impl Into<String>) -> StoreFailure {
    StoreFailure::new(STOPPED, detail)
}
