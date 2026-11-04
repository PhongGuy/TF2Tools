mod support;

use gpui_kit::TestAppContext;
use tf2tools_app::pages::profiles::probe::Tf2Probe;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_app::timing::Case;
use tf2tools_app::timing::cases::CaseConfig;
use tf2tools_core::Tf2Install;

fn quick(intervals: usize) -> CaseConfig {
    CaseConfig {
        intervals,
        warm_frames: 3,
        ..CaseConfig::default()
    }
}

fn shell(cx: &mut TestAppContext, env: &support::Env) -> support::ShellWindow {
    let s = support::shell_window(
        cx,
        env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    );
    cx.update(|cx| Tf2Probe::install(cx, || false));
    s
}

fn max_frames(report: &tf2tools_app::timing::cases::CaseReport) -> usize {
    report
        .notes
        .iter()
        .find(|(k, _)| *k == "max_frames")
        .unwrap()
        .1
        .parse()
        .unwrap()
}

#[gpui_kit::test]
fn the_page_nav_cases_visit_their_page_until_loaded(cx: &mut TestAppContext) {
    for case in [Case::ProfilesNav, Case::AddonsNav, Case::LogsNav] {
        let env = support::env();
        let s = shell(cx, &env);
        let report = support::run_frame_case(cx, &s, case, quick(5)).unwrap();
        assert_eq!(report.stats.count, 5, "{}", case.name());
        assert_eq!(report.actions, 5, "{}", case.name());
        assert!(max_frames(&report) >= 1, "{}", case.name());
    }
}

#[test]
fn the_m4_cases_need_no_store_and_only_logs_scroll_needs_the_seed() {
    for case in [
        Case::ProfilesNav,
        Case::AddonsNav,
        Case::LogsNav,
        Case::LogsScroll,
    ] {
        assert!(!case.needs_store(), "{}", case.name());
    }
    assert!(Case::LogsScroll.needs_seed());
    for case in [
        Case::ProfilesNav,
        Case::AddonsNav,
        Case::LogsNav,
        Case::Calibrate,
    ] {
        assert!(!case.needs_seed(), "{}", case.name());
    }
}

#[gpui_kit::test]
fn logs_scroll_pages_to_the_end_then_scrolls(cx: &mut TestAppContext) {
    let env = support::env();
    support::seed_log_rows(&env, 450);
    let s = shell(cx, &env);
    let report = support::run_frame_case(cx, &s, Case::LogsScroll, quick(40)).unwrap();
    assert_eq!(report.stats.count, 40);
    assert!(report.max_scroll_px > 0.0);
    let rows: usize = report
        .notes
        .iter()
        .find(|(k, _)| *k == "rows")
        .unwrap()
        .1
        .parse()
        .unwrap();
    assert!(rows >= 450, "{rows}");
    let page = cx
        .update(|cx| s.shell.read(cx).logs_page())
        .expect("the Logs page");
    assert!(!cx.update(|cx| page.read(cx).has_more()));
    assert_eq!(cx.update(|cx| s.shell.read(cx).page()), Page::Logs);
}

#[gpui_kit::test]
fn logs_scroll_without_rows_says_so(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let err = support::run_frame_case(cx, &s, Case::LogsScroll, quick(10)).unwrap_err();
    assert!(err.contains("nothing to scroll in logs-scroll"), "{err}");
}

use tf2tools_core::{CrosshairRef, DraftCrosshair, ProfileDraft};

fn two_profiles(s: &support::ShellWindow) {
    let first = s.services.builtins.all()[0].name().to_owned();
    for (name, script) in [
        ("Timing A", "tf_weapon_scattergun"),
        ("Timing B", "tf_weapon_rocketlauncher"),
    ] {
        let draft = ProfileDraft::new(name).with_crosshairs(vec![DraftCrosshair::Set {
            weapon_script: script.into(),
            crosshair: CrosshairRef::Builtin(first.clone()),
        }]);
        s.services.library.create_profile(&draft).unwrap();
    }
}

#[gpui_kit::test]
fn profile_apply_records_only_while_applying(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let env = support::env();
    let s = shell(cx, &env);
    two_profiles(&s);
    let report = support::run_frame_case(cx, &s, Case::ProfileApply, quick(30)).unwrap();
    assert!(report.stats.count >= 30);
    assert!(report.actions >= 1);
    let note = |key: &str| {
        report
            .notes
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.clone())
    };
    assert_eq!(note("applies"), Some(report.actions.to_string()));
    assert!(note("apply_median_ms").is_some() && note("apply_max_ms").is_some());
    // Each apply starts on a frame with none in flight, and that frame is not counted.
    let outside: usize = note("frames_outside").unwrap().parse().unwrap();
    let total: usize = note("frames_total").unwrap().parse().unwrap();
    assert!(outside >= report.actions, "{outside} < {}", report.actions);
    assert_eq!(total, report.stats.count + outside);
    assert!(Case::ProfileApply.needs_seed() && !Case::ProfileApply.needs_store());
    let page = cx
        .update(|cx| s.shell.read(cx).profiles_page())
        .expect("the Profiles page");
    assert!(cx.update(|cx| page.read(cx).activating().is_none()));
    assert!(s.services.library.active_profile().unwrap().is_some());
}

#[gpui_kit::test]
fn profile_apply_needs_two_profiles(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let err = support::run_frame_case(cx, &s, Case::ProfileApply, quick(10)).unwrap_err();
    assert!(err.contains("profile-apply needs two profiles"), "{err}");
}

#[gpui_kit::test]
fn profile_apply_restores_the_probe_it_replaced(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    cx.update(|cx| Tf2Probe::install(cx, || true));
    let err = support::run_frame_case(cx, &s, Case::ProfileApply, quick(10)).unwrap_err();
    assert!(err.contains("profile-apply needs two profiles"), "{err}");
    assert!(
        cx.update(|cx| Tf2Probe::get(cx))(),
        "the earlier probe is back"
    );
}

#[gpui_kit::test]
fn profile_apply_leaves_no_probe_when_none_was_installed(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(
        cx,
        &env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    );
    cx.update(|cx| Tf2Probe::set_proc_root(cx, env.tmp.path().join("proc")));
    let err = support::run_frame_case(cx, &s, Case::ProfileApply, quick(10)).unwrap_err();
    assert!(err.contains("profile-apply needs two profiles"), "{err}");
    assert!(!cx.update(|cx| cx.has_global::<Tf2Probe>()));
}

#[gpui_kit::test]
fn a_failed_apply_fails_the_case(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let env = support::env();
    let s = shell(cx, &env);
    two_profiles(&s);
    // Every apply's crosshair step fails, so no apply completes in full.
    rusqlite::Connection::open(env.data.db_path())
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER no_assignments BEFORE INSERT ON crosshair_assignments
             BEGIN SELECT RAISE(ABORT, 'disk full'); END;",
        )
        .unwrap();
    let err = support::run_frame_case(cx, &s, Case::ProfileApply, quick(30)).unwrap_err();
    assert!(err.contains("profile-apply: apply 1 failed"), "{err}");
}
