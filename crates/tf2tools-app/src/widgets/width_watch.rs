//! A zero-height width probe for a layout that must follow its own column, not the window.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::{IntoElement, Pixels, Styled as _, canvas};

/// Records its parent's width, padding included, in `cell`; a change asks for another frame.
/// Put it last in the column: it has no height, so it never adds to the scroll extent.
pub fn width_watch(cell: &Rc<Cell<Option<Pixels>>>) -> impl IntoElement {
    let measured = Rc::clone(cell);
    canvas(
        move |bounds, window, _| {
            if measured.get() != Some(bounds.size.width) {
                measured.set(Some(bounds.size.width));
                window.request_animation_frame();
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .left_0()
    .w_full()
    .h_0()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::component::{h_flex, v_flex};
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{
        AppContext as _, Context, ParentElement as _, Render, TestAppContext, Window,
        WindowOptions, div, px,
    };

    // A padded column with the watch as its last child; counts its renders.
    struct Column {
        width: Rc<Cell<Option<Pixels>>>,
        renders: Rc<Cell<usize>>,
    }

    impl Render for Column {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.renders.set(self.renders.get() + 1);
            h_flex().size_full().child(
                v_flex()
                    .w(px(500.))
                    .py(px(48.))
                    .px(px(24.))
                    .child(div().h(px(40.)))
                    .child(width_watch(&self.width)),
            )
        }
    }

    #[gpui_kit::test]
    fn the_width_watch_records_the_padded_width_then_asks_for_a_frame(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let width = Rc::new(Cell::new(None));
        let renders = Rc::new(Cell::new(0));
        let (w, r) = (Rc::clone(&width), Rc::clone(&renders));
        let win = cx
            .update(|cx| {
                gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
                    cx.new(|_| Column {
                        width: w,
                        renders: r,
                    })
                })
            })
            .unwrap()
            .0;
        cx.run_until_parked();
        cx.update_window(win, |_, w, cx| w.render_frame(cx))
            .unwrap();
        assert_eq!(width.get(), Some(px(500.)));
        // The first measurement must bring one more render, since a draw's own notify is dropped.
        let before = renders.get();
        cx.update_window(win, |_, w, cx| w.simulate_next_frame(cx))
            .unwrap();
        cx.run_until_parked();
        assert!(renders.get() > before, "no frame was asked for");
    }
}
