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
