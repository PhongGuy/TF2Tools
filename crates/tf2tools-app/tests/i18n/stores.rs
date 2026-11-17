//! The stores, their details windows, the download toasts and the update banner, in Russian.

use std::time::Duration;

// `update_window` is a trait method on the test context.
use gpui_kit::AppContext as _;
use gpui_kit::TestAppContext;
use tf2tools_app::gb_addon_installs::GbAddonOffer;
use tf2tools_app::ids;
use tf2tools_app::pages::addon_store::gamebanana_tab::{AddonTab, search_placeholder};
use tf2tools_app::pages::addon_store::model::AddonState;
use tf2tools_app::pages::addon_store::{InfoLink, info_text};
use tf2tools_app::pages::hud_store::HudStorePage;
use tf2tools_app::pages::hud_store::bodies::{curated_body, gamebanana_body};
use tf2tools_app::pages::sound_store::model::{Category, category_label};
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_app::store_detail::{DetailItem, open_detail};
use tf2tools_app::store_download::{download_failed_title, downloaded_title, stayed_description};
use tf2tools_app::store_grid::{SEARCH_DEBOUNCE, StoreGridConfig, sort_label};
use tf2tools_app::updater::UpdateState;
use tf2tools_core::{AddonSection, GbId, StoreItemId, StoreKind, StoreSort, Tf2Install};

use crate::LocaleGuard;
use crate::library::{shown_text, text_width};
use crate::support;
use crate::support::update_fake::{fake_updater, offer, reach_available, wait_state};

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

const MD: &str = "Generic_MostDownloaded";

fn addon_store(
    cx: &mut TestAppContext,
    env: &support::Env,
    server: &support::StoreServer,
) -> support::AddonStoreWindow {
    let state = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    support::addon_store_window(cx, env, server, state, |_, _| {})
}

fn open_addon_tab(
    cx: &mut TestAppContext,
    w: &support::AddonStoreWindow,
    key: &str,
) -> gpui_kit::Entity<AddonTab> {
    support::click_id(cx, w.win, ids::addon_store_tab(key));
    let section = AddonSection::by_key(key).unwrap();
    let tab = cx
        .update(|cx| w.page.read(cx).gamebanana(section))
        .expect("the tab is built on first show");
    support::wait_real(cx, "the first step", |cx| {
        cx.update(|cx| {
            let t = tab.read(cx);
            !t.is_loading() && (t.wants_load_more() || t.is_no_results() || !t.cards().is_empty())
        })
    });
    tab
}

#[gpui_kit::test]
fn the_addon_store_reads_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let (env, server) = (support::env(), support::StoreServer::start());
    // The only GUI is a HUD, which the GUIs tab leaves out, so the listing ends empty.
    server.mount_gb_index(
        1644,
        MD,
        1,
        support::gb_listing(vec![support::gb_addon(1, 1644, Some(1649))], true),
    );
    let w = addon_store(cx, &env, &server);
    assert_eq!(
        support::label_of(cx, w.win, ids::STORE_EMPTY).as_deref(),
        Some("Не удалось загрузить дополнения")
    );
    assert_eq!(
        support::label_of(cx, w.win, ids::STORE_REFRESH).as_deref(),
        Some("Обновить")
    );
    assert_eq!(
        support::label_of(cx, w.win, ids::addon_store_tab("mastercomfig")).as_deref(),
        Some("mastercomfig")
    );
    assert_eq!(
        support::label_of(cx, w.win, ids::addon_store_tab("skins")).as_deref(),
        Some("Скины")
    );

    let _tab = open_addon_tab(cx, &w, "guis");
    assert_eq!(
        shown_text(cx, w.win, ids::ADDON_SEARCH).as_deref(),
        Some("Поиск интерфейсов...")
    );
    assert_eq!(
        shown_text(cx, w.win, ids::ADDON_SORT).as_deref(),
        Some("Популярные")
    );
    assert_eq!(
        shown_text(cx, w.win, ids::ADDON_CATEGORY).as_deref(),
        Some("Все")
    );
    assert_eq!(
        support::label_of(cx, w.win, ids::ADDON_SORT_LABEL).as_deref(),
        Some("Сортировка:")
    );
    assert_eq!(
        support::label_of(cx, w.win, ids::ADDON_EMPTY).as_deref(),
        Some("Дополнения не найдены")
    );
    assert_eq!(
        support::label_of(cx, w.win, ids::STORE_EMPTY_HINT).as_deref(),
        Some("Попробуйте другую категорию.")
    );
    assert_rows_fit(
        cx,
        w.win,
        ids::ADDON_SORT,
        &["Популярные".into(), "Новые".into()],
    );
}

#[test]
fn addon_store_labels_read_in_russian() {
    let _guard = LocaleGuard::switch("ru");
    let states = [
        AddonState::Download,
        AddonState::InLibrary,
        AddonState::Installed,
        AddonState::Update,
    ]
    .map(|s| s.label().to_string());
    assert_eq!(
        states,
        ["Скачать", "В библиотеке", "Установлено", "Обновить"]
    );
    let offers = [
        GbAddonOffer::Install,
        GbAddonOffer::Update,
        GbAddonOffer::InstallAgain,
        GbAddonOffer::Installed,
    ]
    .map(|o| o.label().to_string());
    assert_eq!(
        offers,
        ["Установить", "Обновить", "Установить снова", "Установлено"]
    );
    assert_eq!(GbAddonOffer::Update.badge().as_deref(), Some("Обновление"));
    assert_eq!(
        GbAddonOffer::Installed.badge().as_deref(),
        Some("Установлено")
    );
    assert_eq!(GbAddonOffer::Install.badge(), None);
    let placeholders: Vec<String> = AddonSection::ALL
        .iter()
        .map(|s| search_placeholder(*s).to_string())
        .collect();
    assert_eq!(
        placeholders,
        [
            "Поиск скинов...",
            "Поиск эффектов...",
            "Поиск интерфейсов...",
            "Поиск текстур...",
            "Поиск файлов игры..."
        ]
    );
    // Each underlined word must occur in the translated sentence, or it loses its link.
    let text = info_text();
    for link in [InfoLink::Mastercomfig, InfoLink::Addons] {
        assert!(
            text.contains(&*link.word()),
            "{text} lacks {:?}",
            link.word()
        );
    }
}

// The add-on tab is kept with the Addon Store, so its search box and Selects retitle after a
// switch and the chosen sort stays.
#[gpui_kit::test]
fn the_addon_tab_follows_a_language_switch(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
    let (env, server) = (support::env(), support::StoreServer::start());
    let listing = |id| support::gb_listing(vec![support::gb_addon(id, 2774, None)], true);
    server.mount_gb_index(2774, MD, 1, listing(1));
    server.mount_gb_index(2774, "Generic_Newest", 1, listing(2));
    let w = addon_store(cx, &env, &server);
    let tab = open_addon_tab(cx, &w, "gamefiles");
    support::click_id(cx, w.win, ids::ADDON_SORT);
    support::click_id(cx, w.win, ids::select_option(ids::ADDON_SORT, "Newest"));
    support::wait_real(cx, "the newest step", |cx| {
        cx.update(|cx| tab.read(cx).cards().first().map(|c| c.id.get()) == Some(2))
    });
    let shown = |cx: &mut TestAppContext| {
        [ids::ADDON_SEARCH, ids::ADDON_SORT, ids::ADDON_CATEGORY]
            .map(|id| shown_text(cx, w.win, id))
    };
    let some = |t: [&str; 3]| t.map(|t| Some(t.to_owned()));
    assert_eq!(shown(cx), some(["Search Game files...", "Newest", "All"]));
    tf2tools_app::i18n::apply("ru");
    cx.update(|cx| cx.refresh_windows());
    assert_eq!(shown(cx), some(["Поиск файлов игры...", "Новые", "Все"]));
    assert_eq!(
        cx.update(|cx| tab.read(cx).sort()),
        tf2tools_core::AddonSort::Newest
    );
}

#[gpui_kit::test]
fn the_details_window_reads_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_files(
        7,
        &[("flawhud.zip", support::hud_zip_bytes("flawhud"))],
        Duration::ZERO,
    );
    let (win, _) = support::store_window(
        cx,
        &env,
        &server,
        |env| {
            support::seed_gb_huds(env, &[(7, "FlawHUD")]);
            // The GameBanana header button shows only for a row that has a profile page.
            rusqlite::Connection::open(env.data.db_path())
                .unwrap()
                .execute(
                    "UPDATE store_huds_gb SET profile_url = 'https://gamebanana.com/mods/7' WHERE gb_id = 7",
                    (),
                )
                .unwrap();
        },
        |_, _| support::Blank,
    );
    let item = DetailItem {
        kind: StoreKind::GameBananaHuds,
        id: gb(7),
        name: "FlawHUD".into(),
        in_library: false,
    };
    cx.update_window(win, |_, window, cx| {
        open_detail(item, gamebanana_body(), window, cx)
    })
    .unwrap();
    support::wait_real(cx, "the files and the link", |cx| {
        support::present(cx, win, ids::store_file(0))
            && support::present(cx, win, ids::store_detail_link("gamebanana"))
    });
    assert_eq!(
        support::label_of(cx, win, ids::store_file(0)).as_deref(),
        Some("Скачать")
    );
    assert_eq!(
        support::label_of(cx, win, ids::store_detail_link("gamebanana")).as_deref(),
        Some("GameBanana")
    );
}

#[gpui_kit::test]
fn a_curated_detail_in_the_library_reads_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let (env, server) = (support::env(), support::StoreServer::start());
    let repo = server.mount_curated("rayshud", support::hud_zip_bytes("rayshud"), Duration::ZERO);
    let json = r#"{"flags":["minmode"],"traits":{"os":["linux"]},"resources":["banner"]}"#;
    let (win, _) = support::store_window(
        cx,
        &env,
        &server,
        move |env| support::seed_curated(env, "rayshud", "rayshud", "raysfire", Some(&repo), json),
        |_, _| support::Blank,
    );
    let item = DetailItem {
        kind: StoreKind::CuratedHuds,
        id: StoreItemId::Curated("rayshud".into()),
        name: "rayshud".into(),
        in_library: true,
    };
    cx.update_window(win, |_, window, cx| {
        open_detail(item, curated_body(), window, cx)
    })
    .unwrap();
    support::wait_real(cx, "the archive and the link", |cx| {
        support::present(cx, win, ids::store_file(0))
            && support::present(cx, win, ids::store_detail_link("repo"))
    });
    assert_eq!(
        support::label_of(cx, win, ids::STORE_DETAIL_BADGE).as_deref(),
        Some("Скачано")
    );
    assert_eq!(
        support::label_of(cx, win, ids::store_file(0)).as_deref(),
        Some("Скачать снова")
    );
    assert_eq!(
        support::label_of(cx, win, ids::store_detail_link("repo")).as_deref(),
        Some("Репозиторий")
    );
}

#[test]
fn download_toasts_read_in_russian() {
    let _guard = LocaleGuard::switch("ru");
    let kinds = [
        StoreKind::GameBananaHuds,
        StoreKind::Sounds,
        StoreKind::Crosshairs,
        StoreKind::Addons,
    ];
    assert_eq!(
        kinds.map(|k| downloaded_title(k).to_string()),
        [
            "HUD скачан",
            "Звук скачан",
            "Прицел скачан",
            "Дополнение скачано"
        ]
    );
    assert_eq!(
        kinds.map(|k| download_failed_title(k).to_string()),
        [
            "Не удалось скачать HUD",
            "Не удалось скачать звук",
            "Не удалось скачать прицел",
            "Не удалось скачать дополнение"
        ]
    );
    assert_eq!(
        stayed_description("Попробуйте снова.", &["a".into(), "b".into()]),
        "Попробуйте снова. Всё ещё в библиотеке: a, b."
    );
}

#[gpui_kit::test]
fn the_addon_detail_reads_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let (env, server) = (support::env(), support::StoreServer::start());
    server.mount_gb_index(
        587,
        MD,
        1,
        support::gb_listing(vec![support::gb_addon(8, 587, None)], true),
    );
    server.mount_gb_profile(
        8,
        serde_json::json!({
            "_idRow": 8, "_sName": "Addon 8", "_sText": "<p>Hello</p>", "_nDownloadCount": 12, "_bIsObsolete": true,
            "_aSubmitter": {"_sName": "vex"}, "_sProfileUrl": "https://gamebanana.com/mods/8",
            "_aContentRatings": {"sa": "Skimpy Attire"},
            "_aFiles": [{"_idRow": 2, "_sFile": "new.zip", "_nFilesize": 2048, "_tsDateAdded": 200, "_sAvResult": "clean", "_sDownloadUrl": "https://gamebanana.com/dl/2"}]
        }),
    );
    let w = addon_store(cx, &env, &server);
    cx.update(|cx| tf2tools_app::pages::profiles::probe::Tf2Probe::install(cx, || false));
    let _tab = open_addon_tab(cx, &w, "textures");
    support::click_id(cx, w.win, ids::addon_gb_card("textures", GbId::new(8)));
    support::wait_real(cx, "the profile", |cx| {
        support::present(cx, w.win, ids::ADDON_DETAIL_DOWNLOADS)
    });
    assert_eq!(
        support::label_of(cx, w.win, ids::ADDON_DETAIL_RATINGS).as_deref(),
        Some("Для взрослых: Skimpy Attire")
    );
    assert_eq!(
        support::label_of(cx, w.win, ids::ADDON_DETAIL_OUTDATED).as_deref(),
        Some("Устарело")
    );
    assert_eq!(
        support::label_of(cx, w.win, ids::ADDON_DETAIL_INSTALL).as_deref(),
        Some("Установить")
    );
    assert_eq!(
        support::label_of(cx, w.win, ids::store_detail_link("gamebanana")).as_deref(),
        Some("Открыть на GameBanana")
    );
}

#[gpui_kit::test]
fn the_update_banner_states_read_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let (u, fake) = fake_updater(cx, true);
    let s = support::shell_window(cx, &env, missing());
    let label = |cx: &mut TestAppContext, id: &'static str| support::label_of(cx, s.win, id);
    reach_available(cx, &u, &fake, offer("6.0.1", true));
    assert_eq!(
        label(cx, ids::UPDATE_BANNER_TEXT).as_deref(),
        Some("Доступно обновление v6.0.1")
    );
    assert_eq!(
        label(cx, ids::UPDATE_BANNER_ACTION).as_deref(),
        Some("Обновить")
    );
    support::click_id(cx, s.win, ids::UPDATE_BANNER_ACTION);
    let (sink, reply) = fake.next_download(cx);
    sink.report(42, 100);
    support::next_repaint(cx);
    assert_eq!(
        label(cx, ids::UPDATE_BANNER_TEXT).as_deref(),
        Some("Загрузка...")
    );
    drop(sink);
    reply.send(Ok(())).unwrap();
    wait_state(cx, &u, "Ready", |st| matches!(st, UpdateState::Ready(_)));
    assert_eq!(
        label(cx, ids::UPDATE_BANNER_TEXT).as_deref(),
        Some("Обновление готово — перезапустите, чтобы применить")
    );
    assert_eq!(
        label(cx, ids::UPDATE_BANNER_ACTION).as_deref(),
        Some("Перезапустить")
    );
}
