//! The HUD Maker's chat: sample lines over the HUD and "Chat at the top", headless.

mod support;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Bounds, Entity, Pixels, ScrollDelta, TestAppContext, point, px};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_app::services::Tf2State;
use tf2tools_core::{PartKey, Scene, ScreenScale, Tf2Install};

const HISTORY: &str = "HudChat/HudChatHistory";

/// A stock-shaped basechat.res with the chat moved clear of every stock part, no Valve bytes.
const CHAT: &str = r##""Resource/UI/BaseChat.res"
{
  HudChat { ControlName EditablePanel fieldName HudChat visible 1 enabled 1 xpos 150 ypos 100 wide 280 tall 120 PaintBackgroundType 2 }
  ChatInputLine { ControlName EditablePanel fieldName ChatInputLine visible 1 enabled 1 xpos 10 ypos 395 wide 260 tall 2 }
  ChatFiltersButton { ControlName Button fieldName ChatFiltersButton xpos 225 ypos 2 wide 45 tall 15 visible 1 labelText "#chat_filterbutton" }
  HudChatHistory { ControlName RichText fieldName HudChatHistory xpos 10 ypos 17 wide 260 tall 75 visible 1 labelText "" font ChatFont }
}
"##;

const CHAT_SCHEME: &str = r#""Scheme" { Colors { } BaseSettings { "TFColors.ChatTextYellow" "251 235 202 255" }
  Fonts { "ChatFont" { "1" { "name" "Verdana" "tall" "12" "weight" "700" } } } }"#;

/// The maker opened on the stock game with `chat` as its basechat.res.
fn opened(
    cx: &mut TestAppContext,
    chat: &str,
) -> (support::Env, support::ShellWindow, Entity<HudMakerPage>) {
    let env = support::env();
    support::seed_stock_game_with(
        &env.game,
        &[
            ("resource/ui/basechat.res", Some(chat)),
            ("resource/chatscheme.res", Some(CHAT_SCHEME)),
        ],
    );
    let state = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    let s = support::shell_window(cx, &env, state);
    support::click_nav(cx, s.win, tf2tools_app::shell::route::Page::HudMaker);
    let page = cx.update(|cx| s.shell.read(cx).hud_maker_page()).unwrap();
    support::wait_real(cx, "the HUD to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    (env, s, page)
}

fn bounds(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    id: impl Into<gpui_kit::ElementId>,
) -> Bounds<Pixels> {
    let id = id.into();
    support::bounds_of(cx, s.win, id.clone()).unwrap_or_else(|| panic!("no {id:?}"))
}

fn f(p: Pixels) -> f32 {
    f32::from(p)
}

#[gpui_kit::test]
fn the_hud_scene_draws_three_chat_lines_in_the_history(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx, CHAT);
    let want = [
        "Rocket Jumper :  Nice shot!",
        "Kritz :  Push the cart!",
        "You :  Thanks, Medic!",
    ];
    for (i, text) in want.iter().enumerate() {
        let label = support::label_of(cx, s.win, ids::hud_maker_chat_line(i));
        assert_eq!(label.as_deref(), Some(*text), "line {i}");
    }
    assert!(!support::present(cx, s.win, ids::hud_maker_chat_line(3)));
    // Each line sits at core's offsets inside the history, at the canvas's scale.
    let canvas = bounds(cx, &s, ids::HUD_MAKER_CANVAS);
    let (k, x, ys) = cx.update(|cx| {
        let e = page.read(cx).editor().unwrap();
        let screen = e.screen();
        let sample = e.model().chat_sample(screen).unwrap();
        let ys: Vec<i32> = sample.lines().iter().map(|l| l.y()).collect();
        (f(canvas.size.width) / screen.wide() as f32, sample.x(), ys)
    });
    let history = bounds(cx, &s, ids::hud_maker_part(&PartKey::new(HISTORY)));
    for (i, y) in ys.iter().enumerate() {
        let line = bounds(cx, &s, ids::hud_maker_chat_line(i));
        let left = f(line.left()) - f(history.left());
        let top = f(line.top()) - f(history.top());
        assert!((left - x as f32 * k).abs() < 1.0, "line {i} left {left}");
        assert!((top - *y as f32 * k).abs() < 1.0, "line {i} top {top}");
        assert!(line.bottom() <= history.bottom(), "line {i}");
    }
}

#[gpui_kit::test]
fn a_click_on_a_chat_line_selects_the_history(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx, CHAT);
    // The HUD options panel floats over the line's left part, so press its right end.
    let line = bounds(cx, &s, ids::hud_maker_chat_line(0));
    let end = point(line.size.width - px(2.), line.size.height / 2.);
    cx.update_window(s.win, |_, w, cx| {
        w.click_at(ids::hud_maker_chat_line(0), end, cx)
    })
    .unwrap();
    support::settle(cx, s.win);
    let selected = cx.update(|cx| page.read(cx).editor().and_then(|e| e.selected().cloned()));
    assert_eq!(selected, Some(PartKey::new(HISTORY)));
}

#[gpui_kit::test]
fn another_tab_draws_no_chat(cx: &mut TestAppContext) {
    let (_env, s, _page) = opened(cx, CHAT);
    assert!(support::present(cx, s.win, ids::hud_maker_chat_line(0)));
    support::click_scene(cx, s.win, Scene::Scoreboard);
    assert!(!support::present(cx, s.win, ids::hud_maker_chat_line(0)));
}

#[gpui_kit::test]
fn a_hidden_history_draws_no_chat(cx: &mut TestAppContext) {
    let quiet = CHAT.replace("tall 75 visible 1", "tall 75 visible 0");
    let (_env, s, _page) = opened(cx, &quiet);
    assert!(!support::present(cx, s.win, ids::hud_maker_chat_line(0)));
}

#[gpui_kit::test]
fn the_open_parts_list_keeps_off_the_chat_lines(cx: &mut TestAppContext) {
    // The open list keeps the spot it settled in while nothing it must avoid comes under it,
    // so drag the chat until its first line ends in the list's middle, then let go of the
    // selection, which the list would fold over instead.
    let (_env, s, page) = opened(cx, CHAT);
    for (w, h) in [(1280., 800.), (1920., 1080.)] {
        cx.simulate_window_resize(s.win, gpui_kit::size(gpui_kit::px(w), gpui_kit::px(h)));
        support::settle(cx, s.win);
        if !support::present(cx, s.win, ids::HUD_MAKER_PARTS) {
            support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_COLLAPSE);
        }
        support::settle(cx, s.win);
        let list = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL);
        let line = bounds(cx, &s, ids::hud_maker_chat_line(0));
        // Press the chat above its history at the right, clear of the options panel over the
        // chat's left part, and move it by what takes the line's right end to the list's middle.
        let chat = bounds(cx, &s, ids::hud_maker_part(&PartKey::new("HudChat")));
        let from = point(chat.right() - px(4.), chat.top() + px(3.));
        let to = from + (list.center() - point(line.right() - px(2.), line.center().y));
        cx.update_window(s.win, |_, w, cx| w.drag(from, to, cx))
            .unwrap();
        support::settle(cx, s.win);
        support::press(cx, s.win, "escape");
        support::settle(cx, s.win);
        let selected = cx.update(|cx| page.read(cx).editor().and_then(|e| e.selected().cloned()));
        assert_eq!(selected, None, "{w}x{h}");
        let parts = bounds(cx, &s, ids::HUD_MAKER_PARTS_PANEL);
        for i in 0..3 {
            let line = bounds(cx, &s, ids::hud_maker_chat_line(i));
            assert!(
                !parts.intersects(&line),
                "{w}x{h}: {parts:?} over chat line {i} {line:?}"
            );
        }
    }
}

/// Clicks `id` in the side panel, scrolling the panel down until it is wholly in view.
fn click_in_panel(cx: &mut TestAppContext, s: &support::ShellWindow, id: gpui_kit::ElementId) {
    for _ in 0..40 {
        let seen = support::present(cx, s.win, id.clone())
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
    support::click_id(cx, s.win, id);
}

#[gpui_kit::test]
fn chat_at_the_top_moves_the_chat_lines_up(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx, CHAT);
    // The option no longer says the preview leaves it out.
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_option_note("chat_top")
    ));
    let canvas = bounds(cx, &s, ids::HUD_MAKER_CANVAS);
    let before = f(bounds(cx, &s, ids::hud_maker_chat_line(0)).top());
    click_in_panel(cx, &s, ids::hud_maker_option("chat_top"));
    let after = f(bounds(cx, &s, ids::hud_maker_chat_line(0)).top());
    // The chat's ypos goes from 100 units to 0, so every line rises by 100 units.
    let screen = cx.update(|cx| page.read(cx).editor().unwrap().screen());
    let k = f(canvas.size.width) / screen.wide() as f32;
    let shift = ScreenScale::new(screen.tall()).scale(100) as f32 * k;
    assert!(
        (before - after - shift).abs() < 1.0,
        "rose {} px, want {shift}",
        before - after
    );
}

#[gpui_kit::test]
fn chat_at_the_top_keeps_the_options_panel_off_the_chat_lines(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx, CHAT);
    click_in_panel(cx, &s, ids::hud_maker_option("chat_top"));
    support::settle(cx, s.win);
    let panel = bounds(cx, &s, ids::HUD_MAKER_PANEL);
    for i in 0..3 {
        let line = bounds(cx, &s, ids::hud_maker_chat_line(i));
        assert!(
            !panel.intersects(&line),
            "{panel:?} over chat line {i} {line:?}"
        );
    }
    // The side panel and the part list come to rest: settling again moves neither.
    let at_rest = |cx: &mut TestAppContext| {
        let parts = support::bounds_of(cx, s.win, ids::HUD_MAKER_PARTS_PANEL);
        let relayouts = cx.update(|cx| page.read(cx).relayouts());
        (bounds(cx, &s, ids::HUD_MAKER_PANEL), parts, relayouts)
    };
    let first = at_rest(cx);
    support::settle(cx, s.win);
    support::settle(cx, s.win);
    assert_eq!(at_rest(cx), first);
}

#[gpui_kit::test]
fn the_chat_lines_count_only_what_the_history_shows(cx: &mut TestAppContext) {
    // A history narrower than its lines clips them; the band the panels avoid ends with it.
    let narrow = CHAT.replace("wide 260 tall 75", "wide 60 tall 75");
    let (_env, s, page) = opened(cx, &narrow);
    let history = bounds(cx, &s, ids::hud_maker_part(&PartKey::new(HISTORY)));
    let band = cx
        .update(|cx| page.read(cx).chat_band())
        .expect("drawn chat");
    assert!(
        band.right() <= history.right() + px(0.5),
        "band {band:?} past the history {history:?}"
    );
}

#[gpui_kit::test]
fn typing_in_chat_shows_the_say_line_at_the_bottom_of_the_chat(cx: &mut TestAppContext) {
    let (_env, s, _page) = opened(cx, CHAT);
    let line = PartKey::new("HudChat/ChatInputLine");
    assert!(!support::present(cx, s.win, ids::hud_maker_text(&line)));
    support::click_id(cx, s.win, ids::HUD_MAKER_SITUATION);
    support::click_id(
        cx,
        s.win,
        ids::hud_maker_situation_switch(tf2tools_core::SituationSwitch::Typing),
    );
    support::press(cx, s.win, "escape");
    support::settle(cx, s.win);
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_text(&line)).as_deref(),
        Some("Say :")
    );
    // The game moves the line from the file's off-screen spot into the chat's lower edge.
    let chat = bounds(cx, &s, ids::hud_maker_part(&PartKey::new("HudChat")));
    let drawn = bounds(cx, &s, ids::hud_maker_part(&line));
    assert!(chat.contains(&drawn.origin), "{drawn:?} outside {chat:?}");
    assert!(drawn.bottom() <= chat.bottom(), "{drawn:?} below {chat:?}");
}
