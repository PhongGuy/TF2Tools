//! v5's SyncIndicator ring: a muted track under a primary arc, drawn by the app because the
//! kit's `ProgressCircle` derives its track from the arc colour.

use std::f32::consts::TAU;
use std::time::Duration;

use gpui_kit::base::{Transition, transition};
use gpui_kit::component::plot::PathCaches;
use gpui_kit::component::plot::shape::{Arc, ArcData};
use gpui_kit::component::{ActiveTheme as _, Theme};
use gpui_kit::{
    Animation, AnimationExt as _, AnyElement, App, Bounds, ElementId, Hsla,
    InteractiveElement as _, IntoElement, ParentElement, Pixels, Point, RenderOnce, Styled as _,
    TestSupportExt as _, Window, canvas, div, fill, point, px, size,
};

/// v5's ring box and stroke (SyncIndicator.svelte SIZE and STROKE).
pub const RING_SIZE: Pixels = px(24.);
pub const RING_STROKE: Pixels = px(3.);
/// The stroke's midline radius: v5's circle r = (SIZE - STROKE) / 2.
pub const RING_RADIUS: Pixels = px(10.5);
/// One turn of the curated spinner (Tailwind's animate-spin).
pub const SPIN_PERIOD: Duration = Duration::from_secs(1);

/// The centres of the round caps of an arc from `start` to `end` percent of a turn, centred in
/// `bounds`: on the stroke's midline ([`RING_RADIUS`]), 0 at 12 o'clock, clockwise.
pub fn cap_centres(bounds: Bounds<Pixels>, start: f32, end: f32) -> [Point<Pixels>; 2] {
    let centre = bounds.center();
    let radius = RING_RADIUS.as_f32();
    [start, end].map(|pct| {
        let angle = pct / 100. * TAU;
        point(
            centre.x + px(radius * angle.sin()),
            centre.y - px(radius * angle.cos()),
        )
    })
}

/// The curated spinner's arc at `delta` (0..=1) of a turn, in percent: a fixed quarter that
/// starts at 3 o'clock; `start` is kept in 0..100 and `end` is `start + 25`.
pub fn spin_arc(delta: f32) -> (f32, f32) {
    let start = (25. + delta * 100.).rem_euclid(100.);
    (start, start + 25.)
}

/// The ring's two colours: v5's muted-foreground/20 track and primary arc.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RingColors {
    pub track: Hsla,
    pub arc: Hsla,
}

impl RingColors {
    /// v5's `text-muted-foreground/20` track and `text-primary` arc, from `theme`.
    pub fn of(theme: &Theme) -> Self {
        Self {
            track: theme.muted_foreground.opacity(0.2),
            arc: theme.primary,
        }
    }
}

/// A 24 px ring, registered for lookup under its id: the full track, then the arc clockwise from
/// 12 o'clock to `value` percent, or a spinning arc while `spinning`; children are centred over it.
#[derive(IntoElement)]
pub struct SyncRing {
    id: ElementId,
    value: f32,
    spinning: bool,
    colors: RingColors,
    children: Vec<AnyElement>,
}

impl SyncRing {
    pub fn new(id: impl Into<ElementId>, colors: RingColors) -> Self {
        Self {
            id: id.into(),
            value: 0.,
            spinning: false,
            colors,
            children: Vec::new(),
        }
    }

    /// The arc's end, in percent of a turn; clamped to 0..=100.
    pub fn value(mut self, percent: f32) -> Self {
        self.value = percent.clamp(0., 100.);
        self
    }

    /// Spin an indeterminate arc instead of showing `value`.
    pub fn spinning(mut self, spinning: bool) -> Self {
        self.spinning = spinning;
        self
    }
}

impl ParentElement for SyncRing {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

// The track as a full turn, then the arc from `start` to `end` percent of a turn, stroked
// inward from the box edge as v5's svg circle is. gpui strokes have no round cap, so a disc as
// wide as the stroke closes each end of the arc like v5's stroke-linecap="round".
fn arcs(start: f32, end: f32, colors: RingColors) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds: Bounds<Pixels>, (), window: &mut Window, cx: &mut App| {
            let outer = bounds.size.width.min(bounds.size.height).as_f32() / 2.;
            let arc = Arc::new()
                .inner_radius(outer - RING_STROKE.as_f32())
                .outer_radius(outer);
            let caches = PathCaches::for_paint("sync-ring-paths", window, cx);
            caches.update(cx, |caches, _| {
                let track = ArcData::new(&(), 0, 100., 0., TAU);
                arc.paint_cached(&track, colors.track, &bounds, caches.slot(0), window);
                if end > start {
                    let data =
                        ArcData::new(&(), 1, end - start, start / 100. * TAU, end / 100. * TAU);
                    arc.paint_cached(&data, colors.arc, &bounds, caches.slot(1), window);
                }
            });
            if end > start {
                for centre in cap_centres(bounds, start, end) {
                    let disc = Bounds::centered_at(centre, size(RING_STROKE, RING_STROKE));
                    window.paint_quad(fill(disc, colors.arc).corner_radii(RING_STROKE / 2.));
                }
            }
        },
    )
    .absolute()
    .size_full()
}

impl RenderOnce for SyncRing {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // v5 eases the arc between ticks (transition-[stroke-dashoffset] duration-300).
        let value = transition(
            (self.id.clone(), "value"),
            self.value,
            Transition::new(cx.theme().motion_tokens().duration_normal)
                .easing(cx.theme().motion_tokens().easing_move.clone()),
            window,
            cx,
        );
        let colors = self.colors;
        let ring = div()
            .id(self.id.clone())
            .test_support()
            .relative()
            .flex()
            .items_center()
            .justify_center()
            .size(RING_SIZE)
            .children(self.children);
        if self.spinning {
            ring.with_animation(
                "sync-ring-spin",
                Animation::new(SPIN_PERIOD).repeat(),
                move |ring, delta| {
                    let (start, end) = spin_arc(delta);
                    ring.child(arcs(start, end, colors))
                },
            )
            .into_any_element()
        } else {
            ring.child(arcs(0., value, colors)).into_any_element()
        }
    }
}
