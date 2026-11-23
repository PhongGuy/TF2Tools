//! Picks the snapshot tick from a stream of per-tick player rows.

use super::{DemoSnapshot, SnapshotPlayer, class_eye_height};
use crate::hud_maker::Team;
use crate::weapons::TfClass;

/// Signon packets carry this tick; they come before the first real tick.
pub(crate) const SENTINEL_TICK: u32 = 1 << 31;
/// A pre-round to running edge later than this is not the match start.
const EDGE_SEARCH_TICKS: u32 = 20_000;
/// The window opens this long after its anchor, once players have spawned and left spawn.
const WINDOW_DELAY: u32 = 1000;
const WINDOW_TICKS: u32 = 1000;
/// `m_iRoundState` values for the round before and after its setup countdown.
const PREROUND: i64 = 3;
const RUNNING: i64 = 4;

/// One player entity as the demo describes it at one tick.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TickRow {
    pub entity: u32,
    pub team: Option<Team>,
    pub class: Option<TfClass>,
    pub feet: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub view_offset_z: Option<f32>,
    pub ducked: bool,
    pub has_userinfo: bool,
    pub deleted: bool,
    pub connected: bool,
    pub life_alive: bool,
    pub resource_alive: bool,
    pub in_pvs: bool,
}

/// Every player row of one tick, with the game's round state at that tick.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TickFrame {
    pub tick: u32,
    pub round_state: i64,
    pub rows: Vec<TickRow>,
}

/// Whether the picker wants more frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    Continue,
    Done,
}

/// Whether a row is a live, visible RED or BLU player standing somewhere on the map.
pub(crate) fn drawable(row: &TickRow) -> bool {
    let on_map = row
        .feet
        .iter()
        .all(|c| c.is_finite() && c.abs() <= 65_536.0)
        && row.feet.iter().any(|&c| c != 0.0);
    row.has_userinfo
        && !row.deleted
        && row.connected
        && row.team.is_some()
        && row.class.is_some()
        && row.life_alive
        && row.resource_alive
        && row.in_pvs
        && on_map
}

/// The best tick seen so far in one window and its drawable rows.
struct Candidate {
    tick: u32,
    score: (usize, usize),
    rows: Vec<TickRow>,
}

/// Streams frames and keeps the fullest tick a little after the match starts.
///
/// Two windows are scored: one after the first real tick, as a fallback, and one after the
/// pre-round to running edge, which wins whenever it holds a drawable tick.
#[derive(Default)]
pub(crate) struct SnapshotPicker {
    first_tick: Option<u32>,
    last_state: Option<i64>,
    edge: Option<u32>,
    fallback: Option<Candidate>,
    at_edge: Option<Candidate>,
}

impl SnapshotPicker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes one frame and says whether the pick is settled.
    pub fn push(&mut self, frame: TickFrame) -> Step {
        let tick = frame.tick;
        if tick >= SENTINEL_TICK {
            return Step::Continue;
        }
        let first = *self.first_tick.get_or_insert(tick);
        if self.edge.is_none()
            && tick < EDGE_SEARCH_TICKS
            && self.last_state == Some(PREROUND)
            && frame.round_state == RUNNING
        {
            self.edge = Some(tick);
        }
        self.last_state = Some(frame.round_state);

        // Only frames inside a window pay for filtering; the overlap of both windows feeds both.
        let to_fallback = in_window(first, tick);
        let to_edge = self.edge.is_some_and(|edge| in_window(edge, tick));
        if to_fallback || to_edge {
            let mut rows: Vec<TickRow> = frame.rows.into_iter().filter(drawable).collect();
            rows.sort_by_key(|r| r.entity);
            match (to_fallback, to_edge) {
                (true, true) => {
                    offer(&mut self.fallback, tick, rows.clone());
                    offer(&mut self.at_edge, tick, rows);
                }
                (true, false) => offer(&mut self.fallback, tick, rows),
                (false, _) => offer(&mut self.at_edge, tick, rows),
            }
        }
        self.step(tick)
    }

    fn step(&self, tick: u32) -> Step {
        let done = match self.edge {
            Some(edge) => tick.saturating_add(1) >= window_end(edge),
            None => tick >= EDGE_SEARCH_TICKS,
        };
        if done { Step::Done } else { Step::Continue }
    }

    /// The picked tick and its drawable rows by entity: the edge's window first, else the
    /// first tick's.
    pub fn finish(self) -> Option<(u32, Vec<TickRow>)> {
        self.at_edge
            .or(self.fallback)
            .map(|candidate| (candidate.tick, candidate.rows))
    }
}

fn window_end(anchor: u32) -> u32 {
    anchor.saturating_add(WINDOW_DELAY + WINDOW_TICKS)
}

fn in_window(anchor: u32, tick: u32) -> bool {
    tick >= anchor.saturating_add(WINDOW_DELAY) && tick < window_end(anchor)
}

/// Keeps `rows` when they beat the candidate: most on the smaller team, then most in all.
/// Only a strictly better score replaces, so the earliest tick wins a tie.
fn offer(slot: &mut Option<Candidate>, tick: u32, rows: Vec<TickRow>) {
    let red = rows.iter().filter(|r| r.team == Some(Team::Red)).count();
    let blu = rows.iter().filter(|r| r.team == Some(Team::Blue)).count();
    let score = (red.min(blu), red + blu);
    if rows.is_empty() || slot.as_ref().is_some_and(|best| score <= best.score) {
        return;
    }
    *slot = Some(Candidate { tick, score, rows });
}

/// The snapshot of the picked rows, trusting the sent eye height only when it is plausible.
pub(crate) fn into_snapshot(tick: u32, rows: Vec<TickRow>) -> DemoSnapshot {
    let players = rows
        .into_iter()
        .filter_map(|row| {
            let (team, class) = (row.team?, row.class?);
            // The range check also rejects NaN and the infinities.
            let eye = row
                .view_offset_z
                .filter(|z| (1.0..=100.0).contains(z))
                .unwrap_or_else(|| class_eye_height(class, row.ducked));
            Some(SnapshotPlayer::new(
                team, class, row.feet, row.yaw, row.pitch, eye, row.ducked,
            ))
        })
        .collect();
    DemoSnapshot::new(tick, players)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(entity: u32, team: Team) -> TickRow {
        TickRow {
            entity,
            team: Some(team),
            class: Some(TfClass::Soldier),
            feet: [100., 200., 64.],
            yaw: 90.,
            pitch: 0.,
            view_offset_z: Some(68.),
            ducked: false,
            has_userinfo: true,
            deleted: false,
            connected: true,
            life_alive: true,
            resource_alive: true,
            in_pvs: true,
        }
    }

    // `red` RED rows (entities 1..) and `blu` BLU rows (entities 101..).
    fn teams(red: u32, blu: u32) -> Vec<TickRow> {
        let reds = (1..=red).map(|e| row(e, Team::Red));
        reds.chain((101..101 + blu).map(|e| row(e, Team::Blue)))
            .collect()
    }

    fn frame(tick: u32, round_state: i64, rows: Vec<TickRow>) -> TickFrame {
        TickFrame {
            tick,
            round_state,
            rows,
        }
    }

    // Pushes one frame per tick until `Done`; returns the tick that answered `Done`.
    fn feed(
        picker: &mut SnapshotPicker,
        ticks: impl IntoIterator<Item = u32>,
        state: impl Fn(u32) -> i64,
        rows: impl Fn(u32) -> Vec<TickRow>,
    ) -> Option<u32> {
        ticks
            .into_iter()
            .find(|&tick| picker.push(frame(tick, state(tick), rows(tick))) == Step::Done)
    }

    // product.dem's opening: RUNNING before the match, PREROUND at 334, RUNNING at 668.
    fn product_states(tick: u32) -> i64 {
        match tick {
            0..334 => RUNNING,
            334..668 => PREROUND,
            _ => RUNNING,
        }
    }

    #[test]
    fn a_full_row_is_drawable() {
        assert!(drawable(&row(1, Team::Red)));
        assert!(drawable(&row(2, Team::Blue)));
    }

    #[test]
    fn each_pitfall_makes_a_row_undrawable() {
        type Spoil = fn(&mut TickRow);
        let cases: [(&str, Spoil); 9] = [
            ("userinfo cleared", |r| r.has_userinfo = false),
            ("entity deleted", |r| r.deleted = true),
            ("not connected", |r| r.connected = false),
            ("spectator or unassigned", |r| r.team = None),
            ("unknown class", |r| r.class = None),
            ("lifeState not alive", |r| r.life_alive = false),
            ("resource says dead", |r| r.resource_alive = false),
            ("out of PVS", |r| r.in_pvs = false),
            ("zero origin", |r| r.feet = [0., 0., 0.]),
        ];
        for (what, spoil) in cases {
            let mut r = row(1, Team::Red);
            spoil(&mut r);
            assert!(!drawable(&r), "{what}");
        }
    }

    #[test]
    fn a_bad_origin_is_dropped_and_the_map_edge_is_kept() {
        for feet in [
            [f32::NAN, 0., 64.],
            [0., f32::INFINITY, 64.],
            [65_537., 0., 64.],
            [0., 0., -65_537.],
        ] {
            let mut r = row(1, Team::Red);
            r.feet = feet;
            assert!(!drawable(&r), "{feet:?}");
        }
        for feet in [[65_536., -65_536., 0.], [0., 0., 1.]] {
            let mut r = row(1, Team::Red);
            r.feet = feet;
            assert!(drawable(&r), "{feet:?}");
        }
    }

    #[test]
    fn the_window_opens_1000_ticks_after_the_preround_edge_and_stops_2000_after() {
        let mut picker = SnapshotPicker::new();
        let done = feed(&mut picker, 1..=10_000, product_states, |_| teams(2, 2));
        assert_eq!(done, Some(668 + 1999));
        let (tick, rows) = picker.finish().unwrap();
        // The no-edge window (1001..2001) ties on score but the edge window wins.
        assert_eq!(tick, 1668);
        assert_eq!(rows.len(), 4);
    }

    #[test]
    fn the_opening_running_state_is_not_an_edge() {
        let mut picker = SnapshotPicker::new();
        let done = feed(&mut picker, 5..=30_000, |_| RUNNING, |_| teams(1, 1));
        assert_eq!(done, Some(EDGE_SEARCH_TICKS));
        assert_eq!(picker.finish().unwrap().0, 1005);
    }

    #[test]
    fn the_best_tick_has_the_most_on_the_smaller_team_then_the_most_in_all() {
        let rows = |tick: u32| match tick {
            1700 => teams(3, 1),
            1800 => teams(2, 2),
            1900 => teams(3, 2),
            2000 => teams(2, 3),
            _ => teams(1, 0),
        };
        let mut picker = SnapshotPicker::new();
        feed(&mut picker, 1..=10_000, product_states, rows);
        let (tick, rows) = picker.finish().unwrap();
        assert_eq!(tick, 1900, "2000 ties 1900 and comes later");
        let red = rows.iter().filter(|r| r.team == Some(Team::Red)).count();
        assert_eq!((red, rows.len()), (3, 5));
    }

    #[test]
    fn an_edge_at_tick_20000_is_too_late() {
        let state = |tick: u32| {
            if tick < EDGE_SEARCH_TICKS {
                PREROUND
            } else {
                RUNNING
            }
        };
        let mut picker = SnapshotPicker::new();
        let done = feed(&mut picker, 0..=25_000, state, |_| teams(1, 1));
        assert_eq!(done, Some(EDGE_SEARCH_TICKS));
        assert_eq!(picker.finish().unwrap().0, 1000);
    }

    #[test]
    fn an_edge_at_tick_19999_still_counts() {
        let state = |tick: u32| if tick < 19_999 { PREROUND } else { RUNNING };
        let mut picker = SnapshotPicker::new();
        let done = feed(&mut picker, 0..=25_000, state, |_| teams(1, 1));
        assert_eq!(done, Some(19_999 + 1999));
        assert_eq!(picker.finish().unwrap().0, 20_999);
    }

    #[test]
    fn sentinel_signon_ticks_are_skipped_and_a_header_count_is_never_needed() {
        let mut picker = SnapshotPicker::new();
        for state in [PREROUND, RUNNING, PREROUND] {
            let step = picker.push(frame(SENTINEL_TICK, state, teams(5, 5)));
            assert_eq!(step, Step::Continue);
        }
        // A RUNNING first real tick after a sentinel PREROUND is not an edge.
        let done = feed(&mut picker, 0..=25_000, |_| RUNNING, |_| teams(1, 1));
        assert_eq!(done, Some(EDGE_SEARCH_TICKS));
        assert_eq!(picker.finish().unwrap().0, 1000);
    }

    #[test]
    fn a_demo_cut_before_the_edge_window_falls_back_to_the_first_window() {
        let mut picker = SnapshotPicker::new();
        assert_eq!(
            feed(&mut picker, 1..=1500, product_states, |_| teams(1, 1)),
            None
        );
        assert_eq!(picker.finish().unwrap().0, 1001);
    }

    #[test]
    fn nothing_drawable_or_nothing_pushed_is_none() {
        assert!(SnapshotPicker::new().finish().is_none());
        let mut picker = SnapshotPicker::new();
        let spectators = |_| {
            let mut r = row(1, Team::Red);
            r.team = None;
            vec![r]
        };
        feed(&mut picker, 1..=10_000, product_states, spectators);
        assert!(picker.finish().is_none());
    }

    #[test]
    fn the_pick_keeps_only_drawable_rows_in_entity_order() {
        let rows = |_| {
            let mut dead = row(3, Team::Red);
            dead.life_alive = false;
            vec![row(5, Team::Red), dead, row(2, Team::Blue)]
        };
        let mut picker = SnapshotPicker::new();
        feed(&mut picker, 1..=10_000, product_states, rows);
        let (_, rows) = picker.finish().unwrap();
        let entities: Vec<u32> = rows.iter().map(|r| r.entity).collect();
        assert_eq!(entities, [2, 5]);
    }

    #[test]
    fn the_sent_view_offset_is_the_eye_unless_it_is_absurd() {
        let mut ducked = row(4, Team::Blue);
        ducked.class = Some(TfClass::Scout);
        ducked.ducked = true;
        let cases = [
            (Some(71.5), 71.5),
            (Some(0.5), 45.),
            (Some(f32::NAN), 45.),
            (Some(150.), 45.),
            (None, 45.),
        ];
        for (sent, want) in cases {
            ducked.view_offset_z = sent;
            let snapshot = into_snapshot(9, vec![ducked.clone()]);
            assert_eq!(snapshot.players()[0].eye_height(), want, "{sent:?}");
        }
        let mut medic = row(1, Team::Red);
        medic.class = Some(TfClass::Medic);
        medic.view_offset_z = None;
        let snapshot = into_snapshot(9, vec![medic]);
        assert_eq!(snapshot.tick(), 9);
        let p = &snapshot.players()[0];
        assert_eq!(
            (
                p.team(),
                p.class(),
                p.feet(),
                p.yaw(),
                p.pitch(),
                p.eye_height(),
                p.ducked()
            ),
            (
                Team::Red,
                TfClass::Medic,
                [100., 200., 64.],
                90.,
                0.,
                75.,
                false
            )
        );
    }
}
