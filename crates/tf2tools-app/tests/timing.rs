use crate::support;

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::TestAppContext;
use tf2tools_app::app::{Boot, start};
use tf2tools_app::audio::RecordingAudio;
use tf2tools_app::ids;
use tf2tools_app::picker::ScriptedPicker;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_app::timing::{self, Case, FrameStats, Timing, TimingMode};
use tf2tools_core::Tf2Install;

fn ms(values: &[u64]) -> Vec<Duration> {
    values.iter().map(|&v| Duration::from_millis(v)).collect()
}

fn marks(cx: &mut TestAppContext) -> Vec<&'static str> {
    cx.update(|cx| cx.global::<Timing>().marks())
}

#[test]
fn the_variable_selects_the_mode() {
    assert_eq!(TimingMode::parse(None), Ok(TimingMode::Off));
    assert_eq!(TimingMode::parse(Some("")), Ok(TimingMode::Off));
    assert_eq!(TimingMode::parse(Some("0")), Ok(TimingMode::Off));
    assert_eq!(TimingMode::parse(Some("1")), Ok(TimingMode::Startup));
    assert_eq!(
        TimingMode::parse(Some("page-switch")),
        Ok(TimingMode::Case(Case::PageSwitch))
    );
    let err = TimingMode::parse(Some("bogus")).unwrap_err();
    assert!(err.contains("bogus"), "{err}");
}

#[test]
fn every_case_has_a_stable_name() {
    let names: Vec<&str> = Case::ALL.iter().map(|c| c.name()).collect();
    assert_eq!(
        names,
        [
            "calibrate",
            "page-switch",
            "class-tab",
            "picker-scroll",
            "hud-grid-scroll",
            "crosshairs-scroll",
            "sound-store-scroll",
            "curated-scroll",
            "gamebanana-scroll",
            "sound-store-sync-scroll",
            "sound-store-nav",
            "sound-store-search",
            "sound-store-walk",
            "profiles-nav",
            "addons-nav",
            "logs-nav",
            "logs-scroll",
            "profile-apply",
            "idle"
        ]
    );
    for case in Case::ALL {
        assert_eq!(Case::from_name(case.name()), Some(case));
    }
    assert_eq!(Case::from_name("Idle"), None);
}

#[test]
fn percentiles_are_nearest_rank() {
    let s = FrameStats::from_intervals(&ms(&(1..=20).collect::<Vec<_>>())).unwrap();
    assert_eq!(s.count, 20);
    assert_eq!(s.median_ms, 10.0);
    assert_eq!(s.p95_ms, 19.0);
    assert_eq!(s.max_ms, 20.0);
    assert_eq!((s.over_25ms, s.over_100ms), (0, 0));
}

#[test]
fn long_intervals_are_counted() {
    // Sorted 16, 17, 30, 120: median rank 2, p95 rank 4.
    let s = FrameStats::from_intervals(&ms(&[17, 16, 30, 120])).unwrap();
    assert_eq!(s.median_ms, 17.0);
    assert_eq!(s.p95_ms, 120.0);
    assert_eq!(s.max_ms, 120.0);
    assert_eq!((s.over_25ms, s.over_100ms), (2, 1));
}

#[test]
fn no_intervals_give_no_stats() {
    assert_eq!(FrameStats::from_intervals(&[]), None);
}

#[gpui_kit::test]
fn marks_without_timing_do_nothing(cx: &mut TestAppContext) {
    cx.update(|cx| {
        timing::mark(cx, "window");
        assert!(!cx.has_global::<Timing>());
    });
}

#[gpui_kit::test]
fn the_first_loaded_huds_frame_is_marked_once(cx: &mut TestAppContext) {
    let env = support::env();
    cx.update(|cx| timing::install(cx, Timing::new(TimingMode::Startup)));
    let state = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    let s = support::shell_window(cx, &env, state);
    support::wait_real(cx, "the first-frame mark", |cx| {
        support::settle(cx, s.win);
        marks(cx).contains(&"first-frame")
    });
    assert_eq!(marks(cx), ["huds-loaded", "first-frame"]);

    // Leaving and returning builds a fresh page whose load must not mark again.
    support::click_id(cx, s.win, ids::nav(Page::Settings));
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    support::wait_real(cx, "the HUDs reload", |cx| {
        cx.update(|cx| {
            s.shell
                .read(cx)
                .huds_page()
                .is_some_and(|page| page.read(cx).is_loaded())
        })
    });
    support::settle(cx, s.win);
    assert_eq!(marks(cx), ["huds-loaded", "first-frame"]);
}

#[gpui_kit::test]
fn startup_marks_keep_their_order_with_the_preloaded_library(cx: &mut TestAppContext) {
    let env = support::env();
    support::init(cx);
    cx.update(|cx| timing::install(cx, Timing::new(TimingMode::Startup)));
    let boot = Boot {
        data: env.data.clone(),
        assets: Ok(support::repo_asset_root()),
        picker: Arc::new(ScriptedPicker::empty()),
        detector: support::no_steam(),
        audio: Arc::new(RecordingAudio::new()),
        endpoints: tf2tools_core::Endpoints::default(),
        auto_sync: false,
        offline: false,
        font_host: None,
        updates: tf2tools_app::updater::UpdateBoot::Off,
    };
    let win = cx.update(|cx| start(boot, cx)).unwrap();
    support::wait_real(cx, "the first-frame mark", |cx| {
        support::settle(cx, win);
        marks(cx).contains(&"first-frame")
    });
    assert_eq!(
        marks(cx),
        ["window", "services", "shell", "huds-loaded", "first-frame"]
    );
}
