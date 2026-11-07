//! The merged client scheme as the engine reads it: named colours with `BaseSettings` aliases,
//! font variants chosen by screen height, and the custom font files a HUD ships.

use crate::error::HudFilesError;
use crate::fonts::Platform;
use crate::fonts::keyvalues::{KvPair, KvValue};

use super::Size;
use super::document::KvDocument;
use super::files::{HudFiles, KeptFiles};

/// How many `BaseSettings` aliases one colour lookup follows before giving up.
const MAX_ALIAS_HOPS: usize = 8;

/// An 8-bit RGBA colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Rgba {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

impl Rgba {
    /// Opaque white, the engine's colour for an unresolved name.
    pub const WHITE: Rgba = Rgba::new(255, 255, 255, 255);

    /// A colour from its four channels.
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Rgba {
        Rgba { r, g, b, a }
    }

    /// The channels as `[r, g, b, a]`.
    pub fn to_array(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

/// One variant of a scheme font: the settings used when the screen height is in its `yres`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct FontVariant {
    family: String,
    tall: i32,
    weight: i32,
    yres: Option<(u32, u32)>,
    antialias: bool,
    additive: bool,
    dropshadow: bool,
    outline: bool,
    italic: bool,
    blur: i32,
}

impl FontVariant {
    /// The font family (`name`).
    pub fn family(&self) -> &str {
        &self.family
    }

    /// The height in scheme units.
    pub fn tall(&self) -> i32 {
        self.tall
    }

    /// The height in screen pixels on a screen `screen_tall` high: a variant limited by
    /// `yres` keeps its `tall` as written, any other scales proportionally; held to 0..=127.
    pub fn pixel_tall(&self, screen_tall: u32) -> i32 {
        let px = match self.yres {
            Some(_) => self.tall,
            None => super::font_tall_px(self.tall, screen_tall),
        };
        px.clamp(0, 127)
    }

    /// The weight, 0 when unset.
    pub fn weight(&self) -> i32 {
        self.weight
    }

    /// The inclusive screen-height range this variant applies to, if limited.
    pub fn yres(&self) -> Option<(u32, u32)> {
        self.yres
    }

    /// Whether glyphs are antialiased.
    pub fn antialias(&self) -> bool {
        self.antialias
    }

    /// Whether glyphs draw additively.
    pub fn additive(&self) -> bool {
        self.additive
    }

    /// Whether glyphs have a drop shadow.
    pub fn dropshadow(&self) -> bool {
        self.dropshadow
    }

    /// Whether glyphs are outlined.
    pub fn outline(&self) -> bool {
        self.outline
    }

    /// Whether glyphs are italic.
    pub fn italic(&self) -> bool {
        self.italic
    }

    /// The blur radius, 0 when unset.
    pub fn blur(&self) -> i32 {
        self.blur
    }
}

/// A `Borders` entry drawn as nine pieces of an image: corners keep their size, edges and
/// centre stretch.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ImageBorder {
    image: String,
    src_corner: Size,
    draw_corner: Size,
}

impl ImageBorder {
    /// The image path as the scheme spells it, relative to `materials/vgui/`.
    pub fn image(&self) -> &str {
        &self.image
    }

    /// The corners' size in the texture, in texels.
    pub fn src_corner(&self) -> Size {
        self.src_corner
    }

    /// The corners' size on screen, in proportional units.
    pub fn draw_corner(&self) -> Size {
        self.draw_corner
    }
}

/// A HUD's client scheme with its `#base` chain merged.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Scheme {
    colours: Vec<(String, String)>,
    base_settings: Vec<(String, String)>,
    fonts: Vec<(String, Vec<FontVariant>)>,
    font_files: Vec<String>,
    /// Every `CustomFontFiles` key, the slot numbers new entries are placed above.
    font_slots: Vec<String>,
    borders: Vec<(String, ImageBorder)>,
    /// Each Fonts and Borders entry as a canonical text, so two definitions can be compared.
    font_defs: Vec<(String, String)>,
    border_defs: Vec<(String, String)>,
    kept: KeptFiles,
}

impl Scheme {
    /// `resource/clientscheme.res` merged with its chain; empty when the file is absent.
    pub fn load(files: &HudFiles, platform: Platform) -> Result<Scheme, HudFilesError> {
        Scheme::load_file(files, "resource/clientscheme.res", platform)
    }

    /// `resource/chatscheme.res`, the chat's own scheme, merged with its chain; empty when
    /// the file is absent.
    pub fn load_chat(files: &HudFiles, platform: Platform) -> Result<Scheme, HudFilesError> {
        Scheme::load_file(files, "resource/chatscheme.res", platform)
    }

    fn load_file(
        files: &HudFiles,
        path: &str,
        platform: Platform,
    ) -> Result<Scheme, HudFilesError> {
        let (pairs, kept) = files.load_kv(path, platform)?;
        Ok(Scheme::from_pairs(&pairs, kept))
    }

    /// What `Scheme::load` gives when `resource/clientscheme.res` holds `own`'s bytes and every
    /// `#base` file it names is as it was at load.
    pub fn with_own(&self, own: &KvDocument, platform: Platform) -> Scheme {
        let pairs = self.kept.load(own.as_bytes(), platform);
        Scheme::from_pairs(&pairs, self.kept.clone())
    }

    /// The sections of a merged scheme file, keeping the chain's other files for `with_own`.
    fn from_pairs(pairs: &[KvPair], kept: KeptFiles) -> Scheme {
        let root = pairs
            .iter()
            .find_map(|pair| block_of(pair))
            .unwrap_or_default();
        let section = |name: &str| find_block(root, name).unwrap_or_default();
        Scheme {
            colours: texts(section("Colors")),
            base_settings: texts(section("BaseSettings")),
            fonts: section("Fonts")
                .iter()
                .filter_map(|font| {
                    let variants = block_of(font)?.iter().filter_map(block_of).map(variant);
                    Some((font.key.clone(), variants.collect()))
                })
                .collect(),
            font_files: section("CustomFontFiles")
                .iter()
                .filter_map(|file| match &file.value {
                    KvValue::Text(path) => Some(path.clone()),
                    KvValue::Block(children) => find_text(children, "font").map(str::to_owned),
                })
                .collect(),
            font_slots: (section("CustomFontFiles").iter())
                .map(|file| file.key.clone())
                .collect(),
            borders: section("Borders")
                .iter()
                .filter_map(|border| Some((border.key.clone(), image_border(block_of(border)?)?)))
                .collect(),
            font_defs: definitions(section("Fonts")),
            border_defs: definitions(section("Borders")),
            kept,
        }
    }

    /// The font `name`'s definition as canonical text, for comparing two schemes' fonts.
    pub(super) fn font_def(&self, name: &str) -> Option<&str> {
        find_pair(&self.font_defs, name)
    }

    /// The border `name`'s definition as canonical text, for comparing two schemes' borders.
    pub(super) fn border_def(&self, name: &str) -> Option<&str> {
        find_pair(&self.border_defs, name)
    }

    /// A scheme name as `CScheme::GetColor` resolves it: a literal, a Colors value, or a
    /// BaseSettings alias looked up again; `None` when unresolved.
    pub fn named_colour(&self, name: &str) -> Option<Rgba> {
        let mut name = name;
        for _ in 0..=MAX_ALIAS_HOPS {
            if let Some(colour) = scan_ints(name) {
                return Some(colour);
            }
            if let Some(value) = find_pair(&self.colours, name) {
                return scan_ints(value);
            }
            name = find_pair(&self.base_settings, name)?;
        }
        None
    }

    /// The `BaseSettings` value of `name` as written, with no alias lookup, as
    /// `IScheme::GetResourceString` reads it.
    pub fn resource_string(&self, name: &str) -> Option<&str> {
        find_pair(&self.base_settings, name)
    }

    /// An element's colour value: a literal (starts with a digit or `.`) or a name; white when unresolved.
    pub fn colour(&self, value: &str) -> Rgba {
        if value.starts_with(|c: char| c.is_ascii_digit() || c == '.') {
            return scan_floats(value);
        }
        self.named_colour(value).unwrap_or(Rgba::WHITE)
    }

    /// The `Borders` entry `name` (first match, ignoring case) when it is a scalable image.
    pub fn image_border(&self, name: &str) -> Option<&ImageBorder> {
        self.borders
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, border)| border)
    }

    /// Every Colors name, as first spelled, sorted ignoring case.
    pub fn colour_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.colours.iter().map(|(name, _)| name.as_str()).collect();
        names.sort_by_key(|name| name.to_ascii_lowercase());
        names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        names
    }

    /// Every font name in the Fonts section, as first spelled, sorted ignoring case.
    pub fn font_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.fonts.iter().map(|(name, _)| name.as_str()).collect();
        names.sort_by_key(|name| name.to_ascii_lowercase());
        names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        names
    }

    /// Every font family (`name`) any variant uses, as first spelled, sorted ignoring case.
    pub fn families(&self) -> Vec<&str> {
        let mut names: Vec<&str> = (self.fonts.iter())
            .flat_map(|(_, variants)| variants.iter().map(FontVariant::family))
            .filter(|family| !family.is_empty())
            .collect();
        // A stable sort keeps the first spelling ahead of later ones, which dedup then drops.
        names.sort_by_key(|name| name.to_ascii_lowercase());
        names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        names
    }

    /// The first variant of `name` whose `yres` is absent or contains `screen_tall`.
    pub fn font(&self, name: &str, screen_tall: u32) -> Option<&FontVariant> {
        let (_, variants) = self
            .fonts
            .iter()
            .find(|(font, _)| font.eq_ignore_ascii_case(name))?;
        variants.iter().find(|v| {
            v.yres
                .is_none_or(|(min, max)| (min..=max).contains(&screen_tall))
        })
    }

    /// `CustomFontFiles` paths in order (a bare value, or a block's `font`).
    pub fn font_files(&self) -> &[String] {
        &self.font_files
    }

    /// One above the highest numeric `CustomFontFiles` slot, or 1 when there is none; keys
    /// that are not a `u32` are ignored, and the result saturates at `u32::MAX`.
    pub fn next_font_slot(&self) -> u32 {
        self.max_font_slot().map_or(1, |max| max.saturating_add(1))
    }

    /// The highest numeric `CustomFontFiles` slot.
    pub(super) fn max_font_slot(&self) -> Option<u32> {
        self.font_slots
            .iter()
            .filter_map(|key| key.parse().ok())
            .max()
    }
}

/// One variant block's settings; missing numbers are 0 and flags are set when non-zero.
fn variant(pairs: &[KvPair]) -> FontVariant {
    let int = |key| find_text(pairs, key).map_or(0, atoi);
    let flag = |key| int(key) != 0;
    // `sscanf("%d %d")` needing both numbers; a bound below 0 clamps to 0, which no height is under.
    let yres = find_text(pairs, "yres").and_then(|value| {
        let (min, rest) = scan_int(value)?;
        let (max, _) = scan_int(rest)?;
        let bound = |v: i64| u32::try_from(v.max(0)).unwrap_or(u32::MAX);
        Some((bound(min), bound(max)))
    });
    FontVariant {
        family: find_text(pairs, "name").unwrap_or_default().to_owned(),
        tall: int("tall"),
        weight: int("weight"),
        yres,
        antialias: flag("antialias"),
        additive: flag("additive"),
        dropshadow: flag("dropshadow"),
        outline: flag("outline"),
        italic: flag("italic"),
        blur: int("blur"),
    }
}

fn block_of(pair: &KvPair) -> Option<&[KvPair]> {
    match &pair.value {
        KvValue::Block(children) => Some(children),
        KvValue::Text(_) => None,
    }
}

/// The first block named `name`, ignoring case, as `FindKey` finds it.
fn find_block<'a>(pairs: &'a [KvPair], name: &str) -> Option<&'a [KvPair]> {
    pairs
        .iter()
        .find(|pair| pair.key.eq_ignore_ascii_case(name))
        .and_then(block_of)
}

/// The first text value named `name`, ignoring case.
fn find_text<'a>(pairs: &'a [KvPair], name: &str) -> Option<&'a str> {
    pairs.iter().find_map(|pair| match &pair.value {
        KvValue::Text(text) if pair.key.eq_ignore_ascii_case(name) => Some(text.as_str()),
        KvValue::Text(_) | KvValue::Block(_) => None,
    })
}

fn find_pair<'a>(pairs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

/// A `scalable_image` border with a non-blank image; other border types draw lines the
/// preview does not reproduce.
fn image_border(pairs: &[KvPair]) -> Option<ImageBorder> {
    let scalable = find_text(pairs, "bordertype")?.eq_ignore_ascii_case("scalable_image");
    let image = find_text(pairs, "image").filter(|image| !image.trim().is_empty())?;
    let number = |key: &str| find_text(pairs, key).map_or(0, atoi);
    scalable.then(|| ImageBorder {
        image: image.to_owned(),
        src_corner: Size::new(number("src_corner_width"), number("src_corner_height")),
        draw_corner: Size::new(number("draw_corner_width"), number("draw_corner_height")),
    })
}

/// A section's text pairs in order; nested blocks are not colours or aliases.
fn texts(pairs: &[KvPair]) -> Vec<(String, String)> {
    pairs
        .iter()
        .filter_map(|pair| match &pair.value {
            KvValue::Text(text) => Some((pair.key.clone(), text.clone())),
            KvValue::Block(_) => None,
        })
        .collect()
}

/// Each block entry of a section as (name, canonical text of its content).
fn definitions(section: &[KvPair]) -> Vec<(String, String)> {
    section
        .iter()
        .filter_map(|pair| Some((pair.key.clone(), canon(block_of(pair)?))))
        .collect()
}

/// A block's content as one lowercase text, nested blocks in braces, order kept.
fn canon(pairs: &[KvPair]) -> String {
    let mut out = String::new();
    for pair in pairs {
        out.push_str(&pair.key.to_ascii_lowercase());
        match &pair.value {
            KvValue::Text(text) => {
                out.push('=');
                out.push_str(text.trim().to_ascii_lowercase().as_str());
            }
            KvValue::Block(children) => {
                out.push('{');
                out.push_str(&canon(children));
                out.push('}');
            }
        }
        out.push(';');
    }
    out
}

/// C `atoi`: leading whitespace, a sign and digits, 0 when none; overflow wraps.
fn atoi(text: &str) -> i32 {
    scan_int(text).map_or(0, |(value, _)| value as i32)
}

/// One `%d` conversion: the value and the unread rest, or `None` when no digits follow.
fn scan_int(text: &str) -> Option<(i64, &str)> {
    let text = text.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let (negative, digits) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let len = digits.bytes().take_while(u8::is_ascii_digit).count();
    if len == 0 {
        return None;
    }
    let value = digits[..len].bytes().fold(0i64, |acc, d| {
        acc.wrapping_mul(10).wrapping_add(i64::from(d - b'0'))
    });
    Some((if negative { -value } else { value }, &digits[len..]))
}

/// `sscanf("%d %d %d %d")` needing at least three numbers, alpha 0 when absent; channels wrap.
fn scan_ints(text: &str) -> Option<Rgba> {
    let mut rest = text;
    let mut channels = [0u8; 4];
    let mut read = 0;
    for channel in &mut channels {
        let Some((value, tail)) = scan_int(rest) else {
            break;
        };
        *channel = value as u8;
        rest = tail;
        read += 1;
    }
    (read >= 3).then(|| Rgba::new(channels[0], channels[1], channels[2], channels[3]))
}

/// `sscanf("%f %f %f %f")` as `Panel::ApplySettings` reads a literal: missing parts are 0 and
/// each float truncates to its channel.
fn scan_floats(text: &str) -> Rgba {
    let mut rest = text;
    let mut channels = [0u8; 4];
    for channel in &mut channels {
        rest = rest.trim_start_matches(|c: char| c.is_ascii_whitespace());
        let len = float_prefix(rest);
        let Some(value) = rest[..len].parse::<f32>().ok() else {
            break;
        };
        *channel = value as i32 as u8;
        rest = &rest[len..];
    }
    Rgba::new(channels[0], channels[1], channels[2], channels[3])
}

/// The length of the longest decimal float (`[+-]digits[.digits][e[+-]digits]`) at the start.
fn float_prefix(text: &str) -> usize {
    let bytes = text.as_bytes();
    let digits = |from: usize| {
        from + bytes[from..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count()
    };
    let mut end = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    end = digits(end);
    if bytes.get(end) == Some(&b'.') {
        end = digits(end + 1);
    }
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let sign = usize::from(matches!(bytes.get(end + 1), Some(b'+' | b'-')));
        let exp = digits(end + 1 + sign);
        if exp > end + 1 + sign {
            end = exp;
        }
    }
    end
}
