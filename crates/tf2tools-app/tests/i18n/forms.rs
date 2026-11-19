//! Forms each language writes its own way: the percent sign and the Escape keycap.

use gpui_kit::{px, size};
use tf2tools_app::i18n::format;
use tf2tools_app::pages::crosshair_maker::model::Viewport;
use tf2tools_app::pages::hud_maker::view::Zoom;

use crate::LocaleGuard;

// CLDR's percent pattern per language; the spaced ones use a no-break space so 100 % never wraps.
const PERCENTS: [(&str, &str); 6] = [
    ("en", "100%"),
    ("pt-BR", "100%"),
    ("fr", "100\u{a0}%"),
    ("de", "100\u{a0}%"),
    ("ru", "100\u{a0}%"),
    ("es", "100\u{a0}%"),
];

#[test]
fn every_language_writes_its_own_percent() {
    for (lang, want) in PERCENTS {
        let _guard = LocaleGuard::switch(lang);
        assert_eq!(format::percent(100), want, "{lang}");
    }
}

#[test]
fn the_maker_zooms_read_in_the_language_percent() {
    for (lang, want) in PERCENTS {
        let _guard = LocaleGuard::switch(lang);
        assert_eq!(Zoom::FIT.label(), want, "{lang} HUD Maker");
        let crosshair = Viewport::new(size(px(512.), px(448.)), 64);
        assert_eq!(crosshair.label(), want, "{lang} Crosshair Maker");
    }
}

// The lines that show a percentage take it whole from `format::percent`, sign and all.
#[test]
fn the_percent_lines_leave_the_sign_to_the_percent_format() {
    for lang in PERCENTS.map(|(lang, _)| lang) {
        for key in [
            "settings.audio.volume",
            "updater.status.downloading",
            "crosshair_maker.opacity_value",
        ] {
            let text = tf2tools_app::_rust_i18n_try_translate(lang, key).unwrap_or_default();
            assert!(text.ends_with("%{percent}"), "{lang} {key}: {text:?}");
        }
    }
}

// Keyboards print the key's short name: "Esc" almost everywhere, "Échap" on French ones.
#[test]
fn the_shortcuts_sheet_names_escape_as_each_keyboard_prints_it() {
    for (lang, want) in [
        ("en", "Esc"),
        ("de", "Esc"),
        ("es", "Esc"),
        ("ru", "Esc"),
        ("pt-BR", "Esc"),
        ("fr", "Échap"),
    ] {
        let _guard = LocaleGuard::switch(lang);
        let keys: Vec<String> = tf2tools_app::pages::hud_maker::shortcuts()
            .iter()
            .map(|(keys, _)| keys.to_string())
            .collect();
        assert!(keys.iter().any(|k| k == want), "{lang}: {keys:?}");
        assert!(!keys.iter().any(|k| k == "Escape"), "{lang}: {keys:?}");
    }
}

// With no full log kept, no language counts zero logs or sizes their empty table.
#[test]
fn every_language_words_an_empty_logs_cache_as_nothing_cached() {
    use tf2tools_app::pages::settings::{size_text, usage_text};
    for lang in PERCENTS.map(|(lang, _)| lang) {
        let _guard = LocaleGuard::switch(lang);
        let (line, empty) = (usage_text(40_960, 0, 4_096), size_text(4_096));
        assert!(line.contains(&*size_text(40_960)), "{lang}: {line}");
        assert!(!line.contains(&*empty), "{lang}: {line}");
        assert_ne!(line, usage_text(40_960, 1, 4_096), "{lang}");
    }
}
