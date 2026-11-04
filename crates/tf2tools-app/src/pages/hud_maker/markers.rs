//! "Hard to read here" markers: an outline around each flagged label's text and a small badge
//! beside it, kept off every label's text, off the other badges and inside the canvas.

use gpui_kit::assets::IconName;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::{
    AnyElement, Bounds, Hsla, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    Size, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div, point, px, size,
};
use tf2tools_core::PartKey;

use crate::ids;

/// What the marker says, in its tooltip and to a screen reader.
pub(super) const HARD_TO_READ: &str = "Hard to read here";

/// A badge's side, in window pixels.
const BADGE: f32 = 16.;
/// The space between a badge and its label.
const GAP: f32 = 2.;
/// How far an outline stands off the text it surrounds, in window pixels.
const PAD: f32 = 1.;
/// How many badge widths a badge may move out from its label before it shares a neighbour's.
const RINGS: usize = 4;
/// A leader line's thickness.
const LINE: f32 = 1.;

/// The colours a marker draws in.
pub(super) struct MarkerLook {
    pub warning: Hsla,
    pub background: Hsla,
}

/// The outline around each flagged label's text and its badge, from boxes local to the canvas:
/// `labels` holds each flagged label's text extent, `texts` every visible label's.
pub(super) fn markers(
    labels: &[(PartKey, Bounds<Pixels>)],
    texts: &[Bounds<Pixels>],
    canvas: Size<Pixels>,
    look: &MarkerLook,
) -> Vec<AnyElement> {
    let outlines: Vec<Bounds<Pixels>> = labels.iter().map(|(_, r)| r.dilate(px(PAD))).collect();
    let badges = place_badges(&outlines, texts, canvas);
    let mut out = Vec::with_capacity(labels.len() * 3);
    // Leaders go first, so the outlines and badges paint over their ends.
    for (((key, _), r), badge) in labels.iter().zip(&outlines).zip(&badges) {
        for (i, piece) in leader(badge, r, texts).into_iter().enumerate() {
            out.push(
                div()
                    .id(ids::hud_maker_hard_to_read_leader(key, i))
                    .test_support()
                    .absolute()
                    .left(piece.origin.x)
                    .top(piece.origin.y)
                    .w(piece.size.width)
                    .h(piece.size.height)
                    .bg(look.warning)
                    .into_any_element(),
            );
        }
    }
    for (((key, _), r), badge) in labels.iter().zip(&outlines).zip(badges) {
        // Neither takes a press, so the canvas still selects under the outline.
        out.push(
            div()
                .id(ids::hud_maker_hard_to_read(key))
                .aria_label(HARD_TO_READ)
                .test_support()
                .absolute()
                .left(r.origin.x)
                .top(r.origin.y)
                .w(r.size.width)
                .h(r.size.height)
                .border_1()
                .border_color(look.warning)
                .into_any_element(),
        );
        out.push(
            div()
                .id(ids::hud_maker_hard_to_read_badge(key))
                .aria_label(HARD_TO_READ)
                .test_support()
                .absolute()
                .left(badge.origin.x)
                .top(badge.origin.y)
                .size(px(BADGE))
                .flex()
                .items_center()
                .justify_center()
                .rounded_sm()
                .border_1()
                .border_color(look.warning)
                .bg(look.background)
                .text_color(look.warning)
                .child(Icon::new(IconName::TriangleAlert).xsmall())
                .tooltip(|window, cx| Tooltip::new(HARD_TO_READ).build(window, cx))
                .into_any_element(),
        );
    }
    out
}

/// One badge per outline, in order. Each takes the first spot beside its outline (right, left,
/// above, below, then the same a badge further out, up to `RINGS` out) that stays inside
/// `canvas` and covers no outline, no text and no badge placed before it. Failing that it
/// shares the nearest placed badge, else takes a spot inside and off its own outline, else its
/// outline's corner.
fn place_badges(
    outlines: &[Bounds<Pixels>],
    texts: &[Bounds<Pixels>],
    canvas: Size<Pixels>,
) -> Vec<Bounds<Pixels>> {
    let inside = |b: &Bounds<Pixels>| {
        b.origin.x >= px(0.)
            && b.origin.y >= px(0.)
            && b.origin.x + b.size.width <= canvas.width
            && b.origin.y + b.size.height <= canvas.height
    };
    let mut placed: Vec<Bounds<Pixels>> = Vec::with_capacity(outlines.len());
    for r in outlines {
        let spots: Vec<Bounds<Pixels>> = (0..=RINGS)
            .flat_map(|ring| beside(r, px((BADGE + GAP) * ring as f32)))
            .collect();
        let free = |b: &Bounds<Pixels>| {
            !(outlines.iter().chain(texts).chain(&placed)).any(|other| meets(b, other))
        };
        let nearest = || {
            let mid = r.center();
            (placed.iter()).min_by(|a, b| {
                let d = |x: &Bounds<Pixels>| {
                    let c = x.center();
                    f32::from(c.x - mid.x).powi(2) + f32::from(c.y - mid.y).powi(2)
                };
                d(a).total_cmp(&d(b))
            })
        };
        let spot = (spots.iter().find(|b| inside(b) && free(b)))
            .or_else(nearest)
            .or_else(|| spots.iter().find(|b| inside(b) && !meets(b, r)))
            .copied()
            .unwrap_or_else(|| clamp(Bounds::new(r.origin, square()), canvas));
        placed.push(spot);
    }
    placed
}

/// The line from a badge set apart from its outline back to the outline's nearest point, as
/// level and upright pieces `LINE` thick with the stretches over any of `texts` left out. It
/// runs from the badge's middle, bending once when the badge is off both ends of the outline;
/// a badge within `GAP` of its outline needs none.
fn leader(
    badge: &Bounds<Pixels>,
    outline: &Bounds<Pixels>,
    texts: &[Bounds<Pixels>],
) -> Vec<Bounds<Pixels>> {
    let apart = |a0: Pixels, a1: Pixels, b0: Pixels, b1: Pixels| (b0 - a1).max(a0 - b1);
    let (o, b) = (outline, badge);
    let gap_x = apart(b.left(), b.right(), o.left(), o.right());
    let gap_y = apart(b.top(), b.bottom(), o.top(), o.bottom());
    if gap_x.max(gap_y) <= px(GAP + 0.5) {
        return Vec::new();
    }
    let from = b.center();
    let to = point(
        from.x.max(o.left()).min(o.right()),
        from.y.max(o.top()).min(o.bottom()),
    );
    let line = px(LINE);
    let level = Bounds::new(
        point(from.x.min(to.x), from.y),
        size((from.x - to.x).abs(), line),
    );
    let upright = Bounds::new(
        point(to.x, from.y.min(to.y)),
        size(line, (from.y - to.y).abs()),
    );
    [level, upright]
        .into_iter()
        .filter(|piece| piece.size.width > line || piece.size.height > line)
        .flat_map(|piece| around_texts(piece, texts))
        .collect()
}

/// `piece`, a thin level or upright bar, less every stretch where it crosses one of `texts`.
fn around_texts(piece: Bounds<Pixels>, texts: &[Bounds<Pixels>]) -> Vec<Bounds<Pixels>> {
    let level = piece.size.width >= piece.size.height;
    // Work along the bar's length: (start, end) of the bar and of each text it crosses.
    let along = |r: &Bounds<Pixels>| {
        if level {
            (r.left(), r.right())
        } else {
            (r.top(), r.bottom())
        }
    };
    let mut stretches = vec![along(&piece)];
    for text in texts.iter().filter(|t| meets(&piece, t)) {
        let (t0, t1) = along(text);
        stretches = (stretches.into_iter())
            .flat_map(|(s0, s1)| [(s0, s1.min(t0)), (s0.max(t1), s1)])
            .filter(|(s0, s1)| s1 > s0)
            .collect();
    }
    (stretches.into_iter())
        .map(|(s0, s1)| {
            if level {
                Bounds::new(point(s0, piece.top()), size(s1 - s0, piece.size.height))
            } else {
                Bounds::new(point(piece.left(), s0), size(piece.size.width, s1 - s0))
            }
        })
        .collect()
}

/// The spots beside `r` and `out` further away, in the order they are tried: each side,
/// aligned to either end.
fn beside(r: &Bounds<Pixels>, out: Pixels) -> [Bounds<Pixels>; 8] {
    let (l, t) = (r.origin.x, r.origin.y);
    let (right, bottom) = (l + r.size.width, t + r.size.height);
    let (b, g) = (px(BADGE), px(GAP));
    let at = |x, y| Bounds::new(point(x, y), square());
    let (east, west) = (right + g + out, l - g - b - out);
    let (north, south) = (t - g - b - out, bottom + g + out);
    [
        at(east, t),
        at(west, t),
        at(l, north),
        at(l, south),
        at(east, bottom - b),
        at(west, bottom - b),
        at(right - b, north),
        at(right - b, south),
    ]
}

fn square() -> Size<Pixels> {
    size(px(BADGE), px(BADGE))
}

/// `b` moved the least to lie inside `canvas`.
fn clamp(b: Bounds<Pixels>, canvas: Size<Pixels>) -> Bounds<Pixels> {
    let x = b.origin.x.min(canvas.width - b.size.width).max(px(0.));
    let y = b.origin.y.min(canvas.height - b.size.height).max(px(0.));
    Bounds::new(point(x, y), b.size)
}

/// Strict overlap: touching edges share no area.
fn meets(a: &Bounds<Pixels>, b: &Bounds<Pixels>) -> bool {
    a.origin.x < b.origin.x + b.size.width
        && b.origin.x < a.origin.x + a.size.width
        && a.origin.y < b.origin.y + b.size.height
        && b.origin.y < a.origin.y + a.size.height
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(w), px(h)))
    }

    fn spans(pieces: &[Bounds<Pixels>]) -> Vec<(f32, f32, f32, f32)> {
        (pieces.iter())
            .map(|b| {
                let (x, y) = (f32::from(b.origin.x), f32::from(b.origin.y));
                (x, y, f32::from(b.size.width), f32::from(b.size.height))
            })
            .collect()
    }

    #[test]
    fn a_badge_touching_its_outline_has_no_leader() {
        let outline = rect(100., 100., 40., 20.);
        for badge in beside(&outline, px(0.)) {
            assert!(leader(&badge, &outline, &[]).is_empty(), "{badge:?}");
        }
        let inside = rect(110., 102., BADGE, BADGE);
        assert!(leader(&inside, &outline, &[]).is_empty());
    }

    #[test]
    fn a_moved_badge_leads_straight_back_to_its_outline() {
        let outline = rect(100., 100., 40., 20.);
        let ring = px((BADGE + GAP) * 2.);
        let [east, _, north, ..] = beside(&outline, ring);
        // East, top-aligned: a level line at the badge's middle from the badge to the right edge.
        assert_eq!(
            spans(&leader(&east, &outline, &[])),
            [(140., 108., f32::from(east.origin.x) + 8. - 140., LINE)]
        );
        // Above, left-aligned: an upright line at the badge's middle down to the top edge.
        let top = f32::from(north.origin.y) + 8.;
        assert_eq!(
            spans(&leader(&north, &outline, &[])),
            [(108., top, LINE, 100. - top)]
        );
    }

    #[test]
    fn a_leader_leaves_out_the_stretches_over_text() {
        let outline = rect(100., 100., 40., 20.);
        let badge = rect(200., 100., BADGE, BADGE);
        let text = rect(160., 104., 10., 10.);
        assert_eq!(
            spans(&leader(&badge, &outline, &[text])),
            [(140., 108., 20., LINE), (170., 108., 38., LINE)]
        );
    }

    #[test]
    fn a_badge_off_both_ends_bends_once_to_the_nearest_corner() {
        let outline = rect(100., 100., 40., 20.);
        let badge = rect(200., 200., BADGE, BADGE);
        assert_eq!(
            spans(&leader(&badge, &outline, &[])),
            [(140., 208., 68., LINE), (140., 120., LINE, 88.)]
        );
    }
}
