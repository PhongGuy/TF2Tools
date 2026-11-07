//! Whole-pixel places, sizes and boxes on a HUD screen, with fields named as TF2's HUD files
//! name them (`xpos`/`ypos`, `wide`/`tall`), so two numbers can no longer be swapped silently.

/// A place on the screen or inside a parent, in pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Point {
    /// Left offset.
    pub x: i32,
    /// Top offset.
    pub y: i32,
}

impl Point {
    /// The point (`x`, `y`).
    pub const fn new(x: i32, y: i32) -> Point {
        Point { x, y }
    }
}

/// A width and height: pixels for a box, texels (`Size<u32>`) for a texture.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Size<T = i32> {
    /// Width.
    pub wide: T,
    /// Height.
    pub tall: T,
}

impl<T> Size<T> {
    /// A size `wide` x `tall`.
    pub const fn new(wide: T, tall: T) -> Size<T> {
        Size { wide, tall }
    }
}

/// A box in pixels: its top-left corner and its size.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Rect {
    /// Left edge.
    pub x: i32,
    /// Top edge.
    pub y: i32,
    /// Width.
    pub wide: i32,
    /// Height.
    pub tall: i32,
}

impl Rect {
    /// The box at (`x`, `y`), `wide` x `tall`.
    pub const fn new(x: i32, y: i32, wide: i32, tall: i32) -> Rect {
        Rect { x, y, wide, tall }
    }

    /// Whether (`x`, `y`) lies inside: the left and top edges count, the right and bottom
    /// do not. Worked in i64, so a box reaching past `i32::MAX` never wraps.
    pub fn contains(self, x: i32, y: i32) -> bool {
        x >= self.x
            && y >= self.y
            && i64::from(x) < i64::from(self.x) + i64::from(self.wide)
            && i64::from(y) < i64::from(self.y) + i64::from(self.tall)
    }
}
