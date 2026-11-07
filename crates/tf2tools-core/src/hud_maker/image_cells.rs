//! How an image part lays its texture over its box: one stretched piece, or the nine pieces
//! of a scalable panel whose corners keep their size while edges and centre stretch.

use super::Size;
use super::expr::{ScreenScale, atoi};
use super::layout::Screen;
use super::model::Part;

/// One piece of an image part: where it draws, in screen pixels from the part's top-left
/// corner, and which texels of the texture it shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ImageCell {
    x: i32,
    y: i32,
    wide: i32,
    tall: i32,
    src_x: u32,
    src_y: u32,
    src_wide: u32,
    src_tall: u32,
}

impl ImageCell {
    /// Left edge on screen, from the part's left.
    pub fn x(&self) -> i32 {
        self.x
    }

    /// Top edge on screen, from the part's top.
    pub fn y(&self) -> i32 {
        self.y
    }

    /// Width on screen in pixels.
    pub fn wide(&self) -> i32 {
        self.wide
    }

    /// Height on screen in pixels.
    pub fn tall(&self) -> i32 {
        self.tall
    }

    /// Left edge in the texture, in texels.
    pub fn src_x(&self) -> u32 {
        self.src_x
    }

    /// Top edge in the texture, in texels.
    pub fn src_y(&self) -> u32 {
        self.src_y
    }

    /// Width in the texture, in texels.
    pub fn src_wide(&self) -> u32 {
        self.src_wide
    }

    /// Height in the texture, in texels.
    pub fn src_tall(&self) -> u32 {
        self.src_tall
    }
}

/// One axis's pieces: (screen start, screen size, texel start, texel size).
type Span = (i32, i32, u32, u32);

/// The pieces `part` draws its `texture` (in texels) in, for a box `wide` x
/// `tall` on `screen`. A scalable panel keeps `draw_corner_*` (proportional units) for the
/// corners' size and `src_corner_*` texels of the texture for them; pieces with no size on
/// screen or in the texture are left out. Rows top to bottom, each left to right.
pub fn image_cells(
    part: &Part,
    wide: i32,
    tall: i32,
    texture: Size<u32>,
    screen: Screen,
) -> Vec<ImageCell> {
    let scalable = part.class().is_some_and(|class| {
        class.eq_ignore_ascii_case("ScalableImagePanel")
            || class.eq_ignore_ascii_case("CTFImagePanel")
    });
    // A plain image reads every corner as 0, which leaves one stretched centre piece.
    let key = |name: &str| {
        if !scalable {
            return 0;
        }
        part.value(name)
            .map_or(0, |line| atoi(line.text().as_bytes()).max(0))
    };
    nine_slice(
        wide,
        tall,
        texture,
        Size::new(key("draw_corner_width"), key("draw_corner_height")),
        Size::new(key("src_corner_width"), key("src_corner_height")),
        screen,
    )
}

/// The nine pieces of a `texture` (in texels) stretched over a box `wide` x
/// `tall` on `screen`: corners `draw_corner` proportional units on screen showing `src_corner`
/// texels, edges and centre stretched. Negative corners count as 0; pieces with no size on
/// screen or in the texture are left out. Rows top to bottom, each left to right.
pub fn nine_slice(
    wide: i32,
    tall: i32,
    texture: Size<u32>,
    draw_corner: Size,
    src_corner: Size,
    screen: Screen,
) -> Vec<ImageCell> {
    let scale = ScreenScale::new(screen.tall());
    let columns = spans(
        wide,
        scale.scale(draw_corner.wide.max(0)),
        texture.wide,
        src_corner.wide.max(0),
    );
    let rows = spans(
        tall,
        scale.scale(draw_corner.tall.max(0)),
        texture.tall,
        src_corner.tall.max(0),
    );
    let mut cells = Vec::new();
    for &(y, tall, src_y, src_tall) in &rows {
        for &(x, wide, src_x, src_wide) in &columns {
            if wide > 0 && tall > 0 && src_wide > 0 && src_tall > 0 {
                cells.push(ImageCell {
                    x,
                    y,
                    wide,
                    tall,
                    src_x,
                    src_y,
                    src_wide,
                    src_tall,
                });
            }
        }
    }
    cells
}

// Near corner, stretched middle, far corner. The source corner is held to half the texture so
// no piece samples past its edge; a screen corner past half the box leaves no middle.
fn spans(size: i32, corner: i32, texels: u32, src_corner: i32) -> [Span; 3] {
    let src = u32::try_from(src_corner).unwrap_or(0).min(texels / 2);
    let src_middle = texels.saturating_sub(src.saturating_mul(2));
    let middle = size.saturating_sub(corner.saturating_mul(2)).max(0);
    [
        (0, corner, 0, src),
        (corner, middle, src, src_middle),
        (
            corner.saturating_add(middle),
            corner,
            src.saturating_add(src_middle),
            src,
        ),
    ]
}
