//! HUD texture helpers shared by the HUD Maker and the logs.tf kill icons: a game texture
//! decoded to pixels, and an icon's region cut out of its sheet.

use std::sync::Arc;

use gpui_kit::RenderImage;
use tf2tools_core::{HudFiles, HudImage, KillIcon, Platform, RgbaImage};

use crate::previews::to_render_image;

/// A texture decoded, with the pixels kept for cutting icons out of a sheet.
pub(crate) type Decoded = Option<(Arc<RgbaImage>, Arc<RenderImage>)>;

/// `name`'s texture as the game draws it, faded by its material; `None` when it is missing or
/// unreadable.
pub(crate) fn texture_pixels(
    files: &HudFiles,
    name: &str,
    platform: Platform,
) -> Option<Arc<RgbaImage>> {
    let Ok(HudImage::Found(texture)) = files.image(name, platform) else {
        return None;
    };
    Some(Arc::new(texture.drawn()))
}

/// `name`'s texture as pixels and as a render image; `None` when it is missing or unreadable.
pub(crate) fn decode_texture(files: &HudFiles, name: &str, platform: Platform) -> Decoded {
    let image = texture_pixels(files, name, platform)?;
    let render = to_render_image(image.width(), image.height(), image.pixels().to_vec())?;
    Some((image, render))
}

/// The icon's texel rectangle out of its sheet; `None` when the rectangle leaves the sheet.
pub(crate) fn crop(sheet: &RgbaImage, icon: &KillIcon) -> Option<RgbaImage> {
    let right = icon.x().checked_add(icon.wide())?;
    let bottom = icon.y().checked_add(icon.tall())?;
    if right > sheet.width() || bottom > sheet.height() {
        return None;
    }
    let (stride, row_bytes) = (sheet.width() as usize * 4, icon.wide() as usize * 4);
    let mut pixels = Vec::with_capacity(row_bytes * icon.tall() as usize);
    for row in icon.y()..bottom {
        let start = row as usize * stride + icon.x() as usize * 4;
        pixels.extend_from_slice(sheet.pixels().get(start..start + row_bytes)?);
    }
    RgbaImage::from_pixels(icon.wide(), icon.tall(), pixels)
}
