//! The Activity Log page, the rows sync and downloads write, and the logs.tf page, in Russian
//! and Brazilian Portuguese.

use gpui_kit::component::select::SelectItem as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, FontWeight, Pixels, TestAppContext, px};
use tf2tools_app::activity::{self, download_entry, sync_entry};
use tf2tools_app::ids;
use tf2tools_app::pages::logs::LogsPage;
use tf2tools_app::pages::logs::model::{
    action_label, category_label, category_options, level_options,
};
use tf2tools_app::pages::logs_tf::LogsTfPage;
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
    ARIE, SIXES, day_ago, expanded, list_body, open_log, open_page, scroll_to_in_log,
    serve_two_logs, shell, sid, to_end,
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

// The Activity Log page in `lang`: every filter option fits its Select, and the toolbar fits
// at 800x600.
fn logs_page_reads_and_fits(cx: &mut TestAppContext, lang: &str) {
    let _guard = LocaleGuard::switch(lang);
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    let _page = open_logs(cx, &s);
    if lang == "ru" {
        assert_eq!(
            support::label_of(cx, s.win, ids::LOGS_EMPTY).as_deref(),
            Some("Записей нет"),
            "empty list"
        );
        assert_eq!(
            shown_text(cx, s.win, ids::LOGS_CATEGORY).as_deref(),
            Some("Все категории"),
            "category filter"
        );
        assert_eq!(
            shown_text(cx, s.win, ids::LOGS_LEVEL).as_deref(),
            Some("Все уровни"),
            "level filter"
        );
    }
    let cats: Vec<String> = category_options()
        .iter()
        .map(|o| o.title().to_string())
        .collect();
    if lang == "ru" {
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
            ],
            "categories"
        );
    }
    let levels: Vec<String> = level_options()
        .iter()
        .map(|o| o.title().to_string())
        .collect();
    if lang == "ru" {
        assert_eq!(
            levels,
            ["Все уровни", "Инфо", "Внимание", "Ошибка"],
            "levels"
        );
    }
    assert_rows_fit(cx, s.win, ids::LOGS_CATEGORY, &cats);
    assert_rows_fit(cx, s.win, ids::LOGS_LEVEL, &levels);
    // At the smallest window the two filters still leave the Clear button on the page.
    cx.simulate_window_resize(s.win, gpui_kit::size(px(800.), px(600.)));
    support::settle(cx, s.win);
    let page = support::bounds_of(cx, s.win, ids::page(Page::Logs)).unwrap();
    let clear = support::bounds_of(cx, s.win, ids::LOGS_CLEAR).unwrap();
    assert!(
        clear.right() <= page.right(),
        "{lang}: {clear:?} past {page:?}"
    );
}

#[gpui_kit::test]
fn the_logs_page_reads_in_russian(cx: &mut TestAppContext) {
    logs_page_reads_and_fits(cx, "ru");
}

#[gpui_kit::test]
fn the_logs_page_fits_in_portuguese(cx: &mut TestAppContext) {
    logs_page_reads_and_fits(cx, "pt-BR");
}

#[gpui_kit::test]
fn the_logs_page_fits_in_spanish(cx: &mut TestAppContext) {
    logs_page_reads_and_fits(cx, "es");
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

#[test]
fn the_apply_action_reads_in_every_language() {
    let ru = {
        let _guard = LocaleGuard::switch("ru");
        action_label("apply").to_string()
    };
    let pt = {
        let _guard = LocaleGuard::switch("pt-BR");
        action_label("apply").to_string()
    };
    let es = {
        let _guard = LocaleGuard::switch("es");
        action_label("apply").to_string()
    };
    assert_eq!(ru, "Применение");
    assert_eq!(pt, "Aplicar");
    assert_eq!(es, "Aplicar");
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
    assert_eq!(streaks_label(13), "Серии убийств (13)");
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
        ("Heals/min", "Леч./мин"),
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

// An open log keeps its English ids in `lang`, its words come from the catalogue, and the
// team table's headers fit their columns at 800x600.
fn open_log_keeps_its_ids_and_fits(cx: &mut TestAppContext, lang: &str) {
    let _guard = LocaleGuard::switch(lang);
    let text = |key: &str| {
        tf2tools_app::_rust_i18n_try_translate(lang, key)
            .map(|t| t.into_owned())
            .unwrap_or_else(|| panic!("{lang} has no {key}"))
    };
    let env = support::env();
    let server = support::StoreServer::start();
    serve_two_logs(&server);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    cx.simulate_window_resize(s.win, gpui_kit::size(px(800.), px(600.)));
    let page = open_page(cx, &s);
    open_log(cx, &s, &page, 3);
    // The players table's 40 px Team column takes the short team header.
    for (id, russian, key) in [
        ("Team", "Ком.", "logs_tf.detail.team_short"),
        ("Name", "Имя", "logs_tf.column.name"),
    ] {
        let label = support::label_of(cx, s.win, ids::logstf_sort(id));
        assert_eq!(label, Some(text(key)), "{lang} {id}");
        if lang == "ru" {
            assert_eq!(label.as_deref(), Some(russian), "{id}");
        }
    }
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_sort("K")).as_deref(),
        Some("K"),
        "K header"
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_team_cell(Team::Blue, "Kills")).as_deref(),
        Some("117"),
        "Blue kills cell"
    );
    let headers = [
        ("Kills", "Фраги", "logs_tf.detail.kills_short"),
        ("Damage", "Урон", "logs_tf.weapons.damage"),
        ("Charges", "Заряды", "logs_tf.detail.charges"),
        ("Drops", "Потери", "logs_tf.detail.drops"),
        ("Caps", "Захв.", "logs_tf.detail.caps"),
        ("Midfights", "Мидфайты", "logs_tf.detail.midfights"),
    ];
    for (column, russian, key) in headers {
        let header = text(key);
        if lang == "ru" {
            assert_eq!(header, russian, "{column}");
        }
        let cell =
            support::bounds_of(cx, s.win, ids::logstf_team_cell(Team::Red, column)).expect(column);
        let w = text_width(cx, s.win, &header, 0.75, FontWeight::MEDIUM);
        assert!(
            w <= cell.size.width,
            "{lang}: {header:?} is {w:?} in {:?}",
            cell.size.width
        );
    }
    // The Team column carries no id; its width is banner.rs's 40 px. The rounds table's
    // Length header sits in its 52 px column.
    let team = text("logs_tf.detail.team_short");
    let length = text("logs_tf.list.length");
    if lang == "ru" {
        assert_eq!(
            (team.as_str(), length.as_str()),
            ("Ком.", "Время"),
            "short headers"
        );
    }
    assert!(
        text_width(cx, s.win, &team, 0.75, FontWeight::MEDIUM) <= px(40.),
        "{lang}: {team:?}"
    );
    assert!(
        text_width(cx, s.win, &length, 0.75, FontWeight::MEDIUM) <= px(52.),
        "{lang}: {length:?}"
    );
    let rahmed = sid(175_790_976);
    scroll_to_in_log(cx, &s, &page, 3, ids::logstf_medic(rahmed));
    let build = support::label_of(
        cx,
        s.win,
        ids::logstf_medic_fact(rahmed, "Avg time to build"),
    );
    if lang == "ru" {
        assert_eq!(build.as_deref(), Some("63\u{a0}с"), "medic fact");
    } else {
        assert!(build.is_some(), "{lang}: no medic fact");
    }
    scroll_to_in_log(cx, &s, &page, 3, ids::LOGSTF_CHAT_TOGGLE);
    let chat = support::label_of(cx, s.win, ids::LOGSTF_CHAT_TOGGLE);
    let download = support::label_of(cx, s.win, ids::logstf_download(3));
    assert_eq!(chat, Some(text("logs_tf.detail.show_chat")), "{lang}");
    assert_eq!(download, Some(text("logs_tf.detail.download")), "{lang}");
    if lang == "ru" {
        assert_eq!(chat.as_deref(), Some("Показать чат"), "chat toggle");
        assert_eq!(download.as_deref(), Some("Скачать лог"), "download button");
    }
}

#[gpui_kit::test]
fn an_open_russian_log_keeps_its_ids_and_fits(cx: &mut TestAppContext) {
    open_log_keeps_its_ids_and_fits(cx, "ru");
}

#[gpui_kit::test]
fn an_open_portuguese_log_keeps_its_ids_and_fits(cx: &mut TestAppContext) {
    open_log_keeps_its_ids_and_fits(cx, "pt-BR");
}

#[gpui_kit::test]
fn an_open_spanish_log_keeps_its_ids_and_fits(cx: &mut TestAppContext) {
    open_log_keeps_its_ids_and_fits(cx, "es");
}

fn type_into_add(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<LogsTfPage>,
    text: &str,
) {
    let input = cx.update(|cx| page.read(cx).add_input().clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.set_value(text, window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn the_logs_tf_page_and_its_dialogs_read_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes("/api/v1/log", list_body(&[]));
    let s = shell(cx, &env, &server.uri(), &[(1, "a"), (2, "b")]);
    let page = open_page(cx, &s);
    let shown = [ids::LOGSTF_CREDIT, ids::LOGSTF_REFRESH, ids::LOGSTF_ADD]
        .map(|id| support::label_of(cx, s.win, id));
    let want = ["Статистика с logs.tf", "Обновить", "+ Добавить игрока"];
    assert_eq!(shown, want.map(|t| Some(t.to_owned())));
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_chip_rename(sid(2))).as_deref(),
        Some("Ник для b")
    );
    support::click_id(cx, s.win, ids::logstf_chip_remove(sid(2)));
    assert_eq!(
        support::label_of(cx, s.win, ids::CONFIRM_TITLE).as_deref(),
        Some("Перестать отслеживать b?")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::CONFIRM_OK).as_deref(),
        Some("Перестать отслеживать")
    );
    support::click_id(cx, s.win, ids::CONFIRM_CANCEL);

    support::click_id(cx, s.win, ids::LOGSTF_ADD);
    assert_eq!(
        shown_text(cx, s.win, ids::LOGSTF_ADD_INPUT).as_deref(),
        Some("Ссылка на профиль или Steam ID")
    );
    type_into_add(cx, &s, &page, "has space");
    support::click_id(cx, s.win, ids::LOGSTF_ADD_CONFIRM);
    assert_eq!(
        support::label_of(cx, s.win, ids::LOGSTF_ADD_ERROR).as_deref(),
        Some("Это не похоже на ссылку на профиль Steam или Steam ID.")
    );
}

// logs.tf is kept while Settings shows, so its add fields retitle after a switch and keep
// what the player typed.
#[gpui_kit::test]
fn the_kept_logs_tf_page_follows_a_language_switch(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes("/api/v1/log", list_body(&[]));
    let s = shell(cx, &env, &server.uri(), &[(1, "a")]);
    let page = open_page(cx, &s);
    support::click_id(cx, s.win, ids::LOGSTF_ADD);
    type_into_add(cx, &s, &page, "gaben");
    assert_eq!(
        shown_text(cx, s.win, ids::LOGSTF_ADD_NICKNAME).as_deref(),
        Some("Nickname (optional)")
    );
    support::click_id(cx, s.win, ids::LOGSTF_ADD_CANCEL);
    crate::settings::open_settings(cx, &s);
    crate::settings::pick(cx, s.win, "ru");
    let _page = open_page(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::LOGSTF_ADD).as_deref(),
        Some("+ Добавить игрока")
    );
    support::click_id(cx, s.win, ids::LOGSTF_ADD);
    let placeholders =
        [ids::LOGSTF_ADD_INPUT, ids::LOGSTF_ADD_NICKNAME].map(|id| shown_text(cx, s.win, id));
    assert_eq!(
        placeholders,
        [
            Some("Ссылка на профиль или Steam ID".to_owned()),
            Some("Ник (по желанию)".to_owned())
        ]
    );
    assert_eq!(
        cx.update(|cx| page.read(cx).add_input().read(cx).value().to_string()),
        "gaben"
    );
    support::click_id(cx, s.win, ids::LOGSTF_ADD_CANCEL);
    // The rename field was built in English before the switch; a has no nickname, so it shows
    // its placeholder.
    support::click_id(cx, s.win, ids::logstf_chip_rename(sid(1)));
    assert_eq!(
        shown_text(cx, s.win, ids::LOGSTF_RENAME_INPUT).as_deref(),
        Some("Ник")
    );
}

// The page reloads when shown again, so the switch happens in place while the error is drawn.
#[gpui_kit::test]
fn a_kept_logs_page_error_follows_a_language_switch(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    support::rename_table(&env, "logs", "logs_away");
    let _page = open_logs(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::LOGS_ERROR).as_deref(),
        Some("TF2Tools couldn't open the activity log. Restart TF2Tools and try again.")
    );
    tf2tools_app::i18n::apply("ru");
    cx.update(|cx| cx.refresh_windows());
    assert_eq!(
        support::label_of(cx, s.win, ids::LOGS_ERROR).as_deref(),
        Some(
            "TF2Tools не удалось открыть журнал действий. Перезапустите TF2Tools и попробуйте снова."
        )
    );
}

// Returning to logs.tf restarts the sync, which rewords the line anyway, so the switch
// happens in place while the error is drawn.
#[gpui_kit::test]
fn a_kept_logs_tf_error_follows_a_language_switch(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
    let env = support::env();
    let s = shell(cx, &env, "http://127.0.0.1:9", &[(ARIE, "Arie")]);
    // The retry after an unreachable run comes at once, so the test waits on no clock.
    let sync = cx.update(tf2tools_app::logs_sync::logs_sync);
    cx.update(|cx| sync.update(cx, |s, _| s.set_retry_delay(std::time::Duration::ZERO)));
    let _page = open_page(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::LOGSTF_ERROR).as_deref(),
        Some("Couldn't reach logs.tf. Check your connection and try again.")
    );
    tf2tools_app::i18n::apply("ru");
    cx.update(|cx| cx.refresh_windows());
    assert_eq!(
        support::label_of(cx, s.win, ids::LOGSTF_ERROR).as_deref(),
        Some("Не удалось связаться с logs.tf. Проверьте подключение и попробуйте снова.")
    );
}

// logs.tf is kept while Settings shows, so an opened row whose fetch failed keeps its line
// and rewords it on return.
#[gpui_kit::test]
fn a_kept_logs_tf_open_error_follows_a_language_switch(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes("/api/v1/log", list_body(&[(3, day_ago(1))]));
    server.mount_status("/json/3", 503);
    let s = shell(cx, &env, &server.uri(), &[(ARIE, "Arie")]);
    let page = open_page(cx, &s);
    to_end(cx, &page);
    support::click_id(cx, s.win, ids::logstf_log(3));
    // The status line reads "Loading log..." until the fetch fails, so wait on the failure text.
    support::wait_real(cx, "the failure line", |cx| {
        support::label_of(cx, s.win, ids::logstf_log_status(3)).as_deref()
            == Some("logs.tf isn't responding right now. Try again later.")
    });
    crate::settings::open_settings(cx, &s);
    crate::settings::pick(cx, s.win, "ru");
    let page = open_page(cx, &s);
    to_end(cx, &page);
    support::settle(cx, s.win);
    assert_eq!(expanded(cx, &page), Some(3), "the row stays open");
    assert_eq!(
        support::label_of(cx, s.win, ids::logstf_log_status(3)).as_deref(),
        Some("logs.tf сейчас не отвечает. Попробуйте позже.")
    );
}

// The add dialog is modal, so the switch happens in place while its error is drawn.
#[gpui_kit::test]
fn a_kept_add_player_error_follows_a_language_switch(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes("/api/v1/log", list_body(&[]));
    let s = shell(cx, &env, &server.uri(), &[(1, "a")]);
    let page = open_page(cx, &s);
    support::click_id(cx, s.win, ids::LOGSTF_ADD);
    type_into_add(cx, &s, &page, "has space");
    support::click_id(cx, s.win, ids::LOGSTF_ADD_CONFIRM);
    assert_eq!(
        support::label_of(cx, s.win, ids::LOGSTF_ADD_ERROR).as_deref(),
        Some("That doesn't look like a Steam profile link or Steam ID.")
    );
    tf2tools_app::i18n::apply("ru");
    cx.update(|cx| cx.refresh_windows());
    assert_eq!(
        support::label_of(cx, s.win, ids::LOGSTF_ADD_ERROR).as_deref(),
        Some("Это не похоже на ссылку на профиль Steam или Steam ID.")
    );
}
