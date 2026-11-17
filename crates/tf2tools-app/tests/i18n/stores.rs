//! The stores, their details windows, the download toasts and the update banner, in Russian.

use gpui_kit::TestAppContext;
use tf2tools_app::ids;
use tf2tools_app::pages::hud_store::HudStorePage;
use tf2tools_app::pages::sound_store::model::{Category, category_label};
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_app::store_grid::{SEARCH_DEBOUNCE, StoreGridConfig, sort_label};
use tf2tools_core::{GbId, StoreItemId, StoreKind, StoreSort};

use crate::LocaleGuard;
use crate::library::{shown_text, text_width};
use crate::support;

// The kit's medium Select trigger around its title: two 1 px borders, 10 px padding a side,
// a 4 px gap and the 16 px caret.
const SELECT_CHROME: gpui_kit::Pixels = gpui_kit::px(42.);

fn missing() -> Tf2State {
    Tf2State::Missing { saved: None }
}

// The widest of `labels` at the Select's text-sm fits the select `id`'s text room.
fn assert_rows_fit(
    cx: &mut TestAppContext,
    win: gpui_kit::AnyWindowHandle,
    id: &'static str,
    labels: &[String],
) {
    let select = support::bounds_of(cx, win, id).unwrap().size.width;
    for label in labels {
        let w = text_width(cx, win, label, 0.875, gpui_kit::FontWeight::NORMAL);
        assert!(
            w + SELECT_CHROME <= select,
            "{label:?} is {w:?} in a {select:?} select"
        );
    }
}

#[gpui_kit::test]
fn the_store_grid_reads_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let config = StoreGridConfig::new(StoreKind::Crosshairs);
    let (win, grid) = support::store_grid_window(cx, &env, config, |env| {
        support::seed_crosshairs(env, 1..=3, 0, false)
    });
    support::wait_real(cx, "the grid", |cx| {
        cx.update(|cx| grid.read(cx).is_loaded() && !grid.read(cx).is_loading())
    });
    assert_eq!(
        shown_text(cx, win, ids::STORE_SEARCH).as_deref(),
        Some("Поиск прицелов...")
    );
    assert_eq!(
        shown_text(cx, win, ids::STORE_SORT).as_deref(),
        Some("Лайки")
    );
    assert_eq!(
        support::label_of(cx, win, ids::STORE_SORT_LABEL).as_deref(),
        Some("Сортировка:")
    );
    let rows: Vec<String> = [
        StoreSort::MostLiked,
        StoreSort::MostViewed,
        StoreSort::Newest,
    ]
    .map(|s| sort_label(s).to_string())
    .to_vec();
    assert_eq!(rows, ["Лайки", "Просмотры", "Новые"]);
    assert_rows_fit(cx, win, ids::STORE_SORT, &rows);

    cx.update(|cx| grid.update(cx, |g, cx| g.search_typed("zzzq".into(), cx)));
    cx.executor().advance_clock(SEARCH_DEBOUNCE);
    support::wait_real(cx, "the search", |cx| {
        cx.update(|cx| grid.read(cx).shown() == Some(0) && !grid.read(cx).is_loading())
    });
    assert_eq!(
        support::label_of(cx, win, ids::STORE_EMPTY).as_deref(),
        Some("Нет прицелов по этому запросу")
    );
    assert_eq!(
        support::label_of(cx, win, ids::STORE_EMPTY_HINT).as_deref(),
        Some("Попробуйте изменить запрос.")
    );
}

#[gpui_kit::test]
fn an_empty_russian_store_says_it_could_not_load(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let config = StoreGridConfig::new(StoreKind::Sounds);
    let (win, grid) = support::store_grid_window(cx, &env, config, |_| {});
    support::wait_real(cx, "the grid", |cx| {
        cx.update(|cx| grid.read(cx).is_loaded() && !grid.read(cx).is_loading())
    });
    assert_eq!(
        support::label_of(cx, win, ids::STORE_UNLOADED).as_deref(),
        Some("Не удалось загрузить звуки")
    );
    assert_eq!(
        support::label_of(cx, win, ids::STORE_UNLOADED_RETRY).as_deref(),
        Some("Повторить")
    );
}

// Stores stay built while another page shows, so the grid's search box and sort Select must
// take the new language when the player switches in Settings and comes back.
#[gpui_kit::test]
fn a_kept_store_grid_follows_a_language_switch(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    support::seed_sounds(
        &env,
        &[
            (1, "Ding", "hitsound", None),
            (2, "Boom", "killsound", None),
        ],
    );
    support::click_id(cx, s.win, ids::nav(Page::SoundStore));
    let page = cx
        .update(|cx| s.shell.read(cx).sound_store_page())
        .expect("the Sound Store");
    let grid = cx.update(|cx| page.read(cx).grid().clone());
    support::wait_real(cx, "the grid", |cx| {
        cx.update(|cx| grid.read(cx).is_loaded())
    });
    support::click_id(cx, s.win, ids::STORE_SORT);
    support::click_id(
        cx,
        s.win,
        ids::select_option(ids::STORE_SORT, "Most Viewed"),
    );
    assert_eq!(cx.update(|cx| grid.read(cx).sort()), StoreSort::MostViewed);
    let shown = |cx: &mut TestAppContext| {
        [ids::STORE_SEARCH, ids::STORE_SORT].map(|id| shown_text(cx, s.win, id))
    };
    assert_eq!(
        shown(cx),
        [
            Some("Search sounds...".to_owned()),
            Some("Most Viewed".to_owned())
        ]
    );

    crate::settings::open_settings(cx, &s);
    crate::settings::pick(cx, s.win, "ru");
    support::click_id(cx, s.win, ids::nav(Page::SoundStore));
    assert_eq!(
        shown(cx),
        [
            Some("Поиск звуков...".to_owned()),
            Some("Просмотры".to_owned())
        ]
    );
    assert_eq!(cx.update(|cx| grid.read(cx).sort()), StoreSort::MostViewed);
}

fn gb(id: u64) -> StoreItemId {
    StoreItemId::GameBanana(GbId::new(id))
}

// budhud on every OS, so the running OS's preset filter keeps it; FlawHUD (7) on GameBanana.
fn hud_store(
    cx: &mut TestAppContext,
    env: &support::Env,
    server: &support::StoreServer,
) -> (gpui_kit::AnyWindowHandle, gpui_kit::Entity<HudStorePage>) {
    let all_os = r#"{"flags":["menus"],"traits":{"os":["windows","mac","linux"]}}"#;
    let (win, page) = support::store_window(
        cx,
        env,
        server,
        |env| {
            support::seed_curated(env, "budhud", "budhud", "Whisker", None, all_os);
            support::seed_gb_huds(env, &[(7, "FlawHUD")]);
        },
        HudStorePage::new,
    );
    let curated = cx
        .update(|cx| page.read(cx).curated())
        .expect("the curated tab");
    support::wait_real(cx, "the curated list", |cx| {
        cx.update(|cx| curated.read(cx).is_loaded())
    });
    (win, page)
}

#[gpui_kit::test]
fn the_hud_store_reads_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let (env, server) = (support::env(), support::StoreServer::start());
    let (win, page) = hud_store(cx, &env, &server);
    let texts = [
        (ids::HUD_STORE_TAB_CURATED, "Подборка"),
        (ids::HUD_STORE_TAB_GAMEBANANA, "GameBanana"),
        (ids::HUD_STORE_CURATED_SOURCE, "Подборка HUD из hud-db."),
        (ids::STORE_FILTER_OS_LABEL, "ОС:"),
        (ids::STORE_FILTER_MORE, "Ещё фильтры"),
    ];
    for (id, text) in texts {
        assert_eq!(
            support::label_of(cx, win, id).as_deref(),
            Some(text),
            "{id}"
        );
    }
    assert_eq!(
        shown_text(cx, win, ids::STORE_SEARCH).as_deref(),
        Some("Поиск HUD...")
    );
    let budhud = StoreItemId::Curated("budhud".into());
    assert_eq!(
        support::label_of(
            cx,
            win,
            ids::store_download(StoreKind::CuratedHuds, &budhud)
        )
        .as_deref(),
        Some("Скачать")
    );

    support::click_id(cx, win, ids::HUD_STORE_TAB_GAMEBANANA);
    let tab = cx
        .update(|cx| page.read(cx).gamebanana())
        .expect("the GameBanana tab");
    support::wait_real(cx, "the grid", |cx| {
        cx.update(|cx| tab.read(cx).grid().read(cx).is_loaded())
    });
    assert_eq!(
        support::label_of(
            cx,
            win,
            ids::store_download(StoreKind::GameBananaHuds, &gb(7))
        )
        .as_deref(),
        Some("Скачать")
    );
    assert_eq!(
        shown_text(cx, win, ids::STORE_SORT).as_deref(),
        Some("Лайки")
    );
}

// The curated tab is kept with the HUD Store, so its search box retitles after a switch.
#[gpui_kit::test]
fn the_curated_search_follows_a_language_switch(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
    let (env, server) = (support::env(), support::StoreServer::start());
    let (win, _page) = hud_store(cx, &env, &server);
    assert_eq!(
        shown_text(cx, win, ids::STORE_SEARCH).as_deref(),
        Some("Search HUDs...")
    );
    tf2tools_app::i18n::apply("ru");
    cx.update(|cx| cx.refresh_windows());
    assert_eq!(
        shown_text(cx, win, ids::STORE_SEARCH).as_deref(),
        Some("Поиск HUD...")
    );
}

#[gpui_kit::test]
fn the_sound_store_reads_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    support::seed_sounds(
        &env,
        &[(1, "Ding", "hitsound", Some("https://x.invalid/1.mp3"))],
    );
    support::click_id(cx, s.win, ids::nav(Page::SoundStore));
    let page = cx
        .update(|cx| s.shell.read(cx).sound_store_page())
        .expect("the Sound Store");
    let grid = cx.update(|cx| page.read(cx).grid().clone());
    support::wait_real(cx, "the grid", |cx| {
        cx.update(|cx| grid.read(cx).is_loaded())
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::HEADER_TITLE).as_deref(),
        Some("Магазин звуков")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::SOUND_STORE_SOURCE).as_deref(),
        Some("Хитсаунды и килсаунды с GameBanana.")
    );
    assert_eq!(
        shown_text(cx, s.win, ids::STORE_SEARCH).as_deref(),
        Some("Поиск звуков...")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::store_download(StoreKind::Sounds, &gb(1))).as_deref(),
        Some("Скачать")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::store_play(StoreKind::Sounds, &gb(1))).as_deref(),
        Some("Воспроизвести")
    );
}

#[test]
fn sound_store_labels_read_in_russian() {
    let _guard = LocaleGuard::switch("ru");
    let pills: Vec<String> = Category::ALL
        .iter()
        .map(|c| c.label().to_string())
        .collect();
    assert_eq!(pills, ["Все", "Хитсаунды", "Килсаунды"]);
    assert_eq!(category_label(Some("hitsound")), "Хитсаунд");
    assert_eq!(category_label(Some("killsound")), "Килсаунд");
    assert_eq!(category_label(None), "Звук");
}
