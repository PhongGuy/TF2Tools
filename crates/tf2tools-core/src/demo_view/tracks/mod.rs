//! Every player's positions over a whole demo, cut into runs the viewer may blend inside.

use crate::demo_view::snapshot::{DemoSnapshot, SnapshotPlayer};
use crate::hud_maker::Team;
use crate::weapons::TfClass;

mod build;
mod hermite;
// Task 3 wires the fit into `eye_at`; until then only the tests call it.
#[allow(dead_code)]
mod smooth;

pub(crate) use build::TrackBuilder;

/// The most of a demo read for its tracks; a longer demo plays up to where this prefix ends.
pub const MAX_DEMO_BYTES: u64 = 512 * 1024 * 1024;
/// The most samples the tracks hold; the timeline ends at the last frame kept under it.
pub const MAX_TRACK_SAMPLES: usize = 8_000_000;

/// The sample flag bit for a crouched player.
pub(crate) const DUCKED: u8 = 1;

/// One player at one tick, packed to 24 bytes: angles in centidegrees, the eye in whole units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TrackSample {
    pub tick: u32,
    pub feet: [f32; 3],
    pub yaw: i16,
    pub pitch: i16,
    pub eye_z: u8,
    pub flags: u8,
}

/// Samples of one player that may be blended: one team, one class, strictly increasing ticks.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TrackRun {
    pub team: Team,
    pub class: TfClass,
    pub samples: Vec<TrackSample>,
}

/// One player entity and its runs in tick order.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlayerTrack {
    pub entity: u32,
    pub runs: Vec<TrackRun>,
}

/// Why the tracks end before the demo does, and at which tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TrackCut {
    /// The demo is longer than `MAX_DEMO_BYTES`.
    FileCap {
        /// The last tick the tracks hold.
        at_tick: u32,
    },
    /// The demo holds more than `MAX_TRACK_SAMPLES` samples.
    SampleCap {
        /// The last tick the tracks hold.
        at_tick: u32,
    },
    /// The file ends well before the tick count its header claims.
    EndedEarly {
        /// The last tick the tracks hold.
        at_tick: u32,
    },
}

/// A player in a demo, named by its entity index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct PlayerId(pub(crate) u32);

impl PlayerId {
    /// The player's entity index in the demo.
    pub fn entity(self) -> u32 {
        self.0
    }
}

/// A player the tracks hold, with the cleaned name the demo gave it, if any.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct TrackedPlayer {
    id: PlayerId,
    name: Option<String>,
}

impl TrackedPlayer {
    pub(crate) fn new(id: PlayerId, name: Option<String>) -> Self {
        Self { id, name }
    }

    /// The player's id.
    pub fn id(&self) -> PlayerId {
        self.id
    }

    /// The player's name, without control characters and at most 32 characters long.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
}

/// The longest name kept whole; a longer one is cut to this many characters plus an ellipsis.
const NAME_KEEP: usize = 31;

/// `raw` without control characters and trimmed, cut to 32 characters; None when nothing is left.
pub(crate) fn clean_name(raw: &str) -> Option<String> {
    let kept: String = raw.chars().filter(|c| !c.is_control()).collect();
    let trimmed = kept.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.chars().count() <= NAME_KEEP + 1 {
        return Some(trimmed.to_owned());
    }
    let mut cut: String = trimmed.chars().take(NAME_KEEP).collect();
    cut.push('…');
    Some(cut)
}

/// Every RED and BLU player's positions over the read part of a demo.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct DemoTracks {
    pub(crate) players: Vec<PlayerTrack>,
    pub(crate) names: Vec<TrackedPlayer>,
    pub(crate) first_tick: u32,
    pub(crate) last_tick: u32,
    pub(crate) seconds_per_tick: f32,
    pub(crate) round_starts: Vec<u32>,
    pub(crate) cut: Option<TrackCut>,
}

impl DemoTracks {
    /// The first kept tick.
    pub fn first_tick(&self) -> u32 {
        self.first_tick
    }

    /// The last kept tick.
    pub fn last_tick(&self) -> u32 {
        self.last_tick
    }

    /// The length of one tick in seconds.
    pub fn seconds_per_tick(&self) -> f32 {
        self.seconds_per_tick
    }

    /// Every tick where a round's setup ends and it starts running, in tick order.
    pub fn round_starts(&self) -> &[u32] {
        &self.round_starts
    }

    /// Why the tracks stop short of the demo, if they do.
    pub fn cut(&self) -> Option<TrackCut> {
        self.cut
    }

    /// Every player with at least one run, in entity order, parallel to the tracks.
    pub fn players(&self) -> &[TrackedPlayer] {
        &self.names
    }

    /// How many player entities have at least one run.
    pub fn player_count(&self) -> usize {
        self.players.len()
    }

    /// How many runs all players hold together.
    pub fn run_count(&self) -> usize {
        self.players.iter().map(|p| p.runs.len()).sum()
    }

    /// How many samples all runs hold together.
    pub fn sample_count(&self) -> usize {
        self.runs().map(|r| r.samples.len()).sum()
    }

    /// The heap the tracks hold: every vector's capacity times its element size.
    pub fn heap_bytes(&self) -> usize {
        let players = self.players.capacity() * size_of::<PlayerTrack>();
        let runs: usize = self
            .players
            .iter()
            .map(|p| p.runs.capacity() * size_of::<TrackRun>())
            .sum();
        let samples: usize = self
            .runs()
            .map(|r| r.samples.capacity() * size_of::<TrackSample>())
            .sum();
        let names = self.names.capacity() * size_of::<TrackedPlayer>()
            + self
                .names
                .iter()
                .map(|p| p.name.as_ref().map_or(0, String::capacity))
                .sum::<usize>();
        players + runs + samples + names + self.round_starts.capacity() * size_of::<u32>()
    }

    /// How many samples repeat the previous one of their run in feet, yaw and pitch.
    pub fn repeated_sample_count(&self) -> usize {
        self.runs()
            .flat_map(|r| r.samples.windows(2))
            .filter(|w| (w[0].feet, w[0].yaw, w[0].pitch) == (w[1].feet, w[1].yaw, w[1].pitch))
            .count()
    }

    /// Every player drawn at the fractional `tick`, in entity order, blended inside its run.
    /// A player whose runs do not cover the tick, or any player at a NaN tick, is left out.
    pub fn at(&self, tick: f64) -> DemoSnapshot {
        let mut players = Vec::with_capacity(self.players.len());
        if !tick.is_nan() {
            players.extend(self.players.iter().filter_map(|p| player_at(p, tick)));
        }
        DemoSnapshot::new(tick.floor() as u32, players)
    }

    /// Tracks through `frames`, for tests outside the crate: player `i` of each frame is one
    /// player with one run. `None` when no frame holds a player.
    #[doc(hidden)]
    pub fn from_keyframes(
        seconds_per_tick: f32,
        frames: &[DemoSnapshot],
        round_starts: Vec<u32>,
        cut: Option<TrackCut>,
    ) -> Option<DemoTracks> {
        let mut runs: Vec<TrackRun> = Vec::new();
        let (mut first_tick, mut last_tick, mut kept) = (None, None, None::<u32>);
        for frame in frames {
            if kept.is_some_and(|k| frame.tick() <= k) {
                continue;
            }
            kept = Some(frame.tick());
            for (i, p) in frame.players().iter().enumerate() {
                if i == runs.len() {
                    runs.push(TrackRun {
                        team: p.team(),
                        class: p.class(),
                        samples: Vec::new(),
                    });
                }
                runs[i].samples.push(keyframe_sample(p, frame.tick()));
            }
            if !frame.players().is_empty() {
                first_tick.get_or_insert(frame.tick());
                last_tick = Some(frame.tick());
            }
        }
        let players: Vec<PlayerTrack> = runs
            .into_iter()
            .zip(1..)
            .map(|(run, entity)| PlayerTrack {
                entity,
                runs: vec![run],
            })
            .collect();
        let names = players
            .iter()
            .map(|p| TrackedPlayer::new(PlayerId(p.entity), None))
            .collect();
        Some(DemoTracks {
            players,
            names,
            first_tick: first_tick?,
            last_tick: last_tick?,
            seconds_per_tick,
            round_starts,
            cut,
        })
    }

    fn runs(&self) -> impl Iterator<Item = &TrackRun> {
        self.players.iter().flat_map(|p| &p.runs)
    }
}

// The player at `tick` from the run covering it: the last sample held for one tick after it,
// otherwise each axis through a monotone cubic over the four samples around the segment.
fn player_at(track: &PlayerTrack, tick: f64) -> Option<SnapshotPlayer> {
    let first = |r: &TrackRun| {
        r.samples
            .first()
            .map_or(f64::INFINITY, |s| f64::from(s.tick))
    };
    let found = track
        .runs
        .partition_point(|r| first(r) <= tick)
        .checked_sub(1)?;
    let run = &track.runs[found];
    let s = &run.samples;
    let last = s.last()?;
    if tick >= f64::from(last.tick) + 1. {
        return None;
    }
    let held = |x: &TrackSample| {
        let yaw = (f32::from(x.yaw) / 100.).rem_euclid(360.);
        let eye = f32::from(x.eye_z);
        (
            x.feet,
            yaw,
            f32::from(x.pitch) / 100.,
            eye,
            x.flags & DUCKED != 0,
        )
    };
    let (feet, yaw, pitch, eye, ducked) = if tick >= f64::from(last.tick) {
        held(last)
    } else {
        let i = s.partition_point(|x| f64::from(x.tick) <= tick) - 1;
        let (lo, hi) = (i.saturating_sub(1), (i + 2).min(s.len() - 1));
        let window = &s[lo..=hi];
        let w = window.len();
        let mut xs = [0f64; 4];
        for (x, k) in xs.iter_mut().zip(window) {
            *x = f64::from(k.tick);
        }
        let axis = |f: &dyn Fn(&TrackSample) -> f64| {
            let mut ys = [0f64; 4];
            for (y, k) in ys.iter_mut().zip(window) {
                *y = f(k);
            }
            hermite::monotone(&xs[..w], &ys[..w], tick)
        };
        // Yaw is unwrapped along the window so the blend turns the short way round.
        let mut yaws = [0f64; 4];
        for (j, k) in window.iter().enumerate() {
            let raw = f64::from(k.yaw) / 100.;
            yaws[j] = match j {
                0 => raw,
                _ => yaws[j - 1] + (raw - yaws[j - 1] + 180.).rem_euclid(360.) - 180.,
            };
        }
        let yaw = hermite::monotone(&xs[..w], &yaws[..w], tick).rem_euclid(360.) as f32;
        let (a, b) = (&s[i], &s[i + 1]);
        let t = ((tick - f64::from(a.tick)) / f64::from(b.tick - a.tick)) as f32;
        let eye = f32::from(a.eye_z) + (f32::from(b.eye_z) - f32::from(a.eye_z)) * t;
        let feet = [0, 1, 2].map(|c| axis(&|k| f64::from(k.feet[c])) as f32);
        let pitch = axis(&|k| f64::from(k.pitch) / 100.) as f32;
        (feet, yaw, pitch, eye, a.flags & DUCKED != 0)
    };
    // A rounding f32 cast can land a tiny negative yaw on 360 itself.
    let yaw = if yaw >= 360. { 0. } else { yaw };
    Some(
        SnapshotPlayer::new(run.team, run.class, feet, yaw, pitch, eye, ducked)
            .with_id(PlayerId(track.entity)),
    )
}

fn keyframe_sample(p: &SnapshotPlayer, tick: u32) -> TrackSample {
    pack_sample(
        tick,
        p.feet(),
        p.yaw(),
        p.pitch(),
        p.eye_height(),
        p.ducked(),
    )
}

/// One player packed into a sample: yaw wrapped to -180..180 so its centidegrees fit an i16,
/// pitch in centidegrees, and the eye rounded into 1..=100.
pub(crate) fn pack_sample(
    tick: u32,
    feet: [f32; 3],
    yaw: f32,
    pitch: f32,
    eye: f32,
    ducked: bool,
) -> TrackSample {
    let yaw = (yaw + 180.).rem_euclid(360.) - 180.;
    TrackSample {
        tick,
        feet,
        yaw: (yaw * 100.).round() as i16,
        pitch: (pitch * 100.).round() as i16,
        eye_z: eye.round().clamp(1., 100.) as u8,
        flags: if ducked { DUCKED } else { 0 },
    }
}

/// The tick length: the server's interval when plausible, else the header's average, else 66 tick.
pub(crate) fn seconds_per_tick(server: f32, header_duration: f32, header_ticks: u32) -> f32 {
    let plausible = |s: f32| s.is_finite() && (0.001..=0.1).contains(&s);
    if plausible(server) {
        return server;
    }
    if header_ticks > 0 {
        let average = header_duration / header_ticks as f32;
        if plausible(average) {
            return average;
        }
    }
    1. / 66.
}

#[cfg(test)]
mod tests {
    use super::build::TrackBuilder;
    use super::*;
    use crate::demo_view::snapshot::{TickFrame, TickRow};
    use crate::demo_view::test_support;
    use crate::hud_maker::Team;
    use crate::weapons::TfClass;
    use std::collections::BTreeMap;

    fn row(entity: u32, x: f32, yaw: f32) -> TickRow {
        TickRow {
            entity,
            team: Some(Team::Blue),
            class: Some(TfClass::Scout),
            feet: [x, 10., 64.],
            yaw,
            pitch: 0.,
            view_offset_z: Some(65.),
            ducked: false,
            has_userinfo: true,
            deleted: false,
            connected: true,
            life_alive: true,
            resource_alive: true,
            in_pvs: true,
        }
    }

    // One frame per (tick, rows) pair at 66 tick.
    fn tracks(frames: Vec<(u32, Vec<TickRow>)>) -> DemoTracks {
        let mut builder = TrackBuilder::new(MAX_TRACK_SAMPLES);
        for (tick, rows) in frames {
            builder.push(
                &TickFrame {
                    tick,
                    round_state: 4,
                    rows,
                },
                1. / 66.,
            );
        }
        builder.finish(1. / 66., 0, false).expect("samples")
    }

    fn x_at(t: &DemoTracks, tick: f64) -> Option<f32> {
        t.at(tick).players().first().map(|p| p.feet()[0])
    }

    #[test]
    fn a_player_moves_between_samples() {
        let t = tracks(
            (0..3)
                .map(|i| (i * 2, vec![row(1, i as f32 * 20., 90.)]))
                .collect(),
        );
        let got = x_at(&t, 1.).expect("drawn");
        assert!((got - 10.).abs() < 1e-3, "{got}");
        assert_eq!(t.at(1.5).tick(), 1);
    }

    #[test]
    fn the_last_sample_holds_for_one_tick_and_nothing_before_the_first() {
        let t = tracks((2..5).map(|i| (i, vec![row(1, i as f32, 90.)])).collect());
        assert_eq!(x_at(&t, 4.5), Some(4.));
        assert_eq!(x_at(&t, 5.), None);
        assert_eq!(x_at(&t, 1.9), None);
        assert_eq!(x_at(&t, f64::NAN), None);
    }

    #[test]
    fn nothing_blends_across_a_teleport() {
        let mut frames: Vec<_> = (0..5).map(|i| (i, vec![row(1, 0., 90.)])).collect();
        frames.extend((5..10).map(|i| (i, vec![])));
        frames.extend((10..15).map(|i| (i, vec![row(1, 5000., 90.)])));
        let t = tracks(frames);
        assert_eq!(x_at(&t, 4.9), Some(0.));
        assert_eq!(x_at(&t, 7.), None);
        assert_eq!(x_at(&t, 10.), Some(5000.));
        assert_eq!(x_at(&t, 10.5), Some(5000.));
    }

    #[test]
    fn yaw_turns_the_short_way_round() {
        let t = tracks(vec![(0, vec![row(1, 0., 359.)]), (2, vec![row(1, 0., 1.)])]);
        let yaw = t.at(1.).players()[0].yaw();
        assert!((0. ..360.).contains(&yaw), "{yaw}");
        let off = (yaw - 0.).abs().min((yaw - 360.).abs());
        assert!(off < 0.02, "{yaw}");
        let t = tracks(vec![
            (0, vec![row(1, 0., 170.)]),
            (2, vec![row(1, 0., 190.)]),
        ]);
        let mid = t.at(1.).players()[0].yaw();
        assert!((mid - 180.).abs() < 0.02, "{mid}");
    }

    #[test]
    fn players_come_in_entity_order_with_their_run_team_and_class() {
        let mut red = row(9, 1., 0.);
        red.team = Some(Team::Red);
        let t = tracks(vec![
            (0, vec![red, row(2, 2., 0.)]),
            (1, vec![row(2, 2., 0.)]),
        ]);
        let snap = t.at(0.);
        let got: Vec<(Team, f32)> = snap
            .players()
            .iter()
            .map(|p| (p.team(), p.feet()[0]))
            .collect();
        assert_eq!(got, [(Team::Blue, 2.), (Team::Red, 1.)]);
        assert_eq!(snap.players()[0].class(), TfClass::Scout);
        assert_eq!(snap.players()[0].eye_height(), 65.);
    }

    #[test]
    fn the_window_matches_the_whole_run() {
        let ticks = [0u32, 1, 3, 4, 7, 9, 10, 14, 15, 18];
        let xs: Vec<f32> = [0., 3., 4., 10., 12., 40., 41., 41., 80., 90.].to_vec();
        let t = tracks(
            ticks
                .iter()
                .zip(&xs)
                .map(|(&k, &x)| (k, vec![row(1, x, 0.)]))
                .collect(),
        );
        let kx: Vec<f64> = ticks.iter().map(|&k| f64::from(k)).collect();
        let ky: Vec<f64> = xs.iter().map(|&x| f64::from(x)).collect();
        for i in 0..180 {
            let tick = f64::from(i) / 10.;
            let want = hermite::monotone(&kx, &ky, tick) as f32;
            let got = x_at(&t, tick).expect("drawn");
            assert!((got - want).abs() < 1e-3, "tick {tick}: {got} vs {want}");
        }
    }

    #[test]
    fn the_server_interval_wins_then_the_header_then_66_tick() {
        assert_eq!(seconds_per_tick(0.015, 0., 0), 0.015);
        assert_eq!(seconds_per_tick(0., 60., 3960), 60. / 3960.);
        assert_eq!(seconds_per_tick(f32::NAN, 0., 0), 1. / 66.);
        assert_eq!(seconds_per_tick(5., 1., 0), 1. / 66.);
        assert_eq!(seconds_per_tick(0., 1e9, 10), 1. / 66.);
    }

    #[test]
    fn players_come_in_entity_order_with_clean_names() {
        let names = BTreeMap::from([(9, "Sniper\u{7}Main".to_owned()), (2, "   ".to_owned())]);
        let t = test_support::tracks_named(
            vec![(
                0,
                vec![
                    test_support::row(9, [1., 1., 1.], 0.),
                    test_support::row(2, [2., 2., 2.], 0.),
                ],
            )],
            names,
        );
        let got: Vec<(u32, Option<&str>)> = t
            .players()
            .iter()
            .map(|p| (p.id().entity(), p.name()))
            .collect();
        assert_eq!(got, [(2, None), (9, Some("SniperMain"))]);
    }

    #[test]
    fn at_sets_each_players_id() {
        let t = test_support::tracks(vec![(
            0,
            vec![
                test_support::row(4, [1., 1., 1.], 0.),
                test_support::row(7, [2., 2., 2.], 0.),
            ],
        )]);
        let ids: Vec<u32> = t.at(0.).players().iter().map(|p| p.id().entity()).collect();
        assert_eq!(ids, [4, 7]);
    }

    #[test]
    fn names_lose_control_characters_and_are_capped_at_32() {
        assert_eq!(clean_name("  Bob\n\t "), Some("Bob".to_owned()));
        assert_eq!(clean_name("\u{1b}[31m"), Some("[31m".to_owned()));
        assert_eq!(clean_name("\u{0}\u{7f}"), None);
        assert_eq!(clean_name(""), None);
        let exact: String = "é".repeat(32);
        assert_eq!(clean_name(&exact), Some(exact.clone()));
        let long: String = "é".repeat(40);
        let cut = clean_name(&long).expect("a name");
        assert_eq!(cut.chars().count(), 32);
        assert_eq!(cut, format!("{}…", "é".repeat(31)));
    }

    #[test]
    fn a_snapshot_player_built_by_hand_has_entity_zero() {
        let p = SnapshotPlayer::new(Team::Red, TfClass::Scout, [0.; 3], 0., 0., 65., false);
        assert_eq!(p.id().entity(), 0);
        assert_eq!(p.with_id(PlayerId(5)).id().entity(), 5);
    }
}
