//! Simplified Chinese: the locale code, Traditional skipping, the kit's own text, the formats and
//! the one plural form.

use chrono::NaiveDate;
use gpui_kit::component as kit;
use tf2tools_app::i18n::{self, apply, current, format, system_language};

use crate::LocaleGuard;

#[test]
fn every_simplified_chinese_tag_maps_to_chinese() {
    for tag in [
        "zh",
        "zh-CN",
        "zh_CN",
        "zh_CN.UTF-8",
        "ZH_cn.utf8",
        "zh-Hans",
        "zh-Hans-CN",
        "zh_SG",
        "zh-Hans-SG",
        "zh-Hans-HK",
    ] {
        assert_eq!(system_language([tag]), "zh-CN", "{tag}");
    }
    assert_eq!(system_language(["it-IT", "zh_CN.UTF-8"]), "zh-CN");
    assert_eq!(system_language(["zh-Hant-TW", "zh-Hans-CN"]), "zh-CN");
    assert_eq!(system_language(["de-DE", "zh-CN"]), "de");
}

#[test]
fn traditional_chinese_tags_are_skipped() {
    assert_eq!(system_language(["zh_TW.UTF-8", "en_US.UTF-8"]), "en");
    assert_eq!(system_language(["zh-Hant-HK"]), "en");
    assert_eq!(system_language(["zh_HK"]), "en");
    assert_eq!(system_language(["zh_MO", "de_DE"]), "de");
    for tag in [
        "zh-TW",
        "zh_TW",
        "zh-Hant",
        "zh-Hant-TW",
        "zh-Hant-CN",
        "zh_HK.UTF-8",
        "zh-MO",
    ] {
        assert_eq!(system_language([tag]), "en", "{tag}");
    }
}

#[test]
fn other_languages_match_as_before() {
    assert_eq!(system_language(["ru_RU.UTF-8"]), "ru");
    assert_eq!(system_language(["pt_BR.UTF-8"]), "pt-BR");
    assert_eq!(system_language(["es_MX.UTF-8"]), "es");
    assert_eq!(system_language(["de_AT.UTF-8"]), "de");
    assert_eq!(system_language(["fr_CA.UTF-8"]), "fr");
    assert_eq!(system_language(["zh_TW.UTF-8", "fr_FR.UTF-8"]), "fr");
}

#[test]
fn chinese_is_applied_as_given() {
    let _guard = LocaleGuard::hold();
    assert_eq!(apply("zh-CN"), "zh-CN");
    assert_eq!(current(), "zh-CN");
    assert_eq!(&*rust_i18n::locale(), "zh-CN");
}

// The kit ships zh-CN, so its own text is Chinese beside the app's.
#[test]
fn the_kit_speaks_chinese_too() {
    let _guard = LocaleGuard::switch("zh-CN");
    assert_eq!(&*kit::locale(), "zh-CN");
    assert_eq!(
        kit::_rust_i18n_translate(&kit::locale(), "Attachment.Remove"),
        "移除"
    );
    assert_eq!(
        tf2tools_app::_rust_i18n_try_translate("zh-CN", "settings.language.title").as_deref(),
        Some("语言")
    );
}

#[test]
fn numbers_sizes_dates_and_durations_take_the_chinese_forms() {
    let _guard = LocaleGuard::switch("zh-CN");
    assert_eq!(format::decimal_point("0.6"), "0.6");
    assert_eq!(format::grouped(4_812), "4,812");
    assert_eq!(format::grouped(1_234_567), "1,234,567");
    assert_eq!(format::compact_count(999), "999");
    assert_eq!(format::compact_count(9_999), "9,999");
    assert_eq!(format::compact_count(12_500), "1.3万");
    assert_eq!(format::compact_count(1_240_000), "124万");
    assert_eq!(format::compact_count(9_999_500), "1000万");
    assert_eq!(format::compact_count(120_000_000), "1.2亿");
    assert_eq!(format::bytes(512), "512 B");
    assert_eq!(format::bytes(1_536), "1.5 KB");
    let d = NaiveDate::from_ymd_opt(2026, 3, 5).unwrap();
    assert_eq!(format::numeric_date(d), "2026-03-05");
    assert_eq!(format::day_month_year(d), "2026年3月5日");
    assert_eq!(format::month_day_year(d), "2026年3月5日");
    let dec = NaiveDate::from_ymd_opt(2026, 12, 5).unwrap();
    assert_eq!(format::day_month_year(dec), "2026年12月5日");
    assert_eq!(format::time_left(160), "2小时40分钟");
    assert_eq!(format::time_left(120), "2小时");
    assert_eq!(format::time_left(40), "40分钟");
    assert_eq!(format::hours_minutes(90), "1小时30分");
    assert_eq!(format::hours_minutes(45), "45分钟");
    assert_eq!(format::seconds(58), "58秒");
    assert_eq!(format::tenths("6.5"), "6.5秒");
}

// The ten-thousand step is Chinese only: English still compacts by thousands.
#[test]
fn english_counts_still_compact_by_thousands() {
    let _guard = LocaleGuard::switch("en");
    assert_eq!(format::compact_count(999), "999");
    assert_eq!(format::compact_count(9_999), "10.0K");
    assert_eq!(format::compact_count(12_500), "12.5K");
    assert_eq!(format::compact_count(1_240_000), "1.2M");
    assert_eq!(format::compact_count(9_999_500), "10.0M");
}

#[test]
fn chinese_counts_read_one_way() {
    let _guard = LocaleGuard::switch("zh-CN");
    assert_eq!(i18n::plural("common.time.days_ago", 0), "0 天前");
    assert_eq!(i18n::plural("common.time.days_ago", 1), "1 天前");
    assert_eq!(i18n::plural("common.time.days_ago", 3), "3 天前");
}

#[test]
fn the_classes_take_the_client_names() {
    let text = |k: &str| tf2tools_app::_rust_i18n_try_translate("zh-CN", k).map(|t| t.into_owned());
    assert_eq!(text("common.class.scout").as_deref(), Some("侦察兵"));
    assert_eq!(text("common.class.heavy").as_deref(), Some("机枪手"));
    assert_eq!(text("common.class.engineer").as_deref(), Some("工程师"));
}

#[test]
fn the_stores_use_the_glossary_words() {
    let text = |k: &str| tf2tools_app::_rust_i18n_try_translate("zh-CN", k).map(|t| t.into_owned());
    assert_eq!(text("addon_store.button.refresh").as_deref(), Some("刷新"));
    assert_eq!(text("addon_store.offer.install").as_deref(), Some("安装"));
    assert_eq!(
        text("addon_store.badge.installed").as_deref(),
        Some("已安装")
    );
}

#[test]
fn chinese_says_a_damaged_download_one_way() {
    let text = |k: &str| tf2tools_app::_rust_i18n_try_translate("zh-CN", k).map(|t| t.into_owned());
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
        tf2tools_app::_rust_i18n_try_translate("zh-CN", k)
            .map(|t| t.into_owned())
            .unwrap_or_default()
    };
    let charged = text("hud_maker.timeline.event.medic_charged");
    assert!(charged.contains("UberCharge"), "{charged}");
    assert_ne!(charged, "ÜberCharge ready");
    assert_ne!(text("hud_maker.history.save"), "Save snapshot");
}

#[test]
fn the_hud_maker_meters_use_the_client_names() {
    let text = |k: &str| {
        tf2tools_app::_rust_i18n_try_translate("zh-CN", k)
            .map(|t| t.into_owned())
            .unwrap_or_default()
    };
    let drink = text("hud_maker.meter.drink");
    assert!(
        drink.contains("爆击可乐") && drink.contains("疯狂的牛奶"),
        "{drink}"
    );
    assert!(text("hud_maker.situation.switch.koth").contains("山丘之王"));
}

#[test]
fn the_hud_maker_names_its_parts_in_chinese() {
    let text = |k: &str| {
        tf2tools_app::_rust_i18n_try_translate("zh-CN", format!("hud_maker.part_name.{k}"))
            .map(|t| t.into_owned())
            .unwrap_or_default()
    };
    assert_ne!(text("health_value"), "Health value");
    assert_ne!(text("kill_streak_icon"), "Killstreak icon");
    assert!(text("rune_haste").contains("急速"));
}

#[test]
fn the_updater_says_a_damaged_download_as_the_stores_do() {
    assert_eq!(
        tf2tools_app::_rust_i18n_try_translate("zh-CN", "updater.failure.damaged"),
        tf2tools_app::_rust_i18n_try_translate("zh-CN", "messages.store.download.damaged")
    );
}

fn han(c: char) -> bool {
    matches!(c, '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}')
}

// Each place a value puts ASCII , . : ; ! ? ( ) right after a Han character, or an ASCII space
// between two Han characters; Chinese sentences take the full-width forms and no space.
fn punctuation_slips(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut slips = Vec::new();
    for (i, &c) in chars.iter().enumerate() {
        let prev = i.checked_sub(1).map(|j| chars[j]);
        let next = chars.get(i + 1).copied();
        if prev.is_some_and(han) && matches!(c, ',' | '.' | ':' | ';' | '!' | '?' | '(' | ')') {
            slips.push(format!("ASCII {c:?} after Han at {i}"));
        }
        if c == ' ' && prev.is_some_and(han) && next.is_some_and(han) {
            slips.push(format!("space between Han at {i}"));
        }
    }
    slips
}

#[test]
fn the_punctuation_scan_flags_only_half_width_slips() {
    for ok in [
        "设置",
        "安装 HUD",
        "%{name} 已安装。",
        "1.3万",
        "保存：%{name}",
        "flawhud（副本 3）",
        "快捷键 (?)",
        "Bonk! 原子能饮料",
        "下载中… %{done} / %{total}",
        "“预设”",
        "https://logs.tf",
        "%{name}",
        "%{logs}约 %{full}",
    ] {
        assert!(
            punctuation_slips(ok).is_empty(),
            "{ok:?}: {:?}",
            punctuation_slips(ok)
        );
    }
    for bad in [
        "设置:",
        "完成.",
        "安装,下载",
        "是吗?",
        "什么!",
        "下载中...",
        "flawhud (副本)",
        "副本(3)",
        "安装 设置",
        "下载;",
    ] {
        assert!(!punctuation_slips(bad).is_empty(), "{bad:?}");
    }
}

#[test]
fn chinese_takes_full_width_punctuation() {
    let messages = tf2tools_app::_rust_i18n_backend()
        .messages_for_locale("zh-CN")
        .unwrap_or_default();
    assert!(!messages.is_empty(), "zh-CN.yml is missing or empty");
    let slips: Vec<String> = messages
        .iter()
        .filter_map(|(key, text)| {
            let found = punctuation_slips(text);
            (!found.is_empty()).then(|| format!("{key}: {found:?}"))
        })
        .collect();
    assert!(slips.is_empty(), "zh-CN punctuation: {slips:#?}");
}

// Each language owns the order and joiner of the Crosshair Maker's position line; Chinese puts
// the direction first and joins with a full-width comma.
#[test]
fn the_crosshair_position_reads_in_each_languages_order() {
    use tf2tools_app::pages::crosshair_maker::model::{CanvasSize, position_text};
    let cases = [
        ("en", "3 right, 3 up", "1 left, 32 down"),
        ("ru", "3 вправо, 3 вверх", "1 влево, 32 вниз"),
        ("pt-BR", "3 à direita, 3 acima", "1 à esquerda, 32 abaixo"),
        (
            "es",
            "3 a la derecha, 3 arriba",
            "1 a la izquierda, 32 abajo",
        ),
        ("de", "3 rechts, 3 oben", "1 links, 32 unten"),
        ("fr", "3 à droite, 3 en haut", "1 à gauche, 32 en bas"),
        ("zh-CN", "向右 3 格，向上 3 格", "向左 1 格，向下 32 格"),
    ];
    for (code, up_right, down_left) in cases {
        let _guard = LocaleGuard::switch(code);
        assert_eq!(position_text(CanvasSize::Large, 34, 29), up_right, "{code}");
        assert_eq!(
            position_text(CanvasSize::Large, 31, 63),
            down_left,
            "{code}"
        );
    }
}

// Lists the app joins in code take the language's separator: the enumeration comma in Chinese.
#[test]
fn joined_lists_take_the_language_separator() {
    use gpui_kit::SharedString;
    use tf2tools_app::pages::addons::model::movement_on_text;
    use tf2tools_app::pages::profiles::model::{ApplyOutcome, outcome_toast};
    let installed: Vec<SharedString> = vec!["A".into(), "B".into()];
    let keys = vec!["w".to_owned(), "s".to_owned()];
    for (lang, sep) in [("en", ", "), ("zh-CN", "、")] {
        let _guard = LocaleGuard::switch(lang);
        assert_eq!(
            i18n::separated(&["a", "b", "c"]),
            format!("a{sep}b{sep}c"),
            "{lang}"
        );
        assert_eq!(i18n::separated(&["a"]), "a", "{lang}");
        let toast = outcome_toast(&ApplyOutcome {
            kept: installed.clone(),
            ..Default::default()
        })
        .2;
        assert!(toast.contains(&format!("A{sep}B")), "{lang}: {toast}");
        let line = movement_on_text(&keys, &[], false);
        assert!(line.contains(&format!("W{sep}S")), "{lang}: {line}");
    }
}

// Code joins a player-facing list through `i18n::separated`, never a hard-coded comma; the
// allowed files join text that is not translated.
#[test]
fn no_player_facing_list_is_joined_with_a_hard_coded_comma() {
    const ALLOWED: [&str; 2] = ["assets.rs", "graphics/model.rs"];
    fn scan(dir: &std::path::Path, hits: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                scan(&path, hits);
            } else if path.extension().is_some_and(|e| e == "rs")
                && !ALLOWED.iter().any(|a| path.ends_with(a))
            {
                let text = std::fs::read_to_string(&path).unwrap();
                for (n, line) in text.lines().enumerate() {
                    if line.contains(r#"join(", ")"#) {
                        hits.push(format!("{}:{}", path.display(), n + 1));
                    }
                }
            }
        }
    }
    let mut hits = Vec::new();
    scan(
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src")),
        &mut hits,
    );
    assert!(hits.is_empty(), "{hits:#?}");
}

// A HUD Store trait row joins its label and value through the language's label format, names
// the systems it knows and passes an unknown one through as hud-db wrote it.
#[test]
fn hud_store_trait_rows_read_in_each_language() {
    use tf2tools_app::pages::hud_store::bodies::trait_rows;
    use tf2tools_app::pages::hud_store::curated::CuratedHud;
    let hud = CuratedHud::parse(
        "budhud",
        "budhud",
        "Whisker",
        None,
        vec![],
        r#"{"traits":{"os":["windows","mac","linux","bsd"],"res":["16/9"]}}"#,
    );
    for (lang, os, res) in [
        ("en", "OS: Windows, macOS, Linux, bsd", "Resolutions: 16/9"),
        ("zh-CN", "系统：Windows、macOS、Linux、bsd", "分辨率：16/9"),
    ] {
        let _guard = LocaleGuard::switch(lang);
        assert_eq!(
            trait_rows(&hud),
            vec![os.to_owned(), res.to_owned()],
            "{lang}"
        );
        assert_eq!(
            i18n::labelled("a", "b"),
            format!("a{}b", if lang == "en" { ": " } else { "：" })
        );
    }
}

// Other code-built "label: value" lines take the same format: the Storage card's total and rows
// and a logs.tf medigun's charges.
#[test]
fn code_built_label_value_lines_take_the_language_colon() {
    use tf2tools_app::pages::logs_tf::text::charge_text;
    use tf2tools_app::pages::settings::{storage_row_label, storage_total_label};
    use tf2tools_core::Medigun;
    for (lang, colon) in [("en", ": "), ("zh-CN", "：")] {
        let _guard = LocaleGuard::switch(lang);
        let total = storage_total_label(2_100_000_000);
        assert!(
            total.ends_with(&format!("{colon}2.0 GB")),
            "{lang}: {total}"
        );
        let row = storage_row_label("HUDs", 1_048_576, 1_048_576);
        assert!(row.starts_with(&format!("HUDs{colon}")), "{lang}: {row}");
        let charges = charge_text(&Medigun::Kritzkrieg, 12, 12);
        assert!(
            charges.starts_with(&format!("Kritzkrieg{colon}12")),
            "{lang}: {charges}"
        );
    }
}

#[test]
fn profile_toasts_end_each_sentence_with_a_chinese_full_stop() {
    use tf2tools_app::pages::profiles::model::{
        ApplyOutcome, modules_foreign, modules_no_catalog, outcome_toast,
    };
    use tf2tools_core::{ApplySkipReason, ApplyStep};
    let _guard = LocaleGuard::switch("zh-CN");
    let warned = ApplyOutcome {
        kept: vec!["x".into()],
        skipped_steps: vec![
            (ApplyStep::Graphics, ApplySkipReason::MastercomfigNotManaged),
            (ApplyStep::Addons, ApplySkipReason::MastercomfigNotInstalled),
        ],
        ..ApplyOutcome::default()
    };
    let (_, _, warning) = outcome_toast(&warned);
    for reason in [modules_no_catalog(), modules_foreign()] {
        let failed = ApplyOutcome {
            failed: vec![(ApplyStep::Graphics, reason.to_string())],
            ..ApplyOutcome::default()
        };
        let (_, _, error) = outcome_toast(&failed);
        let plain = error.replace("modules.cfg", "");
        assert!(!plain.contains('.') && error.ends_with('。'), "{error}");
    }
    assert!(
        !warning.contains('.') && warning.ends_with('。'),
        "{warning}"
    );
}
