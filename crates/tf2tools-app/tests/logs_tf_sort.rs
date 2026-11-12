//! The players table's order: logs.tf's default, any column both ways, stable ties.

use std::cmp::Ordering;

use tf2tools_app::pages::logs_tf::sort::{
    PlayerColumn, PlayerSort, sort_players, sort_players_shown,
};
use tf2tools_core::{LogDetail, PlayerLine, Team, TfClass, parse_log};

use crate::support::logstf::{HIGHLANDER, SIXES};

fn six() -> LogDetail {
    parse_log(SIXES).unwrap()
}

fn main_class(p: &PlayerLine) -> usize {
    p.classes()
        .first()
        .and_then(|c| TfClass::ALL.iter().position(|k| *k == c.class()))
        .unwrap_or(TfClass::ALL.len())
}

// The test's own reading of each column, independent of the page's.
fn compare(column: PlayerColumn, a: &PlayerLine, b: &PlayerLine) -> Ordering {
    let team = |p: &PlayerLine| u8::from(p.team() != Team::Blue);
    let num = |f: fn(&PlayerLine) -> f64| f(a).total_cmp(&f(b));
    match column {
        PlayerColumn::Team => team(a).cmp(&team(b)),
        PlayerColumn::Name => a.name().to_lowercase().cmp(&b.name().to_lowercase()),
        PlayerColumn::Class => main_class(a).cmp(&main_class(b)),
        PlayerColumn::Kills => num(|p| f64::from(p.kills())),
        PlayerColumn::Assists => num(|p| f64::from(p.assists())),
        PlayerColumn::Deaths => num(|p| f64::from(p.deaths())),
        PlayerColumn::Damage => num(|p| f64::from(p.dmg())),
        PlayerColumn::DamagePerMin => num(|p| f64::from(p.da_per_min())),
        PlayerColumn::KaPerD => num(|p| p.kapd()),
        PlayerColumn::KPerD => num(|p| p.kpd()),
        PlayerColumn::DamageTaken => num(|p| f64::from(p.dt())),
        PlayerColumn::DamageTakenPerMin => num(|p| p.dt_per_min()),
        PlayerColumn::Medkits => num(|p| f64::from(p.medkits())),
        PlayerColumn::Backstabs => num(|p| f64::from(p.backstabs())),
        PlayerColumn::Headshots => num(|p| f64::from(p.headshots())),
        PlayerColumn::Airshots => num(|p| f64::from(p.airshots())),
        PlayerColumn::Caps => num(|p| f64::from(p.caps())),
    }
}

#[test]
fn the_default_order_is_blu_then_red_each_by_main_class() {
    let log = six();
    let order = sort_players(log.players(), PlayerSort::default());
    let players = log.players();
    assert_eq!(order.len(), 12);
    assert_eq!(players[order[0]].name(), "The Cat Doesn't Care");
    assert_eq!(players[order[11]].name(), "Rahmed");
    assert!(
        order[..6].iter().all(|&i| players[i].team() == Team::Blue),
        "BLU first"
    );
    let classes: Vec<usize> = order[..6]
        .iter()
        .map(|&i| main_class(&players[i]))
        .collect();
    assert!(classes.windows(2).all(|w| w[0] <= w[1]), "{classes:?}");
}

#[test]
fn every_column_sorts_both_ways_and_ties_keep_the_default_order() {
    for log in [six(), parse_log(HIGHLANDER).unwrap()] {
        let players = log.players();
        let default = sort_players(players, PlayerSort::default());
        let rank = |i: usize| default.iter().position(|&d| d == i).unwrap();
        for column in PlayerColumn::ALL {
            for descending in [false, true] {
                let order = sort_players(players, PlayerSort { column, descending });
                let mut seen = order.clone();
                seen.sort_unstable();
                assert_eq!(seen, (0..players.len()).collect::<Vec<_>>(), "{column:?}");
                for pair in order.windows(2) {
                    let (a, b) = (&players[pair[0]], &players[pair[1]]);
                    let ord = compare(column, a, b);
                    let ord = if descending { ord.reverse() } else { ord };
                    assert_ne!(
                        ord,
                        Ordering::Greater,
                        "{column:?} {descending}: {} before {}",
                        a.name(),
                        b.name()
                    );
                    if ord == Ordering::Equal {
                        assert!(
                            rank(pair[0]) < rank(pair[1]),
                            "{column:?} tie {} {}",
                            a.name(),
                            b.name()
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn numbers_open_high_to_low_and_a_second_click_flips() {
    let log = six();
    let first = |sort| {
        log.players()[sort_players(log.players(), sort)[0]]
            .name()
            .to_owned()
    };
    let da = PlayerSort::default().clicked(PlayerColumn::Damage);
    assert_eq!(
        da,
        PlayerSort {
            column: PlayerColumn::Damage,
            descending: true
        }
    );
    assert_eq!(first(da), "line between insane and genius");
    assert_eq!(first(da.clicked(PlayerColumn::Damage)), "Meta Knightynski");
    let name = da.clicked(PlayerColumn::Name);
    assert!(!name.descending, "names open A to Z");
    // Plain lowercase order: '[' (0x5B) sorts before 'a'.
    assert_eq!(first(name), "[☁]Emiel");
    let team = PlayerSort::default().clicked(PlayerColumn::Team);
    assert_eq!(
        team,
        PlayerSort {
            column: PlayerColumn::Team,
            descending: true
        }
    );
    assert_eq!(first(team), "Arie - serveme.tf");
    let titles: Vec<&str> = PlayerColumn::ALL
        .into_iter()
        .map(PlayerColumn::title)
        .collect();
    assert_eq!(
        titles,
        [
            "Team", "Name", "C", "K", "A", "D", "DA", "DA/M", "KA/D", "K/D", "DT", "DT/M", "HP",
            "BS", "HS", "AS", "CAP"
        ]
    );
}

#[test]
fn a_sort_by_a_column_the_log_hides_falls_back_to_the_default_unhighlighted() {
    let sixes = six();
    let hl = parse_log(HIGHLANDER).unwrap();
    let hs = PlayerSort::default().clicked(PlayerColumn::Headshots);
    assert_eq!(hs.shown_in(hl.columns()), Some(hs), "Highlander records HS");
    assert_eq!(hs.shown_in(sixes.columns()), None, "6v6 hides HS");
    let da = PlayerSort::default().clicked(PlayerColumn::Damage);
    assert_eq!(da.shown_in(sixes.columns()), Some(da));
    assert_eq!(
        PlayerSort::default().shown_in(sixes.columns()),
        None,
        "logs.tf highlights no header until one is clicked"
    );
}

#[test]
fn sorting_by_name_goes_by_the_name_the_table_shows() {
    let log = six();
    let players = log.players();
    let by_name = PlayerSort {
        column: PlayerColumn::Name,
        descending: false,
    };
    let last = *sort_players(players, by_name).last().unwrap();
    let renamed = players[last].steam_id();
    // "!" sorts ahead of every name in the fixture, "[☁]Emiel" included.
    let shown = |p: &PlayerLine| {
        if p.steam_id() == renamed {
            "!".to_owned()
        } else {
            p.name().to_owned()
        }
    };
    assert_eq!(sort_players_shown(players, by_name, &shown)[0], last);
}
