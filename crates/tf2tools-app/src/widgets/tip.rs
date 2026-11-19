//! Tooltips on GPUI's own tooltip, not the kit's window-wide one: it drops on any press or
//! scroll and whenever its control leaves the pointer, so it never stays behind when a panel
//! scrolls or redraws under it. Every tooltip box wraps at one width, so none runs off the window.

use gpui_kit::component::text::Text;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::{
    AnyView, App, AppContext as _, Context, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Point, Render, SharedString, StatefulInteractiveElement as _,
    Styled as _, TestSupportExt as _, Window, div, px,
};

// GPUI lays a tooltip out at its one-line width and only flips it left of the pointer, so a long
// sentence ran off the 800 px window; past this width the words wrap instead.
const MAX_WIDTH: Pixels = px(360.);

/// The kit's tooltip box for `text`, wrapping its words past `MAX_WIDTH`.
pub(crate) fn tip_box(text: impl Into<Text>) -> Tooltip {
    Tooltip::new(text).max_w(MAX_WIDTH)
}

// The tooltip's content, under `id` so tests and the pilot can read its words.
struct Tip {
    id: &'static str,
    text: SharedString,
    shift: Point<Pixels>,
}

impl Render for Tip {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The tooltip draws outside the app root, so it sets the UI font's fallbacks itself.
        let content = crate::theme::with_ui_font(div(), cx)
            .id(self.id)
            .aria_label(self.text.clone())
            .test_support()
            .child(tip_box(self.text.clone()).build(window, cx));
        if self.shift == Point::default() {
            return content.into_any_element();
        }
        // GPUI starts a tooltip at the pointer. A shifted one is an out-of-flow child instead,
        // so the shift moves where it paints without touching the size GPUI places it by.
        div()
            .relative()
            .child(content.absolute().left(self.shift.x).top(self.shift.y))
            .into_any_element()
    }
}

/// A builder for `.tooltip(...)` showing `text`, its content readable under `id`.
pub(crate) fn tip(
    id: &'static str,
    text: impl Into<SharedString>,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text = text.into();
    move |_, cx| tip_view(id, text.clone(), Point::default(), cx)
}

/// The view a tooltip builder returns: `text` under `id`, painted `shift` away from where GPUI
/// places it.
pub(crate) fn tip_view(
    id: &'static str,
    text: SharedString,
    shift: Point<Pixels>,
    cx: &mut App,
) -> AnyView {
    cx.new(|_| Tip { id, text, shift }).into()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{
        AppContext as _, Bounds, Context, InteractiveElement as _, IntoElement, ParentElement as _,
        Pixels, PlatformTextSystem as _, Render, SharedString, Styled as _, TestAppContext,
        TestDispatcher, TestSupportExt as _, Window, WindowBounds, WindowOptions, div, font, point,
        px, size,
    };
    use gpui_wgpu::CosmicTextSystem;

    use super::{MAX_WIDTH, tip_box};
    use crate::theme::UI_FONT_FALLBACKS;

    // Han only, no space, so every break must fall between two characters.
    const LONG: &str = "每次安装完成后请重新启动游戏以便新的界面和音效生效如果仍然看不到任何变化请先检查设置中的游戏文件夹是否正确然后关闭游戏再次安装并重新进入服务器查看效果";
    const SHORT: &str = "安装";
    const PARAGRAPH: Pixels = px(300.);
    // The kit's tooltip popup has `.m_3()` outside its max width: 12 px a side
    // (gpui-component 0.10.0 tooltip.rs, `Tooltip::render`).
    const TIP_MARGIN: Pixels = px(24.);

    struct Sample;

    impl Render for Sample {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let tip = |id: &'static str,
                       text: &'static str,
                       window: &mut Window,
                       cx: &mut Context<Self>| {
                div()
                    .id(id)
                    .test_support()
                    .child(tip_box(SharedString::from(text)).build(window, cx))
            };
            let para = |id: &'static str, text: &'static str| {
                div().id(id).test_support().w(PARAGRAPH).child(text)
            };
            div()
                .flex()
                .flex_col()
                .items_start()
                .gap_4()
                .child(tip("tip-long", LONG, window, cx))
                .child(tip("tip-short", SHORT, window, cx))
                .child(para("para-long", LONG))
                .child(para("para-short", SHORT))
        }
    }

    #[test]
    fn long_chinese_text_wraps_inside_the_tip_and_a_paragraph() {
        let probe = CosmicTextSystem::new("Inter");
        if !UI_FONT_FALLBACKS
            .iter()
            .any(|f| probe.font_id(&font(*f)).is_ok())
        {
            eprintln!("skipped: none of {UI_FONT_FALLBACKS:?} is installed");
            return;
        }
        let mut cx = TestAppContext::build_with_text_system(
            TestDispatcher::new(0),
            None,
            Arc::new(CosmicTextSystem::new("Inter")),
        );
        cx.update(crate::init);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.), px(0.)),
                size: size(px(1200.), px(900.)),
            })),
            ..WindowOptions::default()
        };
        let win = cx
            .update(|cx| gpui_kit::open_window(options, cx, |_, cx| cx.new(|_| Sample)))
            .unwrap()
            .0;
        cx.run_until_parked();
        cx.update_window(win, |_, w, cx| w.render_frame(cx))
            .unwrap();
        let bounds = |cx: &mut TestAppContext, id: &'static str| -> Bounds<Pixels> {
            cx.update_window(win, |_, w, _| w.try_find(id).map(|e| e.bounds()))
                .unwrap()
                .ok_or_else(|| format!("{id} is not drawn"))
                .unwrap()
        };
        for (long, short, room) in [
            ("tip-long", "tip-short", MAX_WIDTH + TIP_MARGIN),
            ("para-long", "para-short", PARAGRAPH),
        ] {
            let (long_box, short_box) = (bounds(&mut cx, long), bounds(&mut cx, short));
            assert!(
                long_box.size.width <= room,
                "{long} {long_box:?} is wider than {room:?}"
            );
            assert!(
                long_box.size.height > short_box.size.height * 1.5,
                "{long} {long_box:?} stays on one line beside {short_box:?}"
            );
        }
        cx.update(|cx| cx.quit());
        cx.run_until_parked();
    }
}
