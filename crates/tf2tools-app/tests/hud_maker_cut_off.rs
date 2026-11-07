//! A selected part poking out of its parent on the HUD Maker page: the canvas outlines its
//! whole box, the Part panel says the game cuts it off, and its button grows the parent.

mod support;

use gpui_kit::{Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_core::PartKey;

const TIME: &str = "HudMatchStatus/ObjectiveStatusTimePanel/TimePanelValue";

/// The round timer's text selected and nudged below the round timer's box.
fn timer_pushed_down(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
) {
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            let e = p.editor_mut().unwrap();
            e.select(Some(PartKey::new(TIME)));
            e.nudge_selected(0, 40).unwrap();
            cx.notify();
        })
    });
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn a_cut_off_part_is_marked_and_one_click_grows_its_parent(cx: &mut TestAppContext) {
    let (_env, s, page) = support::maker_page::open_maker(cx);
    support::put_start_card_away(cx, s.win);
    let mark = ids::hud_maker_cut_off(&PartKey::new(TIME));
    assert!(
        !support::present(cx, s.win, mark.clone()),
        "nothing is marked yet"
    );
    timer_pushed_down(cx, &s, &page);
    assert!(support::present(cx, s.win, mark.clone()));
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_PANEL_CUT_OFF).as_deref(),
        Some("Part of this is outside Round timer, so TF2 cuts it off.")
    );
    support::click_id(cx, s.win, ids::HUD_MAKER_GROW_PARENT);
    assert!(
        !support::present(cx, s.win, mark),
        "the mark goes once it fits"
    );
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_PANEL_CUT_OFF));
    assert_eq!(
        cx.update(|cx| page.read(cx).undo_tip()),
        "Undo grow Round timer to fit (Ctrl+Z)"
    );
}
