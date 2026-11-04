//! The six-screen view: the HUD as wireframe tiles on every preset shape at once, with the
//! parts each shape puts off the screen or onto another part filled in.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _};
use gpui_kit::{
    AnyElement, Bounds, Context, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    Point, Size, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
    point, px, rgb, size,
};
use tf2tools_core::ScreenCheck;

use super::checks::FlagLabels;
use super::view::CanvasFit;
use super::{HudMakerPage, Load, SCREEN_LABELS};
use crate::ids;

/// The space around the grid and between its tiles.
const GAP: f32 = 12.;
/// The row the back button sits in, above the grid.
const BACK_ROW: f32 = 44.;
/// The caption's line under each tile's screen.
const CAPTION: f32 = 22.;
/// An edge arrow's side.
const ARROW: f32 = 14.;
/// An edge arrow's tooltip.
const OFF_THIS_SCREEN: &str = "This part is off this screen";

impl HudMakerPage {
    /// Whether the six-screen view takes the canvas area.
    pub fn every_screen(&self) -> bool {
        self.every_screen
    }

    /// Shows or leaves the six-screen view. Entering ends a running drag and Match size and
    /// hands the keys to the page; leaving settles the toolbar and panels again.
    pub fn set_every_screen(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        if on {
            self.part_drag = None;
            if let Some(editor) = self.editor_mut() {
                editor.cancel_drag();
            }
            self.matching_size = false;
            self.stop_comparing(cx);
            self.focus.focus(window, cx);
        } else {
            // The panels were not drawn, so the next frame settles them as a resize would.
            self.checked = Default::default();
        }
        self.every_screen = on;
        cx.notify();
    }

    /// The screen and contrast checks' cache, for the view and the tests.
    pub fn checks(&self) -> &super::checks::Checks {
        &self.checks
    }

    // The back button, then the grid of six tiles filling the rest of the area, three a row.
    // Tiles read the cached checks, so a repaint lays nothing out.
    pub(super) fn screens_view(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Load::Ready(loaded) = &self.load else {
            return Vec::new();
        };
        let (checks, labels) = self.checks.screens_with_labels(&loaded.editor);
        let area = self.area.get();
        let back = div().absolute().left(px(GAP)).top(px(GAP)).child(
            Button::new(ids::HUD_MAKER_SCREENS_BACK)
                .outline()
                .small()
                .label("Back to one screen")
                .accessibility_label("Back to one screen")
                .on_click(
                    cx.listener(|page, _, window, cx| page.set_every_screen(false, window, cx)),
                ),
        );
        let grid_w = (f32::from(area.size.width) - GAP * 2.).max(0.);
        let grid_h = (f32::from(area.size.height) - BACK_ROW - GAP * 2.).max(0.);
        let (cell_w, cell_h) = (
            ((grid_w - GAP * 2.) / 3.).max(0.),
            ((grid_h - GAP) / 2.).max(0.),
        );
        let tiles: Vec<AnyElement> = checks
            .iter()
            .enumerate()
            .map(|(i, check)| {
                let at = point(
                    px((i % 3) as f32 * (cell_w + GAP)),
                    px((i / 3) as f32 * (cell_h + GAP)),
                );
                self.tile(
                    i,
                    check,
                    &labels[i],
                    Bounds::new(at, size(px(cell_w), px(cell_h))),
                    cx,
                )
            })
            .collect();
        let grid = div()
            .id(ids::HUD_MAKER_SCREENS)
            .test_support()
            .absolute()
            .left(px(GAP))
            .top(px(BACK_ROW + GAP))
            .w(px(grid_w))
            .h(px(grid_h))
            .children(tiles);
        vec![back.into_any_element(), grid.into_any_element()]
    }

    // One shape: its screen fitted above the caption, each drawn part as an outline and each
    // flagged part filled, with the caption under it. A click opens that shape.
    fn tile(
        &self,
        index: usize,
        check: &ScreenCheck,
        labels: &FlagLabels,
        cell: Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let (border, danger, muted) = (theme.border, theme.danger, theme.muted_foreground);
        let shape = check.screen();
        let room = Bounds::new(
            point(px(0.), px(0.)),
            size(
                cell.size.width,
                (cell.size.height - px(CAPTION)).max(px(0.)),
            ),
        );
        let fit = CanvasFit::new(room, shape);
        let screen = fit.screen_bounds();
        let layout = check.layout();
        let outlines = check.drawn().iter().filter_map(|key| {
            let b = layout.get(key)?;
            let r = fit.rect(b.x(), b.y(), b.wide(), b.tall());
            Some(
                div()
                    .absolute()
                    .left(r.origin.x - screen.origin.x)
                    .top(r.origin.y - screen.origin.y)
                    .w(r.size.width)
                    .h(r.size.height)
                    .border_1()
                    .border_color(border),
            )
        });
        // A part off the screen is still drawn where the layout put it, clipped by the screen.
        let flags = labels.iter().filter_map(|(key, text)| {
            let b = layout.get(key)?;
            let r = fit.rect(b.x(), b.y(), b.wide(), b.tall());
            Some(
                div()
                    .id(ids::hud_maker_flag(shape, key))
                    .aria_label(text.clone())
                    .test_support()
                    .absolute()
                    .left(r.origin.x - screen.origin.x)
                    .top(r.origin.y - screen.origin.y)
                    .w(r.size.width)
                    .h(r.size.height)
                    .bg(danger.opacity(0.35))
                    .border_1()
                    .border_color(danger),
            )
        });
        // A flagged part wholly off the tile gets an arrow at the edge, pointing its way.
        let arrows = labels.iter().filter_map(|(key, text)| {
            let b = layout.get(key)?;
            let r = fit.rect(b.x(), b.y(), b.wide(), b.tall());
            let local = Bounds::new(r.origin - screen.origin, r.size);
            let (at, icon) = edge_arrow(local, screen.size)?;
            Some(
                div()
                    .id(ids::hud_maker_edge_arrow(shape, key))
                    .aria_label(text.clone())
                    .test_support()
                    .absolute()
                    .left(at.x)
                    .top(at.y)
                    .size(px(ARROW))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_sm()
                    .bg(danger)
                    .text_color(gpui_kit::white())
                    .child(Icon::new(icon).xsmall())
                    .tooltip(|window, cx| Tooltip::new(OFF_THIS_SCREEN).build(window, cx)),
            )
        });
        let screen_box = div()
            .absolute()
            .left(screen.origin.x)
            .top(screen.origin.y)
            .w(screen.size.width)
            .h(screen.size.height)
            .overflow_hidden()
            .bg(rgb(0x101010))
            .children(outlines)
            .children(flags)
            .children(arrows);
        let caption = caption(index, check, labels.len());
        div()
            .id(ids::hud_maker_screen_tile(shape))
            .aria_label(caption.clone())
            .test_support()
            .absolute()
            .left(cell.origin.x)
            .top(cell.origin.y)
            .w(cell.size.width)
            .h(cell.size.height)
            .cursor_pointer()
            .child(screen_box)
            .child(
                div()
                    .absolute()
                    .left_0()
                    .bottom_0()
                    .w_full()
                    .text_xs()
                    .text_color(muted)
                    .child(caption),
            )
            .on_click(cx.listener(move |page, _, window, cx| {
                page.set_screen(shape, window, cx);
                page.set_every_screen(false, window, cx);
            }))
            .into_any_element()
    }
}

/// Where an arrow for part box `r` (local to a screen of `screen`) sits and which way it points,
/// when the part is wholly off that screen; it points along the axis the part is further off.
fn edge_arrow(r: Bounds<Pixels>, screen: Size<Pixels>) -> Option<(Point<Pixels>, IconName)> {
    let (l, t) = (r.origin.x, r.origin.y);
    let (right, bottom) = (l + r.size.width, t + r.size.height);
    let zero = px(0.);
    let off_x = right <= zero || l >= screen.width;
    let off_y = bottom <= zero || t >= screen.height;
    if !off_x && !off_y {
        return None;
    }
    // How far past an edge the part lies on each axis.
    let past_x = (zero - right).max(l - screen.width);
    let past_y = (zero - bottom).max(t - screen.height);
    let a = px(ARROW);
    let along = |start: Pixels, len: Pixels, room: Pixels| {
        (start + len / 2. - a / 2.).min(room - a).max(zero)
    };
    Some(if off_x && (!off_y || past_x >= past_y) {
        let y = along(t, r.size.height, screen.height);
        if right <= zero {
            (point(zero, y), IconName::ArrowLeft)
        } else {
            (point(screen.width - a, y), IconName::ArrowRight)
        }
    } else {
        let x = along(l, r.size.width, screen.width);
        if bottom <= zero {
            (point(x, zero), IconName::ArrowUp)
        } else {
            (point(x, screen.height - a), IconName::ArrowDown)
        }
    })
}

/// "16:9, 1920x1080: No problems" and so on.
fn caption(index: usize, check: &ScreenCheck, flagged: usize) -> String {
    let shape = check.screen();
    let count = match flagged {
        _ if check.too_crowded() => "Too many overlaps to list here".to_owned(),
        0 => "No problems".to_owned(),
        1 => "1 part has a problem".to_owned(),
        n => format!("{n} parts have problems"),
    };
    let ratio = SCREEN_LABELS.get(index).copied().unwrap_or_default();
    format!("{ratio}, {}x{}: {count}", shape.wide(), shape.tall())
}
