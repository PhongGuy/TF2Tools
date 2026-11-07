//! The Maps page: serveme.tf's list from a local fake, search and sort, and the download,
//! in-game and remove rules in the real shell.

use crate::support;

use gpui_kit::{AppContext as _, Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::maps::model::MapSort;
use tf2tools_app::pages::maps::{MapListSource, MapsPage};
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{MapState, Tf2Install};

const LIST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../tf2tools-core/tests/fixtures/store/serveme-maps.html"
));

fn found(env: &support::Env) -> Tf2State {
    std::fs::create_dir_all(env.game.join("tf/maps")).unwrap();
    Tf2State::Found(Tf2Install::from_path(&env.game).unwrap())
}

fn map_file(env: &support::Env, name: &str) -> std::path::PathBuf {
    env.game.join("tf/maps").join(format!("{name}.bsp"))
}

fn bsp() -> Vec<u8> {
    let mut body = b"VBSP".to_vec();
    body.resize(2048, 1);
    body
}

/// A shell whose store talks to `server`, which serves the fixture list, with the list
/// source installed as the app does at startup.
fn shell(
    cx: &mut TestAppContext,
    env: &support::Env,
    server: &support::StoreServer,
    state: Tf2State,
) -> support::ShellWindow {
    cx.executor().allow_parking();
    server.mount_bytes("/serveme/maps", LIST.as_bytes().to_vec());
    let uri = server.uri();
    let endpoints = server
        .endpoints()
        .with_serveme(format!("{uri}/serveme"))
        .with_map_downloads(format!("{uri}/fastdl/maps"));
    let s = support::shell_window_tweaked(cx, env, state, |services| {
        *services = services.clone().with_endpoints(endpoints);
    });
    let store = s.services.store.clone();
    cx.update(|cx| MapListSource::install(cx, store));
    s
}

/// Opens Maps through the sidebar and waits for the fetched list to show.
fn open_maps(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<MapsPage> {
    support::click_id(cx, s.win, ids::nav(Page::Maps));
    let page = cx
        .update(|cx| s.shell.read(cx).maps_page())
        .expect("a Maps page");
    support::wait_real(cx, "the map list", |cx| {
        cx.update(|cx| {
            let p = page.read(cx);
            p.is_loaded() && !p.is_syncing() && !p.shown_names().is_empty()
        })
    });
    support::settle(cx, s.win);
    page
}

fn names(cx: &mut TestAppContext, page: &Entity<MapsPage>) -> Vec<String> {
    cx.update(|cx| page.read(cx).shown_names())
}

fn state(cx: &mut TestAppContext, page: &Entity<MapsPage>, name: &str) -> Option<MapState> {
    cx.update(|cx| page.read(cx).state_of(name))
}

#[gpui_kit::test]
fn maps_sits_under_the_makers_and_lists_serveme_maps(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let s = shell(cx, &env, &server, Tf2State::Missing { saved: None });
    let page = open_maps(cx, &s);
    let order: Vec<Page> = Page::ALL.to_vec();
    let maps_at = order.iter().position(|p| *p == Page::Maps).unwrap();
    assert_eq!(order[maps_at - 1], Page::HudMaker);
    assert_eq!(
        support::label_of(cx, s.win, ids::HEADER_TITLE).as_deref(),
        Some("Maps")
    );
    assert_eq!(
        names(cx, &page),
        [
            "123_okwtfisthis",
            "1Bathrooms",
            "1cp_afkd",
            "1cp_cage",
            "ze_gris_skial_5"
        ]
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::MAPS_COUNT).as_deref(),
        Some("5 maps")
    );
    assert!(support::present(cx, s.win, ids::map_row("1cp_cage")));
    assert!(support::present(cx, s.win, ids::MAPS_NO_TF2));
    assert!(support::disabled_of(
        cx,
        s.win,
        ids::map_download("1cp_cage")
    ));
}

#[gpui_kit::test]
fn search_and_sort_reorder_the_list(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let s = shell(cx, &env, &server, Tf2State::Missing { saved: None });
    let page = open_maps(cx, &s);

    cx.update(|cx| page.update(cx, |p, cx| p.set_sort(MapSort::MostPlayed, cx)));
    assert_eq!(
        names(cx, &page),
        [
            "123_okwtfisthis",
            "1cp_cage",
            "ze_gris_skial_5",
            "1Bathrooms",
            "1cp_afkd"
        ]
    );

    let input = cx.update(|cx| page.read(cx).search_input().clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.set_value("1CP", window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
    assert_eq!(names(cx, &page), ["1cp_cage", "1cp_afkd"]);
    assert_eq!(
        support::label_of(cx, s.win, ids::MAPS_COUNT).as_deref(),
        Some("2 of 5 maps")
    );
}

#[gpui_kit::test]
fn download_then_remove_a_map(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes("/fastdl/maps/1cp_cage.bsp", bsp());
    let s = shell(cx, &env, &server, found(&env));
    let page = open_maps(cx, &s);

    support::click_id(cx, s.win, ids::map_download("1cp_cage"));
    support::wait_real(cx, "the download", |cx| {
        cx.update(|cx| page.read(cx).state_of("1cp_cage")) == Some(MapState::Downloaded)
    });
    support::settle(cx, s.win);
    assert_eq!(std::fs::read(map_file(&env, "1cp_cage")).unwrap(), bsp());
    assert_eq!(
        support::label_of(cx, s.win, ids::map_state("1cp_cage")).as_deref(),
        Some("Downloaded")
    );

    support::click_id(cx, s.win, ids::map_remove("1cp_cage"));
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    support::wait_real(cx, "the removal", |cx| {
        cx.update(|cx| page.read(cx).state_of("1cp_cage")) == Some(MapState::Available)
    });
    assert!(!map_file(&env, "1cp_cage").exists());
}

#[gpui_kit::test]
fn a_map_the_game_already_has_shows_in_game_without_actions(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let game = found(&env);
    std::fs::write(map_file(&env, "1cp_afkd"), b"VBSP mine").unwrap();
    let s = shell(cx, &env, &server, game);
    let page = open_maps(cx, &s);
    assert_eq!(state(cx, &page, "1cp_afkd"), Some(MapState::InGame));
    assert_eq!(
        support::label_of(cx, s.win, ids::map_state("1cp_afkd")).as_deref(),
        Some("In game")
    );
    assert!(!support::present(cx, s.win, ids::map_download("1cp_afkd")));
    assert!(!support::present(cx, s.win, ids::map_remove("1cp_afkd")));
    assert_eq!(
        std::fs::read(map_file(&env, "1cp_afkd")).unwrap(),
        b"VBSP mine"
    );
}

#[gpui_kit::test]
fn without_a_list_source_the_page_never_fetches(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    support::click_id(cx, s.win, ids::nav(Page::Maps));
    let page = cx.update(|cx| s.shell.read(cx).maps_page()).unwrap();
    support::wait_real(cx, "the cached list", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    assert!(!cx.update(|cx| page.read(cx).is_syncing()));
    assert!(support::present(cx, s.win, ids::MAPS_EMPTY));
}
