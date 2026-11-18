//! Spanish: the locale code, system matching, the kit's fallback and the formats.

use chrono::NaiveDate;
use gpui_kit::component as kit;
use tf2tools_app::i18n::{self, apply, current, format, system_language};

use crate::LocaleGuard;

#[test]
fn every_spanish_tag_maps_to_spanish() {
    for tag in [
        "es",
        "es-ES",
        "es_ES.UTF-8",
        "es_MX",
        "es-MX",
        "es_419",
        "es-419",
        "ES_es@euro",
        "es_AR.UTF-8",
    ] {
        assert_eq!(system_language([tag]), "es", "{tag}");
    }
    assert_eq!(system_language(["de-DE", "es_MX.UTF-8"]), "es");
    assert_eq!(system_language(["ru-RU", "es-ES"]), "ru");
    assert_eq!(system_language(["pt_BR.UTF-8", "es-ES"]), "pt-BR");
    assert_eq!(system_language(["es-ES", "pt_BR.UTF-8"]), "es");
    assert_eq!(system_language(["esx"]), "en");
}

#[test]
fn spanish_is_applied_as_given() {
    let _guard = LocaleGuard::hold();
    assert_eq!(apply("es"), "es");
    assert_eq!(current(), "es");
    assert_eq!(&*rust_i18n::locale(), "es");
}

// The kit has no Spanish catalogue, so its own text falls back to English while the app's is Spanish.
#[test]
fn the_kit_falls_back_to_english_in_spanish() {
    let _guard = LocaleGuard::switch("es");
    assert_eq!(&*kit::locale(), "es");
    assert_eq!(
        kit::_rust_i18n_translate(&kit::locale(), "Attachment.Remove"),
        "Remove"
    );
    assert_eq!(
        tf2tools_app::_rust_i18n_try_translate("es", "settings.language.title").as_deref(),
        Some("Idioma")
    );
}

#[test]
fn numbers_sizes_and_dates_take_the_spanish_forms() {
    let _guard = LocaleGuard::switch("es");
    assert_eq!(format::decimal_point("0.6"), "0,6");
    assert_eq!(format::grouped(999), "999");
    assert_eq!(format::grouped(4_812), "4.812");
    assert_eq!(format::grouped(1_234_567), "1.234.567");
    assert_eq!(format::compact_count(1_250), "1,3\u{a0}mil");
    assert_eq!(format::compact_count(1_240_000), "1,2\u{a0}M");
    assert_eq!(format::bytes(512), "512\u{a0}B");
    assert_eq!(format::bytes(1_536), "1,5\u{a0}KB");
    let d = NaiveDate::from_ymd_opt(2026, 3, 5).unwrap();
    assert_eq!(format::numeric_date(d), "05/03/2026");
    assert_eq!(format::day_month_year(d), "5 mar 2026");
    assert_eq!(format::month_day_year(d), "5 mar 2026");
    let sept = NaiveDate::from_ymd_opt(2026, 9, 5).unwrap();
    assert_eq!(format::day_month_year(sept), "5 sept 2026");
    assert_eq!(format::time_left(160), "2\u{a0}h 40\u{a0}min");
}

#[test]
fn spanish_counts_take_the_plural_from_zero() {
    let _guard = LocaleGuard::switch("es");
    assert_eq!(i18n::plural("common.time.days_ago", 1), "hace 1 día");
    assert_eq!(i18n::plural("common.time.days_ago", 0), "hace 0 días");
    assert_eq!(i18n::plural("common.time.days_ago", 3), "hace 3 días");
}

#[test]
fn spanish_says_a_damaged_download_one_way() {
    let text = |k: &str| tf2tools_app::_rust_i18n_try_translate("es", k).map(|t| t.into_owned());
    let store = text("messages.store.download.damaged");
    assert_eq!(store, text("messages.store.gb_fetch.damaged"));
    assert_ne!(
        store,
        tf2tools_app::_rust_i18n_try_translate("en", "messages.store.download.damaged")
            .map(|t| t.into_owned())
    );
}
