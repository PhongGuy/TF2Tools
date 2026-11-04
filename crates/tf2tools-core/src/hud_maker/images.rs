//! HUD images: an `image` value resolved through its `.vmt` material to a decoded texture,
//! or the reason it cannot be shown.

use crate::error::HudFilesError;
use crate::fonts::Platform;
use crate::fonts::keyvalues::{self, KvValue};
use crate::texture::{RgbaImage, decode_vtf, vtf_size};

use super::files::{HudFiles, normalise_game_path};

/// The most pixels a texture may have before it is refused undecoded (4096 x 4096).
const MAX_PIXELS: u64 = 16_777_216;

/// What looking up a HUD image found.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum HudImage {
    /// The texture decoded.
    Found(HudTexture),
    /// The material or texture file is absent; `path` is the lowercase key looked for, or the
    /// name as given when it is empty or leaves `materials/`.
    Missing {
        /// The key looked for, or the name as given.
        path: String,
    },
    /// The file exists but cannot be shown.
    Unreadable {
        /// The lowercase key of the material or texture at fault.
        path: String,
        /// Why it cannot be shown.
        problem: ImageProblem,
    },
}

/// Why an existing material or texture cannot be shown.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ImageProblem {
    /// The material names no `$basetexture`.
    NoBaseTexture,
    /// The material is a `patch` of another material, which is not followed.
    Patch,
    /// The texture has more pixels than a preview decodes.
    TooLarge {
        /// Width in pixels from the header.
        width: u32,
        /// Height in pixels from the header.
        height: u32,
    },
    /// The texture does not decode; the text is for the developer log.
    Decode(String),
}

/// A decoded HUD texture and the material settings that change how it is drawn.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct HudTexture {
    image: RgbaImage,
    material: String,
    texture: String,
    scale: (f32, f32),
    distance_field: bool,
}

impl HudTexture {
    /// Frame 0 of the texture as RGBA pixels.
    pub fn image(&self) -> &RgbaImage {
        &self.image
    }

    /// The lowercase key of the `.vmt` material.
    pub fn material(&self) -> &str {
        &self.material
    }

    /// The lowercase key of the `.vtf` texture.
    pub fn texture(&self) -> &str {
        &self.texture
    }

    /// `$basetexturetransform`'s `scale x y`, `(1.0, 1.0)` when absent; negative mirrors.
    pub fn scale(&self) -> (f32, f32) {
        self.scale
    }

    /// `$distancealpha` is set nonzero, so the preview cannot match the game exactly.
    pub fn distance_field(&self) -> bool {
        self.distance_field
    }
}

impl HudFiles {
    /// An `image` value (relative to `materials/vgui/`, `.vmt` optional) through its material.
    ///
    /// Absent files are `Missing` and undrawable ones `Unreadable`; only a failure to read a
    /// file that exists is an error.
    pub fn image(&self, name: &str, platform: Platform) -> Result<HudImage, HudFilesError> {
        let Some(material) = game_key("materials/vgui", name, ".vmt") else {
            return Ok(missing(name));
        };
        let Some(file) = self.read(&material)? else {
            return Ok(missing(&material));
        };
        let unreadable = |path: &str, problem| {
            Ok(HudImage::Unreadable {
                path: path.to_owned(),
                problem,
            })
        };
        // A material is its first top-level pair: the shader name and its block of settings.
        let parsed = keyvalues::parse(&String::from_utf8_lossy(file.bytes()), platform);
        let settings = match parsed.pairs.first() {
            Some(pair) if pair.key.eq_ignore_ascii_case("patch") => {
                return unreadable(&material, ImageProblem::Patch);
            }
            Some(keyvalues::KvPair {
                value: KvValue::Block(children),
                ..
            }) => children.as_slice(),
            _ => &[],
        };
        let setting = |key: &str| {
            settings.iter().find_map(|pair| match &pair.value {
                KvValue::Text(text) if pair.key.eq_ignore_ascii_case(key) => Some(text.as_str()),
                _ => None,
            })
        };
        let Some(base) = setting("$basetexture") else {
            return unreadable(&material, ImageProblem::NoBaseTexture);
        };
        let Some(texture) = game_key("materials", base, ".vtf") else {
            return Ok(missing(base));
        };
        let Some(file) = self.read(&texture)? else {
            return Ok(missing(&texture));
        };
        // The header is checked first: the vtf crate panics on very large textures.
        if let Some((width, height)) = vtf_size(file.bytes())
            && u64::from(width) * u64::from(height) > MAX_PIXELS
        {
            return unreadable(&texture, ImageProblem::TooLarge { width, height });
        }
        let image = match decode_vtf(file.bytes()) {
            Ok(image) => image,
            Err(error) => return unreadable(&texture, ImageProblem::Decode(error.to_string())),
        };
        let distance_field = setting("$distancealpha")
            .and_then(|v| v.trim().parse::<f32>().ok())
            .is_some_and(|v| v != 0.0);
        Ok(HudImage::Found(HudTexture {
            image,
            material,
            texture,
            scale: setting("$basetexturetransform").map_or((1.0, 1.0), transform_scale),
            distance_field,
        }))
    }
}

fn missing(path: &str) -> HudImage {
    HudImage::Missing {
        path: path.to_owned(),
    }
}

/// The lowercase key of `name` under `dir` with `ext`, which `name` may already carry in any
/// case; `None` when `name` names no file or the path leaves `materials/`, where the engine's
/// material system looks.
pub(super) fn game_key(dir: &str, name: &str, ext: &str) -> Option<String> {
    let stem = match name.len().checked_sub(ext.len()) {
        Some(at) if name.is_char_boundary(at) && name[at..].eq_ignore_ascii_case(ext) => {
            &name[..at]
        }
        _ => name,
    };
    if stem.split(['/', '\\']).all(|part| matches!(part, "" | ".")) {
        return None;
    }
    let path = normalise_game_path(&format!("{dir}/{stem}"))?.to_ascii_lowercase();
    path.starts_with("materials/")
        .then(|| format!("{path}{ext}"))
}

/// The `scale x y` of a transform such as `center .5 .5 scale -1 1 rotate 0 translate 0 0`,
/// `(1.0, 1.0)` when it has none that parses.
fn transform_scale(transform: &str) -> (f32, f32) {
    let mut words = transform.split_ascii_whitespace();
    while let Some(word) = words.next() {
        if word.eq_ignore_ascii_case("scale") {
            let mut number = || words.next().and_then(|w| w.parse::<f32>().ok());
            if let (Some(x), Some(y)) = (number(), number()) {
                return (x, y);
            }
            break;
        }
    }
    (1.0, 1.0)
}
