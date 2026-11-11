//! The logs.tf page's wording and number formats.

use chrono::NaiveDate;
use tf2tools_app::pages::logs_tf::text::{
    EMPTY_TEXT, NO_LOGS_TEXT, duration_text, one_decimal, progress_text, resolve_message,
    summary_text, sync_message, time_played_text, whole,
};
use tf2tools_core::{LogsSyncError, ResolveError};

#[test]
fn resolve_failures_read_as_the_spec_words_them() {
    assert_eq!(
        resolve_message(&ResolveError::NotFound),
        "We couldn't find that Steam profile."
    );
    let offline = ResolveError::Network {
        url: "u".into(),
        message: "m".into(),
    };
    assert_eq!(
        resolve_message(&offline),
        "Can't reach Steam. Check your internet connection."
    );
    let busy = ResolveError::RateLimited { url: "u".into() };
    assert_eq!(
        resolve_message(&busy),
        "Can't reach Steam. Check your internet connection."
    );
}

#[test]
fn sync_failures_and_progress_read_as_the_spec_words_them() {
    assert_eq!(progress_text(12, 87), "Downloading logs: 12 of 87");
    let url = || "u".to_owned();
    let cases = [
        (
            LogsSyncError::Network {
                url: url(),
                message: "m".into(),
            },
            "You're offline. Showing saved logs.",
        ),
        (
            LogsSyncError::RateLimited { url: url() },
            "logs.tf is busy. Try again in a few minutes.",
        ),
        (
            LogsSyncError::Http {
                url: url(),
                status: 503,
            },
            "logs.tf isn't responding right now. Try again later.",
        ),
        (
            LogsSyncError::TooLarge {
                url: url(),
                limit: 1,
            },
            "logs.tf isn't responding right now. Try again later.",
        ),
        (
            LogsSyncError::Malformed {
                url: url(),
                message: "m".into(),
            },
            "logs.tf sent something we didn't understand.",
        ),
        (
            LogsSyncError::Database {
                message: "m".into(),
            },
            "Couldn't save logs.tf data on this computer.",
        ),
    ];
    for (error, text) in cases {
        assert_eq!(sync_message(&error), text, "{error:?}");
    }
}

#[test]
fn grid_numbers_format_like_the_spec() {
    assert_eq!(time_played_text(0), "0m");
    assert_eq!(time_played_text(1559), "25m");
    assert_eq!(time_played_text(5400), "1h 30m");
    assert_eq!(one_decimal(0.6), "0.6");
    assert_eq!(one_decimal(2.666_666), "2.7");
    assert_eq!(whole(136.38), "136");
}

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

#[test]
fn the_summary_names_the_window_it_covers() {
    let (aug, nov) = (day(2026, 8, 12), day(2026, 11, 10));
    assert_eq!(
        summary_text(aug, nov, 247, false),
        "Aug 12 to Nov 10, 2026: 247 logs"
    );
    assert_eq!(
        summary_text(aug, nov, 1, false),
        "Aug 12 to Nov 10, 2026: 1 log"
    );
    assert_eq!(
        summary_text(day(2022, 6, 3), day(2022, 9, 1), 31, false),
        "Jun 3 to Sep 1, 2022: 31 logs"
    );
    assert_eq!(
        summary_text(day(2021, 11, 20), day(2022, 2, 18), 64, false),
        "Nov 20, 2021 to Feb 18, 2022: 64 logs"
    );
    assert_eq!(
        summary_text(aug, nov, 400, true),
        "Aug 12 to Nov 10, 2026: last 400 logs"
    );
    assert_eq!(
        summary_text(nov, nov, 400, true),
        "Nov 10, 2026: last 400 logs"
    );
}

#[test]
fn no_wording_promises_the_last_90_days_from_today() {
    assert_eq!(NO_LOGS_TEXT, "No logs on logs.tf for this player yet.");
    assert_eq!(
        EMPTY_TEXT,
        "Track a player to see their class averages and logs from their last 90 days of play."
    );
}

#[test]
fn durations_read_as_minutes_and_seconds() {
    assert_eq!(duration_text(1714), "28:34");
    assert_eq!(duration_text(59), "0:59");
    assert_eq!(duration_text(3600), "60:00");
}
