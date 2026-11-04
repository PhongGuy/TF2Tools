mod support;

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Context, IntoElement, MouseDownEvent, Render,
    Styled as _, TestAppContext, Window, WindowBounds, WindowOptions, canvas, point, px, size,
};
use tf2tools_app::font_check::{FontPrompt, prompt_job};
use tf2tools_app::ids;
use tf2tools_app::shell::font_check::{FontAnswer, font_check_dialog};
use tf2tools_core::{FontHost, Tf2Install};

use support::fonts::{FONTS, FakeFontHost, write_font_hud};

/// The prompt for `fonthud` shipping `fonts`, with `installed` already on the system.
fn prompt(env: &support::Env, fonts: &[(&str, &str)], installed: &[&str]) -> FontPrompt {
    write_font_hud(&env.game.join("tf/custom/fonthud"), "fonthud", fonts);
    let host: Arc<dyn FontHost> = Arc::new(FakeFontHost::new(
        installed,
        &env.tmp.path().join("user-fonts"),
    ));
    let services = support::services(env, support::no_steam()).with_font_host(Some(host));
    let tf2 = Tf2Install::from_path(&env.game).unwrap();
    prompt_job(&services, &tf2, Some("fonthud")).expect("a prompt")
}

fn open(
    cx: &mut TestAppContext,
    prompt: FontPrompt,
) -> (AnyWindowHandle, async_channel::Receiver<FontAnswer>) {
    support::init(cx);
    let win = support::blank_window(cx);
    let rx = cx
        .update_window(win, |_, w, cx| font_check_dialog(&prompt, w, cx))
        .unwrap();
    support::settle(cx, win);
    (win, rx)
}

#[gpui_kit::test]
fn the_dialog_lists_missing_and_protected_fonts(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, _rx) = open(cx, prompt(&env, FONTS, &[]));
    assert!(support::present(cx, win, ids::HUD_FONTS_DIALOG));
    assert_eq!(
        support::label_of(cx, win, ids::HUD_FONTS_DESCRIPTION).as_deref(),
        Some(
            "fonthud uses fonts that are not installed, so TF2 would show its default fonts \
             instead. Restart TF2 after installing them."
        )
    );
    assert_eq!(
        support::label_of(cx, win, ids::hud_fonts_font(0)).as_deref(),
        Some(
            format!(
                "Surface: {}",
                std::path::Path::new("resource")
                    .join("fonts")
                    .join("surface.ttf")
                    .display()
            )
            .as_str()
        )
    );
    assert_eq!(
        support::label_of(cx, win, ids::hud_fonts_font(1)).as_deref(),
        Some(
            format!(
                "Code Pro: {}",
                std::path::Path::new("resource")
                    .join("fonts")
                    .join("codepro.ttf")
                    .display()
            )
            .as_str()
        )
    );
    assert_eq!(
        support::label_of(cx, win, ids::hud_fonts_font(2)).as_deref(),
        Some("Marlett: not installed, system font name")
    );
    assert!(!support::present(cx, win, ids::hud_fonts_font(3)));
    assert!(!support::present(cx, win, ids::hud_fonts_issue(0)));
    assert!(support::present(cx, win, ids::HUD_FONTS_INSTALL));
    assert!(support::present(cx, win, ids::HUD_FONTS_SKIP));
    assert!(support::present(cx, win, ids::HUD_FONTS_DONT_ASK));
}

#[gpui_kit::test]
fn the_dialog_is_v5_s_512_px_wide(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, _rx) = open(cx, prompt(&env, FONTS, &[]));
    assert_eq!(support::dialog_box(cx, win).size.width, px(512.));
}

#[gpui_kit::test]
fn the_title_and_description_sit_together_like_v5s_header(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, _rx) = open(cx, prompt(&env, FONTS, &[]));
    let title = support::bounds_of(cx, win, ids::HUD_FONTS_TITLE).unwrap();
    let description = support::bounds_of(cx, win, ids::HUD_FONTS_DESCRIPTION).unwrap();
    let list = support::bounds_of(cx, win, ids::HUD_FONTS_LIST).unwrap();
    assert_eq!(
        description.top() - title.bottom(),
        px(8.),
        "v5's gap-2 header"
    );
    assert_eq!(
        list.top() - description.bottom(),
        px(16.),
        "v5's gap-4 content"
    );
}

fn answers(cx: &mut TestAppContext, id: &'static str, answer: FontAnswer) {
    let env = support::env();
    let (win, rx) = open(cx, prompt(&env, FONTS, &[]));
    support::click_id(cx, win, id);
    assert_eq!(rx.try_recv(), Ok(answer));
    assert!(!support::present(cx, win, ids::HUD_FONTS_DIALOG));
}

#[gpui_kit::test]
fn install_answers_install_and_closes(cx: &mut TestAppContext) {
    answers(cx, ids::HUD_FONTS_INSTALL, FontAnswer::Install);
}

#[gpui_kit::test]
fn skip_answers_skip_and_closes(cx: &mut TestAppContext) {
    answers(cx, ids::HUD_FONTS_SKIP, FontAnswer::Skip);
}

#[gpui_kit::test]
fn dont_ask_answers_dont_ask_and_closes(cx: &mut TestAppContext) {
    answers(cx, ids::HUD_FONTS_DONT_ASK, FontAnswer::DontAsk);
}

#[gpui_kit::test]
fn escape_skips(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, rx) = open(cx, prompt(&env, FONTS, &[]));
    support::press(cx, win, "escape");
    assert_eq!(rx.try_recv(), Ok(FontAnswer::Skip));
    assert!(!support::present(cx, win, ids::HUD_FONTS_DIALOG));
}

#[cfg(target_os = "linux")]
#[gpui_kit::test]
fn an_issue_only_hud_lists_the_issue_without_install(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, _rx) = open(
        cx,
        prompt(
            &env,
            &[("Surface", "resource/fonts/Surface.ttf")],
            &["Surface"],
        ),
    );
    assert_eq!(
        support::label_of(cx, win, ids::hud_fonts_issue(0)).as_deref(),
        Some(
            "resource/fonts/Surface.ttf has capital letters in its file name, which TF2 on \
             Linux may fail to load."
        )
    );
    assert!(!support::present(cx, win, ids::hud_fonts_font(0)));
    assert!(!support::present(cx, win, ids::HUD_FONTS_INSTALL));
    assert!(support::present(cx, win, ids::HUD_FONTS_SKIP));
}

#[gpui_kit::test]
fn the_font_list_is_addressable_for_scrolling(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, _rx) = open(cx, prompt(&env, FONTS, &[]));
    assert!(support::present(cx, win, ids::HUD_FONTS_LIST));
}

/// `n` fonts as (family, HUD-relative file), none of them protected.
fn many_fonts(n: usize) -> Vec<(String, String)> {
    (0..n)
        .map(|i| (format!("Font {i}"), format!("resource/fonts/font{i}.ttf")))
        .collect()
}

#[gpui_kit::test]
fn a_long_font_list_shows_a_scrollbar_that_takes_clicks(cx: &mut TestAppContext) {
    let env = support::env();
    let fonts = many_fonts(12);
    let fonts: Vec<(&str, &str)> = fonts
        .iter()
        .map(|(n, p)| (n.as_str(), p.as_str()))
        .collect();
    let prompt = prompt(&env, &fonts, &[]);
    support::init(cx);
    // The app's minimum window, where the buttons must stay in view.
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(px(0.), px(0.)),
            size: size(px(800.), px(600.)),
        })),
        ..WindowOptions::default()
    };
    let win = cx
        .update(|cx| gpui_kit::open_window(options, cx, |_, cx| cx.new(|_| support::Blank)))
        .unwrap()
        .0;
    let _rx = cx
        .update_window(win, |_, w, cx| font_check_dialog(&prompt, w, cx))
        .unwrap();
    assert_eq!(support::viewport(cx, win), size(px(800.), px(600.)));
    let list = support::bounds_of(cx, win, ids::HUD_FONTS_LIST).unwrap();
    assert_eq!(list.size.height, px(256.), "the list keeps its cap");
    let install = support::bounds_of(cx, win, ids::HUD_FONTS_INSTALL).unwrap();
    assert!(
        install.bottom() <= px(600.),
        "{install:?} falls below the window"
    );
    let last = support::bounds_of(cx, win, ids::hud_fonts_font(11)).unwrap();
    assert!(
        last.bottom() > list.bottom(),
        "the list overflows: {last:?} in {list:?}"
    );
    // The bar's track runs down the list's right edge; a click at its foot jumps to the end.
    cx.update_window(win, |_, w, cx| {
        w.click_at(
            ids::HUD_FONTS_LIST,
            point(list.size.width - px(6.), list.size.height - px(6.)),
            cx,
        )
    })
    .unwrap();
    let moved = support::bounds_of(cx, win, ids::hud_fonts_font(11)).unwrap();
    assert!(
        moved.bottom() <= list.bottom(),
        "no scrollbar took the click: {last:?} -> {moved:?}"
    );
}

/// A root view that counts the mouse-downs nothing above it stopped; a visible scrollbar
/// stops a press on its track, so the count tells whether the bar was drawn there.
struct PressProbe(Rc<Cell<usize>>);

impl Render for PressProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let seen = self.0.clone();
        canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                window.on_mouse_event(move |_: &MouseDownEvent, phase, _, _| {
                    if phase.bubble() {
                        seen.set(seen.get() + 1);
                    }
                });
            },
        )
        .size_full()
    }
}

/// Whether a press at the foot of the scrollbar's track, over `n` fonts, reaches the root.
fn track_press_passes_through(cx: &mut TestAppContext, n: usize) -> bool {
    let env = support::env();
    let fonts = many_fonts(n);
    let fonts: Vec<(&str, &str)> = fonts
        .iter()
        .map(|(n, p)| (n.as_str(), p.as_str()))
        .collect();
    let prompt = prompt(&env, &fonts, &[]);
    support::init(cx);
    let seen = Rc::new(Cell::new(0));
    let probe = seen.clone();
    let win = cx
        .update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
                cx.new(|_| PressProbe(probe))
            })
        })
        .unwrap()
        .0;
    let _rx = cx
        .update_window(win, |_, w, cx| font_check_dialog(&prompt, w, cx))
        .unwrap();
    support::settle(cx, win);
    let list = support::bounds_of(cx, win, ids::HUD_FONTS_LIST).unwrap();
    cx.update_window(win, |_, w, cx| {
        w.click_at(
            ids::HUD_FONTS_LIST,
            point(list.size.width - px(6.), list.size.height - px(6.)),
            cx,
        )
    })
    .unwrap();
    seen.get() > 0
}

#[gpui_kit::test]
fn a_short_font_list_draws_no_scrollbar(cx: &mut TestAppContext) {
    assert!(
        track_press_passes_through(cx, 2),
        "a list that fits draws no bar"
    );
}

#[gpui_kit::test]
fn the_press_probe_sees_a_drawn_scrollbar(cx: &mut TestAppContext) {
    assert!(
        !track_press_passes_through(cx, 12),
        "the overflowing list's bar stops the press"
    );
}
