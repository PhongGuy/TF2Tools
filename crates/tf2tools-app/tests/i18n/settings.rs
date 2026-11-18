use std::sync::Arc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};
use gpui_kit::{AppContext as _, ScrollDelta, point, px};
use tf2tools_app::app::{Boot, start};
use tf2tools_app::audio::RecordingAudio;
use tf2tools_app::i18n;
use tf2tools_app::ids;
use tf2tools_app::pages::settings::{SettingsPage, format_hours, format_synced};
use tf2tools_app::picker::ScriptedPicker;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_app::sync_messages::describe_sync;
use tf2tools_core::{StoreKind, SyncError, Volume};

use crate::LocaleGuard;
use crate::support::{self, ShellWindow};

pub const NOTE_EN: &str = "Parts of TF2Tools not yet translated stay in English.";
pub const NOTE_RU: &str = "Ещё не переведённые части TF2Tools остаются на английском.";
pub const NOTE_PT_BR: &str = "As partes do TF2Tools ainda não traduzidas ficam em inglês.";
pub const NOTE_ES: &str = "Las partes de TF2Tools aún sin traducir se muestran en inglés.";
pub const NOTE_DE: &str = "Noch nicht übersetzte Teile von TF2Tools erscheinen auf Englisch.";
pub const NOTE_FR: &str = "Les parties de TF2Tools pas encore traduites restent en anglais.";

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

/// Opens the Language select and confirms the row for `code`.
pub fn pick(cx: &mut TestAppContext, win: AnyWindowHandle, code: &str) {
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
        locales: Vec::new(),
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
        s.services.db.settings().unwrap().language() == Some("ru")
    });

    pick(cx, s.win, "en");
    assert_eq!(note(cx, s.win).as_deref(), Some(NOTE_EN));
    assert_eq!(
        support::label_of(cx, s.win, ids::SETTINGS_VOLUME_LABEL).as_deref(),
        Some("Volume: 70%")
    );
    assert_eq!(placeholder(cx, &page), "/path/to/Team Fortress 2");
    support::wait_real(cx, "English saved", |_| {
        s.services.db.settings().unwrap().language() == Some("en")
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

// Boots the shell with `locales` as the system's preferred locales and waits for the HUDs page.
fn start_with_locales(
    cx: &mut TestAppContext,
    env: &support::Env,
    locales: &[&str],
) -> AnyWindowHandle {
    support::init(cx);
    let boot = Boot {
        locales: locales.iter().map(|l| (*l).to_owned()).collect(),
        ..boot(env)
    };
    let win = cx.update(|cx| start(boot, cx)).unwrap();
    support::wait_real(cx, "the shell", |cx| {
        support::present(cx, win, ids::page(Page::Huds))
    });
    win
}

#[gpui_kit::test]
fn a_russian_system_starts_in_russian_until_the_player_picks(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
    let env = support::env();
    let win = start_with_locales(cx, &env, &["ru-RU"]);
    assert_eq!(i18n::current(), "ru");
    support::click_id(cx, win, ids::nav(Page::Settings));
    support::wait_real(cx, "the Russian note", |cx| {
        note(cx, win).as_deref() == Some(NOTE_RU)
    });
    // Following the system writes nothing, so a later pick is the only saved choice.
    let saved = cx.update(|cx| {
        let db = &cx.global::<tf2tools_app::services::Services>().db;
        db.settings().unwrap().language().map(str::to_owned)
    });
    assert_eq!(saved, None);
}

#[gpui_kit::test]
fn a_brazilian_system_starts_in_portuguese(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
    let env = support::env();
    let win = start_with_locales(cx, &env, &["pt_BR.UTF-8"]);
    assert_eq!(i18n::current(), "pt-BR");
    support::click_id(cx, win, ids::nav(Page::Settings));
    support::wait_real(cx, "the Portuguese note", |cx| {
        note(cx, win).as_deref() == Some(NOTE_PT_BR)
    });
}

#[gpui_kit::test]
fn a_spanish_system_starts_in_spanish(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
    let env = support::env();
    let win = start_with_locales(cx, &env, &["es_ES.UTF-8"]);
    assert_eq!(i18n::current(), "es");
    support::click_id(cx, win, ids::nav(Page::Settings));
    support::wait_real(cx, "the Spanish note", |cx| {
        note(cx, win).as_deref() == Some(NOTE_ES)
    });
}

#[gpui_kit::test]
fn a_mexican_system_starts_in_spanish(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
    let env = support::env();
    let win = start_with_locales(cx, &env, &["es_MX.UTF-8"]);
    assert_eq!(i18n::current(), "es");
    support::click_id(cx, win, ids::nav(Page::Settings));
    support::wait_real(cx, "the Spanish note", |cx| {
        note(cx, win).as_deref() == Some(NOTE_ES)
    });
}

#[gpui_kit::test]
fn a_german_system_starts_in_german(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
    let env = support::env();
    let win = start_with_locales(cx, &env, &["de_DE.UTF-8"]);
    assert_eq!(i18n::current(), "de");
    support::click_id(cx, win, ids::nav(Page::Settings));
    support::wait_real(cx, "the German note", |cx| {
        note(cx, win).as_deref() == Some(NOTE_DE)
    });
}

#[gpui_kit::test]
fn an_austrian_system_starts_in_german(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
    let env = support::env();
    let win = start_with_locales(cx, &env, &["de_AT.UTF-8"]);
    assert_eq!(i18n::current(), "de");
    support::click_id(cx, win, ids::nav(Page::Settings));
    support::wait_real(cx, "the German note", |cx| {
        note(cx, win).as_deref() == Some(NOTE_DE)
    });
}

#[gpui_kit::test]
fn a_french_system_starts_in_french(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
    let env = support::env();
    let win = start_with_locales(cx, &env, &["fr_FR.UTF-8"]);
    assert_eq!(i18n::current(), "fr");
    support::click_id(cx, win, ids::nav(Page::Settings));
    support::wait_real(cx, "the French note", |cx| {
        note(cx, win).as_deref() == Some(NOTE_FR)
    });
}

#[gpui_kit::test]
fn a_canadian_system_starts_in_french(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
    let env = support::env();
    let win = start_with_locales(cx, &env, &["fr_CA.UTF-8"]);
    assert_eq!(i18n::current(), "fr");
    support::click_id(cx, win, ids::nav(Page::Settings));
    support::wait_real(cx, "the French note", |cx| {
        note(cx, win).as_deref() == Some(NOTE_FR)
    });
}

#[gpui_kit::test]
fn an_unsupported_system_language_starts_in_english(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    start_with_locales(cx, &env, &["it-IT"]);
    assert_eq!(i18n::current(), "en");
}

#[gpui_kit::test]
fn english_picked_in_settings_outlasts_a_russian_system(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
    let env = support::env();
    support::services(&env, support::no_steam())
        .db
        .set_language("en")
        .unwrap();
    start_with_locales(cx, &env, &["ru_RU.UTF-8"]);
    assert_eq!(i18n::current(), "en");
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
        "3 дня, освободит около 11,8\u{a0}МБ"
    );
    assert_eq!(size_text(10_300), "10,1\u{a0}КБ");
    assert_eq!(size_text(512), "512\u{a0}Б");
    assert_eq!(
        usage_text(25_000_000, 21, 216_300),
        "Кэш логов занимает около 23,8\u{a0}МБ; 21 полный лог занимает около 211,2\u{a0}КБ."
    );
    assert_eq!(
        usage_text(25_000_000, 3, 30_900),
        "Кэш логов занимает около 23,8\u{a0}МБ; 3 полных лога занимают около 30,2\u{a0}КБ."
    );
}

#[test]
fn the_storage_rows_read_in_russian() {
    use tf2tools_app::pages::settings::{
        size_text, storage_name, storage_row_label, storage_total_label,
    };
    use tf2tools_core::StorageKind;
    let _guard = LocaleGuard::switch("ru");
    assert_eq!(size_text(2_100_000_000), "2,0\u{a0}ГБ");
    assert_eq!(size_text(3 << 40), "3,0\u{a0}ТБ");
    let names: Vec<String> = StorageKind::ALL
        .iter()
        .map(|k| storage_name(*k).to_string())
        .collect();
    assert_eq!(
        names,
        [
            "HUD",
            "Звуки",
            "Прицелы",
            "Дополнения",
            "Кэш logs.tf",
            "Данные магазинов",
            "Кэш изображений"
        ]
    );
    assert_eq!(
        storage_row_label(&storage_name(StorageKind::Huds), 412_000_000, 2_100_000_000),
        "HUD: 392,9\u{a0}МБ. 2,0\u{a0}ГБ без сжатия"
    );
    assert_eq!(storage_total_label(2_100_000_000), "Всего: 2,0\u{a0}ГБ");
    assert_eq!(
        tf2tools_app::_rust_i18n_try_translate("ru", "settings.storage.unmeasured").as_deref(),
        Some("Не удалось измерить занятое место.")
    );
}

#[gpui_kit::test]
fn the_keep_rows_are_renamed_after_a_language_switch(cx: &mut TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{AppContext as _, ScrollDelta, point};
    use tf2tools_app::logs_trim::LogsTrim;
    let _guard = LocaleGuard::hold();
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let _page = open_settings(cx, &s);
    // The first pass lands before the switch, so no later pass can retitle the rows instead.
    support::wait_real(cx, "the first trim pass", |cx| {
        cx.update(|cx| {
            let trim = LogsTrim::global(cx).read(cx);
            trim.passes() >= 1 && !trim.is_running()
        })
    });
    pick(cx, s.win, "ru");
    // Wheel the page until the keep select is in view, then open it.
    for _ in 0..120 {
        let page = support::bounds_of(cx, s.win, ids::page(Page::Settings)).unwrap();
        let b = support::bounds_of(cx, s.win, ids::SETTINGS_LOG_KEEP).unwrap();
        if b.top() >= page.top() && b.bottom() <= page.bottom() {
            break;
        }
        let step = if b.top() < page.top() { 1. } else { -1. };
        let turn = ScrollDelta::Lines(point(0., step));
        cx.update_window(s.win, |_, w, cx| {
            w.scroll(ids::page(Page::Settings), turn, cx)
        })
        .unwrap();
        support::settle(cx, s.win);
    }
    support::click_id(cx, s.win, ids::SETTINGS_LOG_KEEP);
    assert_eq!(
        support::label_of(cx, s.win, ids::settings_log_keep("3d")).as_deref(),
        Some("3 дня")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::settings_log_keep("forever")).as_deref(),
        Some("Всегда")
    );
}

#[gpui_kit::test]
fn the_settings_saved_entry_is_written_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    open_settings(cx, &s);
    cx.update_window(s.win, |_, w, cx| {
        let down = ScrollDelta::Pixels(point(px(0.), px(-4000.)));
        w.scroll(ids::page(Page::Settings), down, cx)
    })
    .unwrap();
    support::click_id(cx, s.win, ids::SETTINGS_SAVE);
    support::wait_real(cx, "the row", |_| support::log_rows(&s.services).len() == 1);
    let row = &support::log_rows(&s.services)[0];
    assert_eq!((row.category(), row.action()), ("settings", "update"));
    crate::messages::assert_russian(row.message());
}

#[test]
fn the_stores_card_reads_in_russian() {
    let ru = |key: &str| tf2tools_app::_rust_i18n_try_translate("ru", key).map(|t| t.into_owned());
    assert_eq!(ru("settings.stores.title").as_deref(), Some("Магазины"));
    assert_eq!(
        ru("settings.stores.resync_all").as_deref(),
        Some("Обновить все")
    );
    // The per-store button stays about as short as the English, so each row keeps its text room.
    assert_eq!(ru("settings.sync.resync").as_deref(), Some("Обновить"));
    assert_eq!(
        ru("settings.sync.syncing").as_deref(),
        Some("Обновление...")
    );
    assert_eq!(
        ru("settings.sync.curated_huds.description").as_deref(),
        Some("HUD, отобранные hud-db")
    );
}

// Every control of the Storage and Stores cards lies inside the page column; the store lines
// end left of a one-line Resync.
fn assert_settings_fit(cx: &mut TestAppContext, win: AnyWindowHandle, lang: &str) {
    use tf2tools_app::widgets::scroll::BAR_WIDTH;
    use tf2tools_core::{StorageKind, StoreKind};
    let page = support::bounds_of(cx, win, ids::page(Page::Settings)).unwrap();
    let mut inside: Vec<gpui_kit::ElementId> = vec![
        ids::SETTINGS_STORAGE.into(),
        ids::SETTINGS_STORAGE_TOTAL.into(),
        ids::SETTINGS_RESYNC_ALL.into(),
        ids::SETTINGS_CACHE.into(),
        ids::SETTINGS_CACHE_LABEL.into(),
        ids::SETTINGS_MATURE_ADDONS.into(),
        ids::SETTINGS_LOG_KEEP.into(),
        ids::SETTINGS_LOG_SPACE.into(),
        ids::SETTINGS_SAVE.into(),
    ];
    inside.extend(StorageKind::ALL.iter().map(|k| ids::settings_storage(*k)));
    for id in inside {
        let b = support::bounds_of(cx, win, id.clone())
            .unwrap_or_else(|| panic!("{lang}: {id:?} not drawn"));
        assert!(
            b.left() >= page.left() && b.right() <= page.right() - BAR_WIDTH,
            "{lang}: {id:?} {b:?} runs past {page:?}"
        );
    }
    for kind in StoreKind::ALL {
        let lines = support::bounds_of(cx, win, ids::settings_sync(kind)).unwrap();
        let resync = support::bounds_of(cx, win, ids::settings_resync(kind)).unwrap();
        assert!(
            lines.right() <= resync.left(),
            "{lang}: {kind:?} {lines:?} runs under {resync:?}"
        );
        assert!(
            resync.right() <= page.right() - BAR_WIDTH,
            "{lang}: {kind:?} {resync:?}"
        );
        assert_eq!(resync.size.height, px(32.), "{lang}: {kind:?} Resync wraps");
    }
}

#[gpui_kit::test]
fn the_settings_cards_fit_at_800_by_600_in_every_language(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    cx.simulate_window_resize(s.win, gpui_kit::size(px(800.), px(600.)));
    support::settle(cx, s.win);
    let page = open_settings(cx, &s);
    support::wait_real(cx, "the store status and the storage figures", |cx| {
        cx.update(|cx| {
            let pack = tf2tools_app::library_pack::LibraryPack::global(cx);
            let pack = pack.read(cx);
            page.read(cx).is_sync_loaded() && pack.space().is_some() && !pack.is_running()
        })
    });
    support::settle(cx, s.win);
    assert_settings_fit(cx, s.win, "en");
    pick(cx, s.win, "ru");
    assert_settings_fit(cx, s.win, "ru");
    pick(cx, s.win, "pt-BR");
    assert_settings_fit(cx, s.win, "pt-BR");
    pick(cx, s.win, "es");
    assert_settings_fit(cx, s.win, "es");
    pick(cx, s.win, "de");
    assert_settings_fit(cx, s.win, "de");
    pick(cx, s.win, "fr");
    assert_settings_fit(cx, s.win, "fr");
}

// The hub keeps a failed sync for its card, so the Stores card must word it when drawn and take a
// language picked on the same page.
#[gpui_kit::test]
fn a_store_sync_failure_follows_a_language_switch(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
    let env = support::env();
    let (hub, runs) = support::fake_hub(cx);
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    open_settings(cx, &s);
    assert!(cx.update(|cx| hub.update(cx, |h, cx| h.start(StoreKind::Sounds, false, cx))));
    let offline = SyncError::Network {
        url: "https://gamebanana.com/apiv11".into(),
        message: "connection refused".into(),
    };
    runs.try_recv()
        .unwrap()
        .finish(Err(describe_sync(StoreKind::Sounds, &offline)));
    let status = |cx: &mut TestAppContext| {
        support::label_of(cx, s.win, ids::settings_sync(StoreKind::Sounds)).unwrap_or_default()
    };
    support::wait_hub(cx, "the failure on the card", |cx| {
        status(cx).ends_with(
            ". Sync failed: Couldn't reach GameBanana. Check your internet connection and try again.",
        )
    });

    pick(cx, s.win, "ru");
    let shown = status(cx);
    assert!(
        shown.ends_with(
            ". Ошибка синхронизации: Не удалось подключиться к GameBanana. Проверьте подключение к интернету и попробуйте снова."
        ),
        "{shown}"
    );
}
