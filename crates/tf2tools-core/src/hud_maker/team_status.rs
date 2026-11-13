//! The match HUD's player panels beside the timer tab: TeamStatus in hudmatchstatus.res,
//! whose code (`CTFTeamStatus`) makes one panel per player from its `playerpanels_kv` block.
//! The sample is the local RED Soldier and one BLU enemy, as a one-a-side match shows them.

use super::document::KvEntry;
use super::expr::{AxisSize, PosExpr, ScreenScale, SizeExpr, atoi};
use super::layout::Screen;
use super::model::root_block;
use super::scheme::Rgba;
use super::{HudModel, PartKey, Rect, Scene, Team};
use crate::fonts::Platform;

const MATCH_STATUS: &str = "resource/ui/hudmatchstatus.res";

/// One player's panel as the preview draws it, every box in screen pixels and already cut at
/// the panel's and TeamStatus's edges.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct TeamStatusPanel {
    team: Team,
    local: bool,
    rect: Rect,
    backing: Rect,
    backing_colour: Rgba,
    portrait: Rect,
    health: Option<Rect>,
    health_colour: Rgba,
}

impl TeamStatusPanel {
    /// The player's team.
    pub fn team(&self) -> Team {
        self.team
    }

    /// Whether this is the local player's panel.
    pub fn local(&self) -> bool {
        self.local
    }

    /// The panel's box.
    pub fn rect(&self) -> Rect {
        self.rect
    }

    /// `classimagebg`, the coloured square behind the portrait.
    pub fn backing(&self) -> Rect {
        self.backing
    }

    /// The backing's colour, which the code picks by team and whether the panel is yours.
    pub fn backing_colour(&self) -> Rgba {
        self.backing_colour
    }

    /// `classimage`, the player's portrait.
    pub fn portrait(&self) -> Rect {
        self.portrait
    }

    /// `healthbar` at full health; `None` for the other team, whose health the game hides.
    pub fn health(&self) -> Option<Rect> {
        self.health
    }

    /// The health bar's colour at full health, `color_bar_health_high`.
    pub fn health_colour(&self) -> Rgba {
        self.health_colour
    }
}

impl HudModel {
    /// The sample's player panels on `screen`; empty outside the HUD scene, when the match HUD
    /// is hidden, or when the HUD has no TeamStatus block.
    pub fn team_status(&self, screen: Screen) -> Vec<TeamStatusPanel> {
        self.team_status_panels(screen).unwrap_or_default()
    }

    fn team_status_panels(&self, screen: Screen) -> Option<Vec<TeamStatusPanel>> {
        if self.scene() != Scene::Hud {
            return None;
        }
        let layout = self.layout(screen);
        let parent = layout.get(&PartKey::new("HudMatchStatus"))?;
        if parent.hidden().is_some() {
            return None;
        }
        let parent = parent.rect();
        let screen_rect = Rect::new(0, 0, screen.wide() as i32, screen.tall() as i32);
        let scale = ScreenScale::new(screen.tall());
        let platform = self.platform();

        // Merged roots list the bases lowest rank first; the winner goes first here.
        let mut roots = self.merged_roots(MATCH_STATUS);
        let own = usize::from(self.document(MATCH_STATUS).and_then(root_block).is_some());
        let own = own.min(roots.len());
        roots[own..].reverse();
        let status = Blocks {
            blocks: roots,
            platform,
        }
        .child("TeamStatus");
        if status.blocks.is_empty() {
            return None;
        }
        let area = place(&status, parent, screen_rect, scale);
        let kv = status.child("playerpanels_kv");

        // Fewer than seven a side, each panel is max_size wide and steps out from its team's
        // base; the code writes that width back normalised, so the panel loses the remainder.
        let step = scale.scale(status.int("max_size").unwrap_or(19));
        let wide = scale.scale(scale.normalise(step));
        let tall_expr = kv.size("tall");
        let tall = tall_expr.as_ref().map_or(24, |e| {
            e.eval(
                AxisSize::new(24, area.tall),
                None,
                AxisSize::new(wide, area.wide),
                scale,
            )
        });
        let y = kv
            .value("ypos")
            .map_or(0, |t| PosExpr::parse(t).eval(tall, area.tall, scale));
        let colour =
            |key: &str, default: &str| self.scheme().colour(kv.value(key).unwrap_or(default));

        // TF_TEAM_BLUE is team 1, growing west from its base by default; RED is team 2.
        let sample = [(Team::Blue, false, "team1"), (Team::Red, true, "team2")];
        let panels = sample.into_iter().map(|(team, local, side)| {
            let base = status.value(&format!("{side}_base_x")).unwrap_or("0");
            let base = PosExpr::parse(base).eval(area.wide, parent.wide, scale);
            let default_dir = if team == Team::Blue { "west" } else { "east" };
            let dir = status
                .value(&format!("{side}_grow_dir"))
                .unwrap_or(default_dir);
            let x = if dir.eq_ignore_ascii_case("west") {
                base - step
            } else {
                base
            };
            let rect = Rect::new(area.x + x, area.y + y, wide, tall);
            let clip = intersect(rect, area);
            let child = |name: &str| {
                let block = kv.child(name);
                (!block.blocks.is_empty())
                    .then(|| intersect(place(&block, rect, screen_rect, scale), clip))
            };
            let (backing_key, backing_default) = match (team, local) {
                (Team::Red, true) => ("color_portrait_bg_red_local_player", "195 169 168 255"),
                (Team::Blue, true) => ("color_portrait_bg_blue_local_player", "168 169 195 255"),
                (Team::Red, false) => ("color_portrait_bg_red", "119 62 61 255"),
                (Team::Blue, false) => ("color_portrait_bg_blue", "62 81 101 255"),
            };
            TeamStatusPanel {
                team,
                local,
                rect: clip,
                backing: child("classimagebg").unwrap_or_default(),
                backing_colour: colour(backing_key, backing_default),
                portrait: child("classimage").unwrap_or_default(),
                health: child("healthbar").filter(|_| local),
                health_colour: colour("color_bar_health_high", "84 191 58 255"),
            }
        });
        Some(
            panels
                .filter(|p| p.rect.wide > 0 && p.rect.tall > 0)
                .collect(),
        )
    }
}

/// One panel's blocks across the merged files, the winner first.
struct Blocks<'a> {
    blocks: Vec<&'a KvEntry>,
    platform: Platform,
}

impl<'a> Blocks<'a> {
    /// The sub-blocks named `name`, the last in each file winning.
    fn child(&self, name: &str) -> Blocks<'a> {
        let blocks = (self.blocks.iter())
            .flat_map(|b| {
                let mut own: Vec<&KvEntry> = (b.children().iter())
                    .filter(|e| {
                        e.text().is_none()
                            && e.holds(self.platform)
                            && e.key().eq_ignore_ascii_case(name)
                    })
                    .collect();
                own.reverse();
                own
            })
            .collect();
        Blocks {
            blocks,
            platform: self.platform,
        }
    }

    /// The winning text of `key`: the match file loads with `if_match` on, so that sub-block
    /// counts after its parent.
    fn value(&self, key: &str) -> Option<&'a str> {
        let last = |b: &'a KvEntry| {
            (b.children().iter()).rfind(|e| {
                e.text().is_some() && e.holds(self.platform) && e.key().eq_ignore_ascii_case(key)
            })
        };
        (self.blocks.iter())
            .find_map(|b| {
                let matched = (b.children().iter())
                    .filter(|e| e.text().is_none() && e.key().eq_ignore_ascii_case("if_match"))
                    .find_map(last);
                matched.or_else(|| last(b))
            })
            .and_then(KvEntry::text)
    }

    fn int(&self, key: &str) -> Option<i32> {
        self.value(key).map(|t| atoi(t.as_bytes()))
    }

    fn size(&self, key: &str) -> Option<SizeExpr> {
        self.value(key).map(SizeExpr::parse)
    }
}

/// A block's box inside `parent` as the engine places a panel: `r`, `c`, `f` and `p` measure
/// against the screen unless it sets `proportionaltoparent`.
fn place(block: &Blocks, parent: Rect, screen: Rect, scale: ScreenScale) -> Rect {
    let align = match block.int("proportionaltoparent") {
        Some(n) if n != 0 => parent,
        _ => screen,
    };
    let (wide_expr, tall_expr) = (block.size("wide"), block.size("tall"));
    // The engine starts every panel at 64x24 px before its settings apply.
    let (wide_axis, tall_axis) = (AxisSize::new(64, align.wide), AxisSize::new(24, align.tall));
    let wide = wide_expr.as_ref().map_or(64, |e| {
        e.eval(wide_axis, tall_expr.as_ref(), tall_axis, scale)
    });
    let tall = tall_expr.as_ref().map_or(24, |e| {
        e.eval(tall_axis, wide_expr.as_ref(), wide_axis, scale)
    });
    let pos = |key: &str, own: i32, along: i32| {
        block
            .value(key)
            .map_or(0, |t| PosExpr::parse(t).eval(own, along, scale))
    };
    Rect::new(
        parent.x + pos("xpos", wide, align.wide),
        parent.y + pos("ypos", tall, align.tall),
        wide,
        tall,
    )
}

/// The part of `a` inside `b`; an empty box when they do not meet.
fn intersect(a: Rect, b: Rect) -> Rect {
    let (x, y) = (a.x.max(b.x), a.y.max(b.y));
    let right = (a.x + a.wide).min(b.x + b.wide);
    let bottom = (a.y + a.tall).min(b.y + b.tall);
    if right <= x || bottom <= y {
        return Rect::default();
    }
    Rect::new(x, y, right - x, bottom - y)
}
