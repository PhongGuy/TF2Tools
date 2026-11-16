//! The library job: one conversion pass per startup, measuring, and a failing item kept quiet.

use std::path::PathBuf;

use gpui_kit::{Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::library_pack::{self, LibraryPack};
use tf2tools_app::services::Tf2State;
use tf2tools_core::{HudEntry, LibraryKind, Provenance};

use crate::support;

fn job(cx: &mut TestAppContext) -> Entity<LibraryPack> {
    cx.update(LibraryPack::global)
}

fn wait_passes(cx: &mut TestAppContext, job: &Entity<LibraryPack>, at_least: u64) {
    support::wait_real(cx, "a library pass", |cx| {
        cx.update(|cx| {
            let job = job.read(cx);
            job.passes() >= at_least && !job.is_running()
        })
    });
}

// Imports a two-file HUD and leaves it loose, as a library from before packing holds it.
fn loose_hud(s: &support::ShellWindow, env: &support::Env, name: &str) -> HudEntry {
    let archive = env.tmp.path().join(format!("{name}.zip"));
    std::fs::write(&archive, support::hud_zip_bytes(name)).unwrap();
    let hud = s
        .services
        .library
        .import_hud(&archive, Provenance::Local)
        .unwrap();
    s.services.library.unpack_hud_for_tests(hud.id()).unwrap();
    hud
}

fn hud_zip(env: &support::Env, hud: &HudEntry) -> PathBuf {
    env.data
        .library_dir(LibraryKind::Huds)
        .join(hud.folder_name())
        .join(".tf2tools/files.zip")
}

#[gpui_kit::test]
fn the_startup_pass_packs_a_loose_hud_then_measures(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let hud = loose_hud(&s, &env, "rayshud");
    cx.update(library_pack::startup);
    let job = job(cx);
    wait_passes(cx, &job, 1);
    assert!(hud_zip(&env, &hud).is_file());
    let space = cx.update(|cx| job.read(cx).space());
    assert_eq!(space, Some(s.services.library.space().unwrap()));
}

#[gpui_kit::test]
fn a_second_startup_call_starts_no_second_pass(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    cx.update(library_pack::startup);
    let job = job(cx);
    wait_passes(cx, &job, 1);
    let hud = loose_hud(&s, &env, "budhud");
    cx.update(library_pack::startup);
    assert!(!cx.update(|cx| job.read(cx).is_running()));
    support::settle(cx, s.win);
    assert_eq!(cx.update(|cx| job.read(cx).passes()), 1);
    assert!(!hud_zip(&env, &hud).exists());
}

#[gpui_kit::test]
fn a_measure_counts_what_changed_and_converts_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let job = job(cx);
    job.update(cx, |job, cx| job.measure(cx));
    wait_passes(cx, &job, 1);
    let before = cx.update(|cx| job.read(cx).space()).unwrap().on_disk();
    let hud = loose_hud(&s, &env, "rayshud");
    job.update(cx, |job, cx| job.measure(cx));
    wait_passes(cx, &job, 2);
    let after = cx.update(|cx| job.read(cx).space()).unwrap().on_disk();
    assert!(after > before, "{before} -> {after}");
    assert!(!hud_zip(&env, &hud).exists());
}

#[gpui_kit::test]
fn a_hud_that_cannot_be_packed_adds_no_activity_row_or_toast(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let hud = loose_hud(&s, &env, "rayshud");
    std::fs::create_dir_all(hud_zip(&env, &hud)).unwrap();
    let rows = support::log_rows(&s.services).len();
    cx.update(library_pack::startup);
    let job = job(cx);
    wait_passes(cx, &job, 1);
    support::settle(cx, s.win);
    assert_eq!(support::log_rows(&s.services).len(), rows);
    assert!(!support::present(cx, s.win, ids::TOAST));
    assert!(
        !cx.update(|cx| job.read(cx).failed()),
        "the measure still lands"
    );
}
