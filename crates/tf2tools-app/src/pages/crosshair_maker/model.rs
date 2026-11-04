//! The Crosshair Maker's editor state: the drawing, the tools that edit it and its undo
//! history, as a plain struct with no GPUI entities so it is testable without a window, plus
//! the pure grid geometry the page draws and hit-tests with.

use std::collections::VecDeque;

use gpui_kit::{Hsla, Pixels, Point, Rgba, ScrollDelta, Size, point, px};
use tf2tools_core::{CrosshairId, FitChange, RgbaImage};

/// How many steps the undo and redo stacks each keep.
pub const UNDO_LIMIT: usize = 100;
/// A fully transparent cell, what the Eraser paints.
pub const CLEAR: [u8; 4] = [0, 0, 0, 0];

/// The tool a press on the grid uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Pencil,
    Eraser,
    Fill,
    Line,
    Rectangle,
    Circle,
}

impl Tool {
    /// Every tool in toolbar order.
    pub const ALL: [Tool; 6] = [
        Tool::Pencil,
        Tool::Eraser,
        Tool::Fill,
        Tool::Line,
        Tool::Rectangle,
        Tool::Circle,
    ];

    /// "Pencil", "Eraser", "Fill", "Line", "Rectangle", "Circle".
    pub fn label(self) -> &'static str {
        match self {
            Tool::Pencil => "Pencil",
            Tool::Eraser => "Eraser",
            Tool::Fill => "Fill",
            Tool::Line => "Line",
            Tool::Rectangle => "Rectangle",
            Tool::Circle => "Circle",
        }
    }

    /// The tool's key as a gpui keystroke: "p", "e", "f", "l", "r", "c".
    pub fn key(self) -> &'static str {
        match self {
            Tool::Pencil => "p",
            Tool::Eraser => "e",
            Tool::Fill => "f",
            Tool::Line => "l",
            Tool::Rectangle => "r",
            Tool::Circle => "c",
        }
    }

    /// Line, Rectangle and Circle: drawn by dragging from start to end.
    pub fn is_shape(self) -> bool {
        matches!(self, Tool::Line | Tool::Rectangle | Tool::Circle)
    }

    /// Rectangle and Circle: they follow the Filled switch.
    pub fn has_fill_switch(self) -> bool {
        matches!(self, Tool::Rectangle | Tool::Circle)
    }

    /// "Pencil (P)": the label and the upper-cased key.
    pub fn tooltip(self) -> String {
        format!("{} ({})", self.label(), self.key().to_uppercase())
    }
}

/// The Undo button's tooltip, naming its shortcut.
pub const UNDO_TIP: &str = "Undo (Ctrl+Z)";
/// The Redo button's tooltip, naming its shortcut.
pub const REDO_TIP: &str = "Redo (Ctrl+Y)";

/// The two canvas sizes a TF2 crosshair texture takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasSize {
    Small,
    Large,
}

impl CanvasSize {
    /// The side in cells: 32 or 64.
    pub fn side(self) -> u32 {
        match self {
            CanvasSize::Small => 32,
            CanvasSize::Large => 64,
        }
    }

    /// The size the size button switches to.
    pub fn other(self) -> Self {
        match self {
            CanvasSize::Small => CanvasSize::Large,
            CanvasSize::Large => CanvasSize::Small,
        }
    }

    fn of_side(side: u32) -> Option<Self> {
        match side {
            32 => Some(CanvasSize::Small),
            64 => Some(CanvasSize::Large),
            _ => None,
        }
    }
}

/// One undo step: the whole canvas as it was.
type Snapshot = (CanvasSize, Vec<u8>);

/// A stroke in progress: the canvas before it, the tool and cell it started with, and the
/// last cell it reached.
struct Stroke {
    before: Snapshot,
    tool: Tool,
    start: (u32, u32),
    last: (u32, u32),
}

/// The maker's drawing and everything that edits it; no GPUI types.
pub struct Editor {
    size: CanvasSize,
    pixels: Vec<u8>,
    tool: Tool,
    colour: [u8; 3],
    alpha: u8,
    mirror_x: bool,
    mirror_y: bool,
    filled: bool,
    stroke: Option<Stroke>,
    undo: VecDeque<Snapshot>,
    redo: VecDeque<Snapshot>,
    open_id: Option<CrosshairId>,
    name: String,
    dirty: bool,
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

impl Editor {
    /// A clean, empty 64x64 canvas with the opaque white Pencil and no open crosshair.
    pub fn new() -> Self {
        let size = CanvasSize::Large;
        Self {
            size,
            pixels: blank(size),
            tool: Tool::Pencil,
            colour: [255, 255, 255],
            alpha: 255,
            mirror_x: false,
            mirror_y: false,
            filled: false,
            stroke: None,
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            open_id: None,
            name: String::new(),
            dirty: false,
        }
    }

    /// A clean editor over a saved drawing. None unless `drawing` is 32x32 or 64x64.
    pub fn open(id: CrosshairId, name: impl Into<String>, drawing: &RgbaImage) -> Option<Self> {
        if drawing.width() != drawing.height() {
            return None;
        }
        let size = CanvasSize::of_side(drawing.width())?;
        Some(Self {
            size,
            pixels: drawing.pixels().to_vec(),
            open_id: Some(id),
            name: name.into(),
            ..Self::new()
        })
    }

    pub fn size(&self) -> CanvasSize {
        self.size
    }

    /// The RGBA of cell `(x, y)`; panics off the canvas.
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let at = self.offset(x, y);
        let mut out = [0; 4];
        out.copy_from_slice(&self.pixels[at..at + 4]);
        out
    }

    /// The drawing as an image the size of the canvas.
    pub fn drawing(&self) -> RgbaImage {
        let n = self.size.side();
        RgbaImage::from_pixels(n, n, self.pixels.clone()).expect("the canvas is n*n*4 bytes")
    }

    pub fn tool(&self) -> Tool {
        self.tool
    }

    pub fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
    }

    pub fn colour(&self) -> [u8; 3] {
        self.colour
    }

    pub fn set_colour(&mut self, rgb: [u8; 3]) {
        self.colour = rgb;
    }

    /// The paint's alpha; 255 for a new or opened editor.
    pub fn alpha(&self) -> u8 {
        self.alpha
    }

    /// Sets the paint's alpha; 0 is stored as 1, so painting never erases.
    pub fn set_alpha(&mut self, alpha: u8) {
        self.alpha = alpha.max(1);
    }

    /// What Pencil, Fill and the shapes lay down: the colour with the alpha.
    pub fn paint_rgba(&self) -> [u8; 4] {
        let [r, g, b] = self.colour;
        [r, g, b, self.alpha]
    }

    /// Takes the RGB and alpha of cell `(x, y)` as the paint when its alpha is not 0; false
    /// and no change on a clear cell or off the canvas. Never touches the drawing or history.
    pub fn pick(&mut self, x: u32, y: u32) -> bool {
        let side = self.size.side();
        if x >= side || y >= side {
            return false;
        }
        let [r, g, b, a] = self.pixel(x, y);
        if a == 0 {
            return false;
        }
        self.colour = [r, g, b];
        self.alpha = a;
        true
    }

    pub fn mirror_x(&self) -> bool {
        self.mirror_x
    }

    pub fn set_mirror_x(&mut self, on: bool) {
        self.mirror_x = on;
    }

    pub fn mirror_y(&self) -> bool {
        self.mirror_y
    }

    pub fn set_mirror_y(&mut self, on: bool) {
        self.mirror_y = on;
    }

    /// Whether Rectangle and Circle draw filled; false (outline) for a new editor.
    pub fn filled(&self) -> bool {
        self.filled
    }

    pub fn set_filled(&mut self, on: bool) {
        self.filled = on;
    }

    /// Pencil, Eraser and the shapes start a stroke and paint `(x, y)`; Fill fills now as
    /// one step. A press off the canvas does nothing.
    pub fn press(&mut self, x: u32, y: u32) {
        if !self.on_canvas(x, y) {
            return;
        }
        self.release();
        let before = self.snapshot();
        if self.tool == Tool::Fill {
            let colour = self.paint(Tool::Fill);
            for (px, py) in self.mirrored(x, y) {
                self.flood(px, py, colour);
            }
            self.commit(before);
        } else {
            self.paint_mirrored(self.tool, x, y);
            self.stroke = Some(Stroke {
                before,
                tool: self.tool,
                start: (x, y),
                last: (x, y),
            });
        }
    }

    /// In a Pencil or Eraser stroke, paints every cell on the line from the last cell to
    /// `(x, y)`, so a fast drag leaves no gaps. In a shape, redraws the canvas as it was at the
    /// press with the shape from the start to `(x, y)`. Ignored outside a stroke or off the
    /// canvas; the stroke keeps the tool it started with.
    pub fn drag_to(&mut self, x: u32, y: u32) {
        if !self.on_canvas(x, y) {
            return;
        }
        let Some((tool, start, last)) = self.stroke.as_ref().map(|s| (s.tool, s.start, s.last))
        else {
            return;
        };
        let cells = if tool.is_shape() {
            if last == (x, y) {
                return;
            }
            // The preview is the drawing: put back the canvas from the press, then the shape.
            if let Some(stroke) = &self.stroke {
                self.pixels.copy_from_slice(&stroke.before.1);
            }
            shape_cells(tool, self.filled, start, (x, y))
        } else {
            line(last, (x, y))
        };
        for (cx, cy) in cells {
            self.paint_mirrored(tool, cx, cy);
        }
        if let Some(stroke) = &mut self.stroke {
            stroke.last = (x, y);
        }
    }

    /// Ends the stroke, recording one undo step if it changed anything.
    pub fn release(&mut self) {
        if let Some(stroke) = self.stroke.take() {
            self.commit(stroke.before);
        }
    }

    pub fn in_stroke(&self) -> bool {
        self.stroke.is_some()
    }

    /// Clears every cell as one undo step.
    pub fn clear(&mut self) {
        self.release();
        let before = self.snapshot();
        self.pixels.fill(0);
        self.commit(before);
    }

    /// Paints `rgb` at full alpha on every clear cell (alpha 0) that touches a painted cell,
    /// diagonals included, judged on the drawing before the call. One undo step; a drawing
    /// with nothing to outline records none.
    pub fn outline(&mut self, rgb: [u8; 3]) {
        self.release();
        let before = self.snapshot();
        let n = self.size.side() as i64;
        // Neighbours are read from the snapshot, so a freshly painted ring never grows its own.
        let painted = |x: i64, y: i64| {
            (0..n).contains(&x)
                && (0..n).contains(&y)
                && before.1[((y * n + x) * 4 + 3) as usize] != 0
        };
        let [r, g, b] = rgb;
        for y in 0..n {
            for x in 0..n {
                let touches = (-1..=1)
                    .flat_map(|dy| (-1..=1).map(move |dx| (dx, dy)))
                    .any(|(dx, dy)| painted(x + dx, y + dy));
                if !painted(x, y) && touches {
                    self.set_pixel(x as u32, y as u32, [r, g, b, 255]);
                }
            }
        }
        self.commit(before);
    }

    /// Switches the canvas size around the centre: 64 to 32 keeps the middle `16..48`, 32 to
    /// 64 places the drawing at offset 16. One undo step; the current size is a no-op.
    pub fn set_size(&mut self, size: CanvasSize) {
        self.release();
        if size == self.size {
            return;
        }
        let before = self.snapshot();
        let (old, new) = (self.size.side(), size.side());
        let mut pixels = blank(size);
        // Copy the overlapping square: the source starts at the margin when shrinking, the
        // target starts there when growing.
        let margin = old.abs_diff(new) / 2;
        let (src0, dst0) = if old > new { (margin, 0) } else { (0, margin) };
        let row = old.min(new) as usize * 4;
        for i in 0..old.min(new) {
            let src = (((src0 + i) * old + src0) * 4) as usize;
            let dst = (((dst0 + i) * new + dst0) * 4) as usize;
            pixels[dst..dst + row].copy_from_slice(&self.pixels[src..src + row]);
        }
        self.size = size;
        self.pixels = pixels;
        self.commit(before);
    }

    /// Steps back one edit; false when there is none.
    pub fn undo(&mut self) -> bool {
        self.release();
        let Some(prev) = self.undo.pop_back() else {
            return false;
        };
        let current = self.snapshot();
        push_capped(&mut self.redo, current);
        self.restore(prev);
        true
    }

    /// Steps forward one undone edit; false when there is none.
    pub fn redo(&mut self) -> bool {
        self.release();
        let Some(next) = self.redo.pop_back() else {
            return false;
        };
        let current = self.snapshot();
        push_capped(&mut self.undo, current);
        self.restore(next);
        true
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_depth(&self) -> usize {
        self.redo.len()
    }

    /// True once anything changed the drawing since it was opened, created or saved.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// The library crosshair this drawing was opened from or last saved as.
    pub fn open_id(&self) -> Option<&CrosshairId> {
        self.open_id.as_ref()
    }

    /// The crosshair's name, "" for a new drawing.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Records a save of `saved` as `id` and `name`, keeping the history; the drawing becomes
    /// clean only if it still equals `saved`, so edits made while the save ran stay unsaved.
    pub fn mark_saved(&mut self, id: CrosshairId, name: impl Into<String>, saved: &RgbaImage) {
        self.open_id = Some(id);
        self.name = name.into();
        if self.size.side() == saved.width() && self.pixels == saved.pixels() {
            self.dirty = false;
        }
    }

    /// Replaces the drawing with `starter` as a new, unsaved drawing: the open id and name are
    /// dropped and the history then holds one step, from a blank canvas of the starter's size to
    /// the starter. Tool, colour, alpha, mirrors and Filled stay. False and no change unless
    /// `starter` is 32x32 or 64x64.
    pub fn use_starter(&mut self, starter: &RgbaImage) -> bool {
        if starter.width() != starter.height() {
            return false;
        }
        let Some(size) = CanvasSize::of_side(starter.width()) else {
            return false;
        };
        self.release();
        // Start a fresh history on a blank canvas, then lay the starter on as its one step.
        self.undo.clear();
        self.redo.clear();
        self.size = size;
        self.pixels = blank(size);
        let before = self.snapshot();
        self.pixels.copy_from_slice(starter.pixels());
        self.commit(before);
        self.open_id = None;
        self.name.clear();
        true
    }

    /// Drops the open id so the next save makes a new crosshair; the name stays.
    pub fn forget_open(&mut self) {
        self.open_id = None;
    }

    fn on_canvas(&self, x: u32, y: u32) -> bool {
        let n = self.size.side();
        x < n && y < n
    }

    fn offset(&self, x: u32, y: u32) -> usize {
        assert!(self.on_canvas(x, y), "cell ({x}, {y}) is off the canvas");
        ((y * self.size.side() + x) * 4) as usize
    }

    fn set_pixel(&mut self, x: u32, y: u32, rgba: [u8; 4]) {
        let at = self.offset(x, y);
        self.pixels[at..at + 4].copy_from_slice(&rgba);
    }

    /// What `tool` lays down: the paint with its alpha, or clear for the Eraser.
    fn paint(&self, tool: Tool) -> [u8; 4] {
        match tool {
            Tool::Eraser => CLEAR,
            _ => self.paint_rgba(),
        }
    }

    /// `(x, y)` and its reflections under the mirrors that are on.
    fn mirrored(&self, x: u32, y: u32) -> Vec<(u32, u32)> {
        let n = self.size.side();
        let mut out = vec![(x, y)];
        if self.mirror_x {
            out.push((n - 1 - x, y));
        }
        if self.mirror_y {
            out.push((x, n - 1 - y));
        }
        if self.mirror_x && self.mirror_y {
            out.push((n - 1 - x, n - 1 - y));
        }
        out
    }

    fn paint_mirrored(&mut self, tool: Tool, x: u32, y: u32) {
        let colour = self.paint(tool);
        for (px, py) in self.mirrored(x, y) {
            self.set_pixel(px, py, colour);
        }
    }

    /// Repaints the 4-connected region of cells equal to `(x, y)` with `colour`, using an
    /// explicit stack so a whole canvas cannot overflow the call stack.
    fn flood(&mut self, x: u32, y: u32, colour: [u8; 4]) {
        let target = self.pixel(x, y);
        if target == colour {
            return;
        }
        let n = self.size.side();
        let mut stack = vec![(x, y)];
        while let Some((cx, cy)) = stack.pop() {
            if self.pixel(cx, cy) != target {
                continue;
            }
            self.set_pixel(cx, cy, colour);
            if cx > 0 {
                stack.push((cx - 1, cy));
            }
            if cx + 1 < n {
                stack.push((cx + 1, cy));
            }
            if cy > 0 {
                stack.push((cx, cy - 1));
            }
            if cy + 1 < n {
                stack.push((cx, cy + 1));
            }
        }
    }

    fn snapshot(&self) -> Snapshot {
        (self.size, self.pixels.clone())
    }

    fn restore(&mut self, (size, pixels): Snapshot) {
        self.size = size;
        self.pixels = pixels;
        self.dirty = true;
    }

    /// Records `before` as one undo step if the canvas has changed since; an edit that
    /// changed nothing leaves the history and the dirty flag alone.
    fn commit(&mut self, before: Snapshot) {
        if before.0 == self.size && before.1 == self.pixels {
            return;
        }
        push_capped(&mut self.undo, before);
        self.redo.clear();
        self.dirty = true;
    }
}

/// Cell `(x, y)`'s distance from the true centre, counted in cells from 1:
/// "3 right, 2 up". On 64x64 cell 32 is "1 right" and cell 31 "1 left".
pub fn position_text(size: CanvasSize, x: u32, y: u32) -> String {
    // The centre falls between cells half-1 and half, so neither side has a zero.
    let half = size.side() / 2;
    let (across, h) = if x >= half {
        (x - half + 1, "right")
    } else {
        (half - x, "left")
    };
    let (down, v) = if y >= half {
        (y - half + 1, "down")
    } else {
        (half - y, "up")
    };
    format!("{across} {h}, {down} {v}")
}

/// The largest whole-pixel cell that fits `side` cells into `free` both ways; at least 1 px.
pub fn cell_px(free: Size<Pixels>, side: u32) -> Pixels {
    let fit = (free.width / side as f32).min(free.height / side as f32);
    fit.floor().max(px(1.))
}

/// The cell under `at` on a grid whose top-left is `origin`; None off the grid.
pub fn cell_at(
    origin: Point<Pixels>,
    cell: Pixels,
    side: u32,
    at: Point<Pixels>,
) -> Option<(u32, u32)> {
    let index = |offset: Pixels| {
        let i = (offset / cell).floor();
        (i >= 0. && i < side as f32).then_some(i as u32)
    };
    Some((index(at.x - origin.x)?, index(at.y - origin.y)?))
}

/// The cell under `at`, or off the grid the nearest edge cell, so a stroke that leaves the
/// grid runs along its edge.
pub fn nearest_cell(
    origin: Point<Pixels>,
    cell: Pixels,
    side: u32,
    at: Point<Pixels>,
) -> (u32, u32) {
    let index = |offset: Pixels| (offset / cell).floor().clamp(0., (side - 1) as f32) as u32;
    (index(at.x - origin.x), index(at.y - origin.y))
}

/// The zoom factors the grid steps through; the first is the fitted size.
pub const ZOOMS: [u32; 8] = [1, 2, 3, 4, 6, 8, 12, 16];
/// Touchpad travel per zoom level.
pub const ZOOM_WHEEL_PIXELS: Pixels = px(50.);
/// The Zoom in button's tooltip.
pub const ZOOM_IN_TIP: &str = "Zoom in (Ctrl+Plus)";
/// The Zoom out button's tooltip.
pub const ZOOM_OUT_TIP: &str = "Zoom out (Ctrl+Minus)";
/// The zoom label's tooltip; a click fits the grid.
pub const FIT_TIP: &str = "Fit to the window (Ctrl+0)";

/// The grid's zoom and scroll inside the view that shows it; owns every cell lookup.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    view: Size<Pixels>,
    side: u32,
    level: usize,
    scroll: Point<Pixels>,
}

impl Viewport {
    /// A fitted, unscrolled grid of `side` cells in `view`.
    pub fn new(view: Size<Pixels>, side: u32) -> Self {
        Self {
            view,
            side,
            level: 0,
            scroll: Point::default(),
        }
    }

    /// Takes a new view size and side, keeping the zoom level and re-clamping the scroll.
    pub fn set_frame(&mut self, view: Size<Pixels>, side: u32) {
        self.view = view;
        self.side = side;
        self.scroll_by(Point::default());
    }

    /// The index into `ZOOMS`.
    pub fn level(&self) -> usize {
        self.level
    }

    /// How far the grid's top-left sits up and left of the view's, unrounded.
    pub fn scroll(&self) -> Point<Pixels> {
        self.scroll
    }

    /// One cell's side at the current zoom.
    pub fn cell(&self) -> Pixels {
        cell_px(self.view, self.side) * ZOOMS[self.level] as f32
    }

    /// The grid's side at the current zoom.
    pub fn length(&self) -> Pixels {
        self.cell() * self.side as f32
    }

    /// The zoom as a percentage, "100%" to "1600%".
    pub fn label(&self) -> String {
        format!("{}%", ZOOMS[self.level] * 100)
    }

    /// Whether a larger zoom level is left.
    pub fn can_zoom_in(&self) -> bool {
        self.level + 1 < ZOOMS.len()
    }

    /// Whether the grid is zoomed past the fit.
    pub fn can_zoom_out(&self) -> bool {
        self.level > 0
    }

    /// The grid's top-left relative to the view's: centred and floored on an axis where the
    /// grid fits, else the rounded negative scroll.
    pub fn origin(&self) -> Point<Pixels> {
        let length = self.length();
        let axis = |view: Pixels, scroll: Pixels| {
            if length <= view {
                ((view - length) / 2.).floor()
            } else {
                -scroll.round()
            }
        };
        point(
            axis(self.view.width, self.scroll.x),
            axis(self.view.height, self.scroll.y),
        )
    }

    /// Adds `by` to the scroll, clamped per axis to `0..=(length - view).floor().max(0)`.
    pub fn scroll_by(&mut self, by: Point<Pixels>) {
        // The origin rounds the scroll, so a whole-pixel maximum keeps the grid's far edge
        // at or past the view's when the view size is fractional.
        let length = self.length();
        let axis = |scroll: Pixels, by: Pixels, view: Pixels| {
            (scroll + by).clamp(px(0.), (length - view).floor().max(px(0.)))
        };
        self.scroll = point(
            axis(self.scroll.x, by.x, self.view.width),
            axis(self.scroll.y, by.y, self.view.height),
        );
    }

    /// Moves `steps` levels, keeping the grid point under view-relative `anchor` under it
    /// where the scroll clamp allows.
    pub fn zoom_at(&mut self, steps: i32, anchor: Point<Pixels>) {
        let level = (self.level as i64 + i64::from(steps)).clamp(0, ZOOMS.len() as i64 - 1);
        if level as usize == self.level {
            return;
        }
        // The anchor's place on the grid in cells, scaled to the new cell size.
        let (origin, old) = (self.origin(), self.cell());
        let (u, v) = ((anchor.x - origin.x) / old, (anchor.y - origin.y) / old);
        self.level = level as usize;
        let cell = self.cell();
        self.scroll = point(cell * u - anchor.x, cell * v - anchor.y);
        self.scroll_by(Point::default());
    }

    /// Moves `steps` levels around the view's centre.
    pub fn zoom_centre(&mut self, steps: i32) {
        self.zoom_at(steps, point(self.view.width / 2., self.view.height / 2.));
    }

    /// Back to the fitted size with no scroll.
    pub fn fit(&mut self) {
        self.level = 0;
        self.scroll = Point::default();
    }

    /// The cell under view-relative `at`; None outside the view or off the grid.
    pub fn cell_at(&self, at: Point<Pixels>) -> Option<(u32, u32)> {
        let inside = |at: Pixels, view: Pixels| at >= px(0.) && at < view;
        if !inside(at.x, self.view.width) || !inside(at.y, self.view.height) {
            return None;
        }
        cell_at(self.origin(), self.cell(), self.side, at)
    }

    /// The cell under view-relative `at`, or the nearest cell that shows in the view.
    pub fn nearest_cell(&self, at: Point<Pixels>) -> (u32, u32) {
        // Clamp each index to the cells that show, so a drag off a zoomed view runs along
        // its edge rather than onto hidden cells.
        let (origin, cell, last) = (self.origin(), self.cell(), (self.side - 1) as f32);
        let axis = |at: Pixels, o: Pixels, view: Pixels| {
            let first = (-o / cell).floor().clamp(0., last);
            let shown = ((view - o) / cell).ceil() - 1.;
            ((at - o) / cell)
                .floor()
                .clamp(first, shown.clamp(first, last)) as u32
        };
        (
            axis(at.x, origin.x, self.view.width),
            axis(at.y, origin.y, self.view.height),
        )
    }
}

/// Levels for one Ctrl+wheel event: a notched wheel moves one level toward the sign of its y
/// (up zooms in); a touchpad moves one level per `ZOOM_WHEEL_PIXELS` gathered in `carry`.
pub fn wheel_zoom_steps(delta: ScrollDelta, carry: &mut Pixels) -> i32 {
    match delta {
        ScrollDelta::Lines(lines) => {
            if lines.y > 0. {
                1
            } else if lines.y < 0. {
                -1
            } else {
                0
            }
        }
        ScrollDelta::Pixels(pixels) => {
            *carry += pixels.y;
            let steps = if *carry >= ZOOM_WHEEL_PIXELS {
                1
            } else if *carry <= -ZOOM_WHEEL_PIXELS {
                -1
            } else {
                return 0;
            };
            *carry = px(0.);
            steps
        }
    }
}

/// The scroll change for a plain wheel delta: the negated delta; with Shift and no x, the y
/// goes sideways.
pub fn wheel_scroll(delta: Point<Pixels>, shift: bool) -> Point<Pixels> {
    if shift && delta.x == px(0.) {
        point(-delta.y, px(0.))
    } else {
        point(-delta.x, -delta.y)
    }
}

/// The swatches offered beside the picker: white and three greys, which all follow the
/// player's crosshair colour in game.
pub const SWATCHES: [[u8; 3]; 4] = [
    [255, 255, 255],
    [192, 192, 192],
    [128, 128, 128],
    [64, 64, 64],
];
/// The swatches' names, in `SWATCHES` order.
pub const SWATCH_NAMES: [&str; 4] = ["White", "Light grey", "Grey", "Dark grey"];

/// The tooltip of a white or grey swatch, which keeps its meaning visible once another
/// colour is picked.
pub const FOLLOWS_TIP: &str = "Follows your crosshair colour in game";

/// The tooltip of a swatch with colour `rgb`: FOLLOWS_TIP for white and grey, else `name`.
pub fn swatch_tooltip(rgb: [u8; 3], name: &'static str) -> &'static str {
    if colour_hint(rgb) == FOLLOWS {
        FOLLOWS_TIP
    } else {
        name
    }
}

/// The hint under a grey colour.
pub const FOLLOWS: &str = "Follows your crosshair colour";
/// The hint under any other colour.
pub const MIXED: &str = "In game this colour is mixed with your crosshair colour";

/// What a colour does in game: FOLLOWS when red, green and blue are equal, else MIXED.
pub fn colour_hint([r, g, b]: [u8; 3]) -> &'static str {
    if r == g && g == b { FOLLOWS } else { MIXED }
}

/// The picker's colour as RGB bytes, each channel rounded.
pub fn rgb_of(colour: Hsla) -> [u8; 3] {
    let Rgba { r, g, b, .. } = colour.to_rgb();
    [r, g, b].map(|c| (c.clamp(0., 1.) * 255.).round() as u8)
}

/// RGB bytes as the picker's colour.
pub fn hsla_of([r, g, b]: [u8; 3]) -> Hsla {
    hsla_of_rgba([r, g, b, 255])
}

/// RGBA bytes as a colour with that alpha, for drawing see-through cells.
pub fn hsla_of_rgba(rgba: [u8; 4]) -> Hsla {
    let [r, g, b, a] = rgba.map(|c| f32::from(c) / 255.);
    Rgba { r, g, b, a }.into()
}

/// The opacity slider's tooltip.
pub const OPACITY_TIP: &str =
    "See-through paint. Painting over a pixel replaces it; colours never blend.";

/// The note when a copied or imported picture was shrunk onto the canvas.
pub const FIT_SHRUNK: &str = "This image is bigger than the canvas, so it was shrunk to fit.";
/// The note when a copied or imported picture was placed unscaled in the middle.
pub const FIT_CENTRED: &str =
    "This image is smaller than the canvas, so it was placed in the middle.";

/// The note a fitted picture toasts; None when it was kept as it is.
pub fn fit_note(change: FitChange) -> Option<&'static str> {
    match change {
        FitChange::ShrunkToFit => Some(FIT_SHRUNK),
        FitChange::Centred => Some(FIT_CENTRED),
        _ => None,
    }
}

/// The alpha for an opacity percent, clamped to 1..=100: `(p * 255 + 50) / 100`.
pub fn alpha_of_percent(percent: u32) -> u8 {
    ((percent.clamp(1, 100) * 255 + 50) / 100) as u8
}

/// The whole percent an alpha shows as, at least 1: `((a * 100 + 127) / 255).max(1)`.
pub fn percent_of_alpha(alpha: u8) -> u32 {
    ((u32::from(alpha) * 100 + 127) / 255).max(1)
}

/// Each channel times `by`, `(c * b + 127) / 255`; alpha kept.
pub fn tint([r, g, b, a]: [u8; 4], [br, bg, bb]: [u8; 3]) -> [u8; 4] {
    let mul = |c: u8, by: u8| ((u16::from(c) * u16::from(by) + 127) / 255) as u8;
    [mul(r, br), mul(g, bg), mul(b, bb), a]
}

/// The 64x64 in-game texture tinted by `by`: a 32x32 drawing doubled cell for cell.
pub fn preview_texels(drawing: &RgbaImage, by: [u8; 3]) -> RgbaImage {
    const SIDE: u32 = 64;
    // Each preview texel reads the drawing cell it covers, so 32x32 doubles and 64x64 maps 1:1.
    let scale = SIDE / drawing.width().max(1);
    let source = drawing.pixels();
    let mut out = Vec::with_capacity((SIDE * SIDE * 4) as usize);
    for y in 0..SIDE {
        for x in 0..SIDE {
            let at = (((y / scale) * drawing.width() + x / scale) * 4) as usize;
            let mut texel = [0; 4];
            texel.copy_from_slice(&source[at..at + 4]);
            out.extend(tint(texel, by));
        }
    }
    RgbaImage::from_pixels(SIDE, SIDE, out).expect("the preview is 64*64*4 bytes")
}

fn blank(size: CanvasSize) -> Vec<u8> {
    vec![0; (size.side() * size.side() * 4) as usize]
}

/// Pushes onto a history stack, dropping the oldest step past `UNDO_LIMIT`.
fn push_capped(stack: &mut VecDeque<Snapshot>, step: Snapshot) {
    if stack.len() == UNDO_LIMIT {
        stack.pop_front();
    }
    stack.push_back(step);
}

/// The cells `tool` covers when dragged from `from` to `to`: a line, or the rectangle or circle
/// fitting the box the two cells span, outlined unless `filled`.
fn shape_cells(tool: Tool, filled: bool, from: (u32, u32), to: (u32, u32)) -> Vec<(u32, u32)> {
    if tool == Tool::Line {
        return line(from, to);
    }
    let (x0, x1) = (i64::from(from.0.min(to.0)), i64::from(from.0.max(to.0)));
    let (y0, y1) = (i64::from(from.1.min(to.1)), i64::from(from.1.max(to.1)));
    // Inside test of an ellipse a quarter cell inside the box, in integers so no cell sits on
    // a float boundary; a rectangle is the whole box. The centre row and column always count,
    // so a thin oval still reaches all four sides of its box.
    let (pp, qq) = ((2 * (x1 - x0) + 1).pow(2), (2 * (y1 - y0) + 1).pow(2));
    let inside = |x: i64, y: i64| {
        if x < x0 || x > x1 || y < y0 || y > y1 {
            return false;
        }
        let (dx, dy) = (2 * x - x0 - x1, 2 * y - y0 - y1);
        let (p, q) = (2 * dx, 2 * dy);
        tool == Tool::Rectangle
            || dx.abs() <= 1
            || dy.abs() <= 1
            || p * p * qq + q * q * pp <= pp * qq
    };
    // An outline keeps the inside cells with a 4-neighbour outside.
    let mut out = Vec::new();
    for y in y0..=y1 {
        for x in x0..=x1 {
            let edge = [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)]
                .into_iter()
                .any(|(nx, ny)| !inside(nx, ny));
            if inside(x, y) && (filled || edge) {
                out.push((x as u32, y as u32));
            }
        }
    }
    out
}

/// The cells of the Bresenham line between `from` and `to`, both ends included.
fn line(from: (u32, u32), to: (u32, u32)) -> Vec<(u32, u32)> {
    // Bresenham breaks exact ties by direction, so always run from the smaller end and a line
    // covers the same cells whichever way it was dragged.
    let (from, to) = if to < from { (to, from) } else { (from, to) };
    let (mut x, mut y) = (i64::from(from.0), i64::from(from.1));
    let (x1, y1) = (i64::from(to.0), i64::from(to.1));
    let (dx, dy) = ((x1 - x).abs(), -(y1 - y).abs());
    let (sx, sy) = ((x1 - x).signum(), (y1 - y).signum());
    let mut err = dx + dy;
    let mut out = Vec::new();
    loop {
        // Both ends lie on the canvas, so every cell between them does too.
        out.push((x as u32, y as u32));
        if x == x1 && y == y1 {
            return out;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}
