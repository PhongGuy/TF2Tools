//! Image bytes to a drawable `RenderImage`, downsized into its target box. Pure and
//! blocking, so callers run it on the background executor.

use std::sync::Arc;

use gpui_kit::RenderImage;
use image::imageops::FilterType;

use super::ImageTarget;
use crate::previews::to_render_image;

/// How hard a mature card's thumbnail is blurred until the player reveals it.
const MATURE_BLUR_SIGMA: f32 = 16.0;

/// Decodes PNG, JPEG or WebP bytes, shrinking (never enlarging) the image to fit
/// `target`'s box with its aspect ratio kept. None when the bytes are no image.
pub fn decode_image(bytes: &[u8], target: ImageTarget) -> Option<Arc<RenderImage>> {
    let rgba = decode_rgba(bytes, target)?;
    let (width, height) = rgba.dimensions();
    to_render_image(width, height, rgba.into_raw())
}

/// `decode_image`'s pixels before the upload: downsized into `target`'s box, blurred for
/// `CardBlurred`. None when the bytes are no image.
pub fn decode_rgba(bytes: &[u8], target: ImageTarget) -> Option<image::RgbaImage> {
    let mut image = image::load_from_memory(bytes).ok()?;
    let (w, h) = target.bounds();
    if image.width() > w || image.height() > h {
        image = image.resize(w, h, FilterType::Triangle);
    }
    // Blurred after the resize, so the cost and the softness are those of the card box.
    if target == ImageTarget::CardBlurred {
        image = image.fast_blur(MATURE_BLUR_SIGMA);
    }
    Some(image.to_rgba8())
}
