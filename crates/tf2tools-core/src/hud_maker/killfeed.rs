//! The sample killfeed: notice lines laid out from HudDeathNotice's settings the way
//! `hud_basedeathnotice.cpp` sizes them, for the maker to draw over the part's box.

use super::expr::atoi;
use super::{
    ClassSchema, HudModel, Part, PartKey, Rgba, SampleSet, Scene, Screen, ScreenScale, Team,
};

const PANEL: &str = "HudDeathNotice";
/// The most sample lines drawn, a preview bound on MaxDeathNotices.
const MAX_ROWS: i32 = 20;

/// The sample killfeed, in pixels relative to the HudDeathNotice part's box.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DeathNotices {
    lines: Vec<NoticeLine>,
    right_justify: bool,
    margin: i32,
    corner_radius: i32,
    font: String,
    icon_colour: Rgba,
}

impl DeathNotices {
    /// The sample lines, top first.
    pub fn lines(&self) -> &[NoticeLine] {
        &self.lines
    }

    /// RightJustify: lines end at the box's right edge.
    pub fn right_justify(&self) -> bool {
        self.right_justify
    }

    /// XRES(10): the space before the killer and after the victim.
    pub fn margin(&self) -> i32 {
        self.margin
    }

    /// The background's corner radius, truncated to whole pixels.
    pub fn corner_radius(&self) -> i32 {
        self.corner_radius
    }

    /// TextFont, the scheme font the names are drawn in.
    pub fn font(&self) -> &str {
        &self.font
    }

    /// IconColor, the tint of the weapon icon.
    pub fn icon_colour(&self) -> Rgba {
        self.icon_colour
    }
}

/// One sample notice: who killed whom, and where the line sits in the part's box.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct NoticeLine {
    y: i32,
    tall: i32,
    icon: i32,
    killer: &'static str,
    killer_colour: Rgba,
    victim: &'static str,
    victim_colour: Rgba,
    background: Rgba,
    icon_name: String,
}

impl NoticeLine {
    /// The line's top, from the part's top.
    pub fn y(&self) -> i32 {
        self.y
    }

    /// The line's height, LineHeight scaled.
    pub fn tall(&self) -> i32 {
        self.tall
    }

    /// The weapon icon's height, LineHeight - YRES(2) truncated, where YRES is the float
    /// 2 x screen tall / 480; the app draws it twice as wide.
    pub fn icon(&self) -> i32 {
        self.icon
    }

    /// The killer's name.
    pub fn killer(&self) -> &'static str {
        self.killer
    }

    /// The killer's team colour.
    pub fn killer_colour(&self) -> Rgba {
        self.killer_colour
    }

    /// The victim's name.
    pub fn victim(&self) -> &'static str {
        self.victim
    }

    /// The victim's team colour.
    pub fn victim_colour(&self) -> Rgba {
        self.victim_colour
    }

    /// The line's background: the local one when the local player is involved.
    pub fn background(&self) -> Rgba {
        self.background
    }

    /// The weapon icon's name in the HUD icon table: `dneg_` when the local player is the
    /// killer or victim, else `d_`, then the killer's weapon (`skull_tf` with no killer).
    pub fn icon_name(&self) -> &str {
        &self.icon_name
    }
}

impl HudModel {
    /// The sample lines at `screen` from the part's settings: MaxDeathNotices of them (0 to
    /// 20) when the part sets it, else two, each other absent key taking its schema default;
    /// `None` outside the HUD and freeze cam, when the HUD has no HudDeathNotice block
    /// (`Part::blocks()` is empty), when the part is unreadable, or when its box at `screen`
    /// is hidden (`layout(screen).get(..).hidden()`).
    pub fn death_notices(&self, screen: Screen) -> Option<DeathNotices> {
        if !matches!(self.scene(), Scene::Hud | Scene::FreezeCam) {
            return None;
        }
        let key = PartKey::new(PANEL);
        let part = self.part(&key)?;
        if part.blocks().is_empty() || !part.is_readable() {
            return None;
        }
        if self.layout(screen).get(&key)?.hidden().is_some() {
            return None;
        }

        let scale = ScreenScale::new(screen.tall());
        let tall = scale.scale(number(part, "LineHeight"));
        let spacing = scale.scale(number(part, "LineSpacing"));
        // YRES is a float; the engine assigns the difference to an int, which truncates.
        let yres_2 = 2.0 * f64::from(screen.tall()) / 480.0;
        let icon = (f64::from(tall) - yres_2) as i32;
        let colour = |key: &str| self.scheme().colour(text(part, key).unwrap_or(""));
        let (red, blue) = (colour("TeamRed"), colour("TeamBlue"));

        // Only the part's own MaxDeathNotices counts, so an absent key keeps the Stage 2 pair:
        // the local player's kill, then one between two others.
        let rows = part
            .value("MaxDeathNotices")
            .map_or(2, |l| atoi(l.text().as_bytes()).clamp(0, MAX_ROWS));
        let casual = SampleSet::Casual;
        let name = |team: Team, i: usize| casual.players(team).get(i).map_or("", |p| p.name());
        let icon_name = |killer: Team, victim: Team, i: usize| {
            let (killer, victim) = (casual.players(killer).get(i), casual.players(victim).get(i));
            let local = killer.is_some_and(|p| p.local()) || victim.is_some_and(|p| p.local());
            let prefix = if local { "dneg_" } else { "d_" };
            format!("{prefix}{}", killer.map_or("skull_tf", |p| p.kill_weapon()))
        };
        let (local_bg, base_bg) = (
            colour("LocalBackgroundColor"),
            colour("BaseBackgroundColor"),
        );
        // Kills alternate RED on BLU and BLU on RED down the sample players.
        let lines = (0..rows)
            .map(|i| {
                let player = usize::try_from(i).unwrap_or(0) % 12;
                let (killer_team, killer_colour, victim_team, victim_colour) = if i % 2 == 0 {
                    (Team::Red, red, Team::Blue, blue)
                } else {
                    (Team::Blue, blue, Team::Red, red)
                };
                NoticeLine {
                    y: i.saturating_mul(tall.saturating_add(spacing)),
                    tall,
                    icon,
                    killer: name(killer_team, player),
                    killer_colour,
                    victim: name(victim_team, player),
                    victim_colour,
                    background: if i == 0 { local_bg } else { base_bg },
                    icon_name: icon_name(killer_team, victim_team, player),
                }
            })
            .collect();

        // RightJustify is no schema row; its default is 1 (`hud_basedeathnotice.h:133`).
        let right_justify = part
            .value("RightJustify")
            .is_none_or(|l| atoi(l.text().as_bytes()) != 0);
        let margin = (10.0 * f64::from(screen.wide()) / 640.0) as i32;
        Some(DeathNotices {
            lines,
            right_justify,
            margin,
            // The engine reads a float but hands it to the int scaler, so it truncates first.
            corner_radius: scale.scale(number(part, "CornerRadius")),
            font: text(part, "TextFont").unwrap_or("").to_owned(),
            icon_colour: colour("IconColor"),
        })
    }
}

/// The part's text for `key`, else the HudDeathNotice schema's default.
fn text<'a>(part: &'a Part, key: &str) -> Option<&'a str> {
    part.value(key).map(|l| l.text()).or_else(|| {
        ClassSchema::for_part(part)
            .get(key)
            .and_then(|spec| spec.default())
    })
}

/// `text` read with the layout's `atoi`; 0 when neither the part nor the schema has it.
fn number(part: &Part, key: &str) -> i32 {
    text(part, key).map_or(0, |t| atoi(t.as_bytes()))
}
