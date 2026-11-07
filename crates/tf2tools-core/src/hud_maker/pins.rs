//! Attaching a part to a sibling and detaching it, keeping its place on the reference screen.

use super::borrow::{READ_AT, px_to_units};
use super::edits::{code_owns, geometry_ok, set_lines};
use super::probe;
use super::{Change, EditRefused, HudModel, KeyLine, Part, PartKey, PinCorner, Point};

/// Why a pin or unpin made no changes; checked in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PinRefused {
    /// No part has one of the keys.
    UnknownPart,
    /// The target does not share the part's parent.
    NotSibling,
    /// The target is the part, or its pins already lead back to the part.
    Cycle,
    /// The part's files did not load or its place is not well formed.
    ReadOnly,
    /// The game's code owns the part's place.
    NotOffered,
}

const PIN_KEYS: [&str; 3] = [
    "pin_to_sibling",
    "pin_corner_to_sibling",
    "pin_to_sibling_corner",
];

impl HudModel {
    /// Pins `part` to sibling `to`, writing the three pin keys and xpos/ypos as plain units so
    /// the part stays put at 1920x1080. Refusals in the enum's order.
    pub fn pin_part(
        &self,
        part: &PartKey,
        to: &PartKey,
        own: PinCorner,
        corner: PinCorner,
        group: u32,
    ) -> Result<Vec<Change>, PinRefused> {
        let (Some(p), Some(target)) = (self.part(part), self.part(to)) else {
            return Err(PinRefused::UnknownPart);
        };
        if p.parent() != target.parent() {
            return Err(PinRefused::NotSibling);
        }
        // The file names the target, so the engine pins to the first sibling of that name.
        let target = self.sibling_named(p, target.name()).unwrap_or(target);
        if self.pin_reaches(target, p) {
            return Err(PinRefused::Cycle);
        }
        pinnable(p)?;
        // Pin with zero offsets, then measure how far that moved the part: the gap, with the
        // engine's pin sign undone, is the offset that puts it back.
        let before = self.place_at(part).ok_or(PinRefused::UnknownPart)?;
        let trial = pin_lines(p, target.name(), own, corner, Point::default(), group);
        let replay = [self.replayed(), &trial].concat();
        let pinned = self.with_changes(&replay).into_model();
        let after = pinned.place_at(part).ok_or(PinRefused::UnknownPart)?;
        let (sign_x, sign_y) = probe::pin_offset_sign(own, corner);
        let units = |px: i32, sign: i32| px_to_units(px, READ_AT.tall()).saturating_mul(sign);
        let offset = Point::new(
            units(before.x.saturating_sub(after.x), sign_x),
            units(before.y.saturating_sub(after.y), sign_y),
        );
        Ok(pin_lines(p, target.name(), own, corner, offset, group))
    }

    /// Removes the pin keys and writes the resolved place as plain units; no changes when
    /// unpinned.
    pub fn unpin_part(&self, part: &PartKey, group: u32) -> Result<Vec<Change>, PinRefused> {
        let p = self.part(part).ok_or(PinRefused::UnknownPart)?;
        if PIN_KEYS.iter().all(|k| p.value(k).is_none()) {
            return Ok(Vec::new());
        }
        pinnable(p)?;
        let at = self.place_at(part).ok_or(PinRefused::UnknownPart)?;
        let mut changes = Vec::new();
        for key in PIN_KEYS {
            changes.extend(self.remove_key(part, key, group).map_err(refused)?);
        }
        for (key, px) in [("xpos", at.x), ("ypos", at.y)] {
            let text = px_to_units(px, READ_AT.tall()).to_string();
            changes.extend(set_lines(p, key, group, |_| text.clone()));
        }
        Ok(changes)
    }

    /// Siblings `part` can pin to: not itself, not one whose pin chain reaches `part`.
    pub fn pin_targets(&self, part: &PartKey) -> Vec<PartKey> {
        let Some(p) = self.part(part).filter(|p| pinnable(p).is_ok()) else {
            return Vec::new();
        };
        (self.parts().iter())
            .filter(|s| s.parent() == p.parent() && s.key() != p.key())
            // A later sibling sharing a name can never be the one a pin by name reaches.
            .filter(|s| {
                self.sibling_named(p, s.name())
                    .is_some_and(|f| f.key() == s.key())
            })
            .filter(|s| !self.pin_reaches(s, p))
            .map(|s| s.key().clone())
            .collect()
    }

    /// The first part under `part`'s parent named `name`, ignoring ASCII case, as layout
    /// resolves a pin.
    pub(super) fn sibling_named(&self, part: &Part, name: &str) -> Option<&Part> {
        (self.parts().iter())
            .find(|s| s.parent() == part.parent() && s.name().eq_ignore_ascii_case(name))
    }

    /// Whether following pins from `from` arrives at `goal`, `from` itself included; the walk
    /// is bounded by the part count, so a stale cycle ends it.
    fn pin_reaches(&self, from: &Part, goal: &Part) -> bool {
        let mut at = from;
        for _ in 0..=self.parts().len() {
            if at.key() == goal.key() {
                return true;
            }
            let next = (at.value("pin_to_sibling").map(KeyLine::text))
                .and_then(|name| self.sibling_named(at, name));
            match next {
                Some(next) => at = next,
                None => return false,
            }
        }
        false
    }

    /// The part's place inside its parent at the reference screen, in pixels.
    fn place_at(&self, part: &PartKey) -> Option<Point> {
        let layout = self.layout(READ_AT);
        let here = layout.get(part)?;
        let up = (self.part(part)?.parent()).and_then(|k| layout.get(k));
        Some(match up {
            Some(up) => Point::new(
                here.x().saturating_sub(up.x()),
                here.y().saturating_sub(up.y()),
            ),
            None => Point::new(here.x(), here.y()),
        })
    }
}

/// Refuses a part whose place cannot be rewritten: unreadable or malformed, then code-owned.
fn pinnable(part: &Part) -> Result<(), PinRefused> {
    if !geometry_ok(part) {
        return Err(PinRefused::ReadOnly);
    }
    if code_owns(part, "xpos") || code_owns(part, "ypos") {
        return Err(PinRefused::NotOffered);
    }
    Ok(())
}

/// The three pin keys, corners by name, and the offsets as plain units.
fn pin_lines(
    part: &Part,
    to: &str,
    own: PinCorner,
    corner: PinCorner,
    Point { x, y }: Point,
    group: u32,
) -> Vec<Change> {
    let (x, y) = (x.to_string(), y.to_string());
    [
        ("pin_to_sibling", to),
        ("pin_corner_to_sibling", own.key_text()),
        ("pin_to_sibling_corner", corner.key_text()),
        ("xpos", &x),
        ("ypos", &y),
    ]
    .into_iter()
    .flat_map(|(key, text)| set_lines(part, key, group, |_| text.to_owned()))
    .collect()
}

fn refused(e: EditRefused) -> PinRefused {
    match e {
        EditRefused::UnknownPart => PinRefused::UnknownPart,
        EditRefused::ReadOnly => PinRefused::ReadOnly,
        EditRefused::NotOffered | EditRefused::BadValue | EditRefused::RunsGameCommands => {
            PinRefused::NotOffered
        }
    }
}
