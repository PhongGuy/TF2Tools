//! The HUD's own on-screen crosshair: the CustomCrosshair label many HUDs keep in
//! hudlayout.res, read for the preview as the game merges it with the files it `#base`s.

use super::HudModel;
use super::document::KvEntry;
use super::expr::{AxisSize, PosExpr, ScreenScale, SizeExpr, atoi};
use super::layout::Screen;
use super::model::root_block;
use super::scheme::Rgba;

const HUDLAYOUT: &str = "scripts/hudlayout.res";
const CUSTOM_CROSSHAIR: &str = "CustomCrosshair";

/// The HUD crosshair label as the preview draws it: whether it shows, its symbol, font,
/// colour and its box in pixels on one screen.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct HudCrosshair {
    shown: bool,
    symbol: String,
    font: String,
    colour: Rgba,
    x: i32,
    y: i32,
    wide: i32,
    tall: i32,
}

impl HudCrosshair {
    /// Whether the game draws it: `visible` is not 0.
    pub fn shown(&self) -> bool {
        self.shown
    }

    /// The `labelText`, drawn in the crosshair font.
    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    /// The scheme font name it draws with.
    pub fn font(&self) -> &str {
        &self.font
    }

    /// The `fgcolor`, resolved through the scheme.
    pub fn colour(&self) -> Rgba {
        self.colour
    }

    /// Left edge in pixels.
    pub fn x(&self) -> i32 {
        self.x
    }

    /// Top edge in pixels.
    pub fn y(&self) -> i32 {
        self.y
    }

    /// Width in pixels.
    pub fn wide(&self) -> i32 {
        self.wide
    }

    /// Height in pixels.
    pub fn tall(&self) -> i32 {
        self.tall
    }
}

impl HudModel {
    /// The HUD crosshair label on `screen`, or `None` for a HUD without one. Each key is the
    /// file's own line when it has one, else the line of the `#base` file that wins.
    pub fn hud_crosshair(&self, screen: Screen) -> Option<HudCrosshair> {
        // Merged roots list the bases lowest rank first; the winner goes first here.
        let mut roots = self.merged_roots(HUDLAYOUT);
        let own = usize::from(self.document(HUDLAYOUT).and_then(root_block).is_some());
        let own = own.min(roots.len());
        roots[own..].reverse();
        let platform = self.platform();
        let blocks: Vec<&KvEntry> = (roots.into_iter())
            .flat_map(|root| {
                let mut own: Vec<&KvEntry> = (root.children().iter())
                    .filter(|e| {
                        e.text().is_none()
                            && e.holds(platform)
                            && e.key().eq_ignore_ascii_case(CUSTOM_CROSSHAIR)
                    })
                    .collect();
                own.reverse();
                own
            })
            .collect();
        if blocks.is_empty() {
            return None;
        }
        // Within one file the later block and line win, so each is read last first.
        let value = |key: &str| {
            (blocks.iter())
                .find_map(|b| {
                    (b.children().iter()).rfind(|e| {
                        e.text().is_some() && e.holds(platform) && e.key().eq_ignore_ascii_case(key)
                    })
                })
                .and_then(KvEntry::text)
        };
        let scale = ScreenScale::new(screen.tall());
        let (sw, sh) = (screen.wide() as i32, screen.tall() as i32);
        let wide_expr = value("wide").map(SizeExpr::parse);
        let tall_expr = value("tall").map(SizeExpr::parse);
        // The engine starts every panel at 64x24 px before its settings apply.
        let (wide_axis, tall_axis) = (AxisSize::new(64, sw), AxisSize::new(24, sh));
        let wide = (wide_expr.as_ref()).map_or(64, |e| {
            e.eval(wide_axis, tall_expr.as_ref(), tall_axis, scale)
        });
        let tall = (tall_expr.as_ref()).map_or(24, |e| {
            e.eval(tall_axis, wide_expr.as_ref(), wide_axis, scale)
        });
        let pos = |key: &str, own: i32, along: i32| {
            value(key).map_or(0, |t| PosExpr::parse(t).eval(own, along, scale))
        };
        Some(HudCrosshair {
            shown: value("visible").is_none_or(|v| atoi(v.as_bytes()) != 0),
            symbol: value("labelText").unwrap_or_default().to_owned(),
            font: value("font").unwrap_or_default().to_owned(),
            colour: (self.scheme()).colour(value("fgcolor").unwrap_or("255 255 255 255")),
            x: pos("xpos", wide, sw),
            y: pos("ypos", tall, sh),
            wide,
            tall,
        })
    }
}
