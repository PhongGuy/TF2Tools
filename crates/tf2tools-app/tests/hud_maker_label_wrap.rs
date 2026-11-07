//! Wrapped labels on the canvas: the spectator tip, broken and placed as the game does it.

use crate::support;

use gpui_kit::{Bounds, Entity, Pixels, TestAppContext, px};
use support::maker_page::{click_in_panel, on_page};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_app::services::Tf2State;
use tf2tools_core::{PartKey, Scene, Tf2Install};

const TIP: &str = "Spectator/TipLabel";
const STOCK_TIP: &str = "TipLabel { ControlName CExLabel xpos 15 ypos r67 wide 145 tall 64 visible 1 labelText %tip% \
     textAlignment center wrap 1 }";
/// Font 24 px at 1080 lines, so 14.4 px a glyph in the headless text system, in a 326 px box.
const LINES: [&str; 3] = ["Tip: Stick with the ", "Medic to find the ", "next fight."];

const SHADOW_SCHEME: &str = r#""Scheme" { "Colors" { "TanLight" "235 226 202 255" }
  "Fonts" { "Default" { "1" { "name" "Verdana" "tall" "12" } }
            "HudFontSmall" { "1" { "name" "Verdana" "tall" "11" "dropshadow" "1" } } } }"#;

fn key() -> PartKey {
    PartKey::new(TIP)
}

/// The stock spectator scene with the tip label's keys after its size set to `keys`.
fn tip_with(
    cx: &mut TestAppContext,
    keys: &str,
    scheme: Option<&str>,
) -> (support::Env, support::ShellWindow, Entity<HudMakerPage>) {
    let line = format!(
        "TipLabel {{ ControlName CExLabel xpos 15 ypos r67 wide 145 tall 64 visible 1 \
         font HudFontSmall {keys} }}"
    );
    let stock = support::hud_stock::SPECTATOR;
    let text = stock.replace(STOCK_TIP, &line);
    assert_ne!(
        text, stock,
        "the stock spectator file should hold the tip label to replace"
    );
    let env = support::env();
    let mut files = vec![("resource/ui/spectator.res", Some(text.as_str()))];
    if scheme.is_some() {
        files.push(("resource/clientscheme.res", scheme));
    }
    support::seed_stock_game_with(&env.game, &files);
    let state = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    let s = support::shell_window(cx, &env, state);
    support::click_nav(cx, s.win, tf2tools_app::shell::route::Page::HudMaker);
    let page = cx.update(|cx| s.shell.read(cx).hud_maker_page()).unwrap();
    support::wait_real(cx, "the HUD to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    support::click_scene(cx, s.win, Scene::Spectator);
    (env, s, page)
}

fn tip(
    cx: &mut TestAppContext,
    keys: &str,
) -> (support::Env, support::ShellWindow, Entity<HudMakerPage>) {
    tip_with(cx, keys, None)
}

/// The text of each drawn line of the tip, in order.
fn lines(cx: &mut TestAppContext, s: &support::ShellWindow) -> Vec<String> {
    (0..)
        .map_while(|i| support::label_of(cx, s.win, ids::hud_maker_text_line(&key(), i)))
        .collect()
}

/// The window bounds of each drawn line of the tip, in order.
fn line_bounds(cx: &mut TestAppContext, s: &support::ShellWindow) -> Vec<Bounds<Pixels>> {
    (0..)
        .map_while(|i| support::bounds_of(cx, s.win, ids::hud_maker_text_line(&key(), i)))
        .collect()
}

fn part(cx: &mut TestAppContext, s: &support::ShellWindow) -> Bounds<Pixels> {
    support::bounds_of(cx, s.win, ids::hud_maker_part(&key())).unwrap()
}

fn close(a: Pixels, b: Pixels) -> bool {
    (a - b).abs() <= px(1.)
}

#[gpui_kit::test]
fn the_long_tip_wraps_into_lines_at_word_boundaries(cx: &mut TestAppContext) {
    let (_env, s, _page) = tip(cx, "labelText %tip% textAlignment north-west wrap 1");
    assert_eq!(lines(cx, &s), LINES);
    let drawn = line_bounds(cx, &s);
    let step = drawn[1].top() - drawn[0].top();
    assert!(step > px(0.) && close(drawn[2].top() - drawn[1].top(), step));
}

#[gpui_kit::test]
fn the_labels_accessible_text_stays_whole_when_wrapped(cx: &mut TestAppContext) {
    let (_env, s, _page) = tip(cx, "labelText %tip% wrap 1");
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_text(&key())).as_deref(),
        Some("Tip: Stick with the Medic to find the next fight.")
    );
}

#[gpui_kit::test]
fn toggling_wrap_changes_the_preview(cx: &mut TestAppContext) {
    let (_env, s, page) = tip(cx, "labelText %tip% textAlignment north-west wrap 1");
    assert_eq!(lines(cx, &s).len(), 3);
    on_page(cx, &s, &page, |p, _, _| {
        p.editor_mut().unwrap().select(Some(key()));
    });
    click_in_panel(cx, &s, ids::hud_maker_toggle("wrap"));
    assert!(lines(cx, &s).is_empty());
    let one = support::bounds_of(cx, s.win, ids::hud_maker_text(&key())).unwrap();
    click_in_panel(cx, &s, ids::hud_maker_toggle("wrap"));
    assert_eq!(lines(cx, &s).len(), 3);
    let three = support::bounds_of(cx, s.win, ids::hud_maker_text(&key())).unwrap();
    assert!(close(three.size.height, one.size.height * 3.));
}

#[gpui_kit::test]
fn centerwrap_centres_each_line_in_the_box(cx: &mut TestAppContext) {
    let (_env, s, _page) = tip(cx, "labelText %tip% textAlignment north-west centerwrap 1");
    let drawn = line_bounds(cx, &s);
    assert_eq!(drawn.len(), 3);
    assert!(drawn[2].size.width < drawn[0].size.width);
    let middle = part(cx, &s).center().x;
    for line in drawn {
        assert!(close(line.center().x, middle), "{line:?} {middle:?}");
    }
}

#[gpui_kit::test]
fn plain_wrap_starts_every_line_at_the_west_edge_even_when_aligned_east(cx: &mut TestAppContext) {
    let (_env, s, _page) = tip(cx, "labelText %tip% textAlignment east wrap 1");
    let drawn = line_bounds(cx, &s);
    let left = part(cx, &s).left();
    assert_eq!(drawn.len(), 3);
    for line in drawn {
        assert!(close(line.left(), left), "{line:?} {left:?}");
    }
}

#[gpui_kit::test]
fn the_x_inset_narrows_the_lines_and_moves_them_in(cx: &mut TestAppContext) {
    let (_env, s, _page) = tip(
        cx,
        "labelText %tip% textAlignment west wrap 1 textinsetx 80",
    );
    let drawn = line_bounds(cx, &s);
    assert!(drawn.len() > 3, "{drawn:?}");
    let left = part(cx, &s).left();
    assert!(drawn[0].left() - left > px(10.), "{drawn:?}");
    assert!(drawn.iter().all(|l| close(l.left(), drawn[0].left())));
}

#[gpui_kit::test]
fn the_x_inset_places_wrapped_lines_by_the_alignment(cx: &mut TestAppContext) {
    // The block is the box less the inset, so it starts at the box's edge when aligned east
    // and half the inset in when aligned to the middle.
    let keys = "labelText %tip% wrap 1 textinsetx 80 textAlignment";
    let (_env, s, page) = tip(cx, &format!("{keys} east"));
    let (drawn, b) = (line_bounds(cx, &s), part(cx, &s));
    let wide = cx.update(|cx| {
        let e = page.read(cx).editor().unwrap();
        e.shown_layout().get(&key()).unwrap().wide()
    });
    let inset = b.size.width * (80. / wide as f32);
    assert!(inset > px(4.), "{inset:?}");
    assert!(drawn.iter().all(|l| close(l.left(), b.left())), "{drawn:?}");

    let (_env, s, _page) = tip(cx, &format!("{keys} center"));
    let (drawn, b) = (line_bounds(cx, &s), part(cx, &s));
    let left = b.left() + inset / 2.;
    assert!(drawn.iter().all(|l| close(l.left(), left)), "{drawn:?}");
}

#[gpui_kit::test]
fn the_block_keeps_its_vertical_alignment(cx: &mut TestAppContext) {
    let (_env, s, _page) = tip(cx, "labelText %tip% textAlignment north wrap 1");
    let (drawn, b) = (line_bounds(cx, &s), part(cx, &s));
    assert!(close(drawn[0].top(), b.top()));

    let (_env, s, _page) = tip(cx, "labelText %tip% textAlignment south wrap 1");
    let (drawn, b) = (line_bounds(cx, &s), part(cx, &s));
    assert!(close(drawn[2].bottom(), b.bottom()));

    let (_env, s, _page) = tip(cx, "labelText %tip% textAlignment center wrap 1");
    let (drawn, b) = (line_bounds(cx, &s), part(cx, &s));
    assert!(close(
        drawn[0].top() - b.top(),
        b.bottom() - drawn[2].bottom()
    ));
}

#[gpui_kit::test]
fn an_explicit_newline_breaks_a_wrapped_label(cx: &mut TestAppContext) {
    let (_env, s, _page) = tip(cx, r#"labelText "one\ntwo" wrap 1"#);
    assert_eq!(lines(cx, &s), ["one", "two"]);
}

#[gpui_kit::test]
fn a_shadow_copy_draws_the_same_lines(cx: &mut TestAppContext) {
    let keys = "labelText %tip% textAlignment north-west wrap 1";
    let (_env, s, _page) = tip_with(cx, keys, Some(SHADOW_SCHEME));
    let text = support::bounds_of(cx, s.win, ids::hud_maker_text(&key())).unwrap();
    let shadow = support::bounds_of(cx, s.win, ids::hud_maker_text_shadow(&key())).unwrap();
    assert!(line_bounds(cx, &s).len() > 1);
    assert_eq!(shadow.size, text.size);
}

#[gpui_kit::test]
fn an_unwrapped_label_stays_on_one_line(cx: &mut TestAppContext) {
    let (_env, s, _page) = tip(cx, "labelText %tip% textAlignment north-west");
    assert!(lines(cx, &s).is_empty());
    let text = support::bounds_of(cx, s.win, ids::hud_maker_text(&key())).unwrap();
    let (_env, s, _page) = tip(cx, "labelText OneLine textAlignment north-west");
    let short = support::bounds_of(cx, s.win, ids::hud_maker_text(&key())).unwrap();
    assert_eq!(text.size.height, short.size.height);
}
