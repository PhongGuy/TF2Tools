//! The HUD Maker's Parts list: the eye on each row, the changed mark, the filter and folding.

mod support;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{PartKey, Tf2Install};

const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";
const HEALTH: &str = "HudPlayerStatus/HudPlayerHealth";
const AMMO: &str = "HudWeaponAmmo";

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
    toggle_parts(cx, &s);
    (s, page)
}

fn toggle_parts(cx: &mut TestAppContext, s: &support::ShellWindow) {
    support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_COLLAPSE);
}

fn filter(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    text: &str,
) {
    let input = cx.update(|cx| page.read(cx).parts_filter().clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.set_value(text, window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
}

fn selection(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> Option<String> {
    cx.update(|cx| {
        let e = page.read(cx).editor()?;
        e.selected().map(|k| k.as_str().to_owned())
    })
}

#[gpui_kit::test]
fn the_eye_hides_and_shows_a_part_without_selecting_it(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = open(cx, &env);
    support::reveal_part_row(cx, s.win, &key(HV));
    let eye = ids::hud_maker_row_eye(&key(HV));
    assert_eq!(
        support::label_of(cx, s.win, eye.clone()).as_deref(),
        Some("Hide this part")
    );
    support::click_id(cx, s.win, eye.clone());
    assert!(!support::present(cx, s.win, ids::hud_maker_part(&key(HV))));
    assert_eq!(selection(cx, &page), None, "the eye does not select");
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_row(&key(HV))).as_deref(),
        Some("Health value (hidden)")
    );
    assert_eq!(
        support::label_of(cx, s.win, eye.clone()).as_deref(),
        Some("Show this part")
    );
    // One undo step brings it back.
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            assert!(p.editor_mut().unwrap().undo());
            cx.notify();
        })
    });
    support::settle(cx, s.win);
    assert!(support::present(cx, s.win, ids::hud_maker_part(&key(HV))));
    support::click_id(cx, s.win, eye.clone());
    support::click_id(cx, s.win, eye);
    assert!(support::present(cx, s.win, ids::hud_maker_part(&key(HV))));
    assert!(cx.update(|cx| page.read(cx).editor().unwrap().changes().is_empty()));
}

#[gpui_kit::test]
fn a_dot_marks_a_part_changed_from_the_starting_hud(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = open(cx, &env);
    support::reveal_part_row(cx, s.win, &key(HV));
    let dot = ids::hud_maker_row_changed(&key(HV));
    assert!(!support::present(cx, s.win, dot.clone()));
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            let e = p.editor_mut().unwrap();
            e.select(Some(key(HV)));
            e.move_selected(45, 0).unwrap();
            cx.notify();
        })
    });
    support::settle(cx, s.win);
    assert_eq!(
        support::label_of(cx, s.win, dot.clone()).as_deref(),
        Some("Changed")
    );
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_row_changed(&key(HEALTH))
    ));
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            p.editor_mut().unwrap().undo();
            cx.notify();
        })
    });
    support::settle(cx, s.win);
    assert!(!support::present(cx, s.win, dot));
}

#[gpui_kit::test]
fn the_filter_keeps_matches_and_the_groups_they_sit_in(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = open(cx, &env);
    filter(cx, &s, &page, "HEALTH VALUE");
    assert!(support::present(cx, s.win, ids::hud_maker_row(&key(HV))));
    // Every group above the match stays, so the match keeps its place in the tree.
    let ancestors: Vec<PartKey> = cx.update(|cx| {
        let model = page.read(cx).editor().unwrap().model();
        std::iter::successors(model.part(&key(HV)).unwrap().parent().cloned(), |k| {
            model.part(k)?.parent().cloned()
        })
        .collect()
    });
    assert!(!ancestors.is_empty());
    for a in &ancestors {
        assert!(support::present(cx, s.win, ids::hud_maker_row(a)), "{a:?}");
    }
    assert!(!support::present(cx, s.win, ids::hud_maker_row(&key(AMMO))));
    filter(cx, &s, &page, "no part is called this");
    assert!(support::present(cx, s.win, ids::HUD_MAKER_PARTS_NO_MATCH));
    filter(cx, &s, &page, "");
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_PARTS_NO_MATCH));
    support::reveal_part_row(cx, s.win, &key(AMMO));
}

#[gpui_kit::test]
fn a_group_folds_and_stays_folded_while_the_page_is_open(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, _page) = open(cx, &env);
    support::reveal_part_row(cx, s.win, &key(HV));
    let fold = ids::hud_maker_row_fold(&key(HEALTH));
    assert_eq!(
        support::label_of(cx, s.win, fold.clone()).as_deref(),
        Some("Fold")
    );
    assert!(
        !support::present(cx, s.win, ids::hud_maker_row_fold(&key(HV))),
        "HV holds nothing"
    );
    support::click_id(cx, s.win, fold.clone());
    assert!(!support::present(cx, s.win, ids::hud_maker_row(&key(HV))));
    assert!(support::present(
        cx,
        s.win,
        ids::hud_maker_row(&key(HEALTH))
    ));
    assert_eq!(
        support::label_of(cx, s.win, fold.clone()).as_deref(),
        Some("Unfold")
    );
    // Closing and opening the panel keeps the fold.
    toggle_parts(cx, &s);
    toggle_parts(cx, &s);
    support::reveal_part_row(cx, s.win, &key(HEALTH));
    assert!(!support::present(cx, s.win, ids::hud_maker_row(&key(HV))));
    support::click_id(cx, s.win, fold);
    assert!(support::present(cx, s.win, ids::hud_maker_row(&key(HV))));
}

#[gpui_kit::test]
fn the_row_still_selects_and_lights_its_part_on_hover(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = open(cx, &env);
    support::click_part_row(cx, s.win, &key(HV));
    assert_eq!(selection(cx, &page).as_deref(), Some(HV));
    cx.update_window(s.win, |_, w, cx| w.hover(ids::hud_maker_row(&key(HV)), cx))
        .ok();
    support::settle(cx, s.win);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_HOVER));
}

#[gpui_kit::test]
fn a_long_name_beside_the_shows_in_game_mark_stays_readable(cx: &mut TestAppContext) {
    let env = support::env();
    let medic = r#""Resource/UI/HudMedicCharge.res" { ChargeLabel { ControlName CExLabel xpos 0 ypos 0 wide 100 tall 20 } UberchargeIndividualChargesLabel { ControlName CExLabel xpos 0 ypos 30 wide 100 tall 20 } }"#;
    support::seed_stock_game_with(
        &env.game,
        &[("resource/ui/hudmediccharge.res", Some(medic))],
    );
    let found = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    let s = support::shell_window(cx, &env, found);
    support::click_nav(cx, s.win, Page::HudMaker);
    let page = cx
        .update(|cx| s.shell.read(cx).hud_maker_page())
        .expect("a HUD Maker page");
    support::wait_real(cx, "the HUD to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    toggle_parts(cx, &s);
    let long = key("HudMedicCharge/UberchargeIndividualChargesLabel");
    support::reveal_part_row(cx, s.win, &long);
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_row(&long)).as_deref(),
        Some("Ubercharge individual charges label (shows in game)")
    );
    // The mark is one small icon, so the name keeps the rest of the row.
    let bounds = |cx: &mut TestAppContext, id| support::bounds_of(cx, s.win, id).unwrap();
    let row = bounds(cx, ids::hud_maker_row(&long));
    let name = bounds(cx, ids::hud_maker_row_name(&long));
    let mark = bounds(cx, ids::hud_maker_row_state(&long));
    assert!(
        mark.size.width <= gpui_kit::px(20.),
        "{mark:?} is wider than an icon"
    );
    assert!(
        name.size.width >= row.size.width / 2.,
        "{name:?} gets too little of {row:?}"
    );
    // Hovering the mark says what it means.
    cx.update_window(s.win, |_, w, cx| {
        w.hover(ids::hud_maker_row_state(&long), cx)
    })
    .unwrap();
    support::settle(cx, s.win);
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(800));
    support::settle(cx, s.win);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_TIP).as_deref(),
        Some("Shows in game only when TF2 needs it")
    );
}
