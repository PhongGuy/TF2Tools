//! Stage 1 layout: each part's box in whole pixels on one screen shape, with pins, hiding and
//! hit-testing. Every probe-dependent choice goes through `probe`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use super::expr::atoi;
use super::probe::{self, HideHow};
use super::scene::Fit;
use super::{
    AxisSize, GameSituation, HudModel, Part, PartKey, PinCorner, Point, PosExpr, Rect, ScreenScale,
    Size, SizeExpr,
};

/// A screen shape in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Screen {
    wide: u32,
    tall: u32,
}

impl Screen {
    /// 1920x1080 (default), 1920x1200, 1440x1080, 1280x1024, 2560x1080, 3840x1080.
    pub const PRESETS: [Screen; 6] = [
        Screen::new(1920, 1080),
        Screen::new(1920, 1200),
        Screen::new(1440, 1080),
        Screen::new(1280, 1024),
        Screen::new(2560, 1080),
        Screen::new(3840, 1080),
    ];

    /// A screen `wide` x `tall` pixels.
    pub const fn new(wide: u32, tall: u32) -> Screen {
        Screen { wide, tall }
    }

    /// Width in pixels.
    pub fn wide(self) -> u32 {
        self.wide
    }

    /// Height in pixels.
    pub fn tall(self) -> u32 {
        self.tall
    }
}

/// Why a part is not drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum HiddenBy {
    /// `wide` or `tall` is at most 0.
    SizeZero,
    /// The box lies wholly outside the screen.
    OffScreen,
    /// `visible 0` (or another switch the probe says hides it).
    Visible,
    /// `alpha 0`.
    Alpha,
    /// An enclosing part is hidden.
    Parent,
    /// The game's code hides it in the situation the preview shows.
    Game,
}

/// One part's box on the screen.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PartBox {
    key: PartKey,
    x: i32,
    y: i32,
    wide: i32,
    tall: i32,
    z: i32,
    hidden: Option<HiddenBy>,
    pinned: bool,
    clickable: bool,
}

impl PartBox {
    /// The part's key.
    pub fn key(&self) -> &PartKey {
        &self.key
    }

    /// Absolute left edge in pixels.
    pub fn x(&self) -> i32 {
        self.x
    }

    /// Absolute top edge in pixels.
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

    /// The `zpos`; 0 when unset.
    pub fn z(&self) -> i32 {
        self.z
    }

    /// Why the part is not drawn, or `None` when it is.
    pub fn hidden(&self) -> Option<HiddenBy> {
        self.hidden
    }

    /// Whether the part sits pinned to a sibling.
    pub fn pinned(&self) -> bool {
        self.pinned
    }

    /// Whether a click may select it: anything but a bare full-screen panel.
    pub fn clickable(&self) -> bool {
        self.clickable
    }

    /// The box as one rectangle.
    pub fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, self.wide, self.tall)
    }

    fn contains(&self, x: i32, y: i32) -> bool {
        self.rect().contains(x, y)
    }
}

/// Every part's box for one screen shape.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct HudLayout {
    screen: Screen,
    boxes: Vec<PartBox>,
    parents: Vec<Option<usize>>,
    index: HashMap<PartKey, usize>,
    // Box indices back to front, worked out once per layout, and each box's place in it.
    paint: Vec<usize>,
    paint_rank: Vec<usize>,
    // Each box's nearest enclosing button, which the game treats as one control with it.
    buttons: Vec<Option<usize>>,
    generation: u64,
}

/// One part under a click, as `HudLayout::pick_ranked` ranks it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Pick {
    key: PartKey,
    draws: bool,
    rank: usize,
}

impl Pick {
    /// The part.
    pub fn key(&self) -> &PartKey {
        &self.key
    }

    /// Whether it counts as drawing there: it draws, or it is a button one of whose pieces
    /// under the click draws.
    pub fn draws(&self) -> bool {
        self.draws
    }

    /// Its place in the paint order, back to front; a button takes the place of its drawn
    /// piece on top.
    pub fn rank(&self) -> usize {
        self.rank
    }
}

// Hands every new layout its own generation.
static GENERATIONS: AtomicU64 = AtomicU64::new(0);

impl HudLayout {
    /// A number no other layout built in this process shares, so a caller can tell whether
    /// anything was laid out again since it last looked; a clone keeps it.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The screen shape laid out for.
    pub fn screen(&self) -> Screen {
        self.screen
    }

    /// Every box, in the model's part order.
    pub fn boxes(&self) -> &[PartBox] {
        &self.boxes
    }

    /// The box of `key`.
    pub fn get(&self, key: &PartKey) -> Option<&PartBox> {
        self.index.get(key).map(|&i| &self.boxes[i])
    }

    /// The part of `key`'s box the game can draw in: the box cut to
    /// every enclosing part's box and the screen, as the game clips a panel to its parent.
    /// The whole box when nothing of it would show; `None` when `key` has no box.
    pub fn visible(&self, key: &PartKey) -> Option<Rect> {
        self.index.get(key).map(|&i| self.visible_at(i))
    }

    /// The area of `key`'s box the game draws, cut as `visible` cuts it; `None` when `key` has
    /// no box or nothing of it would show.
    pub fn shown(&self, key: &PartKey) -> Option<Rect> {
        self.index.get(key).and_then(|&i| self.shown_at(i))
    }

    fn visible_at(&self, i: usize) -> Rect {
        self.shown_at(i).unwrap_or(self.boxes[i].rect())
    }

    fn shown_at(&self, i: usize) -> Option<Rect> {
        let edges = |b: &PartBox| {
            let (x, y) = (i64::from(b.x), i64::from(b.y));
            (x, y, x + i64::from(b.wide), y + i64::from(b.tall))
        };
        let own = edges(&self.boxes[i]);
        let screen = (
            0,
            0,
            i64::from(self.screen.wide),
            i64::from(self.screen.tall),
        );
        let mut cut = (
            own.0.max(0),
            own.1.max(0),
            own.2.min(screen.2),
            own.3.min(screen.3),
        );
        // Bounded by the part count, so a malformed parent chain can never loop forever.
        let mut at = self.parents[i];
        for _ in 0..self.boxes.len() {
            let Some(p) = at else { break };
            let e = edges(&self.boxes[p]);
            cut = (
                cut.0.max(e.0),
                cut.1.max(e.1),
                cut.2.min(e.2),
                cut.3.min(e.3),
            );
            at = self.parents[p];
        }
        if cut.0 >= cut.2 || cut.1 >= cut.3 {
            return None;
        }
        // Inside the box on every side, so each value fits an i32 as the box's own do.
        let fit = |v: i64| i32::try_from(v).unwrap_or(i32::MAX);
        Some(Rect::new(
            fit(cut.0),
            fit(cut.1),
            fit(cut.2 - cut.0),
            fit(cut.3 - cut.1),
        ))
    }

    /// Walks down from the top level: among siblings containing (x, y), the one painted on top
    /// first; returns the deepest visible clickable part of the first subtree that has one.
    pub fn hit(&self, x: i32, y: i32) -> Option<&PartKey> {
        self.hit_under(None, x, y).map(|i| &self.boxes[i].key)
    }

    /// Every part the canvas paints, back to front: parents before their children and, among
    /// siblings, lower `zpos` first, then part order, except that siblings which all set the
    /// same `zpos` paint in reverse part order.
    pub fn paint_order(&self) -> impl Iterator<Item = &PartBox> {
        self.paint.iter().map(|&i| &self.boxes[i])
    }

    /// The place of `key` in the paint order, back to front.
    pub fn paint_rank(&self, key: &PartKey) -> Option<usize> {
        self.index.get(key).map(|&i| self.paint_rank[i])
    }

    /// Every clickable part whose drawn area (see `HudLayout::shown`) holds (x, y) and that
    /// `accept` lets through, best first: parts that `draws` says paint something, the one
    /// painted on top first; then parts that paint nothing, the smallest box first. A button
    /// and its pieces are one control: when a piece there draws, the button counts as drawing
    /// at that piece's place and comes just before it. Empty when nothing there takes a click.
    pub fn pick(
        &self,
        x: i32,
        y: i32,
        accept: impl Fn(&PartBox) -> bool,
        draws: impl Fn(&PartKey) -> bool,
    ) -> Vec<&PartKey> {
        (self.ranked(x, y, accept, draws).into_iter())
            .map(|(i, _, _)| &self.boxes[i].key)
            .collect()
    }

    /// As `pick`, with whether each part counts as drawing and its paint place.
    pub fn pick_ranked(
        &self,
        x: i32,
        y: i32,
        accept: impl Fn(&PartBox) -> bool,
        draws: impl Fn(&PartKey) -> bool,
    ) -> Vec<Pick> {
        (self.ranked(x, y, accept, draws).into_iter())
            .map(|(i, draws, rank)| Pick {
                key: self.boxes[i].key.clone(),
                draws,
                rank,
            })
            .collect()
    }

    // The parts under (x, y) best first, as (box index, counts as drawing, paint place).
    fn ranked(
        &self,
        x: i32,
        y: i32,
        accept: impl Fn(&PartBox) -> bool,
        draws: impl Fn(&PartKey) -> bool,
    ) -> Vec<(usize, bool, usize)> {
        let area = |b: &PartBox| i64::from(b.wide.max(0)) * i64::from(b.tall.max(0));
        // A click lands only where the part paints, not where its parent cuts it off.
        let holds = |i: usize| self.shown_at(i).is_some_and(|shown| shown.contains(x, y));
        // (box index, counts as drawing, paint place, lifted over its piece)
        let mut under: Vec<(usize, bool, usize, bool)> = (self.paint.iter().enumerate())
            .filter(|&(_, &i)| {
                let b = &self.boxes[i];
                b.clickable && b.contains(x, y) && holds(i) && accept(b)
            })
            .map(|(rank, &i)| (i, draws(&self.boxes[i].key), rank, false))
            .collect();
        // A drawn piece lends its button its drawing and its place, so the button wins there.
        for n in 0..under.len() {
            let (piece, drawn, rank, _) = under[n];
            let button = self.buttons[piece].filter(|_| drawn);
            let Some(m) = under.iter().position(|u| Some(u.0) == button) else {
                continue;
            };
            if !under[m].1 || under[m].2 < rank {
                under[m] = (under[m].0, true, rank, true);
            }
        }
        under.sort_by(|a, b| {
            let (sa, sb) = (&self.boxes[a.0], &self.boxes[b.0]);
            match (a.1, b.1) {
                (true, true) => (b.2, b.3).cmp(&(a.2, a.3)).then(area(sa).cmp(&area(sb))),
                (false, false) => area(sa).cmp(&area(sb)).then(b.2.cmp(&a.2)),
                (drawn, _) => drawn.cmp(&b.1).reverse(),
            }
        });
        under.into_iter().map(|(i, d, r, _)| (i, d, r)).collect()
    }

    /// Whether `key` lies anywhere under `ancestor`.
    pub(super) fn is_under(&self, key: &PartKey, ancestor: &PartKey) -> bool {
        let mut at = self.index.get(key).and_then(|&i| self.parents[i]);
        while let Some(i) = at {
            if &self.boxes[i].key == ancestor {
                return true;
            }
            at = self.parents[i];
        }
        false
    }

    fn hit_under(&self, parent: Option<usize>, x: i32, y: i32) -> Option<usize> {
        let mut under: Vec<usize> = (0..self.boxes.len())
            .filter(|&i| self.parents[i] == parent)
            .filter(|&i| self.boxes[i].hidden.is_none() && self.boxes[i].contains(x, y))
            .collect();
        under.sort_by_key(|&i| std::cmp::Reverse(self.paint_rank[i]));
        under.into_iter().find_map(|i| {
            self.hit_under(Some(i), x, y)
                .or_else(|| self.boxes[i].clickable.then_some(i))
        })
    }
}

impl HudLayout {
    /// The same layout with each box in `rects` put at its rect, shifted by any moved
    /// ancestor, and every other box shifted with its nearest moved ancestor.
    pub(super) fn moved(&self, rects: &HashMap<PartKey, Rect>) -> HudLayout {
        let mut out = self.clone();
        let mut offset = vec![(0i32, 0i32); self.boxes.len()];
        // Parents come first, so each parent's offset is final when its children read it.
        for i in 0..self.boxes.len() {
            let (dx, dy) = self.parents[i].map_or((0, 0), |p| offset[p]);
            let b = &mut out.boxes[i];
            let (ox, oy) = (b.x, b.y);
            match rects.get(&b.key) {
                Some(r) => {
                    (b.x, b.y) = (r.x.saturating_add(dx), r.y.saturating_add(dy));
                    (b.wide, b.tall) = (r.wide, r.tall);
                }
                None => (b.x, b.y) = (ox.saturating_add(dx), oy.saturating_add(dy)),
            }
            offset[i] = (b.x.saturating_sub(ox), b.y.saturating_sub(oy));
        }
        out.generation = GENERATIONS.fetch_add(1, Ordering::Relaxed);
        out
    }
}

// The spectator target ID, the health panel and the team backgrounds its code resizes.
const SPEC_TARGET: &str = "CSpectatorTargetID";
const SPEC_TARGET_HEALTH: &str = "CSpectatorTargetID/SpectatorGUIHealth";
const SPEC_TARGET_BACKGROUNDS: [&str; 2] = [
    "CSpectatorTargetID/TargetIDBG_Spec_Red",
    "CSpectatorTargetID/TargetIDBG_Spec_Blue",
];

impl HudLayout {
    /// The layout with the spectator target ID sized around text `text_wide` screen pixels
    /// wide (the wider of its name and data lines), as `CSpectatorTargetID::PerformLayout`
    /// does: the health panel's width, XRES(15) and the text, centred across the screen, its
    /// team backgrounds as wide. Unchanged when the layout has no spectator target ID.
    pub fn target_id_around(&self, text_wide: i32) -> HudLayout {
        let (Some(root), Some(health)) = (
            self.get(&PartKey::new(SPEC_TARGET)),
            self.get(&PartKey::new(SPEC_TARGET_HEALTH)),
        ) else {
            return self.clone();
        };
        let screen_wide = clamp(self.screen.wide);
        let indents = i32::try_from(15 * i64::from(screen_wide) / 640).unwrap_or(i32::MAX);
        let wide = (health.wide.saturating_add(indents)).saturating_add(text_wide.max(0));
        let x = screen_wide.saturating_sub(wide) / 2;
        let mut rects = HashMap::from([(root.key.clone(), Rect::new(x, root.y, wide, root.tall))]);
        // The backgrounds' rects stand where they were; `moved` carries them with the root.
        for bg in SPEC_TARGET_BACKGROUNDS {
            if let Some(b) = self.get(&PartKey::new(bg)) {
                rects.insert(b.key.clone(), Rect::new(b.x, b.y, wide, b.tall));
            }
        }
        self.moved(&rects)
    }
}

/// A part's box before pins are applied: its size and its position inside its parent.
struct Placed {
    rel: Point,
    size: Size,
    align: Size,
}

impl HudModel {
    /// Lays every part out on `screen`; never fails, an unreadable part uses whatever values
    /// it has.
    pub fn layout(&self, screen: Screen) -> HudLayout {
        self.layout_in(screen, &GameSituation::default())
    }

    /// Lays every part out on `screen`, code-toggled parts shown as situation `s` decides.
    pub fn layout_in(&self, screen: Screen, s: &GameSituation) -> HudLayout {
        let parts = self.parts();
        let index: HashMap<PartKey, usize> = parts
            .iter()
            .enumerate()
            .map(|(i, p)| (p.key().clone(), i))
            .collect();
        let parents: Vec<Option<usize>> = parts
            .iter()
            .map(|p| p.parent().and_then(|k| index.get(k).copied()))
            .collect();
        let scale = ScreenScale::new(screen.tall);
        let screen_size = Size::new(clamp(screen.wide), clamp(screen.tall));

        // Parts come parents first, so each parent's size is known when its children read it.
        let mut placed: Vec<Placed> = Vec::with_capacity(parts.len());
        for (i, part) in parts.iter().enumerate() {
            let parent_size = parents[i].map_or(screen_size, |p| placed[p].size);
            placed.push(place(part, parent_size, screen_size, scale));
        }

        // A part the code places after a sibling takes its x from that sibling's finished width.
        for (i, part) in parts.iter().enumerate() {
            let Some(Fit::AfterSibling { sibling, gap }) = part.code_toggled().map(|t| t.fit())
            else {
                continue;
            };
            let found = (0..parts.len()).find(|&s| {
                parents[s] == parents[i] && parts[s].name().eq_ignore_ascii_case(sibling)
            });
            if let Some(s) = found {
                let units = i64::from(gap) * i64::from(screen_size.wide) / 640;
                let x = units.saturating_add(i64::from(placed[s].size.wide));
                placed[i].rel.x = i32::try_from(x).unwrap_or(i32::MAX);
            }
        }

        // CBaseHudChat::OnTick: the input line is the chat scheme's Default font plus 2 px tall,
        // its top 1.75 such heights above the chat's bottom.
        for (i, part) in parts.iter().enumerate() {
            if part.code_toggled().map(|t| t.fit()) != Some(Fit::ChatInput) {
                continue;
            }
            let Some(chat) = parents[i] else { continue };
            let font = (self.chat_scheme().font("Default", screen.tall)).map_or_else(
                || super::font_tall_px(12, screen.tall),
                |f| f.pixel_tall(screen.tall),
            );
            let tall = font.saturating_add(2);
            let y = f64::from(placed[chat].size.tall) - f64::from(tall) * 1.75;
            placed[i].rel.y = y as i32;
            placed[i].size.tall = tall;
        }

        let pins: Vec<Option<Pin>> = (0..parts.len())
            .map(|i| pin_of(parts, &parents, i))
            .collect();
        let mut rel: Vec<Option<Point>> = vec![None; parts.len()];
        for i in 0..parts.len() {
            pinned_rel(i, parts, &placed, &pins, scale, &mut rel);
        }

        let mut boxes: Vec<PartBox> = Vec::with_capacity(parts.len());
        for (i, part) in parts.iter().enumerate() {
            let at = rel[i].unwrap_or(placed[i].rel);
            let (px, py) = parents[i].map_or((0, 0), |p| (boxes[p].x, boxes[p].y));
            let Size { wide, tall } = placed[i].size;
            let (x, y) = (px.saturating_add(at.x), py.saturating_add(at.y));
            // The health element grows its pulse image about its centre as health moves.
            let grow = match GameSituation::is_pulse_image(part) {
                true => s.health_pulse_grow(self),
                false => 0,
            };
            let (x, y) = (x.saturating_sub(grow), y.saturating_sub(grow));
            let (wide, tall) = (wide.saturating_add(2 * grow), tall.saturating_add(2 * grow));
            let parent_hidden = parents[i].is_some_and(|p| boxes[p].hidden.is_some());
            let hidden = hidden_by(
                part,
                parent_hidden,
                Rect::new(x, y, wide, tall),
                screen_size,
                (s, self),
            );
            let full_screen = x <= 0
                && y <= 0
                && x.saturating_add(wide) >= screen_size.wide
                && y.saturating_add(tall) >= screen_size.tall;
            let painted = ["bgcolor_override", "fillcolor", "image"]
                .iter()
                .any(|k| part.value(k).is_some());
            boxes.push(PartBox {
                key: part.key().clone(),
                x,
                y,
                wide,
                tall,
                z: part.value("zpos").map_or(0, |l| atoi(l.text().as_bytes())),
                hidden,
                pinned: pins[i].is_some() && !in_cycle(i, &pins),
                clickable: (!full_screen || painted) && rule_clickable(part),
            });
        }
        let sets_z: Vec<bool> = parts.iter().map(|p| p.value("zpos").is_some()).collect();
        let paint = paint_order(&boxes, &parents, &sets_z);
        let mut paint_rank = vec![0; paint.len()];
        for (rank, &i) in paint.iter().enumerate() {
            paint_rank[i] = rank;
        }
        let buttons = (0..parts.len())
            .map(|i| button_above(parts, &parents, i))
            .collect();
        HudLayout {
            screen,
            boxes,
            parents,
            index,
            paint,
            paint_rank,
            buttons,
            generation: GENERATIONS.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// Whether every ancestor of `part` is unpinned and, on every preset screen, sits at x 0
    /// spanning the screen's full width; true for a top-level part.
    pub(super) fn parents_span_screen(&self, part: &Part) -> bool {
        let mut chain = Vec::new();
        let mut at = part.parent();
        while let Some(key) = at.filter(|_| chain.len() <= self.parts().len()) {
            let Some(parent) = self.part(key) else {
                return false;
            };
            chain.push(parent);
            at = parent.parent();
        }
        if chain.iter().any(|p| self.is_pinned(p)) {
            return false;
        }
        // Place only the chain, top down, so the check never lays out the whole HUD.
        Screen::PRESETS.iter().all(|s| {
            let screen = Size::new(clamp(s.wide), clamp(s.tall));
            let scale = ScreenScale::new(s.tall);
            let mut size = screen;
            chain.iter().rev().all(|p| {
                let placed = place(p, size, screen, scale);
                size = placed.size;
                placed.rel.x == 0 && placed.size.wide == screen.wide
            })
        })
    }

    /// The root's hide in the HUD's own files, before any change, measured at 1920x1080;
    /// `None` for the HUD scene or a root that shows.
    pub fn hidden_by_hud(&self) -> Option<HiddenBy> {
        self.hud_hides()
    }

    /// The root's hide in the parts as they stand now.
    pub(crate) fn measure_hidden_by_hud(&self) -> Option<HiddenBy> {
        let root = PartKey::new(self.scene_rule().root()?);
        self.layout(Screen::PRESETS[0]).get(&root)?.hidden()
    }
}

/// Box indices back to front: a depth-first walk, each sibling list by ascending `zpos`, with
/// siblings that set an equal `zpos` explicitly painting the earlier one on top.
fn paint_order(boxes: &[PartBox], parents: &[Option<usize>], sets_z: &[bool]) -> Vec<usize> {
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); boxes.len()];
    let mut top = Vec::new();
    for (i, parent) in parents.iter().enumerate() {
        parent.map_or(&mut top, |p| &mut children[p]).push(i);
    }
    let by_z = |v: &mut Vec<usize>| {
        v.sort_by_key(|&i| boxes[i].z);
        v.chunk_by_mut(|&a, &b| boxes[a].z == boxes[b].z)
            .for_each(|run| reverse_explicit(run, sets_z));
    };
    by_z(&mut top);
    children.iter_mut().for_each(by_z);
    let mut order = Vec::with_capacity(boxes.len());
    let mut stack: Vec<usize> = top.into_iter().rev().collect();
    while let Some(i) = stack.pop() {
        order.push(i);
        stack.extend(children[i].iter().rev());
    }
    order
}

// Within one run of equal zpos: the explicit siblings swap end for end among their own slots;
// siblings without a zpos line stay where the stable sort put them.
fn reverse_explicit(run: &mut [usize], sets_z: &[bool]) {
    let slots: Vec<usize> = (0..run.len()).filter(|&k| sets_z[run[k]]).collect();
    let explicit: Vec<usize> = slots.iter().map(|&k| run[k]).collect();
    for (&k, &i) in slots.iter().zip(explicit.iter().rev()) {
        run[k] = i;
    }
}

fn clamp(px: u32) -> i32 {
    i32::try_from(px).unwrap_or(i32::MAX)
}

fn int(part: &Part, key: &str) -> Option<i32> {
    part.value(key).map(|l| atoi(l.text().as_bytes()))
}

/// The nearest part enclosing part `i` whose class is a button, bounded by the part count so a
/// malformed parent chain can never loop forever.
fn button_above(parts: &[Part], parents: &[Option<usize>], i: usize) -> Option<usize> {
    let mut at = parents[i];
    for _ in 0..parts.len() {
        let p = at?;
        let class = parts[p].class().unwrap_or_default();
        if class.to_ascii_lowercase().ends_with("button") {
            return Some(p);
        }
        at = parents[p];
    }
    None
}

/// False only for a rule's own panel whose rule never takes a canvas click.
fn rule_clickable(part: &Part) -> bool {
    !part.is_root()
        || part
            .owner()
            .is_none_or(|id| super::PanelRule::get(id).clickable())
}

/// Whether `key` counts for this part: the game's code owns some keys of a rule's own panel.
fn applies(part: &Part, key: &str) -> bool {
    !(part.is_root()
        && part
            .owner()
            .is_some_and(|id| super::PanelRule::get(id).ignores(key)))
}

/// Size, then position inside the parent. `r`, `c`, `f` and `p` measure against the screen
/// unless the block sets `proportionaltoparent`.
fn place(part: &Part, parent: Size, screen: Size, scale: ScreenScale) -> Placed {
    let align = match int(part, "proportionaltoparent") {
        Some(n) if n != 0 => parent,
        _ => screen,
    };
    // A rule's root the game sizes to the whole screen ignores the file's own size.
    let rule = part
        .owner()
        .filter(|_| part.is_root())
        .map(super::PanelRule::get);
    let wide_expr = part.value("wide").map(|l| SizeExpr::parse(l.text()));
    let tall_expr = part.value("tall").map(|l| SizeExpr::parse(l.text()));
    // The engine starts every panel at 64x24 px before its settings apply.
    let (wide_axis, tall_axis) = (AxisSize::new(64, align.wide), AxisSize::new(24, align.tall));
    let mut wide = wide_expr.as_ref().map_or(64, |e| {
        e.eval(wide_axis, tall_expr.as_ref(), tall_axis, scale)
    });
    let mut tall = tall_expr.as_ref().map_or(24, |e| {
        e.eval(tall_axis, wide_expr.as_ref(), wide_axis, scale)
    });
    if rule.is_some_and(|r| r.fills_screen()) {
        Size { wide, tall } = screen;
    }
    let pos = |key: &str, own: i32, along: i32| {
        part.value(key)
            .filter(|_| applies(part, key))
            .map_or(0, |l| PosExpr::parse(l.text()).eval(own, along, scale))
    };
    // A rule's root the game re-centres every update ignores its own position on that axis.
    let x = match rule.is_some_and(|r| r.centres_x()) {
        true => screen.wide.saturating_sub(wide) / 2,
        false => pos("xpos", wide, align.wide),
    };
    let y = match rule.is_some_and(|r| r.centres_y()) {
        true => screen.tall.saturating_sub(tall) / 2,
        false => pos("ypos", tall, align.tall),
    };
    // The game fits a code-sized bar after its file loads, so the file's box gives way.
    let (x, y, wide, tall) = match part.code_toggled().map_or(Fit::None, |t| t.fit()) {
        Fit::None => (x, y, wide, tall),
        Fit::TopBar => (0, 0, screen.wide, tall),
        Fit::BottomBar => (x, y, screen.wide, screen.tall.saturating_sub(y).max(0)),
        // Placed once every sibling's width is known, in `layout`.
        Fit::AfterSibling { .. } => (x, y, wide, tall),
        // Placed once the chat's height is known, in `layout`.
        Fit::ChatInput => (x, y, wide, tall),
    };
    Placed {
        rel: Point::new(x, y),
        size: Size::new(wide, tall),
        align,
    }
}

/// A part's pin: the sibling's index and both corners.
#[derive(Clone, Copy)]
struct Pin {
    sibling: usize,
    own: PinCorner,
    corner: PinCorner,
}

/// The pin a part asks for when its sibling exists; the first sibling of that name wins.
fn pin_of(parts: &[Part], parents: &[Option<usize>], i: usize) -> Option<Pin> {
    let part = &parts[i];
    let name = part.value("pin_to_sibling")?.text();
    let sibling = (0..parts.len())
        .find(|&s| parents[s] == parents[i] && parts[s].name().eq_ignore_ascii_case(name))?;
    let corner = |key: &str| PinCorner::parse(part.value(key).map(|l| l.text()));
    Some(Pin {
        sibling,
        own: corner("pin_corner_to_sibling"),
        corner: corner("pin_to_sibling_corner"),
    })
}

/// Whether following pins from `i` comes back to `i`.
fn in_cycle(i: usize, pins: &[Option<Pin>]) -> bool {
    let mut at = i;
    for _ in 0..pins.len() {
        match pins[at] {
            Some(pin) if pin.sibling == i => return true,
            Some(pin) => at = pin.sibling,
            None => return false,
        }
    }
    false
}

/// The part's position inside its parent after pins, solved sibling-first; a part in a pin
/// cycle keeps its own position.
fn pinned_rel(
    i: usize,
    parts: &[Part],
    placed: &[Placed],
    pins: &[Option<Pin>],
    scale: ScreenScale,
    rel: &mut [Option<Point>],
) -> Point {
    if let Some(done) = rel[i] {
        return done;
    }
    let own = &placed[i];
    let at = match pins[i] {
        Some(pin) if !in_cycle(i, pins) => {
            let s = pinned_rel(pin.sibling, parts, placed, pins, scale, rel);
            let sib = placed[pin.sibling].size;
            let c = pin.corner.point(sib.wide, sib.tall);
            let o = pin.own.point(own.size.wide, own.size.tall);
            let (sign_x, sign_y) = probe::pin_offset_sign(pin.own, pin.corner);
            let offset = |key: &str, size: i32, along: i32| {
                parts[i]
                    .value(key)
                    .filter(|_| applies(&parts[i], key))
                    .map_or(0, |l| {
                        probe::pinned_offset(&PosExpr::parse(l.text()), size, along, scale)
                    })
            };
            // Values past i32 saturate, so a far-off part stays far off and layout never fails.
            let dx = sign_x.saturating_mul(offset("xpos", own.size.wide, own.align.wide));
            let dy = sign_y.saturating_mul(offset("ypos", own.size.tall, own.align.tall));
            let along = |s: i32, c: i32, d: i32, o: i32| {
                s.saturating_add(c).saturating_add(d).saturating_sub(o)
            };
            Point::new(along(s.x, c.x, dx, o.x), along(s.y, c.y, dy, o.y))
        }
        _ => own.rel,
    };
    rel[i] = Some(at);
    at
}

/// The first hide that holds, in order: parent, the game's code (a code-toggled part ignores
/// its own `visible`/`enabled`), `visible`/`enabled`, `alpha`, size, off screen.
fn hidden_by(
    part: &Part,
    parent_hidden: bool,
    Rect { x, y, wide, tall }: Rect,
    screen: Size,
    (situation, model): (&GameSituation, &HudModel),
) -> Option<HiddenBy> {
    // Top-level parts are hudlayout elements, whose visibility the game resets each frame.
    let element = part.parent().is_none();
    let off = |key: &str| int(part, key) == Some(0) && applies(part, key);
    let outside = x >= screen.wide
        || y >= screen.tall
        || i64::from(x) + i64::from(wide) <= 0
        || i64::from(y) + i64::from(tall) <= 0;
    let toggled = part.code_toggled();
    if parent_hidden {
        Some(HiddenBy::Parent)
    } else if toggled.is_some_and(|t| !t.shown_in(situation, model)) {
        Some(HiddenBy::Game)
    } else if toggled.is_none()
        && ((off("visible") && probe::hide_holds(HideHow::Visible, element))
            || (off("enabled") && probe::hide_holds(HideHow::Enabled, element)))
    {
        Some(HiddenBy::Visible)
    } else if off("alpha") && probe::hide_holds(HideHow::Alpha, element) {
        Some(HiddenBy::Alpha)
    } else if (wide <= 0 || tall <= 0) && probe::hide_holds(HideHow::SizeZero, element) {
        Some(HiddenBy::SizeZero)
    } else if outside && probe::hide_holds(HideHow::OffScreen, element) {
        Some(HiddenBy::OffScreen)
    } else {
        None
    }
}
