//! The scoreboard's player lists: columns and rows the game's code sets from the root's width
//! variables and the list's spacing keys (`tf_clientscoreboard.cpp`, `SectionedListPanel.cpp`).

use super::expr::atoi;
use super::{HudModel, Part, PartKey, Rgba, Screen, ScreenScale, Team};

/// `SectionedListPanel.h`: raw pixels, never scaled.
const COLUMN_DATA_INDENT: i32 = 6;
const BUTTON_HEIGHT_SPACER: i32 = 7;
const DEFAULT_LINE_SPACING: i32 = 20;
/// `LayoutPanels` insets the header and every row by this much on the left and top.
const ROW_INSET: i32 = 5;

/// What a player-list column shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ColumnKind {
    /// The medal (competitive rank) image.
    Medal,
    /// The Steam avatar.
    Avatar,
    /// The gap after the avatar.
    Spacer,
    /// The player's name; takes whatever width is left.
    Name,
    /// The killstreak count.
    Killstreak,
    /// The killstreak icon.
    KillstreakImage,
    /// The dominating icon.
    Dominating,
    /// The nemesis icon.
    Nemesis,
    /// The score.
    Score,
    /// The class icon.
    Class,
    /// The ping.
    Ping,
    /// The gap after the medal (MvM table).
    MedalSpacer,
    /// The tour of duty count (MvM table).
    Tour,
    /// Damage dealt (MvM table).
    Damage,
    /// Damage done to tanks (MvM table).
    Tank,
    /// Healing given (MvM table).
    Healing,
    /// Support points (MvM table).
    Support,
    /// Credits collected (MvM table).
    Credits,
    /// The squad surplus marker (MvM table).
    SquadSurplus,
}

/// One column of a player row, in pixels from the row's left.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ListColumn {
    kind: ColumnKind,
    x: i32,
    wide: i32,
}

impl ListColumn {
    /// What the column shows.
    pub fn kind(&self) -> ColumnKind {
        self.kind
    }

    /// Its left edge, from the row's left.
    pub fn x(&self) -> i32 {
        self.x
    }

    /// Its width.
    pub fn wide(&self) -> i32 {
        self.wide
    }
}

/// The columns and rows of one team's player list on one screen, inside the list's box.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PlayerListLayout {
    team: Team,
    columns: Vec<ListColumn>,
    row_wide: i32,
    header_tall: i32,
    spacing: i32,
    gap: i32,
}

impl PlayerListLayout {
    /// Every row's background (`tf_clientscoreboard.cpp:1569`).
    pub const ROW_BACKGROUND: Rgba = Rgba::new(0, 0, 0, 80);
    /// The section header's text colour (`:893`).
    pub const HEADER_COLOUR: Rgba = Rgba::WHITE;
    /// The scheme font every row is drawn in (`:317`, `:1570`).
    pub const ROW_FONT: &'static str = "Default";
    /// An MvM row's background: clear.
    pub const MVM_ROW_BACKGROUND: Rgba = Rgba::new(0, 0, 0, 0);
    /// A dead defender's row background.
    pub const MVM_DEAD_BACKGROUND: Rgba = Rgba::new(78, 78, 78, 150);
    /// A dead defender's text colour.
    pub const MVM_DEAD_COLOUR: Rgba = Rgba::new(117, 107, 94, 255);
    /// The scheme font the MvM table is drawn in.
    pub const MVM_FONT: &'static str = "ScoreboardVerySmall";

    /// The team the list shows.
    pub fn team(&self) -> Team {
        self.team
    }

    /// The columns, left to right.
    pub fn columns(&self) -> &[ListColumn] {
        &self.columns
    }

    /// The left edge of the header and every row.
    pub fn row_x(&self) -> i32 {
        ROW_INSET
    }

    /// The width of the header and every row.
    pub fn row_wide(&self) -> i32 {
        self.row_wide
    }

    /// The section header as (y, tall).
    pub fn header(&self) -> (i32, i32) {
        (ROW_INSET, self.header_tall)
    }

    /// The first `count` player rows as (y, tall); rows past the list's bottom are included.
    pub fn rows(&self, count: usize) -> Vec<(i32, i32)> {
        let first = ROW_INSET.saturating_add(self.header_tall);
        let step = self.spacing.saturating_add(self.gap);
        (0..count)
            .map(|i| {
                let i = i32::try_from(i).unwrap_or(i32::MAX);
                (first.saturating_add(i.saturating_mul(step)), self.spacing)
            })
            .collect()
    }
}

impl HudModel {
    /// The rows of `BluePlayerList` or `RedPlayerList` (a direct child of the scene's root whose
    /// class is SectionedListPanel) or, in MvM, of `MvMPlayerList` (a child of the root's
    /// `MvMScoreboard`) at `screen`; `header_font_px` is the canvas's header font height.
    /// `None` for any other part or scene.
    pub fn player_list(
        &self,
        part: &PartKey,
        screen: Screen,
        header_font_px: i32,
    ) -> Option<PlayerListLayout> {
        let root = self.part(&PartKey::new(self.scene_rule().root()?))?;
        let list = self.part(part)?;
        if !list
            .class()
            .is_some_and(|c| c.eq_ignore_ascii_case("SectionedListPanel"))
        {
            return None;
        }
        if list.name().eq_ignore_ascii_case("MvMPlayerList") {
            let board = self.part(list.parent()?)?;
            let in_board = board.name().eq_ignore_ascii_case("MvMScoreboard")
                && board.parent() == Some(root.key());
            return in_board.then(|| self.mvm_list(list, screen, header_font_px));
        }
        if list.parent() != Some(root.key()) {
            return None;
        }
        let team = if list.name().eq_ignore_ascii_case("BluePlayerList") {
            Team::Blue
        } else if list.name().eq_ignore_ascii_case("RedPlayerList") {
            Team::Red
        } else {
            return None;
        };
        let list_wide = self.layout(screen).get(part)?.wide();
        let scale = ScreenScale::new(screen.tall());
        let width = |key: &str, default: i32| scale.scale(int(root, key).unwrap_or(default));

        // The SDK's column order and defaults; the name column is sized last from what is left.
        let spacer = width("spacer", 5);
        let nemesis = width("nemesis_width", 20);
        let mut columns = [
            (ColumnKind::Medal, width("medal_column_width", 15)),
            (ColumnKind::Avatar, width("avatar_width", 18)),
            (ColumnKind::Spacer, spacer),
            (ColumnKind::Name, 0),
            (ColumnKind::Killstreak, width("killstreak_width", 20)),
            (
                ColumnKind::KillstreakImage,
                width("killstreak_image_width", 20),
            ),
            (ColumnKind::Dominating, nemesis),
            (ColumnKind::Nemesis, nemesis),
            (ColumnKind::Score, width("score_width", 35)),
            (ColumnKind::Class, width("class_width", 35)),
            (ColumnKind::Ping, width("ping_width", 23)),
        ];
        let others = columns
            .iter()
            .fold(0i32, |sum, &(_, wide)| sum.saturating_add(wide));
        columns[3].1 = list_wide
            .saturating_sub(others)
            .saturating_sub(spacer)
            .saturating_sub(COLUMN_DATA_INDENT.saturating_mul(2))
            .max(0);
        let mut x = 0i32;
        let columns = columns
            .iter()
            .map(|&(kind, wide)| {
                let column = ListColumn { kind, x, wide };
                x = x.saturating_add(wide);
                column
            })
            .collect();

        let spacing = match int(list, "linespacing") {
            None | Some(0) => DEFAULT_LINE_SPACING,
            Some(n) => n,
        };
        Some(PlayerListLayout {
            team,
            columns,
            row_wide: list_wide.saturating_sub(2 * ROW_INSET),
            header_tall: header_font_px.saturating_add(BUTTON_HEIGHT_SPACER),
            spacing: scale.scale(spacing),
            gap: scale.scale(int(list, "linegap").unwrap_or(0)),
        })
    }
}

impl HudModel {
    /// The MvM table: the SDK's fifteen columns at their default widths (no corpus HUD sets a
    /// width variable), the name column keeping its own width.
    fn mvm_list(&self, list: &Part, screen: Screen, header_font_px: i32) -> PlayerListLayout {
        use ColumnKind::*;
        let scale = ScreenScale::new(screen.tall());
        let list_wide = self.layout(screen).get(list.key()).map_or(0, |b| b.wide());
        // Widths in game units; the avatar is the one column that does not scale.
        let units = [
            (Medal, 20),
            (MedalSpacer, 4),
            (Avatar, 0),
            (Spacer, 2),
            (Name, 94),
            (Class, 25),
            (Tour, 25),
            (Score, 43),
            (Damage, 43),
            (Tank, 43),
            (Healing, 43),
            (Support, 43),
            (Credits, 43),
            (SquadSurplus, 43),
            (Ping, 25),
        ];
        let mut x = 0i32;
        let columns = units
            .iter()
            .map(|&(kind, unit)| {
                let wide = if kind == Avatar {
                    32
                } else {
                    scale.scale(unit)
                };
                let column = ListColumn { kind, x, wide };
                x = x.saturating_add(wide);
                column
            })
            .collect();
        let spacing = match int(list, "linespacing") {
            None | Some(0) => DEFAULT_LINE_SPACING,
            Some(n) => n,
        };
        PlayerListLayout {
            team: Team::Red,
            columns,
            row_wide: list_wide.saturating_sub(2 * ROW_INSET),
            header_tall: header_font_px.saturating_add(BUTTON_HEIGHT_SPACER),
            spacing: scale.scale(spacing),
            gap: scale.scale(int(list, "linegap").unwrap_or(0)),
        }
    }
}

fn int(part: &Part, key: &str) -> Option<i32> {
    part.value(key).map(|l| atoi(l.text().as_bytes()))
}
