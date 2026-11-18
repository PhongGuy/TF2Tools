//! Store sync failures in player words. The messages never carry URLs, paths or codes; the
//! raw error rides along as `detail` for the developer log.

use gpui_kit::SharedString;
use tf2tools_core::{StoreKind, SyncError};

use crate::i18n::{Phrase, tr};

// Only GameBanana calls are paced, so a throttled call always names GameBanana.
pub(crate) fn throttled() -> SharedString {
    tr!("messages.sync.throttled")
}

/// What the player reads about a failed sync, and the raw error for the developer log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncFailure {
    pub message: Phrase,
    pub detail: String,
}

impl SyncFailure {
    /// A failure with its player message, worded when read, and its raw developer detail.
    pub fn new(message: Phrase, detail: impl Into<String>) -> Self {
        Self {
            message,
            detail: detail.into(),
        }
    }
}

/// A failed sync of `kind` in player words; `detail` is the error's own text.
pub fn describe_sync(kind: StoreKind, err: &SyncError) -> SyncFailure {
    // Classify now and word later: the phrase keeps only the source name, never the error.
    let source = source_name(kind);
    let message = match err {
        SyncError::Network { .. } => {
            Phrase::new(move || tr!("messages.shared.source_unreachable", source = source))
        }
        SyncError::RateLimited { .. } => {
            Phrase::new(move || tr!("messages.shared.source_busy", source = source))
        }
        SyncError::Http { status, .. } if *status >= 500 => {
            Phrase::new(move || tr!("messages.shared.source_down", source = source))
        }
        SyncError::Http { .. }
        | SyncError::Malformed { .. }
        | SyncError::TooLarge { .. }
        | SyncError::Archive(_) => {
            Phrase::new(move || tr!("messages.shared.source_unreadable", source = source))
        }
        SyncError::Io { .. } => Phrase::new(|| tr!("messages.sync.disk")),
        SyncError::Database { .. } => Phrase::new(|| tr!("messages.sync.database")),
        SyncError::Interrupted { .. } => Phrase::new(|| tr!("messages.sync.stopped")),
        SyncError::Throttled { .. } => Phrase::new(throttled),
        _ => Phrase::new(|| tr!("messages.sync.unknown")),
    };
    SyncFailure::new(message, err.to_string())
}

/// The title of a failed sync's toast, one whole sentence per store so each language words it its own way.
pub fn sync_failed_title(kind: StoreKind) -> SharedString {
    match kind {
        StoreKind::CuratedHuds => tr!("messages.sync.failed_title.curated_huds"),
        StoreKind::GameBananaHuds => tr!("messages.sync.failed_title.gamebanana_huds"),
        StoreKind::Sounds => tr!("messages.sync.failed_title.sounds"),
        StoreKind::Crosshairs => tr!("messages.sync.failed_title.crosshairs"),
        StoreKind::Addons => tr!("messages.sync.failed_title.addons"),
        _ => tr!("messages.sync.failed_title.store"),
    }
}

/// A sync whose task ended without an outcome, keeping `detail` for the log.
pub fn sync_stopped(detail: String) -> SyncFailure {
    SyncFailure::new(Phrase::new(|| tr!("messages.sync.stopped")), detail)
}

// Add-ons, maps and previews outside the sync stores always come from GameBanana.
pub(crate) const GAMEBANANA: &str = "GameBanana";

// The site a player knows each store by: hud-db and mastercomfig are both fetched from GitHub.
pub(crate) fn source_name(kind: StoreKind) -> &'static str {
    match kind {
        StoreKind::CuratedHuds | StoreKind::Addons => "GitHub",
        StoreKind::GameBananaHuds | StoreKind::Sounds | StoreKind::Crosshairs | _ => GAMEBANANA,
    }
}
