//! One still frame of a demo: where each RED and BLU player stands and looks at one tick.

use crate::hud_maker::Team;
use crate::weapons::TfClass;

/// The most of a demo read when looking for players; the early-match tick sits well inside it.
pub const MAX_SNAPSHOT_BYTES: u64 = 64 * 1024 * 1024;

/// The players of a demo at one tick.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct DemoSnapshot {
    tick: u32,
    players: Vec<SnapshotPlayer>,
}

impl DemoSnapshot {
    /// A snapshot of `players` at `tick`, kept in the given order.
    pub fn new(tick: u32, players: Vec<SnapshotPlayer>) -> Self {
        Self { tick, players }
    }

    /// The demo tick the players were read at.
    pub fn tick(&self) -> u32 {
        self.tick
    }

    /// The players, in the order the snapshot was built with.
    pub fn players(&self) -> &[SnapshotPlayer] {
        &self.players
    }
}

/// One player as the viewer draws it.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct SnapshotPlayer {
    team: Team,
    class: TfClass,
    feet: [f32; 3],
    yaw: f32,
    pitch: f32,
    eye_height: f32,
    ducked: bool,
}

impl SnapshotPlayer {
    /// A player on `team` playing `class`, standing at `feet` and looking along `yaw` and `pitch`.
    pub fn new(
        team: Team,
        class: TfClass,
        feet: [f32; 3],
        yaw: f32,
        pitch: f32,
        eye_height: f32,
        ducked: bool,
    ) -> Self {
        Self {
            team,
            class,
            feet,
            yaw,
            pitch,
            eye_height,
            ducked,
        }
    }

    /// The player's side.
    pub fn team(&self) -> Team {
        self.team
    }

    /// The player's class.
    pub fn class(&self) -> TfClass {
        self.class
    }

    /// The player's origin in world units: the bottom of the hull.
    pub fn feet(&self) -> [f32; 3] {
        self.feet
    }

    /// The view yaw in degrees: 0 looks along +X and 90 along +Y.
    pub fn yaw(&self) -> f32 {
        self.yaw
    }

    /// The view pitch in degrees; positive looks down.
    pub fn pitch(&self) -> f32 {
        self.pitch
    }

    /// The eye's height above the feet in world units.
    pub fn eye_height(&self) -> f32 {
        self.eye_height
    }

    /// Whether the player is crouched.
    pub fn ducked(&self) -> bool {
        self.ducked
    }
}

/// The eye height above the feet: the class's standing view offset, or 45 for any ducked class.
#[cfg_attr(not(test), expect(dead_code))]
pub(crate) fn class_eye_height(class: TfClass, ducked: bool) -> f32 {
    if ducked {
        return 45.;
    }
    match class {
        TfClass::Scout => 65.,
        TfClass::Soldier | TfClass::Pyro | TfClass::Demoman | TfClass::Engineer => 68.,
        TfClass::Sniper | TfClass::Medic | TfClass::Heavy | TfClass::Spy => 75.,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_class_stands_at_its_eye_height_and_ducks_to_45() {
        let want = [
            (TfClass::Scout, 65.),
            (TfClass::Soldier, 68.),
            (TfClass::Pyro, 68.),
            (TfClass::Demoman, 68.),
            (TfClass::Engineer, 68.),
            (TfClass::Sniper, 75.),
            (TfClass::Medic, 75.),
            (TfClass::Heavy, 75.),
            (TfClass::Spy, 75.),
        ];
        for (class, height) in want {
            assert_eq!(class_eye_height(class, false), height, "{class:?}");
            assert_eq!(class_eye_height(class, true), 45., "{class:?} ducked");
        }
    }
}
