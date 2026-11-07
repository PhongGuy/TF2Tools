//! Snapping for what core's `HudLayout::snap_move` does not cover: several parts moving as one
//! box, and the one edge a resize handle moves. Targets are the same as core's: the screen
//! centre and the start, centre and end of every drawn part outside the movers.

use tf2tools_core::{GuideAxis, HudLayout, HudModel, PartKey, Rect, Snap};

/// One guide line in screen pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Guide {
    axis: GuideAxis,
    at: i32,
}

impl Guide {
    /// Which way the line runs.
    pub fn axis(&self) -> GuideAxis {
        self.axis
    }

    /// The x of a vertical guide or the y of a horizontal one.
    pub fn at(&self) -> i32 {
        self.at
    }
}

/// A drag delta after snapping and the guides it met, at most one per axis, vertical first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapped {
    dx: i32,
    dy: i32,
    guides: Vec<Guide>,
}

impl Snapped {
    pub fn dx(&self) -> i32 {
        self.dx
    }

    pub fn dy(&self) -> i32 {
        self.dy
    }

    pub fn guides(&self) -> &[Guide] {
        &self.guides
    }
}

impl From<Snap> for Snapped {
    fn from(s: Snap) -> Snapped {
        let guides = (s.guides().iter())
            .map(|g| Guide {
                axis: g.axis(),
                at: g.at(),
            })
            .collect();
        Snapped {
            dx: s.dx(),
            dy: s.dy(),
            guides,
        }
    }
}

/// The lines a dragged box offers on each axis: the x lines and the y lines, before the delta.
pub(super) struct Moving {
    pub across: Vec<i64>,
    pub down: Vec<i64>,
}

/// Snaps a drag of `movers` by (`dx`, `dy`) whose moving lines are `moving`, on `layout` as
/// the drag began. Each axis shifts to the nearest target within `threshold`.
pub(super) fn snap(
    model: &HudModel,
    layout: &HudLayout,
    movers: &[PartKey],
    moving: &Moving,
    (dx, dy): (i32, i32),
    threshold: i32,
) -> Snapped {
    let mut out = Snapped {
        dx,
        dy,
        guides: Vec::new(),
    };
    if threshold < 0 {
        return out;
    }
    // A mover's own children ride along with it, so they offer no lines either.
    let moves = |key: &PartKey| {
        let mut at = Some(key);
        while let Some(k) = at {
            if movers.contains(k) {
                return true;
            }
            at = model.part(k).and_then(|p| p.parent());
        }
        false
    };
    let targets: Vec<Rect> = (layout.boxes().iter())
        .filter(|b| b.hidden().is_none() && !moves(b.key()))
        .filter_map(|b| layout.shown(b.key()))
        .collect();
    let screen = layout.screen();
    let x = nearest(
        moving.across.iter().map(|l| l + i64::from(dx)),
        i64::from(screen.wide() / 2),
        targets.iter().flat_map(|r| lines(r.x, r.wide)),
        threshold,
    );
    let y = nearest(
        moving.down.iter().map(|l| l + i64::from(dy)),
        i64::from(screen.tall() / 2),
        targets.iter().flat_map(|r| lines(r.y, r.tall)),
        threshold,
    );
    for (found, delta, axis) in [
        (x, &mut out.dx, GuideAxis::Vertical),
        (y, &mut out.dy, GuideAxis::Horizontal),
    ] {
        if let Some((shift, at)) = found {
            *delta = saturate(i64::from(*delta) + shift);
            out.guides.push(Guide {
                axis,
                at: saturate(at),
            });
        }
    }
    out
}

/// A box's start, centre and end along one axis.
pub(super) fn lines(start: i32, len: i32) -> [i64; 3] {
    let (start, len) = (i64::from(start), i64::from(len));
    [start, start + len / 2, start + len]
}

/// The shift that brings the nearest moving line onto a target line within `threshold`, and
/// that line; the screen centre comes first and ties keep the earlier pair, as core does.
fn nearest(
    moving: impl Iterator<Item = i64> + Clone,
    centre: i64,
    targets: impl Iterator<Item = i64>,
    threshold: i32,
) -> Option<(i64, i64)> {
    let mut best: Option<(i64, i64)> = None;
    for target in std::iter::once(centre).chain(targets) {
        for line in moving.clone() {
            let shift = target - line;
            let closer = best.is_none_or(|(b, _)| shift.abs() < b.abs());
            if shift.abs() <= i64::from(threshold) && closer {
                best = Some((shift, target));
            }
        }
    }
    best
}

fn saturate(value: i64) -> i32 {
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}
