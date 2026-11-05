use chrono::{DateTime, Duration, Utc};
use tf2tools_app::pages::logs::model::{
    CATEGORIES, LEVELS, PAGE_SIZE, action_label, category_label, level_name, relative_time,
};
use tf2tools_app::shell::confirm::Confirm;
use tf2tools_core::LogLevel;

fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-10-17T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

fn ago(d: Duration) -> String {
    (now() - d).to_rfc3339_opts(chrono::SecondsFormat::Micros, false)
}

#[test]
fn relative_time_reads_like_v5() {
    assert_eq!(
        relative_time(&ago(Duration::seconds(30)), now()),
        "Just now"
    );
    assert_eq!(
        relative_time(&ago(Duration::seconds(60)), now()),
        "1 minute ago"
    );
    assert_eq!(
        relative_time(&ago(Duration::seconds(3599)), now()),
        "59 minutes ago"
    );
    assert_eq!(relative_time(&ago(Duration::hours(1)), now()), "1 hour ago");
    assert_eq!(
        relative_time(&ago(Duration::hours(23)), now()),
        "23 hours ago"
    );
    assert_eq!(relative_time(&ago(Duration::days(1)), now()), "1 day ago");
    assert_eq!(relative_time(&ago(Duration::days(3)), now()), "3 days ago");
}

#[test]
fn a_future_or_garbled_time_does_not_break() {
    assert_eq!(
        relative_time(&ago(Duration::seconds(-90)), now()),
        "Just now"
    );
    assert_eq!(relative_time("yesterday-ish", now()), "yesterday-ish");
}

#[test]
fn v5_microsecond_stamps_parse() {
    assert_eq!(
        relative_time("2026-10-17T11:58:00.123456+00:00", now()),
        "1 minute ago"
    );
}

#[test]
fn the_selects_list_v5s_options_plus_profiles_and_addons() {
    let cats: Vec<_> = CATEGORIES.iter().map(|(l, v)| (*l, *v)).collect();
    assert_eq!(
        cats,
        [
            ("All categories", None),
            ("HUDs", Some("hud")),
            ("Sounds", Some("sound")),
            ("Crosshairs", Some("crosshair")),
            ("Store", Some("store")),
            ("Settings", Some("settings")),
            ("System", Some("system")),
            ("Profiles", Some("profile")),
            ("Addons", Some("addon")),
        ]
    );
    let levels: Vec<_> = LEVELS.iter().map(|(l, v)| (*l, *v)).collect();
    assert_eq!(
        levels,
        [
            ("All levels", None),
            ("Info", Some(LogLevel::Info)),
            ("Warning", Some(LogLevel::Warning)),
            ("Error", Some(LogLevel::Error)),
        ]
    );
    assert_eq!(PAGE_SIZE, 100);
    assert_eq!(level_name(LogLevel::Info), "info");
    assert_eq!(level_name(LogLevel::Warning), "warning");
    assert_eq!(level_name(LogLevel::Error), "error");
}

#[test]
fn rows_name_categories_and_actions_in_player_words() {
    // Rows keep the stored names; only what the page shows is reworded.
    assert_eq!(category_label("hud"), "HUD");
    assert_eq!(category_label("addon_store"), "Addon Store");
    assert_eq!(category_label("hud_store"), "HUD Store");
    assert_eq!(category_label("store"), "Store");
    assert_eq!(category_label("profile"), "Profile");
    assert_eq!(action_label("save_current"), "Save current setup");
    assert_eq!(action_label("install_fonts"), "Install fonts");
    assert_eq!(action_label("download"), "Download");
    // A name this build never wrote still reads as words.
    assert_eq!(category_label("some_thing"), "Some thing");
    assert_eq!(action_label("re_sync"), "Re sync");
}

#[test]
fn clear_all_asks_like_v5() {
    let c = Confirm::clear_logs();
    assert_eq!(c.title.as_ref(), "Clear All Logs");
    assert_eq!(
        c.description.as_ref(),
        "Are you sure you want to clear all activity logs? This cannot be undone."
    );
    assert_eq!(c.confirm_label.as_ref(), "Clear All");
}
