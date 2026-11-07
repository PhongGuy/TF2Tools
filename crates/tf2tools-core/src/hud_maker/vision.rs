//! How readable the HUD is: WCAG 2 contrast, colour-blind views (Machado et al. 2009 at full
//! severity) and what text is judged against: a screenshot behind the HUD and the HUD's own
//! backgrounds over it.

use super::background::paints_background;
use super::drawn_image::scene_image;
use super::sample::Team;
use super::{ClassSchema, HudLayout, HudModel, Part, PartKey, PropertyKind, Rect, Rgba, Screen};

/// The WCAG 2 large-text minimum; text under it is "Hard to read here".
pub const MIN_CONTRAST: f64 = 3.0;

/// WCAG 2 relative luminance of `c`'s colour channels (alpha is not read).
pub fn relative_luminance(c: Rgba) -> f64 {
    let [r, g, b, _] = c.to_array();
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

/// WCAG 2 contrast ratio, `(lighter + 0.05) / (darker + 0.05)`, from 1 to 21; alpha is not read.
pub fn contrast_ratio(a: Rgba, b: Rgba) -> f64 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// sRGB decode of one 8-bit channel to 0..1 linear light.
fn linear(c: u8) -> f64 {
    let c = f64::from(c) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// sRGB encode of linear light, clamped to 0..1 and rounded to 8 bits.
fn encode(l: f64) -> u8 {
    let l = l.clamp(0.0, 1.0);
    let c = if l <= 0.003_130_8 {
        12.92 * l
    } else {
        1.055 * l.powf(1.0 / 2.4) - 0.055
    };
    // In 0..=255 after the clamp, so the cast never truncates.
    (c * 255.0).round().clamp(0.0, 255.0) as u8
}

/// How the preview is seen: normal, or one colour-blind view at full severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Vision {
    /// Unaltered colours.
    Normal,
    /// Red-blind.
    Protanopia,
    /// Green-blind.
    Deuteranopia,
    /// Blue-blind.
    Tritanopia,
}

type Matrix = [[f64; 3]; 3];

// Machado, Oliveira and Fernandes 2009, severity 1.0, applied to linear channels.
const PROTANOPIA: Matrix = [
    [0.152286, 1.052583, -0.204868],
    [0.114503, 0.786281, 0.099216],
    [-0.003882, -0.048116, 1.051998],
];
const DEUTERANOPIA: Matrix = [
    [0.367322, 0.860646, -0.227968],
    [0.280085, 0.672501, 0.047413],
    [-0.011820, 0.042940, 0.968881],
];
const TRITANOPIA: Matrix = [
    [1.255528, -0.076749, -0.178779],
    [-0.078411, 0.930809, 0.147602],
    [0.004733, 0.691367, 0.303900],
];

impl Vision {
    /// Every view, Normal first.
    pub const ALL: [Vision; 4] = [
        Vision::Normal,
        Vision::Protanopia,
        Vision::Deuteranopia,
        Vision::Tritanopia,
    ];

    fn matrix(self) -> Option<&'static Matrix> {
        match self {
            Vision::Normal => None,
            Vision::Protanopia => Some(&PROTANOPIA),
            Vision::Deuteranopia => Some(&DEUTERANOPIA),
            Vision::Tritanopia => Some(&TRITANOPIA),
        }
    }

    /// `c` as this view sees it; alpha kept.
    pub fn simulate(self, c: Rgba) -> Rgba {
        let [r, g, b, a] = c.to_array();
        let Some(m) = self.matrix() else {
            return c;
        };
        let rgb = [linear(r), linear(g), linear(b)];
        let row = |n: usize| encode(m[n].iter().zip(rgb).map(|(w, v)| w * v).sum());
        Rgba::new(row(0), row(1), row(2), a)
    }

    /// `simulate` on each RGBA pixel in place; Normal leaves the bytes untouched.
    pub fn simulate_rgba(self, pixels: &mut [u8]) {
        if self == Vision::Normal {
            return;
        }
        for px in pixels.as_chunks_mut::<4>().0 {
            *px = self
                .simulate(Rgba::new(px[0], px[1], px[2], px[3]))
                .to_array();
        }
    }
}

/// A picture behind the HUD, as RGBA rows, stretched over whatever screen it is drawn on.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Backdrop {
    wide: u32,
    tall: u32,
    rgba: Vec<u8>,
}

impl Backdrop {
    /// `None` when a side is 0 or `rgba` is not exactly `wide * tall * 4` bytes.
    pub fn new(wide: u32, tall: u32, rgba: Vec<u8>) -> Option<Backdrop> {
        let len = u64::from(wide) * u64::from(tall) * 4;
        (wide > 0 && tall > 0 && u64::try_from(rgba.len()).ok() == Some(len)).then_some(Backdrop {
            wide,
            tall,
            rgba,
        })
    }

    /// Width in pixels.
    pub fn wide(&self) -> u32 {
        self.wide
    }

    /// Height in pixels.
    pub fn tall(&self) -> u32 {
        self.tall
    }

    /// The RGBA rows.
    pub fn pixels(&self) -> &[u8] {
        &self.rgba
    }

    /// The mean colour (opaque, each channel rounded) of the picture under the part of the
    /// screen box (`x`, `y`, `wide`, `tall`) that lies on `screen`; `None` when none does.
    pub fn mean_under(&self, screen: Screen, x: i32, y: i32, wide: i32, tall: i32) -> Option<Rgba> {
        let (sw, sh) = (i64::from(screen.wide()), i64::from(screen.tall()));
        let (x, y) = (i64::from(x), i64::from(y));
        let (x0, x1) = (x.max(0), (x + i64::from(wide)).min(sw));
        let (y0, y1) = (y.max(0), (y + i64::from(tall)).min(sh));
        if x0 >= x1 || y0 >= y1 {
            return None;
        }
        // The same stretch the canvas draws with: screen pixels onto picture pixels, at least one.
        // In u128, since a side times a screen edge can pass i64; both are non-negative here.
        let span = |lo: i64, hi: i64, size: u32, along: i64| {
            let (lo, hi, along) = (lo as u128, hi as u128, along as u128);
            let size = u128::from(size);
            let start = lo * size / along;
            let end = (hi * size).div_ceil(along).clamp(start + 1, size);
            (start as usize, end as usize)
        };
        let (px0, px1) = span(x0, x1, self.wide, sw);
        let (py0, py1) = span(y0, y1, self.tall, sh);
        let row_len = self.wide as usize * 4;
        let mut sums = [0u64; 3];
        for row in self.rgba.chunks_exact(row_len).take(py1).skip(py0) {
            for px in row[px0 * 4..px1 * 4].as_chunks::<4>().0 {
                for (sum, &c) in sums.iter_mut().zip(px) {
                    *sum += u64::from(c);
                }
            }
        }
        let count = ((px1 - px0) * (py1 - py0)) as u64;
        // Round half up; a mean of 8-bit channels fits a u8.
        let [r, g, b] = sums.map(|s| ((s * 2 + count) / (count * 2)) as u8);
        Some(Rgba::new(r, g, b, 255))
    }
}

impl HudModel {
    /// The colour a part's text draws in: its class's first Colour row through the scheme,
    /// white when unset (the rule the app's canvas draws with).
    pub fn text_colour(&self, part: &Part) -> Rgba {
        // The game's update overwrites an owned colour with the scheme's label colour, white
        // when the scheme names none.
        if part.code_toggled().is_some_and(|t| t.owns("fgcolor")) {
            return (self.scheme().named_colour("Label.TextColor")).unwrap_or(Rgba::WHITE);
        }
        ClassSchema::for_part(part)
            .up_to(1)
            .find(|row| row.kind() == PropertyKind::Colour)
            .and_then(|row| part.value(row.key()))
            .map(|line| self.scheme().colour(line.text()))
            .unwrap_or(Rgba::WHITE)
    }

    /// Every visible label in `layout` whose text has a contrast under `MIN_CONTRAST` with what
    /// is behind it, both as `vision` sees them; part order.
    ///
    /// Behind a label is the backdrop under the part of its box that shows, then each background
    /// painted before its text (its own included) that covers at least half of that area, mixed
    /// in by its alpha. A background covering less is ignored. Text left over a picture the core
    /// cannot read has no known colour behind it, so it is never flagged.
    pub fn hard_to_read(
        &self,
        layout: &HudLayout,
        backdrop: &Backdrop,
        vision: Vision,
    ) -> Vec<PartKey> {
        let screen = layout.screen();
        let backgrounds = self.backgrounds(layout);
        self.parts()
            .iter()
            .zip(layout.boxes())
            .filter(|(part, b)| part.value("labelText").is_some() && b.hidden().is_none())
            .filter(|(part, _)| {
                let Some(area) = layout.shown(part.key()) else {
                    return false;
                };
                let Rect { x, y, wide, tall } = area;
                let Some(mean) = backdrop.mean_under(screen, x, y, wide, tall) else {
                    return false;
                };
                let rank = layout.paint_rank(part.key());
                // Not a try_fold: an opaque fill over a picture makes the colour known again.
                let mut under = Some(vision.simulate(mean));
                let behind = (backgrounds.iter()).take_while(|bg| Some(bg.rank) <= rank);
                for bg in behind.filter(|bg| covers_half(bg.area, area)) {
                    under = match bg.colour {
                        Some(c) if c.to_array()[3] == 255 => Some(vision.simulate(c)),
                        Some(c) => under.map(|u| mix(vision.simulate(c), u)),
                        None => None,
                    };
                }
                let text = vision.simulate(self.text_colour(part));
                under.is_some_and(|u| contrast_ratio(mix(text, u), u) < MIN_CONTRAST)
            })
            .map(|(part, _)| part.key().clone())
            .collect()
    }

    /// Everything the drawn parts paint that text may sit on, back to front, as the canvas layers
    /// one part: its picture border, its colour, then its picture.
    fn backgrounds(&self, layout: &HudLayout) -> Vec<Background> {
        let mut backgrounds = Vec::new();
        for b in layout.paint_order().filter(|b| b.hidden().is_none()) {
            let (Some(part), Some(rank), Some(area)) = (
                self.part(b.key()),
                layout.paint_rank(b.key()),
                layout.shown(b.key()),
            ) else {
                continue;
            };
            // Any team's picture will do, since only whether one draws matters; RED is the preview's.
            let colours = [
                self.picture_border(part).map(|_| None),
                (self.fill_colour(part))
                    .filter(|_| paints_background(part))
                    .map(Some),
                scene_image(self, part, Team::Red).map(|_| None),
            ];
            let layers = colours.into_iter().flatten();
            backgrounds.extend(layers.map(|colour| Background { rank, area, colour }));
        }
        backgrounds
    }
}

/// One thing a part paints behind what comes after it.
struct Background {
    /// The part's place in the paint order.
    rank: usize,
    /// The screen area it shows in.
    area: Rect,
    /// Its colour; `None` for a picture, whose pixels the core does not have.
    colour: Option<Rgba>,
}

/// Whether `top` lies over at least half of `area`.
fn covers_half(top: Rect, area: Rect) -> bool {
    let overlap = |start: i32, size: i32, other: i32, other_size: i32| {
        let end =
            (i64::from(start) + i64::from(size)).min(i64::from(other) + i64::from(other_size));
        (end - i64::from(start.max(other))).max(0)
    };
    let wide = overlap(top.x, top.wide, area.x, area.wide);
    let tall = overlap(top.y, top.tall, area.y, area.tall);
    // Two i32 sides, doubled, can pass an i64.
    2 * i128::from(wide) * i128::from(tall) >= i128::from(area.wide) * i128::from(area.tall)
}

/// `top` drawn over the opaque `under` by `top`'s alpha, each channel rounded.
fn mix(top: Rgba, under: Rgba) -> Rgba {
    let [tr, tg, tb, a] = top.to_array();
    let [ur, ug, ub, _] = under.to_array();
    let a = u32::from(a);
    let ch = |t: u8, u: u8| ((u32::from(t) * a + u32::from(u) * (255 - a) + 127) / 255) as u8;
    Rgba::new(ch(tr, ur), ch(tg, ug), ch(tb, ub), 255)
}
