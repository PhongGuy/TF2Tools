//! Toasts sit above every dialog. GPUI hands the pointer to whatever painted last, so each test
//! here reads paint order by sending real pointer events at the toast's own pixels.

use std::time::Duration;

use gpui_kit::component::WindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, ElementId, InputEvent as _, Modifiers, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement as _, Pixels, Point, Styled as _,
    TestAppContext, WindowBounds, WindowOptions, div, point, px, size,
};
use tf2tools_app::ids;
use tf2tools_app::shell::toast::{ToastKind, toast};
use tf2tools_app::widgets::dialog_frame::{DETAIL_WIDTH, DialogFrame};

use crate::support;

/// The two window sizes the bug was seen at.
const SIZES: [(f32, f32); 2] = [(1280., 800.), (900., 600.)];

// A window of `size` showing a dialog tall enough to reach the toast corner, as the HUD Store's
// detail dialog does, and a toast over it.
fn dialog_and_toast(cx: &mut TestAppContext, (width, height): (f32, f32)) -> AnyWindowHandle {
    support::init(cx);
    let (win, _) = cx
        .update(|cx| {
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(width), px(height)),
                })),
                ..WindowOptions::default()
            };
            gpui_kit::open_window(options, cx, |_, cx| cx.new(|_| support::Blank))
        })
        .unwrap();
    open_dialog(cx, win);
    cx.update_window(win, |_, w, cx| {
        toast(w, cx, ToastKind::Error, "Failed to download", "budhud")
    })
    .unwrap();
    support::settle(cx, win);
    assert!(
        support::present(cx, win, ids::TOAST),
        "the error toast should show over the open dialog"
    );
    win
}

fn open_dialog(cx: &mut TestAppContext, win: AnyWindowHandle) {
    cx.update_window(win, |_, w, cx| {
        let frame = DialogFrame::new(DETAIL_WIDTH);
        w.open_dialog(cx, move |dialog, w, cx| {
            frame
                .apply(dialog, &*w, &*cx)
                .child(frame.measure(div().h(px(560.))))
        });
    })
    .unwrap();
    support::settle(cx, win);
}

fn dialogs_open(cx: &mut TestAppContext, win: AnyWindowHandle) -> bool {
    cx.update_window(win, |_, w, cx| w.has_active_dialog(cx))
        .unwrap()
}

fn toast_bounds(cx: &mut TestAppContext, win: AnyWindowHandle) -> Bounds<Pixels> {
    support::bounds_of(cx, win, ids::TOAST).expect("the toast")
}

// One press and release of `button` at `at`, after the pointer moves there.
fn press_at(cx: &mut TestAppContext, win: AnyWindowHandle, at: Point<Pixels>, button: MouseButton) {
    let modifiers = Modifiers::default();
    hover_at(cx, win, at);
    cx.update_window(win, |_, w, cx| {
        let down = MouseDownEvent {
            button,
            position: at,
            modifiers,
            click_count: 1,
            first_mouse: false,
        };
        let up = MouseUpEvent {
            button,
            position: at,
            modifiers,
            click_count: 1,
        };
        let _ = w.dispatch_event(down.to_platform_input(), cx);
        let _ = w.dispatch_event(up.to_platform_input(), cx);
    })
    .unwrap();
    support::settle(cx, win);
}

fn hover_at(cx: &mut TestAppContext, win: AnyWindowHandle, at: Point<Pixels>) {
    cx.update_window(win, |_, w, cx| {
        let moved = MouseMoveEvent {
            position: at,
            ..MouseMoveEvent::default()
        };
        let _ = w.dispatch_event(moved.to_platform_input(), cx);
    })
    .unwrap();
    support::settle(cx, win);
}

// Whether the toast's close animation ends inside two seconds, far short of the 5 s autohide, so
// only a dismissal the toast really received can pass.
fn toast_leaves(cx: &mut TestAppContext, win: AnyWindowHandle) -> bool {
    for _ in 0..40 {
        cx.executor().advance_clock(Duration::from_millis(50));
        if !support::present(cx, win, ids::TOAST) {
            return true;
        }
    }
    false
}

#[gpui_kit::test]
fn a_toast_keeps_its_whole_box_inside_a_small_window(cx: &mut TestAppContext) {
    let win = dialog_and_toast(cx, SIZES[1]);
    let view = cx.update_window(win, |_, w, _| w.viewport_size()).unwrap();
    let t = toast_bounds(cx, win);
    assert!(
        t.left() >= px(0.)
            && t.top() >= px(0.)
            && t.right() <= view.width
            && t.bottom() <= view.height,
        "{t:?} left {view:?}"
    );
}

#[gpui_kit::test]
fn a_toast_at_1280x800_takes_the_pointer_from_a_dialog(cx: &mut TestAppContext) {
    pointer_reaches_the_toast(cx, SIZES[0]);
}

#[gpui_kit::test]
fn a_toast_at_900x600_takes_the_pointer_from_a_dialog(cx: &mut TestAppContext) {
    pointer_reaches_the_toast(cx, SIZES[1]);
}

// A middle click dismisses a toast and does nothing to a dialog, so the toast going away proves
// the toast topped the hit test at its own centre.
fn pointer_reaches_the_toast(cx: &mut TestAppContext, window: (f32, f32)) {
    let win = dialog_and_toast(cx, window);
    let centre = toast_bounds(cx, win).center();
    press_at(cx, win, centre, MouseButton::Middle);
    assert!(
        toast_leaves(cx, win),
        "the dialog took the press at {centre:?}"
    );
    assert!(
        dialogs_open(cx, win),
        "the dialog should stay open after the press passes through"
    );
}

#[gpui_kit::test]
fn a_toast_stays_on_top_of_a_second_dialog(cx: &mut TestAppContext) {
    let win = dialog_and_toast(cx, SIZES[1]);
    open_dialog(cx, win);
    let centre = toast_bounds(cx, win).center();
    press_at(cx, win, centre, MouseButton::Middle);
    assert!(toast_leaves(cx, win), "the top dialog took the press");
}

#[gpui_kit::test]
fn a_left_click_on_a_toast_leaves_the_dialog_open(cx: &mut TestAppContext) {
    for window in SIZES {
        let win = dialog_and_toast(cx, window);
        let centre = toast_bounds(cx, win).center();
        press_at(cx, win, centre, MouseButton::Left);
        assert!(dialogs_open(cx, win), "the click closed the dialog");
    }
}

#[gpui_kit::test]
fn escape_closes_the_dialog_after_a_click_on_a_toast(cx: &mut TestAppContext) {
    let win = dialog_and_toast(cx, SIZES[0]);
    let centre = toast_bounds(cx, win).center();
    press_at(cx, win, centre, MouseButton::Left);
    assert!(dialogs_open(cx, win), "the click closed the dialog");
    support::press(cx, win, "escape");
    assert!(
        !dialogs_open(cx, win),
        "escape no longer reaches the dialog"
    );
    assert!(support::present(cx, win, ids::TOAST), "the toast went too");
}

#[gpui_kit::test]
fn the_toasts_close_button_dismisses_it_and_escape_still_works(cx: &mut TestAppContext) {
    let win = dialog_and_toast(cx, SIZES[1]);
    let t = toast_bounds(cx, win);
    hover_at(cx, win, t.center());
    let close = close_button(cx, win);
    press_at(cx, win, close, MouseButton::Left);
    assert!(
        toast_leaves(cx, win),
        "the close button never got the click"
    );
    assert!(dialogs_open(cx, win));
    support::press(cx, win, "escape");
    assert!(
        !dialogs_open(cx, win),
        "escape no longer reaches the dialog"
    );
}

// The centre of the close button inside the kit's toast, which only shows on hover.
fn close_button(cx: &mut TestAppContext, win: AnyWindowHandle) -> Point<Pixels> {
    let (close, notification) = (ElementId::from("close"), ElementId::from("notification"));
    cx.update_window(win, |_, w, _| {
        let all = gpui_kit::base::test_support::snapshots(w);
        let hit = all
            .iter()
            .find(|s| s.path().last() == Some(&close) && s.path().contains(&notification));
        hit.map(|s| s.bounds().center()).unwrap_or_else(|| {
            let paths: Vec<_> = all.iter().map(|s| s.path().to_vec()).collect();
            panic!("no close button in {paths:?}")
        })
    })
    .unwrap()
}
