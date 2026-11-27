//! Builds per-player runs of samples from the tick frame stream.

use std::collections::BTreeMap;

use super::projectiles::{ProjectileBuilder, sample_of};
use super::{
    DemoKill, DemoTracks, MAX_KILLS, MAX_PROJECTILE_SAMPLES, MAX_TRACK_SAMPLES, PlayerId,
    PlayerTrack, TrackCut, TrackRun, TrackSample, TrackedPlayer, clean_name, pack_sample,
};
use crate::demo_view::snapshot::{SENTINEL_TICK, Step, TickFrame, TickRow, drawable, eye_height};
use crate::weapons::TfClass;

/// A move faster than this (`sv_maxvelocity` 3500 times the square root of 3) is a teleport.
const MAX_SPEED: f32 = 6100.0;
/// `m_iRoundState` values for a round's setup and its running play.
const PREROUND: i64 = 3;
const RUNNING: i64 = 4;
/// A parse ending more than this many ticks before the header's count is a cut-off file.
const END_SLACK: u32 = 66;

// A player and the serial of the last kept frame it was drawable in; its last run is open
// only while that serial is the previous frame's.
struct Building {
    track: PlayerTrack,
    seen_in: u64,
}

/// The sample limits a builder stops at, each refusing the whole frame that would pass it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Caps {
    pub samples: usize,
    pub projectile_samples: usize,
    pub kills: usize,
}

impl Default for Caps {
    fn default() -> Self {
        Self {
            samples: MAX_TRACK_SAMPLES,
            projectile_samples: MAX_PROJECTILE_SAMPLES,
            kills: MAX_KILLS,
        }
    }
}

/// Turns tick frames into per-player runs, breaking a run wherever blending would lie.
pub(crate) struct TrackBuilder {
    caps: Caps,
    players: BTreeMap<u32, Building>,
    shots: ProjectileBuilder,
    kills: Vec<DemoKill>,
    samples: usize,
    frames: u64,
    first_tick: Option<u32>,
    last_tick: Option<u32>,
    last_state: Option<i64>,
    round_starts: Vec<u32>,
    capped: Option<u32>,
    names: BTreeMap<u32, String>,
}

impl TrackBuilder {
    /// A builder that stops before holding more than `sample_cap` samples.
    pub fn new(sample_cap: usize) -> Self {
        Self::with_caps(Caps {
            samples: sample_cap,
            ..Caps::default()
        })
    }

    /// A builder that stops before passing any of `caps`.
    pub fn with_caps(caps: Caps) -> Self {
        Self {
            caps,
            players: BTreeMap::new(),
            shots: ProjectileBuilder::default(),
            kills: Vec::new(),
            samples: 0,
            frames: 0,
            first_tick: None,
            last_tick: None,
            last_state: None,
            round_starts: Vec::new(),
            capped: None,
            names: BTreeMap::new(),
        }
    }

    /// The raw names by entity that `finish` cleans and hands to the tracks.
    pub fn set_names(&mut self, names: BTreeMap<u32, String>) {
        self.names = names;
    }

    /// Takes one frame and says whether the builder wants more.
    pub fn push(&mut self, frame: &TickFrame, seconds_per_tick: f32) -> Step {
        if self.capped.is_some() {
            return Step::Done;
        }
        if frame.tick >= SENTINEL_TICK {
            return Step::Continue;
        }
        // Ticks must strictly increase. A lone opening frame at the server's own tick is thrown
        // away when the recording restarts near zero; any other step back drops its positions
        // but keeps its kills, since each kill arrives only once.
        if let Some(last) = self.last_tick
            && frame.tick <= last
        {
            if frame.tick < last && self.frames == 1 {
                *self = Self::with_caps(self.caps);
            } else if self.kills.len() + frame.kills.len() > self.caps.kills {
                self.capped = Some(last);
                return Step::Done;
            } else {
                self.kills.extend(frame.kills.iter().map(DemoKill::from));
                return Step::Continue;
            }
        }
        let rows: Vec<&TickRow> = frame.rows.iter().filter(|r| drawable(r)).collect();
        let sampled = frame
            .projectiles
            .iter()
            .filter(|r| sample_of(r).is_some())
            .count();
        if self.samples + rows.len() > self.caps.samples
            || self.shots.sample_count() + sampled > self.caps.projectile_samples
            || self.kills.len() + frame.kills.len() > self.caps.kills
        {
            self.capped = Some(self.last_tick.unwrap_or(frame.tick));
            return Step::Done;
        }

        self.frames += 1;
        self.first_tick.get_or_insert(frame.tick);
        self.last_tick = Some(frame.tick);
        if self.last_state == Some(PREROUND) && frame.round_state == RUNNING {
            self.round_starts.push(frame.tick);
        }
        self.last_state = Some(frame.round_state);
        for row in rows {
            self.add(row, frame.tick, seconds_per_tick);
        }
        self.shots.push(frame.tick, &frame.projectiles);
        self.kills.extend(frame.kills.iter().map(DemoKill::from));
        Step::Continue
    }

    // Appends the row's sample to its player's open run, or starts a new run when the player
    // was missing last frame, changed team or class, or moved impossibly fast.
    fn add(&mut self, row: &TickRow, tick: u32, seconds_per_tick: f32) {
        let (Some(team), Some(class)) = (row.team, row.class) else {
            return;
        };
        let sample = sample(row, class, tick);
        let serial = self.frames;
        let building = self.players.entry(row.entity).or_insert_with(|| Building {
            track: PlayerTrack {
                entity: row.entity,
                runs: Vec::new(),
            },
            seen_in: 0,
        });
        let open = building.seen_in + 1 == serial;
        building.seen_in = serial;
        self.samples += 1;
        if let Some(run) = building.track.runs.last_mut()
            && open
            && run.team == team
            && run.class == class
            && run
                .samples
                .last()
                .is_some_and(|prev| !too_fast(prev, &sample, seconds_per_tick))
        {
            run.samples.push(sample);
            return;
        }
        building.track.runs.push(TrackRun {
            team,
            class,
            samples: vec![sample],
        });
    }

    /// The finished tracks, or None when no sample was kept. The cut is the sample cap first,
    /// then the file cap, then a parse that ended well short of the header's tick count.
    pub fn finish(
        self,
        seconds_per_tick: f32,
        header_ticks: u32,
        file_capped: bool,
    ) -> Option<DemoTracks> {
        let (Some(first_tick), Some(last_tick)) = (self.first_tick, self.last_tick) else {
            return None;
        };
        if self.samples == 0 {
            return None;
        }
        let cut = match self.capped {
            Some(at_tick) => Some(TrackCut::SampleCap { at_tick }),
            None if file_capped => Some(TrackCut::FileCap { at_tick: last_tick }),
            None if header_ticks > 0 && last_tick.saturating_add(END_SLACK) < header_ticks => {
                Some(TrackCut::EndedEarly { at_tick: last_tick })
            }
            None => None,
        };
        let mut players: Vec<PlayerTrack> = self.players.into_values().map(|b| b.track).collect();
        for player in &mut players {
            for run in &mut player.runs {
                run.samples.shrink_to_fit();
            }
            player.runs.shrink_to_fit();
        }
        players.shrink_to_fit();
        let names = players
            .iter()
            .map(|p| {
                let name = self.names.get(&p.entity).and_then(|n| clean_name(n));
                TrackedPlayer::new(PlayerId(p.entity), name)
            })
            .collect();
        let mut round_starts = self.round_starts;
        round_starts.shrink_to_fit();
        let (projectiles, projectile_samples) = self.shots.finish();
        let mut kills = self.kills;
        kills.sort_by_key(DemoKill::tick);
        kills.shrink_to_fit();
        Some(DemoTracks {
            players,
            names,
            first_tick,
            last_tick,
            seconds_per_tick,
            round_starts,
            cut,
            projectiles,
            projectile_samples,
            kills,
        })
    }
}

// The row packed into a sample, with the sent eye height trusted only when plausible.
fn sample(row: &TickRow, class: TfClass, tick: u32) -> TrackSample {
    let eye = eye_height(row, class);
    pack_sample(tick, row.feet, row.yaw, row.pitch, eye, row.ducked)
}

fn too_fast(prev: &TrackSample, next: &TrackSample, seconds_per_tick: f32) -> bool {
    let distance = prev
        .feet
        .iter()
        .zip(next.feet)
        .map(|(a, b)| (b - a) * (b - a))
        .sum::<f32>()
        .sqrt();
    distance / ((next.tick - prev.tick) as f32 * seconds_per_tick) > MAX_SPEED
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo_view::snapshot::{SENTINEL_TICK, Step, TickFrame, TickRow};
    use crate::demo_view::tracks::{DemoTracks, MAX_TRACK_SAMPLES, TrackCut, TrackSample};
    use crate::hud_maker::Team;
    use crate::weapons::TfClass;

    const SPT: f32 = 1.0 / 66.0;

    fn row(entity: u32, x: f32) -> TickRow {
        TickRow {
            entity,
            team: Some(Team::Red),
            class: Some(TfClass::Soldier),
            feet: [x, 10., 64.],
            yaw: 90.,
            pitch: 10.,
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

    fn frame(tick: u32, rows: Vec<TickRow>) -> TickFrame {
        TickFrame {
            tick,
            round_state: 4,
            rows,
            ..Default::default()
        }
    }

    fn build(frames: &[TickFrame]) -> DemoTracks {
        let mut builder = TrackBuilder::new(MAX_TRACK_SAMPLES);
        for f in frames {
            builder.push(f, SPT);
        }
        builder.finish(SPT, 0, false).expect("samples were kept")
    }

    // Each run of `entity` as (first tick, last tick, sample count).
    fn spans(tracks: &DemoTracks, entity: u32) -> Vec<(u32, u32, usize)> {
        let player = tracks
            .players
            .iter()
            .find(|p| p.entity == entity)
            .expect("player");
        player
            .runs
            .iter()
            .map(|r| {
                (
                    r.samples[0].tick,
                    r.samples[r.samples.len() - 1].tick,
                    r.samples.len(),
                )
            })
            .collect()
    }

    #[test]
    fn a_steady_player_makes_one_run() {
        let frames: Vec<_> = (0..10)
            .map(|t| frame(t, vec![row(1, t as f32 * 5.)]))
            .collect();
        let tracks = build(&frames);
        assert_eq!(spans(&tracks, 1), [(0, 9, 10)]);
        assert_eq!((tracks.first_tick(), tracks.last_tick()), (0, 9));
        assert_eq!(tracks.sample_count(), 10);
        assert_eq!(tracks.cut(), None);
    }

    #[test]
    fn an_undrawable_frame_breaks_the_run() {
        let frames: Vec<_> = (0..10)
            .map(|t| {
                let mut r = row(1, t as f32);
                r.life_alive = t != 4;
                frame(t, vec![r])
            })
            .collect();
        assert_eq!(spans(&build(&frames), 1), [(0, 3, 4), (5, 9, 5)]);
    }

    #[test]
    fn a_missing_row_breaks_the_run() {
        let frames: Vec<_> = (0..10)
            .map(|t| {
                frame(
                    t,
                    if t == 4 {
                        vec![]
                    } else {
                        vec![row(1, t as f32)]
                    },
                )
            })
            .collect();
        assert_eq!(spans(&build(&frames), 1), [(0, 3, 4), (5, 9, 5)]);
    }

    #[test]
    fn a_class_or_team_change_breaks_the_run() {
        let frames: Vec<_> = (0..6)
            .map(|t| {
                let mut medic = row(1, 0.);
                if t >= 3 {
                    medic.class = Some(TfClass::Medic);
                }
                let mut turncoat = row(2, 0.);
                if t >= 2 {
                    turncoat.team = Some(Team::Blue);
                }
                frame(t, vec![medic, turncoat])
            })
            .collect();
        let tracks = build(&frames);
        assert_eq!(spans(&tracks, 1), [(0, 2, 3), (3, 5, 3)]);
        assert_eq!(spans(&tracks, 2), [(0, 1, 2), (2, 5, 4)]);
        let runs = &tracks
            .players
            .iter()
            .find(|p| p.entity == 1)
            .expect("medic")
            .runs;
        assert_eq!(
            (runs[0].class, runs[1].class),
            (TfClass::Soldier, TfClass::Medic)
        );
    }

    #[test]
    fn a_teleport_breaks_the_run_and_a_fast_move_does_not() {
        // Entity 1 jumps 2000 units in one tick (132000 u/s); entity 2 runs at 5940 u/s.
        let frames: Vec<_> = (0..4)
            .map(|t| {
                let teleporter = row(1, if t >= 2 { 2000. } else { 0. });
                frame(t, vec![teleporter, row(2, t as f32 * 90.)])
            })
            .collect();
        let tracks = build(&frames);
        assert_eq!(spans(&tracks, 1), [(0, 1, 2), (2, 3, 2)]);
        assert_eq!(spans(&tracks, 2), [(0, 3, 4)]);
    }

    #[test]
    fn speed_counts_the_ticks_between_frames() {
        // Frames two ticks apart, 150 units each: 4950 u/s, one run.
        let frames: Vec<_> = [0, 2, 4]
            .iter()
            .map(|&t| frame(t, vec![row(1, t as f32 * 75.)]))
            .collect();
        assert_eq!(spans(&build(&frames), 1), [(0, 4, 3)]);
    }

    #[test]
    fn the_opening_frame_at_the_server_tick_is_dropped() {
        let mut frames = vec![frame(90_000, vec![row(1, 0.)])];
        frames.extend((0..3).map(|t| frame(t, vec![row(1, t as f32)])));
        let tracks = build(&frames);
        assert_eq!(tracks.first_tick(), 0);
        assert_eq!(spans(&tracks, 1), [(0, 2, 3)]);
    }

    #[test]
    fn a_tick_that_does_not_advance_is_dropped() {
        let ticks = [0, 1, 1, 2, 1, 3];
        let frames: Vec<_> = ticks
            .iter()
            .enumerate()
            .map(|(i, &t)| frame(t, vec![row(1, i as f32 * 10.)]))
            .collect();
        let tracks = build(&frames);
        let samples = &tracks.players[0].runs[0].samples;
        let got: Vec<(u32, f32)> = samples.iter().map(|s| (s.tick, s.feet[0])).collect();
        assert_eq!(got, [(0, 0.), (1, 10.), (2, 30.), (3, 50.)]);
    }

    #[test]
    fn signon_ticks_are_skipped() {
        let mut frames = vec![frame(SENTINEL_TICK, vec![row(1, 0.)])];
        frames.extend((0..3).map(|t| frame(t, vec![row(1, 0.)])));
        assert_eq!(build(&frames).first_tick(), 0);
    }

    #[test]
    fn round_starts_are_every_preround_to_running_edge() {
        let state = |t: u32| match t {
            10..20 | 30..40 => 3,
            _ => 4,
        };
        let frames: Vec<_> = (0..50)
            .map(|t| TickFrame {
                tick: t,
                round_state: state(t),
                rows: vec![row(1, 0.)],
                ..Default::default()
            })
            .collect();
        assert_eq!(build(&frames).round_starts(), [20, 40]);
    }

    #[test]
    fn the_sample_cap_ends_the_timeline_at_the_last_kept_frame() {
        let mut builder = TrackBuilder::new(5);
        let steps: Vec<Step> = (10..13)
            .map(|t| builder.push(&frame(t, vec![row(1, 0.), row(2, 0.)]), SPT))
            .collect();
        assert_eq!(steps, [Step::Continue, Step::Continue, Step::Done]);
        assert_eq!(builder.push(&frame(13, vec![row(1, 0.)]), SPT), Step::Done);
        let tracks = builder.finish(SPT, 0, true).expect("samples");
        assert_eq!((tracks.last_tick(), tracks.sample_count()), (11, 4));
        assert_eq!(tracks.cut(), Some(TrackCut::SampleCap { at_tick: 11 }));
    }

    #[test]
    fn a_cut_off_demo_ends_early_and_a_whole_one_does_not() {
        let frames: Vec<_> = (0..100).map(|t| frame(t, vec![row(1, 0.)])).collect();
        let finish = |header_ticks, file_capped| {
            let mut builder = TrackBuilder::new(MAX_TRACK_SAMPLES);
            for f in &frames {
                builder.push(f, SPT);
            }
            builder
                .finish(SPT, header_ticks, file_capped)
                .expect("samples")
                .cut()
        };
        assert_eq!(
            finish(1000, false),
            Some(TrackCut::EndedEarly { at_tick: 99 })
        );
        assert_eq!(finish(165, false), None);
        assert_eq!(finish(0, false), None);
        assert_eq!(finish(1000, true), Some(TrackCut::FileCap { at_tick: 99 }));
    }

    #[test]
    fn no_drawable_player_gives_no_tracks() {
        let mut builder = TrackBuilder::new(MAX_TRACK_SAMPLES);
        for t in 0..10 {
            let mut r = row(1, 0.);
            r.in_pvs = false;
            builder.push(&frame(t, vec![r]), SPT);
        }
        assert!(builder.finish(SPT, 0, false).is_none());
    }

    #[test]
    fn a_sample_stores_angles_eye_and_duck() {
        assert_eq!(std::mem::size_of::<TrackSample>(), 24);
        let mut ducked = row(1, 0.);
        (
            ducked.yaw,
            ducked.pitch,
            ducked.view_offset_z,
            ducked.ducked,
        ) = (350., -12.34, Some(68.4), true);
        let mut scout = row(2, 0.);
        (scout.class, scout.view_offset_z) = (Some(TfClass::Scout), None);
        let mut zero = row(3, 0.);
        zero.view_offset_z = Some(0.);
        let tracks = build(&[frame(0, vec![ducked, scout, zero])]);
        let first = |e: u32| {
            tracks
                .players
                .iter()
                .find(|p| p.entity == e)
                .expect("p")
                .runs[0]
                .samples[0]
        };
        let d = first(1);
        assert_eq!(
            (d.yaw, d.pitch, d.eye_z, d.flags & 1),
            (-1000, -1234, 68, 1)
        );
        assert_eq!((first(2).eye_z, first(2).flags & 1), (65, 0));
        assert_eq!(first(3).eye_z, 68);
    }

    #[test]
    fn players_are_kept_by_entity() {
        let tracks = build(&[frame(0, vec![row(7, 0.), row(3, 0.)])]);
        let entities: Vec<u32> = tracks.players.iter().map(|p| p.entity).collect();
        assert_eq!(entities, [3, 7]);
        assert_eq!(tracks.player_count(), 2);
    }

    #[test]
    fn heap_bytes_counts_the_samples_and_repeats_are_counted() {
        let frames: Vec<_> = (0..10)
            .map(|t| frame(t, vec![row(1, (t / 2) as f32)]))
            .collect();
        let tracks = build(&frames);
        assert!(tracks.heap_bytes() >= 10 * 24, "{}", tracks.heap_bytes());
        assert!(
            tracks.heap_bytes() < 10 * 24 + 4096,
            "{}",
            tracks.heap_bytes()
        );
        assert_eq!(tracks.repeated_sample_count(), 5);
    }
}
