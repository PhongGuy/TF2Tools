//! v5's dialog frame: shadcn's widths and a vertically centred box. The kit's dialog is
//! 448 px wide and sits a tenth of the viewport from the top; it centres nothing itself
//! because it cannot know its height before layout, so the frame measures the content and
//! feeds the result back as `margin_top` on the next frame.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::{ActiveTheme as _, window_paddings};
use gpui_kit::prelude::*;
use gpui_kit::{App, Bounds, Pixels, Window, canvas, div, px};

/// v5's `max-w-lg` dialog width.
pub const DIALOG_WIDTH: Pixels = px(512.);
/// v5's `max-w-2xl` detail dialog width.
pub const DETAIL_WIDTH: Pixels = px(672.);
/// v5's `p-6` dialog content padding.
pub const DIALOG_PADDING: Pixels = px(24.);
/// The box's 24 px padding and 1 px border, top and bottom.
pub const BOX_CHROME: Pixels = px(50.);

/// The top offset that centres a `box_height` box in `view_height`, never above `margin`.
pub fn centred_top(view_height: Pixels, box_height: Pixels, margin: Pixels) -> Pixels {
    ((view_height - box_height) / 2.).max(margin)
}

/// v5's `max-h-[90vh]` content cap for a window `viewport_height` tall that spends
/// `insets` on its frame: 90% of the visible window, but never past the kit's surface,
/// which keeps `margin` clear above and below. An outgrown surface scrolls itself and
/// takes the wheel from the dialog's own body.
pub fn content_cap(viewport_height: Pixels, insets: Pixels, margin: Pixels) -> Pixels {
    let visible = viewport_height - insets;
    (visible * 0.9).min(visible - margin * 2.) - BOX_CHROME
}

/// `content_cap` for `window` as it is now.
pub fn content_max_height(window: &Window, cx: &App) -> Pixels {
    let pads = window_paddings(window);
    content_cap(
        window.viewport_size().height,
        pads.top + pads.bottom,
        cx.theme().spacing_tokens().lg,
    )
}

/// A dialog's width and its last measured content height, shared across builder frames.
#[derive(Clone)]
pub struct DialogFrame {
    width: Pixels,
    padding: Pixels,
    content_height: Rc<Cell<Option<Pixels>>>,
}

impl DialogFrame {
    /// A frame of `width` padded at `DIALOG_PADDING`, not yet measured.
    pub fn new(width: Pixels) -> Self {
        Self {
            width,
            padding: DIALOG_PADDING,
            content_height: Rc::new(Cell::new(None)),
        }
    }

    /// The same frame with the box padded by `padding` on every side instead of v5's 24 px.
    pub fn padded(mut self, padding: Pixels) -> Self {
        self.padding = padding;
        self
    }

    /// Sets the width and, once the content has been measured, the centring `margin_top`.
    pub fn apply(&self, dialog: Dialog, window: &Window, cx: &App) -> Dialog {
        // The kit's box pads 16 px on its own, so the frame always sets its padding explicitly.
        let p = self.padding;
        let dialog = dialog.w(self.width).p(p);
        let chrome = BOX_CHROME + (p - DIALOG_PADDING) * 2.;
        // Unmeasured, the first frame keeps the kit's offset; the slide-in starts at the top anyway.
        let Some(content) = self.content_height.get() else {
            return dialog;
        };
        let pads = window_paddings(window);
        let view = window.viewport_size().height - pads.top - pads.bottom;
        dialog.margin_top(centred_top(
            view,
            content + chrome,
            cx.theme().spacing_tokens().lg,
        ))
    }

    /// Wraps the dialog's whole content so its height is measured on every frame.
    pub fn measure(&self, content: impl IntoElement) -> impl IntoElement {
        let cell = Rc::clone(&self.content_height);
        div().relative().w_full().child(content).child(
            canvas(
                move |bounds: Bounds<Pixels>, window: &mut Window, _: &mut App| {
                    // A changed height re-centres on the next frame, so growth (a loaded file
                    // list, an error row) never leaves the box off centre. `refresh` is a no-op
                    // while drawing, so it is deferred to the next frame.
                    if cell.get() != Some(bounds.size.height) {
                        cell.set(Some(bounds.size.height));
                        window.on_next_frame(|window, _| window.refresh());
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
    }
}
