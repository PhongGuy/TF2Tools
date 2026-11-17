//! The Activity Log page's pure parts: the filter options, the page size and v5's
//! relative time and level texts.

use chrono::{DateTime, Utc};
use tf2tools_core::LogLevel;

use crate::i18n::{plural, tr};

/// Rows fetched per query; the next page's `before` is the last row's id.
pub const PAGE_SIZE: u32 = 100;

/// The category select: label, then the filter (`None` is every category). v5's order, then
/// Profiles and Addons.
pub const CATEGORIES: [(&str, Option<&str>); 9] = [
    ("All categories", None),
    ("HUDs", Some("hud")),
    ("Sounds", Some("sound")),
    ("Crosshairs", Some("crosshair")),
    ("Store", Some("store")),
    ("Settings", Some("settings")),
    ("System", Some("system")),
    ("Profiles", Some("profile")),
    ("Addons", Some("addon")),
];

/// The level select: label, then the filter.
pub const LEVELS: [(&str, Option<LogLevel>); 4] = [
    ("All levels", None),
    ("Info", Some(LogLevel::Info)),
    ("Warning", Some(LogLevel::Warning)),
    ("Error", Some(LogLevel::Error)),
];

/// A row's stored category in player words; an unknown one has its underscores spaced out.
pub fn category_label(category: &str) -> String {
    let known = match category {
        "hud" => "HUD",
        "sound" => "Sound",
        "crosshair" => "Crosshair",
        "addon" => "Addon",
        "profile" => "Profile",
        "hud_store" => "HUD Store",
        "sound_store" => "Sound Store",
        "crosshair_store" => "Crosshair Store",
        "addon_store" => "Addon Store",
        _ => return spaced(category),
    };
    known.to_owned()
}

/// A row's stored action in player words, read the same way as `category_label`.
pub fn action_label(action: &str) -> String {
    match action {
        "save_current" => "Save current setup".to_owned(),
        _ => spaced(action),
    }
}

// "install_fonts" reads "Install fonts": underscores become spaces, the first letter a capital.
fn spaced(name: &str) -> String {
    let words = name.replace('_', " ");
    let mut chars = words.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// v5's `formatRelativeTime` between an RFC 3339 `timestamp` and `now`: "Just now" under a
/// minute (and for a future time), else "N minutes/hours/days ago" in the player's language.
/// An unparseable timestamp is returned as it is.
pub fn relative_time(timestamp: &str, now: DateTime<Utc>) -> String {
    let Ok(then) = DateTime::parse_from_rfc3339(timestamp) else {
        return timestamp.to_owned();
    };
    // Whole seconds, floored like v5's Math.floor; a future stamp counts as now.
    let seconds = (now - then.with_timezone(&Utc)).num_seconds();
    let text = match seconds {
        ..60 => return tr!("common.time.just_now").to_string(),
        60..3600 => plural("common.time.minutes_ago", (seconds / 60).unsigned_abs()),
        3600..86400 => plural("common.time.hours_ago", (seconds / 3600).unsigned_abs()),
        _ => plural("common.time.days_ago", (seconds / 86400).unsigned_abs()),
    };
    text.to_string()
}

/// v5's level name: "info", "warning", "error".
pub fn level_name(level: LogLevel) -> &'static str {
    // Core's own name is private, and its enum is non-exhaustive.
    match level {
        LogLevel::Warning => "warning",
        LogLevel::Error => "error",
        LogLevel::Info | _ => "info",
    }
}
