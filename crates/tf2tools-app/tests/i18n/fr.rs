//! French: the locale code, system matching, the kit's fallback, the formats and the file's escapes.

use chrono::NaiveDate;
use gpui_kit::component as kit;
use tf2tools_app::i18n::{self, apply, current, format, system_language};

use crate::LocaleGuard;

#[test]
fn every_french_tag_maps_to_french() {
    for tag in [
        "fr",
        "fr-FR",
        "fr_FR.UTF-8",
        "fr_CA.UTF-8",
        "fr-CA",
        "fr-BE",
        "fr_BE",
        "fr_CH",
        "fr-CH",
        "fr_LU.UTF-8",
        "FR_fr@euro",
    ] {
        assert_eq!(system_language([tag]), "fr", "{tag}");
    }
    assert_eq!(system_language(["it-IT", "fr_CA.UTF-8"]), "fr");
    assert_eq!(system_language(["fr-FR", "de_AT.UTF-8"]), "fr");
    assert_eq!(system_language(["de-DE", "fr-FR"]), "de");
    assert_eq!(system_language(["es-ES", "fr-FR"]), "es");
    assert_eq!(system_language(["ru-RU", "fr-FR"]), "ru");
    assert_eq!(system_language(["pt_BR.UTF-8", "fr-FR"]), "pt-BR");
    assert_eq!(system_language(["frr"]), "en");
}

#[test]
fn french_is_applied_as_given() {
    let _guard = LocaleGuard::hold();
    assert_eq!(apply("fr"), "fr");
    assert_eq!(current(), "fr");
    assert_eq!(&*rust_i18n::locale(), "fr");
}

// The kit has no French catalogue, so its own text falls back to English while the app's is French.
#[test]
fn the_kit_falls_back_to_english_in_french() {
    let _guard = LocaleGuard::switch("fr");
    assert_eq!(&*kit::locale(), "fr");
    assert_eq!(
        kit::_rust_i18n_translate(&kit::locale(), "Attachment.Remove"),
        "Remove"
    );
    assert_eq!(
        tf2tools_app::_rust_i18n_try_translate("fr", "settings.language.title").as_deref(),
        Some("Langue")
    );
}

#[test]
fn numbers_sizes_and_dates_take_the_french_forms() {
    let _guard = LocaleGuard::switch("fr");
    assert_eq!(format::decimal_point("0.6"), "0,6");
    assert_eq!(format::grouped(999), "999");
    assert_eq!(format::grouped(4_812), "4\u{202f}812");
    assert_eq!(format::grouped(1_234_567), "1\u{202f}234\u{202f}567");
    assert_eq!(format::compact_count(1_250), "1,3\u{a0}k");
    assert_eq!(format::compact_count(1_240_000), "1,2\u{a0}M");
    assert_eq!(format::bytes(512), "512\u{a0}o");
    assert_eq!(format::bytes(1_536), "1,5\u{a0}Ko");
    let unit = |k: &str| tf2tools_app::_rust_i18n_try_translate("fr", k).unwrap();
    assert_eq!(unit("common.size.mb"), "%{size}\u{a0}Mo");
    assert_eq!(unit("common.size.gb"), "%{size}\u{a0}Go");
    assert_eq!(unit("common.size.tb"), "%{size}\u{a0}To");
    let d = NaiveDate::from_ymd_opt(2026, 3, 5).unwrap();
    assert_eq!(format::numeric_date(d), "05/03/2026");
    assert_eq!(format::day_month_year(d), "5 mars 2026");
    assert_eq!(format::month_day_year(d), "5 mars 2026");
    let month = |m| format::day_month_year(NaiveDate::from_ymd_opt(2026, m, 5).unwrap());
    assert_eq!(month(2), "5 févr. 2026");
    assert_eq!(month(6), "5 juin 2026");
    assert_eq!(month(7), "5 juil. 2026");
    assert_eq!(month(8), "5 août 2026");
    assert_eq!(month(9), "5 sept. 2026");
    assert_eq!(month(12), "5 déc. 2026");
    assert_eq!(format::time_left(160), "2\u{a0}h 40\u{a0}min");
    assert_eq!(format::time_left(40), "40\u{a0}min");
    assert_eq!(format::hours_minutes(160), "2\u{a0}h 40\u{a0}min");
    assert_eq!(format::seconds(58), "58\u{a0}s");
}

#[test]
fn french_counts_take_one_at_zero() {
    let _guard = LocaleGuard::switch("fr");
    assert_eq!(i18n::plural("common.time.days_ago", 0), "il y a 0 jour");
    assert_eq!(i18n::plural("common.time.days_ago", 1), "il y a 1 jour");
    assert_eq!(i18n::plural("common.time.days_ago", 3), "il y a 3 jours");
}

// Reviewers must see every no-break space, so fr.yml writes them as escapes, never as raw bytes.
#[test]
fn fr_yml_writes_no_break_spaces_as_escapes() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/locales/fr.yml");
    let text = std::fs::read_to_string(path).unwrap();
    for (n, line) in text.lines().enumerate() {
        assert!(
            !line.contains(['\u{a0}', '\u{202f}']),
            "fr.yml:{}: write \\u00A0 or \\u202F instead",
            n + 1
        );
    }
    assert!(text.contains("\\u202F"), "the group separator is an escape");
}

// A damaged download reads the same whichever store fetched it.
#[test]
fn french_says_a_damaged_download_one_way() {
    let text = |k: &str| tf2tools_app::_rust_i18n_try_translate("fr", k).map(|t| t.into_owned());
    let store = text("messages.store.download.damaged");
    assert_eq!(store, text("messages.store.gb_fetch.damaged"));
    assert_ne!(
        store,
        tf2tools_app::_rust_i18n_try_translate("en", "messages.store.download.damaged")
            .map(|t| t.into_owned())
    );
}

// The HUD Maker's timeline names game events as the French client does.
#[test]
fn the_hud_maker_timeline_uses_the_client_words() {
    let text = |k: &str| {
        tf2tools_app::_rust_i18n_try_translate("fr", k)
            .map(|t| t.into_owned())
            .unwrap_or_default()
    };
    let charged = text("hud_maker.timeline.event.medic_charged");
    assert!(charged.contains("ÜberCharge"), "{charged}");
    assert_ne!(charged, "ÜberCharge ready");
    assert_ne!(text("hud_maker.history.save"), "Save snapshot");
}

// The HUD Maker's meters and situations name weapons and modes as the French client does.
#[test]
fn the_hud_maker_meters_use_the_client_names() {
    let text = |k: &str| {
        tf2tools_app::_rust_i18n_try_translate("fr", k)
            .map(|t| t.into_owned())
            .unwrap_or_default()
    };
    assert!(text("hud_maker.meter.drink").contains("Lait frelaté"));
    assert!(text("hud_maker.situation.switch.koth").contains("roi de la colline"));
}

// The HUD Maker's part names read in French, the power-ups as the client names them.
#[test]
fn the_hud_maker_names_its_parts_in_french() {
    let text = |k: &str| {
        tf2tools_app::_rust_i18n_try_translate("fr", format!("hud_maker.part_name.{k}"))
            .map(|t| t.into_owned())
            .unwrap_or_default()
    };
    assert_ne!(text("health_value"), "Health value");
    assert_ne!(text("kill_streak_icon"), "Killstreak icon");
    assert!(text("rune_haste").contains("Rapidité"));
}

#[test]
fn the_updater_says_a_damaged_download_as_the_stores_do() {
    assert_eq!(
        tf2tools_app::_rust_i18n_try_translate("fr", "updater.failure.damaged"),
        tf2tools_app::_rust_i18n_try_translate("fr", "messages.store.download.damaged")
    );
}

// Names the client spells with glued punctuation; the scan reads past them.
const PUNCTUATED_NAMES: &[&str] = &["Bonk!"];

// Each place a value lacks the U+00A0 French wants before : ; ! ? » and a literal %, or after «.
// Clock times, URLs and key caps such as "(?)" are not prose and pass.
fn typography_slips(text: &str) -> Vec<String> {
    let mut text = text.to_owned();
    for name in PUNCTUATED_NAMES {
        text = text.replace(name, "");
    }
    let chars: Vec<char> = text.chars().collect();
    let digit = |c: Option<char>| c.is_some_and(|c| c.is_ascii_digit());
    let mut slips = Vec::new();
    for (i, &c) in chars.iter().enumerate() {
        let prev = i.checked_sub(1).map(|j| chars[j]);
        let next = chars.get(i + 1).copied();
        let wants_space_before = match c {
            ':' => !(digit(prev) && digit(next)) && next != Some('/'),
            ';' | '!' | '?' => !matches!(prev, Some('(' | '!' | '?')),
            '%' => next != Some('{'),
            '»' => true,
            _ => false,
        };
        if wants_space_before && prev != Some('\u{a0}') {
            slips.push(format!("no U+00A0 before {c:?} at {i}"));
        }
        if c == '«' && next != Some('\u{a0}') {
            slips.push(format!("no U+00A0 after '«' at {i}"));
        }
    }
    slips
}

#[test]
fn the_typography_scan_flags_only_missing_no_break_spaces() {
    for ok in [
        "Volume\u{a0}: %{percent}\u{a0}%",
        "Supprimer le HUD %{name}\u{a0}?",
        "\u{a0}: scripts trouvés",
        "«\u{a0}HUD\u{a0}»",
        "Raccourcis clavier (?)",
        "Vraiment\u{a0}?!",
        "à 12:30",
        "https://logs.tf",
        "Bonk!, Crit-a-Cola et Lait frelaté",
        "%{name}",
    ] {
        assert!(
            typography_slips(ok).is_empty(),
            "{ok:?}: {:?}",
            typography_slips(ok)
        );
    }
    for bad in [
        "Volume : 50",
        "Volume: 50",
        "%{percent}%",
        "%{percent} %",
        "Prêt !",
        "Prêt!",
        "Pourquoi ;",
        ": scripts trouvés",
        "« HUD\u{a0}»",
        "«\u{a0}HUD »",
    ] {
        assert!(!typography_slips(bad).is_empty(), "{bad:?}");
    }
}

#[test]
fn french_punctuation_takes_a_no_break_space() {
    let messages = tf2tools_app::_rust_i18n_backend()
        .messages_for_locale("fr")
        .unwrap_or_default();
    assert!(!messages.is_empty(), "fr.yml is missing or empty");
    let slips: Vec<String> = messages
        .iter()
        .filter_map(|(key, text)| {
            let found = typography_slips(text);
            (!found.is_empty()).then(|| format!("{key}: {found:?}"))
        })
        .collect();
    assert!(slips.is_empty(), "fr typography: {slips:#?}");
}
