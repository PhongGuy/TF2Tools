//! Every projectile's path over a whole demo.

use std::collections::BTreeMap;

use crate::demo_view::snapshot::ProjectileRow;
use crate::hud_maker::Team;

/// What a projectile is, as the viewer draws it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ProjectileKind {
    /// A rocket, from a launcher or a sentry.
    Rocket,
    /// A grenade launcher's pipe.
    Pipe,
    /// A stickybomb.
    Sticky,
    /// A Huntsman arrow or a Crusader's Crossbow bolt.
    Arrow,
    /// A flare.
    Flare,
    /// A Cow Mangler shot.
    EnergyBall,
    /// A Righteous Bison or Pomson shot.
    EnergyRing,
    /// A jar, cleaver or baseball.
    Thrown,
    /// Anything else that flies.
    Other,
}

/// One projectile at one tick, packed to 16 bytes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ProjectileSample {
    pub tick: u32,
    pub origin: [f32; 3],
}

/// One stretch of one projectile's flight, a range of the flat sample vector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ProjectileTrack {
    pub kind: ProjectileKind,
    pub team: Team,
    pub critical: bool,
    pub start: u32,
    pub len: u32,
}

/// The team and origin a row samples at, or None when it makes no sample.
pub(crate) fn sample_of(row: &ProjectileRow) -> Option<(Team, [f32; 3])> {
    let team = row.team?;
    let origin = row.origin?;
    let on_map = origin.iter().all(|c| c.is_finite() && c.abs() <= 65_536.0)
        && origin.iter().any(|&c| c != 0.0);
    on_map.then_some((team, origin))
}

// A track still being extended: how it looks and the samples gathered so far.
struct Open {
    kind: ProjectileKind,
    team: Team,
    critical: bool,
    samples: Vec<ProjectileSample>,
}

/// Turns kept frames of projectile rows into tracks, one per unbroken stretch of a serial.
#[derive(Default)]
pub(crate) struct ProjectileBuilder {
    open: BTreeMap<u32, Open>,
    tracks: Vec<ProjectileTrack>,
    samples: Vec<ProjectileSample>,
    open_samples: usize,
}

impl ProjectileBuilder {
    /// One kept frame's rows at `tick`; rows that make no sample count as missing.
    pub fn push(&mut self, tick: u32, rows: &[ProjectileRow]) {
        // A serial extends only if it was open after the last push and still looks the same;
        // whatever is left in the old open set afterwards went missing and closes.
        let mut previous = std::mem::take(&mut self.open);
        for row in rows {
            let Some((team, origin)) = sample_of(row) else {
                continue;
            };
            let sample = ProjectileSample { tick, origin };
            let mut track = match previous.remove(&row.serial) {
                Some(open)
                    if (open.kind, open.team, open.critical) == (row.kind, team, row.critical) =>
                {
                    open
                }
                Some(old) => {
                    self.close(old);
                    Open::new(row.kind, team, row.critical)
                }
                None => Open::new(row.kind, team, row.critical),
            };
            track.samples.push(sample);
            self.open_samples += 1;
            if let Some(dup) = self.open.insert(row.serial, track) {
                self.close(dup);
            }
        }
        for (_, gone) in previous {
            self.close(gone);
        }
    }

    /// How many samples the builder holds, open and closed.
    pub fn sample_count(&self) -> usize {
        self.samples.len() + self.open_samples
    }

    /// Closes every open track; tracks sorted by first tick, stably.
    pub fn finish(mut self) -> (Vec<ProjectileTrack>, Vec<ProjectileSample>) {
        for (_, open) in std::mem::take(&mut self.open) {
            self.close(open);
        }
        let (mut tracks, mut samples) = (self.tracks, self.samples);
        tracks.sort_by_key(|t| samples[t.start as usize].tick);
        tracks.shrink_to_fit();
        samples.shrink_to_fit();
        (tracks, samples)
    }

    // Moves an open track's samples onto the flat vector and records its range.
    fn close(&mut self, open: Open) {
        let len = open.samples.len();
        self.open_samples -= len;
        if len == 0 {
            return;
        }
        let start = self.samples.len();
        self.samples.extend_from_slice(&open.samples);
        self.tracks.push(ProjectileTrack {
            kind: open.kind,
            team: open.team,
            critical: open.critical,
            start: start as u32,
            len: len as u32,
        });
    }
}

impl Open {
    fn new(kind: ProjectileKind, team: Team, critical: bool) -> Self {
        Self {
            kind,
            team,
            critical,
            samples: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo_view::snapshot::{ProjectileRow, SENTINEL_TICK, Step, TickFrame};
    use crate::demo_view::test_support::{SPT, row};
    use crate::demo_view::tracks::build::{Caps, TrackBuilder};
    use crate::demo_view::tracks::{DemoTracks, TrackCut};
    use crate::hud_maker::Team;

    fn shot(serial: u32, x: f32) -> ProjectileRow {
        ProjectileRow {
            serial,
            kind: ProjectileKind::Rocket,
            team: Some(Team::Red),
            critical: false,
            origin: Some([x, 50., 100.]),
        }
    }

    // One drawable player keeps every frame worth keeping.
    fn frame(tick: u32, shots: Vec<ProjectileRow>) -> TickFrame {
        let mut f = TickFrame {
            tick,
            round_state: 4,
            rows: vec![row(1, [10., 10., 10.], 0.)],
            ..Default::default()
        };
        f.projectiles = shots;
        f
    }

    fn build_with(caps: Caps, frames: &[TickFrame]) -> Option<DemoTracks> {
        let mut builder = TrackBuilder::with_caps(caps);
        for f in frames {
            builder.push(f, SPT);
        }
        builder.finish(SPT, 0, false)
    }

    fn build(frames: &[TickFrame]) -> DemoTracks {
        build_with(Caps::default(), frames).expect("tracks")
    }

    // Each projectile track as (first tick, last tick, sample count).
    fn spans(t: &DemoTracks) -> Vec<(u32, u32, u32)> {
        t.projectiles
            .iter()
            .map(|p| {
                let s = &t.projectile_samples[p.start as usize..(p.start + p.len) as usize];
                (s[0].tick, s[s.len() - 1].tick, p.len)
            })
            .collect()
    }

    #[test]
    fn a_sample_is_sixteen_bytes() {
        assert_eq!(std::mem::size_of::<ProjectileSample>(), 16);
    }

    #[test]
    fn one_serial_makes_one_track() {
        let frames: Vec<_> = (0..10)
            .map(|t| frame(t, vec![shot(1, t as f32 * 10.)]))
            .collect();
        let t = build(&frames);
        assert_eq!(spans(&t), [(0, 9, 10)]);
        assert_eq!((t.projectile_count(), t.projectile_sample_count()), (1, 10));
        let s = &t.projectile_samples;
        assert_eq!((s[3].tick, s[3].origin), (3, [30., 50., 100.]));
    }

    #[test]
    fn a_new_serial_on_the_same_slot_makes_a_second_track() {
        let frames: Vec<_> = (0..10)
            .map(|t| frame(t, vec![shot(if t < 5 { 1 } else { 2 }, 0.5)]))
            .collect();
        assert_eq!(spans(&build(&frames)), [(0, 4, 5), (5, 9, 5)]);
    }

    #[test]
    fn a_missing_frame_closes_the_track() {
        let frames: Vec<_> = (0..10)
            .map(|t| frame(t, if t == 5 { vec![] } else { vec![shot(1, 1.)] }))
            .collect();
        assert_eq!(spans(&build(&frames)), [(0, 4, 5), (6, 9, 4)]);
    }

    #[test]
    fn a_team_kind_or_crit_change_splits_the_track() {
        let frames: Vec<_> = (0..9)
            .map(|t| {
                let mut s = shot(1, 1.);
                if t >= 3 {
                    s.team = Some(Team::Blue);
                }
                if t >= 5 {
                    s.critical = true;
                }
                if t >= 7 {
                    s.kind = ProjectileKind::Sticky;
                }
                frame(t, vec![s])
            })
            .collect();
        let t = build(&frames);
        assert_eq!(spans(&t), [(0, 2, 3), (3, 4, 2), (5, 6, 2), (7, 8, 2)]);
        let looks: Vec<(ProjectileKind, Team, bool)> = t
            .projectiles
            .iter()
            .map(|p| (p.kind, p.team, p.critical))
            .collect();
        assert_eq!(
            looks,
            [
                (ProjectileKind::Rocket, Team::Red, false),
                (ProjectileKind::Rocket, Team::Blue, false),
                (ProjectileKind::Rocket, Team::Blue, true),
                (ProjectileKind::Sticky, Team::Blue, true),
            ]
        );
    }

    #[test]
    fn rows_without_a_usable_origin_or_team_make_no_sample() {
        let frames: Vec<_> = (0..6)
            .map(|t| {
                let mut late = shot(1, 1.);
                if t < 3 {
                    late.origin = None;
                }
                let mut zero = shot(2, 1.);
                zero.origin = Some([0., 0., 0.]);
                let mut nan = shot(3, 1.);
                nan.origin = Some([f32::NAN, 1., 1.]);
                let mut far = shot(4, 70_000.);
                far.origin = Some([70_000., 1., 1.]);
                let mut teamless = shot(5, 1.);
                teamless.team = None;
                frame(t, vec![late, zero, nan, far, teamless])
            })
            .collect();
        let t = build(&frames);
        assert_eq!(spans(&t), [(3, 5, 3)]);
        assert_eq!(sample_of(&shot(9, 0.)), Some((Team::Red, [0., 50., 100.])));
    }

    #[test]
    fn tracks_are_sorted_by_first_tick() {
        let frames: Vec<_> = (0..10)
            .map(|t| {
                let mut shots = vec![shot(2, 1.)];
                if (5..8).contains(&t) {
                    shots.insert(0, shot(1, 2.));
                }
                frame(t, shots)
            })
            .collect();
        assert_eq!(spans(&build(&frames)), [(0, 9, 10), (5, 7, 3)]);
    }

    #[test]
    fn the_projectile_cap_refuses_the_whole_frame() {
        let caps = Caps {
            projectile_samples: 5,
            ..Caps::default()
        };
        let mut builder = TrackBuilder::with_caps(caps);
        let steps: Vec<Step> = (10..13)
            .map(|t| builder.push(&frame(t, vec![shot(1, 1.), shot(2, 2.)]), SPT))
            .collect();
        assert_eq!(steps, [Step::Continue, Step::Continue, Step::Done]);
        let t = builder.finish(SPT, 0, false).expect("tracks");
        assert_eq!(t.last_tick(), 11);
        assert_eq!(t.cut(), Some(TrackCut::SampleCap { at_tick: 11 }));
        assert_eq!((t.projectile_sample_count(), t.sample_count()), (4, 2));
    }

    #[test]
    fn a_restart_drops_the_opening_projectiles() {
        let mut frames = vec![frame(90_000, vec![shot(7, 1.)])];
        frames.extend((0..3).map(|t| frame(t, vec![shot(8, 1.)])));
        let t = build(&frames);
        assert_eq!(spans(&t), [(0, 2, 3)]);
    }

    #[test]
    fn signon_and_non_advancing_frames_add_no_samples() {
        let mut frames = vec![frame(SENTINEL_TICK, vec![shot(1, 1.)])];
        frames.extend([0, 1, 1, 2].map(|t| frame(t, vec![shot(1, 1.)])));
        assert_eq!(spans(&build(&frames)), [(0, 2, 3)]);
    }

    #[test]
    fn projectiles_without_players_give_no_tracks() {
        let frames: Vec<_> = (0..5)
            .map(|t| {
                let mut f = frame(t, vec![shot(1, 1.)]);
                f.rows.clear();
                f
            })
            .collect();
        assert!(build_with(Caps::default(), &frames).is_none());
    }

    #[test]
    fn heap_bytes_counts_the_projectile_samples() {
        let with: Vec<_> = (0..10).map(|t| frame(t, vec![shot(1, 1.)])).collect();
        let without: Vec<_> = (0..10).map(|t| frame(t, vec![])).collect();
        let extra = build(&with).heap_bytes() - build(&without).heap_bytes();
        assert!((10 * 16..10 * 16 + 1024).contains(&extra), "{extra}");
    }
}
