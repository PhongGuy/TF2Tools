//! The Activity Log page, the rows sync and downloads write, and the logs.tf page, in Russian.

use gpui_kit::component::select::SelectItem as _;
use gpui_kit::{AnyWindowHandle, Entity, FontWeight, Pixels, TestAppContext, px};
use tf2tools_app::activity::{self, download_entry, sync_entry};
use tf2tools_app::ids;
use tf2tools_app::pages::logs::LogsPage;
use tf2tools_app::pages::logs::model::{
    action_label, category_label, category_options, level_options,
};
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_app::store_download::{DownloadEvent, DownloadTarget};
use tf2tools_app::sync::SyncSummary;
use tf2tools_core::{GbId, LogLevel, NewLogEntry, StoreItemId, StoreKind};

use crate::LocaleGuard;
use crate::library::{shown_text, text_width};
use crate::support;

// The kit's medium Select trigger around its title: borders, padding, gap and caret.
const SELECT_CHROME: Pixels = px(42.);

fn missing() -> Tf2State {
    Tf2State::Missing { saved: None }
}

fn open_logs(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<LogsPage> {
    support::click_id(cx, s.win, ids::nav(Page::Logs));
    let page = cx
        .update(|cx| s.shell.read(cx).logs_page())
        .expect("a Logs page");
    support::wait_real(cx, "logs to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

// Every label fits the Select `id`'s text room at the Select's text-sm.
fn assert_rows_fit(
    cx: &mut TestAppContext,
    win: AnyWindowHandle,
    id: &'static str,
    labels: &[String],
) {
    let select = support::bounds_of(cx, win, id).unwrap().size.width;
    for label in labels {
        let w = text_width(cx, win, label, 0.875, FontWeight::NORMAL);
        assert!(
            w + SELECT_CHROME <= select,
            "{label:?} is {w:?} in a {select:?} select"
        );
    }
}

#[gpui_kit::test]
fn the_logs_page_reads_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    let _page = open_logs(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::LOGS_EMPTY).as_deref(),
        Some("Записей нет")
    );
    assert_eq!(
        shown_text(cx, s.win, ids::LOGS_CATEGORY).as_deref(),
        Some("Все категории")
    );
    assert_eq!(
        shown_text(cx, s.win, ids::LOGS_LEVEL).as_deref(),
        Some("Все уровни")
    );
    let cats: Vec<String> = category_options()
        .iter()
        .map(|o| o.title().to_string())
        .collect();
    assert_eq!(
        cats,
        [
            "Все категории",
            "HUD",
            "Звуки",
            "Прицелы",
            "Магазин",
            "Настройки",
            "Система",
            "Профили",
            "Дополнения"
        ]
    );
    let levels: Vec<String> = level_options()
        .iter()
        .map(|o| o.title().to_string())
        .collect();
    assert_eq!(levels, ["Все уровни", "Инфо", "Внимание", "Ошибка"]);
    assert_rows_fit(cx, s.win, ids::LOGS_CATEGORY, &cats);
    assert_rows_fit(cx, s.win, ids::LOGS_LEVEL, &levels);
}

#[gpui_kit::test]
fn a_russian_log_row_names_its_category_action_and_level(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    let entry = NewLogEntry::new(LogLevel::Error, "hud_store", "download", "x").with_details("d");
    s.services.activity.append(&entry).unwrap();
    let id = support::log_rows(&s.services)[0].id();
    let _page = open_logs(cx, &s);
    let shown = [
        ids::log_category(id),
        ids::log_action(id),
        ids::log_level(id),
        ids::log_details_toggle(id),
    ]
    .map(|e| support::label_of(cx, s.win, e));
    let want = ["Магазин HUD", "Скачивание", "ошибка", "Подробности"];
    assert_eq!(shown, want.map(|t| Some(t.to_owned())));
    assert_eq!(
        support::label_of(cx, s.win, ids::LOGS_CLEAR).as_deref(),
        Some("Очистить")
    );
}

#[test]
fn categories_and_actions_read_in_russian_and_unknown_ones_stay_spaced() {
    let _guard = LocaleGuard::switch("ru");
    let cats = [
        "hud",
        "sound",
        "addon_store",
        "store",
        "system",
        "some_thing",
    ]
    .map(|c| category_label(c).to_string());
    assert_eq!(
        cats,
        [
            "HUD",
            "Звук",
            "Магазин дополнений",
            "Магазин",
            "Система",
            "Some thing"
        ]
    );
    let acts = [
        "save_current",
        "install_fonts",
        "update-download",
        "re_sync",
    ]
    .map(|a| action_label(a).to_string());
    assert_eq!(
        acts,
        [
            "Сохранение текущей настройки",
            "Установка шрифтов",
            "Скачивание обновления",
            "Re sync"
        ]
    );
}

fn info(cat: &str, action: &str, msg: &str) -> NewLogEntry {
    NewLogEntry::new(LogLevel::Info, cat, action, msg)
}

#[test]
fn sync_rows_are_written_in_russian_with_their_plural_forms() {
    let _guard = LocaleGuard::switch("ru");
    let synced = |kind, n| {
        sync_entry(
            kind,
            SyncSummary {
                fresh: false,
                fetched: n,
                rows: u64::from(n),
            },
        )
    };
    let sounds: Vec<_> = [1, 2, 5, 21].map(|n| synced(StoreKind::Sounds, n)).to_vec();
    assert_eq!(
        sounds,
        [
            Some(info(
                "sound_store",
                "sync",
                "Синхронизирован 1 звук с GameBanana"
            )),
            Some(info(
                "sound_store",
                "sync",
                "Синхронизировано 2 звука с GameBanana"
            )),
            Some(info(
                "sound_store",
                "sync",
                "Синхронизировано 5 звуков с GameBanana"
            )),
            Some(info(
                "sound_store",
                "sync",
                "Синхронизирован 21 звук с GameBanana"
            )),
        ]
    );
    assert_eq!(
        synced(StoreKind::CuratedHuds, 41),
        Some(info("store", "sync", "Синхронизирован 41 HUD из подборки"))
    );
    assert_eq!(
        synced(StoreKind::Addons, 14),
        Some(info(
            "addon_store",
            "sync",
            "Синхронизировано 14 дополнений из mastercomfig"
        ))
    );
}

#[test]
fn download_rows_are_written_in_russian() {
    let _guard = LocaleGuard::switch("ru");
    let target = |kind, item, name: &str| DownloadTarget {
        kind,
        item,
        name: name.into(),
    };
    let gb = StoreItemId::GameBanana(GbId::new(7));
    let done = download_entry(&DownloadEvent::Finished {
        target: target(StoreKind::GameBananaHuds, gb.clone(), "FlawHUD"),
    });
    assert_eq!(
        done,
        info("hud_store", "download", "HUD скачан с GameBanana: FlawHUD")
            .with_details("ID на GameBanana: 7")
    );
    let failed = download_entry(&DownloadEvent::Failed {
        target: target(StoreKind::Crosshairs, gb, "Ring"),
        message: "boom".into(),
    });
    assert_eq!(
        failed,
        NewLogEntry::new(
            LogLevel::Error,
            "crosshair_store",
            "download",
            "Не удалось скачать прицел (ID на GameBanana: 7)"
        )
        .with_details("boom")
    );
}

#[gpui_kit::test]
fn the_startup_row_is_written_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    support::init(cx);
    let services = support::services(&env, support::no_steam());
    support::install_services(cx, services.clone());
    cx.update(|cx| activity::startup(cx));
    support::wait_real(cx, "the startup row", |_| {
        support::log_rows(&services).len() == 1
    });
    assert_eq!(
        support::log_rows(&services)[0].message(),
        "TF2Tools запущен"
    );
}

// The Logs page is kept while Settings shows, so its Selects must retitle after a switch and
// keep the chosen filters.
#[gpui_kit::test]
fn the_kept_logs_page_follows_a_language_switch(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    let page = open_logs(cx, &s);
    support::click_id(cx, s.win, ids::LOGS_CATEGORY);
    support::click_id(cx, s.win, ids::select_option(ids::LOGS_CATEGORY, "Sounds"));
    support::click_id(cx, s.win, ids::LOGS_LEVEL);
    support::click_id(cx, s.win, ids::select_option(ids::LOGS_LEVEL, "Error"));
    let shown = |cx: &mut TestAppContext| {
        [ids::LOGS_CATEGORY, ids::LOGS_LEVEL].map(|id| shown_text(cx, s.win, id))
    };
    assert_eq!(
        shown(cx),
        [Some("Sounds".to_owned()), Some("Error".to_owned())]
    );
    crate::settings::open_settings(cx, &s);
    crate::settings::pick(cx, s.win, "ru");
    support::click_id(cx, s.win, ids::nav(Page::Logs));
    assert_eq!(
        shown(cx),
        [Some("Звуки".to_owned()), Some("Ошибка".to_owned())]
    );
    assert_eq!(cx.update(|cx| page.read(cx).category()), Some("sound"));
    assert_eq!(cx.update(|cx| page.read(cx).level()), Some(LogLevel::Error));
}
