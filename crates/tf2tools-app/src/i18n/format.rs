//! Numbers, sizes, dates and durations in the current language: its decimal point, digit
//! grouping, units, month names and date shapes. The digit work is pure and takes the
//! separators; the public functions read them from the catalogue.

use chrono::{Datelike as _, NaiveDate, NaiveDateTime, Timelike as _};
use gpui_kit::SharedString;

use super::tr;

/// How far a compact count was scaled down: to the first tier (a thousand, or a Chinese ten
/// thousand) or the second (that step squared).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scale {
    Units,
    Thousands,
    Millions,
}

/// A number already rounded with an English point ("0.6") in the current language ("0,6").
pub fn decimal_point(rounded: &str) -> String {
    with_point(rounded, &tr!("common.number.decimal"))
}

/// `n` with its thousands grouped: "4,812" in English, "4 812" (no-break space) in Russian.
pub fn grouped(n: u64) -> String {
    group_with(n, &tr!("common.number.group"))
}

/// v5's compact like and view count: "999", "1.3K", "1.2M" in English, "9,999", "1.3万",
/// "124万", "1.2亿" in Chinese.
pub fn compact_count(n: i64) -> String {
    let (digits, scale) = compact_parts(n, compact_step(&tr!("common.count.step")));
    let number = decimal_point(&digits);
    match scale {
        Scale::Units => match u64::try_from(n) {
            Ok(n) => grouped(n),
            Err(_) => number,
        },
        Scale::Thousands => tr!("common.count.thousand", number = number).into(),
        Scale::Millions => tr!("common.count.million", number = number).into(),
    }
}

/// A byte count in the largest 1024 unit that keeps it under 1024: "512 B", "1.5 KB".
pub fn bytes(n: u64) -> SharedString {
    let (digits, unit) = scale_bytes(n);
    let size = decimal_point(&digits);
    match unit {
        0 => tr!("common.size.b", size = size),
        1 => tr!("common.size.kb", size = size),
        2 => tr!("common.size.mb", size = size),
        3 => tr!("common.size.gb", size = size),
        _ => tr!("common.size.tb", size = size),
    }
}

/// A date as digits, padded: "2026-03-05" in English, "05.03.2026" in Russian.
pub fn numeric_date(d: NaiveDate) -> SharedString {
    tr!(
        "common.date.numeric",
        year = d.year(),
        month = format!("{:02}", d.month()),
        day = format!("{:02}", d.day())
    )
}

/// A numeric date and a 24-hour clock time, with the seconds when `seconds` is set.
pub fn date_time(dt: NaiveDateTime, seconds: bool) -> SharedString {
    let time = if seconds {
        format!("{:02}:{:02}:{:02}", dt.hour(), dt.minute(), dt.second())
    } else {
        format!("{:02}:{:02}", dt.hour(), dt.minute())
    };
    tr!(
        "common.date.date_time",
        date = numeric_date(dt.date()),
        time = time
    )
}

/// A date with a short month name and an unpadded day: "13 Dec 2022".
pub fn day_month_year(d: NaiveDate) -> SharedString {
    tr!(
        "common.date.day_month_year",
        day = d.day(),
        month = month_name(d.month()),
        year = d.year()
    )
}

/// A date led by its short month name in English: "Nov 16, 2026".
pub fn month_day_year(d: NaiveDate) -> SharedString {
    tr!(
        "common.date.month_day_year",
        day = d.day(),
        month = month_name(d.month()),
        year = d.year()
    )
}

/// Time played, in compact units: "45m", "1h 30m".
pub fn hours_minutes(total_minutes: u64) -> SharedString {
    let (hours, minutes) = (total_minutes / 60, total_minutes % 60);
    if hours == 0 {
        tr!("common.duration.played_minutes", minutes = minutes)
    } else {
        tr!(
            "common.duration.played_hours_minutes",
            hours = hours,
            minutes = minutes
        )
    }
}

/// Time remaining, dropping a zero part: "40 min", "2 h", "2 h 40 min".
pub fn time_left(total_minutes: u64) -> SharedString {
    match (total_minutes / 60, total_minutes % 60) {
        (0, minutes) => tr!("common.duration.minutes", minutes = minutes),
        (hours, 0) => tr!("common.duration.hours", hours = hours),
        (hours, minutes) => tr!(
            "common.duration.hours_minutes",
            hours = hours,
            minutes = minutes
        ),
    }
}

/// Whole seconds: "58 s".
pub fn seconds(n: u64) -> SharedString {
    tr!("common.duration.seconds", seconds = n)
}

/// Seconds the caller already rounded to tenths with an English point: "6.5" -> "6.5 s".
pub fn tenths(rounded: &str) -> SharedString {
    tr!("common.duration.seconds", seconds = decimal_point(rounded))
}

// The short name of `month` (1 to 12); each arm names its key so the catalogue scans see it.
fn month_name(month: u32) -> SharedString {
    match month {
        1 => tr!("common.date.month.jan"),
        2 => tr!("common.date.month.feb"),
        3 => tr!("common.date.month.mar"),
        4 => tr!("common.date.month.apr"),
        5 => tr!("common.date.month.may"),
        6 => tr!("common.date.month.jun"),
        7 => tr!("common.date.month.jul"),
        8 => tr!("common.date.month.aug"),
        9 => tr!("common.date.month.sep"),
        10 => tr!("common.date.month.oct"),
        11 => tr!("common.date.month.nov"),
        _ => tr!("common.date.month.dec"),
    }
}

// The digits of `n` with `sep` before every group of three counted from the right.
fn group_with(n: u64, sep: &str) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 * sep.len());
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push_str(sep);
        }
        out.push(c);
    }
    out
}

// Swaps the first '.' of an already rounded number for the language's `point`.
fn with_point(rounded: &str, point: &str) -> String {
    rounded.replacen('.', point, 1)
}

// The tier size a catalogue's `common.count.step` names: a power of ten from 10 to 10^9, else
// a thousand.
fn compact_step(text: &str) -> i64 {
    match text.parse::<i64>() {
        Ok(step) if (1..=9).any(|k| 10_i64.pow(k) == step) => step,
        _ => 1_000,
    }
}

// The English digits of a compact count with one decimal, and the tier of size `step` (or
// `step` squared) they were scaled to.
fn compact_parts(n: i64, step: i64) -> (String, Scale) {
    let (unit, scale) = if n >= step * step {
        (step * step, Scale::Millions)
    } else if n >= step {
        (step, Scale::Thousands)
    } else {
        return (n.to_string(), Scale::Units);
    };
    // `{:.1}` rounds the exact binary value as `toFixed` does, except on a tie exact in binary
    // (a quarter or three quarters of a unit), where it rounds to even and `toFixed` rounds up.
    let digits = if n % (unit / 2) == unit / 4 {
        let tenths = (n * 10 + unit / 2) / unit;
        format!("{}.{}", tenths / 10, tenths % 10)
    } else {
        format!("{:.1}", n as f64 / unit as f64)
    };
    // A ten-thousand step writes three or more whole digits without a decimal ("124万").
    let whole_digits = digits.find('.').unwrap_or(digits.len());
    if step >= 10_000 && whole_digits >= 3 {
        return (((n + unit / 2) / unit).to_string(), scale);
    }
    (digits, scale)
}

// A byte count as English digits with one decimal and its unit index (0 B to 4 TB).
fn scale_bytes(bytes: u64) -> (String, usize) {
    if bytes < 1024 {
        return (bytes.to_string(), 0);
    }
    // Step up while the value would print as 1024.0 or more (judged on the one-decimal
    // rounding the label shows), stopping at TB.
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 1;
    while (value * 10.0).round() / 10.0 >= 1024.0 && unit < 4 {
        value /= 1024.0;
        unit += 1;
    }
    (format!("{value:.1}"), unit)
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::{
        Scale, bytes, compact_count, compact_parts, compact_step, date_time, day_month_year,
        decimal_point, group_with, grouped, hours_minutes, month_day_year, month_name,
        numeric_date, scale_bytes, seconds, tenths, time_left, with_point,
    };

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn digits_group_in_threes_with_either_separator() {
        assert_eq!(group_with(0, ","), "0");
        assert_eq!(group_with(999, ","), "999");
        assert_eq!(group_with(4_812, ","), "4,812");
        assert_eq!(group_with(1_234_567, ","), "1,234,567");
        assert_eq!(group_with(4_812, "\u{a0}"), "4\u{a0}812");
        assert_eq!(group_with(1_000_000, "\u{a0}"), "1\u{a0}000\u{a0}000");
    }

    #[test]
    fn the_decimal_point_is_swapped_after_rounding() {
        assert_eq!(with_point("0.6", "."), "0.6");
        assert_eq!(with_point("0.6", ","), "0,6");
        assert_eq!(with_point("1023.9", ","), "1023,9");
        assert_eq!(with_point("136", ","), "136");
    }

    #[test]
    fn compact_counts_keep_their_thresholds_and_ties() {
        assert_eq!(compact_parts(999, 1_000), ("999".to_owned(), Scale::Units));
        assert_eq!(
            compact_parts(1_000, 1_000),
            ("1.0".to_owned(), Scale::Thousands)
        );
        assert_eq!(
            compact_parts(1_049, 1_000),
            ("1.0".to_owned(), Scale::Thousands)
        );
        assert_eq!(
            compact_parts(1_050, 1_000),
            ("1.1".to_owned(), Scale::Thousands)
        );
        assert_eq!(
            compact_parts(1_250, 1_000),
            ("1.3".to_owned(), Scale::Thousands)
        );
        assert_eq!(
            compact_parts(1_150, 1_000),
            ("1.1".to_owned(), Scale::Thousands)
        );
        assert_eq!(
            compact_parts(999_999, 1_000),
            ("1000.0".to_owned(), Scale::Thousands)
        );
        assert_eq!(
            compact_parts(1_000_000, 1_000),
            ("1.0".to_owned(), Scale::Millions)
        );
        assert_eq!(
            compact_parts(1_250_000, 1_000),
            ("1.3".to_owned(), Scale::Millions)
        );
    }

    #[test]
    fn ten_thousand_steps_compact_by_wan_and_yi() {
        let wan = |n| compact_parts(n, 10_000);
        assert_eq!(wan(9_999), ("9999".to_owned(), Scale::Units));
        assert_eq!(wan(10_000), ("1.0".to_owned(), Scale::Thousands));
        assert_eq!(wan(12_500), ("1.3".to_owned(), Scale::Thousands));
        assert_eq!(wan(999_400), ("99.9".to_owned(), Scale::Thousands));
        assert_eq!(wan(999_950), ("100".to_owned(), Scale::Thousands));
        assert_eq!(wan(1_240_000), ("124".to_owned(), Scale::Thousands));
        assert_eq!(wan(1_245_000), ("125".to_owned(), Scale::Thousands));
        assert_eq!(wan(9_999_500), ("1000".to_owned(), Scale::Thousands));
        assert_eq!(wan(120_000_000), ("1.2".to_owned(), Scale::Millions));
    }

    #[test]
    fn the_compact_step_reads_a_power_of_ten_else_a_thousand() {
        assert_eq!(compact_step("1000"), 1_000);
        assert_eq!(compact_step("10000"), 10_000);
        for bad in [
            "",
            "abc",
            "1500",
            "0",
            "1",
            "-1000",
            "10000000000",
            "%{step}",
        ] {
            assert_eq!(compact_step(bad), 1_000, "{bad:?}");
        }
    }

    #[test]
    fn bytes_scale_by_1024_with_one_decimal() {
        assert_eq!(scale_bytes(1_023), ("1023".to_owned(), 0));
        assert_eq!(scale_bytes(1_280), ("1.2".to_owned(), 1));
        assert_eq!(scale_bytes(1_048_524), ("1023.9".to_owned(), 1));
        assert_eq!(scale_bytes(1_048_575), ("1.0".to_owned(), 2));
        assert_eq!(scale_bytes(3 << 40), ("3.0".to_owned(), 4));
    }

    #[test]
    fn english_numbers_and_sizes_are_todays() {
        assert_eq!(decimal_point("0.6"), "0.6");
        assert_eq!(grouped(4_812), "4,812");
        assert_eq!(compact_count(999), "999");
        assert_eq!(compact_count(1_250), "1.3K");
        assert_eq!(compact_count(1_240_000), "1.2M");
        assert_eq!(bytes(512), "512 B");
        assert_eq!(bytes(1_536), "1.5 KB");
        assert_eq!(bytes(3 << 40), "3.0 TB");
    }

    #[test]
    fn the_month_table_names_all_twelve() {
        let names: Vec<String> = (1..=12).map(|m| month_name(m).to_string()).collect();
        assert_eq!(
            names,
            [
                "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"
            ]
        );
    }

    #[test]
    fn english_dates_and_durations_are_todays() {
        assert_eq!(numeric_date(day(2026, 3, 5)), "2026-03-05");
        let at = day(2026, 10, 17).and_hms_opt(20, 56, 9).unwrap();
        assert_eq!(date_time(at, false), "2026-10-17 20:56");
        assert_eq!(date_time(at, true), "2026-10-17 20:56:09");
        assert_eq!(day_month_year(day(2022, 12, 13)), "13 Dec 2022");
        assert_eq!(day_month_year(day(2022, 5, 3)), "3 May 2022");
        assert_eq!(month_day_year(day(2026, 11, 16)), "Nov 16, 2026");
        assert_eq!(hours_minutes(0), "0m");
        assert_eq!(hours_minutes(45), "45m");
        assert_eq!(hours_minutes(60), "1h 0m");
        assert_eq!(hours_minutes(90), "1h 30m");
        assert_eq!(time_left(160), "2 h 40 min");
        assert_eq!(time_left(60), "1 h");
        assert_eq!(time_left(12), "12 min");
        assert_eq!(seconds(58), "58 s");
        assert_eq!(tenths("6.5"), "6.5 s");
    }
}
