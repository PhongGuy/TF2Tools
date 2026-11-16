//! Dates, numbers, sizes, durations and counted sentences in Russian.

use tf2tools_app::i18n::{counted_plural, format};

use crate::LocaleGuard;
use crate::messages::assert_russian;

/// Russian text with no page-surface key showing either.
pub fn assert_ru(text: &str) {
    assert_russian(text);
    for prefix in [
        "maps.",
        "logs_tf.",
        "profiles.",
        "crosshairs_page.",
        "hud_maker.",
        "hud_store.",
        "addon_store.",
        "store_grid.",
        "confirm.",
        "font_check.",
    ] {
        assert!(!text.contains(prefix), "a raw key: {text:?}");
    }
}

#[test]
fn numbers_and_sizes_take_the_russian_separators() {
    let _guard = LocaleGuard::switch("ru");
    assert_eq!(format::decimal_point("0.6"), "0,6");
    assert_eq!(format::decimal_point("136"), "136");
    assert_eq!(format::grouped(999), "999");
    assert_eq!(format::grouped(4_812), "4\u{a0}812");
    assert_eq!(format::grouped(1_234_567), "1\u{a0}234\u{a0}567");
    assert_eq!(format::compact_count(999), "999");
    assert_eq!(format::compact_count(1_250), "1,3\u{a0}тыс.");
    assert_eq!(format::compact_count(62_500), "62,5\u{a0}тыс.");
    assert_eq!(format::compact_count(1_240_000), "1,2\u{a0}млн");
    assert_eq!(format::bytes(512), "512\u{a0}Б");
    assert_eq!(format::bytes(1_048_524), "1023,9\u{a0}КБ");
    assert_eq!(format::bytes(12_345_678), "11,8\u{a0}МБ");
    assert_eq!(format::bytes(2_100_000_000), "2,0\u{a0}ГБ");
    assert_eq!(format::bytes(3 << 40), "3,0\u{a0}ТБ");
}

#[test]
fn a_russian_size_is_joined_to_its_unit_by_a_no_break_space() {
    let _guard = LocaleGuard::switch("ru");
    assert_eq!(
        format::bytes(1_536).as_bytes(),
        b"1,5\xc2\xa0\xd0\x9a\xd0\x91"
    );
}

#[test]
fn counted_plurals_take_the_formatted_count_and_other_variables() {
    let _guard = LocaleGuard::switch("ru");
    let text = counted_plural(
        "messages.local.copy_updated.no_longer_fit",
        21,
        "21",
        &[("list", "a и b")],
    );
    assert_eq!(text, "21 изменение больше не подходит: a и b.");
    let text = counted_plural("settings.sync.items", 1_400, "1\u{a0}400", &[]);
    assert_eq!(text, "1\u{a0}400 элементов в кэше");
    assert_ru(&text);
}
