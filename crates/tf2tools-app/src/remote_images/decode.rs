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
/// `CardBlurred` and with a weapon icon's white ground cleared. None when the bytes are no image.
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
    let mut rgba = image.to_rgba8();
    if target == ImageTarget::WeaponIcon {
        clear_white_backdrop(&mut rgba);
    }
    Some(rgba)
}

/// Makes an opaque near-white ground transparent: only when all four corners are opaque with
/// every channel >= 240, it clears every such pixel connected to a corner to `[0, 0, 0, 0]`.
pub fn clear_white_backdrop(image: &mut image::RgbaImage) {
    let ground = |p: &image::Rgba<u8>| p.0[3] == 255 && p.0[..3].iter().all(|&c| c >= 240);
    let (w, h) = image.dimensions();
    if w == 0 || h == 0 {
        return;
    }
    let corners = [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)];
    if !corners.iter().all(|&(x, y)| ground(image.get_pixel(x, y))) {
        return;
    }
    // Stack-based 4-connected fill from the corners; a cleared pixel no longer matches
    // `ground`, so clearing doubles as the visited mark.
    let mut stack = corners.to_vec();
    while let Some((x, y)) = stack.pop() {
        if !ground(image.get_pixel(x, y)) {
            continue;
        }
        image.put_pixel(x, y, image::Rgba([0, 0, 0, 0]));
        if x > 0 {
            stack.push((x - 1, y));
        }
        if x + 1 < w {
            stack.push((x + 1, y));
        }
        if y > 0 {
            stack.push((x, y - 1));
        }
        if y + 1 < h {
            stack.push((x, y + 1));
        }
    }
}
