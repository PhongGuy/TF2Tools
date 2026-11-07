//! Images tinted by the colour the game draws them in, made once per texture and colour.

use std::cell::RefCell;
use std::sync::Arc;

use gpui_kit::RenderImage;
use tf2tools_core::{Rgba, Vision, tint_pixels};

use crate::previews::to_render_image;

/// How many tinted textures stay ready; dragging a colour makes one per step.
const KEEP: usize = 32;

/// Tinted textures by source texture and colour. A source is held while its tints are, so
/// pointer identity cannot be reused under it. Textures pushed out wait in `stale` until
/// the page drops them from the window's atlas.
#[derive(Default)]
pub(super) struct Tints {
    made: Vec<(Arc<RenderImage>, Rgba, Arc<RenderImage>)>,
    stale: Vec<Arc<RenderImage>>,
}

impl Tints {
    /// `source` tinted by `colour`, made on first use; `None` when its pixels are unreadable.
    pub(super) fn get(
        &mut self,
        source: &Arc<RenderImage>,
        colour: Rgba,
    ) -> Option<Arc<RenderImage>> {
        let found = (self.made.iter()).find(|(s, c, _)| Arc::ptr_eq(s, source) && *c == colour);
        if let Some((_, _, tinted)) = found {
            return Some(Arc::clone(tinted));
        }
        let tinted = tinted(source, colour)?;
        if self.made.len() >= KEEP {
            let (_, _, old) = self.made.remove(0);
            self.stale.push(old);
        }
        self.made
            .push((Arc::clone(source), colour, Arc::clone(&tinted)));
        Some(tinted)
    }

    /// The tints pushed out since the last call, for the atlas to drop.
    pub(super) fn take_stale(&mut self) -> Vec<Arc<RenderImage>> {
        std::mem::take(&mut self.stale)
    }
}

/// `image` as drawn in `colour` seen through `vision`: tinted from `tints`, or itself when
/// it draws plain white or its pixels are unreadable.
pub(super) fn drawn(
    tints: &RefCell<Tints>,
    image: &Arc<RenderImage>,
    colour: Option<Rgba>,
    vision: Vision,
) -> Arc<RenderImage> {
    let Some(colour) = colour else {
        return Arc::clone(image);
    };
    (tints.borrow_mut().get(image, vision.simulate(colour))).unwrap_or_else(|| Arc::clone(image))
}

// A BGRA texture's first frame multiplied by `colour`, rebuilt as a new texture.
fn tinted(image: &RenderImage, colour: Rgba) -> Option<Arc<RenderImage>> {
    let size = image.size(0);
    let (wide, tall) = (
        u32::try_from(size.width.0).ok()?,
        u32::try_from(size.height.0).ok()?,
    );
    let mut rgba = image.as_bytes(0)?.to_vec();
    for px in rgba.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
    }
    tint_pixels(&mut rgba, colour);
    to_render_image(wide, tall, rgba)
}
