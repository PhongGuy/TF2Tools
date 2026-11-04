//! The Minimal HUD switch in the situation menu: it holds on every tab, redraws the HUD and
//! edits follow it.

mod support;

use gpui_kit::{AppContext as _, Entity, TestAppContext};
use support::maker_page::open_maker;
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::{Field, HudMakerPage};
use tf2tools_core::{PartKey, Scene};

const HEALTH: &str = "HudPlayerStatus/HudPlayerHealth";

fn on(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> bool {
    cx.update(|cx| page.read(cx).minmode())
}

/// The winning line of `k` on part `key` as (its real key, its text).
fn won(
    cx: &mut TestAppContext,
    page: &Entity<HudMakerPage>,
    key: &str,
    k: &str,
) -> (String, String) {
    cx.update(|cx| {
        let e = page.read(cx).editor().unwrap();
        let line = e
            .model()
            .part(&PartKey::new(key))
            .unwrap()
            .value(k)
            .unwrap();
        (line.key().to_owned(), line.text().to_owned())
    })
}

/// Flips the Minimal HUD switch from the HUD tab's situation menu.
fn flip(cx: &mut TestAppContext, s: &support::ShellWindow) {
    support::click_id(cx, s.win, ids::HUD_MAKER_SITUATION);
    support::click_id(cx, s.win, ids::HUD_MAKER_MINMODE);
}

fn commit(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    field: Field,
    text: &str,
) {
    use gpui_kit::component::input::InputEvent;
    let input = cx.update(|cx| page.read(cx).field(field).clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| {
            st.set_value(text, window, cx);
            cx.emit(InputEvent::PressEnter {
                secondary: false,
                shift: false,
            });
        })
    })
    .unwrap();
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn the_switch_holds_on_every_tab_and_redraws_the_hud(cx: &mut TestAppContext) {
    let (_env, s, page) = open_maker(cx);
    support::click_id(cx, s.win, ids::HUD_MAKER_SITUATION);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_MINMODE));
    support::click_id(cx, s.win, ids::HUD_MAKER_SITUATION);
    assert!(!on(cx, &page));
    assert_eq!(won(cx, &page, HEALTH, "xpos"), ("xpos".into(), "0".into()));
    flip(cx, &s);
    assert!(on(cx, &page));
    assert_eq!(
        won(cx, &page, HEALTH, "xpos"),
        ("xpos_minmode".into(), "-5".into())
    );
    support::click_scene(cx, s.win, Scene::Scoreboard);
    assert!(on(cx, &page), "the switch belongs to the page, not the tab");
    support::click_scene(cx, s.win, Scene::Hud);
    assert_eq!(
        won(cx, &page, HEALTH, "xpos"),
        ("xpos_minmode".into(), "-5".into())
    );
    flip(cx, &s);
    assert!(!on(cx, &page));
}

#[gpui_kit::test]
fn the_note_shows_only_for_a_scene_without_minimal_keys(cx: &mut TestAppContext) {
    let (_env, s, _page) = open_maker(cx);
    flip(cx, &s);
    assert!(
        !support::present(cx, s.win, ids::HUD_MAKER_MINMODE_NOTE),
        "health has xpos_minmode"
    );
    support::click_scene(cx, s.win, Scene::Scoreboard);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_MINMODE_NOTE).as_deref(),
        Some("This HUD looks the same with the minimal HUD on.")
    );
    support::click_scene(cx, s.win, Scene::Hud);
    flip(cx, &s);
    support::click_scene(cx, s.win, Scene::Scoreboard);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_MINMODE_NOTE));
}

#[gpui_kit::test]
fn an_x_typed_in_minimal_mode_moves_only_the_minimal_hud_and_undoes(cx: &mut TestAppContext) {
    let (_env, s, page) = open_maker(cx);
    flip(cx, &s);
    if !support::present(cx, s.win, ids::HUD_MAKER_PARTS) {
        support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_COLLAPSE);
    }
    support::click_part_row(cx, s.win, &PartKey::new(HEALTH));
    commit(cx, &s, &page, Field::X, "7");
    // The X field is the box's screen pixel, so 7 px lands on whole file unit 3.
    assert_eq!(
        won(cx, &page, HEALTH, "xpos"),
        ("xpos_minmode".into(), "3".into())
    );
    flip(cx, &s);
    assert_eq!(won(cx, &page, HEALTH, "xpos"), ("xpos".into(), "0".into()));
    support::click_id(cx, s.win, ids::HUD_MAKER_UNDO);
    flip(cx, &s);
    assert_eq!(
        won(cx, &page, HEALTH, "xpos"),
        ("xpos_minmode".into(), "-5".into())
    );
}

#[test]
fn a_switch_reload_keeps_the_mode() {
    use tf2tools_app::pages::hud_maker::editor::HudEditor;
    use tf2tools_core::{HudModel, Platform};
    let load = || HudModel::load(&support::hud_stock::stock_files(), Platform::Linux).unwrap();
    let mut e = HudEditor::new(load(), Vec::new());
    e.set_minmode(true);
    assert!(e.minmode());
    let choices = e.switch_choices();
    assert!(
        e.rebase(load(), choices),
        "a fresh model with the same choices is taken"
    );
    assert!(e.minmode(), "the reload keeps the minimal HUD");
    let x = e
        .model()
        .part(&PartKey::new(HEALTH))
        .unwrap()
        .value("xpos")
        .unwrap();
    assert_eq!((x.key(), x.text()), ("xpos_minmode", "-5"));
}

#[gpui_kit::test]
fn the_switch_carries_the_shared_tip_holder(cx: &mut TestAppContext) {
    let (_env, s, _page) = open_maker(cx);
    support::click_id(cx, s.win, ids::HUD_MAKER_SITUATION);
    let holder = ids::hud_maker_tip_for(&ids::HUD_MAKER_MINMODE.into());
    assert!(support::present(cx, s.win, holder));
}

#[gpui_kit::test]
fn another_tab_shows_a_chip_that_turns_the_minimal_hud_off(cx: &mut TestAppContext) {
    let (_env, s, page) = open_maker(cx);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_MINMODE_CHIP));
    flip(cx, &s);
    assert!(
        !support::present(cx, s.win, ids::HUD_MAKER_MINMODE_CHIP),
        "the HUD tab has the switch itself"
    );
    support::click_scene(cx, s.win, Scene::Scoreboard);
    let holder = ids::hud_maker_tip_for(&ids::HUD_MAKER_MINMODE_CHIP.into());
    assert!(support::present(cx, s.win, holder));
    support::click_id(cx, s.win, ids::HUD_MAKER_MINMODE_CHIP);
    assert!(!on(cx, &page));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_MINMODE_CHIP));
}

#[gpui_kit::test]
fn the_minimal_hud_stays_on_when_another_hud_opens(cx: &mut TestAppContext) {
    let (_env, s, page) = open_maker(cx);
    flip(cx, &s);
    let saved = s
        .services
        .library
        .save_made_hud(
            &tf2tools_core::MakeTarget::New,
            "Made",
            &support::hud_stock::stock_files(),
            tf2tools_core::Platform::Linux,
            &[],
        )
        .unwrap();
    let id = saved.entry().id().clone();
    cx.update(|cx| page.update(cx, |p, cx| p.open(id, cx)));
    support::wait_real(cx, "the made HUD", |cx| {
        cx.update(|cx| page.read(cx).saved_hud().is_some())
    });
    support::settle(cx, s.win);
    assert!(on(cx, &page), "the switch lasts the session");
    assert_eq!(
        won(cx, &page, HEALTH, "xpos"),
        ("xpos_minmode".into(), "-5".into())
    );
}
