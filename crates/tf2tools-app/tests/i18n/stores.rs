//! The stores, their details windows, the download toasts and the update banner, in Russian.

use gpui_kit::TestAppContext;
use tf2tools_app::ids;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_app::store_grid::{SEARCH_DEBOUNCE, StoreGridConfig, sort_label};
use tf2tools_core::{StoreKind, StoreSort};

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
