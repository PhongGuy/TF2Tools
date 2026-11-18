//! The Activity Log page's pure parts: the filter options, the page size and v5's
//! relative time and level texts.

use chrono::{DateTime, Utc};
use gpui_kit::SharedString;
use tf2tools_core::LogLevel;

use crate::i18n::{plural, tr};
use crate::ids;
use crate::widgets::label_option::LabelOption;

/// Rows fetched per query; the next page's `before` is the last row's id.
pub const PAGE_SIZE: u32 = 100;

/// The category select: English label (the row's value and id), then the filter (`None` is
/// every category). v5's order, then Profiles and Addons.
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

/// The category Select's rows: each answers its English `CATEGORIES` label, shown in the
/// player's language.
pub fn category_options() -> Vec<LabelOption> {
    let label = |value: &str| match value {
        "HUDs" => tr!("nav.huds"),
        "Sounds" => tr!("nav.sounds"),
        "Crosshairs" => tr!("nav.crosshairs"),
        "Store" => tr!("activity_log.category.store"),
        "Settings" => tr!("nav.settings"),
        "System" => tr!("activity_log.category.system"),
        "Profiles" => tr!("nav.profiles"),
        "Addons" => tr!("nav.addons"),
        "All categories" => tr!("activity_log.filter.all_categories"),
        // A row added without its own arm shows its English label rather than another row's.
        other => SharedString::from(other.to_owned()),
    };
    LabelOption::keyed(
        ids::LOGS_CATEGORY,
        CATEGORIES.iter().map(|(value, _)| (*value, label(value))),
    )
}

/// The level Select's rows, keyed by their English `LEVELS` label like `category_options`.
pub fn level_options() -> Vec<LabelOption> {
    let label = |level: Option<LogLevel>| match level {
        None => tr!("activity_log.filter.all_levels"),
        Some(LogLevel::Warning) => tr!("activity_log.level.warning"),
        Some(LogLevel::Error) => tr!("activity_log.level.error"),
        Some(LogLevel::Info | _) => tr!("activity_log.level.info"),
    };
    LabelOption::keyed(
        ids::LOGS_LEVEL,
        LEVELS.iter().map(|(value, level)| (*value, label(*level))),
    )
}

/// A row's stored category in player words; an unknown one has its underscores spaced out.
pub fn category_label(category: &str) -> SharedString {
    match category {
        "hud" => tr!("activity_log.category.hud"),
        "sound" => tr!("activity_log.category.sound"),
        "crosshair" => tr!("activity_log.category.crosshair"),
        "addon" => tr!("activity_log.category.addon"),
        "profile" => tr!("activity_log.category.profile"),
        "store" => tr!("activity_log.category.store"),
        "system" => tr!("activity_log.category.system"),
        "settings" => tr!("nav.settings"),
        "hud_store" => tr!("nav.hud_store"),
        "sound_store" => tr!("nav.sound_store"),
        "crosshair_store" => tr!("nav.crosshair_store"),
        "addon_store" => tr!("nav.addon_store"),
        _ => spaced(category).into(),
    }
}

/// A row's stored action in player words, read the same way as `category_label`.
pub fn action_label(action: &str) -> SharedString {
    match action {
        "activate" => tr!("activity_log.action.activate"),
        "copy" => tr!("activity_log.action.copy"),
        "create" => tr!("activity_log.action.create"),
        "delete" => tr!("activity_log.action.delete"),
        "download" => tr!("activity_log.action.download"),
        "import" => tr!("activity_log.action.import"),
        "install" => tr!("activity_log.action.install"),
        "install_fonts" => tr!("activity_log.action.install_fonts"),
        "make" => tr!("activity_log.action.make"),
        "paint" => tr!("activity_log.action.paint"),
        "remove" => tr!("activity_log.action.remove"),
        "rename" => tr!("activity_log.action.rename"),
        "repair" => tr!("activity_log.action.repair"),
        "save_current" => tr!("activity_log.action.save_current"),
        "startup" => tr!("activity_log.action.startup"),
        "sync" => tr!("activity_log.action.sync"),
        "uninstall" => tr!("activity_log.action.uninstall"),
        "update" => tr!("activity_log.action.update"),
        "update-download" => tr!("activity_log.action.update_download"),
        "update-install" => tr!("activity_log.action.update_install"),
        "update-restart" => tr!("activity_log.action.update_restart"),
        _ => spaced(action).into(),
    }
}

/// A level's name for the row's level icon, in the player's language.
pub fn level_label(level: LogLevel) -> SharedString {
    match level {
        LogLevel::Warning => tr!("activity_log.level_name.warning"),
        LogLevel::Error => tr!("activity_log.level_name.error"),
        LogLevel::Info | _ => tr!("activity_log.level_name.info"),
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
