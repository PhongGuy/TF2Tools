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
    /// white: the health pulse image's colour from the health element, else the part's own
    /// `drawcolor`.
    pub fn draw_colour(&self, part: &Part, s: &GameSituation) -> Option<Rgba> {
        let colour = match GameSituation::is_pulse_image(part) {
            true => s.pulse_colour(self),
            false => self.scheme().colour(part.value("drawcolor")?.text().trim()),
        };
        (colour != Rgba::WHITE).then_some(colour)
    }
}
