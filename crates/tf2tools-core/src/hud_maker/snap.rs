//! Snapping a dragged part to the screen centre and to other parts' edges and centres. A pure
//! search over the boxes of one layout; the caller passes the layout the drag began on.

use super::{HudLayout, PartKey};

/// Which way a guide line runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum GuideAxis {
    /// A line at one x, for a snap across.
    Vertical,
    /// A line at one y, for a snap up or down.
    Horizontal,
}

/// One guide line in screen pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
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

/// A move delta after snapping and the guides it met: at most one per axis, vertical first.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Snap {
    dx: i32,
    dy: i32,
    guides: Vec<Guide>,
}

impl Snap {
    /// The snapped horizontal delta in screen pixels.
    pub fn dx(&self) -> i32 {
        self.dx
    }

    /// The snapped vertical delta in screen pixels.
    pub fn dy(&self) -> i32 {
        self.dy
    }

    /// The guide lines the snap met.
    pub fn guides(&self) -> &[Guide] {
        &self.guides
    }
}

impl HudLayout {
    /// Snaps a move of `part` by (`dx`, `dy`) screen pixels from its box in this layout to the
    /// nearest target line within `threshold` pixels on each axis. An unknown part, or a
    /// negative threshold, gives the delta back with no guides.
    pub fn snap_move(&self, part: &PartKey, dx: i32, dy: i32, threshold: i32) -> Snap {
        let unsnapped = Snap {
            dx,
            dy,
            guides: Vec::new(),
        };
        let Some(own) = self.get(part).filter(|_| threshold >= 0) else {
            return unsnapped;
        };
        // Targets are drawn parts other than this one and its own children, which move with it,
        // by the area the game shows of each: a part it cuts off whole offers no edges. The
        // mover keeps its whole box, since its shown area changes shape as it leaves a clip.
        let targets: Vec<(i32, i32, i32, i32)> = (self.boxes().iter())
            .filter(|b| b.hidden().is_none() && b.key() != part && !self.is_under(b.key(), part))
            .filter_map(|b| self.shown(b.key()))
            .collect();
        let screen = self.screen();
        let mut snap = unsnapped;
        let across = |&(x, _, wide, _): &(i32, i32, i32, i32)| lines(x, wide);
        let down = |&(_, y, _, tall): &(i32, i32, i32, i32)| lines(y, tall);
        let x = nearest(
            lines(own.x(), own.wide()).map(|l| l + i64::from(dx)),
            i64::from(screen.wide() / 2),
            targets.iter().flat_map(across),
            threshold,
        );
        let y = nearest(
            lines(own.y(), own.tall()).map(|l| l + i64::from(dy)),
            i64::from(screen.tall() / 2),
            targets.iter().flat_map(down),
            threshold,
        );
        for (found, delta, axis) in [
            (x, &mut snap.dx, GuideAxis::Vertical),
            (y, &mut snap.dy, GuideAxis::Horizontal),
        ] {
            if let Some((shift, at)) = found {
                *delta = saturate(i64::from(*delta) + shift);
                snap.guides.push(Guide {
                    axis,
                    at: saturate(at),
                });
            }
        }
        snap
    }
}

/// A box's start, centre and end along one axis.
fn lines(start: i32, len: i32) -> [i64; 3] {
    let (start, len) = (i64::from(start), i64::from(len));
    [start, start + len / 2, start + len]
}

/// The shift that brings the nearest moving line onto a target line within `threshold`, and
/// that target line. The screen centre comes first; ties keep the earlier target, then the
/// earlier moving line.
fn nearest(
    moving: [i64; 3],
    centre: i64,
    targets: impl Iterator<Item = i64>,
    threshold: i32,
) -> Option<(i64, i64)> {
    let mut best: Option<(i64, i64)> = None;
    for target in std::iter::once(centre).chain(targets) {
        for line in moving {
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
