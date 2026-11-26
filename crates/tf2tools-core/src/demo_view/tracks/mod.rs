//! Every player's positions over a whole demo, cut into runs the viewer may blend inside.

use crate::hud_maker::Team;
use crate::weapons::TfClass;

mod build;

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

/// Every RED and BLU player's positions over the read part of a demo.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct DemoTracks {
    pub(crate) players: Vec<PlayerTrack>,
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
        players + runs + samples + self.round_starts.capacity() * size_of::<u32>()
    }

    /// How many samples repeat the previous one of their run in feet, yaw and pitch.
    pub fn repeated_sample_count(&self) -> usize {
        self.runs()
            .flat_map(|r| r.samples.windows(2))
            .filter(|w| (w[0].feet, w[0].yaw, w[0].pitch) == (w[1].feet, w[1].yaw, w[1].pitch))
            .count()
    }

    fn runs(&self) -> impl Iterator<Item = &TrackRun> {
        self.players.iter().flat_map(|p| &p.runs)
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
    use super::*;

    #[test]
    fn the_server_interval_wins_then_the_header_then_66_tick() {
        assert_eq!(seconds_per_tick(0.015, 0., 0), 0.015);
        assert_eq!(seconds_per_tick(0., 60., 3960), 60. / 3960.);
        assert_eq!(seconds_per_tick(f32::NAN, 0., 0), 1. / 66.);
        assert_eq!(seconds_per_tick(5., 1., 0), 1. / 66.);
        assert_eq!(seconds_per_tick(0., 1e9, 10), 1. / 66.);
    }
}
