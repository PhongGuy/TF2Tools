//! The stores and the Activity Log outlive a visit to another page: leaving and coming back
//! finds their tab, search, filters, sort, category and scroll as the player left them, while
//! what changed meanwhile (the library, a sync, new log rows) still shows on return.

mod support;

use std::time::Duration;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext, point, px};
use tf2tools_app::ids;
use tf2tools_app::library_events::LibraryEvents;
use tf2tools_app::pages::hud_store::curated::{Facet, Os};
use tf2tools_app::pages::hud_store::gamebanana_tab::GameBananaTab;
use tf2tools_app::pages::hud_store::{HudStorePage, HudStoreTab};
use tf2tools_app::pages::sound_store::model::Category;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_app::store_download::StoreDownloads;
use tf2tools_app::store_grid::SEARCH_DEBOUNCE;
use tf2tools_app::sync::SyncSummary;
use tf2tools_core::{
    GbId, HudEntry, LogLevel, NewLogEntry, Provenance, StoreItemId, StoreKind, StoreSort,
    Tf2Install,
};

const GB: StoreKind = StoreKind::GameBananaHuds;

fn missing() -> Tf2State {
    Tf2State::Missing { saved: None }
}

fn gb(id: u64) -> StoreItemId {
    StoreItemId::GameBanana(GbId::new(id))
}

// A trip through the HUDs page and back to `page` by the sidebar.
fn away_and_back(cx: &mut TestAppContext, s: &support::ShellWindow, page: Page) {
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    support::click_id(cx, s.win, ids::nav(page));
}

fn hud_store(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<HudStorePage> {
    cx.update(|cx| s.shell.read(cx).hud_store_page())
        .expect("the HUD Store")
}

fn gamebanana(cx: &mut TestAppContext, page: &Entity<HudStorePage>) -> Entity<GameBananaTab> {
    let tab = cx
        .update(|cx| page.read(cx).gamebanana())
        .expect("the GameBanana tab");
    support::wait_real(cx, "the grid", |cx| {
        cx.update(|cx| tab.read(cx).grid().read(cx).is_loaded())
    });
    tab
}

fn card_names(cx: &mut TestAppContext, tab: &Entity<GameBananaTab>) -> Vec<String> {
    cx.update(|cx| {
        tab.read(cx)
            .grid()
            .read(cx)
            .cards()
            .iter()
            .map(|c| c.name.clone())
            .collect()
    })
}

// Imports a HUD archive named `name` into the library with `provenance`.
fn import(env: &support::Env, s: &support::ShellWindow, name: &str, provenance: Provenance) {
    let archive = env.tmp.path().join(format!("{name}.zip"));
    std::fs::write(&archive, support::hud_zip_bytes(name)).unwrap();
    s.services.library.import_hud(&archive, provenance).unwrap();
}

fn library_hud(s: &support::ShellWindow, gb_id: u64) -> HudEntry {
    s.services
        .library
        .list_huds()
        .unwrap()
        .into_iter()
        .find(|h| {
            matches!(h.provenance(), Provenance::GameBanana { gb_id: id, .. } if *id == GbId::new(gb_id))
        })
        .expect("the HUD in the library")
}

#[gpui_kit::test]
fn the_hud_store_keeps_its_tab_search_sort_and_curated_filters(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    support::seed_gb_huds(&env, &[(7, "FlawHUD"), (8, "ToonHUD"), (9, "FlawHUD Lite")]);
    support::seed_curated(
        &env,
        "rayshud",
        "rayshud",
        "raysfire",
        None,
        r#"{"flags":["menus"],"traits":{"os":["windows","mac","linux"]}}"#,
    );
    support::click_id(cx, s.win, ids::nav(Page::HudStore));
    let page = hud_store(cx, &s);
    let curated = cx.update(|cx| page.read(cx).curated()).expect("Curated");
    support::wait_real(cx, "the curated HUDs", |cx| {
        cx.update(|cx| curated.read(cx).is_loaded())
    });
    support::click_id(cx, s.win, ids::store_filter_os(Os::Mac.key()));
    support::click_id(cx, s.win, ids::STORE_FILTER_MORE);
    support::click_id(cx, s.win, ids::store_filter(Facet::Flag.key(), "menus"));
    cx.update(|cx| curated.update(cx, |t, cx| t.set_search("rays".into(), cx)));
    let filter = cx.update(|cx| curated.read(cx).filter().clone());

    support::click_id(cx, s.win, ids::HUD_STORE_TAB_GAMEBANANA);
    let tab = gamebanana(cx, &page);
    let grid = cx.update(|cx| tab.read(cx).grid().clone());
    cx.update(|cx| grid.update(cx, |g, cx| g.set_sort(StoreSort::Newest, cx)));
    cx.update(|cx| grid.update(cx, |g, cx| g.search_typed("Flaw".into(), cx)));
    cx.executor().advance_clock(SEARCH_DEBOUNCE);
    support::wait_real(cx, "the search", |cx| {
        card_names(cx, &tab) == ["FlawHUD", "FlawHUD Lite"]
    });

    away_and_back(cx, &s, Page::HudStore);
    let again = hud_store(cx, &s);
    assert_eq!(again.entity_id(), page.entity_id(), "the same page");
    assert_eq!(
        cx.update(|cx| again.read(cx).tab()),
        HudStoreTab::GameBanana
    );
    assert!(support::present(cx, s.win, ids::store_card(GB, &gb(7))));
    assert_eq!(cx.update(|cx| grid.read(cx).sort()), StoreSort::Newest);
    assert_eq!(cx.update(|cx| grid.read(cx).search().to_owned()), "Flaw");
    assert_eq!(card_names(cx, &tab), ["FlawHUD", "FlawHUD Lite"]);
    support::click_id(cx, s.win, ids::HUD_STORE_TAB_CURATED);
    assert_eq!(cx.update(|cx| curated.read(cx).filter().clone()), filter);
    assert_eq!(
        support::checked_of(cx, s.win, ids::store_filter_os(Os::Mac.key())),
        Some(filter.os.contains(&Os::Mac))
    );
}

#[gpui_kit::test]
fn the_sound_store_keeps_its_category_and_sort(cx: &mut TestAppContext) {
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
    support::click_id(cx, s.win, ids::store_category(Category::Killsounds.slug()));
    let grid = cx.update(|cx| page.read(cx).grid().clone());
    cx.update(|cx| grid.update(cx, |g, cx| g.set_sort(StoreSort::Newest, cx)));

    away_and_back(cx, &s, Page::SoundStore);
    let again = cx
        .update(|cx| s.shell.read(cx).sound_store_page())
        .expect("the Sound Store again");
    assert_eq!(again.entity_id(), page.entity_id(), "the same page");
    assert_eq!(
        cx.update(|cx| again.read(cx).category()),
        Category::Killsounds
    );
    assert_eq!(
        support::selected_of(cx, s.win, ids::store_category(Category::Killsounds.slug())),
        Some(true)
    );
    assert_eq!(cx.update(|cx| grid.read(cx).sort()), StoreSort::Newest);
    assert_eq!(
        cx.update(|cx| grid.read(cx).category().map(str::to_owned)),
        Some("killsound".to_owned())
    );
}

#[gpui_kit::test]
fn the_crosshair_store_keeps_its_sort(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    support::seed_crosshairs(&env, 1..=3, 0, false);
    support::click_id(cx, s.win, ids::nav(Page::CrosshairStore));
    let page = cx
        .update(|cx| s.shell.read(cx).crosshair_store_page())
        .expect("the Crosshair Store");
    let grid = cx.update(|cx| page.read(cx).grid().clone());
    cx.update(|cx| grid.update(cx, |g, cx| g.set_sort(StoreSort::Newest, cx)));

    away_and_back(cx, &s, Page::CrosshairStore);
    let again = cx
        .update(|cx| s.shell.read(cx).crosshair_store_page())
        .expect("the Crosshair Store again");
    assert_eq!(again.entity_id(), page.entity_id(), "the same page");
    assert_eq!(cx.update(|cx| grid.read(cx).sort()), StoreSort::Newest);
}

// The Addon Store is kept, and rereads the library on return, since the Addons page may have
// changed it meanwhile.
#[gpui_kit::test]
fn the_addon_store_is_kept_and_rereads_on_return(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    support::click_id(cx, s.win, ids::nav(Page::AddonStore));
    let page = cx
        .update(|cx| s.shell.read(cx).addon_store_page())
        .expect("the Addon Store");
    support::wait_real(cx, "the first load", |cx| {
        cx.update(|cx| page.read(cx).reloads() == 1)
    });

    away_and_back(cx, &s, Page::AddonStore);
    let again = cx
        .update(|cx| s.shell.read(cx).addon_store_page())
        .expect("the Addon Store again");
    assert_eq!(again.entity_id(), page.entity_id(), "the same page");
    support::wait_real(cx, "the reload on return", |cx| {
        cx.update(|cx| page.read(cx).reloads() == 2)
    });
}

#[gpui_kit::test]
fn the_activity_log_keeps_its_filters_and_shows_rows_logged_while_away(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    let append = |level, cat: &str, msg: &str| {
        s.services
            .activity
            .append(&NewLogEntry::new(level, cat, "test", msg))
            .unwrap();
    };
    append(LogLevel::Error, "hud", "first");
    append(LogLevel::Info, "hud", "quiet");
    support::click_id(cx, s.win, ids::nav(Page::Logs));
    let page = cx
        .update(|cx| s.shell.read(cx).logs_page())
        .expect("the Logs page");
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            p.set_category(Some("hud"), cx);
            p.set_level(Some(LogLevel::Error), cx);
        })
    });
    support::wait_real(cx, "the filtered load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });

    support::click_id(cx, s.win, ids::nav(Page::Huds));
    append(LogLevel::Error, "hud", "second");
    support::click_id(cx, s.win, ids::nav(Page::Logs));
    let again = cx
        .update(|cx| s.shell.read(cx).logs_page())
        .expect("the Logs page again");
    assert_eq!(again.entity_id(), page.entity_id(), "the same page");
    support::wait_real(cx, "the row logged while away", |cx| {
        cx.update(|cx| page.read(cx).is_loaded() && page.read(cx).entries().len() == 2)
    });
    assert_eq!(cx.update(|cx| page.read(cx).category()), Some("hud"));
    assert_eq!(cx.update(|cx| page.read(cx).level()), Some(LogLevel::Error));
}

#[gpui_kit::test]
fn the_gamebanana_grid_keeps_its_scroll(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    let rows: Vec<(i64, String)> = (1..=40).map(|id| (id, format!("HUD {id}"))).collect();
    let rows: Vec<(i64, &str)> = rows.iter().map(|(id, n)| (*id, n.as_str())).collect();
    support::seed_gb_huds(&env, &rows);
    support::click_id(cx, s.win, ids::nav(Page::HudStore));
    let page = hud_store(cx, &s);
    support::click_id(cx, s.win, ids::HUD_STORE_TAB_GAMEBANANA);
    let tab = gamebanana(cx, &page);
    support::settle(cx, s.win);
    let handle = |cx: &mut TestAppContext, tab: &Entity<GameBananaTab>| {
        cx.update(|cx| {
            tab.read(cx)
                .grid()
                .read(cx)
                .card_grid()
                .scroll_handle()
                .0
                .borrow()
                .base_handle
                .clone()
        })
    };
    handle(cx, &tab).set_offset(point(px(0.), px(-900.)));
    support::settle(cx, s.win);
    let scrolled = handle(cx, &tab).offset();
    assert!(scrolled.y < px(0.), "the grid scrolled: {scrolled:?}");
    let first = cx.update(|cx| tab.read(cx).grid().read(cx).first_visible_index());
    assert!(first > Some(0), "a later row is first: {first:?}");

    away_and_back(cx, &s, Page::HudStore);
    support::settle(cx, s.win);
    let page = hud_store(cx, &s);
    let tab = gamebanana(cx, &page);
    assert_eq!(handle(cx, &tab).offset(), scrolled);
    assert_eq!(
        cx.update(|cx| tab.read(cx).grid().read(cx).first_visible_index()),
        first
    );
}

// The HUD library changed while the store was away (one HUD imported, one deleted): the
// GameBanana cards say so on return.
#[gpui_kit::test]
fn a_library_change_while_away_shows_on_the_store_cards(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    support::seed_gb_huds(&env, &[(7, "FlawHUD"), (8, "ToonHUD")]);
    let from_gb = |id| Provenance::GameBanana {
        gb_id: GbId::new(id),
        author: "vex".into(),
    };
    import(&env, &s, "toonhud", from_gb(8));
    support::click_id(cx, s.win, ids::nav(Page::HudStore));
    let page = hud_store(cx, &s);
    support::click_id(cx, s.win, ids::HUD_STORE_TAB_GAMEBANANA);
    gamebanana(cx, &page);
    assert_eq!(
        support::label_of(cx, s.win, ids::store_download(GB, &gb(8))).as_deref(),
        Some("In Library")
    );

    support::click_id(cx, s.win, ids::nav(Page::Huds));
    import(&env, &s, "flawhud", from_gb(7));
    let toon = library_hud(&s, 8);
    s.services.library.delete_hud(toon.id()).unwrap();
    cx.update(LibraryEvents::changed);
    support::click_id(cx, s.win, ids::nav(Page::HudStore));
    support::wait_real(cx, "the cards to reread the library", |cx| {
        support::label_of(cx, s.win, ids::store_download(GB, &gb(7))).as_deref()
            == Some("In Library")
            && support::label_of(cx, s.win, ids::store_download(GB, &gb(8))).as_deref()
                == Some("Download")
    });
}

// A HUD downloaded from the store this visit and then deleted on the HUDs page reads as not
// in the library on return, though the store remembered the download.
#[gpui_kit::test]
fn a_download_deleted_while_away_reads_download_again(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_gb_files(
        7,
        &[("flawhud.zip", support::hud_zip_bytes("flawhud"))],
        Duration::ZERO,
    );
    let s = support::shell_window(
        cx,
        &env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    );
    support::store_ready(cx, &server);
    support::seed_gb_huds(&env, &[(7, "FlawHUD")]);
    support::click_id(cx, s.win, ids::nav(Page::HudStore));
    let page = hud_store(cx, &s);
    support::click_id(cx, s.win, ids::HUD_STORE_TAB_GAMEBANANA);
    gamebanana(cx, &page);
    support::click_id(cx, s.win, ids::store_download(GB, &gb(7)));
    support::wait_real(cx, "the file row", |cx| {
        support::present(cx, s.win, ids::store_file(0))
    });
    support::settle(cx, s.win);
    cx.update_window(s.win, |_, win, cx| win.click(ids::store_file(0), cx))
        .unwrap();
    let dl = cx.update(|cx| StoreDownloads::global(cx)).unwrap();
    support::wait_real(cx, "the download", |cx| {
        cx.update(|cx| !dl.read(cx).is_busy())
    });
    cx.update_window(s.win, |_, window, cx| {
        use gpui_kit::component::WindowExt as _;
        window.close_dialog(cx)
    })
    .unwrap();
    assert_eq!(
        support::label_of(cx, s.win, ids::store_download(GB, &gb(7))).as_deref(),
        Some("In Library")
    );

    support::click_id(cx, s.win, ids::nav(Page::Huds));
    let flaw = library_hud(&s, 7);
    s.services.library.delete_hud(flaw.id()).unwrap();
    cx.update(LibraryEvents::changed);
    support::click_id(cx, s.win, ids::nav(Page::HudStore));
    support::wait_real(cx, "the card to read Download", |cx| {
        support::label_of(cx, s.win, ids::store_download(GB, &gb(7))).as_deref() == Some("Download")
    });
}

// A hidden store holds its re-queries while a sync streams rows in, and catches up once on
// return.
#[gpui_kit::test]
fn a_hidden_store_requeries_once_on_return_not_during_a_sync(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    let (hub, runs) = support::fake_hub(cx);
    support::seed_crosshairs(&env, 1..=10, 0, false);
    support::click_id(cx, s.win, ids::nav(Page::CrosshairStore));
    let page = cx
        .update(|cx| s.shell.read(cx).crosshair_store_page())
        .expect("the Crosshair Store");
    let grid = cx.update(|cx| page.read(cx).grid().clone());
    support::wait_real(cx, "the grid", |cx| {
        cx.update(|cx| grid.read(cx).is_loaded())
    });

    support::click_id(cx, s.win, ids::nav(Page::Huds));
    cx.update(|cx| hub.update(cx, |h, cx| h.start(StoreKind::Crosshairs, false, cx)));
    let run = runs.recv_timeout(Duration::from_secs(5)).unwrap();
    support::seed_crosshairs(&env, 1001..=1002, 5_000, false);
    run.report(2, 12);
    run.finish(Ok(SyncSummary::default()));
    support::wait_hub(cx, "the sync to end", |cx| {
        cx.update(|cx| !hub.read(cx).is_running(StoreKind::Crosshairs))
    });
    assert_eq!(
        cx.update(|cx| grid.read(cx).requeries()),
        0,
        "no work while hidden"
    );

    support::click_id(cx, s.win, ids::nav(Page::CrosshairStore));
    support::wait_real(cx, "the catch-up re-query", |cx| {
        cx.update(|cx| grid.read(cx).requeries() == 1)
    });
    assert_eq!(
        cx.update(|cx| grid.read(cx).cards()[0].name.clone()),
        "Cross 1002"
    );
}

// Library pages are not kept: the HUDs page is built anew on each visit.
#[gpui_kit::test]
fn the_huds_page_is_rebuilt_on_return(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, missing());
    let first = cx
        .update(|cx| s.shell.read(cx).huds_page())
        .expect("the HUDs page");
    support::click_id(cx, s.win, ids::nav(Page::Settings));
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    let second = cx
        .update(|cx| s.shell.read(cx).huds_page())
        .expect("the HUDs page again");
    assert_ne!(first.entity_id(), second.entity_id());
}
