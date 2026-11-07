//! The HUD Maker's ways in: the Start from card on a new HUD, the keyboard shortcuts sheet,
//! the hide and deselect keys, Undo and Redo naming their step, and Save and leave.

mod support;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{PartKey, Tf2Install};

const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";

fn key(k: &str) -> PartKey {
    PartKey::new(k)
}

fn open(
    cx: &mut TestAppContext,
    env: &support::Env,
) -> (support::ShellWindow, Entity<HudMakerPage>) {
    support::seed_stock_game(&env.game);
    let found = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    let s = support::shell_window(cx, env, found);
    support::click_nav(cx, s.win, Page::HudMaker);
    let page = cx
        .update(|cx| s.shell.read(cx).hud_maker_page())
        .expect("a HUD Maker page");
    support::wait_real(cx, "the HUD to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    (s, page)
}

fn page_now(cx: &mut TestAppContext, s: &support::ShellWindow) -> Page {
    cx.update(|cx| s.shell.read(cx).page())
}

fn selection(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> Option<String> {
    cx.update(|cx| {
        let e = page.read(cx).editor()?;
        e.selected().map(|k| k.as_str().to_owned())
    })
}

// Selects HV from its Parts list row, which also hands the page the keys.
fn select_hv(cx: &mut TestAppContext, s: &support::ShellWindow) {
    support::click_id(cx, s.win, ids::HUD_MAKER_START_BLANK);
    support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_COLLAPSE);
    support::click_part_row(cx, s.win, &key(HV));
    support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_COLLAPSE);
}

fn hidden(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> bool {
    cx.update(|cx| {
        page.read(cx)
            .editor()
            .unwrap()
            .part_hidden_by_player(&key(HV))
    })
}

// Hovers a toolbar button's holder and reads the tooltip it shows.
fn tip_of(cx: &mut TestAppContext, s: &support::ShellWindow, id: &'static str) -> Option<String> {
    let holder = ids::hud_maker_tip_for(&id.into());
    cx.update_window(s.win, |_, w, cx| w.hover(holder, cx))
        .unwrap();
    support::settle(cx, s.win);
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(800));
    support::settle(cx, s.win);
    support::label_of(cx, s.win, ids::HUD_MAKER_TIP)
}

#[gpui_kit::test]
fn a_new_hud_offers_where_to_start(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _page) = open(cx, &env);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_START_CARD));
    for id in [
        ids::HUD_MAKER_START_READY,
        ids::HUD_MAKER_START_MINE,
        ids::HUD_MAKER_START_BLANK,
    ] {
        assert!(support::present(cx, s.win, id), "{id} missing");
    }
    // A ready-made HUD opens its starting points in the card; a pick previews it.
    support::click_id(cx, s.win, ids::HUD_MAKER_START_READY);
    let point = ids::hud_maker_start_card_point("Competitive minimal");
    support::click_id(cx, s.win, point);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_PREVIEW));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_START_CARD));
}

#[gpui_kit::test]
fn blank_keeps_tf2s_own_hud_and_closes_the_card(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = open(cx, &env);
    support::click_id(cx, s.win, ids::HUD_MAKER_START_BLANK);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_START_CARD));
    assert!(cx.update(|cx| page.read(cx).editor().unwrap().changes().is_empty()));
}

#[gpui_kit::test]
fn one_of_my_huds_goes_to_the_library(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _page) = open(cx, &env);
    support::click_id(cx, s.win, ids::HUD_MAKER_START_MINE);
    assert_eq!(page_now(cx, &s), Page::Huds);
}

#[gpui_kit::test]
fn the_card_goes_once_the_hud_changes(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = open(cx, &env);
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            let e = p.editor_mut().unwrap();
            e.select(Some(key(HV)));
            e.nudge_selected(5, 0).unwrap();
            cx.notify();
        })
    });
    support::settle(cx, s.win);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_START_CARD));
}

#[gpui_kit::test]
fn the_question_mark_opens_the_shortcuts(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _page) = open(cx, &env);
    support::click_id(cx, s.win, ids::HUD_MAKER_START_BLANK);
    support::click_id(cx, s.win, ids::HUD_MAKER_SHORTCUTS);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_SHORTCUTS_SHEET));
    let rows: Vec<String> = (0..tf2tools_app::pages::hud_maker::SHORTCUTS.len())
        .filter_map(|i| support::label_of(cx, s.win, ids::hud_maker_shortcut(i)))
        .collect();
    for want in [
        "Arrow keys: Move the selected part",
        "H or Delete: Hide the selected part",
        "Escape: Clear the selection",
        "Ctrl+Z: Undo",
        "Space + drag: Move around the canvas",
        "Ctrl + drag: Move without snapping",
    ] {
        assert!(rows.iter().any(|r| r == want), "{want} not in {rows:?}");
    }
    support::press(cx, s.win, "escape");
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_SHORTCUTS_SHEET));
    // The ? key opens the same sheet.
    support::press(cx, s.win, "shift-/");
    assert!(support::present(cx, s.win, ids::HUD_MAKER_SHORTCUTS_SHEET));
}

#[gpui_kit::test]
fn h_hides_the_selected_part_as_one_undo_step(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = open(cx, &env);
    select_hv(cx, &s);
    support::press(cx, s.win, "h");
    assert!(hidden(cx, &page), "H hid nothing");
    assert!(!support::present(cx, s.win, ids::hud_maker_part(&key(HV))));
    // H again shows it.
    support::press(cx, s.win, "h");
    assert!(!hidden(cx, &page), "a second H left it hidden");
    support::press(cx, s.win, "h");
    support::press(cx, s.win, "secondary-z");
    assert!(!hidden(cx, &page), "one undo did not bring it back");
}

#[gpui_kit::test]
fn delete_hides_the_selected_part_and_never_shows_it(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = open(cx, &env);
    select_hv(cx, &s);
    support::press(cx, s.win, "delete");
    assert!(hidden(cx, &page), "Delete hid nothing");
    support::press(cx, s.win, "delete");
    assert!(hidden(cx, &page), "a second Delete showed it again");
}

#[gpui_kit::test]
fn escape_clears_the_selection(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = open(cx, &env);
    select_hv(cx, &s);
    assert_eq!(selection(cx, &page).as_deref(), Some(HV));
    support::press(cx, s.win, "escape");
    assert_eq!(selection(cx, &page), None);
}

#[gpui_kit::test]
fn undo_and_redo_name_their_step(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _page) = open(cx, &env);
    assert_eq!(
        tip_of(cx, &s, ids::HUD_MAKER_UNDO).as_deref(),
        Some("Undo (Ctrl+Z)")
    );
    select_hv(cx, &s);
    support::press(cx, s.win, "right");
    assert_eq!(
        tip_of(cx, &s, ids::HUD_MAKER_UNDO).as_deref(),
        Some("Undo move Health value (Ctrl+Z)")
    );
    support::press(cx, s.win, "secondary-z");
    assert_eq!(
        tip_of(cx, &s, ids::HUD_MAKER_REDO).as_deref(),
        Some("Redo move Health value (Ctrl+Y)")
    );
    support::press(cx, s.win, "secondary-y");
    support::press(cx, s.win, "h");
    assert_eq!(
        tip_of(cx, &s, ids::HUD_MAKER_UNDO).as_deref(),
        Some("Undo hide Health value (Ctrl+Z)")
    );
}

#[gpui_kit::test]
fn save_and_leave_saves_then_goes(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = open(cx, &env);
    let name = cx.update(|cx| page.read(cx).name_input().clone());
    cx.update_window(s.win, |_, window, cx| {
        name.update(cx, |st, cx| st.set_value("Left in a hurry", window, cx))
    })
    .unwrap();
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            let e = p.editor_mut().unwrap();
            e.select(Some(key(HV)));
            e.nudge_selected(5, 0).unwrap();
            cx.notify();
        })
    });
    support::settle(cx, s.win);
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    assert!(support::present(cx, s.win, ids::CONFIRM_DIALOG));
    assert_eq!(
        support::label_of(cx, s.win, ids::CONFIRM_SAVE).as_deref(),
        Some("Save and leave")
    );
    support::click_id(cx, s.win, ids::CONFIRM_SAVE);
    support::wait_real(cx, "the save to land and the page to change", |cx| {
        support::settle(cx, s.win);
        page_now(cx, &s) == Page::Huds
    });
    let saved = s.services.library.list_huds().unwrap();
    assert!(
        saved.iter().any(|h| h.name() == "Left in a hurry"),
        "not saved"
    );
}

#[gpui_kit::test]
fn a_nameless_hud_is_only_offered_discard(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = open(cx, &env);
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            let e = p.editor_mut().unwrap();
            e.select(Some(key(HV)));
            e.nudge_selected(5, 0).unwrap();
            cx.notify();
        })
    });
    support::settle(cx, s.win);
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    assert!(support::present(cx, s.win, ids::CONFIRM_DIALOG));
    assert!(!support::present(cx, s.win, ids::CONFIRM_SAVE));
    let said = support::label_of(cx, s.win, ids::CONFIRM_DESCRIPTION).unwrap();
    assert!(said.contains("name"), "{said}");
}

#[gpui_kit::test]
fn picking_a_part_puts_the_card_away(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = open(cx, &env);
    support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_COLLAPSE);
    support::click_part_row(cx, s.win, &key(HV));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_START_CARD));
    support::press(cx, s.win, "escape");
    assert_eq!(selection(cx, &page), None);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_START_CARD));
}
