mod support;

use std::io::Write as _;
use std::path::{Path, PathBuf};

use gpui_kit::{TestAppContext, px};
use tf2tools_app::services::Tf2State;
use tf2tools_app::timing::Case;
use tf2tools_app::timing::cases::{self, CaseConfig};
use tf2tools_core::{Provenance, Tf2Install};

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

fn hud_zip(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(format!("{name}.zip"));
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file(format!("{name}/info.vdf"), options).unwrap();
    write!(zip, "\"{name}\"\n{{\n\t\"ui_version\"\t\"3\"\n}}\n").unwrap();
    zip.finish().unwrap();
    path
}

fn seed_huds(s: &support::ShellWindow, env: &support::Env, n: usize) {
    for i in 0..n {
        let archive = hud_zip(env.tmp.path(), &format!("hud{i:03}"));
        s.services
            .library
            .import_hud(&archive, Provenance::Local)
            .unwrap();
    }
}

#[gpui_kit::test]
fn picker_scroll_moves_the_picker_list(cx: &mut TestAppContext) {
    // The 405 builtins alone overflow the 160 px picker.
    let env = support::env();
    let s = shell(cx, &env);
    let report = support::run_frame_case(cx, &s, Case::PickerScroll, quick()).unwrap();
    assert_eq!(report.actions, 60);
    assert!(report.max_scroll_px > 0.0, "{report:?}");
    let page = cx.update(|cx| s.shell.read(cx).crosshairs_page()).unwrap();
    assert!(cx.update(|cx| page.read(cx).picker_scroll()).max_offset().y > px(0.));
}

#[gpui_kit::test]
fn hud_grid_scroll_moves_the_grid(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    seed_huds(&s, &env, 30);
    let report = support::run_frame_case(cx, &s, Case::HudGridScroll, quick()).unwrap();
    assert_eq!(report.actions, 60);
    assert!(report.max_scroll_px > 0.0, "{report:?}");
    let page = cx.update(|cx| s.shell.read(cx).huds_page()).unwrap();
    assert!(cx.update(|cx| page.read(cx).grid_scroll()).max_offset().y > px(0.));
}

#[gpui_kit::test]
fn crosshairs_scroll_moves_the_page(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let report = support::run_frame_case(cx, &s, Case::CrosshairsScroll, quick()).unwrap();
    assert_eq!(report.actions, 60);
    assert!(report.max_scroll_px > 0.0, "{report:?}");
}

#[gpui_kit::test]
fn a_scroll_case_with_nothing_to_scroll_fails(cx: &mut TestAppContext) {
    // An empty library draws the empty state, so the grid's handle tracks nothing.
    let env = support::env();
    let s = shell(cx, &env);
    let err = support::run_frame_case(cx, &s, Case::HudGridScroll, quick()).unwrap_err();
    assert!(err.contains("nothing to scroll"), "{err}");
}

#[test]
fn bounce_turns_at_both_ends() {
    assert_eq!(cases::bounce(0., 100., 40., true), (40., true));
    assert_eq!(cases::bounce(80., 100., 40., true), (100., false));
    assert_eq!(cases::bounce(100., 100., 40., false), (60., false));
    assert_eq!(cases::bounce(20., 100., 40., false), (0., true));
}
