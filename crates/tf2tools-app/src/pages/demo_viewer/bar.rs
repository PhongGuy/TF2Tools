//! Where the Demo Viewer's floating bar sits, and the setting text that remembers it.

use gpui_kit::{Pixels, Point, Size, point, px};

/// The bar's centre as fractions 0..=1 of the page area, and whether it is collapsed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarPlace {
    pub x: f32,
    pub y: f32,
    pub collapsed: bool,
}

impl BarPlace {
    /// Open, bottom centre; `clamp` pulls it up so the whole bar shows.
    pub const DEFAULT: BarPlace = BarPlace {
        x: 0.5,
        y: 0.94,
        collapsed: false,
    };

    /// The setting text, such as `"0.500 0.940 open"`.
    pub fn to_setting(self) -> String {
        let state = if self.collapsed { "collapsed" } else { "open" };
        format!("{:.3} {:.3} {}", self.x, self.y, state)
    }

    /// Reads `to_setting`'s text back; `None` for anything else, including numbers outside 0..=1.
    pub fn from_setting(text: &str) -> Option<BarPlace> {
        let fraction = |word: &str| {
            word.parse::<f32>()
                .ok()
                .filter(|v| v.is_finite() && (0.0..=1.0).contains(v))
        };
        let mut words = text.split_whitespace();
        let x = fraction(words.next()?)?;
        let y = fraction(words.next()?)?;
        let collapsed = match words.next()? {
            "open" => false,
            "collapsed" => true,
            _ => return None,
        };
        words
            .next()
            .is_none()
            .then_some(BarPlace { x, y, collapsed })
    }

    /// The top-left offset of a `bar`-sized bar in `area`: centred on this place, then pulled
    /// back inside, or pinned to 0 on an axis where the bar does not fit.
    pub fn clamp(self, area: Size<Pixels>, bar: Size<Pixels>) -> Point<Pixels> {
        let axis = |fraction: f32, a: Pixels, b: Pixels| {
            let room = f32::from(a - b).max(0.);
            px((fraction * f32::from(a) - f32::from(b) / 2.).clamp(0., room))
        };
        point(
            axis(self.x, area.width, bar.width),
            axis(self.y, area.height, bar.height),
        )
    }

    /// This place with its centre moved from `from` by `by` pixels, as fractions of `area`
    /// kept inside 0..=1; unchanged over an area with no width or height.
    pub fn dragged(self, from: Point<Pixels>, by: Point<Pixels>, area: Size<Pixels>) -> BarPlace {
        let fraction = |at: Pixels, length: Pixels| f32::from(at) / f32::from(length);
        let centre = from + by;
        let x = fraction(centre.x, area.width);
        let y = fraction(centre.y, area.height);
        if !(area.width > px(0.) && area.height > px(0.) && x.is_finite() && y.is_finite()) {
            return self;
        }
        BarPlace {
            x: x.clamp(0., 1.),
            y: y.clamp(0., 1.),
            ..self
        }
    }
}
