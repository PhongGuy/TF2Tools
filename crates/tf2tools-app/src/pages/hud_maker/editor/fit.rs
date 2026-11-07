//! A part that pokes out of its parent's box, which the game cuts off at the parent's edges,
//! and growing that parent just enough to hold it again.

use tf2tools_core::{Change, EditRefused, HudLayout, HudModel, PartBox, PartKey};

use super::{HudEditor, Units, next_group};
use crate::pages::hud_maker::parts::row_label;

// Each pass grows by whole file units, rounded up, and puts the parts back; a second pass
// settles a pixel the rounding left. The cap only guards against a box that will not grow.
const PASSES: usize = 4;

type Edges = (i64, i64, i64, i64);

fn edges(b: &PartBox) -> Edges {
    let (x, y) = (i64::from(b.x()), i64::from(b.y()));
    (x, y, x + i64::from(b.wide()), y + i64::from(b.tall()))
}

fn inside(inner: Edges, outer: Edges) -> bool {
    inner.0 >= outer.0 && inner.1 >= outer.1 && inner.2 <= outer.2 && inner.3 <= outer.3
}

/// The parent whose box `key`'s box pokes out of, so the game cuts that much of it off; `None`
/// when it fits, has no size, or the parent spans the whole screen, where growing shows no more.
pub fn outside_parent<'a>(
    model: &'a HudModel,
    layout: &HudLayout,
    key: &PartKey,
) -> Option<&'a PartKey> {
    let parent = model.part(key)?.parent()?;
    let own = layout.get(key).filter(|b| b.wide() > 0 && b.tall() > 0)?;
    let around = edges(layout.get(parent)?);
    let screen = layout.screen();
    let whole = (0, 0, i64::from(screen.wide()), i64::from(screen.tall()));
    (!inside(edges(own), around) && !inside(whole, around)).then_some(parent)
}

impl HudEditor {
    /// The selection's parent when the selection pokes out of it; see `outside_parent`.
    pub fn selection_outside_parent(&self) -> Option<&PartKey> {
        outside_parent(&self.model, &self.layout, self.selected.as_ref()?)
    }

    /// Grows the parent the selection pokes out of just enough to hold it, as one undo step
    /// named "grow <parent> to fit". The parent's other parts keep their place on screen.
    /// `NotOffered` when the selection fits; a parent that cannot move or resize refuses.
    pub fn grow_parent_to_fit(&mut self) -> Result<(), EditRefused> {
        let child = self.selected.clone().ok_or(EditRefused::UnknownPart)?;
        let parent = (self.selection_outside_parent().cloned()).ok_or(EditRefused::NotOffered)?;
        self.cancel_preview();
        self.end_drag();
        let units = Units::new(self.screen);
        let group = next_group(&self.changes);
        let mut next = self.changes.clone();
        // Every pass puts the parent's parts back where they sat before the first, so rounding
        // never walks them a pixel further each pass.
        let origin = self.layout.clone();
        let (mut model, mut layout) = (self.model.clone(), self.layout.clone());
        for _ in 0..PASSES {
            let (Some(p), Some(c)) = (layout.get(&parent), layout.get(&child)) else {
                return Err(EditRefused::UnknownPart);
            };
            let (pe, ce) = (edges(p), edges(c));
            if inside(ce, pe) {
                break;
            }
            // How far each edge must travel outward, as whole units so the move never rounds short.
            let out = |by: i64| units.px(units.of(i32::try_from(by.max(0)).unwrap_or(i32::MAX)));
            let (left, top) = (out(pe.0 - ce.0), out(pe.1 - ce.1));
            let (right, bottom) = (out(ce.2 - pe.2), out(ce.3 - pe.3));
            let wide = (left > 0 || right > 0).then(|| p.wide().saturating_add(left + right));
            let tall = (top > 0 || bottom > 0).then(|| p.tall().saturating_add(top + bottom));
            // Resize first, then move the parent's corner to where the grown box starts: a
            // parent placed from the screen's centre or far edge shifts as it resizes.
            next.extend(model.resize_part(&parent, wide, tall, self.screen, group)?);
            (model, layout) = self.laid_out(&next);
            let now = layout.get(&parent).ok_or(EditRefused::UnknownPart)?;
            let start = |was: i64, by: i32| i32::try_from(was).unwrap_or(0) - by;
            let (dx, dy) = (start(pe.0, left) - now.x(), start(pe.1, top) - now.y());
            if dx != 0 || dy != 0 {
                next.extend(model.move_part(&parent, dx, dy, self.screen, group)?);
                (model, layout) = self.laid_out(&next);
            }
            // Moving or resizing the parent carries its parts along, and shifts those placed
            // from its centre or far edge, so each goes back to where it sat.
            let mut back = Vec::new();
            for part in (model.parts().iter()).filter(|p| p.parent() == Some(&parent)) {
                let (Some(was), Some(now)) = (origin.get(part.key()), layout.get(part.key()))
                else {
                    continue;
                };
                let (dx, dy) = (was.x() - now.x(), was.y() - now.y());
                // Back by the nearest whole units, not the truncated ones a pointer move takes,
                // so each part lands within a pixel of where it sat.
                let (ux, uy) = (units.nearest(dx), units.nearest(dy));
                if ux != 0 || uy != 0 {
                    // A part whose place the game sets stays where the game puts it.
                    back.extend(model.nudge_part(part.key(), ux, uy, group).ok());
                }
            }
            if !back.is_empty() {
                next.extend(back.into_iter().flatten());
                (model, layout) = self.laid_out(&next);
            }
        }
        if next.len() == self.changes.len() {
            return Ok(());
        }
        let name = (self.model.part(&parent)).map(|p| format!("grow {} to fit", row_label(p)));
        self.commit_named(next, name);
        Ok(())
    }

    fn laid_out(&self, changes: &[Change]) -> (HudModel, HudLayout) {
        let model = self.pristine.with_changes(changes).into_model();
        let layout = model.layout_in(self.screen, &self.situation);
        (model, layout)
    }
}
