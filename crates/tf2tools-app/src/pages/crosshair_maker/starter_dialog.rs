//! The starter shapes dialog: pick a shape, size it with sliders and watch a live preview.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::slider::{Slider, SliderState};
use gpui_kit::component::{
    ActiveTheme as _, Selectable as _, Sizable as _, WindowExt as _, h_flex, v_flex,
};
use gpui_kit::{
    App, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, WeakEntity, Window, canvas, div, fill, point, px, size,
};
use tf2tools_core::RgbaImage;

use super::CrosshairMakerPage;
use super::model::{CanvasSize, hsla_of_rgba};
use super::starter::{Param, Starter, StarterParams, starter_drawing};
use crate::ids;
use crate::widgets::dialog_frame::{DIALOG_WIDTH, DialogFrame};

/// The preview box's side; the drawing is scaled to whole pixels per cell inside it.
const PREVIEW_SIZE: Pixels = px(128.);

/// The dialog's shape, sliders and the canvas size and paint the starter is drawn in.
pub struct StarterDialog {
    /// The page Use hands the drawing to.
    page: WeakEntity<CrosshairMakerPage>,
    size: CanvasSize,
    paint: [u8; 4],
    shape: Starter,
    /// One slider per `Param::ALL`, in that order; a shape shows only the ones it uses.
    sliders: [Entity<SliderState>; 4],
}

impl StarterDialog {
    /// Builds the dialog for a `size` canvas painted with `paint` and opens it (deferred, as
    /// `confirm` does) on Cross with its defaults.
    pub fn open(
        page: WeakEntity<CrosshairMakerPage>,
        size: CanvasSize,
        paint: [u8; 4],
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        let dialog = cx.new(|cx| Self::new(page, size, paint, cx));
        let content = dialog.clone();
        // Deferred so the clicked toolbar button cannot pull focus back out of the dialog.
        window.defer(cx, move |window, cx| {
            let frame = DialogFrame::new(DIALOG_WIDTH);
            window.open_dialog(cx, move |dialog, window, cx| {
                // The kit closes on true: Enter does nothing, Escape and the X close.
                frame
                    .apply(dialog, &*window, &*cx)
                    .on_ok(|_, _, _| false)
                    .on_cancel(|_, _, _| true)
                    .child(frame.measure(content.clone()))
            });
        });
        dialog
    }

    fn new(
        page: WeakEntity<CrosshairMakerPage>,
        size: CanvasSize,
        paint: [u8; 4],
        cx: &mut Context<Self>,
    ) -> Self {
        let shape = Starter::Cross;
        let defaults = shape.defaults(size);
        let sliders = Param::ALL.map(|param| {
            let (min, max, step) = param.limits(size);
            let slider = cx.new(|_| {
                SliderState::new()
                    .min(min as f32)
                    .max(max as f32)
                    .step(step as f32)
                    .default_value(defaults.get(param) as f32)
            });
            // The labels and preview are drawn from the slider values, so any change redraws.
            cx.observe(&slider, |_, _, cx| cx.notify()).detach();
            slider
        });
        Self {
            page,
            size,
            paint,
            shape,
            sliders,
        }
    }

    pub fn shape(&self) -> Starter {
        self.shape
    }

    pub fn slider(&self, param: Param) -> &Entity<SliderState> {
        let ix = Param::ALL
            .iter()
            .position(|p| *p == param)
            .expect("every param has a slider");
        &self.sliders[ix]
    }

    /// Every slider snapped (`Param::snap`).
    pub fn params(&self, cx: &App) -> StarterParams {
        let mut params = self.shape.defaults(self.size);
        for param in Param::ALL {
            let value = self.slider(param).read(cx).value().start();
            params.set(param, param.snap(self.size, value));
        }
        params
    }

    /// `starter_drawing` of the shape and params in the dialog's size and paint.
    pub fn drawing(&self, cx: &App) -> RgbaImage {
        starter_drawing(self.shape, self.params(cx), self.size, self.paint)
    }

    // A new shape starts from its own defaults, so every slider is reset to them.
    fn choose(&mut self, shape: Starter, window: &mut Window, cx: &mut Context<Self>) {
        self.shape = shape;
        let defaults = shape.defaults(self.size);
        for param in Param::ALL {
            let value = defaults.get(param) as f32;
            self.slider(param)
                .clone()
                .update(cx, |state, cx| state.set_value(value, window, cx));
        }
        cx.notify();
    }

    // Closes the dialog before handing over: close_dialog pops it at once and the page's
    // discard confirm opens in a deferred step, so the two dialogs never stack.
    fn use_drawing(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let drawing = self.drawing(cx);
        window.close_dialog(cx);
        let _ = self
            .page
            .update(cx, |page, cx| page.use_starter(drawing, window, cx));
    }

    // The drawing scaled to whole pixels per cell over a checkerboard, so see-through paint
    // reads as it does on the grid.
    fn preview(&self, cx: &App) -> impl IntoElement {
        let theme = cx.theme();
        let (light, dark, border) = (theme.muted, theme.background, theme.border);
        let drawing = self.drawing(cx);
        let side = self.size.side();
        let cell = (PREVIEW_SIZE / side as f32).floor();
        let picture = canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                window.paint_quad(fill(bounds, dark));
                let at = |x: u32, y: u32| {
                    gpui_kit::Bounds::new(
                        point(
                            bounds.origin.x + cell * x as f32,
                            bounds.origin.y + cell * y as f32,
                        ),
                        size(cell, cell),
                    )
                };
                for (i, &rgba) in drawing.pixels().as_chunks::<4>().0.iter().enumerate() {
                    let (x, y) = (i as u32 % side, i as u32 / side);
                    if (x + y) % 2 == 0 {
                        window.paint_quad(fill(at(x, y), light));
                    }
                    if rgba[3] != 0 {
                        window.paint_quad(fill(at(x, y), hsla_of_rgba(rgba)));
                    }
                }
            },
        )
        .size(cell * side as f32);
        div()
            .id(ids::CROSSHAIR_MAKER_STARTER_PREVIEW)
            .aria_label(format!("{} preview", self.shape.label()))
            .test_support()
            .flex_none()
            .size(PREVIEW_SIZE)
            .rounded_lg()
            .border_1()
            .border_color(border)
            .overflow_hidden()
            .flex()
            .items_center()
            .justify_center()
            .child(picture)
    }
}

impl Render for StarterDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let shapes = h_flex()
            .gap_2()
            .flex_wrap()
            .children(Starter::ALL.map(|shape| {
                Button::new(ids::crosshair_maker_starter(shape.slug()))
                    .outline()
                    .small()
                    .label(shape.label())
                    .accessibility_label(shape.label())
                    .selected(shape == self.shape)
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.choose(shape, window, cx)),
                    )
            }));
        let params = self.params(cx);
        let sliders = v_flex()
            .gap_3()
            .text_sm()
            .children(self.shape.params().iter().map(|&param| {
                let text = format!("{}: {} px", param.label(), params.get(param));
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .id(ids::crosshair_maker_starter_value(param.slug()))
                            .aria_label(text.clone())
                            .test_support()
                            .child(text),
                    )
                    .child(
                        div()
                            .id(ids::crosshair_maker_starter_slider(param.slug()))
                            .test_support()
                            .child(Slider::new(self.slider(param))),
                    )
            }));
        v_flex()
            .id(ids::CROSSHAIR_MAKER_STARTER_DIALOG)
            .test_support()
            .gap_4()
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Starter shapes"),
                    )
                    .child(div().text_sm().text_color(muted).child(
                        "Pick a shape and set it up. Use starts a new drawing you can keep editing.",
                    )),
            )
            .child(shapes)
            .child(
                h_flex()
                    .gap_4()
                    .items_start()
                    .child(self.preview(cx))
                    .child(sliders.flex_1()),
            )
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new(ids::CROSSHAIR_MAKER_STARTER_CANCEL)
                            .outline()
                            .label("Cancel")
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new(ids::CROSSHAIR_MAKER_STARTER_USE)
                            .primary()
                            .label("Use")
                            .on_click(
                                cx.listener(|this, _, window, cx| this.use_drawing(window, cx)),
                            ),
                    ),
            )
    }
}
