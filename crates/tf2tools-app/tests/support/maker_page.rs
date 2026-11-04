//! The HUD Maker on the stock game with animation files, shared by the Stage 6 tests.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Context, ElementId, Entity, ScrollDelta, TestAppContext, Window, point,
};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_app::pages::hud_maker::editor::HudEditor;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{HudModel, Platform, Tf2Install};

pub const VALUE: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";
pub const IMAGE: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthImage";
pub const MANIFEST: &str = "hudanimations_manifest { file scripts/hudanimations_tf.txt }";
pub const ANIMS: &str = r#"event HudHealthDyingPulse
{
    Animate PlayerStatusHealthValue FgColor "HUDRedTeamSolid" Linear 0.0 0.1
    Animate PlayerStatusHealthValue FgColor "TanDark" Accel 0.1 0.1
    RunEvent HudHealthDyingPulseLoop 0.25
}
event HudHealthDyingPulseLoop { RunEvent HudHealthDyingPulse 0.0 }
event HudMedicCharged { Animate ChargeLabel FgColor "TanLight" Linear 0.0 0.1 }
event DamagedPlayer { }
event TestNudge { Animate PlayerStatusHealthValue xpos "10" Linear 0 0 }
event Risky
{
    FireCommand 0 "engine echo hi"
    Animate PlayerStatusHealthValue Alpha "0" Linear 0 1
}
"#;

pub fn overrides() -> [(&'static str, Option<&'static str>); 2] {
    [
        ("scripts/hudanimations_manifest.txt", Some(MANIFEST)),
        ("scripts/hudanimations_tf.txt", Some(ANIMS)),
    ]
}

/// An editor over the stock files with ANIMS, nothing changed.
pub fn editor() -> HudEditor {
    let files = super::hud_stock::stock_files_with(&overrides());
    HudEditor::new(HudModel::load(&files, Platform::Linux).unwrap(), Vec::new())
}

/// The HUD Maker page, loaded, over a seeded stock game with ANIMS.
pub fn open_maker(
    cx: &mut TestAppContext,
) -> (super::Env, super::ShellWindow, Entity<HudMakerPage>) {
    open_maker_with(cx, ANIMS)
}

/// The HUD Maker page, loaded, over a seeded stock game whose animation file is `anims`.
pub fn open_maker_with(
    cx: &mut TestAppContext,
    anims: &str,
) -> (super::Env, super::ShellWindow, Entity<HudMakerPage>) {
    let env = super::env();
    let overrides = [
        ("scripts/hudanimations_manifest.txt", Some(MANIFEST)),
        ("scripts/hudanimations_tf.txt", Some(anims)),
    ];
    super::seed_stock_game_with(&env.game, &overrides);
    let state = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    let s = super::shell_window(cx, &env, state);
    super::click_nav(cx, s.win, Page::HudMaker);
    let page = cx
        .update(|cx| s.shell.read(cx).hud_maker_page())
        .expect("a HUD Maker page");
    super::wait_real(cx, "the HUD to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    super::settle(cx, s.win);
    (env, s, page)
}

/// Runs `f` on the page with its window, then settles.
pub fn on_page(
    cx: &mut TestAppContext,
    s: &super::ShellWindow,
    page: &Entity<HudMakerPage>,
    f: impl FnOnce(&mut HudMakerPage, &mut Window, &mut Context<HudMakerPage>),
) {
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| f(p, window, cx))
    })
    .unwrap();
    super::settle(cx, s.win);
}

/// Wheels the side panel until `id` shows, then clicks it.
pub fn click_in_panel(cx: &mut TestAppContext, s: &super::ShellWindow, id: ElementId) {
    for _ in 0..40 {
        let seen = super::present(cx, s.win, id.clone())
            && cx
                .update_window(s.win, |_, w, _| {
                    let body = w.try_find(ids::HUD_MAKER_PANEL_BODY).map(|b| b.bounds());
                    w.try_find(id.clone()).is_some_and(|e| {
                        let b = e.bounds();
                        e.visible() && body.is_none_or(|body| b.bottom() < body.bottom())
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
    super::click_id(cx, s.win, id);
}

/// Wheels the side panel until all of `id` lies inside its body, up when it sits above.
pub fn scroll_panel_to(cx: &mut TestAppContext, s: &super::ShellWindow, id: ElementId) {
    for _ in 0..80 {
        let body = super::bounds_of(cx, s.win, ids::HUD_MAKER_PANEL_BODY).unwrap();
        let b = super::bounds_of(cx, s.win, id.clone()).unwrap();
        if b.top() >= body.top() && b.bottom() <= body.bottom() {
            return;
        }
        let step = if b.top() < body.top() { 1. } else { -1. };
        let turn = ScrollDelta::Lines(point(0., step));
        cx.update_window(s.win, |_, w, cx| {
            w.scroll(ids::HUD_MAKER_PANEL_BODY, turn, cx)
        })
        .unwrap();
    }
    panic!("{id:?} never scrolled into the side panel");
}
