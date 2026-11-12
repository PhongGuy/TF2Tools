//! The one timestamp format the database is written with: RFC 3339 UTC, microseconds,
//! `+00:00`. Every value is 32 characters, so text order is time order.

use chrono::{DateTime, SecondsFormat, Utc};

/// `t` in the database format.
pub(crate) fn format(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Micros, false)
}

/// The current time in the database format.
pub(crate) fn now() -> String {
    format(Utc::now())
}

/// `text` as unix seconds, for any RFC 3339 time v5 or v6 wrote; `None` when it is not one.
pub(crate) fn unix_seconds(text: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|t| t.timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeDelta, TimeZone, Utc};

    #[test]
    fn timestamps_are_fixed_width_utc_with_microseconds() {
        let t = Utc.with_ymd_and_hms(2026, 10, 16, 5, 27, 0).unwrap();
        assert_eq!(format(t), "2026-10-16T05:27:00.000000+00:00");
        let now = now();
        assert_eq!(now.len(), 32, "{now}");
        assert!(now.ends_with("+00:00"));
        assert!(chrono::DateTime::parse_from_rfc3339(&now).is_ok());
    }

    #[test]
    fn text_order_is_time_order() {
        // Stale-row cleanup compares these as text, so text order must be time order.
        let base = Utc.with_ymd_and_hms(2026, 10, 16, 5, 27, 0).unwrap();
        let texts: Vec<String> = [
            base,
            base + TimeDelta::microseconds(1),
            base + TimeDelta::milliseconds(500),
            base + TimeDelta::seconds(1),
            base + TimeDelta::hours(30),
        ]
        .into_iter()
        .map(format)
        .collect();
        let mut sorted = texts.clone();
        sorted.sort();
        assert_eq!(texts, sorted);
        // v5 wrote chrono's variable-width default; a later v6 value still sorts after it.
        assert!(format(base + TimeDelta::seconds(1)).as_str() > "2026-10-16T05:27:00.5+00:00");
        assert!(format(base).as_str() > "2026-10-16T05:26:59.123456789+00:00");
    }

    #[test]
    fn unix_seconds_reads_v5_and_v6_text_and_rejects_junk() {
        assert_eq!(unix_seconds("2026-01-01T00:00:00Z"), Some(1_767_225_600));
        assert_eq!(
            unix_seconds("2026-01-01T00:00:00.000000+00:00"),
            Some(1_767_225_600)
        );
        assert_eq!(unix_seconds("garbage"), None);
    }
}
