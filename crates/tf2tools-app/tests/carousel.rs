use crate::support;

use std::sync::{Arc, Mutex};

use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::remote_images::queue::MAX_FETCHES;
use tf2tools_app::remote_images::{FetchFailure, ImageFetch, ImageKey, RemoteImage, RemoteImages};
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_app::tokio_bridge;
use tf2tools_app::widgets::carousel::{Carousel, open_lightbox};
use tf2tools_core::{GbId, Provenance, Tf2Install};

fn urls(n: usize) -> Vec<String> {
    (0..n)
        .map(|i| format!("https://images.gamebanana.com/img/ss/mods/shot{i}.jpg"))
        .collect()
}

fn open(cx: &mut TestAppContext, n: usize) -> (AnyWindowHandle, Entity<Carousel>) {
    support::init(cx);
    cx.update(|cx| {
        gpui_kit::open_window(support::window_options(), cx, |_, cx| {
            cx.new(|cx| Carousel::new(urls(n), cx))
        })
    })
    .unwrap()
}

fn index(cx: &mut TestAppContext, c: &Entity<Carousel>) -> usize {
    cx.update(|cx| c.read(cx).index())
}

#[gpui_kit::test]
fn steps_loop_both_ways(cx: &mut TestAppContext) {
    let (win, c) = open(cx, 3);
    assert_eq!(index(cx, &c), 0);
    support::click_id(cx, win, ids::CAROUSEL_PREV);
    assert_eq!(
        index(cx, &c),
        2,
        "previous from the first wraps to the last"
    );
    support::click_id(cx, win, ids::CAROUSEL_NEXT);
    assert_eq!(index(cx, &c), 0, "next from the last wraps to the first");
    support::click_id(cx, win, ids::CAROUSEL_NEXT);
    assert_eq!(index(cx, &c), 1);
    cx.update(|cx| c.update(cx, |c, cx| c.set_urls(urls(2), cx)));
    assert_eq!((index(cx, &c), cx.update(|cx| c.read(cx).len())), (0, 2));
}

#[gpui_kit::test]
fn one_image_has_no_controls_and_none_says_so(cx: &mut TestAppContext) {
    let (win, _) = open(cx, 1);
    assert!(support::present(cx, win, ids::CAROUSEL_IMAGE));
    assert!(!support::present(cx, win, ids::CAROUSEL_PREV));
    assert!(!support::present(cx, win, ids::CAROUSEL_NEXT));
    let (win, _) = open(cx, 0);
    assert!(support::present(cx, win, ids::CAROUSEL_EMPTY));
    assert!(!support::present(cx, win, ids::CAROUSEL_IMAGE));
}

#[gpui_kit::test]
fn position_label_follows_the_steps_and_hides_for_one_image(cx: &mut TestAppContext) {
    let (win, _) = open(cx, 3);
    let position = |cx: &mut TestAppContext| support::label_of(cx, win, ids::CAROUSEL_POSITION);
    assert_eq!(position(cx).as_deref(), Some("1 / 3"));
    support::click_id(cx, win, ids::CAROUSEL_NEXT);
    assert_eq!(position(cx).as_deref(), Some("2 / 3"));
    support::click_id(cx, win, ids::CAROUSEL_PREV);
    support::click_id(cx, win, ids::CAROUSEL_PREV);
    assert_eq!(position(cx).as_deref(), Some("3 / 3"));
    let (win, _) = open(cx, 1);
    assert!(!support::present(cx, win, ids::CAROUSEL_POSITION));
    let (win, _) = open(cx, 0);
    assert!(!support::present(cx, win, ids::CAROUSEL_POSITION));
}

#[gpui_kit::test]
fn wants_the_shown_and_the_next_screenshot_at_carousel_size(cx: &mut TestAppContext) {
    support::init(cx);
    let asked = Arc::new(Mutex::new(Vec::<String>::new()));
    let log = asked.clone();
    let fetch: ImageFetch = Arc::new(move |url: String| {
        log.lock().unwrap().push(url);
        Box::pin(async { Err(FetchFailure::Permanent) })
    });
    // Fetches cross the tokio runtime, so the test executor must be allowed to park.
    cx.executor().allow_parking();
    cx.update(|cx| {
        tokio_bridge::init(cx);
        RemoteImages::install(cx, fetch);
    });
    let (win, _c) = cx
        .update(|cx| {
            gpui_kit::open_window(support::window_options(), cx, |_, cx| {
                cx.new(|cx| Carousel::new(urls(3), cx))
            })
        })
        .unwrap();
    support::wait_real(cx, "the first two fetches", |_| {
        asked.lock().unwrap().len() >= 2
    });
    let first: Vec<String> = asked.lock().unwrap().clone();
    assert!(
        first.contains(&urls(3)[0]) && first.contains(&urls(3)[1]),
        "{first:?}"
    );
    assert!(!first.contains(&urls(3)[2]), "only the shown and the next");
    support::click_id(cx, win, ids::CAROUSEL_NEXT);
    support::wait_real(cx, "the third fetch", |_| {
        asked.lock().unwrap().contains(&urls(3)[2])
    });
}

// Card fetches fill every slot, so the carousel's two screenshots wait queued; once the
// carousel is gone, freeing the slots must fetch neither.
#[gpui_kit::test]
fn a_dropped_carousel_hands_back_its_wants(cx: &mut TestAppContext) {
    support::init(cx);
    let asked = Arc::new(Mutex::new(Vec::<String>::new()));
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let (log, held) = (asked.clone(), gate.clone());
    let fetch: ImageFetch = Arc::new(move |url: String| {
        log.lock().unwrap().push(url);
        let held = held.clone();
        Box::pin(async move {
            let _permit = held.acquire().await;
            Err(FetchFailure::Permanent)
        })
    });
    // Fetches cross the tokio runtime, so the test executor must be allowed to park.
    cx.executor().allow_parking();
    let images = cx.update(|cx| RemoteImages::install(cx, fetch));
    let cards: Vec<ImageKey> = (0..MAX_FETCHES)
        .map(|i| ImageKey::card(format!("u/card{i}")))
        .collect();
    cx.update(|cx| images.update(cx, |i, cx| i.want("cards", cards.clone(), cx)));
    let (win, carousel) = cx
        .update(|cx| {
            gpui_kit::open_window(support::window_options(), cx, |_, cx| {
                cx.new(|cx| Carousel::new(urls(3), cx))
            })
        })
        .unwrap();
    support::settle(cx, win);
    assert_eq!(
        asked.lock().unwrap().len(),
        MAX_FETCHES,
        "the screenshots wait for a slot"
    );
    cx.update_window(win, |_, window, _| window.remove_window())
        .unwrap();
    // Release listeners run when an update flushes, not on a bare drop, so drop the last
    // handle inside one and check its wants are gone before any slot frees.
    let wants = cx.update(|cx| images.read(cx).stats().wants);
    cx.update(move |_| drop(carousel));
    assert_eq!(
        cx.update(|cx| images.read(cx).stats().wants),
        wants + 1,
        "the carousel handed back its wants"
    );
    gate.add_permits(1_000);
    support::wait_real(cx, "the card fetches to fail", |cx| {
        cx.update(|cx| {
            cards
                .iter()
                .all(|key| matches!(images.read(cx).image(key), RemoteImage::Failed))
        })
    });
    cx.run_until_parked();
    let asked = asked.lock().unwrap().clone();
    assert!(
        !urls(3).iter().any(|url| asked.contains(url)),
        "a released screenshot was fetched: {asked:?}"
    );
}

#[gpui_kit::test]
fn the_lightbox_is_a_dialog_with_a_carousel(cx: &mut TestAppContext) {
    support::init(cx);
    let win = support::blank_window(cx);
    cx.update_window(win, |_, window, cx| {
        open_lightbox("FlawHUD".into(), urls(2), window, cx)
    })
    .unwrap();
    assert!(support::present(cx, win, ids::LIGHTBOX));
    assert!(support::present(cx, win, ids::CAROUSEL_NEXT));
}

#[gpui_kit::test]
fn a_store_hud_screenshot_opens_its_gallery(cx: &mut TestAppContext) {
    let env = support::env();
    let found = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    let s = support::shell_window(cx, &env, found);
    support::seed_gb_huds(&env, &[(7, "FlawHUD")]);
    let archive = env.tmp.path().join("flawhud.zip");
    std::fs::write(&archive, support::hud_zip_bytes("flawhud")).unwrap();
    let gb = Provenance::GameBanana {
        gb_id: GbId::new(7),
        author: "vex".into(),
    };
    s.services.library.import_hud(&archive, gb).unwrap();
    support::click_id(cx, s.win, ids::nav(Page::Settings));
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    support::wait_real(cx, "the HUD card", |cx| {
        support::present(cx, s.win, ids::hud_shot("flawhud"))
    });
    support::click_id(cx, s.win, ids::hud_shot("flawhud"));
    support::wait_real(cx, "the lightbox", |cx| {
        support::present(cx, s.win, ids::LIGHTBOX)
    });
    assert!(
        support::present(cx, s.win, ids::CAROUSEL_NEXT),
        "two gallery images"
    );
}

// Both clicks of a double click land before the gallery read returns; one lightbox opens.
#[gpui_kit::test]
fn a_double_click_on_a_screenshot_opens_one_lightbox(cx: &mut TestAppContext) {
    let env = support::env();
    let found = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    let s = support::shell_window(cx, &env, found);
    support::seed_gb_huds(&env, &[(7, "FlawHUD")]);
    let archive = env.tmp.path().join("flawhud.zip");
    std::fs::write(&archive, support::hud_zip_bytes("flawhud")).unwrap();
    let gb = Provenance::GameBanana {
        gb_id: GbId::new(7),
        author: "vex".into(),
    };
    s.services.library.import_hud(&archive, gb).unwrap();
    support::click_id(cx, s.win, ids::nav(Page::Settings));
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    support::wait_real(cx, "the HUD card", |cx| {
        support::present(cx, s.win, ids::hud_shot("flawhud"))
    });
    cx.update_window(s.win, |_, w, cx| {
        w.double_click(ids::hud_shot("flawhud"), cx)
    })
    .unwrap();
    support::wait_real(cx, "the lightbox", |cx| {
        support::present(cx, s.win, ids::LIGHTBOX)
    });
    cx.update_window(s.win, |_, w, cx| w.close_dialog(cx))
        .unwrap();
    support::settle(cx, s.win);
    let stacked = cx
        .update_window(s.win, |_, w, cx| w.has_active_dialog(cx))
        .unwrap();
    assert!(!stacked, "a second lightbox was under the first");
}

#[gpui_kit::test]
fn the_lightbox_is_960_wide_and_centred(cx: &mut TestAppContext) {
    support::init(cx);
    let win = support::blank_window(cx);
    cx.update_window(win, |_, window, cx| {
        open_lightbox("FlawHUD".into(), urls(2), window, cx)
    })
    .unwrap();
    let b = support::dialog_box(cx, win);
    let view = support::viewport(cx, win);
    assert_eq!(b.size.width, gpui_kit::px(960.));
    let off = b.origin.y + b.size.height / 2. - view.height / 2.;
    assert!(off.abs() <= gpui_kit::px(1.), "box {b:?} in {view:?}");
}
