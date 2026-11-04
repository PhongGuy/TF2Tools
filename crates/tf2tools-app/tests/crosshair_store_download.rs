mod support;

use std::time::Duration;

use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::crosshair_store::CrosshairStorePage;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_app::store_download::StoreDownloads;
use tf2tools_core::{GbId, StoreItemId, StoreKind, Tf2Install};

const C: StoreKind = StoreKind::Crosshairs;

fn gb(id: u64) -> StoreItemId {
    StoreItemId::GameBanana(GbId::new(id))
}

#[gpui_kit::test]
fn a_card_click_opens_the_detail_with_its_carousel(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page) = support::store_window(
        cx,
        &env,
        &server,
        |env| support::seed_crosshairs(env, [1], 0, false),
        CrosshairStorePage::new,
    );
    support::wait_real(cx, "the grid", |cx| {
        cx.update(|cx| page.read(cx).grid().read(cx).is_loaded())
    });
    support::click_id(cx, win, ids::store_card(C, &gb(1)));
    assert!(support::present(cx, win, ids::STORE_DETAIL));
    assert!(
        support::present(cx, win, ids::CAROUSEL_EMPTY),
        "crosshairs keep the carousel"
    );
}

#[gpui_kit::test]
fn a_downloaded_crosshair_lands_on_the_crosshairs_page(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_gb_files(
        7,
        &[("cross.zip", support::crosshair_zip_bytes())],
        Duration::from_millis(800),
    );
    let s = support::shell_window(
        cx,
        &env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    );
    support::store_ready(cx, &server);
    support::seed_crosshairs(&env, [7], 0, false);
    support::click_id(cx, s.win, ids::nav(Page::CrosshairStore));
    let page = cx
        .update(|cx| s.shell.read(cx).crosshair_store_page())
        .expect("the page");
    support::wait_real(cx, "the grid", |cx| {
        cx.update(|cx| page.read(cx).grid().read(cx).is_loaded())
    });
    support::click_id(cx, s.win, ids::store_download(C, &gb(7)));
    support::wait_real(cx, "the file row", |cx| {
        support::present(cx, s.win, ids::store_file(0))
    });
    // The fake server's delay is real time and a settle can outlast it, so click without
    // settling past the click and check the download is running straight away.
    support::settle(cx, s.win);
    cx.update_window(s.win, |_, win, cx| win.click(ids::store_file(0), cx))
        .unwrap();
    let dl = cx.update(|cx| StoreDownloads::global(cx)).unwrap();
    assert!(cx.update(|cx| dl.read(cx).is_downloading(C, &gb(7))));
    // One frame draws the busy card; `label_of` would settle and could outlast the download.
    let label = cx
        .update_window(s.win, |_, win, cx| {
            win.render_frame(cx);
            win.try_find(ids::store_download(C, &gb(7)))
                .and_then(|e| e.label().map(str::to_owned))
        })
        .unwrap()
        .unwrap();
    assert!(
        label.starts_with("Downloading..."),
        "the card shows progress: {label}"
    );
    support::wait_real(cx, "the download", |cx| {
        cx.update(|cx| !dl.read(cx).is_busy())
    });
    support::wait_real(cx, "the toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::TOAST).as_deref(),
        Some("Crosshair downloaded. Cross 7 added to your library")
    );
    cx.update_window(s.win, |_, window, cx| window.close_dialog(cx))
        .unwrap();
    let card = cx.update(|cx| page.read(cx).grid().read(cx).cards()[0].clone());
    assert!(!card.in_library, "no re-query yet");
    assert!(cx.update(|cx| page.read(cx).is_in_library(&card)));
    assert_eq!(
        support::label_of(cx, s.win, ids::store_download(C, &gb(7))).as_deref(),
        Some("In Library")
    );
    let entries = s.services.library.list_crosshairs().unwrap();
    assert_eq!(entries.len(), 1);
    support::click_id(cx, s.win, ids::nav(Page::Crosshairs));
    let crosshairs = cx
        .update(|cx| s.shell.read(cx).crosshairs_page())
        .expect("the Crosshairs page");
    support::wait_real(cx, "Crosshairs to load", |cx| {
        cx.update(|cx| crosshairs.read(cx).is_loaded())
    });
    let input = cx.update(|cx| crosshairs.read(cx).search_input().clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.set_value("cross 7", window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
    assert!(support::present(
        cx,
        s.win,
        ids::crosshair_library(entries[0].folder_name())
    ));
}

#[gpui_kit::test]
fn the_store_detail_is_padded_like_v5(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, page) = support::store_window(
        cx,
        &env,
        &server,
        |env| support::seed_crosshairs(env, [1], 0, false),
        CrosshairStorePage::new,
    );
    support::wait_real(cx, "the grid", |cx| {
        cx.update(|cx| page.read(cx).grid().read(cx).is_loaded())
    });
    support::click_id(cx, win, ids::store_card(C, &gb(1)));
    let b = support::dialog_box(cx, win);
    let detail = support::bounds_of(cx, win, ids::STORE_DETAIL).unwrap();
    assert_eq!(
        detail.left() - b.left(),
        gpui_kit::px(25.),
        "{detail:?} in {b:?}"
    );
    assert_eq!(b.size.width, gpui_kit::px(672.));
}
