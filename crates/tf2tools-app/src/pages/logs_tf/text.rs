//! The logs.tf page's wording and links, kept free of UI so tests read them without a window.

use tf2tools_core::{ResolveError, SteamId};

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
