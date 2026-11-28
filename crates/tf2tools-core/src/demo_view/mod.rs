//! The demo viewer's UI-free pieces: the map mesh drawn behind a demo, the free-fly and
//! spectator cameras, the demo header, the players at one tick, their tracks over the whole
//! demo and the playback clock.

mod camera;
mod chase;
mod clock;
mod free_fly;
mod header;
mod mesh;
mod playback;
mod ray_grid;
mod snapshot;
mod studio;
#[cfg(test)]
pub(crate) mod test_support;
mod tracks;

pub use camera::{CameraMode, CameraView, SpectatorCamera, TargetLife};
pub use clock::PlaybackClock;
pub use free_fly::{FlyInput, FreeFly};
pub use header::{DemoHeader, read_demo_header};
pub use mesh::{
    MAX_MAP_BYTES, MAX_MAP_TRIANGLES, MapMesh, load_map_file, load_map_file_capped, load_map_mesh,
};
pub use playback::{PlaybackEvent, read_demo_playback, read_demo_tracks};
pub use snapshot::{DemoSnapshot, MAX_SNAPSHOT_BYTES, SnapshotPlayer, read_demo_snapshot};
pub use tracks::{
    DemoKill, DemoTestKill, DemoTestParty, DemoTestShot, DemoTracks, EyePose, FollowPose,
    KillParty, MAX_DEMO_BYTES, MAX_KILLS, MAX_PROJECTILE_SAMPLES, MAX_TRACK_SAMPLES, PlayerId,
    ProjectileKind, ProjectileView, TrackCut, TrackedPlayer,
};
