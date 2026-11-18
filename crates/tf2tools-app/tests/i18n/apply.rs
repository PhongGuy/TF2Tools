use gpui_kit::component as kit;
use tf2tools_app::i18n::{apply, current, system_language};

use crate::LocaleGuard;

#[test]
fn an_unknown_code_falls_back_to_english() {
    let _guard = LocaleGuard::switch("ru");
    assert_eq!(apply("xx"), "en");
    assert_eq!(current(), "en");
    assert_eq!(&*rust_i18n::locale(), "en");
}

#[test]
fn a_shipped_code_is_applied_as_given() {
    let _guard = LocaleGuard::hold();
    assert_eq!(apply("ru"), "ru");
    assert_eq!(current(), "ru");
}

#[test]
fn the_kit_and_the_app_share_one_locale() {
    let _guard = LocaleGuard::switch("ru");
    assert_eq!(&*kit::locale(), "ru");
    assert_eq!(
        kit::_rust_i18n_translate(&kit::locale(), "Attachment.Remove"),
        "Удалить"
    );
    assert_eq!(
        tf2tools_app::_rust_i18n_try_translate(&rust_i18n::locale(), "settings.language.title")
            .as_deref(),
        Some("Язык")
    );
    kit::set_locale("en");
    assert_eq!(current(), "en");
}

#[test]
fn the_guard_puts_english_back_even_after_a_panic() {
    let failed = std::panic::catch_unwind(|| {
        let _guard = LocaleGuard::switch("ru");
        assert_eq!(current(), "ru");
        panic!("a locale test failed");
    });
    assert!(failed.is_err());
    // The panic poisoned the lock; the next holder must still get it, in English.
    let _guard = LocaleGuard::hold();
    assert_eq!(current(), "en");
}

#[test]
fn a_region_or_encoding_tag_maps_to_its_language() {
    for tag in [
        "ru",
        "ru-RU",
        "ru_RU.UTF-8",
        "RU_ru",
        "ru_RU@euro",
        "ru-Cyrl-RU",
    ] {
        assert_eq!(system_language([tag]), "ru", "{tag}");
    }
    assert_eq!(system_language(["en-GB"]), "en");
}

#[test]
fn the_first_preferred_locale_with_a_catalogue_wins() {
    assert_eq!(system_language(["it-IT", "ru-RU"]), "ru");
    assert_eq!(system_language(["en-US", "ru-RU"]), "en");
}

#[test]
fn a_system_without_a_catalogue_starts_in_english() {
    assert_eq!(system_language(["it-IT", "nl_NL.UTF-8"]), "en");
    assert_eq!(system_language(Vec::<String>::new()), "en");
    assert_eq!(system_language(["C", "POSIX", ""]), "en");
}

#[test]
fn placeholder_locales_do_not_stop_the_walk() {
    assert_eq!(system_language(["C", "", "ru_RU.UTF-8"]), "ru");
}
