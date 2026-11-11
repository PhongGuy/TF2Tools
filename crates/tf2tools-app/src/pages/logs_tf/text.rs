//! The logs.tf page's wording and links, kept free of UI so tests read them without a window.

use tf2tools_core::SteamId;

/// logs.tf's front page, which the page's credit opens.
pub const LOGS_TF_URL: &str = "https://logs.tf";

/// The empty page's explanation, shown until a player is tracked.
pub const EMPTY_TEXT: &str =
    "Track a player to see their class averages and logs from the last 90 days.";

/// Why a logs.tf change could not be kept, in player words.
pub const SAVE_FAILED: &str = "Couldn't save logs.tf data on this computer.";

/// One log's page on logs.tf.
pub fn log_url(id: u64) -> String {
    format!("{LOGS_TF_URL}/{id}")
}

/// A player's profile on logs.tf, addressed by their SteamID64.
pub fn profile_url(id: SteamId) -> String {
    format!("{LOGS_TF_URL}/profile/{}", id.id64())
}
