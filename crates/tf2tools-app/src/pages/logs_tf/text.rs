//! The logs.tf page's wording and links, kept free of UI so tests read them without a window.

use tf2tools_core::{LogsSyncError, ResolveError, SteamId};

/// logs.tf's front page, which the page's credit opens.
pub const LOGS_TF_URL: &str = "https://logs.tf";

/// The empty page's explanation, shown until a player is tracked.
pub const EMPTY_TEXT: &str =
    "Track a player to see their class averages and logs from the last 90 days.";

/// Why a logs.tf change could not be kept, in player words.
pub const SAVE_FAILED: &str = "Couldn't save logs.tf data on this computer.";

/// Why a Steam lookup failed when Steam could not answer, in player words.
pub const STEAM_UNREACHABLE: &str = "Can't reach Steam. Check your internet connection.";

/// One log's page on logs.tf.
pub fn log_url(id: u64) -> String {
    format!("{LOGS_TF_URL}/{id}")
}

/// A player's profile on logs.tf, addressed by their SteamID64.
pub fn profile_url(id: SteamId) -> String {
    format!("{LOGS_TF_URL}/profile/{}", id.id64())
}

/// The add dialog's message for a failed lookup.
pub fn resolve_message(error: &ResolveError) -> &'static str {
    match error {
        ResolveError::NotFound => "We couldn't find that Steam profile.",
        ResolveError::RateLimited { .. }
        | ResolveError::Http { .. }
        | ResolveError::Network { .. }
        | ResolveError::TooLarge { .. }
        | ResolveError::Malformed { .. }
        | _ => STEAM_UNREACHABLE,
    }
}

/// Why a sync could not reach logs.tf or got an error page back, in player words.
pub const LOGS_TF_DOWN: &str = "logs.tf isn't responding right now. Try again later.";

/// The progress line while a sync downloads logs: "Downloading logs: 12 of 87".
pub fn progress_text(done: usize, total: usize) -> String {
    format!("Downloading logs: {done} of {total}")
}

/// The line under the chips for a failed sync. A cache failure and a player removed while
/// their sync waited both mean the logs could not be kept.
pub fn sync_message(error: &LogsSyncError) -> &'static str {
    match error {
        LogsSyncError::Network { .. } => "You're offline. Showing saved logs.",
        LogsSyncError::RateLimited { .. } => "logs.tf is busy. Try again in a few minutes.",
        LogsSyncError::Malformed { .. } => "logs.tf sent something we didn't understand.",
        LogsSyncError::Database { .. } | LogsSyncError::NotTracked { .. } => SAVE_FAILED,
        LogsSyncError::Http { .. } | LogsSyncError::TooLarge { .. } | _ => LOGS_TF_DOWN,
    }
}
