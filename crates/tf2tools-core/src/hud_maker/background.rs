//! How a panel's background draws: whether it paints, and which corners round and by how much,
//! read as the engine's `Panel` reads its keys.

use super::expr::atoi;
use super::{HudModel, ImageBorder, Part, PartId, Rgba, Screen};

/// The background type that draws a box with rounded corners.
const ROUNDED: i32 = 2;
/// Every corner: top-left 1, top-right 2, bottom-left 4, bottom-right 8.
const ALL_CORNERS: i32 = 15;

/// Whether `part` paints its background: unless `paintbackground` reads as 0.
pub fn paints_background(part: &Part) -> bool {
    part.value("paintbackground")
        .is_none_or(|line| atoi(line.text().as_bytes()) != 0)
}

/// The rounded corners' mask when `part` paints a rounded background (`PaintBackgroundType`
/// 2): `RoundedCorners` (default 15) kept to its four bits; `None` for any other background.
pub fn background_corners(part: &Part) -> Option<u8> {
    let number = |key: &str| part.value(key).map(|line| atoi(line.text().as_bytes()));
    if !paints_background(part) || number("PaintBackgroundType") != Some(ROUNDED) {
        return None;
    }
    // The mask is four bits, so the cast cannot truncate.
    Some((number("RoundedCorners").unwrap_or(ALL_CORNERS) & ALL_CORNERS) as u8)
}

/// The engine's corner size in screen pixels: half the proportional 8, at least 8.
pub fn corner_radius(screen: Screen) -> i32 {
    let tall = i64::from(screen.tall());
    i32::try_from(tall * 8 / 480 / 2).map_or(i32::MAX, |r| r.max(8))
}

impl HudModel {
    /// The colour `part` paints behind its content: `bgcolor_override`, else `fillcolor`, else
    /// what the scene's game code fills it with. `None` when blank, a name the scheme lacks,
    /// or fully clear. `paints_background` is the caller's to check.
    pub fn fill_colour(&self, part: &Part) -> Option<Rgba> {
        // The game repaints some parts after their file loads, so its fill beats any colour key.
        // The meter's code fills its bars in white each update; the preview shows them full.
        if is_meter_bar(part) {
            return Some(Rgba::WHITE);
        }
        let sample = self.scene_rule().sample();
        if sample.code_fill_wins(part.name()) {
            return sample.code_fill(part.name());
        }
        let Some(line) = ["bgcolor_override", "fillcolor"]
            .iter()
            .find_map(|k| part.value(k))
        else {
            // No colour key: the game's code may still paint the part, as the class menu's room.
            return sample.code_fill(part.name());
        };
        let value = line.text().trim();
        let literal = value.starts_with(|c: char| c.is_ascii_digit() || c == '.');
        let colour = match literal {
            true => Some(self.scheme().colour(value)),
            false if value.is_empty() => None,
            false => self.scheme().named_colour(value),
        };
        colour.filter(|c| c.to_array()[3] > 0)
    }

    /// The scheme border `part` draws when that border is a scalable image.
    pub fn picture_border(&self, part: &Part) -> Option<&ImageBorder> {
        // The game's code sets some borders itself, over what the file's border key names.
        let sample = self.scene_rule().sample();
        let name = match sample.code_border(part.name()) {
            Some(name) => name,
            None => part.value("border")?.text().trim(),
        };
        self.scheme().image_border(name)
    }
}

/// Whether `part` is one of the Item meter's progress bars the game's code fills: a
/// `ContinuousProgressBar` named `ItemEffectMeter`, `ItemEffectMeter2` and so on.
fn is_meter_bar(part: &Part) -> bool {
    let numbered = |name: &str| {
        let lower = name.to_ascii_lowercase();
        lower.strip_prefix("itemeffectmeter").is_some_and(|n| n.bytes().all(|b| b.is_ascii_digit()))
    };
    part.owner() == Some(PartId::ItemMeter)
        && !part.is_root()
        && part.class().is_some_and(|c| c.eq_ignore_ascii_case("ContinuousProgressBar"))
        && numbered(part.name())
}
