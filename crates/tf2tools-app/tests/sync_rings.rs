use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{Bounds, Entity, TestAppContext, point, px, size};
use tf2tools_app::ids;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_app::shell::{nav_icon, ring_icon};
use tf2tools_app::sync::{SyncHub, SyncSummary};
use tf2tools_app::widgets::sync_ring::{
    RING_RADIUS, RING_SIZE, RING_STROKE, RingColors, SPIN_PERIOD, cap_centres, spin_arc,
};
use tf2tools_core::StoreKind;

use crate::support;

fn start(cx: &mut TestAppContext, hub: &Entity<SyncHub>, kind: StoreKind) {
    assert!(cx.update(|cx| hub.update(cx, |h, cx| h.start(kind, false, cx))));
}

#[test]
fn ring_ids_carry_the_store_slug() {
    assert_eq!(
        ids::sync_ring(StoreKind::Sounds),
        gpui_kit::ElementId::Name("sync-ring:sounds".into())
    );
}

#[gpui_kit::test]
fn no_ring_shows_while_nothing_syncs(cx: &mut TestAppContext) {
    let env = support::env();
    let (_hub, _runs) = support::fake_hub(cx);
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    for kind in StoreKind::ALL {
        assert!(
            !support::present(cx, s.win, ids::sync_ring(kind)),
            "{kind:?}"
        );
    }
}

#[gpui_kit::test]
fn a_running_kind_shows_its_ring_with_fetched_over_total(cx: &mut TestAppContext) {
    let env = support::env();
    let (hub, runs) = support::fake_hub(cx);
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    start(cx, &hub, StoreKind::Sounds);
    let run = runs.try_recv().unwrap();
    run.report(120, 1000);
    support::next_repaint(cx);
    assert_eq!(
        support::label_of(cx, s.win, ids::sync_ring(StoreKind::Sounds)).as_deref(),
        Some("Syncing Sounds: 120 / 1000")
    );
    start(cx, &hub, StoreKind::Addons);
    assert!(
        !support::present(cx, s.win, ids::sync_ring(StoreKind::Addons)),
        "addons sync without a ring"
    );
}

#[gpui_kit::test]
fn curated_huds_spin_without_a_total(cx: &mut TestAppContext) {
    let env = support::env();
    let (hub, _runs) = support::fake_hub(cx);
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    start(cx, &hub, StoreKind::CuratedHuds);
    assert_eq!(
        support::label_of(cx, s.win, ids::sync_ring(StoreKind::CuratedHuds)).as_deref(),
        Some("Syncing Curated HUDs...")
    );
}

#[gpui_kit::test]
fn the_rings_run_s_c_h_m_left_of_the_badge(cx: &mut TestAppContext) {
    let env = support::env();
    let (hub, _runs) = support::fake_hub(cx);
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let order = [
        StoreKind::Sounds,
        StoreKind::Crosshairs,
        StoreKind::GameBananaHuds,
        StoreKind::CuratedHuds,
    ];
    for kind in order {
        start(cx, &hub, kind);
    }
    let xs: Vec<_> = order
        .iter()
        .map(|k| support::bounds_of(cx, s.win, ids::sync_ring(*k)).expect("a ring"))
        .collect();
    for pair in xs.windows(2) {
        assert!(pair[0].origin.x < pair[1].origin.x);
    }
    for ring in &xs {
        assert_eq!(ring.size.width, gpui_kit::px(24.));
    }
    let badge = support::bounds_of(cx, s.win, ids::TF2_BADGE).unwrap();
    let last = xs.last().unwrap();
    assert!(last.origin.x + last.size.width <= badge.origin.x);
}

#[gpui_kit::test]
fn a_finished_sync_hides_its_ring(cx: &mut TestAppContext) {
    let env = support::env();
    let (hub, runs) = support::fake_hub(cx);
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    start(cx, &hub, StoreKind::Crosshairs);
    assert!(support::present(
        cx,
        s.win,
        ids::sync_ring(StoreKind::Crosshairs)
    ));
    runs.try_recv().unwrap().finish(Ok(SyncSummary::default()));
    support::wait_hub(cx, "the ring to hide", |cx| {
        !support::present(cx, s.win, ids::sync_ring(StoreKind::Crosshairs))
    });
    assert!(!support::present(cx, s.win, ids::TOAST));
}

#[gpui_kit::test]
fn a_failed_sync_toasts_and_hides_its_ring(cx: &mut TestAppContext) {
    let env = support::env();
    let (hub, runs) = support::fake_hub(cx);
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    start(cx, &hub, StoreKind::Sounds);
    runs.try_recv()
        .unwrap()
        .finish(Err(support::offline_failure()));
    support::wait_hub(cx, "the toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    let toast = support::label_of(cx, s.win, ids::TOAST).unwrap_or_default();
    assert_eq!(
        toast,
        "Failed to sync Sounds. Couldn't reach GameBanana. Check your internet connection and try again."
    );
    assert!(
        !toast.contains("http") && !toast.contains("refused"),
        "{toast}"
    );
    assert!(!support::present(
        cx,
        s.win,
        ids::sync_ring(StoreKind::Sounds)
    ));
}

#[gpui_kit::test]
fn rings_sit_6_px_apart_and_8_px_before_the_badge(cx: &mut TestAppContext) {
    let env = support::env();
    let (hub, runs) = support::fake_hub(cx);
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    start(cx, &hub, StoreKind::Sounds);
    start(cx, &hub, StoreKind::Crosshairs);
    // Held, not dropped: a dropped run ends its sync and its ring.
    let held: Vec<_> = (0..2).map(|_| runs.try_recv().unwrap()).collect();
    for run in &held {
        run.report(1, 10);
    }
    support::next_repaint(cx);
    let a = support::bounds_of(cx, s.win, ids::sync_ring(StoreKind::Sounds)).unwrap();
    let b = support::bounds_of(cx, s.win, ids::sync_ring(StoreKind::Crosshairs)).unwrap();
    let badge = support::bounds_of(cx, s.win, ids::TF2_BADGE).unwrap();
    assert_eq!(b.origin.x - (a.origin.x + a.size.width), px(6.));
    assert_eq!(badge.origin.x - (b.origin.x + b.size.width), px(8.));
}

// v5's SyncIndicator: a muted-foreground/20 track under a primary arc, 24 px with a 3 px stroke.
// The kit's ProgressCircle would draw the track as primary/20.
#[gpui_kit::test]
fn the_ring_track_is_v5_s_muted_foreground_at_20_percent(cx: &mut TestAppContext) {
    support::init(cx);
    cx.update(|cx| {
        let theme = cx.theme();
        let colors = RingColors::of(theme);
        assert_eq!(colors.track, theme.muted_foreground.opacity(0.2));
        assert_eq!(colors.arc, theme.primary);
        assert_ne!(
            colors.track,
            theme.primary.opacity(0.2),
            "the kit's track colour"
        );
    });
    assert_eq!(RING_SIZE, px(24.));
    assert_eq!(RING_STROKE, px(3.));
}

// Both ring shapes (a value arc and the curated spinner) draw v5's 24 px ring; the kit's
// ProgressCircle drew an 18 px one inside the 24 px box.
#[gpui_kit::test]
fn a_value_ring_and_the_curated_spinner_draw_24_px_arcs(cx: &mut TestAppContext) {
    let env = support::env();
    let (hub, runs) = support::fake_hub(cx);
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    start(cx, &hub, StoreKind::Sounds);
    // Held, not dropped: a dropped run ends its sync and its ring.
    let run = runs.try_recv().unwrap();
    run.report(500, 1000);
    start(cx, &hub, StoreKind::CuratedHuds);
    support::next_repaint(cx);
    for slug in ["sounds", "curated-huds"] {
        let arc = support::bounds_of(cx, s.win, ids::keyed("sync-ring-arc", slug)).expect("an arc");
        assert_eq!(arc.size.width, px(24.), "{slug}");
        assert_eq!(arc.size.height, px(24.), "{slug}");
    }
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

// v5's arc has stroke-linecap="round": a disc as wide as the stroke closes each end, centred on
// the stroke's midline, which is v5's circle radius (24 - 3) / 2.
#[test]
fn round_caps_sit_on_the_stroke_midline_at_each_end() {
    assert_eq!(RING_RADIUS, px(10.5));
    let ring = Bounds::new(point(px(100.), px(10.)), size(px(24.), px(24.)));
    let [start, end] = cap_centres(ring, 0., 25.);
    assert!(
        close(start.x.as_f32(), 112.) && close(start.y.as_f32(), 11.5),
        "{start:?}"
    );
    assert!(
        close(end.x.as_f32(), 122.5) && close(end.y.as_f32(), 22.),
        "{end:?}"
    );
    let [_, half] = cap_centres(ring, 0., 50.);
    assert!(
        close(half.x.as_f32(), 112.) && close(half.y.as_f32(), 32.5),
        "{half:?}"
    );
    let [from_three, _] = cap_centres(ring, 25., 50.);
    assert!(
        close(from_three.x.as_f32(), 122.5) && close(from_three.y.as_f32(), 22.),
        "{from_three:?}"
    );
}

// v5's curated ring is animate-spin (one linear turn a second) over a fixed 25% arc that starts
// at 3 o'clock, since that svg is not rotated -90 deg like the value rings.
#[test]
fn the_curated_spinner_turns_a_fixed_quarter_once_a_second() {
    assert_eq!(SPIN_PERIOD, std::time::Duration::from_secs(1));
    let (start, end) = spin_arc(0.);
    assert!(close(start, 25.) && close(end, 50.), "{start} {end}");
    let (start, end) = spin_arc(0.1);
    assert!(close(start, 35.) && close(end, 60.), "{start} {end}");
    let (start, end) = spin_arc(0.9);
    assert!(close(start, 15.) && close(end, 40.), "{start} {end}");
    for step in 0..=100 {
        let (start, end) = spin_arc(step as f32 / 100.);
        assert!((0. ..100.).contains(&start), "{step}: {start}");
        assert!(close(end - start, 25.), "{step}: {start} {end}");
    }
}

// Each ring shows its store's sidebar icon where v5 had a letter; Curated HUDs shares the
// HUD Store's.
#[test]
fn each_ring_icon_is_its_stores_sidebar_icon() {
    for (kind, page) in [
        (StoreKind::Sounds, Page::SoundStore),
        (StoreKind::Crosshairs, Page::CrosshairStore),
        (StoreKind::GameBananaHuds, Page::HudStore),
        (StoreKind::CuratedHuds, Page::HudStore),
    ] {
        assert_eq!(ring_icon(kind), nav_icon(page), "{kind:?}");
    }
}

#[gpui_kit::test]
fn the_ring_icon_is_12_px_and_centred_in_the_ring(cx: &mut TestAppContext) {
    let env = support::env();
    let (hub, _runs) = support::fake_hub(cx);
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    start(cx, &hub, StoreKind::CuratedHuds);
    support::next_repaint(cx);
    let ring = support::bounds_of(cx, s.win, ids::sync_ring(StoreKind::CuratedHuds)).unwrap();
    let icon = support::bounds_of(cx, s.win, ids::keyed("sync-ring-icon", "curated-huds"))
        .expect("an icon in the ring");
    assert_eq!(icon.size, size(px(12.), px(12.)));
    assert_eq!(icon.center(), ring.center());
}
