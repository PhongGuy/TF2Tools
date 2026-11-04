mod support;

use std::time::Duration;

use gpui_kit::TestAppContext;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_app::timing::cases::{self, CaseConfig, CaseReport};
use tf2tools_app::timing::{Case, FrameStats};
use tf2tools_core::{Tf2Install, TfClass};

/// Short runs keep the headless suite quick; the real default is 300 intervals.
fn quick() -> CaseConfig {
    CaseConfig {
        intervals: 60,
        ..CaseConfig::default()
    }
}

fn shell(cx: &mut TestAppContext, env: &support::Env) -> support::ShellWindow {
    let state = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    support::shell_window(cx, env, state)
}

#[gpui_kit::test]
fn calibrate_only_records(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let report = support::run_frame_case(cx, &s, Case::Calibrate, quick()).unwrap();
    assert_eq!(report.stats.count, 60);
    assert_eq!(report.actions, 0);
    assert_eq!(report.max_scroll_px, 0.0);
    assert_eq!(cx.update(|cx| s.shell.read(cx).page()), Page::Huds);
}

#[gpui_kit::test]
fn page_switch_walks_the_sidebar_in_order(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let report = support::run_frame_case(cx, &s, Case::PageSwitch, quick()).unwrap();
    assert_eq!(report.stats.count, 60);
    assert_eq!(report.actions, 10);
    assert_eq!(cx.update(|cx| s.shell.read(cx).page()), Page::ALL[10]);
}

#[gpui_kit::test]
fn class_tab_cycles_the_classes(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let report = support::run_frame_case(cx, &s, Case::ClassTab, quick()).unwrap();
    assert_eq!(report.actions, 10);
    let page = cx
        .update(|cx| s.shell.read(cx).crosshairs_page())
        .expect("the Crosshairs page");
    assert_eq!(cx.update(|cx| page.read(cx).class()), TfClass::ALL[10 % 9]);
}

#[gpui_kit::test]
fn idle_is_not_a_frame_case(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let err = support::run_frame_case(cx, &s, Case::Idle, quick()).unwrap_err();
    assert!(err.contains("idle has no frame runner"), "{err}");
}

#[test]
fn the_report_line_names_the_case_and_its_numbers() {
    let stats =
        FrameStats::from_intervals(&[Duration::from_micros(16_667), Duration::from_micros(17_000)])
            .unwrap();
    let report = CaseReport {
        stats,
        actions: 2,
        max_scroll_px: 0.0,
        notes: vec![],
    };
    assert_eq!(
        cases::report_line(Case::PageSwitch, &Ok(report)),
        "frames case=page-switch intervals=2 median_ms=16.667 p95_ms=17.000 max_ms=17.000 \
         over_25ms=0 over_100ms=0 actions=2 max_scroll_px=0"
    );
    assert_eq!(
        cases::report_line(
            Case::HudGridScroll,
            &Err("nothing to scroll in hud-grid-scroll".into())
        ),
        "frames case=hud-grid-scroll error=nothing to scroll in hud-grid-scroll"
    );
}
