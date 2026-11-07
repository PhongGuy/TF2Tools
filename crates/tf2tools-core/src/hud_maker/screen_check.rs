//! Every screen shape at once: the HUD laid out on each preset, with the drawn parts that leave
//! the screen or overlap an unrelated drawn part, judged against the HUD as it was opened.

use std::collections::{HashMap, HashSet};

use super::text_wrap::{wrap_lines, wrapped_text_left};
use super::{ALIGNMENTS, GameText, HiddenBy, HudLayout, HudModel, PartKey, Rect, Screen};

/// The keys whose presence makes a part paint something.
const PAINTS: [&str; 4] = ["labelText", "image", "bgcolor_override", "fillcolor"];

/// What one screen shape does to one drawn part.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ScreenProblem {
    /// The part's box is not wholly on the screen.
    LeavesScreen,
    /// The part's box shares area with this other drawn part, neither enclosing the other.
    Overlaps(PartKey),
}

/// One flag on one part.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ScreenFlag {
    part: PartKey,
    problem: ScreenProblem,
}

impl ScreenFlag {
    /// The flagged part.
    pub fn part(&self) -> &PartKey {
        &self.part
    }

    /// What the shape does to it.
    pub fn problem(&self) -> &ScreenProblem {
        &self.problem
    }
}

/// One screen shape's layout, its drawn parts and its flags.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ScreenCheck {
    layout: HudLayout,
    drawn: Vec<PartKey>,
    flags: Vec<ScreenFlag>,
    too_crowded: bool,
}

impl ScreenCheck {
    /// The screen shape checked.
    pub fn screen(&self) -> Screen {
        self.layout.screen()
    }

    /// The layout the check read; the view draws from it without laying out again.
    pub fn layout(&self) -> &HudLayout {
        &self.layout
    }

    /// The drawn parts visible on this shape, in part order.
    pub fn drawn(&self) -> &[PartKey] {
        &self.drawn
    }

    /// Every leaving part in part order, then each overlapping pair: a part leaving on some
    /// shapes but not all or that the HUD as opened kept on this one, and an overlap the HUD as
    /// opened did not have on this shape.
    pub fn flags(&self) -> &[ScreenFlag] {
        &self.flags
    }

    /// Whether this shape has too many overlaps to judge, so none are flagged.
    pub fn too_crowded(&self) -> bool {
        self.too_crowded
    }
}

/// The problems of a HUD as opened, on each shape: what `check_screens` excuses.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ScreenBaseline {
    shapes: Vec<Problems>,
    off_every: HashSet<PartKey>,
}

/// A glyph's mean advance as a share of the font's height, for estimating a label's text width.
const GLYPH_WIDTH: f64 = 0.55;

/// Past this many overlapping pairs on one shape, the shape is too crowded to judge overlaps,
/// so a pile of parts costs a bounded pass instead of every pair.
const MAX_PAIRS: usize = 4096;

/// One shape's problems, keyed by part so a model with parts added or removed compares cleanly.
#[derive(Debug, Clone, Default)]
struct Problems {
    leaving: HashSet<PartKey>,
    /// Each pair with its keys in sorted order; `None` when the shape is too crowded to judge.
    pairs: Option<HashSet<(PartKey, PartKey)>>,
}

/// One shape's problems by part index, with the layout they were read from.
struct Found {
    layout: HudLayout,
    drawn: Vec<usize>,
    leaving: Vec<usize>,
    /// Overlapping pairs, earlier part first, in part order; `None` past `MAX_PAIRS`.
    pairs: Option<Vec<(usize, usize)>>,
}

impl HudModel {
    /// This model's problems on each of `Screen::PRESETS`; call it on the pristine model.
    /// `text` is the game's text, which `#token` labels draw.
    pub fn screen_baseline(&self, text: &GameText) -> ScreenBaseline {
        let found = self.find_problems(text);
        let parts = self.parts();
        let off_every = (0..parts.len())
            .filter(|&i| {
                (found.iter()).all(|f| f.layout.boxes()[i].hidden() == Some(HiddenBy::OffScreen))
            })
            .map(|i| parts[i].key().clone())
            .collect();
        let shapes = found
            .iter()
            .map(|f| Problems {
                leaving: f.leaving.iter().map(|&i| parts[i].key().clone()).collect(),
                pairs: (f.pairs.as_ref())
                    .map(|pairs| pairs.iter().map(|&(a, b)| sorted(parts, a, b)).collect()),
            })
            .collect();
        ScreenBaseline { shapes, off_every }
    }

    /// Lays the model out once on each of `Screen::PRESETS` and flags against `opened`: a part
    /// leaving on some shapes but not all, or that `opened` kept on that shape, unless `opened`
    /// has it off every shape; and an overlap `opened` lacks on that shape, unless either is too
    /// crowded to judge. A label leaves only by the text it draws (with `text` and the scene's
    /// sample filled in), so one that draws nothing never leaves. One check per preset.
    pub fn check_screens(&self, opened: &ScreenBaseline, text: &GameText) -> Vec<ScreenCheck> {
        let found = self.find_problems(text);
        let parts = self.parts();
        // By index, so "on every shape" is one count per part.
        let mut leaving_on = vec![0usize; parts.len()];
        for &i in found.iter().flat_map(|f| &f.leaving) {
            leaving_on[i] += 1;
        }
        let flags: Vec<Vec<ScreenFlag>> = (found.iter().enumerate())
            .map(|(s, f)| {
                let base = opened.shapes.get(s);
                let mut flags = Vec::new();
                for &i in &f.leaving {
                    let part = parts[i].key();
                    if opened.off_every.contains(part) {
                        continue;
                    }
                    let shipped = base.is_some_and(|b| b.leaving.contains(part));
                    if !shipped || leaving_on[i] < found.len() {
                        flags.push(ScreenFlag {
                            part: part.clone(),
                            problem: ScreenProblem::LeavesScreen,
                        });
                    }
                }
                for pair in f.pairs.iter().flatten() {
                    let shipped = base.is_some_and(|b| {
                        (b.pairs.as_ref())
                            .is_none_or(|p| p.contains(&sorted(parts, pair.0, pair.1)))
                    });
                    if !shipped {
                        let (a, b) = (parts[pair.0].key(), parts[pair.1].key());
                        flags.push(overlap(a, b));
                        flags.push(overlap(b, a));
                    }
                }
                flags
            })
            .collect();
        (found.into_iter().zip(flags))
            .map(|(f, flags)| ScreenCheck {
                drawn: f.drawn.iter().map(|&i| parts[i].key().clone()).collect(),
                too_crowded: f.pairs.is_none(),
                layout: f.layout,
                flags,
            })
            .collect()
    }

    /// Every preset's layout with its leaving parts and overlapping pairs, by part index.
    fn find_problems(&self, text: &GameText) -> Vec<Found> {
        let parts = self.parts();
        let painted: Vec<bool> = (parts.iter())
            .map(|p| PAINTS.iter().any(|k| p.value(k).is_some()))
            .collect();
        // A label that paints nothing but text leaves by that text, as the preview draws it
        // (a game-filled `%token%` with no sample is empty), or None.
        let sample = self.scene_rule().sample();
        let shown: Vec<Option<String>> = (parts.iter())
            .map(|p| shown_text(p, text, sample))
            .collect();
        let index: HashMap<&PartKey, usize> = (parts.iter().enumerate())
            .map(|(i, p)| (p.key(), i))
            .collect();
        let parents: Vec<Option<usize>> = (parts.iter())
            .map(|p| p.parent().and_then(|k| index.get(k).copied()))
            .collect();
        Screen::PRESETS
            .iter()
            .map(|&screen| {
                let layout = self.layout(screen);
                let boxes = layout.boxes();
                let rects: Vec<Edges> = boxes.iter().map(rect).collect();
                let (sw, sh) = (i64::from(screen.wide()), i64::from(screen.tall()));
                let drawn: Vec<usize> = (0..boxes.len())
                    .filter(|&i| painted[i] && boxes[i].hidden().is_none())
                    .collect();
                let leaving = (0..boxes.len())
                    .filter(|&i| painted[i] && !shown[i].as_deref().is_some_and(blank))
                    .filter(|&i| match boxes[i].hidden() {
                        None => {
                            let drawn = match &shown[i] {
                                Some(text) => self.text_rect(&parts[i], &boxes[i], text, screen),
                                None => rects[i],
                            };
                            !inside(drawn, sw, sh)
                        }
                        Some(h) => h == HiddenBy::OffScreen,
                    })
                    .collect();
                let pairs = overlapping(&drawn, &rects, &parents);
                Found {
                    layout,
                    drawn,
                    leaving,
                    pairs,
                }
            })
            .collect()
    }
}

impl HudModel {
    /// The area `key` draws in `layout` in screen pixels: a label that paints
    /// nothing but text covers its text's estimated extent, any other part its whole box.
    /// `None` when `layout` has no box for `key`.
    pub fn text_extent(&self, layout: &HudLayout, key: &PartKey, text: &GameText) -> Option<Rect> {
        let b = layout.get(key)?;
        let part = self.part(key)?;
        let sample = self.scene_rule().sample();
        let (l, t, r, bottom) = match shown_text(part, text, sample) {
            Some(shown) if !blank(&shown) => self.text_rect(part, b, &shown, layout.screen()),
            _ => rect(b),
        };
        // Clipped to the box, so each side fits an i32 as the box's own does.
        let side = |v: i64| i32::try_from(v).unwrap_or(i32::MAX);
        Some(Rect::new(side(l), side(t), side(r - l), side(bottom - t)))
    }

    /// The text inset of the part at `key` as (x, y) screen pixels: `textinsetx` and
    /// `textinsety`, scaled from proportional units when `use_proportional_insets` is on (y is
    /// read as a decimal and rounded up). The x inset moves text in from the side it is aligned
    /// to, west or east; the y inset moves text down whatever its alignment.
    pub fn text_inset(&self, key: &PartKey, screen: Screen) -> (i32, i32) {
        let Some(part) = self.part(key) else {
            return (0, 0);
        };
        let bytes = |k: &str| part.value(k).map(|v| v.text().as_bytes());
        let int = |k: &str| bytes(k).map_or(0, super::expr::atoi);
        let (x, y) = (int("textinsetx"), int("textinsety"));
        if int("use_proportional_insets") <= 0 {
            return (x, y);
        }
        let scale = super::ScreenScale::new(screen.tall());
        let y = bytes("textinsety").map_or(0., super::expr::atof);
        // A float to int cast saturates, so a huge inset stays in range.
        let y = (f64::from(y) * f64::from(scale.scale(1000)) / 1000.).ceil() as i32;
        (scale.scale(x), y)
    }

    /// Where a label's `text` lands, estimated from its font's height and placed by its
    /// `textAlignment` and text inset, clipped to its box as the label clips it. With `wrap` or
    /// `centerwrap` on, it is the lines the game breaks it into at that estimate.
    fn text_rect(
        &self,
        part: &super::Part,
        b: &super::PartBox,
        text: &str,
        screen: Screen,
    ) -> Edges {
        let (l, t, r, bottom) = rect(b);
        let font = (part.value("font"))
            .and_then(|f| self.scheme().font(f.text(), screen.tall()))
            .map(|f| i64::from(f.pixel_tall(screen.tall())));
        // An unknown font draws at 60 % of the box, as the preview does.
        let tall = font.unwrap_or((bottom - t) * 3 / 5);
        let glyph = tall as f64 * GLYPH_WIDTH;
        let (inset_x, iy) = self.text_inset(part.key(), screen);
        let (ix, iy) = (i64::from(inset_x), i64::from(iy));
        let centre_lines = switched_on(part, "centerwrap");
        let wrapped = centre_lines || switched_on(part, "wrap");
        let lines: Vec<&str> = if wrapped {
            let room = (r - l).saturating_sub(ix) as f32;
            let lines = wrap_lines(text, room, |_| glyph as f32);
            lines.into_iter().map(str::trim_end).collect()
        } else {
            text.trim().lines().collect()
        };
        let chars = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
        let wide = (chars as f64 * glyph).ceil() as i64;
        let high = tall.saturating_mul(i64::try_from(lines.len()).unwrap_or(i64::MAX));
        let align = part.value("textAlignment").map_or("", |a| a.text());
        let (column, row) = (ALIGNMENTS.iter())
            .position(|a| a.eq_ignore_ascii_case(align))
            .map_or((0, 1), |i| (i % 3, i / 3));
        // The text draws at most the box's width less the x inset, whatever its alignment.
        // Text still wider than its box starts at the west edge, an east label then moved
        // back by its inset.
        let wide = wide.min((r - l).saturating_sub(ix)).max(0);
        let x = match column {
            _ if wrapped => {
                // Each centred line sits in the width left by the inset, so the widest one
                // bounds them all.
                let indent = if centre_lines {
                    (r - l).saturating_sub(ix).saturating_sub(wide) / 2
                } else {
                    0
                };
                l.saturating_add(i64::from(wrapped_text_left(column, inset_x)))
                    .saturating_add(indent)
            }
            0 => l.saturating_add(ix),
            1 if wide > r - l => l,
            _ if wide > r - l => l.saturating_sub(ix),
            1 => l + (r - l - wide) / 2,
            _ => (r - wide).saturating_sub(ix),
        };
        let y = match row {
            0 => t,
            2 => bottom.saturating_sub(high),
            _ => t + (bottom - t).saturating_sub(high) / 2,
        }
        .saturating_add(iy);
        // The label clips what an inset pushes out, so the rect stays inside the box and is
        // never less than empty.
        let (x0, y0) = (x.clamp(l, r), y.clamp(t, bottom));
        (
            x0,
            y0,
            x.saturating_add(wide).clamp(x0, r),
            y.saturating_add(high).clamp(y0, bottom),
        )
    }
}

/// A label that paints nothing but text: the text it shows with `text` and `sample` filled in;
/// a game-filled `%token%` with no sample is empty.
fn shown_text(part: &super::Part, text: &GameText, sample: super::SampleSet) -> Option<String> {
    let label = part.value("labelText")?;
    if PAINTS
        .iter()
        .any(|k| *k != "labelText" && part.value(k).is_some())
    {
        return None;
    }
    Some(sample.fill(&text.show(label.text())).replace("\\n", "\n"))
}

/// Whether shown text draws nothing at all.
fn blank(text: &str) -> bool {
    text.trim().is_empty()
}

/// Whether a 0/1 key is on, read as the label reads it: a number above zero.
fn switched_on(part: &super::Part, key: &str) -> bool {
    part.value(key)
        .is_some_and(|v| super::expr::atoi(v.text().as_bytes()) > 0)
}

/// A box as (left, top, right, bottom) in `i64`, so huge boxes never overflow.
type Edges = (i64, i64, i64, i64);

/// The overlapping pairs among `drawn`, neither under the other, sorted; `None` past
/// `MAX_PAIRS`. A sweep by left edge compares only boxes whose x-ranges meet.
fn overlapping(
    drawn: &[usize],
    rects: &[Edges],
    parents: &[Option<usize>],
) -> Option<Vec<(usize, usize)>> {
    let mut order = drawn.to_vec();
    order.sort_by_key(|&i| rects[i].0);
    let mut pairs = Vec::new();
    for (n, &i) in order.iter().enumerate() {
        for &j in &order[n + 1..] {
            // Later boxes start further right, so none past this one meets `i` either.
            if rects[j].0 >= rects[i].2 {
                break;
            }
            if meets(&rects[i], &rects[j]) && !under(parents, i, j) && !under(parents, j, i) {
                if pairs.len() == MAX_PAIRS {
                    return None;
                }
                pairs.push((i.min(j), i.max(j)));
            }
        }
    }
    pairs.sort_unstable();
    Some(pairs)
}

/// Whether part `i` lies anywhere under part `ancestor`; a broken chain ends the walk.
fn under(parents: &[Option<usize>], i: usize, ancestor: usize) -> bool {
    let mut at = parents[i];
    for _ in 0..parents.len() {
        match at {
            Some(p) if p == ancestor => return true,
            Some(p) => at = parents[p],
            None => return false,
        }
    }
    false
}

fn overlap(part: &PartKey, other: &PartKey) -> ScreenFlag {
    ScreenFlag {
        part: part.clone(),
        problem: ScreenProblem::Overlaps(other.clone()),
    }
}

/// The keys of parts `a` and `b`, in sorted order.
fn sorted(parts: &[super::Part], a: usize, b: usize) -> (PartKey, PartKey) {
    let (a, b) = (parts[a].key(), parts[b].key());
    let (a, b) = if a.as_str() <= b.as_str() {
        (a, b)
    } else {
        (b, a)
    };
    (a.clone(), b.clone())
}

fn rect(b: &super::PartBox) -> Edges {
    let (x, y) = (i64::from(b.x()), i64::from(b.y()));
    (x, y, x + i64::from(b.wide()), y + i64::from(b.tall()))
}

fn inside((l, t, r, b): Edges, sw: i64, sh: i64) -> bool {
    l >= 0 && t >= 0 && r <= sw && b <= sh
}

/// Strict overlap: touching edges share no area.
fn meets(a: &Edges, b: &Edges) -> bool {
    a.0 < b.2 && b.0 < a.2 && a.1 < b.3 && b.1 < a.3
}
