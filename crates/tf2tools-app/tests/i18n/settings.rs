use std::sync::Arc;

use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};
use tf2tools_app::app::{Boot, start};
use tf2tools_app::audio::RecordingAudio;
use tf2tools_app::i18n;
use tf2tools_app::ids;
use tf2tools_app::pages::settings::{SettingsPage, format_hours, format_synced};
use tf2tools_app::picker::ScriptedPicker;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::Volume;

use crate::LocaleGuard;
use crate::support::{self, ShellWindow};

pub const NOTE_EN: &str = "Parts of TF2Tools not yet translated stay in English.";
pub const NOTE_RU: &str = "Ещё не переведённые части TF2Tools остаются на английском.";

/// Opens Settings through the sidebar and waits for its first load.
pub fn open_settings(cx: &mut TestAppContext, s: &ShellWindow) -> Entity<SettingsPage> {
    support::click_id(cx, s.win, ids::nav(Page::Settings));
    let page = cx
        .update(|cx| s.shell.read(cx).settings_page())
        .expect("a settings page");
    support::wait_real(cx, "settings to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

// Opens the Language select and confirms the row for `code`.
fn pick(cx: &mut TestAppContext, win: AnyWindowHandle, code: &str) {
    support::click_id(cx, win, ids::SETTINGS_LANGUAGE);
    support::click_id(cx, win, ids::settings_language(code));
    support::settle(cx, win);
}

fn note(cx: &mut TestAppContext, win: AnyWindowHandle) -> Option<String> {
    support::label_of(cx, win, ids::SETTINGS_LANGUAGE_NOTE)
}

fn boot(env: &support::Env) -> Boot {
    Boot {
        data: env.data.clone(),
        assets: Ok(support::repo_asset_root()),
        picker: Arc::new(ScriptedPicker::empty()),
        detector: support::no_steam(),
        audio: Arc::new(RecordingAudio::new()),
        endpoints: tf2tools_core::Endpoints::default(),
        auto_sync: false,
        offline: false,
        font_host: None,
        updates: tf2tools_app::updater::UpdateBoot::Off,
    }
}

fn placeholder(cx: &mut TestAppContext, page: &Entity<SettingsPage>) -> String {
    cx.update(|cx| {
        page.read(cx)
            .path_input()
            .read(cx)
            .presentation()
            .placeholder()
            .to_string()
    })
}

#[gpui_kit::test]
fn picking_russian_switches_settings_at_once_and_saves_it(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    s.services.db.set_volume(Volume::new(70).unwrap()).unwrap();
    let page = open_settings(cx, &s);
    assert_eq!(note(cx, s.win).as_deref(), Some(NOTE_EN));
    assert_eq!(placeholder(cx, &page), "/path/to/Team Fortress 2");

    pick(cx, s.win, "ru");
    assert_eq!(note(cx, s.win).as_deref(), Some(NOTE_RU));
    assert_eq!(
        support::label_of(cx, s.win, ids::SETTINGS_VOLUME_LABEL).as_deref(),
        Some("Громкость: 70%")
    );
    assert_eq!(placeholder(cx, &page), "/путь/к/Team Fortress 2");
    assert_eq!(i18n::current(), "ru");
    support::wait_real(cx, "Russian saved", |_| {
        s.services.db.settings().unwrap().language() == "ru"
    });

    pick(cx, s.win, "en");
    assert_eq!(note(cx, s.win).as_deref(), Some(NOTE_EN));
    assert_eq!(
        support::label_of(cx, s.win, ids::SETTINGS_VOLUME_LABEL).as_deref(),
        Some("Volume: 70%")
    );
    assert_eq!(placeholder(cx, &page), "/path/to/Team Fortress 2");
    support::wait_real(cx, "English saved", |_| {
        s.services.db.settings().unwrap().language() == "en"
    });
}

#[test]
fn russian_counts_take_their_plural_forms() {
    let _guard = LocaleGuard::switch("ru");
    let hours = [
        (1, "1 час"),
        (2, "2 часа"),
        (5, "5 часов"),
        (21, "21 час"),
        (24, "1 день"),
        (48, "2 дня"),
        (120, "5 дней"),
        (168, "7 дней"),
        (25, "1 д 1 ч"),
        (720, "30 дней"),
    ];
    for (n, text) in hours {
        assert_eq!(format_hours(n), text, "{n} hours");
    }
    let items = [
        (1, "1 элемент в кэше"),
        (3, "3 элемента в кэше"),
        (5, "5 элементов в кэше"),
        (11, "11 элементов в кэше"),
        (21, "21 элемент в кэше"),
    ];
    for (n, text) in items {
        assert_eq!(i18n::plural("settings.sync.items", n), text, "{n} items");
    }
    assert_eq!(format_synced(None), "Никогда");
    assert_eq!(format_synced(Some("not a time")), "Неизвестно");
}

#[gpui_kit::test]
fn a_language_saved_last_time_applies_at_startup(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
    let env = support::env();
    support::services(&env, support::no_steam())
        .db
        .set_language("ru")
        .unwrap();
    support::init(cx);
    let win = cx.update(|cx| start(boot(&env), cx)).unwrap();
    support::wait_real(cx, "the shell", |cx| {
        support::present(cx, win, ids::page(Page::Huds))
    });
    assert_eq!(i18n::current(), "ru");
    support::click_id(cx, win, ids::nav(Page::Settings));
    support::wait_real(cx, "the Russian note", |cx| {
        note(cx, win).as_deref() == Some(NOTE_RU)
    });
}

#[test]
fn the_logs_cache_lines_read_in_russian() {
    use tf2tools_app::pages::settings::{keep_text, size_text, usage_text};
    use tf2tools_core::KeepFullLogs;
    let _guard = LocaleGuard::switch("ru");
    assert_eq!(keep_text(KeepFullLogs::Day1, 0), "1 день");
    assert_eq!(keep_text(KeepFullLogs::Days3, 0), "3 дня");
    assert_eq!(keep_text(KeepFullLogs::Days7, 0), "7 дней");
    assert_eq!(keep_text(KeepFullLogs::Forever, 0), "Всегда");
    assert_eq!(
        keep_text(KeepFullLogs::Days3, 12_345_678),
        "3 дня, освободит около 12.3 МБ"
    );
    assert_eq!(size_text(10_300), "10 КБ");
    assert_eq!(
        usage_text(25_000_000, 21, 216_300),
        "Кэш логов занимает около 25.0 МБ; 21 полный лог занимает около 216 КБ."
    );
    assert_eq!(
        usage_text(25_000_000, 3, 30_900),
        "Кэш логов занимает около 25.0 МБ; 3 полных лога занимают около 31 КБ."
    );
}
