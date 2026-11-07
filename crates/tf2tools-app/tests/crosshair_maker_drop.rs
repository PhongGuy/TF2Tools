//! Dropping image files onto the Crosshair Maker.

use crate::support;

use std::path::{Path, PathBuf};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Entity, ExternalPaths, FileDropEvent, InputEvent as _, Pixels, Point,
    TestAppContext, point,
};
use tf2tools_app::ids;
use tf2tools_app::pages::crosshair_maker::CrosshairMakerPage;
use tf2tools_app::pages::crosshair_maker::model::CLEAR;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;

const DROP_A_PNG: &str = "Drop a PNG image.";

fn open_maker(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<CrosshairMakerPage> {
    support::click_id(cx, s.win, ids::nav(Page::CrosshairMaker));
    cx.update(|cx| s.shell.read(cx).crosshair_maker_page())
        .expect("a Crosshair Maker page")
}

fn shell(cx: &mut TestAppContext, env: &support::Env) -> support::ShellWindow {
    support::shell_window(cx, env, Tf2State::Missing { saved: None })
}

/// A 32x32 PNG with one green texel at (3, 4), written to `name` under the test's temp dir.
fn green_png(env: &support::Env, name: &str) -> PathBuf {
    let path = env.tmp.path().join(name);
    let mut picture = image::RgbaImage::new(32, 32);
    picture.put_pixel(3, 4, image::Rgba([0, 255, 0, 255]));
    picture
        .save_with_format(&path, image::ImageFormat::Png)
        .unwrap();
    path
}

fn pixel(cx: &mut TestAppContext, page: &Entity<CrosshairMakerPage>, x: u32, y: u32) -> [u8; 4] {
    cx.update(|cx| page.read(cx).editor().pixel(x, y))
}

/// Clicks cell (x, y) of the fitted grid with the Pencil.
fn paint_cell(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<CrosshairMakerPage>,
    x: u32,
    y: u32,
) {
    support::settle(cx, s.win);
    let cell = cx.update(|cx| page.read(cx).cell_size());
    let offset = point(cell * (x as f32 + 0.5), cell * (y as f32 + 0.5));
    cx.update_window(s.win, |_, w, cx| {
        w.click_at(ids::CROSSHAIR_MAKER_GRID, offset, cx)
    })
    .unwrap();
    support::settle(cx, s.win);
}

fn over_grid(cx: &mut TestAppContext, s: &support::ShellWindow) -> Point<Pixels> {
    support::bounds_of(cx, s.win, ids::CROSSHAIR_MAKER_VIEW)
        .expect("the grid view")
        .center()
}

fn send(cx: &mut TestAppContext, s: &support::ShellWindow, event: FileDropEvent) {
    cx.update_window(s.win, |_, w, cx| {
        w.dispatch_event(event.to_platform_input(), cx);
    })
    .unwrap();
    support::settle(cx, s.win);
}

/// Drags `paths` in from the desktop over the grid, as the platform reports it.
fn drag_in(cx: &mut TestAppContext, s: &support::ShellWindow, paths: &[&Path]) {
    let position = over_grid(cx, s);
    let paths = ExternalPaths(paths.iter().map(|p| p.to_path_buf()).collect());
    send(cx, s, FileDropEvent::Entered { position, paths });
    send(cx, s, FileDropEvent::Pending { position });
}

/// Drags `paths` over the grid and lets go, then waits for the page to finish loading.
fn drop_files(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<CrosshairMakerPage>,
    paths: &[&Path],
) {
    drag_in(cx, s, paths);
    let position = over_grid(cx, s);
    send(cx, s, FileDropEvent::Submit { position });
    send(cx, s, FileDropEvent::Ended);
    support::wait_real(cx, "the drop", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn dragging_a_file_over_the_maker_shows_where_to_drop_it(cx: &mut TestAppContext) {
    let env = support::env();
    let path = green_png(&env, "mine.png");
    let s = shell(cx, &env);
    let _page = open_maker(cx, &s);
    assert!(!support::present(
        cx,
        s.win,
        ids::CROSSHAIR_MAKER_DROP_TARGET
    ));
    drag_in(cx, &s, &[&path]);
    assert!(support::present(
        cx,
        s.win,
        ids::CROSSHAIR_MAKER_DROP_TARGET
    ));
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIR_MAKER_DROP_TARGET).as_deref(),
        Some("Drop a PNG to open it")
    );
    send(cx, &s, FileDropEvent::Exited);
    assert!(!support::present(
        cx,
        s.win,
        ids::CROSSHAIR_MAKER_DROP_TARGET
    ));
}

#[gpui_kit::test]
fn dropping_a_png_opens_it_like_import_png(cx: &mut TestAppContext) {
    let env = support::env();
    let path = green_png(&env, "mine.png");
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    drop_files(cx, &s, &page, &[&path]);
    assert_eq!(pixel(cx, &page, 3, 4), [0, 255, 0, 255]);
    assert!(cx.update(|cx| page.read(cx).editor().is_dirty()));
    assert!(cx.update(|cx| page.read(cx).editor().open_id().is_none()));
    assert!(support::toast_labels(cx, s.win).is_empty());
    assert!(!support::present(
        cx,
        s.win,
        ids::CROSSHAIR_MAKER_DROP_TARGET
    ));
}

#[gpui_kit::test]
fn a_png_named_in_capitals_is_opened_too(cx: &mut TestAppContext) {
    let env = support::env();
    let path = green_png(&env, "MINE.PNG");
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    drop_files(cx, &s, &page, &[&path]);
    assert_eq!(pixel(cx, &page, 3, 4), [0, 255, 0, 255]);
}

#[gpui_kit::test]
fn dropping_another_kind_of_file_says_so_and_changes_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let path = env.tmp.path().join("notes.txt");
    std::fs::write(&path, b"hello").unwrap();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    drop_files(cx, &s, &page, &[&path]);
    assert_eq!(support::toast_labels(cx, s.win), [DROP_A_PNG]);
    assert!(cx.update(|cx| page.read(cx).can_leave()));
    assert!(!support::present(cx, s.win, ids::CONFIRM_DIALOG));
}

#[gpui_kit::test]
fn dropping_a_broken_png_says_so_like_import_png(cx: &mut TestAppContext) {
    let env = support::env();
    let path = env.tmp.path().join("bad.png");
    std::fs::write(&path, b"not a png").unwrap();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    drop_files(cx, &s, &page, &[&path]);
    support::wait_real(cx, "the toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    assert_eq!(
        support::toast_labels(cx, s.win),
        ["Failed to import PNG. This file isn't a PNG TF2Tools can read. Pick another file."]
    );
    assert!(cx.update(|cx| page.read(cx).can_leave()));
}

#[gpui_kit::test]
fn the_first_png_among_several_files_is_opened(cx: &mut TestAppContext) {
    let env = support::env();
    let notes = env.tmp.path().join("notes.txt");
    std::fs::write(&notes, b"hello").unwrap();
    let path = green_png(&env, "mine.png");
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    drop_files(cx, &s, &page, &[&notes, &path]);
    assert_eq!(pixel(cx, &page, 3, 4), [0, 255, 0, 255]);
    assert!(support::toast_labels(cx, s.win).is_empty());
}

#[gpui_kit::test]
fn dropping_over_unsaved_changes_asks_first(cx: &mut TestAppContext) {
    let env = support::env();
    let path = green_png(&env, "mine.png");
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    paint_cell(cx, &s, &page, 1, 1);
    drop_files(cx, &s, &page, &[&path]);
    assert!(support::present(cx, s.win, ids::CONFIRM_DIALOG));
    support::click_id(cx, s.win, ids::CONFIRM_CANCEL);
    assert_eq!(pixel(cx, &page, 3, 4), CLEAR);
    assert_eq!(pixel(cx, &page, 1, 1), [255, 255, 255, 255]);
    drop_files(cx, &s, &page, &[&path]);
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    assert_eq!(pixel(cx, &page, 1, 1), CLEAR);
    assert_eq!(pixel(cx, &page, 3, 4), [0, 255, 0, 255]);
}
