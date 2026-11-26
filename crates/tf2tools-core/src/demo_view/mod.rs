//! The demo viewer's UI-free pieces: the map mesh drawn behind a demo, the free-fly camera,
//! the demo header, the players at one tick, their tracks over the whole demo and the
//! playback clock.

mod clock;
mod free_fly;
mod header;
mod mesh;
mod playback;
mod ray_grid;
mod snapshot;
#[cfg(test)]
pub(crate) mod test_support;
mod tracks;

pub use clock::PlaybackClock;
pub use free_fly::{FlyInput, FreeFly};
pub use header::{DemoHeader, read_demo_header};
pub use mesh::{
    MAX_MAP_BYTES, MAX_MAP_TRIANGLES, MapMesh, load_map_file, load_map_file_capped, load_map_mesh,
};
pub use playback::{PlaybackEvent, read_demo_playback, read_demo_tracks};
pub use snapshot::{DemoSnapshot, MAX_SNAPSHOT_BYTES, SnapshotPlayer, read_demo_snapshot};
pub use tracks::{
    DemoTracks, EyePose, FollowPose, MAX_DEMO_BYTES, MAX_TRACK_SAMPLES, PlayerId, TrackCut,
    TrackedPlayer,
};
