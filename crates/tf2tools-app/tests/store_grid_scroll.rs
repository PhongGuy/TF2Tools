//! The scaffold at 19,716 rows with images.

use crate::support;

use std::time::{Duration, Instant};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext, point, px};
use tf2tools_app::remote_images::RemoteImages;
use tf2tools_app::remote_images::decode::decode_image;
use tf2tools_app::store_grid::{StoreGrid, StoreGridConfig};
use tf2tools_core::StoreKind;

const ROWS: i64 = 19_716;
/// A third of the real budget, so the ~5k distinct 22 KB thumbnails must evict.
const SMOKE_BUDGET: u64 = 32 * 1024 * 1024;

fn open(
    cx: &mut TestAppContext,
    env: &support::Env,
) -> (AnyWindowHandle, Entity<StoreGrid>, Entity<RemoteImages>) {
    cx.executor().allow_parking();
    let images = cx.update(|cx| {
        RemoteImages::install_with(cx, support::thumbnail_fetch(), SMOKE_BUDGET, decode_image)
    });
    let config = StoreGridConfig::new(StoreKind::Crosshairs, "crosshairs").image_card(px(64.));
    let (win, grid) = support::store_grid_window(cx, env, config, |env| {
        support::seed_crosshairs(env, 1..=ROWS, 0, true)
    });
    support::wait_real(cx, "the first page", |cx| {
        cx.update(|cx| grid.read(cx).is_loaded())
    });
    support::settle(cx, win);
    (win, grid, images)
}

#[gpui_kit::test]
fn a_19716_row_store_pages_to_its_end_within_the_image_budget(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, grid, images) = open(cx, &env);
    assert_eq!(cx.update(|cx| grid.read(cx).total()), Some(ROWS as u64));
    let (mut steps, deadline) = (0, Instant::now() + Duration::from_secs(300));
    while !cx.update(|cx| grid.read(cx).is_end()) {
        cx.update(|cx| grid.read(cx).card_grid().scroll_handle().scroll_to_bottom());
        support::settle(cx, win);
        support::wait_real(cx, "the next page", |cx| {
            !cx.update(|cx| grid.read(cx).is_loading())
        });
        steps += 1;
        assert!(
            Instant::now() < deadline,
            "paging stalled at {}",
            cx.update(|cx| grid.read(cx).cards().len())
        );
    }
    assert_eq!(cx.update(|cx| grid.read(cx).cards().len()), ROWS as usize);
    let stats = cx.update(|cx| images.read(cx).stats());
    assert_eq!(stats.foreground_decodes, 0);
    assert!(
        stats.bytes <= SMOKE_BUDGET && stats.evicted > 0 && stats.released == stats.evicted,
        "{stats:?}"
    );
    // Only on-screen rows ask for images: at most two screens of cards per step.
    assert!(stats.fetches <= steps * 48, "{stats:?} over {steps} steps");
}

/// Probe: CARGO_PROFILE_DEV_DEBUG=0 env -u WAYLAND_DISPLAY -u DISPLAY cargo test -p tf2tools-app
/// --profile timing --test store_grid_scroll -- --ignored --nocapture
#[gpui_kit::test]
#[ignore = "timing probe; run with --ignored --nocapture"]
fn probe_store_grid_scroll_frames(cx: &mut TestAppContext) {
    let env = support::env();
    let opened = Instant::now();
    let (win, grid, images) = open(cx, &env);
    let first_page = opened.elapsed();
    let mut frames = Vec::new();
    let mut y = 0.0f32;
    for frame in 0..330 {
        y += 40.0;
        cx.update(|cx| {
            let handle = grid
                .read(cx)
                .card_grid()
                .scroll_handle()
                .0
                .borrow()
                .base_handle
                .clone();
            handle.set_offset(point(px(0.), px(-y)));
        });
        let start = Instant::now();
        cx.update_window(win, |_, w, cx| {
            w.render_frame(cx);
            w.simulate_next_frame(cx);
        })
        .unwrap();
        cx.run_until_parked();
        if frame >= 30 {
            frames.push(start.elapsed());
        }
    }
    frames.sort();
    let p = |q: f64| frames[((frames.len() - 1) as f64 * q) as usize];
    let stats = cx.update(|cx| images.read(cx).stats());
    eprintln!(
        "store grid probe: first page {first_page:?}; draw p50 {:?} p95 {:?} max {:?} over {} frames; \
         over 16.7 ms {}; cards {}; {stats:?}",
        p(0.5),
        p(0.95),
        p(1.0),
        frames.len(),
        frames
            .iter()
            .filter(|f| **f > Duration::from_micros(16_700))
            .count(),
        cx.update(|cx| grid.read(cx).cards().len()),
    );
}
