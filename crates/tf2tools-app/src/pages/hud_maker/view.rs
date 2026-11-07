//! Pure geometry for the HUD Maker's canvas and the things floating over it.

use gpui_kit::{Bounds, Pixels, Point, Size, point, px, size};
use tf2tools_core::{PlayerListLayout, Scheme, Screen, font_tall_px};

/// The player lists' text height in screen pixels: the scheme's `Default` font at `screen`,
/// sized by `pixel_tall`, else `font_tall_px(12, screen.tall())`.
pub fn list_font_px(scheme: &Scheme, screen: Screen) -> i32 {
    scheme
        .font(PlayerListLayout::ROW_FONT, screen.tall())
        .map_or(font_tall_px(12, screen.tall()), |font| {
            font.pixel_tall(screen.tall())
        })
}

/// Where a screen shape sits inside a free area, and how screen pixels map to window pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanvasFit {
    origin: Point<Pixels>,
    scale: f32,
    screen: Screen,
}

impl CanvasFit {
    /// The largest scale at which `screen` fits `area`, centred; an empty area gives scale 0.
    pub fn new(area: Bounds<Pixels>, screen: Screen) -> CanvasFit {
        let (wide, tall) = (screen.wide() as f32, screen.tall() as f32);
        let (w, h) = (f32::from(area.size.width), f32::from(area.size.height));
        let scale = if w > 0. && h > 0. && wide > 0. && tall > 0. {
            (w / wide).min(h / tall)
        } else {
            0.
        };
        let origin = area.origin + point(px((w - wide * scale) / 2.), px((h - tall * scale) / 2.));
        CanvasFit {
            origin,
            scale,
            screen,
        }
    }

    /// The fit scaled by `zoom` and moved by its pan. A screen longer than `area` on an axis is
    /// kept covering it there, so a pan never shows past the screen's edge; a shorter one centres.
    pub fn zoomed(area: Bounds<Pixels>, screen: Screen, zoom: Zoom) -> CanvasFit {
        let fit = CanvasFit::new(area, screen);
        if zoom.is_fit() || fit.scale <= 0. {
            return fit;
        }
        let scale = fit.scale * zoom.level();
        let centred = centred_origin(area, screen, scale);
        let place = |start: Pixels, room: Pixels, len: f32, at: Pixels| {
            let len = px(len);
            if len > room {
                at.min(start).max(start + room - len)
            } else {
                start + (room - len) / 2.
            }
        };
        let (wide, tall) = (screen.wide() as f32 * scale, screen.tall() as f32 * scale);
        let origin = point(
            place(area.origin.x, area.size.width, wide, centred.x + zoom.pan.x),
            place(
                area.origin.y,
                area.size.height,
                tall,
                centred.y + zoom.pan.y,
            ),
        );
        CanvasFit {
            origin,
            scale,
            screen,
        }
    }

    /// Window pixels per screen pixel.
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// The whole screen in window pixels.
    pub fn screen_bounds(&self) -> Bounds<Pixels> {
        self.rect(0, 0, self.screen.wide() as i32, self.screen.tall() as i32)
    }

    /// A box in screen pixels, in window pixels.
    pub fn rect(&self, x: i32, y: i32, wide: i32, tall: i32) -> Bounds<Pixels> {
        let s = self.scale;
        Bounds {
            origin: self.origin + point(px(x as f32 * s), px(y as f32 * s)),
            size: size(px(wide as f32 * s), px(tall as f32 * s)),
        }
    }

    /// The screen pixel under window point `at`; `None` outside the screen or at scale 0.
    pub fn to_screen(&self, at: Point<Pixels>) -> Option<(i32, i32)> {
        if self.scale <= 0. {
            return None;
        }
        let x = (f32::from(at.x - self.origin.x) / self.scale).floor();
        let y = (f32::from(at.y - self.origin.y) / self.scale).floor();
        let inside = |v: f32, end: u32| v >= 0. && v < end as f32;
        (inside(x, self.screen.wide()) && inside(y, self.screen.tall()))
            .then_some((x as i32, y as i32))
    }
}

// Where `screen` at `scale` sits centred in `area`.
fn centred_origin(area: Bounds<Pixels>, screen: Screen, scale: f32) -> Point<Pixels> {
    let (wide, tall) = (screen.wide() as f32 * scale, screen.tall() as f32 * scale);
    area.origin
        + point(
            (area.size.width - px(wide)) / 2.,
            (area.size.height - px(tall)) / 2.,
        )
}

/// The canvas's zoom levels, as times the fit: the fit, then up to four times it.
pub const ZOOM_LEVELS: [f32; 7] = [1., 1.25, 1.5, 2., 2.5, 3., 4.];

/// How far the player zoomed into the canvas past the fit, and how far they panned it from
/// centred. Every step and pan comes back clamped, so the zoom always draws what it says.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Zoom {
    step: usize,
    pan: Point<Pixels>,
}

impl Zoom {
    /// The screen fitted to the area, which is where every canvas starts.
    pub const FIT: Zoom = Zoom {
        step: 0,
        pan: Point {
            x: px(0.),
            y: px(0.),
        },
    };

    /// Times the fit this zoom draws at.
    pub fn level(&self) -> f32 {
        ZOOM_LEVELS[self.step]
    }

    pub fn is_fit(&self) -> bool {
        self.step == 0
    }

    pub fn can_zoom_in(&self) -> bool {
        self.step + 1 < ZOOM_LEVELS.len()
    }

    pub fn can_zoom_out(&self) -> bool {
        self.step > 0
    }

    /// The level as a player reads it, the fit being 100%.
    pub fn label(&self) -> String {
        format!("{}%", (self.level() * 100.).round())
    }

    /// `steps` levels in (or out, when negative), keeping the screen pixel under window point
    /// `anchor` where it is as far as the screen's edges allow. Back at the fit, it centres.
    pub fn step(
        self,
        steps: i32,
        anchor: Point<Pixels>,
        area: Bounds<Pixels>,
        screen: Screen,
    ) -> Zoom {
        let last = ZOOM_LEVELS.len() as i64 - 1;
        let to = (self.step as i64 + i64::from(steps)).clamp(0, last) as usize;
        let now = CanvasFit::zoomed(area, screen, self);
        if to == 0 || now.scale <= 0. {
            return Zoom::FIT;
        }
        let scale = now.scale / self.level() * ZOOM_LEVELS[to];
        let origin = anchor - (anchor - now.origin) * (scale / now.scale);
        let pan = origin - centred_origin(area, screen, scale);
        Zoom { step: to, pan }.clamped(area, screen)
    }

    /// The view moved by `by` window pixels, as far as the screen's edges allow.
    pub fn pan_by(self, by: Point<Pixels>, area: Bounds<Pixels>, screen: Screen) -> Zoom {
        Zoom {
            pan: self.pan + by,
            ..self
        }
        .clamped(area, screen)
    }

    // The pan the drawn origin really has, so a pan past an edge does not wind up.
    fn clamped(self, area: Bounds<Pixels>, screen: Screen) -> Zoom {
        let fit = CanvasFit::zoomed(area, screen, self);
        let pan = fit.origin - centred_origin(area, screen, fit.scale);
        Zoom { pan, ..self }
    }
}

/// The side panel's width, floating or docked.
pub const PANEL_WIDTH: Pixels = px(280.);

/// The two floating panels: the side panel with the selection's options, and the part list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Options,
    Parts,
}

impl Panel {
    pub const ALL: [Panel; 2] = [Panel::Options, Panel::Parts];

    /// The panel that is not `self`.
    pub fn other(self) -> Panel {
        match self {
            Panel::Options => Panel::Parts,
            Panel::Parts => Panel::Options,
        }
    }

    /// The panel's width, floating or docked: the side panel is wider than the part list.
    pub fn width(self) -> Pixels {
        match self {
            Panel::Options => PANEL_WIDTH,
            Panel::Parts => px(240.),
        }
    }
}

/// Whether the side panel floats over the canvas area or docks inside one of its edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelMode {
    Floating,
    DockLeft,
    DockRight,
}

/// Where the side panel sits; `x` and `y` are its floating offset in the canvas area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelPlace {
    pub mode: PanelMode,
    pub x: f32,
    pub y: f32,
    pub collapsed: bool,
}

impl PanelPlace {
    /// Floating 16 px in from the area's top left, open.
    pub const DEFAULT: PanelPlace = PanelPlace {
        mode: PanelMode::Floating,
        x: 16.,
        y: 16.,
        collapsed: false,
    };

    /// Floating at the area's top right (clamping pulls the huge `x` in), collapsed.
    pub const PARTS_DEFAULT: PanelPlace = PanelPlace {
        mode: PanelMode::Floating,
        x: 100_000.,
        y: 16.,
        collapsed: true,
    };

    /// `"<floating|left|right> <x> <y> <open|collapsed>"`, with x and y rounded.
    pub fn to_setting(&self) -> String {
        let mode = match self.mode {
            PanelMode::Floating => "floating",
            PanelMode::DockLeft => "left",
            PanelMode::DockRight => "right",
        };
        let state = if self.collapsed { "collapsed" } else { "open" };
        format!("{mode} {} {} {state}", self.x.round(), self.y.round())
    }

    /// Reads `to_setting`'s text back; anything but four valid words with finite numbers is `None`.
    pub fn from_setting(text: &str) -> Option<PanelPlace> {
        let [mode, x, y, state] = text.split_whitespace().collect::<Vec<_>>()[..] else {
            return None;
        };
        let mode = match mode {
            "floating" => PanelMode::Floating,
            "left" => PanelMode::DockLeft,
            "right" => PanelMode::DockRight,
            _ => return None,
        };
        let number = |w: &str| w.parse::<f32>().ok().filter(|v| v.is_finite());
        let collapsed = match state {
            "open" => false,
            "collapsed" => true,
            _ => return None,
        };
        Some(PanelPlace {
            mode,
            x: number(x)?,
            y: number(y)?,
            collapsed,
        })
    }

    /// This place with its offset moved so a `panel` of that size stays inside `area`.
    pub fn clamp(self, area: Size<Pixels>, panel: Size<Pixels>) -> PanelPlace {
        let room = |a: Pixels, p: Pixels| f32::from(a - p).max(0.);
        PanelPlace {
            x: self.x.clamp(0., room(area.width, panel.width)),
            y: self.y.clamp(0., room(area.height, panel.height)),
            ..self
        }
    }
}

/// Every panel's place and whether the toolbar is collapsed; the HUD Maker's one stored setting.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Places {
    pub options: PanelPlace,
    pub parts: PanelPlace,
    pub toolbar_collapsed: bool,
}

impl Places {
    /// Docked beside the canvas so it is never covered: the side panel open on the left, the
    /// part list a rail on the right. Floating starts from the panels' own default places.
    pub const DEFAULT: Places = Places {
        options: PanelPlace {
            mode: PanelMode::DockLeft,
            ..PanelPlace::DEFAULT
        },
        parts: PanelPlace {
            mode: PanelMode::DockRight,
            ..PanelPlace::PARTS_DEFAULT
        },
        toolbar_collapsed: false,
    };

    pub fn get(&self, panel: Panel) -> PanelPlace {
        match panel {
            Panel::Options => self.options,
            Panel::Parts => self.parts,
        }
    }

    pub fn set(&mut self, panel: Panel, place: PanelPlace) {
        match panel {
            Panel::Options => self.options = place,
            Panel::Parts => self.parts = place,
        }
    }

    /// `"<options place>; <parts place>; toolbar <open|collapsed>"`.
    pub fn to_setting(&self) -> String {
        let toolbar = if self.toolbar_collapsed {
            "collapsed"
        } else {
            "open"
        };
        format!(
            "{}; {}; toolbar {toolbar}",
            self.options.to_setting(),
            self.parts.to_setting()
        )
    }

    /// Reads `to_setting`'s text back. A one-place value from before the parts panel loads as
    /// the side panel; a damaged later segment falls back to its default, a damaged first is `None`.
    pub fn from_setting(text: &str) -> Option<Places> {
        // The side panel's place leads; the rest are found by shape, so a missing one is skipped.
        let mut segments = text.split(';').map(str::trim);
        let options = PanelPlace::from_setting(segments.next()?)?;
        let rest: Vec<&str> = segments.collect();
        let parts = (rest.iter().find_map(|s| PanelPlace::from_setting(s)))
            .unwrap_or(PanelPlace::PARTS_DEFAULT);
        let toolbar_collapsed = rest.contains(&"toolbar collapsed");
        Some(Places {
            options,
            parts,
            toolbar_collapsed,
        })
    }
}

/// Where a floating `panel` goes so it no longer covers `part` (both in canvas-area coordinates):
/// unchanged without overlap; else the nearest of left, right, above or below the part with an
/// 8 px gap that fits `area`; unchanged when none fits.
pub fn avoid(panel: Bounds<Pixels>, part: Bounds<Pixels>, area: Size<Pixels>) -> Point<Pixels> {
    if !panel.intersects(&part) {
        return panel.origin;
    }
    let here = panel.origin;
    let distance = |at: &Point<Pixels>| f32::from((at.x - here.x).abs() + (at.y - here.y).abs());
    // `min_by` keeps the first of equals, so ties go left, right, above, below.
    sides(panel, part)
        .into_iter()
        .filter(|at| fits(*at, panel.size, area))
        .min_by(|a, b| distance(a).total_cmp(&distance(b)))
        .unwrap_or(here)
}

/// How a panel's height follows where it stands: its bottom stays within `share` of the area's
/// height unless that leaves it shorter than `min`, and it is never taller than `max`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeightCap {
    pub share: f32,
    pub min: Pixels,
    pub max: Pixels,
}

impl HeightCap {
    /// The tallest the panel may be with its top at `y` in an area `area` high.
    pub fn at(self, area: Pixels, y: Pixels) -> Pixels {
        (area * self.share - y).max(self.min).min(self.max)
    }
}

/// One floating panel to settle: its home in area coordinates at its uncapped size, its height
/// cap, its size folded to its header (`None` when it can't fold), the height of its title bar,
/// which no other panel may cover, whether, unable to clear the part or fold, it takes the open
/// spot overlapping the part least, whether it folds before moving off a part under it, and
/// where it was drawn last, kept among equal spots, and a box it keeps off as it does the part.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Floating {
    pub home: Bounds<Pixels>,
    pub cap: Option<HeightCap>,
    pub header: Option<Size<Pixels>>,
    pub title: Pixels,
    pub spill: bool,
    pub fold_first: bool,
    pub shown: Option<Point<Pixels>>,
    pub avoid: [Option<Bounds<Pixels>>; 2],
}

impl Floating {
    /// A thing of `home`'s fixed size that never folds or spills.
    pub fn fixed(home: Bounds<Pixels>) -> Floating {
        Floating {
            home,
            cap: None,
            header: None,
            title: home.size.height,
            spill: false,
            fold_first: false,
            shown: None,
            avoid: [None, None],
        }
    }

    /// The open size with the top at `y` in an area of `area`.
    pub fn size_at(&self, area: Size<Pixels>, y: Pixels) -> Size<Pixels> {
        let tall = match self.cap {
            Some(cap) => self.home.size.height.min(cap.at(area.height, y)),
            None => self.home.size.height,
        };
        size(self.home.size.width, tall)
    }
}

/// Where a floating thing is drawn this frame, and whether it shows only its header.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settled {
    pub origin: Point<Pixels>,
    pub collapsed: bool,
}

/// `settle_folding` for things that never collapse, returning only where each goes.
pub fn settle(
    homes: &[Bounds<Pixels>],
    fixed: &[Bounds<Pixels>],
    part: Option<Bounds<Pixels>>,
    area: Size<Pixels>,
) -> Vec<Point<Pixels>> {
    let things: Vec<Floating> = homes.iter().map(|&home| Floating::fixed(home)).collect();
    (settle_folding(&things, fixed, part, area).into_iter())
        .map(|s| s.origin)
        .collect()
}

/// `settle_shortening` without the shortened heights.
pub fn settle_folding(
    things: &[Floating],
    fixed: &[Bounds<Pixels>],
    part: Option<Bounds<Pixels>>,
    area: Size<Pixels>,
) -> Vec<Settled> {
    (settle_shortening(things, fixed, part, area).into_iter())
        .map(|(settled, _)| settled)
        .collect()
}

/// Where each thing goes, in order: home if clear; else the nearest clear spot, for a `spill`
/// thing the nearest spot clear once shortened (its height returned), a spot clear of the part
/// on another's body (all skipped by a `fold_first` thing over the part), its folded header, the
/// spot overlapping the part least (`spill`), or the older fallbacks. Sizes follow each spot's
/// height cap, so a second run agrees.
pub fn settle_shortening(
    things: &[Floating],
    fixed: &[Bounds<Pixels>],
    part: Option<Bounds<Pixels>>,
    area: Size<Pixels>,
) -> Vec<(Settled, Option<Pixels>)> {
    // Earlier things are obstacles where they settled, later ones at home; working from homes
    // every frame means nothing chases anything.
    let mut placed: Vec<Bounds<Pixels>> = (things.iter())
        .map(|t| Bounds::new(t.home.origin, t.size_at(area, t.home.origin.y)))
        .collect();
    let mut out: Vec<Settled> = (placed.iter())
        .map(|b| Settled {
            origin: b.origin,
            collapsed: false,
        })
        .collect();
    let mut short: Vec<Option<Pixels>> = vec![None; things.len()];
    for (i, thing) in things.iter().enumerate() {
        let home = placed[i];
        let blocked = part.is_some_and(|p| home.intersects(&p))
            || thing.avoid.iter().flatten().any(|a| home.intersects(a))
            || fixed.iter().chain(&placed[..i]).any(|o| home.intersects(o));
        if !blocked {
            continue;
        }
        let others: Vec<Bounds<Pixels>> = (fixed.iter().copied())
            .chain(
                placed
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .map(|(_, b)| *b),
            )
            .chain(thing.avoid.into_iter().flatten())
            .collect();
        let obstacles: Vec<Bounds<Pixels>> = part.into_iter().chain(others.clone()).collect();
        let titles = (placed.iter().zip(things).enumerate())
            .filter(|(j, _)| *j != i)
            .map(|(_, (b, t))| {
                Bounds::new(b.origin, size(b.size.width, t.title.min(b.size.height)))
            });
        // The avoided boxes come last, so the spill's `[1..]` still drops exactly the part.
        let off_part: Vec<Bounds<Pixels>> = (part.into_iter())
            .chain(fixed.iter().copied())
            .chain(titles)
            .chain(thing.avoid.into_iter().flatten())
            .collect();
        let open = |y: Pixels| thing.size_at(area, y);
        let shown = thing.shown;
        let spot = |sized: &dyn Fn(Pixels) -> Size<Pixels>, keep_off: &[Bounds<Pixels>]| {
            nearest_clear(home, sized, shown, &obstacles, keep_off, area)
        };
        let boxed = |at: Point<Pixels>, sized: &dyn Fn(Pixels) -> Size<Pixels>| {
            Bounds::new(at, sized(at.y))
        };
        // A thing that folds first stays where it stands, showing its header, over moving open.
        let fold_here =
            thing.fold_first && thing.header.is_some() && part.is_some_and(|p| home.intersects(&p));
        let (bounds, collapsed) = (!fold_here)
            .then(|| spot(&open, &obstacles))
            .flatten()
            .map(|at| (boxed(at, &open), false))
            .or_else(|| {
                // A side panel too tall for every clear spot stands shorter, its body scrolling,
                // before it settles on another panel's body and pushes that panel aside.
                let cap = thing.cap.filter(|_| thing.spill && !fold_here)?;
                let b = shortened(home, &open, shown, cap.min, &obstacles, area)?;
                short[i] = Some(b.size.height);
                Some((b, false))
            })
            .or_else(|| {
                part.filter(|_| !fold_here)
                    .and_then(|_| spot(&open, &off_part))
                    .map(|at| (boxed(at, &open), false))
            })
            .or_else(|| {
                // Only the selection folds a panel; without one it keeps the older fallback.
                let header = thing.header.filter(|_| part.is_some())?;
                let folded = |_: Pixels| header;
                let at = spot(&folded, &obstacles)
                    .or_else(|| spot(&folded, &off_part))
                    .or_else(|| spot(&folded, &others))
                    .unwrap_or_else(|| match part {
                        Some(p) => avoid(Bounds::new(home.origin, header), p, area),
                        None => home.origin,
                    });
                Some((Bounds::new(at, header), true))
            })
            .or_else(|| {
                let part = part.filter(|_| thing.spill)?;
                let at = least_over(home, &open, shown, part, &off_part[1..], area);
                Some((boxed(at, &open), false))
            })
            .unwrap_or_else(|| {
                let at = (!others.is_empty())
                    .then(|| spot(&open, &others))
                    .flatten()
                    .unwrap_or_else(|| match part {
                        Some(p) => avoid(home, p, area),
                        None => home.origin,
                    });
                (boxed(at, &open), false)
            });
        placed[i] = bounds;
        out[i] = Settled {
            origin: bounds.origin,
            collapsed,
        };
    }
    out.into_iter().zip(short).collect()
}

// The space a shortened thing keeps from what lies below it, as `sides` keeps beside one.
const SHORT_GAP: Pixels = px(8.);

// The nearest spot, as `nearest_clear` weighs it, where a box sized by `sized` and cut short
// above whatever it would reach below stays at least `min` tall and touches none of
// `obstacles`; `None` when no spot does.
fn shortened(
    home: Bounds<Pixels>,
    sized: &dyn Fn(Pixels) -> Size<Pixels>,
    shown: Option<Point<Pixels>>,
    min: Pixels,
    obstacles: &[Bounds<Pixels>],
    area: Size<Pixels>,
) -> Option<Bounds<Pixels>> {
    let cut = |at: Point<Pixels>| {
        let full = sized(at.y);
        let mut tall = full.height;
        for o in obstacles {
            let across = o.left() < at.x + full.width && o.right() > at.x && o.bottom() > at.y;
            if !across {
                continue;
            }
            // Something already level with the top can't be cleared by shortening.
            if o.top() <= at.y {
                return None;
            }
            tall = tall.min(o.top() - SHORT_GAP - at.y);
        }
        let b = Bounds::new(at, size(full.width, tall));
        (tall >= min && clear(at, b.size, obstacles, area)).then_some(b)
    };
    let from = Bounds::new(home.origin, sized(home.origin.y));
    let cost = |b: &Bounds<Pixels>| {
        let at = b.origin;
        let distance = f32::from((at.x - home.origin.x).abs() + (at.y - home.origin.y).abs());
        (distance, Some(at) != shown)
    };
    candidates(from, shown, obstacles, &[home.origin])
        .into_iter()
        .filter_map(cut)
        .min_by(|a, b| {
            let ((da, sa), (db, sb)) = (cost(a), cost(b));
            da.total_cmp(&db).then(sa.cmp(&sb))
        })
}

// Every pairing of an x and a y taken from home, the last drawn spot or the side of an
// obstacle, so corner spots beside two obstacles at once are tried too.
fn candidates(
    home: Bounds<Pixels>,
    shown: Option<Point<Pixels>>,
    obstacles: &[Bounds<Pixels>],
    extra: &[Point<Pixels>],
) -> Vec<Point<Pixels>> {
    let (xs, ys): (Vec<Pixels>, Vec<Pixels>) = (obstacles.iter())
        .flat_map(|o| sides(home, *o))
        .chain(shown)
        .chain(extra.iter().copied())
        .map(|at| (at.x, at.y))
        .unzip();
    xs.iter()
        .flat_map(|&x| ys.iter().map(move |&y| point(x, y)))
        .collect()
}

// Whether a box of `size` at `at` fits `area` and touches none of `keep_off`.
fn clear(
    at: Point<Pixels>,
    size: Size<Pixels>,
    keep_off: &[Bounds<Pixels>],
    area: Size<Pixels>,
) -> bool {
    let moved = Bounds::new(at, size);
    fits(at, size, area) && keep_off.iter().all(|o| !moved.intersects(o))
}

// The clear spot nearest `home`'s origin for a box sized by `sized` at each spot's top; among
// equally near spots, the last drawn one.
fn nearest_clear(
    home: Bounds<Pixels>,
    sized: &dyn Fn(Pixels) -> Size<Pixels>,
    shown: Option<Point<Pixels>>,
    obstacles: &[Bounds<Pixels>],
    keep_off: &[Bounds<Pixels>],
    area: Size<Pixels>,
) -> Option<Point<Pixels>> {
    let from = Bounds::new(home.origin, sized(home.origin.y));
    let cost = |at: &Point<Pixels>| {
        let distance = f32::from((at.x - home.origin.x).abs() + (at.y - home.origin.y).abs());
        (distance, Some(*at) != shown)
    };
    candidates(from, shown, obstacles, &[home.origin])
        .into_iter()
        .filter(|&at| clear(at, sized(at.y), keep_off, area))
        .min_by(|a, b| {
            let ((da, sa), (db, sb)) = (cost(a), cost(b));
            da.total_cmp(&db).then(sa.cmp(&sb))
        })
}

/// Where a thing sized by `sized` at each spot's top covers the least of `drawn` (what the
/// canvas draws), counted at its size at `home` so a shorter spot gains nothing; with `screen`,
/// then the least of the screen; among spots that fit `area` and touch none of `keep_off`, the
/// one nearest `home`, then the last drawn one. A home over nothing drawn, or no better spot,
/// keeps `home`.
pub fn least_covering(
    home: Point<Pixels>,
    sized: &dyn Fn(Pixels) -> Size<Pixels>,
    shown: Option<Point<Pixels>>,
    drawn: &[Bounds<Pixels>],
    screen: Option<Bounds<Pixels>>,
    keep_off: &[Bounds<Pixels>],
    area: Size<Pixels>,
) -> Point<Pixels> {
    covering(true, home, sized, shown, drawn, screen, keep_off, area)
}

/// As `least_covering`, but `home` itself counts only when it touches none of `keep_off`, so a
/// thing on a kept-off box moves off it to the best clear spot; `home` when there is none.
pub fn least_covering_off(
    home: Point<Pixels>,
    sized: &dyn Fn(Pixels) -> Size<Pixels>,
    shown: Option<Point<Pixels>>,
    drawn: &[Bounds<Pixels>],
    screen: Option<Bounds<Pixels>>,
    keep_off: &[Bounds<Pixels>],
    area: Size<Pixels>,
) -> Point<Pixels> {
    covering(false, home, sized, shown, drawn, screen, keep_off, area)
}

// `least_covering`, where `home_stays` lets `home` stand even on a kept-off box.
#[allow(clippy::too_many_arguments)]
fn covering(
    home_stays: bool,
    home: Point<Pixels>,
    sized: &dyn Fn(Pixels) -> Size<Pixels>,
    shown: Option<Point<Pixels>>,
    drawn: &[Bounds<Pixels>],
    screen: Option<Bounds<Pixels>>,
    keep_off: &[Bounds<Pixels>],
    area: Size<Pixels>,
) -> Point<Pixels> {
    let at_home = sized(home.y);
    let home_ok = home_stays || clear(home, at_home, keep_off, area);
    let overlap = |b: Bounds<Pixels>, o: &Bounds<Pixels>| {
        let over = b.intersect(o);
        f32::from(over.size.width).max(0.) * f32::from(over.size.height).max(0.)
    };
    let cost = |at: &Point<Pixels>| {
        let b = Bounds::new(*at, at_home);
        let covered: f32 = drawn.iter().map(|d| overlap(b, d)).sum();
        let on_screen = screen.map_or(0., |s| overlap(b, &s));
        let distance = f32::from((at.x - home.x).abs() + (at.y - home.y).abs());
        (covered, on_screen, distance, Some(*at) != shown)
    };
    if home_ok && cost(&home).0 <= 0. {
        return home;
    }
    // Every spot beside a drawn part, the screen or a kept-off box, plus the area's corners.
    let (w, h) = (at_home.width, at_home.height);
    let edges = [
        home,
        point(px(0.), px(0.)),
        point(area.width - w, px(0.)),
        point(px(0.), area.height - h),
        point(area.width - w, area.height - h),
    ];
    let obstacles: Vec<Bounds<Pixels>> = (drawn.iter().copied())
        .chain(screen)
        .chain(keep_off.iter().copied())
        .collect();
    candidates(Bounds::new(home, at_home), shown, &obstacles, &edges)
        .into_iter()
        .filter(|&at| (home_ok && at == home) || clear(at, sized(at.y), keep_off, area))
        .min_by(|a, b| {
            let ((ca, ra, da, sa), (cb, rb, db, sb)) = (cost(a), cost(b));
            (ca.total_cmp(&cb))
                .then(ra.total_cmp(&rb))
                .then(da.total_cmp(&db))
                .then(sa.cmp(&sb))
        })
        .unwrap_or(home)
}

// The spot for `home` that fits `area`, touches none of `keep_off` and overlaps `part` least;
// among equals the last drawn spot, then the one nearest home; home when none fits.
fn least_over(
    home: Bounds<Pixels>,
    sized: &dyn Fn(Pixels) -> Size<Pixels>,
    shown: Option<Point<Pixels>>,
    part: Bounds<Pixels>,
    keep_off: &[Bounds<Pixels>],
    area: Size<Pixels>,
) -> Point<Pixels> {
    let from = Bounds::new(home.origin, sized(home.origin.y));
    let (w, h) = (from.size.width, from.size.height);
    let edges = [
        home.origin,
        point(px(0.), px(0.)),
        point(area.width - w, area.height - h),
    ];
    let obstacles: Vec<Bounds<Pixels>> = std::iter::once(part)
        .chain(keep_off.iter().copied())
        .collect();
    let cost = |at: &Point<Pixels>| {
        let over = Bounds::new(*at, sized(at.y)).intersect(&part);
        let covered = f32::from(over.size.width).max(0.) * f32::from(over.size.height).max(0.);
        let distance = f32::from((at.x - home.origin.x).abs() + (at.y - home.origin.y).abs());
        (covered, Some(*at) != shown, distance)
    };
    candidates(from, shown, &obstacles, &edges)
        .into_iter()
        .filter(|&at| clear(at, sized(at.y), keep_off, area))
        .min_by(|a, b| {
            let ((ca, sa, da), (cb, sb, db)) = (cost(a), cost(b));
            ca.total_cmp(&cb).then(sa.cmp(&sb)).then(da.total_cmp(&db))
        })
        .unwrap_or(home.origin)
}

/// Left of, right of, above and below `obstacle` with an 8 px gap, keeping `panel`'s other coordinate.
fn sides(panel: Bounds<Pixels>, obstacle: Bounds<Pixels>) -> [Point<Pixels>; 4] {
    const GAP: Pixels = px(8.);
    let (here, size) = (panel.origin, panel.size);
    [
        point(obstacle.left() - GAP - size.width, here.y),
        point(obstacle.right() + GAP, here.y),
        point(here.x, obstacle.top() - GAP - size.height),
        point(here.x, obstacle.bottom() + GAP),
    ]
}

/// Whether a box of `size` at `at` lies inside `area`.
fn fits(at: Point<Pixels>, size: Size<Pixels>, area: Size<Pixels>) -> bool {
    at.x >= px(0.)
        && at.y >= px(0.)
        && at.x + size.width <= area.width
        && at.y + size.height <= area.height
}
