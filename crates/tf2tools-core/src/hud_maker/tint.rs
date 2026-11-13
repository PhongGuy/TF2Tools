//! The colour the game draws an image in, and tinting its pixels by it.

use super::{GameSituation, HudModel, Part, Rgba};

/// Multiplies each RGBA pixel in `pixels` channel by channel by `by`, alpha included, as the
/// engine modulates a texture by its draw colour. A trailing partial pixel is left alone.
pub fn tint_pixels(pixels: &mut [u8], by: Rgba) {
    let by = by.to_array();
    for pixel in pixels.as_chunks_mut::<4>().0 {
        for (channel, &c) in pixel.iter_mut().zip(&by) {
            // Rounded, so a white texel takes the colour exactly.
            *channel = ((u16::from(*channel) * u16::from(c) + 127) / 255) as u8;
        }
    }
}

impl HudModel {
    /// The colour the game draws image `part` in during situation `s`, or `None` for plain
    /// white: the health pulse image's colour from the health element, the health cross's
    /// death warning colour while dying, else the part's own `drawcolor`.
    pub fn draw_colour(&self, part: &Part, s: &GameSituation) -> Option<Rgba> {
        // The pulse colour is white unless dying, when the game tints the cross the same way.
        let warned = (GameSituation::is_health_image(part))
            .then(|| s.pulse_colour(self))
            .filter(|c| *c != Rgba::WHITE);
        let colour = match (GameSituation::is_pulse_image(part), warned) {
            (true, _) => s.pulse_colour(self),
            (false, Some(c)) => c,
            (false, None) => self.scheme().colour(part.value("drawcolor")?.text().trim()),
        };
        (colour != Rgba::WHITE).then_some(colour)
    }

    /// The share of image `part`'s height the game draws in situation `s`, from the bottom:
    /// the health cross's health over max, `None` for an image drawn whole.
    pub fn image_fill(&self, part: &Part, s: &GameSituation) -> Option<f32> {
        GameSituation::is_health_image(part).then(|| s.health_share())
    }

    /// The share of progress bar `part` the game fills in situation `s`, from the left: the
    /// Medic's charge on the charge meter, `None` for any other part.
    pub fn bar_fill(&self, part: &Part, s: &GameSituation) -> Option<f32> {
        GameSituation::is_charge_meter(part).then(|| f32::from(s.charge()) / 100.)
    }
}
