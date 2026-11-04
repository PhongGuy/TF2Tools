//! The Activity Log page's pure parts: the filter options, the page size and v5's
//! relative time and level texts.

use chrono::{DateTime, Utc};
use tf2tools_core::LogLevel;

/// Rows fetched per query; the next page's `before` is the last row's id.
pub const PAGE_SIZE: u32 = 100;

/// The category select: label, then the filter (`None` is All). v5's order, then Profiles and Addons.
pub const CATEGORIES: [(&str, Option<&str>); 9] = [
    ("All", None),
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
    ("All", None),
    ("Info", Some(LogLevel::Info)),
    ("Warning", Some(LogLevel::Warning)),
    ("Error", Some(LogLevel::Error)),
];

/// v5's `formatRelativeTime` between an RFC 3339 `timestamp` and `now`: "Just now" under a
/// minute (and for a future time), else "N minute(s)/hour(s)/day(s) ago", singular at 1.
/// An unparseable timestamp is returned as it is.
pub fn relative_time(timestamp: &str, now: DateTime<Utc>) -> String {
    let Ok(then) = DateTime::parse_from_rfc3339(timestamp) else {
        return timestamp.to_owned();
    };
    // Whole seconds, floored like v5's Math.floor; a future stamp counts as now.
    let seconds = (now - then.with_timezone(&Utc)).num_seconds();
    let (n, unit) = match seconds {
        ..60 => return "Just now".to_owned(),
        60..3600 => (seconds / 60, "minute"),
        3600..86400 => (seconds / 3600, "hour"),
        _ => (seconds / 86400, "day"),
    };
    let plural = if n == 1 { "" } else { "s" };
    format!("{n} {unit}{plural} ago")
}

/// v5's level name: "info", "warning", "error".
pub fn level_name(level: LogLevel) -> &'static str {
    // Core's own name is private, and its enum is non-exhaustive.
    match level {
        LogLevel::Warning => "warning",
        LogLevel::Error => "error",
        _ => "info",
    }
}
