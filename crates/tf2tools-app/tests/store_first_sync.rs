//! v5's first-sync states on the three GameBanana store grids: skeleton cards while the first
//! sync fills an empty store, a "Could not load" card with Retry when it fails, and cached cards
//! kept through a later failure; a search that matches nothing is none of these.

mod support;

use std::sync::mpsc::Receiver;
use std::time::Duration;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, ElementId, Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::crosshair_store::CrosshairStorePage;
use tf2tools_app::pages::hud_store::{HudStorePage, HudStoreTab};
use tf2tools_app::pages::sound_store::SoundStorePage;
use tf2tools_app::shell::route::Page;
use tf2tools_app::store_grid::{SEARCH_DEBOUNCE, StoreGrid};
use tf2tools_app::sync::{SyncHub, SyncSummary};
use tf2tools_core::{GbId, StoreItemId, StoreKind};

use support::FakeRun;

/// v5 draws eight skeleton cards on each of these stores.
const SKELETONS: usize = 8;

#[derive(Clone, Copy)]
enum Store {
    Sounds,
    Crosshairs,
    GameBananaHuds,
}

impl Store {
    fn kind(self) -> StoreKind {
        match self {
            Store::Sounds => StoreKind::Sounds,
            Store::Crosshairs => StoreKind::Crosshairs,
            Store::GameBananaHuds => StoreKind::GameBananaHuds,
        }
    }

    fn page(self) -> Page {
        match self {
            Store::Sounds => Page::SoundStore,
            Store::Crosshairs => Page::CrosshairStore,
            Store::GameBananaHuds => Page::HudStore,
        }
    }

    fn unloaded_title(self) -> &'static str {
        match self {
            Store::Sounds => "Could not load sounds",
            Store::Crosshairs => "Could not load crosshairs",
            Store::GameBananaHuds => "Could not load HUDs",
        }
    }

    // Rows 1 and 2 in this store's table, as a finished sync would leave them.
    fn seed(self, env: &support::Env) {
        match self {
            Store::Sounds => support::seed_sounds(
                env,
                &[
                    (1, "Ding", "hitsound", None),
                    (2, "Boom", "killsound", None),
                ],
            ),
            Store::Crosshairs => support::seed_crosshairs(env, 1..=2, 0, false),
            Store::GameBananaHuds => support::seed_gb_huds(env, &[(1, "FlawHUD"), (2, "ToonHUD")]),
        }
    }
}

struct Opened {
    win: AnyWindowHandle,
    grid: Entity<StoreGrid>,
    hub: Entity<SyncHub>,
    runs: Receiver<FakeRun>,
    store: Store,
    _server: support::StoreServer,
    env: support::Env,
}

/// What the page shows of the first-sync states, all read from one settled frame inside the page.
#[derive(Debug, PartialEq)]
struct Seen {
    loading: usize,
    skeletons: usize,
    unloaded: Option<String>,
    retry: bool,
    card_one: bool,
}

// Opens `store`'s page over a fake hub, seeded with rows 1 and 2 when `cached`, with its first
// page landed.
fn open(cx: &mut TestAppContext, store: Store, cached: bool) -> Opened {
    let env = support::env();
    let server = support::StoreServer::start();
    let (hub, runs) = support::fake_hub(cx);
    let seed = move |env: &support::Env| {
        if cached {
            store.seed(env);
        }
    };
    let (win, grid) = match store {
        Store::Sounds => {
            let w = support::sound_store_window(cx, &env, &server, seed);
            let grid = cx.update(|cx| w.page.read(cx).grid().clone());
            (w.win, grid)
        }
        Store::Crosshairs => {
            let (win, page) =
                support::store_window(cx, &env, &server, seed, CrosshairStorePage::new);
            let grid = cx.update(|cx| page.read(cx).grid().clone());
            (win, grid)
        }
        Store::GameBananaHuds => {
            let (win, page) = support::store_window(cx, &env, &server, seed, HudStorePage::new);
            support::click_id(cx, win, ids::HUD_STORE_TAB_GAMEBANANA);
            let tab = cx
                .update(|cx| page.read(cx).gamebanana())
                .expect("the GameBanana tab");
            let grid = cx.update(|cx| tab.read(cx).grid().clone());
            (win, grid)
        }
    };
    support::wait_real(cx, "the first page", |cx| {
        cx.update(|cx| grid.read(cx).is_loaded() && grid.read(cx).total().is_some())
    });
    Opened {
        win,
        grid,
        hub,
        runs,
        store,
        _server: server,
        env,
    }
}

impl Opened {
    fn seen(&self, cx: &mut TestAppContext) -> Seen {
        support::settle(cx, self.win);
        seen_now(cx, self.win, self.store)
    }

    // Starts this store's sync and returns its run, skipping runs of other kinds.
    fn start(&self, cx: &mut TestAppContext) -> FakeRun {
        let kind = self.store.kind();
        cx.update(|cx| self.hub.update(cx, |h, cx| h.start(kind, false, cx)));
        self.next_run()
    }

    fn next_run(&self) -> FakeRun {
        loop {
            let run = self.runs.recv_timeout(Duration::from_secs(5)).unwrap();
            if run.kind == self.store.kind() {
                return run;
            }
        }
    }

    fn wait_idle(&self, cx: &mut TestAppContext) {
        let (hub, kind) = (self.hub.clone(), self.store.kind());
        support::wait_hub(cx, "the sync to end", |cx| {
            cx.update(|cx| !hub.read(cx).is_running(kind))
        });
    }
}

// Draws one frame without running pending work, so a read still in flight stays in flight,
// and reads the states from it inside `store`'s page.
fn seen_now(cx: &mut TestAppContext, win: AnyWindowHandle, store: Store) -> Seen {
    let (page, kind) = (ids::page(store.page()), store.kind());
    let card: ElementId = ids::store_card(kind, &StoreItemId::GameBanana(GbId::new(1)));
    cx.update_window(win, |_, w, cx| {
        w.render_frame(cx);
        let mut page = w.within(page);
        let mut count = |container: &'static str| match page.try_find(container) {
            Some(_) => {
                let skeleton = page.within(container);
                (0..SKELETONS + 2)
                    .filter(|ix| skeleton.try_find(ids::skeleton_card(*ix)).is_some())
                    .count()
            }
            None => 0,
        };
        let (loading, skeletons) = (count(ids::STORE_LOADING), count(ids::STORE_SYNCING));
        Seen {
            loading,
            skeletons,
            unloaded: page
                .try_find(ids::STORE_UNLOADED)
                .and_then(|e| e.label().map(str::to_owned)),
            retry: page.try_find(ids::STORE_UNLOADED_RETRY).is_some(),
            card_one: page.try_find(card).is_some(),
        }
    })
    .unwrap()
}

fn skeletons_then_error_card(cx: &mut TestAppContext, store: Store) {
    let o = open(cx, store, false);
    let run = o.start(cx);
    support::next_repaint(cx);
    assert_eq!(
        o.seen(cx),
        Seen {
            loading: 0,
            skeletons: SKELETONS,
            unloaded: None,
            retry: false,
            card_one: false,
        },
        "the first sync of an empty store shows v5's skeletons"
    );
    run.finish(Err(support::offline_failure()));
    o.wait_idle(cx);
    assert_eq!(
        o.seen(cx),
        Seen {
            loading: 0,
            skeletons: 0,
            unloaded: Some(store.unloaded_title().to_owned()),
            retry: true,
            card_one: false,
        },
        "a failed first sync says so, with Retry"
    );
}

fn retry_starts_a_new_sync(cx: &mut TestAppContext, store: Store) {
    let o = open(cx, store, false);
    o.start(cx).finish(Err(support::offline_failure()));
    o.wait_idle(cx);
    assert!(o.seen(cx).retry, "the error card's Retry");
    support::click_id(cx, o.win, ids::STORE_UNLOADED_RETRY);
    let run = o.next_run();
    support::next_repaint(cx);
    assert_eq!(o.seen(cx).skeletons, SKELETONS, "Retry syncs again");
    // The retried sync lands rows, and the grid shows them in place of the skeletons.
    store.seed(&o.env);
    run.finish(Ok(SyncSummary::default()));
    o.wait_idle(cx);
    let grid = o.grid.clone();
    support::wait_hub(cx, "the synced cards", |cx| {
        cx.update(|cx| !grid.read(cx).cards().is_empty())
    });
    assert_eq!(
        o.seen(cx),
        Seen {
            loading: 0,
            skeletons: 0,
            unloaded: None,
            retry: false,
            card_one: true,
        }
    );
}

fn a_later_failure_keeps_cached_cards(cx: &mut TestAppContext, store: Store) {
    let o = open(cx, store, true);
    let run = o.start(cx);
    support::next_repaint(cx);
    let cards = Seen {
        loading: 0,
        skeletons: 0,
        unloaded: None,
        retry: false,
        card_one: true,
    };
    assert_eq!(o.seen(cx), cards, "a sync over cached cards keeps them");
    run.finish(Err(support::offline_failure()));
    o.wait_idle(cx);
    assert_eq!(
        o.seen(cx),
        cards,
        "a failed sync over cached cards keeps them"
    );
}

#[gpui_kit::test]
fn sound_store_shows_skeletons_then_the_error_card(cx: &mut TestAppContext) {
    skeletons_then_error_card(cx, Store::Sounds);
}

#[gpui_kit::test]
fn crosshair_store_shows_skeletons_then_the_error_card(cx: &mut TestAppContext) {
    skeletons_then_error_card(cx, Store::Crosshairs);
}

#[gpui_kit::test]
fn gamebanana_huds_show_skeletons_then_the_error_card(cx: &mut TestAppContext) {
    skeletons_then_error_card(cx, Store::GameBananaHuds);
}

#[gpui_kit::test]
fn sound_store_retry_starts_a_new_sync(cx: &mut TestAppContext) {
    retry_starts_a_new_sync(cx, Store::Sounds);
}

#[gpui_kit::test]
fn crosshair_store_retry_starts_a_new_sync(cx: &mut TestAppContext) {
    retry_starts_a_new_sync(cx, Store::Crosshairs);
}

#[gpui_kit::test]
fn gamebanana_huds_retry_starts_a_new_sync(cx: &mut TestAppContext) {
    retry_starts_a_new_sync(cx, Store::GameBananaHuds);
}

#[gpui_kit::test]
fn sound_store_keeps_cached_cards_through_a_failed_sync(cx: &mut TestAppContext) {
    a_later_failure_keeps_cached_cards(cx, Store::Sounds);
}

#[gpui_kit::test]
fn crosshair_store_keeps_cached_cards_through_a_failed_sync(cx: &mut TestAppContext) {
    a_later_failure_keeps_cached_cards(cx, Store::Crosshairs);
}

#[gpui_kit::test]
fn gamebanana_huds_keep_cached_cards_through_a_failed_sync(cx: &mut TestAppContext) {
    a_later_failure_keeps_cached_cards(cx, Store::GameBananaHuds);
}

fn a_search_with_no_match_is_not_unloaded(cx: &mut TestAppContext, store: Store) {
    let o = open(cx, store, true);
    let (grid, first) = (
        o.grid.clone(),
        cx.update(|cx| o.grid.read(cx).first_pages()),
    );
    cx.update(|cx| grid.update(cx, |g, cx| g.search_typed("no such item".into(), cx)));
    cx.executor().advance_clock(SEARCH_DEBOUNCE);
    support::wait_real(cx, "the empty search", |cx| {
        cx.update(|cx| {
            let g = grid.read(cx);
            g.first_pages() > first && g.shown() == Some(0) && !g.is_loading()
        })
    });
    assert_eq!(
        o.seen(cx),
        Seen {
            loading: 0,
            skeletons: 0,
            unloaded: None,
            retry: false,
            card_one: false,
        },
        "a store with rows only filtered them out"
    );
}

// Opens `store`'s empty page and reads the first frame before its page and counts are read.
fn loading_skeletons_before_the_first_page(cx: &mut TestAppContext, store: Store) {
    let env = support::env();
    let server = support::StoreServer::start();
    let none = |_: &support::Env| {};
    let win = match store {
        Store::Sounds => support::store_window(cx, &env, &server, none, SoundStorePage::new).0,
        Store::Crosshairs => {
            support::store_window(cx, &env, &server, none, CrosshairStorePage::new).0
        }
        Store::GameBananaHuds => {
            let (win, page) = support::store_window(cx, &env, &server, none, HudStorePage::new);
            // The GameBanana tab and its grid are made on first select, so its reads queue here.
            support::settle(cx, win);
            cx.update_window(win, |_, w, cx| {
                page.update(cx, |p, cx| p.select(HudStoreTab::GameBanana, w, cx));
            })
            .unwrap();
            win
        }
    };
    let blank = Seen {
        loading: 0,
        skeletons: 0,
        unloaded: None,
        retry: false,
        card_one: false,
    };
    assert_eq!(
        seen_now(cx, win, store),
        Seen {
            loading: SKELETONS,
            ..blank
        },
        "the first frame shows v5's skeletons"
    );
    // The reads land on an empty store with no sync, so the loading skeletons give way.
    support::settle(cx, win);
    let after = seen_now(cx, win, store);
    assert_eq!((after.loading, after.unloaded.is_some()), (0, true));
}

#[gpui_kit::test]
fn sound_store_search_with_no_match_shows_no_store_state(cx: &mut TestAppContext) {
    a_search_with_no_match_is_not_unloaded(cx, Store::Sounds);
}

#[gpui_kit::test]
fn crosshair_store_search_with_no_match_shows_no_store_state(cx: &mut TestAppContext) {
    a_search_with_no_match_is_not_unloaded(cx, Store::Crosshairs);
}

#[gpui_kit::test]
fn gamebanana_huds_search_with_no_match_shows_no_store_state(cx: &mut TestAppContext) {
    a_search_with_no_match_is_not_unloaded(cx, Store::GameBananaHuds);
}

#[gpui_kit::test]
fn sound_store_shows_loading_skeletons_before_its_first_page(cx: &mut TestAppContext) {
    loading_skeletons_before_the_first_page(cx, Store::Sounds);
}

#[gpui_kit::test]
fn crosshair_store_shows_loading_skeletons_before_its_first_page(cx: &mut TestAppContext) {
    loading_skeletons_before_the_first_page(cx, Store::Crosshairs);
}

#[gpui_kit::test]
fn gamebanana_huds_show_loading_skeletons_before_their_first_page(cx: &mut TestAppContext) {
    loading_skeletons_before_the_first_page(cx, Store::GameBananaHuds);
}
