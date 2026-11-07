//! In a copy, parts drawn from a #base stub edit like any other; the missing-base note stays.

use crate::support;

use std::path::Path;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_app::services::{Services, Tf2State};
use tf2tools_app::shell::route::Page;
use tf2tools_core::{HudEntry, PartKey, Provenance, Scene, Tf2Install};

const KILLER: &str = "FreezePanel/FreezePanelBase/FreezeLabelKiller";

fn stock_shell(cx: &mut TestAppContext, env: &support::Env) -> support::ShellWindow {
    support::seed_stock_game(&env.game);
    support::shell_window(
        cx,
        env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    )
}

/// A copy of a HUD whose freeze panel file is the stub `stub` over the stock panel.
fn stub_copy(services: &Services, dir: &Path, stub: &str) -> HudEntry {
    let extra = [
        (
            "resource/ui/freezepanel_basic_variant.res",
            support::hud_stock::FREEZE_PANEL,
        ),
        ("resource/ui/freezepanel_basic.res", stub),
    ];
    let base =
        support::hud_copies::import_stock_hud(services, dir, "stubhud", &extra, Provenance::Local);
    services.library.copy_hud(base.id()).unwrap()
}

fn edit_from_huds(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    hud: &HudEntry,
) -> Entity<HudMakerPage> {
    support::click_id(cx, s.win, ids::nav(Page::Sounds));
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    support::click_id(cx, s.win, ids::hud_edit(hud.folder_name()));
    let page = cx.update(|cx| s.shell.read(cx).hud_maker_page()).unwrap();
    support::wait_real(cx, "the HUD to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::click_scene(cx, s.win, Scene::FreezeCam);
    page
}

fn select_killer(cx: &mut TestAppContext, s: &support::ShellWindow, page: &Entity<HudMakerPage>) {
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            p.editor_mut().unwrap().select(Some(PartKey::new(KILLER)));
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
fn a_stub_part_in_a_copy_is_editable(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let copy = stub_copy(
        &s.services,
        env.tmp.path(),
        "#base \"freezepanel_basic_variant.res\"\n",
    );
    let page = edit_from_huds(cx, &s, &copy);
    focus_canvas(cx, &s);
    select_killer(cx, &s, &page);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_PANEL_FROM_BASE));
    assert!(!support::present(
        cx,
        s.win,
        ids::HUD_MAKER_PANEL_BASE_MISSING
    ));
    assert!(support::present(cx, s.win, ids::HUD_MAKER_LAYER));
    support::press(cx, s.win, "right");
    assert!(cx.update(|cx| page.read(cx).editor().unwrap().can_undo()));
}

#[gpui_kit::test]
fn a_copy_stub_with_a_missing_base_keeps_its_note_and_stays_editable(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let stub = "#base \"freezepanel_basic_variant.res\"\n#base \"gone.res\"\n";
    let copy = stub_copy(&s.services, env.tmp.path(), stub);
    let page = edit_from_huds(cx, &s, &copy);
    select_killer(cx, &s, &page);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_PANEL_BASE_MISSING).as_deref(),
        Some("Some files this part builds on weren't found, so it may look different in the game.")
    );
    assert!(support::present(cx, s.win, ids::HUD_MAKER_LAYER));
}
