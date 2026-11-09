//! Store sync failures in player words. The messages never carry URLs, paths or codes; the
//! raw error rides along as `detail` for the developer log.

use tf2tools_core::{StoreKind, SyncError};

const STOPPED: &str = "The sync stopped before it finished. Try again.";
const DISK: &str = "Couldn't save the store data. Make sure your disk has free space and TF2Tools can write to its folder, then try again.";
const DATABASE: &str = "TF2Tools couldn't update its store data. Restart TF2Tools and try again.";
const UNKNOWN: &str = "Something went wrong while syncing. Try again later.";
// Only GameBanana calls are paced, so a throttled call always names GameBanana.
pub(crate) const THROTTLED: &str = "GameBanana is busy, try again in a minute.";

/// What the player reads about a failed sync, and the raw error for the developer log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncFailure {
    pub message: String,
    pub detail: String,
}

impl SyncFailure {
    /// A failure with its player message and its raw developer detail.
    pub fn new(message: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            detail: detail.into(),
        }
    }
}

/// A failed sync of `kind` in player words; `detail` is the error's own text.
pub fn describe_sync(kind: StoreKind, err: &SyncError) -> SyncFailure {
    let source = source_name(kind);
    let message = match err {
        SyncError::Network { .. } => {
            format!("Couldn't reach {source}. Check your internet connection and try again.")
        }
        SyncError::RateLimited { .. } => {
            format!("{source} is busy right now. Wait a few minutes, then try again.")
        }
        SyncError::Http { status, .. } if *status >= 500 => {
            format!("{source} is having problems right now. Try again later.")
        }
        SyncError::Http { .. }
        | SyncError::Malformed { .. }
        | SyncError::TooLarge { .. }
        | SyncError::Archive(_) => {
            format!("{source} sent something TF2Tools couldn't read. Try again later.")
        }
        SyncError::Io { .. } => DISK.to_owned(),
        SyncError::Database { .. } => DATABASE.to_owned(),
        SyncError::Interrupted { .. } => STOPPED.to_owned(),
        SyncError::Throttled { .. } => THROTTLED.to_owned(),
        _ => UNKNOWN.to_owned(),
    };
    SyncFailure::new(message, err.to_string())
}

/// A sync whose task ended without an outcome, keeping `detail` for the log.
pub fn sync_stopped(detail: String) -> SyncFailure {
    SyncFailure::new(STOPPED, detail)
}

// The site a player knows each store by: hud-db and mastercomfig are both fetched from GitHub.
pub(crate) fn source_name(kind: StoreKind) -> &'static str {
    match kind {
        StoreKind::CuratedHuds | StoreKind::Addons => "GitHub",
        StoreKind::GameBananaHuds | StoreKind::Sounds | StoreKind::Crosshairs | _ => "GameBanana",
    }
}
