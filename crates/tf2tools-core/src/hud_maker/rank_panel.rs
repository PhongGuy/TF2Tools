//! What a rank panel (`CPvPRankPanel`) draws from code: the medal, the player's name, the rank
//! lines, the level bar and its xp, laid out as the game's own `PvPRankPanel.res` places them and
//! filled with a new player's casual rank.

use super::expr::{AxisSize, PosExpr, ScreenScale, SizeExpr};
use super::layout::{HudLayout, Screen};
use super::model::{HudModel, Part, PartKey};
use super::{HudFiles, Rect, Scene, SceneRule};
use crate::fonts::Platform;
use crate::fonts::keyvalues::{self, KvPair, KvValue};

/// Where a rank panel's code-drawn pieces sit, in screen pixels from the panel's top-left, each `None` when the panel's keys turn it off,
/// with the look the HUD's own rank file gives them.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RankPieces {
    medal: Option<Rect>,
    name: Option<Rect>,
    name_box: Option<Rect>,
    name_font: Option<String>,
    name_colour: Option<String>,
    name_alignment: String,
    name_all_caps: bool,
    bar: Option<Rect>,
    track_colour: Option<String>,
    segments: Vec<Rect>,
    labels: Vec<RankLabel>,
}

impl RankPieces {
    /// The sample player's name, which the game draws in `NameLabel`.
    pub const SAMPLE_NAME: &'static str = "Player";

    /// The medal, a square centred on the panel; the game draws it as a 3D model.
    pub fn medal(&self) -> Option<Rect> {
        self.medal
    }

    /// The rank name label's visible part: its box clipped to its parents.
    pub fn name(&self) -> Option<Rect> {
        self.name
    }

    /// The label's own box, which may reach past `name()`; text aligns inside it, then clips.
    /// `None` when the label is hidden, or clipped away entirely (off the panel, or no width).
    pub fn name_box(&self) -> Option<Rect> {
        self.name_box
    }

    /// The label's font name from the file, if it sets one.
    pub fn name_font(&self) -> Option<&str> {
        self.name_font.as_deref()
    }

    /// The label's colour: `fgcolor_override`, else `fgcolor` for a `CExLabel`, else unset.
    pub fn name_colour(&self) -> Option<&str> {
        self.name_colour.as_deref()
    }

    /// The label's `textAlignment`, empty when unset.
    pub fn name_alignment(&self) -> &str {
        &self.name_alignment
    }

    /// Whether the label draws in capitals (`allcaps` above 0).
    pub fn name_all_caps(&self) -> bool {
        self.name_all_caps
    }

    /// The level bar's track, its visible part.
    pub fn bar(&self) -> Option<Rect> {
        self.bar
    }

    /// The track's colour from the file, if it sets one.
    pub fn track_colour(&self) -> Option<&str> {
        self.track_colour.as_deref()
    }

    /// The track's ten segments, each clipped to the track; empty when the track is hidden.
    /// The sample has no xp yet, so the green progress bar over them is empty.
    pub fn segments(&self) -> &[Rect] {
        &self.segments
    }

    /// The rank lines and xp labels that show: `DescLine1`, `DescLine2`, `CurrentXPLabel`, then
    /// `NextLevelXPLabel`.
    pub fn labels(&self) -> &[RankLabel] {
        &self.labels
    }
}

/// One of the panel's code-filled labels besides the name, with its sample text and the look
/// the HUD's rank file gives it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RankLabel {
    field: &'static str,
    text: &'static str,
    shown: Rect,
    own: Rect,
    font: Option<String>,
    colour: Option<String>,
    alignment: String,
    all_caps: bool,
}

impl RankLabel {
    /// The label's block name in the file, such as `CurrentXPLabel`.
    pub fn field(&self) -> &str {
        self.field
    }

    /// The sample text the preview draws in it.
    pub fn text(&self) -> &str {
        self.text
    }

    /// The label's visible part: its box clipped to its parents.
    pub fn shown(&self) -> Rect {
        self.shown
    }

    /// The label's own box, which may reach past `shown()`; text aligns inside it, then clips.
    pub fn own(&self) -> Rect {
        self.own
    }

    /// The font name from the file, if it sets one.
    pub fn font(&self) -> Option<&str> {
        self.font.as_deref()
    }

    /// The colour: `fgcolor_override`, else `fgcolor` for a `CExLabel`, else unset.
    pub fn colour(&self) -> Option<&str> {
        self.colour.as_deref()
    }

    /// The `textAlignment`, empty when unset.
    pub fn alignment(&self) -> &str {
        &self.alignment
    }

    /// Whether the label draws in capitals (`allcaps` above 0).
    pub fn all_caps(&self) -> bool {
        self.all_caps
    }
}

const NAME_PATH: &str = "BGPanel/NameLabel";
const TRACK_PATH: &str = "BGPanel/StatsContainer/XPBar/ProgressBarsContainer/ProgressBar";

/// The labels the game fills for a new casual player: the rank type and level under the name,
/// and the xp so far and needed for the next level under the bar's ends.
const SAMPLE_LABELS: [(&str, &str); 4] = [
    ("BGPanel/DescLine1", "Casual"),
    ("BGPanel/DescLine2", "Level: 1"),
    ("BGPanel/StatsContainer/XPBar/CurrentXPLabel", "0 xp"),
    ("BGPanel/StatsContainer/XPBar/NextLevelXPLabel", "750 xp"),
];

/// The casual level bar is split into this many segments.
const SEGMENTS: i32 = 10;

/// Edges (left, top, right, bottom) of the area a control may draw in.
type Edges = (i32, i32, i32, i32);

/// A control placed on the panel: its own box and the part of it that shows.
struct Placed<'a> {
    control: &'a RankControl,
    own: Rect,
    shown: Option<Rect>,
}

fn edges(r: Rect) -> Edges {
    (
        r.x,
        r.y,
        r.x.saturating_add(r.wide),
        r.y.saturating_add(r.tall),
    )
}

/// `a` and `b` overlapped as edges, `None` when they share no area.
fn overlap(a: Edges, b: Edges) -> Option<Edges> {
    let e = (a.0.max(b.0), a.1.max(b.1), a.2.min(b.2), a.3.min(b.3));
    (e.2 > e.0 && e.3 > e.1).then_some(e)
}

fn rect_of((l, t, r, b): Edges) -> Rect {
    Rect::new(l, t, r.saturating_sub(l), b.saturating_sub(t))
}

/// Places the control at `path` down the layout's tree for a panel of `wide` x `tall`: each
/// box from the file's own expressions relative to its parent, and what shows of it clipped to
/// every parent's visible box. `None` when the file has no such control. A key the block lacks
/// reads as unset: position 0, size 0, visible.
fn place<'a>(
    layout: &'a RankLayout,
    path: &str,
    wide: i32,
    tall: i32,
    scale: ScreenScale,
) -> Option<Placed<'a>> {
    let mut at = String::new();
    // The panel itself is the first parent; nothing clips it here, the canvas does.
    let mut parent = Rect::new(0, 0, wide, tall);
    let mut clip = Some((i32::MIN, i32::MIN, i32::MAX, i32::MAX));
    let mut placed = None;
    for name in path.split('/') {
        if !at.is_empty() {
            at.push('/');
        }
        at.push_str(name);
        let control = layout.control(&at)?;
        let size = |key: &str| control.value(key).map(SizeExpr::parse);
        let (wide_expr, tall_expr) = (size("wide"), size("tall"));
        let (wide_axis, tall_axis) = (AxisSize::new(0, parent.wide), AxisSize::new(0, parent.tall));
        let own_wide = wide_expr.as_ref().map_or(0, |e| {
            e.eval(wide_axis, tall_expr.as_ref(), tall_axis, scale)
        });
        let own_tall = tall_expr.as_ref().map_or(0, |e| {
            e.eval(tall_axis, wide_expr.as_ref(), wide_axis, scale)
        });
        let pos = |key: &str, own: i32, along: i32| {
            control
                .value(key)
                .map_or(0, |t| PosExpr::parse(t).eval(own, along, scale))
        };
        let x = parent.x.saturating_add(pos("xpos", own_wide, parent.wide));
        let y = parent.y.saturating_add(pos("ypos", own_tall, parent.tall));
        let own = Rect::new(x, y, own_wide, own_tall);
        let visible = control
            .value("visible")
            .and_then(|t| t.trim().parse::<i64>().ok())
            .is_none_or(|v| v != 0);
        clip = clip
            .filter(|_| visible)
            .and_then(|c| overlap(c, edges(own)));
        placed = Some(Placed {
            control,
            own,
            shown: clip.map(rect_of),
        });
        parent = own;
    }
    placed
}

/// The pieces a rank panel `part` of `wide` x `tall` pixels draws on `screen`, laid out by the
/// HUD's own rank file (`model.rank_layout()`). `show_model 0` leaves out the medal and
/// `show_progress 0` the name and bar; unset, both show.
pub fn rank_pieces(
    model: &HudModel,
    part: &Part,
    wide: i32,
    tall: i32,
    screen: Screen,
) -> RankPieces {
    let scale = ScreenScale::new(screen.tall());
    let on = |key: &str| {
        part.value(key)
            .and_then(|l| l.text().trim().parse::<i64>().ok())
            .is_none_or(|v| v != 0)
    };
    // The model's view is a square 12 % of the panel's height; the casual medal fills 7/10 of
    // it from the game's camera, 45 units away at fov 70.
    let medal = on("show_model").then(|| {
        let side = (i64::from(tall.max(0)) * 12 / 100 * 7 / 10) as i32;
        let at = |size: i32| ((i64::from(size) - i64::from(side)) / 2) as i32;
        Rect::new(at(wide), at(tall), side, side)
    });
    let level = on("show_progress");
    let layout = model.rank_layout();
    let find = |path: &str| {
        level
            .then(|| place(layout, path, wide, tall, scale))
            .flatten()
    };
    let (name, track) = (find(NAME_PATH), find(TRACK_PATH));
    let text =
        |p: Option<&Placed>, key: &str| p.and_then(|p| p.control.value(key)).map(str::to_owned);
    // The label shows as a whole box or not at all: hidden or clipped away leaves neither.
    let name_box = name.as_ref().and_then(|p| p.shown.map(|_| p.own));
    let labels = SAMPLE_LABELS
        .iter()
        .filter_map(|&(path, sample)| {
            let p = find(path)?;
            let field = path.rsplit('/').next().unwrap_or(path);
            Some(RankLabel {
                field,
                text: sample,
                shown: p.shown?,
                own: p.own,
                font: text(Some(&p), "font"),
                colour: label_colour(&p),
                alignment: text(Some(&p), "textAlignment").unwrap_or_default(),
                all_caps: all_caps(&p),
            })
        })
        .collect();
    RankPieces {
        medal,
        name: name.as_ref().and_then(|p| p.shown),
        name_box,
        name_font: text(name.as_ref(), "font"),
        name_colour: name.as_ref().and_then(label_colour),
        name_alignment: text(name.as_ref(), "textAlignment").unwrap_or_default(),
        name_all_caps: name.as_ref().is_some_and(all_caps),
        bar: track.as_ref().and_then(|p| p.shown),
        track_colour: text(track.as_ref(), "fgcolor_override"),
        segments: track.as_ref().map_or_else(Vec::new, |p| segments(p, scale)),
        labels,
    }
}

/// A label's colour: only a CExLabel reads `fgcolor`; a plain Label has just the override.
fn label_colour(p: &Placed) -> Option<String> {
    let value = |key: &str| p.control.value(key).map(str::to_owned);
    value("fgcolor_override").or_else(|| {
        let is_ex =
            (p.control.value("ControlName")).is_some_and(|c| c.eq_ignore_ascii_case("CExLabel"));
        is_ex.then(|| value("fgcolor")).flatten()
    })
}

fn all_caps(p: &Placed) -> bool {
    (p.control.value("allcaps"))
        .and_then(|t| t.trim().parse::<i64>().ok())
        .is_some_and(|v| v > 0)
}

/// The track's own box cut into equal segments with a one-unit gap between neighbours, each
/// clipped to what shows of the track; none when the track is hidden or clipped away.
fn segments(track: &Placed, scale: ScreenScale) -> Vec<Rect> {
    let Some(shown) = track.shown.map(edges) else {
        return Vec::new();
    };
    let gap = i64::from(scale.scale(1).max(1));
    let (x, wide) = (i64::from(track.own.x), i64::from(track.own.wide));
    let (top, bottom) = (i64::from(track.own.y), i64::from(edges(track.own).3));
    let n = i64::from(SEGMENTS);
    (0..n)
        .filter_map(|i| {
            let left = x + i * wide / n;
            let right = x + (i + 1) * wide / n - if i + 1 < n { gap } else { 0 };
            let clamp = |v: i64| v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
            overlap(
                (clamp(left), clamp(top), clamp(right), clamp(bottom)),
                shown,
            )
            .map(rect_of)
        })
        .collect()
}

/// The medal stand-in of rank panel `key`, as (x, y, wide, tall) from the panel's top-left.
/// The game's 3D medal sits left of the rank name, so the square moves left until it ends a few
/// units short of any name level with it, and shrinks if that would push it off the panel.
pub fn rank_medal(
    model: &HudModel,
    layout: &HudLayout,
    key: &PartKey,
    screen: Screen,
) -> Option<Rect> {
    let b = layout.get(key)?;
    let Rect { x, y, wide, tall } =
        rank_pieces(model, model.part(key)?, b.wide(), b.tall(), screen).medal()?;
    let gap = ScreenScale::new(screen.tall()).scale(4);
    let (top, bottom) = (
        b.y().saturating_add(y),
        b.y().saturating_add(y).saturating_add(tall),
    );
    let left = b.x().saturating_add(x);
    let mut right = left.saturating_add(wide);
    for part in model
        .parts()
        .iter()
        .filter(|p| (p.class()).is_some_and(|c| c.eq_ignore_ascii_case("CPvPRankPanel")))
    {
        let Some(pb) = layout.get(part.key()) else {
            continue;
        };
        // The label's full box, so a name clipped on its left does not move the medal.
        let Some(name) = rank_pieces(model, part, pb.wide(), pb.tall(), screen).name_box() else {
            continue;
        };
        let (name_x, name_y) = (pb.x().saturating_add(name.x), pb.y().saturating_add(name.y));
        // Only a name that starts right of the square's left edge can be covered by it.
        if name_x > left && name_y < bottom && top < name_y.saturating_add(name.tall) {
            right = right.min(name_x.saturating_sub(gap));
        }
    }
    let new_x = right.saturating_sub(b.x()).saturating_sub(wide);
    if new_x >= 0 {
        return Some(Rect::new(new_x, y, wide, tall));
    }
    // Too little room left of the name: shrink the square about its centre height.
    let side = right.saturating_sub(b.x()).clamp(0, wide);
    Some(Rect::new(0, y + (tall - side) / 2, side, side))
}

/// One control of the rank panel's file: its block name, its text keys in file order and its
/// child blocks. Read-only; the preview never edits the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankControl {
    name: String,
    values: Vec<(String, String)>,
    children: Vec<RankControl>,
}

impl RankControl {
    fn from_pairs(name: &str, pairs: &[KvPair]) -> RankControl {
        let mut control = RankControl {
            name: name.to_owned(),
            values: Vec::new(),
            children: Vec::new(),
        };
        for pair in pairs {
            match &pair.value {
                KvValue::Text(text) => control.values.push((pair.key.clone(), text.clone())),
                KvValue::Block(inner) => control
                    .children
                    .push(RankControl::from_pairs(&pair.key, inner)),
            }
        }
        control
    }

    /// The first text value of `key`, ignoring case.
    pub fn value(&self, key: &str) -> Option<&str> {
        self.values
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }

    fn child(&self, name: &str) -> Option<&RankControl> {
        self.children
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
    }
}

/// The text of the built-in tree: only the keys the preview draws, written by hand.
const STOCK_RANK_PANEL: &str = r##""Resource/UI/PvPRankPanel.res"
{
  BGPanel { ControlName EditablePanel xpos rs1 ypos 20 wide 260 tall f40 visible 1
    NameLabel { ControlName Label xpos 65 ypos 5 wide f0 tall 20 visible 1 font HudFontSmallishBold fgcolor_override TanLight textAlignment north-west }
    DescLine1 { ControlName CAutoFittingLabel xpos 65 ypos 19 wide 195 tall 20 font HudFontSmallestBold fgcolor_override TanLight textAlignment north-west }
    DescLine2 { ControlName CAutoFittingLabel xpos 65 ypos 29 wide 195 tall 20 font HudFontSmallestBold fgcolor_override TanLight textAlignment north-west }
    StatsContainer { ControlName EditablePanel xpos rs1-5 ypos 0 wide f70 tall f0
      XPBar { ControlName EditablePanel xpos cs-0.5 ypos rs1-3 wide p1 tall 30
        CurrentXPLabel { ControlName Label xpos 0 ypos rs1 wide 100 tall 20 font ItemFontAttribSmall fgcolor_override TanLight textAlignment south-west }
        NextLevelXPLabel { ControlName Label xpos rs1 ypos rs1 wide 100 tall 20 font ItemFontAttribSmall fgcolor_override TanLight textAlignment south-east }
        ProgressBarsContainer { ControlName EditablePanel xpos 0 ypos rs1-10 wide p1 tall 7
          ProgressBar { ControlName ProgressBar xpos 0 ypos cs-0.5 wide f0 tall f-2 fgcolor_override "20 20 20 180" }
          ContinuousProgressBar { ControlName ContinuousProgressBar xpos cs-0.5 ypos cs-0.5 wide f2 tall f2 fgcolor_override CreditsGreen } } } } }
}
"##;

/// The rank panel's control tree, from the HUD's own `pvprankpanel.res` or the built-in stock
/// tree when the HUD has no usable one.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RankLayout {
    root: RankControl,
}

impl RankLayout {
    /// The built-in tree, parsed through the same path as a file.
    pub fn stock() -> RankLayout {
        let file = keyvalues::parse(STOCK_RANK_PANEL, Platform::Linux);
        // The text above always has its root block; an empty tree is the unreachable fallback.
        RankLayout::from_pairs(&file.pairs).unwrap_or_else(|| RankLayout {
            root: RankControl::from_pairs("", &[]),
        })
    }

    /// The tree of the Main menu scene's read-only rank file, `#base` followed. A missing,
    /// unreadable, oversized or blockless file gives the stock tree.
    pub fn read(files: &HudFiles, platform: Platform) -> RankLayout {
        let rule = SceneRule::get(Scene::MainMenu);
        rule.read_only_files()
            .first()
            .and_then(|path| files.load_kv(path, platform).ok())
            .and_then(|(pairs, _)| RankLayout::from_pairs(&pairs))
            .unwrap_or_else(RankLayout::stock)
    }

    fn from_pairs(pairs: &[KvPair]) -> Option<RankLayout> {
        pairs.iter().find_map(|pair| match &pair.value {
            KvValue::Block(inner) => Some(RankLayout {
                root: RankControl::from_pairs(&pair.key, inner),
            }),
            KvValue::Text(_) => None,
        })
    }

    /// The control at `path`, block names joined by `/` from the file's root block, ignoring
    /// case; `None` when the file has no such block.
    pub fn control(&self, path: &str) -> Option<&RankControl> {
        path.split('/')
            .try_fold(&self.root, |at, name| at.child(name))
    }
}
