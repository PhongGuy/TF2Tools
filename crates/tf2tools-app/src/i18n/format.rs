//! Numbers and sizes in the current language: its decimal point, digit grouping and units.
//! The digit work is pure and takes the separators; the public functions read them from
//! the catalogue.

use gpui_kit::SharedString;

use super::tr;

/// How far a compact count was scaled down.
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

/// v5's compact like and view count: "999", "1.3K", "1.2M" in English.
pub fn compact_count(n: i64) -> String {
    let (digits, scale) = compact_parts(n);
    let number = decimal_point(&digits);
    match scale {
        Scale::Units => number,
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

// The English digits of a compact count with one decimal, and the tier they were scaled to.
fn compact_parts(n: i64) -> (String, Scale) {
    let (unit, scale) = match n {
        1_000_000.. => (1_000_000, Scale::Millions),
        1_000.. => (1_000, Scale::Thousands),
        _ => return (n.to_string(), Scale::Units),
    };
    // `{:.1}` rounds the exact binary value as `toFixed` does, except on a tie exact in binary
    // (a quarter or three quarters of a unit), where it rounds to even and `toFixed` rounds up.
    let digits = if n % (unit / 2) == unit / 4 {
        let tenths = (n * 10 + unit / 2) / unit;
        format!("{}.{}", tenths / 10, tenths % 10)
    } else {
        format!("{:.1}", n as f64 / unit as f64)
    };
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
    use super::{
        Scale, bytes, compact_count, compact_parts, decimal_point, group_with, grouped,
        scale_bytes, with_point,
    };

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
        assert_eq!(compact_parts(999), ("999".to_owned(), Scale::Units));
        assert_eq!(compact_parts(1_000), ("1.0".to_owned(), Scale::Thousands));
        assert_eq!(compact_parts(1_049), ("1.0".to_owned(), Scale::Thousands));
        assert_eq!(compact_parts(1_050), ("1.1".to_owned(), Scale::Thousands));
        assert_eq!(compact_parts(1_250), ("1.3".to_owned(), Scale::Thousands));
        assert_eq!(compact_parts(1_150), ("1.1".to_owned(), Scale::Thousands));
        assert_eq!(
            compact_parts(999_999),
            ("1000.0".to_owned(), Scale::Thousands)
        );
        assert_eq!(
            compact_parts(1_000_000),
            ("1.0".to_owned(), Scale::Millions)
        );
        assert_eq!(
            compact_parts(1_250_000),
            ("1.3".to_owned(), Scale::Millions)
        );
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
}
