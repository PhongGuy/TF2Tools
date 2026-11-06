//! The canvas's zoom and pan: the toolbar's zoom controls, the keys, Ctrl+wheel around the
//! pointer, and pans by middle-drag, Space+drag or the wheel.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::{Disableable as _, Sizable as _, v_flex};
use gpui_kit::{
    Anchor, AnyElement, Bounds, Context, IntoElement, KeyDownEvent, KeyUpEvent, MouseMoveEvent,
    ParentElement as _, Pixels, Point, ScrollWheelEvent, Styled as _, Window, point, px,
};
use tf2tools_core::Screen;

use super::HudMakerPage;
use super::view::{CanvasFit, Zoom};
use crate::ids;
use crate::pages::crosshair_maker::model::{
    ZOOM_IN_TIP, ZOOM_OUT_TIP, wheel_scroll, wheel_zoom_steps,
};

const FIT_TIP: &str = "Fit the screen to the window (Ctrl+0)";
const ZOOM_TIP: &str = "Zoom (Ctrl+wheel; middle-drag or Space+drag to pan)";

impl HudMakerPage {
    /// Where the screen sits on the canvas at the player's zoom. Every conversion between
    /// window and screen pixels on the canvas goes through this one fit.
    pub(super) fn canvas_fit(&self, screen: Screen) -> CanvasFit {
        CanvasFit::zoomed(self.screen_area(), screen, self.zoom_for(screen))
    }

    // A zoom made for another screen shape no longer applies, so a new shape starts fitted.
    fn zoom_for(&self, screen: Screen) -> Zoom {
        match self.zoom {
            Some((made_for, zoom)) if made_for == screen => zoom,
            _ => Zoom::FIT,
        }
    }

    // Applies `change` to the current zoom; a part drag measures at the scale it began at, so
    // the view holds still while one runs.
    fn change_zoom(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(Zoom, Bounds<Pixels>, Screen) -> Zoom,
    ) {
        let Some(screen) = self.editor().map(|e| e.screen()) else {
            return;
        };
        if self.part_drag.is_some() {
            return;
        }
        let before = self.zoom_for(screen);
        let after = change(before, self.screen_area(), screen);
        if after != before {
            self.zoom = Some((screen, after));
            cx.notify();
        }
    }

    /// Zooms `steps` levels in (out when negative) around the middle of the canvas.
    pub(super) fn zoom_canvas(&mut self, steps: i32, cx: &mut Context<Self>) {
        let mid = self.screen_area().center();
        self.change_zoom(cx, |z, area, screen| z.step(steps, mid, area, screen));
    }

    pub(super) fn fit_canvas(&mut self, cx: &mut Context<Self>) {
        self.change_zoom(cx, |_, _, _| Zoom::FIT);
    }

    /// Ctrl+wheel zooms around the pointer; a plain wheel pans, sideways with Shift.
    pub(super) fn wheel_canvas(
        &mut self,
        ev: &ScrollWheelEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if ev.modifiers.secondary() {
            let steps = wheel_zoom_steps(ev.delta, &mut self.zoom_carry);
            let at = ev.position;
            self.change_zoom(cx, |z, area, screen| z.step(steps, at, area, screen));
        } else {
            self.zoom_carry = px(0.);
            let travel = ev.delta.pixel_delta(window.line_height());
            // The wheel moves the view the way a page scrolls: down shows what is below.
            let scroll = wheel_scroll(travel, ev.modifiers.shift);
            let by = point(-scroll.x, -scroll.y);
            self.change_zoom(cx, |z, area, screen| z.pan_by(by, area, screen));
        }
    }

    /// Starts a pan from window point `at`; the press also hands the keys to the page.
    pub(super) fn start_canvas_pan(
        &mut self,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.part_drag.is_none() {
            self.canvas_pan = Some(at);
            self.focus.focus(window, cx);
            cx.notify();
        }
    }

    /// Moves a running pan with the pointer; a move with no button held ends it, in case its
    /// release never reached the window.
    pub(super) fn pan_canvas(&mut self, ev: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(last) = self.canvas_pan else {
            return;
        };
        if ev.pressed_button.is_none() {
            return self.end_canvas_pan(cx);
        }
        self.canvas_pan = Some(ev.position);
        let by = ev.position - last;
        self.change_zoom(cx, |z, area, screen| z.pan_by(by, area, screen));
    }

    pub(super) fn end_canvas_pan(&mut self, cx: &mut Context<Self>) {
        if self.canvas_pan.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn is_panning(&self) -> bool {
        self.canvas_pan.is_some()
    }

    /// Space down makes a left press on the canvas pan; it may arrive as a repeat.
    pub(super) fn space_down(&mut self, ev: &KeyDownEvent, cx: &mut Context<Self>) {
        if ev.keystroke.key == "space" && !self.space_held {
            self.space_held = true;
            cx.notify();
        }
    }

    pub(super) fn space_up(&mut self, ev: &KeyUpEvent, cx: &mut Context<Self>) {
        if ev.keystroke.key == "space" && self.space_held {
            self.space_held = false;
            cx.notify();
        }
    }

    /// The toolbar's zoom menu: a button reading the level, opening Zoom in, Zoom out and Fit.
    /// One button keeps the toolbar on one row; the menu stays open so Zoom in can repeat.
    pub(super) fn zoom_controls(&self, screen: Screen, cx: &mut Context<Self>) -> AnyElement {
        let zoom = self.zoom_for(screen);
        let page = cx.entity().downgrade();
        let trigger = Button::new(ids::HUD_MAKER_ZOOM)
            .ghost()
            .label(zoom.label())
            .tooltip(ZOOM_TIP);
        let item = |id: &'static str, icon: IconName, label: &'static str, tip: &'static str| {
            Button::new(id)
                .ghost()
                .small()
                .icon(icon)
                .label(label)
                .tooltip(tip)
        };
        let zoom_in = item(
            ids::HUD_MAKER_ZOOM_IN,
            IconName::ZoomIn,
            "Zoom in",
            ZOOM_IN_TIP,
        )
        .disabled(!zoom.can_zoom_in())
        .on_click(cx.listener(|page, _, _, cx| page.zoom_canvas(1, cx)));
        let zoom_out = item(
            ids::HUD_MAKER_ZOOM_OUT,
            IconName::ZoomOut,
            "Zoom out",
            ZOOM_OUT_TIP,
        )
        .disabled(!zoom.can_zoom_out())
        .on_click(cx.listener(|page, _, _, cx| page.zoom_canvas(-1, cx)));
        let fit = item(ids::HUD_MAKER_ZOOM_FIT, IconName::Maximize, "Fit", FIT_TIP)
            .disabled(zoom.is_fit())
            .on_click(cx.listener(|page, _, _, cx| {
                page.zoom_open = false;
                page.fit_canvas(cx);
            }));
        Popover::new("hud-maker-zoom-menu")
            .anchor(Anchor::BottomLeft)
            .open(self.zoom_open)
            .on_open_change(move |&open, _, cx| {
                let _ = page.update(cx, |page, cx| {
                    page.zoom_open = open;
                    cx.notify();
                });
            })
            .trigger(trigger)
            .child(
                v_flex()
                    .gap_1()
                    .items_start()
                    .children([zoom_in, zoom_out, fit]),
            )
            .into_any_element()
    }
}
