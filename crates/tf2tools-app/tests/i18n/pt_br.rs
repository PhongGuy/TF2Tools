//! Brazilian Portuguese: the locale code, system matching, the kit's own text and the formats.

use chrono::NaiveDate;
use gpui_kit::component as kit;
use tf2tools_app::i18n::{self, apply, current, format, system_language};

use crate::LocaleGuard;

#[test]
fn a_portuguese_tag_maps_to_brazilian_portuguese() {
    for tag in [
        "pt",
        "pt-BR",
        "pt_BR",
        "pt_BR.UTF-8",
        "PT_br",
        "pt_BR@latin",
        "pt-PT",
        "pt_PT.UTF-8",
    ] {
        assert_eq!(system_language([tag]), "pt-BR", "{tag}");
    }
    assert_eq!(system_language(["de-DE", "pt_BR.UTF-8"]), "pt-BR");
    assert_eq!(system_language(["ru-RU", "pt-BR"]), "ru");
    assert_eq!(system_language(["pt-BR", "ru-RU"]), "pt-BR");
    assert_eq!(system_language(["ptx"]), "en");
}

#[test]
fn brazilian_portuguese_is_applied_as_given() {
    let _guard = LocaleGuard::hold();
    assert_eq!(apply("pt-BR"), "pt-BR");
    assert_eq!(current(), "pt-BR");
    assert_eq!(&*rust_i18n::locale(), "pt-BR");
}

#[test]
fn the_kit_speaks_brazilian_portuguese() {
    let _guard = LocaleGuard::switch("pt-BR");
    assert_eq!(&*kit::locale(), "pt-BR");
    assert_eq!(
        kit::_rust_i18n_translate(&kit::locale(), "Attachment.Remove"),
        "Remover"
    );
    assert_eq!(
        tf2tools_app::_rust_i18n_try_translate("pt-BR", "settings.language.title").as_deref(),
        Some("Idioma")
    );
}

#[test]
fn numbers_sizes_and_dates_take_the_brazilian_forms() {
    let _guard = LocaleGuard::switch("pt-BR");
    assert_eq!(format::decimal_point("0.6"), "0,6");
    assert_eq!(format::grouped(999), "999");
    assert_eq!(format::grouped(4_812), "4.812");
    assert_eq!(format::grouped(1_234_567), "1.234.567");
    assert_eq!(format::compact_count(1_250), "1,3\u{a0}mil");
    assert_eq!(format::compact_count(1_240_000), "1,2\u{a0}mi");
    assert_eq!(format::bytes(512), "512\u{a0}B");
    assert_eq!(format::bytes(1_536), "1,5\u{a0}KB");
    let d = NaiveDate::from_ymd_opt(2026, 3, 5).unwrap();
    assert_eq!(format::numeric_date(d), "05/03/2026");
    assert_eq!(format::day_month_year(d), "5 de mar. de 2026");
    assert_eq!(format::month_day_year(d), "5 de mar. de 2026");
    assert_eq!(format::time_left(160), "2\u{a0}h 40\u{a0}min");
}

#[test]
fn brazilian_counts_take_the_plural_from_zero() {
    let _guard = LocaleGuard::switch("pt-BR");
    assert_eq!(i18n::plural("common.time.days_ago", 1), "há 1 dia");
    assert_eq!(i18n::plural("common.time.days_ago", 0), "há 0 dias");
    assert_eq!(i18n::plural("common.time.days_ago", 3), "há 3 dias");
}
