//! German: the locale code, system matching, the kit's fallback and the formats.

use chrono::NaiveDate;
use gpui_kit::component as kit;
use tf2tools_app::i18n::{self, apply, current, format, system_language};

use crate::LocaleGuard;

#[test]
fn every_german_tag_maps_to_german() {
    for tag in [
        "de",
        "de-DE",
        "de_DE.UTF-8",
        "de_AT",
        "de-AT",
        "de_CH.UTF-8",
        "de-LI",
        "DE_de@euro",
    ] {
        assert_eq!(system_language([tag]), "de", "{tag}");
    }
    assert_eq!(system_language(["fr-FR", "de_AT.UTF-8"]), "de");
    assert_eq!(system_language(["es-ES", "de-DE"]), "es");
    assert_eq!(system_language(["de-DE", "es-ES"]), "de");
    assert_eq!(system_language(["ru-RU", "de-DE"]), "ru");
    assert_eq!(system_language(["dex"]), "en");
}

#[test]
fn german_is_applied_as_given() {
    let _guard = LocaleGuard::hold();
    assert_eq!(apply("de"), "de");
    assert_eq!(current(), "de");
    assert_eq!(&*rust_i18n::locale(), "de");
}

// The kit has no German catalogue, so its own text falls back to English while the app's is German.
#[test]
fn the_kit_falls_back_to_english_in_german() {
    let _guard = LocaleGuard::switch("de");
    assert_eq!(&*kit::locale(), "de");
    assert_eq!(
        kit::_rust_i18n_translate(&kit::locale(), "Attachment.Remove"),
        "Remove"
    );
    assert_eq!(
        tf2tools_app::_rust_i18n_try_translate("de", "settings.language.title").as_deref(),
        Some("Sprache")
    );
}

#[test]
fn numbers_sizes_and_dates_take_the_german_forms() {
    let _guard = LocaleGuard::switch("de");
    assert_eq!(format::decimal_point("0.6"), "0,6");
    assert_eq!(format::grouped(999), "999");
    assert_eq!(format::grouped(4_812), "4.812");
    assert_eq!(format::grouped(1_234_567), "1.234.567");
    assert_eq!(format::compact_count(1_250), "1,3\u{a0}Tsd.");
    assert_eq!(format::compact_count(1_240_000), "1,2\u{a0}Mio.");
    assert_eq!(format::bytes(512), "512\u{a0}B");
    assert_eq!(format::bytes(1_536), "1,5\u{a0}KB");
    let d = NaiveDate::from_ymd_opt(2026, 3, 5).unwrap();
    assert_eq!(format::numeric_date(d), "05.03.2026");
    assert_eq!(format::day_month_year(d), "5. März 2026");
    assert_eq!(format::month_day_year(d), "5. März 2026");
    let month = |m| format::day_month_year(NaiveDate::from_ymd_opt(2026, m, 5).unwrap());
    assert_eq!(month(6), "5. Juni 2026");
    assert_eq!(month(7), "5. Juli 2026");
    assert_eq!(month(9), "5. Sept. 2026");
    assert_eq!(month(12), "5. Dez. 2026");
    assert_eq!(format::time_left(160), "2\u{a0}Std. 40\u{a0}Min.");
    assert_eq!(format::seconds(58), "58\u{a0}Sek.");
}

#[test]
fn german_counts_take_the_plural_from_zero() {
    let _guard = LocaleGuard::switch("de");
    assert_eq!(i18n::plural("common.time.days_ago", 1), "vor 1 Tag");
    assert_eq!(i18n::plural("common.time.days_ago", 0), "vor 0 Tagen");
    assert_eq!(i18n::plural("common.time.days_ago", 3), "vor 3 Tagen");
}

#[test]
fn german_says_a_damaged_download_one_way() {
    let text = |k: &str| tf2tools_app::_rust_i18n_try_translate("de", k).map(|t| t.into_owned());
    let store = text("messages.store.download.damaged");
    assert_eq!(store, text("messages.store.gb_fetch.damaged"));
    assert_ne!(
        store,
        tf2tools_app::_rust_i18n_try_translate("en", "messages.store.download.damaged")
            .map(|t| t.into_owned())
    );
}

#[test]
fn the_hud_maker_timeline_uses_the_client_words() {
    let text = |k: &str| {
        tf2tools_app::_rust_i18n_try_translate("de", k)
            .map(|t| t.into_owned())
            .unwrap_or_default()
    };
    assert!(text("hud_maker.timeline.event.medic_charged").contains("Überladung"));
    assert_ne!(text("hud_maker.history.save"), "Save snapshot");
}

#[test]
fn the_hud_maker_meters_use_the_client_names() {
    let text = |k: &str| {
        tf2tools_app::_rust_i18n_try_translate("de", k)
            .map(|t| t.into_owned())
            .unwrap_or_default()
    };
    assert!(text("hud_maker.meter.drink").contains("Krit-a-Cola"));
    assert!(text("hud_maker.situation.switch.koth").contains("Karte"));
}

#[test]
fn the_hud_maker_names_its_parts_in_german() {
    let text = |k: &str| {
        tf2tools_app::_rust_i18n_try_translate("de", format!("hud_maker.part_name.{k}"))
            .map(|t| t.into_owned())
            .unwrap_or_default()
    };
    assert_ne!(text("health_value"), "Health value");
    assert!(text("kill_streak_icon").contains("Killserie"));
    assert_ne!(text("rune_haste"), "Rune haste");
}
