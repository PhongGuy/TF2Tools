//! Synthetic tracks for the camera tests: hand-made rows through the real track builder.

use std::collections::BTreeMap;

use super::snapshot::{TickFrame, TickRow};
use super::tracks::{DemoTracks, MAX_TRACK_SAMPLES, TrackBuilder};
use crate::hud_maker::Team;
use crate::weapons::TfClass;

/// 66 tick.
pub(crate) const SPT: f32 = 1.0 / 66.0;

/// A drawable BLU soldier standing at `feet`, looking along `yaw`, level, eye 68 up.
pub(crate) fn row(entity: u32, feet: [f32; 3], yaw: f32) -> TickRow {
    TickRow {
        entity,
        team: Some(Team::Blue),
        class: Some(TfClass::Soldier),
        feet,
        yaw,
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

/// Tracks built from `(tick, rows)` frames at 66 tick, with `names` by entity.
pub(crate) fn tracks_named(
    frames: Vec<(u32, Vec<TickRow>)>,
    names: BTreeMap<u32, String>,
) -> DemoTracks {
    let mut builder = TrackBuilder::new(MAX_TRACK_SAMPLES);
    for (tick, rows) in frames {
        builder.push(
            &TickFrame {
                tick,
                round_state: 4,
                rows,
            },
            SPT,
        );
    }
    builder.set_names(names);
    builder.finish(SPT, 0, false).expect("samples")
}

/// Tracks built from `(tick, rows)` frames at 66 tick, with no names.
pub(crate) fn tracks(frames: Vec<(u32, Vec<TickRow>)>) -> DemoTracks {
    tracks_named(frames, BTreeMap::new())
}
