//! History snapshots in the HUD Maker, headless.

use crate::support;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, ScrollDelta, TestAppContext, point, px};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{HudId, PartKey, Tf2Install};

const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";

fn opened(cx: &mut TestAppContext) -> (support::Env, support::ShellWindow, Entity<HudMakerPage>) {
    let env = support::env();
    support::seed_stock_game(&env.game);
    let state = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    let s = support::shell_window(cx, &env, state);
    support::click_nav(cx, s.win, Page::HudMaker);
    let page = cx.update(|cx| s.shell.read(cx).hud_maker_page()).unwrap();
    support::wait_real(cx, "the HUD to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    (env, s, page)
}

fn recolour(cx: &mut TestAppContext, page: &Entity<HudMakerPage>, colour: &str) {
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            let e = p.editor_mut().unwrap();
            e.select(Some(PartKey::new(HV)));
            e.recolour_selected(colour).unwrap();
            e.select(None);
            cx.notify();
        })
    });
}

fn type_into(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    input: Entity<gpui_kit::component::input::InputState>,
    text: &str,
) {
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.set_value(text, window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
}

/// Names the HUD and saves it, so it has a library folder to keep history in.
fn save_hud(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
) -> HudId {
    let name = cx.update(|cx| page.read(cx).name_input().clone());
    type_into(cx, s, name, "My HUD");
    support::click_id(cx, s.win, ids::HUD_MAKER_SAVE);
    support::wait_real(cx, "the save", |cx| {
        cx.update(|cx| !page.read(cx).is_saving() && page.read(cx).saved_hud().is_some())
    });
    support::settle(cx, s.win);
    cx.update(|cx| page.read(cx).saved_hud().cloned()).unwrap()
}

fn open_history(cx: &mut TestAppContext, s: &support::ShellWindow) {
    support::click_id(cx, s.win, ids::HUD_MAKER_MORE);
    support::click_id(cx, s.win, ids::HUD_MAKER_HISTORY_OPEN);
    support::settle(cx, s.win);
}

/// Scrolls the side panel down until `id` is wholly in view, then clicks it.
fn click_in_panel(cx: &mut TestAppContext, s: &support::ShellWindow, id: gpui_kit::ElementId) {
    for _ in 0..60 {
        let body = support::bounds_of(cx, s.win, ids::HUD_MAKER_PANEL_BODY);
        let seen = cx
            .update_window(s.win, |_, w, _| {
                w.try_find(id.clone()).is_some_and(|e| {
                    let b = e.bounds();
                    e.visible()
                        && body.is_some_and(|body| {
                            b.top() >= body.top() && b.bottom() <= body.bottom()
                        })
                })
            })
            .unwrap();
        if seen {
            break;
        }
        let down = ScrollDelta::Lines(point(0., -3.));
        cx.update_window(s.win, |_, w, cx| {
            w.scroll(ids::HUD_MAKER_PANEL_BODY, down, cx)
        })
        .unwrap();
    }
    support::click_id(cx, s.win, id);
}

/// Saves a snapshot named `name` and returns its file once its row shows.
fn take_snapshot(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    name: &str,
) -> String {
    let input = cx.update(|cx| page.read(cx).snapshot_name_input().clone());
    type_into(cx, s, input, name);
    click_in_panel(cx, s, ids::HUD_MAKER_SNAPSHOT_SAVE.into());
    support::wait_real(cx, "the snapshot to list", |cx| {
        cx.update(|cx| page.read(cx).snapshots().iter().any(|e| e.name() == name))
    });
    let file = cx.update(|cx| {
        let p = page.read(cx);
        p.snapshots()
            .iter()
            .find(|e| e.name() == name)
            .unwrap()
            .file()
            .to_owned()
    });
    support::wait_real(cx, "the snapshot row", |cx| {
        support::present(cx, s.win, ids::hud_maker_snapshot(&file))
    });
    file
}

#[gpui_kit::test]
fn a_new_hud_says_to_save_first(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    open_history(cx, &s);
    assert!(cx.update(|cx| page.read(cx).history_open()));
    assert!(support::present(cx, s.win, ids::HUD_MAKER_HISTORY_UNSAVED));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_SNAPSHOT_SAVE));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_PANEL_EMPTY));
}

/// True when `when` reads "YYYY-MM-DD HH:MM:SS, <tail>".
fn reads_as_time_then(when: &str, tail: &str) -> bool {
    let Some((time, rest)) = when.split_once(", ") else {
        return false;
    };
    let shape: String = time
        .chars()
        .map(|c| if c.is_ascii_digit() { '9' } else { c })
        .collect();
    shape == "9999-99-99 99:99:99" && rest == tail
}

#[gpui_kit::test]
fn snapshots_save_and_list(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    recolour(cx, &page, "255 0 0 255");
    let id = save_hud(cx, &s, &page);
    open_history(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_HISTORY_EMPTY).as_deref(),
        Some("This HUD has no snapshots yet.")
    );
    let file = take_snapshot(cx, &s, &page, "Red");
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_snapshot(&file)).as_deref(),
        Some("Red")
    );
    let when = support::label_of(cx, s.win, ids::hud_maker_snapshot_when(&file)).unwrap();
    assert!(reads_as_time_then(&when, "1 change"), "{when}");
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_HISTORY_EMPTY));
    let input = cx.update(|cx| page.read(cx).snapshot_name_input().clone());
    assert_eq!(cx.update(|cx| input.read(cx).value().to_string()), "");
    assert_eq!(s.services.library.list_hud_snapshots(&id).unwrap().len(), 1);
    assert_eq!(
        cx.update(|cx| page.read(cx).snapshots_for().cloned()),
        Some(id)
    );
}

#[gpui_kit::test]
fn a_snapshot_needs_a_name(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    let id = save_hud(cx, &s, &page);
    open_history(cx, &s);
    click_in_panel(cx, &s, ids::HUD_MAKER_SNAPSHOT_SAVE.into());
    support::wait_real(cx, "a toast", |cx| {
        !support::toast_labels(cx, s.win).is_empty()
    });
    assert!(
        support::toast_labels(cx, s.win)
            .iter()
            .any(|t| t.contains("Give the snapshot a name."))
    );
    assert!(
        s.services
            .library
            .list_hud_snapshots(&id)
            .unwrap()
            .is_empty()
    );
}

#[gpui_kit::test]
fn history_steps_aside_for_a_part_and_closes_back_to_the_hud(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    save_hud(cx, &s, &page);
    open_history(cx, &s);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_HISTORY));
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            p.editor_mut().unwrap().select(Some(PartKey::new(HV)));
            cx.notify();
        })
    });
    support::settle(cx, s.win);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_PANEL_PART));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_HISTORY));
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            p.editor_mut().unwrap().select(None);
            cx.notify();
        })
    });
    support::settle(cx, s.win);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_HISTORY));
    click_in_panel(cx, &s, ids::HUD_MAKER_HISTORY_CLOSE.into());
    assert!(!cx.update(|cx| page.read(cx).history_open()));
    assert!(support::present(cx, s.win, ids::HUD_MAKER_PANEL_EMPTY));
}

#[gpui_kit::test]
fn delete_asks_first_and_keep_keeps_it(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    recolour(cx, &page, "255 0 0 255");
    let id = save_hud(cx, &s, &page);
    open_history(cx, &s);
    let file = take_snapshot(cx, &s, &page, "Red");
    let changes = cx.update(|cx| page.read(cx).editor().unwrap().changes().to_vec());

    click_in_panel(cx, &s, ids::hud_maker_snapshot_delete(&file));
    assert_eq!(
        support::label_of(cx, s.win, ids::CONFIRM_TITLE).as_deref(),
        Some("Delete this snapshot?")
    );
    support::click_id(cx, s.win, ids::CONFIRM_CANCEL);
    support::settle(cx, s.win);
    assert!(support::present(cx, s.win, ids::hud_maker_snapshot(&file)));
    assert_eq!(s.services.library.list_hud_snapshots(&id).unwrap().len(), 1);

    click_in_panel(cx, &s, ids::hud_maker_snapshot_delete(&file));
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    support::wait_real(cx, "the row to go", |cx| {
        !support::present(cx, s.win, ids::hud_maker_snapshot(&file))
    });
    assert!(
        s.services
            .library
            .list_hud_snapshots(&id)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        cx.update(|cx| page.read(cx).editor().unwrap().changes().to_vec()),
        changes
    );
}

#[gpui_kit::test]
fn save_as_new_shows_the_new_huds_history(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    recolour(cx, &page, "255 0 0 255");
    save_hud(cx, &s, &page);
    open_history(cx, &s);
    take_snapshot(cx, &s, &page, "Red");
    let name = cx.update(|cx| page.read(cx).name_input().clone());
    type_into(cx, &s, name, "My copy");
    // Save as new sits in the toolbar's overflow menu.
    support::click_id(cx, s.win, ids::HUD_MAKER_MORE);
    support::click_id(cx, s.win, ids::HUD_MAKER_SAVE_AS_NEW);
    support::wait_real(cx, "the new HUD's empty history", |cx| {
        support::present(cx, s.win, ids::HUD_MAKER_HISTORY_EMPTY)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_HISTORY_EMPTY).as_deref(),
        Some("This HUD has no snapshots yet.")
    );
    let (saved, read_for) = cx.update(|cx| {
        let p = page.read(cx);
        (p.saved_hud().cloned(), p.snapshots_for().cloned())
    });
    assert!(saved.is_some());
    assert_eq!(read_for, saved, "the list is the new HUD's");
    assert!(cx.update(|cx| page.read(cx).snapshots().is_empty()));
}

fn colour(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> Option<String> {
    cx.update(|cx| {
        let e = page.read(cx).editor()?;
        let part = e.model().part(&PartKey::new(HV))?;
        part.value("fgcolor").map(|l| l.text().to_owned())
    })
}

fn undo(cx: &mut TestAppContext, s: &support::ShellWindow, page: &Entity<HudMakerPage>) {
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| p.undo(window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
}

fn toasted(cx: &mut TestAppContext, s: &support::ShellWindow, text: &str) -> bool {
    support::wait_real(cx, "a toast", |cx| {
        !support::toast_labels(cx, s.win).is_empty()
    });
    support::toast_labels(cx, s.win)
        .iter()
        .any(|t| t.contains(text))
}

#[gpui_kit::test]
fn restore_brings_back_a_snapshot_as_one_undo_step(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    recolour(cx, &page, "255 0 0 255");
    save_hud(cx, &s, &page);
    open_history(cx, &s);
    let file = take_snapshot(cx, &s, &page, "Red");
    recolour(cx, &page, "0 0 255 255");
    click_in_panel(cx, &s, ids::hud_maker_snapshot_restore(&file));
    support::wait_real(cx, "the restore", |cx| {
        cx.update(|cx| {
            let e = page.read(cx).editor().unwrap();
            let part = e.model().part(&PartKey::new(HV)).unwrap();
            part.value("fgcolor")
                .is_some_and(|l| l.text() == "255 0 0 255")
        })
    });
    assert!(toasted(cx, &s, "Restored Red."));
    undo(cx, &s, &page);
    assert_eq!(colour(cx, &page).as_deref(), Some("0 0 255 255"));
}

#[gpui_kit::test]
fn restoring_what_the_hud_already_is_says_so(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    recolour(cx, &page, "255 0 0 255");
    save_hud(cx, &s, &page);
    open_history(cx, &s);
    let file = take_snapshot(cx, &s, &page, "Red");
    let before = cx.update(|cx| page.read(cx).editor().unwrap().changes().to_vec());
    click_in_panel(cx, &s, ids::hud_maker_snapshot_restore(&file));
    assert!(toasted(cx, &s, "Your HUD already matches Red."));
    assert_eq!(
        cx.update(|cx| page.read(cx).editor().unwrap().changes().to_vec()),
        before
    );
}

#[gpui_kit::test]
fn a_snapshot_that_cant_be_read_toasts_and_changes_nothing(cx: &mut TestAppContext) {
    let (env, s, page) = opened(cx);
    recolour(cx, &page, "255 0 0 255");
    let id = save_hud(cx, &s, &page);
    open_history(cx, &s);
    let file = take_snapshot(cx, &s, &page, "Red");
    let folder = s
        .services
        .library
        .list_huds()
        .unwrap()
        .into_iter()
        .find(|h| *h.id() == id)
        .unwrap()
        .folder_name()
        .to_owned();
    let path = env
        .data
        .library_dir(tf2tools_core::LibraryKind::Huds)
        .join(folder)
        .join(".tf2tools/history")
        .join(&file);
    std::fs::write(&path, b"not a snapshot").unwrap();
    recolour(cx, &page, "0 0 255 255");
    click_in_panel(cx, &s, ids::hud_maker_snapshot_restore(&file));
    assert!(toasted(cx, &s, "That snapshot can't be read."));
    assert_eq!(colour(cx, &page).as_deref(), Some("0 0 255 255"));
}

fn bounds(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    id: impl Into<gpui_kit::ElementId>,
) -> gpui_kit::Bounds<gpui_kit::Pixels> {
    support::bounds_of(cx, s.win, id).expect("drawn")
}

fn f(p: gpui_kit::Pixels) -> f32 {
    f32::from(p)
}

/// Saves the HUD red, snapshots it as "Red", turns it blue and compares with the snapshot.
fn comparing_red(
    cx: &mut TestAppContext,
) -> (
    support::Env,
    support::ShellWindow,
    Entity<HudMakerPage>,
    String,
) {
    let (env, s, page) = opened(cx);
    recolour(cx, &page, "255 0 0 255");
    save_hud(cx, &s, &page);
    open_history(cx, &s);
    let file = take_snapshot(cx, &s, &page, "Red");
    recolour(cx, &page, "0 0 255 255");
    click_in_panel(cx, &s, ids::hud_maker_snapshot_compare(&file));
    support::wait_real(cx, "the comparison", |cx| {
        cx.update(|cx| page.read(cx).comparing().is_some())
    });
    support::settle(cx, s.win);
    (env, s, page, file)
}

#[gpui_kit::test]
fn compare_splits_the_canvas_between_the_snapshot_and_now(cx: &mut TestAppContext) {
    let (_env, s, page, _file) = comparing_red(cx);
    assert_eq!(
        cx.update(|cx| page.read(cx).comparing().map(str::to_owned))
            .as_deref(),
        Some("Red")
    );
    let canvas = bounds(cx, &s, ids::HUD_MAKER_CANVAS);
    let before = bounds(cx, &s, ids::HUD_MAKER_COMPARE_BEFORE);
    let now = bounds(cx, &s, ids::HUD_MAKER_COMPARE_NOW);
    let half = f(canvas.size.width) / 2.;
    assert!(
        (f(before.size.width) - half).abs() <= 1.,
        "{before:?} {canvas:?}"
    );
    assert!(
        (f(now.left()) - (f(canvas.left()) + half)).abs() <= 1.,
        "{now:?}"
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_COMPARE_LABEL_BEFORE).as_deref(),
        Some("Red")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_COMPARE_LABEL_NOW).as_deref(),
        Some("Now")
    );
    // Each side draws its own health value.
    let part = ids::hud_maker_part(&PartKey::new(HV));
    let found = cx
        .update_window(s.win, |_, w, _| {
            let before = w
                .within(ids::HUD_MAKER_COMPARE_BEFORE)
                .try_find(part.clone())
                .is_some();
            let now = w
                .within(ids::HUD_MAKER_COMPARE_NOW)
                .try_find(part.clone())
                .is_some();
            (before, now)
        })
        .unwrap();
    assert_eq!(found, (true, true));
    // A comparison is not a change.
    let e = cx.update(|cx| page.read(cx).editor().unwrap().changes().len());
    assert_eq!(colour(cx, &page).as_deref(), Some("0 0 255 255"));
    assert_eq!(e, 2);
}

#[gpui_kit::test]
fn dragging_the_divider_moves_the_split_and_lays_nothing_out(cx: &mut TestAppContext) {
    let (_env, s, page, _file) = comparing_red(cx);
    let builds = cx.update(|cx| page.read(cx).editor().unwrap().compare_builds());
    let changes = cx.update(|cx| page.read(cx).editor().unwrap().changes().to_vec());
    let canvas = bounds(cx, &s, ids::HUD_MAKER_CANVAS);
    let grip = bounds(cx, &s, ids::HUD_MAKER_COMPARE_HANDLE).center();
    let to = gpui_kit::point(canvas.left() + canvas.size.width * 0.25, grip.y);
    cx.update_window(s.win, |_, w, cx| w.drag(grip, to, cx))
        .unwrap();
    support::settle(cx, s.win);
    let split = cx.update(|cx| page.read(cx).compare_split());
    assert!((split - 0.25).abs() < 0.01, "{split}");
    let before = bounds(cx, &s, ids::HUD_MAKER_COMPARE_BEFORE);
    assert!((f(before.size.width) - f(canvas.size.width) * 0.25).abs() <= 1.);
    cx.update(|cx| {
        let e = page.read(cx).editor().unwrap();
        assert_eq!(e.compare_builds(), builds);
        assert_eq!(e.changes(), changes);
        assert!(e.selected().is_none(), "a divider drag selects nothing");
    });
    // The split stays on the screen.
    cx.update(|cx| page.update(cx, |p, cx| p.set_compare_split(1.7, cx)));
    assert_eq!(cx.update(|cx| page.read(cx).compare_split()), 1.);
}

#[gpui_kit::test]
fn deleting_the_compared_snapshot_stops_comparing(cx: &mut TestAppContext) {
    let (_env, s, page, file) = comparing_red(cx);
    click_in_panel(cx, &s, ids::hud_maker_snapshot_delete(&file));
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    support::wait_real(cx, "the row to go", |cx| {
        !support::present(cx, s.win, ids::hud_maker_snapshot(&file))
    });
    support::settle(cx, s.win);
    assert!(cx.update(|cx| page.read(cx).comparing().is_none()));
    assert!(cx.update(|cx| page.read(cx).editor().unwrap().compared().is_none()));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_COMPARE_BEFORE));
}

#[gpui_kit::test]
fn compare_stops_from_its_button_and_when_history_closes(cx: &mut TestAppContext) {
    let (_env, s, page, file) = comparing_red(cx);
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_snapshot_compare(&file)).as_deref(),
        Some("Stop comparing")
    );
    click_in_panel(cx, &s, ids::hud_maker_snapshot_compare(&file));
    assert!(cx.update(|cx| page.read(cx).comparing().is_none()));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_COMPARE_BEFORE));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_COMPARE_HANDLE));

    click_in_panel(cx, &s, ids::hud_maker_snapshot_compare(&file));
    support::wait_real(cx, "the comparison", |cx| {
        cx.update(|cx| page.read(cx).comparing().is_some())
    });
    click_in_panel(cx, &s, ids::HUD_MAKER_HISTORY_CLOSE.into());
    assert!(cx.update(|cx| page.read(cx).comparing().is_none()));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_COMPARE_NOW));
}

#[gpui_kit::test]
fn the_chip_and_escape_stop_comparing(cx: &mut TestAppContext) {
    let (_env, s, page, file) = comparing_red(cx);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_COMPARE_STOP).as_deref(),
        Some("Stop comparing")
    );
    support::click_id(cx, s.win, ids::HUD_MAKER_COMPARE_STOP);
    assert!(cx.update(|cx| page.read(cx).comparing().is_none()));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_COMPARE_STOP));

    click_in_panel(cx, &s, ids::hud_maker_snapshot_compare(&file));
    support::wait_real(cx, "the comparison", |cx| {
        cx.update(|cx| page.read(cx).comparing().is_some())
    });
    // The default split puts the grip at the canvas centre, so this press lands on the grip.
    // Its mouse-down must hand the keys to the page for Escape to arrive.
    let canvas = bounds(cx, &s, ids::HUD_MAKER_CANVAS);
    let grip = bounds(cx, &s, ids::HUD_MAKER_COMPARE_HANDLE).center();
    assert!((f(grip.x) - (f(canvas.left()) + f(canvas.size.width) / 2.)).abs() <= 1.);
    support::click_id(cx, s.win, ids::HUD_MAKER_COMPARE_HANDLE);
    support::press(cx, s.win, "escape");
    assert!(cx.update(|cx| page.read(cx).comparing().is_none()));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_COMPARE_BEFORE));
}

#[gpui_kit::test]
fn a_recipe_preview_ends_the_comparison(cx: &mut TestAppContext) {
    use tf2tools_core::{HudAssets, Recipe, Screen};

    let (_env, s, page, _file) = comparing_red(cx);
    let builds = cx.update(|cx| page.read(cx).editor().unwrap().compare_builds());
    let recipe = Recipe::new("Nothing", &[], &HudAssets::new());
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| p.start_preview(recipe, 0, window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
    assert!(cx.update(|cx| page.read(cx).comparing().is_none()));
    assert!(!cx.update(|cx| page.read(cx).history_open()));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_COMPARE_BEFORE));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_COMPARE_HANDLE));
    // With the comparison gone, a new screen lays nothing out for it.
    cx.update(|cx| {
        page.update(cx, |p, _| {
            p.editor_mut().unwrap().set_screen(Screen::new(1280, 1024))
        })
    });
    assert_eq!(
        cx.update(|cx| page.read(cx).editor().unwrap().compare_builds()),
        builds
    );
}

#[gpui_kit::test]
fn starting_the_six_screen_view_stops_comparing(cx: &mut TestAppContext) {
    let (_env, s, page, _file) = comparing_red(cx);
    support::click_id(cx, s.win, ids::HUD_MAKER_MORE);
    support::click_id(cx, s.win, ids::HUD_MAKER_EVERY_SCREEN);
    support::settle(cx, s.win);
    assert!(cx.update(|cx| page.read(cx).every_screen()));
    assert!(cx.update(|cx| page.read(cx).comparing().is_none()));
    assert!(cx.update(|cx| page.read(cx).editor().unwrap().compared().is_none()));
    // Leaving the view finds the plain canvas, not a comparison.
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| p.set_every_screen(false, window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_COMPARE_BEFORE));
}

#[gpui_kit::test]
fn compare_is_refused_in_the_six_screen_view(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    recolour(cx, &page, "255 0 0 255");
    save_hud(cx, &s, &page);
    open_history(cx, &s);
    let file = take_snapshot(cx, &s, &page, "Red");
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| {
            // One request already reading when the view starts must not land either.
            p.compare_snapshot(&file, window, cx);
            p.set_every_screen(true, window, cx);
            p.compare_snapshot(&file, window, cx);
        })
    })
    .unwrap();
    support::settle(cx, s.win);
    std::thread::sleep(std::time::Duration::from_millis(300));
    support::settle(cx, s.win);
    assert!(cx.update(|cx| page.read(cx).comparing().is_none()));
    let e = cx.update(|cx| {
        let e = page.read(cx).editor().unwrap();
        (e.compared().is_some(), e.compare_builds())
    });
    assert_eq!(e, (false, 0));
}

#[gpui_kit::test]
fn the_captions_sit_beside_the_divider(cx: &mut TestAppContext) {
    let (_env, s, _page, _file) = comparing_red(cx);
    let divider = bounds(cx, &s, ids::HUD_MAKER_COMPARE_DIVIDER);
    let before = bounds(cx, &s, ids::HUD_MAKER_COMPARE_LABEL_BEFORE);
    let now = bounds(cx, &s, ids::HUD_MAKER_COMPARE_LABEL_NOW);
    assert!(
        before.right() <= divider.left() + px(1.),
        "{before:?} {divider:?}"
    );
    assert!(
        f(divider.left() - before.right()) < 16.,
        "{before:?} {divider:?}"
    );
    assert!(
        now.left() >= divider.right() - px(1.),
        "{now:?} {divider:?}"
    );
    assert!(f(now.left() - divider.right()) < 16., "{now:?} {divider:?}");
}

#[gpui_kit::test]
fn a_divider_near_the_edge_keeps_its_captions_on_the_screen(cx: &mut TestAppContext) {
    let (_env, s, page, _file) = comparing_red(cx);
    let canvas = bounds(cx, &s, ids::HUD_MAKER_CANVAS);
    for split in [0.02, 0.98] {
        cx.update(|cx| page.update(cx, |p, cx| p.set_compare_split(split, cx)));
        support::settle(cx, s.win);
        for id in [
            ids::HUD_MAKER_COMPARE_LABEL_BEFORE,
            ids::HUD_MAKER_COMPARE_LABEL_NOW,
        ] {
            let b = bounds(cx, &s, id);
            assert!(
                b.left() >= canvas.left() - px(1.) && b.right() <= canvas.right() + px(1.),
                "{id} at {split}: {b:?} in {canvas:?}"
            );
        }
    }
}

#[gpui_kit::test]
fn a_long_snapshot_name_is_cut_short_in_its_caption(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    save_hud(cx, &s, &page);
    open_history(cx, &s);
    let file = take_snapshot(cx, &s, &page, &"W".repeat(80));
    click_in_panel(cx, &s, ids::hud_maker_snapshot_compare(&file));
    support::wait_real(cx, "the comparison", |cx| {
        cx.update(|cx| page.read(cx).comparing().is_some())
    });
    support::settle(cx, s.win);
    let before = bounds(cx, &s, ids::HUD_MAKER_COMPARE_LABEL_BEFORE);
    assert!(f(before.size.width) <= 240., "{before:?}");
}

#[gpui_kit::test]
fn the_before_caption_keeps_clear_of_the_colour_blind_badge(cx: &mut TestAppContext) {
    let (_env, s, page, _file) = comparing_red(cx);
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            p.set_vision(tf2tools_core::Vision::Protanopia, cx);
            p.set_compare_split(0.02, cx);
        })
    });
    support::settle(cx, s.win);
    let badge = bounds(cx, &s, ids::HUD_MAKER_VISION_BADGE);
    let before = bounds(cx, &s, ids::HUD_MAKER_COMPARE_LABEL_BEFORE);
    assert!(!badge.intersects(&before), "{badge:?} {before:?}");
}

#[gpui_kit::test]
fn escape_stops_comparing_before_it_clears_the_selection(cx: &mut TestAppContext) {
    let (_env, s, page, _file) = comparing_red(cx);
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            p.editor_mut().unwrap().select(Some(PartKey::new(HV)));
            cx.notify();
        })
    });
    support::settle(cx, s.win);
    // The grip's press hands the page the keys; Escape then arrives at the page.
    support::click_id(cx, s.win, ids::HUD_MAKER_COMPARE_HANDLE);
    support::press(cx, s.win, "escape");
    assert!(cx.update(|cx| page.read(cx).comparing().is_none()));
    assert!(cx.update(|cx| page.read(cx).editor().unwrap().selected().is_some()));
    support::press(cx, s.win, "escape");
    assert!(cx.update(|cx| page.read(cx).editor().unwrap().selected().is_none()));
}

#[gpui_kit::test]
fn a_press_on_the_grip_gives_the_page_focus(cx: &mut TestAppContext) {
    let (_env, s, _page, _file) = comparing_red(cx);
    cx.update_window(s.win, |_, w, cx| w.blur(cx)).unwrap();
    support::settle(cx, s.win);
    let root = ids::page(Page::HudMaker);
    assert_ne!(support::focused_of(cx, s.win, root.clone()), Some(true));
    support::click_id(cx, s.win, ids::HUD_MAKER_COMPARE_HANDLE);
    assert_eq!(support::focused_of(cx, s.win, root), Some(true));
}

const CROSS: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthImage";
const CROSS_VMT: &str = "materials/hud/health_color.vmt";

fn has_cross(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> bool {
    cx.update(|cx| {
        let Some(e) = page.read(cx).editor() else {
            return false;
        };
        e.model()
            .assets()
            .iter()
            .any(|(p, s)| p == CROSS_VMT && e.assets().get(s).is_some())
    })
}

#[gpui_kit::test]
fn a_snapshot_of_an_unsaved_pick_restores_it_after_undo_save_and_reopen(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    recolour(cx, &page, "255 0 0 255");
    let id = save_hud(cx, &s, &page);
    let texture = tf2tools_core::hud_texture_vtf(
        &tf2tools_core::RgbaImage::from_pixels(2, 2, vec![50; 16]).unwrap(),
    )
    .unwrap();
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            let e = p.editor_mut().unwrap();
            e.select(Some(PartKey::new(CROSS)));
            e.replace_picture_selected(texture).unwrap();
            e.select(None);
            cx.notify();
        })
    });
    open_history(cx, &s);
    let file = take_snapshot(cx, &s, &page, "Cross");
    undo(cx, &s, &page);
    support::click_id(cx, s.win, ids::HUD_MAKER_SAVE);
    support::wait_real(cx, "the second save", |cx| {
        cx.update(|cx| page.read(cx).editor().is_some_and(|e| !e.is_dirty()))
    });
    // Reopen, so the editor holds only what is on disk.
    cx.update(|cx| page.update(cx, |p, cx| p.open(id.clone(), cx)));
    support::wait_real(cx, "the reopen", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    assert!(!has_cross(cx, &page));
    if !support::present(cx, s.win, ids::hud_maker_snapshot(&file)) {
        open_history(cx, &s);
    }
    click_in_panel(cx, &s, ids::hud_maker_snapshot_restore(&file));
    support::wait_real(cx, "the restored cross", |cx| has_cross(cx, &page));
}
