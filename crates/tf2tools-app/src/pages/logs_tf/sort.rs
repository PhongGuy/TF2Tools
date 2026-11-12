//! The logs.tf tables' orders: the open log's players table (logs.tf's default, BLU first, then
//! main class in TF2 order, or any column, ties keeping that default), its kill matrix and the
//! class averages table.

use std::cmp::Ordering;

use tf2tools_core::{ClassAverage, LogColumns, MatrixRow, PlayerLine, Team, TfClass};

/// One players table column, in logs.tf's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerColumn {
    Team,
    Name,
    Class,
    Kills,
    Assists,
    Deaths,
    Damage,
    DamagePerMin,
    KaPerD,
    KPerD,
    DamageTaken,
    DamageTakenPerMin,
    Medkits,
    Backstabs,
    Headshots,
    Airshots,
    Caps,
}

impl PlayerColumn {
    /// Every column in logs.tf's order.
    pub const ALL: [PlayerColumn; 17] = [
        Self::Team,
        Self::Name,
        Self::Class,
        Self::Kills,
        Self::Assists,
        Self::Deaths,
        Self::Damage,
        Self::DamagePerMin,
        Self::KaPerD,
        Self::KPerD,
        Self::DamageTaken,
        Self::DamageTakenPerMin,
        Self::Medkits,
        Self::Backstabs,
        Self::Headshots,
        Self::Airshots,
        Self::Caps,
    ];

    /// The column's header on logs.tf.
    pub fn title(self) -> &'static str {
        match self {
            Self::Team => "Team",
            Self::Name => "Name",
            Self::Class => "C",
            Self::Kills => "K",
            Self::Assists => "A",
            Self::Deaths => "D",
            Self::Damage => "DA",
            Self::DamagePerMin => "DA/M",
            Self::KaPerD => "KA/D",
            Self::KPerD => "K/D",
            Self::DamageTaken => "DT",
            Self::DamageTakenPerMin => "DT/M",
            Self::Medkits => "HP",
            Self::Backstabs => "BS",
            Self::Headshots => "HS",
            Self::Airshots => "AS",
            Self::Caps => "CAP",
        }
    }

    /// Whether a first click sorts high to low: numbers do, team, name and class do not.
    pub fn descending_first(self) -> bool {
        !matches!(self, Self::Team | Self::Name | Self::Class)
    }

    /// Whether a log with `columns` shows this column; the first ten always show.
    pub fn recorded(self, columns: &LogColumns) -> bool {
        match self {
            Self::DamageTaken | Self::DamageTakenPerMin => columns.dt(),
            Self::Medkits => columns.hp(),
            Self::Backstabs => columns.bs(),
            Self::Headshots => columns.hs(),
            Self::Airshots => columns.airshots(),
            Self::Caps => columns.caps(),
            Self::Team
            | Self::Name
            | Self::Class
            | Self::Kills
            | Self::Assists
            | Self::Deaths
            | Self::Damage
            | Self::DamagePerMin
            | Self::KaPerD
            | Self::KPerD => true,
        }
    }

    // This column's ascending order of two players, names read through `name`.
    fn compare(
        self,
        a: &PlayerLine,
        b: &PlayerLine,
        name: &dyn Fn(&PlayerLine) -> String,
    ) -> Ordering {
        let num = |f: fn(&PlayerLine) -> f64| f(a).total_cmp(&f(b));
        match self {
            Self::Team => blu_first(a).cmp(&blu_first(b)),
            Self::Name => name(a).to_lowercase().cmp(&name(b).to_lowercase()),
            Self::Class => main_class(a).cmp(&main_class(b)),
            Self::Kills => a.kills().cmp(&b.kills()),
            Self::Assists => a.assists().cmp(&b.assists()),
            Self::Deaths => a.deaths().cmp(&b.deaths()),
            Self::Damage => a.dmg().cmp(&b.dmg()),
            Self::DamagePerMin => a.da_per_min().cmp(&b.da_per_min()),
            Self::KaPerD => num(PlayerLine::kapd),
            Self::KPerD => num(PlayerLine::kpd),
            Self::DamageTaken => a.dt().cmp(&b.dt()),
            Self::DamageTakenPerMin => num(PlayerLine::dt_per_min),
            Self::Medkits => a.medkits().cmp(&b.medkits()),
            Self::Backstabs => a.backstabs().cmp(&b.backstabs()),
            Self::Headshots => a.headshots().cmp(&b.headshots()),
            Self::Airshots => a.airshots().cmp(&b.airshots()),
            Self::Caps => a.caps().cmp(&b.caps()),
        }
    }
}

/// The players table's sort: a column and its direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerSort {
    pub column: PlayerColumn,
    pub descending: bool,
}

impl Default for PlayerSort {
    /// Team ascending, which is logs.tf's default order.
    fn default() -> Self {
        Self {
            column: PlayerColumn::Team,
            descending: false,
        }
    }
}

impl PlayerSort {
    /// The sort after a click on `column`: the same column flips, another opens in its own
    /// first direction.
    pub fn clicked(self, column: PlayerColumn) -> PlayerSort {
        let descending = if column == self.column {
            !self.descending
        } else {
            column.descending_first()
        };
        PlayerSort { column, descending }
    }

    /// The sort a log with `columns` shows and highlights; None for the default order or a
    /// column the log hides, which then lists in the default order with no header lit.
    pub fn shown_in(self, columns: &LogColumns) -> Option<PlayerSort> {
        (self != Self::default() && self.column.recorded(columns)).then_some(self)
    }
}

/// Indexes into `players` in display order by their names in the log; ties keep the default
/// order.
pub fn sort_players(players: &[PlayerLine], sort: PlayerSort) -> Vec<usize> {
    sort_players_shown(players, sort, &|p| p.name().to_owned())
}

/// Like `sort_players`, with the Name column going by `shown`, the name the table shows.
pub fn sort_players_shown(
    players: &[PlayerLine],
    sort: PlayerSort,
    shown: &dyn Fn(&PlayerLine) -> String,
) -> Vec<usize> {
    // Both sorts are stable, so the default order breaks every column's ties.
    let mut order: Vec<usize> = (0..players.len()).collect();
    order.sort_by_key(|&i| (blu_first(&players[i]), main_class(&players[i])));
    order.sort_by(|&a, &b| {
        let ord = sort.column.compare(&players[a], &players[b], shown);
        if sort.descending { ord.reverse() } else { ord }
    });
    order
}

fn blu_first(p: &PlayerLine) -> u8 {
    u8::from(p.team() != Team::Blue)
}

// A player's longest-played class in TF2's order; one without class lines sorts last.
fn main_class(p: &PlayerLine) -> usize {
    p.classes()
        .first()
        .and_then(|c| TfClass::ALL.iter().position(|k| *k == c.class()))
        .unwrap_or(TfClass::ALL.len())
}

/// A sort by one column of a table and its direction; the page keeps `None` for the table's
/// default order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NumberSort<C> {
    pub column: C,
    pub descending: bool,
}

/// A column a `NumberSort` can lead with.
pub trait SortColumn: Copy + PartialEq {
    /// Whether a first click sorts high to low: numbers do.
    fn descending_first(self) -> bool {
        true
    }
}

impl<C: SortColumn> NumberSort<C> {
    /// The sort after a click on `column` while `sort` leads: the same column flips, another
    /// opens in its own first direction.
    pub fn clicked(sort: Option<Self>, column: C) -> Self {
        let descending = match sort {
            Some(s) if s.column == column => !s.descending,
            _ => column.descending_first(),
        };
        Self { column, descending }
    }
}

/// One kill matrix column that sorts: a class's counts or the total.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatrixColumn {
    Class(TfClass),
    Total,
}

impl SortColumn for MatrixColumn {}

/// The kill matrix's sort, kept across its three tabs.
pub type MatrixSort = NumberSort<MatrixColumn>;

/// `rows` (in the matrix's default order) by `sort`'s column; ties keep the default order.
#[allow(clippy::expect_used)]
pub fn sort_matrix_rows<'a>(
    rows: Vec<(&'a PlayerLine, &'a MatrixRow)>,
    sort: Option<MatrixSort>,
) -> Vec<(&'a PlayerLine, &'a MatrixRow)> {
    let Some(sort) = sort else {
        return rows;
    };
    // A stable sort, so rows with the same count keep the default order.
    let value = |row: &MatrixRow| match sort.column {
        MatrixColumn::Class(class) => TfClass::ALL
            .iter()
            .position(|c| *c == class)
            .map(|i| row.counts()[i])
            .expect("every class is in TfClass::ALL"),
        MatrixColumn::Total => row.total(),
    };
    let mut rows = rows;
    rows.sort_by(|(_, a), (_, b)| {
        let ord = value(a).cmp(&value(b));
        if sort.descending { ord.reverse() } else { ord }
    });
    rows
}

/// One class table column that sorts, in the table's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassStat {
    Class,
    Played,
    Logs,
    KillsPerMin,
    AssistsPerMin,
    DeathsPerMin,
    DmgPerMin,
    Kad,
    HealsPerMin,
    UbersPer10Min,
}

impl ClassStat {
    /// Every number column in the table's order, after the Class title.
    pub const ALL: [ClassStat; 9] = [
        Self::Played,
        Self::Logs,
        Self::KillsPerMin,
        Self::AssistsPerMin,
        Self::DeathsPerMin,
        Self::DmgPerMin,
        Self::Kad,
        Self::HealsPerMin,
        Self::UbersPer10Min,
    ];

    /// The column's title.
    pub fn title(self) -> &'static str {
        match self {
            Self::Class => "Class",
            Self::Played => "Played",
            Self::Logs => "Logs",
            Self::KillsPerMin => "Kills/min",
            Self::AssistsPerMin => "Assists/min",
            Self::DeathsPerMin => "Deaths/min",
            Self::DmgPerMin => "Dmg/min",
            Self::Kad => "KA/D",
            Self::HealsPerMin => "Heals/min",
            Self::UbersPer10Min => "Ubers/10 min",
        }
    }

    // The value behind the column's text; an empty Medic cell is None.
    fn value(self, a: &ClassAverage) -> Option<f64> {
        match self {
            Self::Class => TfClass::ALL
                .iter()
                .position(|c| *c == a.class())
                .map(|i| i as f64),
            Self::Played => Some(a.seconds() as f64),
            Self::Logs => Some(f64::from(a.logs())),
            Self::KillsPerMin => Some(a.kills_per_min()),
            Self::AssistsPerMin => Some(a.assists_per_min()),
            Self::DeathsPerMin => Some(a.deaths_per_min()),
            Self::DmgPerMin => Some(a.dmg_per_min()),
            Self::Kad => Some(a.kad()),
            Self::HealsPerMin => a.heal_per_min(),
            Self::UbersPer10Min => a.ubers_per_10min(),
        }
    }
}

impl SortColumn for ClassStat {
    // Class opens Scout to Spy; the numbers open high to low.
    fn descending_first(self) -> bool {
        self != Self::Class
    }
}

/// The class table's sort.
pub type ClassSort = NumberSort<ClassStat>;

/// Indexes into `averages` in display order: class order unsorted, else the played classes
/// by `sort` with ties in class order, then the unplayed ones in class order.
pub fn sort_classes(averages: &[ClassAverage], sort: Option<ClassSort>) -> Vec<usize> {
    let mut order: Vec<usize> = (0..averages.len()).collect();
    let Some(sort) = sort else {
        return order;
    };
    // Unplayed classes go last in class order whatever the direction; an empty cell reads
    // lowest. The sort is stable, so ties keep class order.
    let key = |i: usize| sort.column.value(&averages[i]).unwrap_or(f64::NEG_INFINITY);
    order.sort_by(|&a, &b| {
        let (pa, pb) = (averages[a].played(), averages[b].played());
        if !(pa && pb) {
            return pb.cmp(&pa);
        }
        let ord = key(a).total_cmp(&key(b));
        if sort.descending { ord.reverse() } else { ord }
    });
    order
}
