//! The Activity Log page, the rows sync and downloads write, and the logs.tf page, in Russian.

use gpui_kit::component::select::SelectItem as _;
use gpui_kit::{AnyWindowHandle, Entity, FontWeight, Pixels, TestAppContext, px};
use tf2tools_app::activity::{self, download_entry, sync_entry};
use tf2tools_app::ids;
use tf2tools_app::pages::logs::LogsPage;
use tf2tools_app::pages::logs::model::{
    action_label, category_label, category_options, level_options,
};
use tf2tools_app::pages::logs_tf::text::{
    counting_text, download_reason, empty_text, event_text, medigun_name, player_input_error,
    range_label, resolve_message, ring_label, streaks_label, summary_text, weapons_title,
};
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_app::store_download::{DownloadEvent, DownloadTarget};
use tf2tools_app::sync::SyncSummary;
use tf2tools_core::{
    EventKind, LogsFetchError, LogsRange, Medigun, PlayerInputError, ResolveError, Team, TfClass,
};
use tf2tools_core::{GbId, LogLevel, NewLogEntry, StoreItemId, StoreKind};

use crate::LocaleGuard;
use crate::library::{shown_text, text_width};
use crate::support;
use crate::support::logstf::{
    ARIE, SIXES, day_ago, list_body, open_log, open_page, scroll_to_in_log, serve_two_logs, shell,
    sid,
};

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

#[test]
fn the_logs_tf_texts_read_in_russian() {
    let _guard = LocaleGuard::switch("ru");
    let counted: Vec<String> = [1, 2, 5, 21]
        .map(|n| summary_text(LogsRange::Months3, n, None, 0))
        .to_vec();
    assert_eq!(
        counted,
        [
            "За последние 3 месяца: 1 лог",
            "За последние 3 месяца: 2 лога",
            "За последние 3 месяца: 5 логов",
            "За последние 3 месяца: 21 лог",
        ]
    );
    let ranges: Vec<String> = LogsRange::STOPS
        .into_iter()
        .map(|r| range_label(r).to_string())
        .collect();
    assert_eq!(
        ranges,
        ["1 месяц", "3 месяца", "6 месяцев", "1 год", "2 года", "Все"]
    );
    assert_eq!(
        weapons_title(TfClass::Scout, LogsRange::Months3, 1),
        "Оружие: Разведчик · 3 месяца · 1 лог"
    );
    assert_eq!(
        player_input_error(&PlayerInputError::NotSteam),
        "Это не похоже на ссылку на профиль Steam или Steam ID."
    );
    assert_eq!(
        resolve_message(&ResolveError::NotFound),
        "Профиль Steam не найден."
    );
    assert_eq!(
        download_reason(&LogsFetchError::Gone),
        "На logs.tf больше нет этого лога."
    );
    assert_eq!(ring_label(None), "Синхронизация logs.tf...");
    assert_eq!(streaks_label(13), "Показать серии (13)");
    assert_eq!(counting_text(3), "Подсчёт старых логов, осталось: 3");
    assert_eq!(
        event_text(&EventKind::RoundWin { team: Team::Red }, &|_| String::new()),
        (
            "Конец раунда".to_owned(),
            "Раунд выиграла команда RED".to_owned()
        )
    );
    assert_eq!(medigun_name(&Medigun::QuickFix), "Quick-Fix");
    assert_eq!(
        empty_text(),
        "Отслеживайте игрока, чтобы видеть его средние показатели по классам и логи с logs.tf."
    );
}

// The class table's titles read Russian while their ids keep the English title.
#[gpui_kit::test]
fn the_russian_class_table_keeps_its_english_ids(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes("/api/v1/log", list_body(&[(3, day_ago(1))]));
    server.mount_bytes("/json/3", SIXES.as_bytes().to_vec());
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let _page = open_page(cx, &s);
    let heads = [
        ("Played", "Сыграно"),
        ("Logs", "Логи"),
        ("Kills/min", "Убийств/мин"),
        ("Assists/min", "Помощь/мин"),
        ("Dmg/min", "Урон/мин"),
        ("KA/D", "KA/D"),
        ("Heals/min", "Хил/мин"),
        ("Ubers/10 min", "Убер/10 мин"),
    ];
    for (id, russian) in heads {
        assert_eq!(
            support::label_of(cx, s.win, ids::logstf_class_head(id)).as_deref(),
            Some(russian),
            "{id}"
        );
    }
    assert_eq!(
        support::label_of(cx, s.win, ids::LOGSTF_RANGE).as_deref(),
        Some("Период истории")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_class_unplayed(TfClass::Soldier)).as_deref(),
        Some("Не играл")
    );
}

// An open log keeps its English ids in Russian, its words read Russian, and the team table's
// Russian headers fit their columns at 800x600.
#[gpui_kit::test]
fn an_open_russian_log_keeps_its_ids_and_fits(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let server = support::StoreServer::start();
    serve_two_logs(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    cx.simulate_window_resize(s.win, gpui_kit::size(px(800.), px(600.)));
    let page = open_page(cx, &s);
    open_log(cx, &s, &page, 3);
    // The players table's 40 px Team column takes the short team header.
    for (id, russian) in [("Team", "Ком."), ("Name", "Имя"), ("K", "K")] {
        assert_eq!(
            support::label_of(cx, s.win, ids::logstf_sort(id)).as_deref(),
            Some(russian),
            "{id}"
        );
    }
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_team_cell(Team::Blue, "Kills")).as_deref(),
        Some("117")
    );
    let headers = [
        ("Kills", "Фраги"),
        ("Damage", "Урон"),
        ("Charges", "Заряды"),
        ("Drops", "Потери"),
        ("Caps", "Захваты"),
        ("Midfights", "Мидфайты"),
    ];
    for (column, russian) in headers {
        let cell =
            support::bounds_of(cx, s.win, ids::logstf_team_cell(Team::Red, column)).expect(column);
        let w = text_width(cx, s.win, russian, 0.75, FontWeight::MEDIUM);
        assert!(
            w <= cell.size.width,
            "{russian:?} is {w:?} in {:?}",
            cell.size.width
        );
    }
    // The Team column carries no id; its width is banner.rs's 48 px. The rounds table's
    // Length header reads "Время" in its 52 px column.
    assert!(text_width(cx, s.win, "Ком.", 0.75, FontWeight::MEDIUM) <= px(48.));
    assert!(text_width(cx, s.win, "Время", 0.75, FontWeight::MEDIUM) <= px(52.));
    let rahmed = sid(175_790_976);
    scroll_to_in_log(cx, &s, &page, 3, ids::logstf_medic(rahmed));
    assert_eq!(
        support::label_of(
            cx,
            s.win,
            ids::logstf_medic_fact(rahmed, "Avg time to build")
        )
        .as_deref(),
        Some("63\u{a0}с")
    );
    scroll_to_in_log(cx, &s, &page, 3, ids::LOGSTF_CHAT_TOGGLE);
    assert_eq!(
        support::label_of(cx, s.win, ids::LOGSTF_CHAT_TOGGLE).as_deref(),
        Some("Показать чат")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_download(3)).as_deref(),
        Some("Скачать лог")
    );
}
