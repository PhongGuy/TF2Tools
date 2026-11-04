//! What a rank panel (`CPvPRankPanel`) draws from code: the medal, the rank name and the level
//! bar, laid out as the game's own `PvPRankPanel.res` places them and filled with a sample rank.

use super::expr::{AxisSize, PosExpr, ScreenScale, SizeExpr};
use super::layout::{HudLayout, Screen};
use super::model::{HudModel, Part, PartKey};
use super::{HudFiles, Scene, SceneRule};
use crate::fonts::Platform;
use crate::fonts::keyvalues::{self, KvPair, KvValue};

/// A rectangle as (x, y, wide, tall) in screen pixels from the panel's top-left corner.
type Rect = (i32, i32, i32, i32);

/// Where a rank panel's code-drawn pieces sit, each `None` when the panel's keys turn it off,
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
    fill: Option<Rect>,
    fill_colour: Option<String>,
}

impl RankPieces {
    /// The sample rank's name, a casual level, as the preview draws it.
    pub const SAMPLE_NAME: &'static str = "Level 42";

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

    /// The filled part, clipped; its width is three fifths of the fill control's own width, the
    /// sample's progress. A hidden track leaves it alone.
    pub fn fill(&self) -> Option<Rect> {
        self.fill
    }

    /// The fill's colour from the file, if it sets one.
    pub fn fill_colour(&self) -> Option<&str> {
        self.fill_colour.as_deref()
    }
}

const NAME_PATH: &str = "BGPanel/NameLabel";
const TRACK_PATH: &str = "BGPanel/StatsContainer/XPBar/ProgressBarsContainer/ProgressBar";
const FILL_PATH: &str = "BGPanel/StatsContainer/XPBar/ProgressBarsContainer/ContinuousProgressBar";

/// Edges (left, top, right, bottom) of the area a control may draw in.
type Edges = (i32, i32, i32, i32);

/// A control placed on the panel: its own box and the part of it that shows.
struct Placed<'a> {
    control: &'a RankControl,
    own: Rect,
    shown: Option<Rect>,
}

fn edges((x, y, w, h): Rect) -> Edges {
    (x, y, x.saturating_add(w), y.saturating_add(h))
}

/// `a` and `b` overlapped as edges, `None` when they share no area.
fn overlap(a: Edges, b: Edges) -> Option<Edges> {
    let e = (a.0.max(b.0), a.1.max(b.1), a.2.min(b.2), a.3.min(b.3));
    (e.2 > e.0 && e.3 > e.1).then_some(e)
}

fn rect_of((l, t, r, b): Edges) -> Rect {
    (l, t, r.saturating_sub(l), b.saturating_sub(t))
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
    let mut parent: Rect = (0, 0, wide, tall);
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
        let (wide_axis, tall_axis) = (AxisSize::new(0, parent.2), AxisSize::new(0, parent.3));
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
        let x = parent.0.saturating_add(pos("xpos", own_wide, parent.2));
        let y = parent.1.saturating_add(pos("ypos", own_tall, parent.3));
        let own = (x, y, own_wide, own_tall);
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
        (at(wide), at(tall), side, side)
    });
    let level = on("show_progress");
    let layout = model.rank_layout();
    let find = |path: &str| {
        level
            .then(|| place(layout, path, wide, tall, scale))
            .flatten()
    };
    let (name, track, fill) = (find(NAME_PATH), find(TRACK_PATH), find(FILL_PATH));
    let text = |p: &Option<Placed>, key: &str| {
        p.as_ref()
            .and_then(|p| p.control.value(key))
            .map(str::to_owned)
    };
    // Only a CExLabel reads `fgcolor`; a plain Label has just the override.
    let name_colour = text(&name, "fgcolor_override").or_else(|| {
        let is_ex = name.as_ref().is_some_and(|p| {
            (p.control.value("ControlName")).is_some_and(|c| c.eq_ignore_ascii_case("CExLabel"))
        });
        is_ex.then(|| text(&name, "fgcolor")).flatten()
    });
    // The label shows as a whole box or not at all: hidden or clipped away leaves neither.
    let name_box = name.as_ref().and_then(|p| p.shown.map(|_| p.own));
    // Progress is three fifths of the fill's own width, then clipped like any other box.
    let fill_rect = fill.as_ref().and_then(|p| {
        let (x, y, w, h) = p.own;
        let progress = edges((x, y, (w as f32 * 0.6) as i32, h));
        let shown = p.shown.map(edges)?;
        overlap(progress, shown).map(rect_of)
    });
    RankPieces {
        medal,
        name: name.as_ref().and_then(|p| p.shown),
        name_box,
        name_font: text(&name, "font"),
        name_colour,
        name_alignment: text(&name, "textAlignment").unwrap_or_default(),
        name_all_caps: name
            .as_ref()
            .and_then(|p| p.control.value("allcaps"))
            .and_then(|t| t.trim().parse::<i64>().ok())
            .is_some_and(|v| v > 0),
        bar: track.as_ref().and_then(|p| p.shown),
        track_colour: text(&track, "fgcolor_override"),
        fill: fill_rect,
        fill_colour: text(&fill, "fgcolor_override"),
    }
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
    let (x, y, wide, tall) =
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
        let (name_x, name_y) = (pb.x().saturating_add(name.0), pb.y().saturating_add(name.1));
        // Only a name that starts right of the square's left edge can be covered by it.
        if name_x > left && name_y < bottom && top < name_y.saturating_add(name.3) {
            right = right.min(name_x.saturating_sub(gap));
        }
    }
    let new_x = right.saturating_sub(b.x()).saturating_sub(wide);
    if new_x >= 0 {
        return Some((new_x, y, wide, tall));
    }
    // Too little room left of the name: shrink the square about its centre height.
    let side = right.saturating_sub(b.x()).clamp(0, wide);
    Some((0, y + (tall - side) / 2, side, side))
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
    StatsContainer { ControlName EditablePanel xpos rs1-5 ypos 0 wide f70 tall f0
      XPBar { ControlName EditablePanel xpos cs-0.5 ypos rs1-3 wide p1 tall 30
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
