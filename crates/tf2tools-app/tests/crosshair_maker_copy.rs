//! Copying crosshairs into the maker, Save as new, and PNG export and import.

use crate::support;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext, point};
use tf2tools_app::ids;
use tf2tools_app::pages::crosshair_maker::model::{CLEAR, CanvasSize, FIT_CENTRED, FIT_SHRUNK};
use tf2tools_app::pages::crosshair_maker::{BUSY, CrosshairMakerPage};
use tf2tools_app::picker::{PickKind, Release, ScriptedPicker, file_name_for};
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::leave;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{
    CrosshairEntry, CrosshairId, CrosshairRef, GbId, LibraryKind, PaintTarget, Provenance,
    RgbaImage, decode_vtf, read_png, rgba_to_vtf,
};

fn shell(cx: &mut TestAppContext, env: &support::Env) -> support::ShellWindow {
    support::shell_window(cx, env, Tf2State::Missing { saved: None })
}

fn open_maker(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<CrosshairMakerPage> {
    support::click_id(cx, s.win, ids::nav(Page::CrosshairMaker));
    cx.update(|cx| s.shell.read(cx).crosshair_maker_page())
        .expect("a Crosshair Maker page")
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

fn set_name(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<CrosshairMakerPage>,
    text: &str,
) {
    let input = cx.update(|cx| page.read(cx).name_input().clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.set_value(text, window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
}

fn name_of(cx: &mut TestAppContext, page: &Entity<CrosshairMakerPage>) -> String {
    cx.update(|cx| page.read(cx).name_input().read(cx).value().to_string())
}

/// Clicks `button` (Save or Save as new) and waits for the background save.
fn save_with(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<CrosshairMakerPage>,
    button: &'static str,
) {
    support::click_id(cx, s.win, button);
    support::wait_real(cx, "the save", |cx| {
        cx.update(|cx| !page.read(cx).is_saving())
    });
    support::settle(cx, s.win);
}

fn open_id(cx: &mut TestAppContext, page: &Entity<CrosshairMakerPage>) -> Option<CrosshairId> {
    cx.update(|cx| page.read(cx).editor().open_id().cloned())
}

/// A 64x64 drawing with one red texel at (5, 6), saved as painted crosshair `name`.
fn seed_painted(s: &support::ShellWindow, name: &str) -> (CrosshairEntry, RgbaImage) {
    let mut pixels = vec![0u8; 64 * 64 * 4];
    let at = (6 * 64 + 5) * 4;
    pixels[at..at + 4].copy_from_slice(&[255, 0, 0, 255]);
    let drawing = RgbaImage::from_pixels(64, 64, pixels).unwrap();
    let entry = s
        .services
        .library
        .save_painted_crosshair(&PaintTarget::New, name, &drawing)
        .unwrap();
    (entry, drawing)
}

fn crosshair_dir(env: &support::Env, entry: &CrosshairEntry) -> PathBuf {
    env.data
        .library_dir(LibraryKind::Crosshairs)
        .join(entry.folder_name())
}

/// Every file under `dir` by its path inside `dir`, with its bytes.
fn files_under(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(at) = stack.pop() {
        for entry in std::fs::read_dir(&at).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.insert(
                    path.strip_prefix(dir).unwrap().to_path_buf(),
                    std::fs::read(&path).unwrap(),
                );
            }
        }
    }
    assert!(!out.is_empty(), "no files under {dir:?}");
    out
}

fn assignments(s: &support::ShellWindow) -> Vec<(String, CrosshairRef)> {
    s.services
        .library
        .assignments()
        .unwrap()
        .into_iter()
        .map(|a| (a.weapon_script().to_owned(), a.crosshair().clone()))
        .collect()
}

/// Shows the Crosshairs page, searches for `entry` by name and selects it.
fn select_on_crosshairs(cx: &mut TestAppContext, s: &support::ShellWindow, entry: &CrosshairEntry) {
    support::click_id(cx, s.win, ids::nav(Page::Crosshairs));
    let page = cx
        .update(|cx| s.shell.read(cx).crosshairs_page())
        .expect("a Crosshairs page");
    support::wait_real(cx, "Crosshairs to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    let input = cx.update(|cx| page.read(cx).search_input().clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.set_value(entry.name(), window, cx))
    })
    .unwrap();
    support::click_id(cx, s.win, ids::crosshair_library(entry.folder_name()));
}

/// Clicks `button` on the Crosshairs page and waits for the maker to load what it asked for.
fn to_maker(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    button: &'static str,
) -> Entity<CrosshairMakerPage> {
    support::click_id(cx, s.win, button);
    let page = cx
        .update(|cx| s.shell.read(cx).crosshair_maker_page())
        .expect("the maker");
    support::wait_real(cx, "the drawing to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

#[gpui_kit::test]
fn save_as_new_forks_an_opened_crosshair_and_leaves_the_original_alone(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let (zebra, drawing) = seed_painted(&s, "Zebra");
    let original = CrosshairRef::Library(zebra.id().clone());
    s.services
        .library
        .assign("tf_weapon_scattergun", &original)
        .unwrap();
    let files = files_under(&crosshair_dir(&env, &zebra));
    let assigned = assignments(&s);
    select_on_crosshairs(cx, &s, &zebra);
    let page = to_maker(cx, &s, ids::CROSSHAIRS_EDIT);
    paint_cell(cx, &s, &page, 1, 1);
    set_name(cx, &s, &page, "Zebra two");
    save_with(cx, &s, &page, ids::CROSSHAIR_MAKER_SAVE_AS_NEW);
    let library = s.services.library.list_crosshairs().unwrap();
    assert_eq!(library.len(), 2);
    let fork = library
        .iter()
        .find(|c| c.name() == "Zebra two")
        .expect("the fork");
    assert_ne!(fork.id(), zebra.id());
    assert_eq!(open_id(cx, &page).as_ref(), Some(fork.id()));
    assert!(!cx.update(|cx| page.read(cx).editor().is_dirty()));
    assert_eq!(
        s.services
            .library
            .painted_source(fork.id())
            .unwrap()
            .drawing(),
        &cx.update(|cx| page.read(cx).editor().drawing())
    );
    assert_eq!(files_under(&crosshair_dir(&env, &zebra)), files);
    assert_eq!(
        s.services
            .library
            .painted_source(zebra.id())
            .unwrap()
            .drawing(),
        &drawing
    );
    assert_eq!(assignments(&s), assigned);
    assert_eq!(support::toast_labels(cx, s.win), ["Saved"]);
}

#[gpui_kit::test]
fn save_as_new_under_the_same_name_says_it_is_taken(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let (zebra, _) = seed_painted(&s, "Zebra");
    select_on_crosshairs(cx, &s, &zebra);
    let page = to_maker(cx, &s, ids::CROSSHAIRS_EDIT);
    save_with(cx, &s, &page, ids::CROSSHAIR_MAKER_SAVE_AS_NEW);
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIR_MAKER_NAME_ERROR).as_deref(),
        Some("You already have a crosshair called Zebra. Pick another name.")
    );
    assert_eq!(s.services.library.list_crosshairs().unwrap().len(), 1);
    assert_eq!(open_id(cx, &page).as_ref(), Some(zebra.id()));
}

#[gpui_kit::test]
fn save_as_new_waits_for_a_name_and_always_adds_another(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIR_MAKER_SAVE_AS_NEW).as_deref(),
        Some("Save as new")
    );
    assert!(support::disabled_of(
        cx,
        s.win,
        ids::CROSSHAIR_MAKER_SAVE_AS_NEW
    ));
    paint_cell(cx, &s, &page, 1, 1);
    set_name(cx, &s, &page, "Dot");
    assert!(!support::disabled_of(
        cx,
        s.win,
        ids::CROSSHAIR_MAKER_SAVE_AS_NEW
    ));
    save_with(cx, &s, &page, ids::CROSSHAIR_MAKER_SAVE_AS_NEW);
    let first = open_id(cx, &page).expect("a saved crosshair");
    paint_cell(cx, &s, &page, 2, 2);
    set_name(cx, &s, &page, "Dot two");
    save_with(cx, &s, &page, ids::CROSSHAIR_MAKER_SAVE_AS_NEW);
    let second = open_id(cx, &page).expect("another saved crosshair");
    assert_ne!(first, second);
    assert_eq!(s.services.library.list_crosshairs().unwrap().len(), 2);
    let lib = &s.services.library;
    assert_ne!(
        lib.painted_source(&first).unwrap().drawing(),
        lib.painted_source(&second).unwrap().drawing()
    );
    assert_eq!(name_of(cx, &page), "Dot two");
}

#[gpui_kit::test]
fn only_the_clicked_save_button_reads_saving_and_both_wait(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    let buttons = [
        (ids::CROSSHAIR_MAKER_SAVE, "Save"),
        (ids::CROSSHAIR_MAKER_SAVE_AS_NEW, "Save as new"),
    ];
    for (round, (clicked, _)) in buttons.into_iter().enumerate() {
        paint_cell(cx, &s, &page, round as u32, 1);
        set_name(cx, &s, &page, &format!("Dot {round}"));
        // Click and draw before the executor runs the save, so the frame shows it mid-flight.
        let seen = cx
            .update_window(s.win, |_, w, cx| {
                w.click(clicked, cx);
                w.render_frame(cx);
                buttons.map(|(id, _)| {
                    let label = w.find(id).label().map(str::to_owned);
                    (label, support::button_disabled(w, id))
                })
            })
            .unwrap();
        let want = buttons.map(|(id, label)| {
            let label = if id == clicked { "Saving..." } else { label };
            (Some(label.to_owned()), true)
        });
        assert_eq!(seen, want, "{clicked}");
        support::wait_real(cx, "the save", |cx| {
            cx.update(|cx| !page.read(cx).is_saving())
        });
    }
    assert_eq!(s.services.library.list_crosshairs().unwrap().len(), 2);
}

#[gpui_kit::test]
fn both_save_buttons_stay_inside_the_panel_mid_save_at_the_minimum_window(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    cx.simulate_window_resize(
        s.win,
        gpui_kit::size(gpui_kit::px(800.), gpui_kit::px(600.)),
    );
    let buttons = [ids::CROSSHAIR_MAKER_SAVE, ids::CROSSHAIR_MAKER_SAVE_AS_NEW];
    for (round, clicked) in buttons.into_iter().enumerate() {
        paint_cell(cx, &s, &page, round as u32, 1);
        set_name(cx, &s, &page, &format!("Dot {round}"));
        // Click and draw before the executor runs the save, so the frame shows it mid-flight.
        let (panel, seen) = cx
            .update_window(s.win, |_, w, cx| {
                w.click(clicked, cx);
                w.render_frame(cx);
                let panel = w.find(ids::CROSSHAIR_MAKER_PANEL).bounds();
                (panel, buttons.map(|id| w.find(id).bounds()))
            })
            .unwrap();
        for b in seen {
            assert!(
                b.left() >= panel.left() && b.right() <= panel.right(),
                "{clicked}: {b:?} leaves the panel {panel:?}"
            );
        }
        support::wait_real(cx, "the save", |cx| {
            cx.update(|cx| !page.read(cx).is_saving())
        });
    }
}

#[gpui_kit::test]
fn a_picture_that_lands_while_a_save_runs_says_so(cx: &mut TestAppContext) {
    use tf2tools_app::pages::crosshair_maker::starter::{Starter, starter_drawing};
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    paint_cell(cx, &s, &page, 1, 1);
    set_name(cx, &s, &page, "Dot");
    let large = CanvasSize::Large;
    let dot = starter_drawing(Starter::Dot, Starter::Dot.defaults(large), large, [255; 4]);
    let before = cx.update(|cx| page.read(cx).editor().drawing());
    // The picture arrives in the frame the save starts, before the save can land.
    let (import_disabled, busy) = cx
        .update_window(s.win, |_, w, cx| {
            w.click(ids::CROSSHAIR_MAKER_SAVE, cx);
            w.render_frame(cx);
            let import_disabled = support::button_disabled(w, ids::CROSSHAIR_MAKER_IMPORT_PNG);
            page.update(cx, |page, cx| page.use_starter(dot, w, cx));
            (import_disabled, page.read(cx).is_saving())
        })
        .unwrap();
    assert!(import_disabled && busy);
    let toasts = support::toast_labels(cx, s.win);
    assert!(toasts.contains(&BUSY.to_owned()), "{toasts:?}");
    support::assert_plain(BUSY);
    support::wait_real(cx, "the save", |cx| {
        cx.update(|cx| !page.read(cx).is_saving())
    });
    assert_eq!(cx.update(|cx| page.read(cx).editor().drawing()), before);
    assert!(!support::present(cx, s.win, ids::CONFIRM_DIALOG));
}

/// Starts a copy of `crosshair` on the open maker and waits for it to land.
fn copy(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<CrosshairMakerPage>,
    crosshair: CrosshairRef,
) {
    cx.update(|cx| page.update(cx, |p, cx| p.copy_from(crosshair, cx)));
    support::wait_real(cx, "the copy", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
}

fn pixel(cx: &mut TestAppContext, page: &Entity<CrosshairMakerPage>, x: u32, y: u32) -> [u8; 4] {
    cx.update(|cx| page.read(cx).editor().pixel(x, y))
}

/// Imports a `side`x`side` VTF of one colour as library crosshair `name`.
fn seed_vtf(
    s: &support::ShellWindow,
    env: &support::Env,
    name: &str,
    side: u32,
    rgba: [u8; 4],
) -> CrosshairEntry {
    let image = RgbaImage::from_pixels(side, side, rgba.repeat((side * side) as usize)).unwrap();
    let path = env.tmp.path().join(format!("{name}.vtf"));
    std::fs::write(&path, rgba_to_vtf(&image).unwrap()).unwrap();
    s.services
        .library
        .import_crosshair(&path, name, Provenance::Local)
        .unwrap()
}

#[gpui_kit::test]
fn copying_a_builtin_opens_its_pixels_as_a_new_unsaved_drawing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    copy(cx, &s, &page, CrosshairRef::Builtin("Crossdot".to_owned()));
    let vtf = std::fs::read(s.services.builtins.get("Crossdot").unwrap().vtf_path()).unwrap();
    let (drawing, id, dirty, undo) = cx.update(|cx| {
        let e = page.read(cx).editor();
        (
            e.drawing(),
            e.open_id().cloned(),
            e.is_dirty(),
            e.undo_depth(),
        )
    });
    assert_eq!(drawing, decode_vtf(&vtf).unwrap());
    assert_eq!((id, dirty, undo), (None, true, 1));
    assert_eq!(name_of(cx, &page), "");
    assert!(!cx.update(|cx| page.read(cx).can_leave()));
    assert!(support::toast_labels(cx, s.win).is_empty());
}

#[gpui_kit::test]
fn a_painted_32_crosshair_copies_at_its_own_size(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let mut pixels = vec![0u8; 32 * 32 * 4];
    pixels[(4 * 32 + 3) * 4..(4 * 32 + 4) * 4].copy_from_slice(&[0, 255, 0, 128]);
    let small = RgbaImage::from_pixels(32, 32, pixels).unwrap();
    let entry = s
        .services
        .library
        .save_painted_crosshair(&PaintTarget::New, "Small", &small)
        .unwrap();
    let page = open_maker(cx, &s);
    copy(cx, &s, &page, CrosshairRef::Library(entry.id().clone()));
    assert_eq!(
        cx.update(|cx| page.read(cx).editor().size()),
        CanvasSize::Small
    );
    assert_eq!(cx.update(|cx| page.read(cx).editor().drawing()), small);
    assert_eq!(open_id(cx, &page), None);
}

#[gpui_kit::test]
fn a_big_crosshair_is_shrunk_with_a_note(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let big = seed_vtf(&s, &env, "Big", 128, [255, 255, 255, 255]);
    let page = open_maker(cx, &s);
    copy(cx, &s, &page, CrosshairRef::Library(big.id().clone()));
    assert_eq!(
        cx.update(|cx| page.read(cx).editor().size()),
        CanvasSize::Large
    );
    assert!(pixel(cx, &page, 32, 32)[3] >= 250);
    assert_eq!(support::toast_labels(cx, s.win), [FIT_SHRUNK]);
}

#[gpui_kit::test]
fn a_small_crosshair_is_centred_with_a_note(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let tiny = seed_vtf(&s, &env, "Tiny", 16, [0, 0, 255, 255]);
    let page = open_maker(cx, &s);
    copy(cx, &s, &page, CrosshairRef::Library(tiny.id().clone()));
    assert_eq!(pixel(cx, &page, 24, 24), [0, 0, 255, 255]);
    assert_eq!(pixel(cx, &page, 39, 39), [0, 0, 255, 255]);
    assert_eq!(pixel(cx, &page, 23, 24), CLEAR);
    assert_eq!(pixel(cx, &page, 40, 39), CLEAR);
    assert_eq!(support::toast_labels(cx, s.win), [FIT_CENTRED]);
}

#[gpui_kit::test]
fn a_copy_that_cannot_be_read_says_so_and_leaves_the_canvas(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let big = seed_vtf(&s, &env, "Big", 128, [255, 255, 255, 255]);
    let dir = crosshair_dir(&env, &big);
    std::fs::write(dir.join(format!("{}.vtf", big.folder_name())), b"not a vtf").unwrap();
    let page = open_maker(cx, &s);
    copy(cx, &s, &page, CrosshairRef::Library(big.id().clone()));
    assert_eq!(
        support::toast_labels(cx, s.win),
        [
            "Failed to copy crosshair. This crosshair image couldn't be read. Delete it and add it again."
        ]
    );
    assert_eq!(pixel(cx, &page, 32, 32), CLEAR);
    assert!(cx.update(|cx| page.read(cx).can_leave()));
}

#[gpui_kit::test]
fn copying_over_unsaved_changes_asks_first(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let page = open_maker(cx, &s);
    paint_cell(cx, &s, &page, 0, 0);
    copy(cx, &s, &page, CrosshairRef::Builtin("Crossdot".to_owned()));
    assert!(support::present(cx, s.win, ids::CONFIRM_DIALOG));
    support::click_id(cx, s.win, ids::CONFIRM_CANCEL);
    assert_eq!(pixel(cx, &page, 0, 0), [255, 255, 255, 255]);
    copy(cx, &s, &page, CrosshairRef::Builtin("Crossdot".to_owned()));
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    let vtf = std::fs::read(s.services.builtins.get("Crossdot").unwrap().vtf_path()).unwrap();
    assert_eq!(
        cx.update(|cx| page.read(cx).editor().drawing()),
        decode_vtf(&vtf).unwrap()
    );
    assert_eq!(open_id(cx, &page), None);
}

#[gpui_kit::test]
fn copy_shows_for_every_selected_crosshair(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let (zebra, _) = seed_painted(&s, "Zebra");
    let yak = support::seed_crosshair(&s.services, &env, "Yak");
    support::click_id(cx, s.win, ids::nav(Page::Crosshairs));
    let page = cx.update(|cx| s.shell.read(cx).crosshairs_page()).unwrap();
    support::wait_real(cx, "Crosshairs to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    assert!(!support::present(cx, s.win, ids::CROSSHAIRS_COPY_TO_MAKER));
    let first = s.services.builtins.all()[0].name().to_owned();
    support::click_id(cx, s.win, ids::crosshair_builtin(&first));
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIRS_COPY_TO_MAKER).as_deref(),
        Some("Copy to Crosshair Maker")
    );
    for entry in [&zebra, &yak] {
        select_on_crosshairs(cx, &s, entry);
        assert!(
            support::present(cx, s.win, ids::CROSSHAIRS_COPY_TO_MAKER),
            "{}",
            entry.name()
        );
    }
}

#[gpui_kit::test]
fn a_copied_builtin_saves_as_new_and_the_builtin_is_untouched(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let first = s.services.builtins.all()[0].clone();
    let builtin = CrosshairRef::Builtin(first.name().to_owned());
    s.services
        .library
        .assign("tf_weapon_scattergun", &builtin)
        .unwrap();
    let vtf = std::fs::read(first.vtf_path()).unwrap();
    let assigned = assignments(&s);
    support::click_id(cx, s.win, ids::nav(Page::Crosshairs));
    let crosshairs = cx.update(|cx| s.shell.read(cx).crosshairs_page()).unwrap();
    support::wait_real(cx, "Crosshairs to load", |cx| {
        cx.update(|cx| crosshairs.read(cx).is_loaded())
    });
    support::click_id(cx, s.win, ids::crosshair_builtin(first.name()));
    let page = to_maker(cx, &s, ids::CROSSHAIRS_COPY_TO_MAKER);
    assert_eq!(
        cx.update(|cx| s.shell.read(cx).page()),
        Page::CrosshairMaker
    );
    let want = s
        .services
        .library
        .crosshair_drawing(&builtin, &s.services.builtins)
        .unwrap();
    assert_eq!(cx.update(|cx| page.read(cx).editor().drawing()), want);
    assert_eq!(
        (open_id(cx, &page), name_of(cx, &page)),
        (None, String::new())
    );
    set_name(cx, &s, &page, "My cross");
    save_with(cx, &s, &page, ids::CROSSHAIR_MAKER_SAVE);
    let library = s.services.library.list_crosshairs().unwrap();
    assert_eq!(library.len(), 1);
    assert_eq!(open_id(cx, &page).as_ref(), Some(library[0].id()));
    assert_eq!(std::fs::read(first.vtf_path()).unwrap(), vtf);
    assert_eq!(assignments(&s), assigned);
}

#[gpui_kit::test]
fn a_copied_painted_crosshair_saves_as_new_and_the_original_is_byte_identical(
    cx: &mut TestAppContext,
) {
    let env = support::env();
    let s = shell(cx, &env);
    let (zebra, drawing) = seed_painted(&s, "Zebra");
    s.services
        .library
        .assign(
            "tf_weapon_rocketlauncher",
            &CrosshairRef::Library(zebra.id().clone()),
        )
        .unwrap();
    let files = files_under(&crosshair_dir(&env, &zebra));
    let assigned = assignments(&s);
    select_on_crosshairs(cx, &s, &zebra);
    let page = to_maker(cx, &s, ids::CROSSHAIRS_COPY_TO_MAKER);
    assert_eq!(cx.update(|cx| page.read(cx).editor().drawing()), drawing);
    assert_eq!(
        (open_id(cx, &page), name_of(cx, &page)),
        (None, String::new())
    );
    paint_cell(cx, &s, &page, 1, 1);
    set_name(cx, &s, &page, "Zebra copy");
    save_with(cx, &s, &page, ids::CROSSHAIR_MAKER_SAVE);
    assert_eq!(s.services.library.list_crosshairs().unwrap().len(), 2);
    assert_ne!(open_id(cx, &page).as_ref(), Some(zebra.id()));
    assert_eq!(files_under(&crosshair_dir(&env, &zebra)), files);
    assert_eq!(assignments(&s), assigned);
}

/// A 64x64 one-colour VTF imported as if downloaded from GameBanana, on the rocket launcher.
fn seed_downloaded(s: &support::ShellWindow, env: &support::Env) -> CrosshairEntry {
    let image = RgbaImage::from_pixels(64, 64, [0, 200, 0, 255].repeat(64 * 64)).unwrap();
    let path = env.tmp.path().join("Downloaded.vtf");
    std::fs::write(&path, rgba_to_vtf(&image).unwrap()).unwrap();
    let from_gb = Provenance::GameBanana {
        gb_id: GbId::new(7),
        author: "vex".into(),
    };
    let entry = s
        .services
        .library
        .import_crosshair(&path, "Downloaded", from_gb)
        .unwrap();
    s.services
        .library
        .assign(
            "tf_weapon_rocketlauncher",
            &CrosshairRef::Library(entry.id().clone()),
        )
        .unwrap();
    entry
}

#[gpui_kit::test]
fn a_copied_downloaded_crosshair_saves_as_new_and_the_original_is_byte_identical(
    cx: &mut TestAppContext,
) {
    let env = support::env();
    let s = shell(cx, &env);
    let downloaded = seed_downloaded(&s, &env);
    let files = files_under(&crosshair_dir(&env, &downloaded));
    let assigned = assignments(&s);
    select_on_crosshairs(cx, &s, &downloaded);
    let page = to_maker(cx, &s, ids::CROSSHAIRS_COPY_TO_MAKER);
    assert_eq!(pixel(cx, &page, 10, 10), [0, 200, 0, 255]);
    assert_eq!(open_id(cx, &page), None);
    paint_cell(cx, &s, &page, 1, 1);
    set_name(cx, &s, &page, "Downloaded copy");
    save_with(cx, &s, &page, ids::CROSSHAIR_MAKER_SAVE);
    assert_eq!(s.services.library.list_crosshairs().unwrap().len(), 2);
    assert_ne!(open_id(cx, &page).as_ref(), Some(downloaded.id()));
    assert_eq!(files_under(&crosshair_dir(&env, &downloaded)), files);
    assert_eq!(assignments(&s), assigned);
}

#[gpui_kit::test]
fn a_maker_closed_mid_copy_drops_the_copy_safely(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let downloaded = seed_downloaded(&s, &env);
    let files = files_under(&crosshair_dir(&env, &downloaded));
    let page = open_maker(cx, &s).downgrade();
    // Start the copy and leave in the same frame, before the background read can land.
    let id = downloaded.id().clone();
    cx.update_window(s.win, |_, w, cx| {
        if let Some(page) = page.upgrade() {
            page.update(cx, |p, cx| p.copy_from(CrosshairRef::Library(id), cx));
        }
        w.click(ids::nav(Page::Huds), cx);
    })
    .unwrap();
    support::settle(cx, s.win);
    assert!(page.upgrade().is_none(), "the maker was dropped");
    assert!(support::toast_labels(cx, s.win).is_empty());
    assert!(!support::present(cx, s.win, ids::CONFIRM_DIALOG));
    assert_eq!(files_under(&crosshair_dir(&env, &downloaded)), files);
    // A maker opened afterwards starts blank; the dropped copy never reaches it.
    let fresh = open_maker(cx, &s);
    support::settle(cx, s.win);
    assert_eq!(pixel(cx, &fresh, 10, 10), CLEAR);
    assert!(cx.update(|cx| fresh.read(cx).can_leave()));
}

#[gpui_kit::test]
fn leaving_a_copied_drawing_asks_first(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let (zebra, _) = seed_painted(&s, "Zebra");
    select_on_crosshairs(cx, &s, &zebra);
    let _page = to_maker(cx, &s, ids::CROSSHAIRS_COPY_TO_MAKER);
    assert!(!cx.update(|cx| leave::can_leave(cx)));
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    assert!(support::present(cx, s.win, ids::CONFIRM_DIALOG));
    support::click_id(cx, s.win, ids::CONFIRM_CANCEL);
    assert_eq!(
        cx.update(|cx| s.shell.read(cx).page()),
        Page::CrosshairMaker
    );
}

fn shell_picking(
    cx: &mut TestAppContext,
    env: &support::Env,
    picks: Vec<PathBuf>,
) -> (support::ShellWindow, Arc<ScriptedPicker>) {
    let picker = Arc::new(ScriptedPicker::new(picks));
    let s = support::shell_window_with(
        cx,
        env,
        Tf2State::Missing { saved: None },
        Arc::clone(&picker),
        support::no_steam(),
    );
    (s, picker)
}

fn write_png_file(path: &Path, image: &image::RgbaImage) {
    image
        .save_with_format(path, image::ImageFormat::Png)
        .unwrap();
}

/// Clicks Import PNG and waits until `done` holds.
fn import(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<CrosshairMakerPage>,
    done: impl Fn(&CrosshairMakerPage) -> bool,
) {
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_IMPORT_PNG);
    support::wait_real(cx, "the import", |cx| {
        cx.update(|cx| page.read(cx).is_loaded() && done(page.read(cx)))
    });
    support::settle(cx, s.win);
}

/// A shell whose picker holds every answer until the returned sender releases it.
fn shell_holding(
    cx: &mut TestAppContext,
    env: &support::Env,
    picks: Vec<PathBuf>,
) -> (support::ShellWindow, Arc<ScriptedPicker>, Release) {
    let (picker, release) = ScriptedPicker::held(picks);
    let picker = Arc::new(picker);
    let s = support::shell_window_with(
        cx,
        env,
        Tf2State::Missing { saved: None },
        Arc::clone(&picker),
        support::no_steam(),
    );
    (s, picker, release)
}

/// A 32x32 PNG with one green texel at (3, 4), written to `name` under the test's temp dir.
fn green_png(env: &support::Env, name: &str) -> PathBuf {
    let path = env.tmp.path().join(name);
    let mut picture = image::RgbaImage::new(32, 32);
    picture.put_pixel(3, 4, image::Rgba([0, 255, 0, 255]));
    write_png_file(&path, &picture);
    path
}

#[gpui_kit::test]
fn import_png_opens_one_dialog_at_a_time(cx: &mut TestAppContext) {
    let env = support::env();
    let path = green_png(&env, "mine.png");
    let (s, picker, release) = shell_holding(cx, &env, vec![path.clone(), path]);
    let page = open_maker(cx, &s);
    cx.update_window(s.win, |_, w, cx| {
        w.click(ids::CROSSHAIR_MAKER_IMPORT_PNG, cx);
        w.click(ids::CROSSHAIR_MAKER_IMPORT_PNG, cx);
    })
    .unwrap();
    assert!(support::disabled_of(
        cx,
        s.win,
        ids::CROSSHAIR_MAKER_IMPORT_PNG
    ));
    assert_eq!(picker.asked(), vec![PickKind::MakerPng]);
    release.release();
    support::wait_real(cx, "the import", |cx| {
        cx.update(|cx| page.read(cx).editor().is_dirty() && page.read(cx).is_loaded())
    });
    assert_eq!(pixel(cx, &page, 3, 4), [0, 255, 0, 255]);
    assert!(!support::disabled_of(
        cx,
        s.win,
        ids::CROSSHAIR_MAKER_IMPORT_PNG
    ));
    assert_eq!(picker.asked(), vec![PickKind::MakerPng]);
}

#[gpui_kit::test]
fn a_png_picked_while_a_save_runs_is_never_read_during_it(cx: &mut TestAppContext) {
    let env = support::env();
    let path = green_png(&env, "mine.png");
    let (s, _, release) = shell_holding(cx, &env, vec![path]);
    let page = open_maker(cx, &s);
    paint_cell(cx, &s, &page, 1, 1);
    set_name(cx, &s, &page, "Dot");
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_IMPORT_PNG);
    // The player saves while the dialog is open; the path arrives during the save or after it,
    // as the scheduler orders them, so the outcome is checked either way.
    cx.update_window(s.win, |_, w, cx| {
        w.click(ids::CROSSHAIR_MAKER_SAVE, cx);
        release.release();
    })
    .unwrap();
    let mut read_during_save = false;
    while cx.update(|cx| page.read(cx).is_saving()) && cx.executor().tick() {
        read_during_save |= cx.update(|cx| !page.read(cx).is_loaded());
    }
    support::wait_real(cx, "the save and import", |cx| {
        cx.update(|cx| !page.read(cx).is_saving() && page.read(cx).is_loaded())
    });
    assert!(!read_during_save, "the picture was read during the save");
    let toasts = support::toast_labels(cx, s.win);
    if pixel(cx, &page, 3, 4) == CLEAR {
        // Turned away on arrival: the saved drawing stays and the player is told why.
        assert_eq!(pixel(cx, &page, 1, 1), [255, 255, 255, 255]);
        assert!(toasts.contains(&BUSY.to_owned()), "{toasts:?}");
    } else {
        assert!(!toasts.contains(&BUSY.to_owned()), "{toasts:?}");
    }
}

#[gpui_kit::test]
fn import_png_opens_a_picture_as_a_new_unsaved_drawing(cx: &mut TestAppContext) {
    let env = support::env();
    let path = env.tmp.path().join("mine.png");
    let mut picture = image::RgbaImage::new(32, 32);
    picture.put_pixel(3, 4, image::Rgba([0, 255, 0, 128]));
    write_png_file(&path, &picture);
    let (s, picker) = shell_picking(cx, &env, vec![path]);
    let page = open_maker(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIR_MAKER_IMPORT_PNG).as_deref(),
        Some("Import PNG")
    );
    import(cx, &s, &page, |p| p.editor().is_dirty());
    assert_eq!(picker.asked(), vec![PickKind::MakerPng]);
    let want = RgbaImage::from_pixels(32, 32, picture.into_raw()).unwrap();
    assert_eq!(cx.update(|cx| page.read(cx).editor().drawing()), want);
    assert_eq!(
        (open_id(cx, &page), name_of(cx, &page)),
        (None, String::new())
    );
    assert!(support::toast_labels(cx, s.win).is_empty());
}

#[gpui_kit::test]
fn a_big_png_is_shrunk_with_a_note(cx: &mut TestAppContext) {
    let env = support::env();
    let path = env.tmp.path().join("big.png");
    write_png_file(
        &path,
        &image::RgbaImage::from_pixel(200, 100, image::Rgba([255, 255, 255, 255])),
    );
    let (s, _) = shell_picking(cx, &env, vec![path]);
    let page = open_maker(cx, &s);
    import(cx, &s, &page, |p| p.editor().is_dirty());
    assert_eq!(pixel(cx, &page, 10, 5), CLEAR);
    assert!(pixel(cx, &page, 32, 32)[3] >= 250);
    assert_eq!(support::toast_labels(cx, s.win), [FIT_SHRUNK]);
}

#[gpui_kit::test]
fn a_file_that_is_not_a_png_says_so_and_keeps_the_drawing(cx: &mut TestAppContext) {
    let env = support::env();
    let path = env.tmp.path().join("bad.png");
    std::fs::write(&path, b"not a png").unwrap();
    let (s, _) = shell_picking(cx, &env, vec![path]);
    let page = open_maker(cx, &s);
    paint_cell(cx, &s, &page, 1, 1);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_IMPORT_PNG);
    support::wait_real(cx, "the toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    assert_eq!(
        support::toast_labels(cx, s.win),
        ["Failed to import PNG. This file isn't a PNG TF2Tools can read. Pick another file."]
    );
    assert_eq!(pixel(cx, &page, 1, 1), [255, 255, 255, 255]);
    assert!(cx.update(|cx| page.read(cx).is_loaded()));
}

#[gpui_kit::test]
fn a_picture_that_is_too_big_says_so_and_keeps_the_drawing(cx: &mut TestAppContext) {
    let env = support::env();
    let path = env.tmp.path().join("wide.png");
    write_png_file(
        &path,
        &image::RgbaImage::new(tf2tools_core::PNG_MAX_SIDE + 1, 1),
    );
    let (s, _) = shell_picking(cx, &env, vec![path]);
    let page = open_maker(cx, &s);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_IMPORT_PNG);
    support::wait_real(cx, "the toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    assert_eq!(
        support::toast_labels(cx, s.win),
        ["Failed to import PNG. This picture is too big. Pick a smaller one."]
    );
    assert!(cx.update(|cx| page.read(cx).can_leave()));
}

#[gpui_kit::test]
fn a_cancelled_pick_changes_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, picker) = shell_picking(cx, &env, vec![]);
    let page = open_maker(cx, &s);
    support::click_id(cx, s.win, ids::CROSSHAIR_MAKER_IMPORT_PNG);
    support::settle(cx, s.win);
    assert_eq!(picker.asked(), vec![PickKind::MakerPng]);
    assert!(cx.update(|cx| page.read(cx).can_leave() && page.read(cx).is_loaded()));
    assert!(support::toast_labels(cx, s.win).is_empty());
}

#[gpui_kit::test]
fn importing_over_unsaved_changes_asks_first(cx: &mut TestAppContext) {
    let env = support::env();
    let path = env.tmp.path().join("mine.png");
    write_png_file(&path, &image::RgbaImage::new(64, 64));
    let (s, _) = shell_picking(cx, &env, vec![path.clone(), path]);
    let page = open_maker(cx, &s);
    paint_cell(cx, &s, &page, 1, 1);
    import(cx, &s, &page, |_| true);
    assert!(support::present(cx, s.win, ids::CONFIRM_DIALOG));
    support::click_id(cx, s.win, ids::CONFIRM_CANCEL);
    assert_eq!(pixel(cx, &page, 1, 1), [255, 255, 255, 255]);
    import(cx, &s, &page, |_| true);
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    assert_eq!(pixel(cx, &page, 1, 1), CLEAR);
}

/// Selects the first builtin on the Crosshairs page.
fn select_first_builtin(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
) -> tf2tools_core::BuiltinCrosshair {
    support::click_id(cx, s.win, ids::nav(Page::Crosshairs));
    let page = cx.update(|cx| s.shell.read(cx).crosshairs_page()).unwrap();
    support::wait_real(cx, "Crosshairs to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    let first = s.services.builtins.all()[0].clone();
    support::click_id(cx, s.win, ids::crosshair_builtin(first.name()));
    first
}

fn export(cx: &mut TestAppContext, s: &support::ShellWindow) {
    support::click_id(cx, s.win, ids::CROSSHAIRS_EXPORT_PNG);
    support::wait_real(cx, "the toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
}

#[gpui_kit::test]
fn export_writes_a_builtin_as_a_png(cx: &mut TestAppContext) {
    let env = support::env();
    let out = env.tmp.path().join("out.png");
    let (s, picker) = shell_picking(cx, &env, vec![out.clone()]);
    let first = select_first_builtin(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIRS_EXPORT_PNG).as_deref(),
        Some("Export PNG")
    );
    export(cx, &s);
    assert_eq!(support::toast_labels(cx, s.win), ["PNG exported"]);
    assert_eq!(picker.asked(), vec![PickKind::ExportPng]);
    assert_eq!(
        picker.suggested(),
        vec![file_name_for(first.display_name(), "png")]
    );
    let builtin = CrosshairRef::Builtin(first.name().to_owned());
    assert_eq!(
        read_png(&out).unwrap(),
        s.services
            .library
            .crosshair_drawing(&builtin, &s.services.builtins)
            .unwrap()
    );
}

#[gpui_kit::test]
fn export_then_import_gives_back_the_same_pixels(cx: &mut TestAppContext) {
    let env = support::env();
    let out = env.tmp.path().join("zebra.png");
    let (s, _) = shell_picking(cx, &env, vec![out.clone(), out]);
    let mut pixels = vec![0u8; 32 * 32 * 4];
    pixels[0..4].copy_from_slice(&[255, 0, 0, 255]);
    pixels[40..44].copy_from_slice(&[10, 20, 30, 102]);
    let drawing = RgbaImage::from_pixels(32, 32, pixels).unwrap();
    let zebra = s
        .services
        .library
        .save_painted_crosshair(&PaintTarget::New, "Zebra", &drawing)
        .unwrap();
    select_on_crosshairs(cx, &s, &zebra);
    export(cx, &s);
    let page = open_maker(cx, &s);
    import(cx, &s, &page, |p| p.editor().is_dirty());
    assert_eq!(cx.update(|cx| page.read(cx).editor().drawing()), drawing);
}

#[gpui_kit::test]
fn export_adds_png_when_the_name_has_none(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _) = shell_picking(cx, &env, vec![env.tmp.path().join("mine")]);
    select_first_builtin(cx, &s);
    export(cx, &s);
    assert!(env.tmp.path().join("mine.png").is_file());
}

#[gpui_kit::test]
fn export_never_overwrites_a_png_the_added_extension_would_hit(cx: &mut TestAppContext) {
    let env = support::env();
    let taken = env.tmp.path().join("mine.png");
    std::fs::write(&taken, b"keep me").unwrap();
    let (s, _) = shell_picking(cx, &env, vec![env.tmp.path().join("mine")]);
    select_first_builtin(cx, &s);
    export(cx, &s);
    let want = "A file named mine.png is already there. Pick another name.";
    assert_eq!(support::toast_labels(cx, s.win), [want]);
    support::assert_plain(want);
    assert_eq!(std::fs::read(&taken).unwrap(), b"keep me");
    assert!(!env.tmp.path().join("mine").exists());
}

#[gpui_kit::test]
fn export_over_a_png_named_in_full_replaces_it(cx: &mut TestAppContext) {
    let env = support::env();
    let out = env.tmp.path().join("mine.png");
    std::fs::write(&out, b"old").unwrap();
    let (s, _) = shell_picking(cx, &env, vec![out.clone()]);
    select_first_builtin(cx, &s);
    export(cx, &s);
    assert_eq!(support::toast_labels(cx, s.win), ["PNG exported"]);
    assert!(read_png(&out).is_ok());
}

#[gpui_kit::test]
fn export_writes_a_downloaded_crosshair_as_a_png(cx: &mut TestAppContext) {
    let env = support::env();
    let out = env.tmp.path().join("downloaded.png");
    let (s, _) = shell_picking(cx, &env, vec![out.clone(), out.clone()]);
    let local = seed_vtf(&s, &env, "Local", 64, [0, 0, 200, 255]);
    let downloaded = seed_downloaded(&s, &env);
    for (entry, rgba) in [(&local, [0, 0, 200, 255]), (&downloaded, [0, 200, 0, 255])] {
        select_on_crosshairs(cx, &s, entry);
        support::click_id(cx, s.win, ids::CROSSHAIRS_EXPORT_PNG);
        support::wait_real(cx, "the export", |_| read_png(&out).is_ok());
        let got = read_png(&out).unwrap();
        assert_eq!((got.width(), got.height()), (64, 64), "{}", entry.name());
        assert_eq!(&got.pixels()[..4], rgba, "{}", entry.name());
        std::fs::remove_file(&out).unwrap();
    }
    assert_eq!(support::toast_labels(cx, s.win), ["PNG exported"; 2]);
}

#[gpui_kit::test]
fn an_export_that_cannot_be_written_says_so(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _) = shell_picking(cx, &env, vec![env.tmp.path().join("missing").join("x.png")]);
    select_first_builtin(cx, &s);
    export(cx, &s);
    assert_eq!(
        support::toast_labels(cx, s.win),
        ["Failed to export PNG. Couldn't save the PNG there. Pick another folder, then try again."]
    );
}

#[gpui_kit::test]
fn a_cancelled_export_writes_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, picker) = shell_picking(cx, &env, vec![]);
    select_first_builtin(cx, &s);
    support::click_id(cx, s.win, ids::CROSSHAIRS_EXPORT_PNG);
    support::settle(cx, s.win);
    assert_eq!(picker.asked(), vec![PickKind::ExportPng]);
    assert!(support::toast_labels(cx, s.win).is_empty());
    assert!(!support::disabled_of(cx, s.win, ids::CROSSHAIRS_EXPORT_PNG));
}
