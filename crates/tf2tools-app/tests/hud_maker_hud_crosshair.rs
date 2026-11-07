//! The HUD crosshair in the HUD Maker, headless: its note, the two-crosshairs warning and the
//! preview drawing the HUD's own crosshair label.

use crate::support;

use gpui_kit::{Entity, TestAppContext};
use std::sync::Arc;
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_app::picker::ScriptedPicker;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{CrosshairRef, OptionValue, Tf2Install};

const SCHEME: &str = r#""Scheme"
{
  Colors { TanLight "235 226 202 255" Crosshair "255 255 255 255" }
  Fonts { Crosshairs { "1" { name "TF2 Crosshairs" tall 24 } } }
}"#;
const CROSSHAIR: &str = "  CustomCrosshair { ControlName CExLabel fieldName CustomCrosshair visible 0 enabled 0 zpos 2 xpos c-50 ypos c-50 wide 100 tall 100 font Crosshairs labelText \"+\" textAlignment center fgcolor Crosshair }\n";

/// Opens the maker on a HUD with a CustomCrosshair label, after `seed` ran on the services.
fn opened(
    cx: &mut TestAppContext,
    seed: impl FnOnce(&support::Env, &support::ShellWindow),
) -> (support::Env, support::ShellWindow, Entity<HudMakerPage>) {
    let env = support::env();
    let layout = support::hud_stock::hudlayout_with_killfeed(&format!(
        "{}{CROSSHAIR}",
        support::hud_stock::KILLFEED
    ));
    support::seed_stock_game_with(
        &env.game,
        &[
            ("resource/clientscheme.res", Some(SCHEME)),
            ("scripts/hudlayout.res", Some(layout.as_str())),
        ],
    );
    let state = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    let picker = Arc::new(ScriptedPicker::empty());
    let s = support::shell_window_with(cx, &env, state, picker, support::no_steam());
    seed(&env, &s);
    support::click_nav(cx, s.win, Page::HudMaker);
    let page = cx
        .update(|cx| s.shell.read(cx).hud_maker_page())
        .expect("a HUD Maker page");
    support::wait_real(cx, "the HUD to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    (env, s, page)
}

fn assign_weapon_crosshair(_: &support::Env, s: &support::ShellWindow) {
    let builtin = CrosshairRef::Builtin("cross".into());
    s.services
        .library
        .assign("tf_weapon_scattergun", &builtin)
        .unwrap();
}

fn turn_on(cx: &mut TestAppContext, s: &support::ShellWindow, page: &Entity<HudMakerPage>) {
    support::maker_page::click_in_panel(cx, s, ids::hud_maker_option("hud_crosshair"));
    let on = cx.update(|cx| (page.read(cx).editor()).and_then(|e| e.option_value("hud_crosshair")));
    assert_eq!(on, Some(OptionValue::Bool(true)));
}

#[gpui_kit::test]
fn the_hud_crosshair_says_it_is_apart_from_the_crosshairs_page(cx: &mut TestAppContext) {
    let (_env, s, _page) = opened(cx, |_, _| {});
    let note = support::label_of(cx, s.win, ids::HUD_MAKER_HUD_CROSSHAIR_NOTE).unwrap();
    assert!(note.contains("Crosshairs page"), "{note}");
    assert!(!support::present(
        cx,
        s.win,
        ids::HUD_MAKER_HUD_CROSSHAIR_WARNING
    ));
}

#[gpui_kit::test]
fn the_preview_draws_the_hud_crosshair_once_it_is_on(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx, |_, _| {});
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_HUD_CROSSHAIR));
    turn_on(cx, &s, &page);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_HUD_CROSSHAIR).as_deref(),
        Some("+")
    );
}

#[gpui_kit::test]
fn weapon_crosshairs_and_the_hud_crosshair_together_warn(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx, assign_weapon_crosshair);
    // Weapon crosshairs alone are no clash.
    assert!(!support::present(
        cx,
        s.win,
        ids::HUD_MAKER_HUD_CROSSHAIR_WARNING
    ));
    turn_on(cx, &s, &page);
    support::wait_real(cx, "the warning", |cx| {
        support::present(cx, s.win, ids::HUD_MAKER_HUD_CROSSHAIR_WARNING)
    });
    let warning = support::label_of(cx, s.win, ids::HUD_MAKER_HUD_CROSSHAIR_WARNING).unwrap();
    assert!(warning.contains("two crosshairs"), "{warning}");
}

#[gpui_kit::test]
fn the_hud_crosshair_alone_does_not_warn(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx, |_, _| {});
    turn_on(cx, &s, &page);
    assert!(!support::present(
        cx,
        s.win,
        ids::HUD_MAKER_HUD_CROSSHAIR_WARNING
    ));
}

#[gpui_kit::test]
fn the_symbol_size_and_colour_show_under_the_switch(cx: &mut TestAppContext) {
    let (_env, s, _page) = opened(cx, |_, _| {});
    for id in [
        "hud_crosshair_symbol",
        "hud_crosshair_size",
        "hud_crosshair_colour",
    ] {
        assert!(
            support::present(cx, s.win, ids::hud_maker_option(id)),
            "{id}"
        );
    }
}
