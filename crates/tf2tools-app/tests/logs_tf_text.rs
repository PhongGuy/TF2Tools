//! The logs.tf page's wording and number formats.

use tf2tools_app::pages::logs_tf::text::{
    duration_text, one_decimal, progress_text, resolve_message, summary_text, sync_message,
    time_played_text, whole,
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
        (
            LogsSyncError::NotTracked { steam_id: 1 },
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
    assert_eq!(summary_text(87, false), "Last 90 days, 87 logs");
    assert_eq!(summary_text(1, false), "Last 90 days, 1 log");
    assert_eq!(summary_text(400, true), "Last 400 logs");
}

#[test]
fn durations_read_as_minutes_and_seconds() {
    assert_eq!(duration_text(1714), "28:34");
    assert_eq!(duration_text(59), "0:59");
    assert_eq!(duration_text(3600), "60:00");
}
