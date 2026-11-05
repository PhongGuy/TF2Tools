//! Starter shapes: a Dot, Cross, Cross with gap, Circle or Circle with dot, centred on the
//! canvas and sized by a few sliders.

use tf2tools_core::RgbaImage;

use super::model::CanvasSize;

/// A shape a drawing can start from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Starter {
    Dot,
    Cross,
    CrossWithGap,
    Circle,
    CircleWithDot,
}

impl Starter {
    /// Every starter, in the order the dialog lists them.
    pub const ALL: [Starter; 5] = [
        Starter::Dot,
        Starter::Cross,
        Starter::CrossWithGap,
        Starter::Circle,
        Starter::CircleWithDot,
    ];

    /// The shape's plain name.
    pub fn label(self) -> &'static str {
        match self {
            Starter::Dot => "Dot",
            Starter::Cross => "Cross",
            Starter::CrossWithGap => "Cross with gap",
            Starter::Circle => "Circle",
            Starter::CircleWithDot => "Circle with dot",
        }
    }

    /// The shape's kebab-case name, for element ids.
    pub fn slug(self) -> &'static str {
        match self {
            Starter::Dot => "dot",
            Starter::Cross => "cross",
            Starter::CrossWithGap => "cross-gap",
            Starter::Circle => "circle",
            Starter::CircleWithDot => "circle-dot",
        }
    }

    /// The sliders this shape uses, in `Param::ALL` order.
    pub fn params(self) -> &'static [Param] {
        match self {
            Starter::Dot => &[Param::Size],
            Starter::Cross => &[Param::Length, Param::Thickness],
            Starter::CrossWithGap => &[Param::Length, Param::Thickness, Param::Gap],
            Starter::Circle | Starter::CircleWithDot => &[Param::Thickness, Param::Size],
        }
    }

    /// `(min, max, step)` of `param` for this shape at `size`, given the other sliders: a Cross
    /// with gap's arms must end on the canvas, so its Length tops out at what the Gap leaves.
    pub fn limits(self, param: Param, params: StarterParams, size: CanvasSize) -> (u32, u32, u32) {
        let (min, max, step) = param.limits(size);
        if self == Starter::CrossWithGap && param == Param::Length {
            let gap = Param::Gap.snap(size, params.gap as f32);
            return (min, max.min(size.side() / 2 - gap).max(min), step);
        }
        (min, max, step)
    }

    /// `param`'s value in `params` as the nearest step inside `limits`.
    pub fn snap(self, param: Param, params: StarterParams, size: CanvasSize) -> u32 {
        let (min, max, _) = self.limits(param, params, size);
        param.snap(size, params.get(param) as f32).clamp(min, max)
    }

    /// The sliders' starting values at `size`; a Dot starts small, everything else at half.
    pub fn defaults(self, size: CanvasSize) -> StarterParams {
        let n = size.side();
        StarterParams {
            length: n / 8,
            thickness: 2,
            gap: n / 16,
            size: if self == Starter::Dot { n / 16 } else { n / 2 },
        }
    }
}

/// One slider a starter can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Param {
    Length,
    Thickness,
    Gap,
    Size,
}

impl Param {
    /// Every slider, in the order the dialog shows them.
    pub const ALL: [Param; 4] = [Param::Length, Param::Thickness, Param::Gap, Param::Size];

    /// The slider's plain name.
    pub fn label(self) -> &'static str {
        match self {
            Param::Length => "Length",
            Param::Thickness => "Thickness",
            Param::Gap => "Gap",
            Param::Size => "Size",
        }
    }

    /// The slider's kebab-case name, for element ids.
    pub fn slug(self) -> &'static str {
        match self {
            Param::Length => "length",
            Param::Thickness => "thickness",
            Param::Gap => "gap",
            Param::Size => "size",
        }
    }

    /// `(min, max, step)` at `size`. Widths move in steps of 2, since an odd width cannot
    /// sit centred between two cells.
    pub fn limits(self, size: CanvasSize) -> (u32, u32, u32) {
        let n = size.side();
        match self {
            Param::Length => (1, n / 2, 1),
            Param::Thickness => (2, n / 4, 2),
            Param::Gap => (1, n / 2 - 1, 1),
            Param::Size => (2, n, 2),
        }
    }

    /// A slider value as the nearest step from min, clamped to the limits.
    pub fn snap(self, size: CanvasSize, value: f32) -> u32 {
        let (min, max, step) = self.limits(size);
        let (min, max, step) = (min as f32, max as f32, step as f32);
        let k = ((value - min) / step).round();
        (min + k * step).clamp(min, max) as u32
    }
}

/// The values of every slider; a shape reads only the ones it shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StarterParams {
    pub length: u32,
    pub thickness: u32,
    pub gap: u32,
    pub size: u32,
}

impl StarterParams {
    /// The value of `param`'s slider.
    pub fn get(&self, param: Param) -> u32 {
        match param {
            Param::Length => self.length,
            Param::Thickness => self.thickness,
            Param::Gap => self.gap,
            Param::Size => self.size,
        }
    }

    /// Sets the value of `param`'s slider.
    pub fn set(&mut self, param: Param, value: u32) {
        match param {
            Param::Length => self.length = value,
            Param::Thickness => self.thickness = value,
            Param::Gap => self.gap = value,
            Param::Size => self.size = value,
        }
    }
}

/// The cells `shape` covers at `size`, row by row from the top, each once. Every value is
/// snapped to its slider's limits first.
pub fn starter_cells(shape: Starter, params: StarterParams, size: CanvasSize) -> Vec<(u32, u32)> {
    let n = size.side();
    let snap = |param: Param| shape.snap(param, params, size);
    let (length, thickness, span) = (
        snap(Param::Length),
        snap(Param::Thickness),
        snap(Param::Size),
    );
    let gap = if shape == Starter::CrossWithGap {
        snap(Param::Gap)
    } else {
        0
    };
    // Each test is on a cell's distance from the true centre, or on doubled offsets from it,
    // so every shape is its own mirror image both ways and sits on the centre line.
    let h = n / 2;
    let d = |i: u32| if i >= h { i - h + 1 } else { h - i };
    let dot = |x: u32, y: u32, s: u32| d(x) <= s / 2 && d(y) <= s / 2;
    let cross = |x: u32, y: u32| {
        let arm = |along: u32| gap < d(along) && d(along) <= gap + length;
        (d(y) <= thickness / 2 && arm(x)) || (d(x) <= thickness / 2 && arm(y))
    };
    let circle = |x: u32, y: u32| {
        let (p, q) = (
            2 * i64::from(x) - i64::from(n) + 1,
            2 * i64::from(y) - i64::from(n) + 1,
        );
        let outer = i64::from(span);
        let inner = i64::from(span.saturating_sub(2 * thickness));
        let r = p * p + q * q;
        inner * inner < r && r <= outer * outer
    };
    let covers = |x: u32, y: u32| match shape {
        Starter::Dot => dot(x, y, span),
        Starter::Cross | Starter::CrossWithGap => cross(x, y),
        Starter::Circle => circle(x, y),
        Starter::CircleWithDot => circle(x, y) || dot(x, y, thickness),
    };
    (0..n)
        .flat_map(|y| (0..n).map(move |x| (x, y)))
        .filter(|&(x, y)| covers(x, y))
        .collect()
}

/// A `size` drawing with `paint` on the shape's cells and the rest clear.
pub fn starter_drawing(
    shape: Starter,
    params: StarterParams,
    size: CanvasSize,
    paint: [u8; 4],
) -> RgbaImage {
    let n = size.side();
    let mut pixels = vec![0; (n * n * 4) as usize];
    for (x, y) in starter_cells(shape, params, size) {
        let at = ((y * n + x) * 4) as usize;
        pixels[at..at + 4].copy_from_slice(&paint);
    }
    RgbaImage::from_pixels(n, n, pixels).expect("the canvas is n*n*4 bytes")
}
