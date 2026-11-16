//! Dates, numbers, sizes, durations and counted sentences in Russian.

use chrono::NaiveDate;
use tf2tools_app::i18n::{self, counted_plural, format};

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

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

#[test]
fn dates_read_day_first_with_cldr_months() {
    let _guard = LocaleGuard::switch("ru");
    assert_eq!(format::numeric_date(day(2026, 10, 17)), "17.10.2026");
    assert_eq!(format::numeric_date(day(2026, 3, 5)), "05.03.2026");
    let at = day(2026, 10, 17).and_hms_opt(20, 56, 9).unwrap();
    assert_eq!(format::date_time(at, false), "17.10.2026 20:56");
    assert_eq!(format::date_time(at, true), "17.10.2026 20:56:09");
    assert_eq!(
        format::day_month_year(day(2022, 12, 13)),
        "13 дек. 2022\u{a0}г."
    );
    assert_eq!(
        format::month_day_year(day(2026, 11, 16)),
        "16 нояб. 2026\u{a0}г."
    );
    let months = [
        "янв.",
        "февр.",
        "мар.",
        "апр.",
        "мая",
        "июн.",
        "июл.",
        "авг.",
        "сент.",
        "окт.",
        "нояб.",
        "дек.",
    ];
    for (m, name) in (1..=12).zip(months) {
        let expected = format!("3 {name} 2026\u{a0}г.");
        assert_eq!(format::day_month_year(day(2026, m, 3)), expected.as_str());
    }
}

#[test]
fn durations_read_in_russian_units() {
    let _guard = LocaleGuard::switch("ru");
    assert_eq!(format::hours_minutes(45), "45\u{a0}мин");
    assert_eq!(format::hours_minutes(90), "1\u{a0}ч 30\u{a0}мин");
    assert_eq!(format::time_left(160), "2\u{a0}ч 40\u{a0}мин");
    assert_eq!(format::time_left(40), "40\u{a0}мин");
    assert_eq!(format::time_left(120), "2\u{a0}ч");
    assert_eq!(format::seconds(58), "58\u{a0}с");
    assert_eq!(format::tenths("6.5"), "6,5\u{a0}с");
}

#[test]
fn english_reads_the_same_after_a_switch_to_russian_and_back() {
    let _guard = LocaleGuard::hold();
    let read = || {
        (
            format::grouped(4_812),
            format::bytes(1_536).to_string(),
            format::compact_count(1_250),
            format::day_month_year(day(2022, 12, 13)).to_string(),
            format::time_left(160).to_string(),
        )
    };
    let before = read();
    i18n::apply("ru");
    assert_ne!(read(), before);
    i18n::apply("en");
    assert_eq!(read(), before);
    let english = ["4,812", "1.5 KB", "1.3K", "13 Dec 2022", "2 h 40 min"].map(str::to_owned);
    let [a, b, c, d, e] = english;
    assert_eq!(before, (a, b, c, d, e));
}

/// Unix seconds for a wall-clock time on this computer.
pub fn local_unix(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> i64 {
    use chrono::TimeZone as _;
    chrono::Local
        .with_ymd_and_hms(y, mo, d, h, mi, s)
        .unwrap()
        .timestamp()
}

#[test]
fn shown_dates_read_day_first_in_russian() {
    use chrono::TimeZone as _;
    use tf2tools_app::pages::hud_store::bodies::added_text;
    use tf2tools_app::pages::logs_tf::text::{date_text, date_time_text};
    use tf2tools_app::pages::maps::model::last_played_label;
    use tf2tools_app::pages::settings::format_synced;
    let _guard = LocaleGuard::switch("ru");
    let unix = local_unix(2026, 11, 12, 20, 56, 30);
    assert_eq!(date_time_text(unix), "12.11.2026 20:56");
    assert_eq!(date_text(unix), "12.11.2026");
    let stamp = "2026-10-17T18:56:00Z";
    let local = chrono::DateTime::parse_from_rfc3339(stamp)
        .unwrap()
        .with_timezone(&chrono::Local)
        .format("%d.%m.%Y %H:%M")
        .to_string();
    assert_eq!(format_synced(Some(stamp)), local);
    let added = chrono::Utc
        .with_ymd_and_hms(2026, 5, 3, 12, 0, 0)
        .unwrap()
        .timestamp();
    let added = added_text(added).unwrap();
    assert_eq!(added, "Добавлено 3 мая 2026\u{a0}г.");
    assert_ru(&added);
    assert_eq!(last_played_label(Some("2026-11-01")), "01.11.2026");
    assert_eq!(last_played_label(Some("a while ago")), "a while ago");
}

#[test]
fn store_sizes_and_counts_read_in_russian() {
    use tf2tools_app::counts::format_count;
    use tf2tools_app::pages::addon_store::model::downloads_label;
    use tf2tools_app::store_download::{Progress, format_bytes, progress_label};
    use tf2tools_core::DownloadPhase;
    let _guard = LocaleGuard::switch("ru");
    assert_eq!(format_bytes(1_536), "1,5\u{a0}КБ");
    assert_eq!(progress_label(None), "Скачивание...");
    let p = Progress {
        phase: DownloadPhase::Downloading,
        downloaded: 1_048_576,
        total: Some(2_097_152),
    };
    assert_eq!(
        progress_label(Some(p)),
        "Скачивание... 1,0\u{a0}МБ / 2,0\u{a0}МБ"
    );
    let unsized_ = Progress { total: None, ..p };
    assert_eq!(progress_label(Some(unsized_)), "Скачивание... 1,0\u{a0}МБ");
    let importing = Progress {
        phase: DownloadPhase::Importing,
        ..p
    };
    assert_eq!(progress_label(Some(importing)), "Распаковка...");
    assert_eq!(format_count(999), "999");
    assert_eq!(format_count(1_250), "1,3\u{a0}тыс.");
    assert_eq!(format_count(999_950), "1000,0\u{a0}тыс.");
    assert_eq!(format_count(1_240_000), "1,2\u{a0}млн");
    assert_eq!(downloads_label(1), "1 загрузка");
    assert_eq!(downloads_label(22), "22 загрузки");
    assert_eq!(downloads_label(40_838), "40\u{a0}838 загрузок");
    assert_ru(&downloads_label(40_838));
}

#[test]
fn relative_times_take_russian_forms() {
    use chrono::{DateTime, Duration, Utc};
    use tf2tools_app::pages::logs::model::relative_time;
    let _guard = LocaleGuard::switch("ru");
    let now = DateTime::parse_from_rfc3339("2026-10-17T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let ago = |d: Duration| relative_time(&(now - d).to_rfc3339(), now);
    assert_eq!(ago(Duration::seconds(30)), "Только что");
    let cases = [
        (1, "1 минуту", "1 час", "1 день"),
        (2, "2 минуты", "2 часа", "2 дня"),
        (5, "5 минут", "5 часов", "5 дней"),
        (11, "11 минут", "11 часов", "11 дней"),
        (21, "21 минуту", "21 час", "21 день"),
        (22, "22 минуты", "22 часа", "22 дня"),
    ];
    for (n, minutes, hours, days) in cases {
        assert_eq!(ago(Duration::minutes(n)), format!("{minutes} назад"));
        assert_eq!(ago(Duration::hours(n)), format!("{hours} назад"));
        assert_eq!(ago(Duration::days(n)), format!("{days} назад"));
    }
    assert_eq!(relative_time("yesterday-ish", now), "yesterday-ish");
}

#[test]
fn logs_tf_numbers_read_in_russian() {
    use tf2tools_app::pages::logs_tf::text::{
        count_text, kapd_text, one_decimal, reads_zero, seconds_text, tenths_text, time_played_text,
    };
    let _guard = LocaleGuard::switch("ru");
    assert_eq!(count_text(4_812), "4\u{a0}812");
    assert_eq!(one_decimal(0.25), "0,3");
    assert_eq!(kapd_text(3, 2, 2), "2,5");
    assert!(reads_zero("0"));
    assert!(reads_zero("0,0"));
    assert!(!reads_zero("0,4"));
    assert_eq!(time_played_text(5_400), "1\u{a0}ч 30\u{a0}мин");
    assert_eq!(time_played_text(1_559), "25\u{a0}мин");
    assert_eq!(seconds_text(58.9), "58\u{a0}с");
    assert_eq!(tenths_text(6.66), "6,7\u{a0}с");
}

#[test]
fn logs_tf_sentences_read_in_russian() {
    use std::time::Duration;
    use tf2tools_app::pages::logs_tf::text::{
        finding_text, progress_text, summary_text, weapons_title,
    };
    use tf2tools_core::{LogsRange, TfClass};
    let _guard = LocaleGuard::switch("ru");
    assert_eq!(
        finding_text(2_000, 4_812),
        "Поиск логов: 2\u{a0}000 из 4\u{a0}812"
    );
    let at = |secs| progress_text(120, 4_812, Duration::from_secs(secs));
    let line = "Загрузка логов: 120 из 4\u{a0}812";
    assert_eq!(
        at(9_600),
        format!("{line}, осталось около 2\u{a0}ч 40\u{a0}мин")
    );
    assert_eq!(at(3_600), format!("{line}, осталось около 1\u{a0}ч"));
    assert_eq!(at(720), format!("{line}, осталось около 12\u{a0}мин"));
    assert_eq!(at(59), line);
    let now = local_unix(2026, 11, 11, 12, 0, 0);
    let recent = Some(now - 86_400);
    let logs = [
        (1, "1 лог"),
        (2, "2 лога"),
        (5, "5 логов"),
        (11, "11 логов"),
        (21, "21 лог"),
        (22, "22 лога"),
    ];
    for (n, text) in logs {
        assert_eq!(
            summary_text(LogsRange::Month1, n, recent, now),
            format!("За последний месяц: {text}")
        );
    }
    assert_eq!(
        summary_text(LogsRange::Year1, 1_400, recent, now),
        "За последний год: 1\u{a0}400 логов"
    );
    assert_eq!(
        summary_text(LogsRange::Months3, 87, recent, now),
        "За последние 3 months: 87 логов"
    );
    assert_eq!(
        summary_text(LogsRange::All, 4_812, recent, now),
        "Все логи: 4\u{a0}812"
    );
    let old = Some(local_unix(2022, 12, 13, 12, 0, 0));
    let up_to = summary_text(LogsRange::Year1, 274, old, now);
    assert_eq!(up_to, "1 year до 13 дек. 2022\u{a0}г.: 274 лога");
    assert_ru(&up_to);
    assert_eq!(
        weapons_title(TfClass::Scout, LogsRange::Months3, 1),
        "Оружие: Scout · 3 months · 1 лог"
    );
    assert_eq!(
        weapons_title(TfClass::Heavy, LogsRange::All, 1234),
        "Оружие: Heavy · All · 1\u{a0}234 лога"
    );
}

#[test]
fn library_and_store_counts_read_in_russian() {
    use tf2tools_app::pages::maps::model::count_label;
    use tf2tools_app::pages::profiles::editor_model::weapon_count;
    use tf2tools_app::pages::profiles::model::crosshair_count;
    use tf2tools_app::store_grid::count_text;
    use tf2tools_core::StoreKind;
    let _guard = LocaleGuard::switch("ru");
    let maps = [(1, "1 карта"), (2, "2 карты"), (5, "5 карт")];
    let maps = maps
        .into_iter()
        .chain([(11, "11 карт"), (21, "21 карта"), (22, "22 карты")]);
    for (n, text) in maps {
        assert_eq!(count_label(n, n), text, "{n} maps");
    }
    assert_eq!(count_label(2, 21), "2 из 21 карты");
    assert_eq!(count_label(2, 4368), "2 из 4368 карт");
    assert_eq!(count_text(StoreKind::Sounds, 2, 5), "2 из 5 звуков");
    assert_eq!(count_text(StoreKind::Crosshairs, 1, 21), "1 из 21 прицела");
    assert_eq!(count_text(StoreKind::GameBananaHuds, 2, 3), "2 из 3 HUD");
    let curated = tf2tools_app::pages::hud_store::curated::count_label(2, 304);
    assert_eq!(curated, "2 из 304 HUD");
    assert_eq!(crosshair_count(1), "1 прицел");
    assert_eq!(crosshair_count(3), "3 прицела");
    assert_eq!(crosshair_count(5), "5 прицелов");
    assert_eq!(weapon_count(1), "1 оружие");
    assert_eq!(weapon_count(2), "2 оружия");
    assert_eq!(weapon_count(5), "5 оружий");
}

#[test]
fn crosshair_and_font_sentences_read_in_russian() {
    use tf2tools_app::font_check::installed_text;
    use tf2tools_app::pages::crosshairs::weapons_detail;
    use tf2tools_app::shell::confirm::Confirm;
    let _guard = LocaleGuard::switch("ru");
    assert_eq!(weapons_detail(0), "Пока не назначен ни на одно оружие");
    assert_eq!(weapons_detail(21), "Назначен на 21 оружие");
    assert_eq!(weapons_detail(3), "Назначен на 3 оружия");
    let two = Confirm::delete_crosshair("Dot", 2, false).description;
    let used = " Его используют 2 оружия. Они вернутся к прицелу по умолчанию.";
    assert!(two.contains(used), "{two}");
    let many = Confirm::delete_crosshair("Dot", 21, false).description;
    let after = " Его использует 21 оружие. После удаления там будет прицел по умолчанию.";
    assert!(many.contains(after), "{many}");
    let none = Confirm::delete_crosshair("Dot", 0, false).description;
    assert!(!none.contains("Его"), "{none}");
    let fonts = installed_text("fonthud", 2, 0);
    let tail = "Перезапустите TF2, чтобы применить их.";
    assert_eq!(fonts, format!("Установлено 2 шрифта для fonthud. {tail}"));
    assert_ru(&fonts);
    assert_eq!(
        installed_text("fonthud", 21, 0),
        "Установлен 21 шрифт для fonthud. Перезапустите TF2, чтобы изменения вступили в силу."
    );
    assert_eq!(
        installed_text("fonthud", 1, 1),
        format!("Установлено 1 из 2 шрифтов для fonthud; подробности в журнале действий. {tail}")
    );
    assert_eq!(
        installed_text("fonthud", 0, 5),
        "Пропущено 5 шрифтов для fonthud; подробности в журнале действий."
    );
}

#[test]
fn hud_maker_counts_read_in_russian() {
    let _guard = LocaleGuard::switch("ru");
    let time = [("time", "27.10.2026 21:04:09")];
    let when = |n: u64| counted_plural("hud_maker.history.when", n, &n.to_string(), &time);
    assert_eq!(when(1), "27.10.2026 21:04:09, 1 изменение");
    assert_eq!(when(22), "27.10.2026 21:04:09, 22 изменения");
    assert_eq!(when(11), "27.10.2026 21:04:09, 11 изменений");
    let plural = tf2tools_app::i18n::plural;
    assert_eq!(
        plural("hud_maker.screens.problems", 21),
        "21 часть с проблемой"
    );
    let misfits = plural("hud_maker.options.misfits", 2);
    assert_eq!(misfits, "2 настройки не подходят к этой версии HUD.");
    assert_eq!(
        plural("hud_maker.recipes.fonts_left_out", 5),
        "5 шрифтов пропущено."
    );
    let commands = plural("hud_maker.borrow.animations", 3);
    assert_eq!(commands, "Анимации: пропущены 3 консольные команды.");
    for key in [
        "hud_maker.recipes.changes_dont_fit",
        "hud_maker.recipes.no_longer_fit",
        "hud_maker.borrow.code_keys",
        "hud_maker.borrow.unresolved_pins",
    ] {
        for n in [1, 2, 5] {
            let text = plural(key, n);
            assert_ru(&text);
            assert!(text.contains(&n.to_string()), "{key} {n}: {text}");
        }
    }
}
