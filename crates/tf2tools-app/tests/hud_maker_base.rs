//! A scene file that is a #base stub: shown, pickable, read-only, with one plain note.

mod support;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_app::services::Tf2State;
use tf2tools_core::{PartKey, Scene, Tf2Install};

const NAME: &str = "FreezePanel/FreezePanelBase/FreezeLabelKiller";
const NOTE: &str =
    "This part comes from another file the HUD includes, so it can't be changed here yet";
const MISSING: &str =
    "Some files this part builds on weren't found, so it may look different in the game.";
const VARIANT: &str = "resource/ui/freezepanel_basic_variant.res";

fn open_maker(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<HudMakerPage> {
    support::click_nav(cx, s.win, tf2tools_app::shell::route::Page::HudMaker);
    let page = cx.update(|cx| s.shell.read(cx).hud_maker_page()).unwrap();
    support::wait_real(cx, "the HUD to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

fn stub_shell(cx: &mut TestAppContext, env: &support::Env, stub: &str) -> support::ShellWindow {
    support::seed_stock_game_with(
        &env.game,
        &[
            (VARIANT, Some(support::hud_stock::FREEZE_PANEL)),
            ("resource/ui/freezepanel_basic.res", Some(stub)),
        ],
    );
    support::shell_window(
        cx,
        env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    )
}

// The part list is open by default in a roomy window; open it if a narrow one folded it.
fn show_parts(cx: &mut TestAppContext, s: &support::ShellWindow) {
    if !support::present(cx, s.win, ids::HUD_MAKER_PARTS) {
        support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_COLLAPSE);
    }
}

#[gpui_kit::test]
fn a_stub_freeze_cam_shows_its_parts_and_says_why_they_are_fixed(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stub_shell(cx, &env, "#base \"freezepanel_basic_variant.res\"\n");
    let page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::FreezeCam);
    assert!(support::present(
        cx,
        s.win,
        ids::hud_maker_part(&PartKey::new(NAME))
    ));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_SCENE_NOTE));
    show_parts(cx, &s);
    support::click_part_row(cx, s.win, &PartKey::new(NAME));
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_PANEL_FROM_BASE).as_deref(),
        Some(NOTE)
    );
    assert!(!support::present(
        cx,
        s.win,
        ids::HUD_MAKER_PANEL_BASE_MISSING
    ));
    assert!(!support::present(
        cx,
        s.win,
        ids::HUD_MAKER_PANEL_UNREADABLE
    ));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_LAYER));
    assert!(cx.update(|cx| page.read(cx).editor().unwrap().changes().is_empty()));
}

#[gpui_kit::test]
fn a_stub_with_a_base_missing_adds_the_second_line(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stub_shell(
        cx,
        &env,
        "#base \"freezepanel_basic_variant.res\"\n#base \"gone.res\"\n",
    );
    let _page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::FreezeCam);
    show_parts(cx, &s);
    support::click_part_row(cx, s.win, &PartKey::new(NAME));
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_PANEL_BASE_MISSING).as_deref(),
        Some(MISSING)
    );
}

const NUDGE: &str =
    "This part comes from another file the HUD includes, so it can't be moved here yet.";

fn select_part(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    key: &str,
) {
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            p.editor_mut().unwrap().select(Some(PartKey::new(key)));
            cx.notify();
        })
    });
    support::settle(cx, s.win);
}

// Focuses the canvas with a click at its far corner, where no freeze cam part sits.
fn focus_canvas(cx: &mut TestAppContext, s: &support::ShellWindow) {
    let canvas = support::bounds_of(cx, s.win, ids::HUD_MAKER_CANVAS).unwrap();
    let at = gpui_kit::point(canvas.size.width * 0.99, canvas.size.height * 0.99);
    cx.update_window(s.win, |_, w, cx| w.click_at(ids::HUD_MAKER_CANVAS, at, cx))
        .unwrap();
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn an_arrow_key_on_a_stub_part_says_why_and_records_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stub_shell(cx, &env, "#base \"freezepanel_basic_variant.res\"\n");
    let page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::FreezeCam);
    focus_canvas(cx, &s);
    select_part(cx, &s, &page, NAME);
    support::press(cx, s.win, "right");
    assert_eq!(support::toast_labels(cx, s.win), [NUDGE]);
    assert!(!cx.update(|cx| page.read(cx).editor().unwrap().can_undo()));
}
