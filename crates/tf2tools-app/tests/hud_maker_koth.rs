//! The King of the Hill switch and Show it on the KOTH timer.

use crate::support;

use gpui_kit::{Entity, TestAppContext};
use support::maker_page::open_maker;
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_core::{PartKey, SituationSwitch};

const KOTH_VALUE: &str = "HudKothTimeStatus/BlueTimer/TimePanelValue";
const ROUND: &str = "HudMatchStatus/ObjectiveStatusTimePanel";

fn koth_on(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> bool {
    cx.update(|cx| {
        page.read(cx)
            .editor()
            .unwrap()
            .situation()
            .is_on(SituationSwitch::Koth)
    })
}

fn shown(cx: &mut TestAppContext, s: &support::ShellWindow, key: &str) -> bool {
    support::present(cx, s.win, ids::hud_maker_part(&PartKey::new(key)))
}

#[gpui_kit::test]
fn the_switch_swaps_the_round_timer_for_the_koth_timer(cx: &mut TestAppContext) {
    let (_env, s, page) = open_maker(cx);
    assert!(shown(cx, &s, ROUND) && !shown(cx, &s, KOTH_VALUE));
    support::click_id(cx, s.win, ids::HUD_MAKER_SITUATION);
    assert_eq!(
        support::label_of(
            cx,
            s.win,
            ids::hud_maker_situation_switch(SituationSwitch::Koth)
        )
        .as_deref(),
        Some("King of the Hill map")
    );
    support::click_id(
        cx,
        s.win,
        ids::hud_maker_situation_switch(SituationSwitch::Koth),
    );
    assert!(koth_on(cx, &page));
    assert!(shown(cx, &s, KOTH_VALUE) && !shown(cx, &s, ROUND));
}

#[gpui_kit::test]
fn show_it_on_the_koth_timer_turns_koth_on_and_back(cx: &mut TestAppContext) {
    let (_env, s, page) = open_maker(cx);
    if !support::present(cx, s.win, ids::HUD_MAKER_PARTS) {
        support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_COLLAPSE);
    }
    support::click_part_row(cx, s.win, &PartKey::new("HudKothTimeStatus"));
    support::click_id(cx, s.win, ids::HUD_MAKER_PANEL_SHOW_IT);
    assert!(koth_on(cx, &page));
    support::click_part_row(cx, s.win, &PartKey::new(ROUND));
    support::click_id(cx, s.win, ids::HUD_MAKER_PANEL_SHOW_IT);
    assert!(
        !koth_on(cx, &page),
        "Show it on the round timer leaves KOTH"
    );
}

#[gpui_kit::test]
fn the_koth_clocks_show_a_round_in_progress(cx: &mut TestAppContext) {
    let (_env, s, _page) = open_maker(cx);
    support::click_id(cx, s.win, ids::HUD_MAKER_SITUATION);
    support::click_id(
        cx,
        s.win,
        ids::hud_maker_situation_switch(SituationSwitch::Koth),
    );
    let text = |cx: &mut TestAppContext, timer: &str| {
        let key = PartKey::new(format!("HudKothTimeStatus/{timer}/TimePanelValue"));
        support::label_of(cx, s.win, ids::hud_maker_text(&key))
    };
    assert_eq!(text(cx, "RedTimer").as_deref(), Some("3:00"));
    assert_eq!(text(cx, "BlueTimer").as_deref(), Some("2:14"));
}
