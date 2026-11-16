//! Store sync failures in player words. The messages never carry URLs, paths or codes; the
//! raw error rides along as `detail` for the developer log.

use gpui_kit::SharedString;
use tf2tools_core::{StoreKind, SyncError};

use crate::i18n::tr;

// Only GameBanana calls are paced, so a throttled call always names GameBanana.
pub(crate) fn throttled() -> SharedString {
    tr!("messages.sync.throttled")
}

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
        SyncError::Network { .. } => tr!("messages.shared.source_unreachable", source = source),
        SyncError::RateLimited { .. } => tr!("messages.shared.source_busy", source = source),
        SyncError::Http { status, .. } if *status >= 500 => {
            tr!("messages.shared.source_down", source = source)
        }
        SyncError::Http { .. }
        | SyncError::Malformed { .. }
        | SyncError::TooLarge { .. }
        | SyncError::Archive(_) => tr!("messages.shared.source_unreadable", source = source),
        SyncError::Io { .. } => tr!("messages.sync.disk"),
        SyncError::Database { .. } => tr!("messages.sync.database"),
        SyncError::Interrupted { .. } => tr!("messages.sync.stopped"),
        SyncError::Throttled { .. } => throttled(),
        _ => tr!("messages.sync.unknown"),
    };
    SyncFailure::new(message, err.to_string())
}

/// A sync whose task ended without an outcome, keeping `detail` for the log.
pub fn sync_stopped(detail: String) -> SyncFailure {
    SyncFailure::new(tr!("messages.sync.stopped"), detail)
}

// The site a player knows each store by: hud-db and mastercomfig are both fetched from GitHub.
pub(crate) fn source_name(kind: StoreKind) -> &'static str {
    match kind {
        StoreKind::CuratedHuds | StoreKind::Addons => "GitHub",
        StoreKind::GameBananaHuds | StoreKind::Sounds | StoreKind::Crosshairs | _ => "GameBanana",
    }
}
