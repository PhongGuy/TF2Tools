//! The spectator camera: free-fly, first person and chase over a demo's players, as TF2 cycles them.

use super::chase::{ChaseDistance, chase_origin, wanted_distance};
use super::free_fly::{FlyInput, FreeFly};
use super::mesh::MapMesh;
use super::tracks::{DemoTracks, FollowPose, PlayerId};

/// What the camera is doing: flying free or following its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CameraMode {
    /// Flying free under the viewer's control.
    FreeFly,
    /// In the target's eye.
    FirstPerson,
    /// Behind the target's eye, kept out of walls.
    Chase,
}

/// Whether a followed player is alive at the drawn tick or held on their death spot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TargetLife {
    /// Alive at the drawn tick.
    Alive,
    /// Dead, held on the death spot.
    Dead,
}

/// The pose to draw one frame from, the player box to leave out and the followed target.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct CameraView {
    origin: [f32; 3],
    yaw: f32,
    pitch: f32,
    hidden: Option<PlayerId>,
    target: Option<(PlayerId, TargetLife)>,
}

impl CameraView {
    /// The camera position in map units.
    pub fn origin(&self) -> [f32; 3] {
        self.origin
    }

    /// The yaw in degrees.
    pub fn yaw(&self) -> f32 {
        self.yaw
    }

    /// The pitch in degrees, positive looking down.
    pub fn pitch(&self) -> f32 {
        self.pitch
    }

    /// The player whose box is not drawn, because the camera sits in their eye.
    pub fn hidden(&self) -> Option<PlayerId> {
        self.hidden
    }

    /// The followed player and whether they are alive, in a follow mode.
    pub fn target(&self) -> Option<(PlayerId, TargetLife)> {
        self.target
    }
}

/// The viewer's camera: owns the free-fly, the follow mode, the target and the chase distance.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct SpectatorCamera {
    mode: CameraMode,
    target: Option<PlayerId>,
    fly: FreeFly,
    chase: ChaseDistance,
    snap: bool,
    last: ([f32; 3], f32, f32),
}

fn is_alive(tracks: &DemoTracks, id: PlayerId, tick: f64) -> bool {
    matches!(tracks.eye_at(id, tick), FollowPose::Alive(_))
}

impl SpectatorCamera {
    /// A camera in free-fly with no target, starting from `fly`'s pose.
    pub fn new(fly: FreeFly) -> Self {
        let last = (fly.origin(), fly.yaw(), fly.pitch());
        Self {
            mode: CameraMode::FreeFly,
            target: None,
            fly,
            chase: ChaseDistance::new(),
            snap: false,
            last,
        }
    }

    /// The current mode.
    pub fn mode(&self) -> CameraMode {
        self.mode
    }

    /// The followed player, kept while flying free so a later cycle can return to them.
    pub fn target(&self) -> Option<PlayerId> {
        self.target
    }

    /// The free-fly this camera steers while flying free; its pose is stale in a follow mode.
    pub fn fly(&self) -> &FreeFly {
        &self.fly
    }

    // Every mode or target change snaps the next chase distance instead of growing into it.
    fn set(&mut self, mode: CameraMode, target: Option<PlayerId>) {
        if (mode, target) != (self.mode, self.target) {
            self.snap = true;
        }
        self.mode = mode;
        self.target = target;
    }

    // Free-fly restarts at rest from the last drawn pose, so leaving a follow mode does not jump.
    fn drop_to_fly(&mut self) {
        let (origin, yaw, pitch) = self.last;
        self.fly = FreeFly::new(origin, yaw, pitch);
        self.set(CameraMode::FreeFly, self.target);
    }

    fn alive(tracks: &DemoTracks, tick: f64) -> impl DoubleEndedIterator<Item = PlayerId> + '_ {
        tracks
            .players()
            .iter()
            .map(|p| p.id())
            .filter(move |&id| is_alive(tracks, id, tick))
    }

    /// Space: first person to chase to free-fly to first person, skipping follow with nobody alive.
    pub fn cycle(&mut self, tracks: &DemoTracks, tick: f64) {
        match self.mode {
            CameraMode::FirstPerson => self.set(CameraMode::Chase, self.target),
            CameraMode::Chase => self.drop_to_fly(),
            CameraMode::FreeFly => {
                let pick = self
                    .target
                    .filter(|&id| is_alive(tracks, id, tick))
                    .or_else(|| Self::alive(tracks, tick).next());
                if let Some(id) = pick {
                    self.set(CameraMode::FirstPerson, Some(id));
                }
            }
        }
    }

    /// The next alive player after the target in entity order, wrapping; enters first person.
    pub fn next(&mut self, tracks: &DemoTracks, tick: f64) {
        let after = self
            .target
            .and_then(|t| Self::alive(tracks, tick).find(|&id| id > t));
        if let Some(id) = after.or_else(|| Self::alive(tracks, tick).next()) {
            self.follow(id);
        }
    }

    /// The previous alive player before the target in entity order, wrapping; enters first person.
    pub fn prev(&mut self, tracks: &DemoTracks, tick: f64) {
        let before = self
            .target
            .and_then(|t| Self::alive(tracks, tick).rev().find(|&id| id < t));
        if let Some(id) = before.or_else(|| Self::alive(tracks, tick).next_back()) {
            self.follow(id);
        }
    }

    /// Follows `id`, entering first person from free-fly.
    pub fn follow(&mut self, id: PlayerId) {
        let mode = match self.mode {
            CameraMode::FreeFly => CameraMode::FirstPerson,
            CameraMode::FirstPerson | CameraMode::Chase => self.mode,
        };
        self.set(mode, Some(id));
    }

    /// Turns the free-fly view; follow modes ignore it.
    pub fn look(&mut self, d_yaw_deg: f32, d_pitch_deg: f32) {
        if self.mode == CameraMode::FreeFly {
            self.fly.look(d_yaw_deg, d_pitch_deg);
        }
    }

    /// Makes the next chase draw jump to its distance, as after a seek.
    pub fn snap(&mut self) {
        self.snap = true;
    }

    /// Advances the active camera by `dt_seconds` and returns the pose to draw at `tick`.
    pub fn view(
        &mut self,
        tracks: Option<&DemoTracks>,
        mesh: Option<&MapMesh>,
        tick: f64,
        dt_seconds: f32,
        input: FlyInput,
    ) -> CameraView {
        let followed = match (self.mode, self.target, tracks) {
            (CameraMode::FreeFly, _, _) | (_, None, _) | (_, _, None) => None,
            (CameraMode::FirstPerson | CameraMode::Chase, Some(id), Some(t)) => {
                match t.eye_at(id, tick) {
                    FollowPose::Alive(pose) => Some((id, TargetLife::Alive, pose)),
                    FollowPose::Dead(pose) => Some((id, TargetLife::Dead, pose)),
                    FollowPose::Absent => None,
                }
            }
        };
        let view = match followed {
            None => {
                if self.mode != CameraMode::FreeFly {
                    self.drop_to_fly();
                }
                self.fly.advance(dt_seconds, input);
                CameraView {
                    origin: self.fly.origin(),
                    yaw: self.fly.yaw(),
                    pitch: self.fly.pitch(),
                    hidden: None,
                    target: None,
                }
            }
            Some((id, TargetLife::Alive, pose)) if self.mode == CameraMode::FirstPerson => {
                // The next death cuts to its chase in place rather than growing from a stale distance.
                self.snap = true;
                CameraView {
                    origin: pose.eye(),
                    yaw: pose.yaw(),
                    pitch: pose.pitch(),
                    hidden: Some(id),
                    target: Some((id, TargetLife::Alive)),
                }
            }
            Some((id, life, pose)) => {
                let (eye, yaw, pitch) = (pose.eye(), pose.yaw(), pose.pitch());
                let wanted = wanted_distance(mesh, eye, yaw, pitch);
                if self.snap {
                    self.chase.snap(wanted);
                    self.snap = false;
                } else {
                    self.chase.advance(dt_seconds, wanted);
                }
                CameraView {
                    origin: chase_origin(eye, yaw, pitch, self.chase.current()),
                    yaw,
                    pitch,
                    hidden: None,
                    target: Some((id, life)),
                }
            }
        };
        self.last = (view.origin, view.yaw, view.pitch);
        view
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo_view::test_support::{row, tracks};

    // Players 1, 2 and 3 at x = 100, 200, 300 looking along +x; player 2 is gone for ticks
    // 40 to 89 (39 to 90 is 51 ticks, 0.77 s: a death).
    fn three() -> DemoTracks {
        let frames = (0..=100u32)
            .map(|t| {
                let mut rows = vec![row(1, [100., 0., 64.], 0.), row(3, [300., 0., 64.], 0.)];
                if !(40..90).contains(&t) {
                    rows.push(row(2, [200., 0., 64.], 0.));
                }
                (t, rows)
            })
            .collect();
        tracks(frames)
    }

    fn camera() -> SpectatorCamera {
        SpectatorCamera::new(FreeFly::new([0., 0., 0.], 0., 0.))
    }

    fn target(c: &SpectatorCamera) -> Option<u32> {
        c.target().map(PlayerId::entity)
    }

    fn close(a: [f32; 3], b: [f32; 3]) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-3)
    }

    fn draw(c: &mut SpectatorCamera, t: &DemoTracks, tick: f64) -> CameraView {
        c.view(Some(t), None, tick, 0.016, FlyInput::default())
    }

    #[test]
    fn space_cycles_first_person_chase_free_fly() {
        let t = three();
        let mut c = camera();
        assert_eq!(c.mode(), CameraMode::FreeFly);
        c.cycle(&t, 10.);
        assert_eq!((c.mode(), target(&c)), (CameraMode::FirstPerson, Some(1)));
        c.cycle(&t, 10.);
        assert_eq!(c.mode(), CameraMode::Chase);
        c.cycle(&t, 10.);
        assert_eq!(c.mode(), CameraMode::FreeFly);
    }

    #[test]
    fn next_and_prev_skip_the_dead_and_wrap() {
        let t = three();
        let mut c = camera();
        c.next(&t, 50.);
        assert_eq!((c.mode(), target(&c)), (CameraMode::FirstPerson, Some(1)));
        c.next(&t, 50.);
        assert_eq!(target(&c), Some(3));
        c.next(&t, 50.);
        assert_eq!(target(&c), Some(1));
        c.prev(&t, 50.);
        assert_eq!(target(&c), Some(3));
        let mut c = camera();
        c.prev(&t, 50.);
        assert_eq!((c.mode(), target(&c)), (CameraMode::FirstPerson, Some(3)));
    }

    #[test]
    fn with_nobody_alive_the_camera_stays_free() {
        let t = tracks(
            (0..=10u32)
                .map(|k| (k, vec![row(1, [100., 0., 64.], 0.)]))
                .collect(),
        );
        let mut c = camera();
        c.cycle(&t, 50.);
        c.next(&t, 50.);
        c.prev(&t, 50.);
        assert_eq!((c.mode(), c.target()), (CameraMode::FreeFly, None));
    }

    #[test]
    fn first_person_sits_at_the_eye_and_hides_the_target() {
        let t = three();
        let mut c = camera();
        c.next(&t, 10.);
        let v = draw(&mut c, &t, 10.);
        assert!(close(v.origin(), [100., 0., 132.]), "{:?}", v.origin());
        let one = t.players()[0].id();
        assert_eq!(v.hidden(), Some(one));
        assert_eq!(v.target(), Some((one, TargetLife::Alive)));
    }

    #[test]
    fn chase_sits_behind_and_hides_nothing() {
        let t = three();
        let mut c = camera();
        c.next(&t, 10.);
        c.cycle(&t, 10.);
        let v = draw(&mut c, &t, 10.);
        assert!(close(v.origin(), [4., 0., 132.]), "{:?}", v.origin());
        assert_eq!(v.hidden(), None);
    }

    #[test]
    fn a_dead_target_is_chased_on_its_death_spot_then_followed_again() {
        let t = three();
        let two = t.players()[1].id();
        let mut c = camera();
        c.follow(two);
        assert_eq!(c.mode(), CameraMode::FirstPerson);
        let v = draw(&mut c, &t, 60.);
        assert_eq!(v.target(), Some((two, TargetLife::Dead)));
        assert_eq!(v.hidden(), None);
        assert!(close(v.origin(), [104., 0., 132.]), "{:?}", v.origin());
        let v = draw(&mut c, &t, 95.);
        assert_eq!(v.target(), Some((two, TargetLife::Alive)));
        assert!(close(v.origin(), [200., 0., 132.]), "{:?}", v.origin());
        assert_eq!(c.mode(), CameraMode::FirstPerson);
    }

    #[test]
    fn leaving_follow_seeds_free_fly_where_the_camera_was() {
        let t = three();
        let mut c = camera();
        c.next(&t, 10.);
        c.cycle(&t, 10.);
        let chased = draw(&mut c, &t, 10.);
        c.cycle(&t, 10.);
        let v = c.view(Some(&t), None, 10., 0., FlyInput::default());
        assert_eq!(c.mode(), CameraMode::FreeFly);
        assert!(close(v.origin(), chased.origin()));
        assert_eq!((v.yaw(), v.pitch()), (chased.yaw(), chased.pitch()));
    }

    #[test]
    fn an_absent_target_drops_to_free_fly_in_place() {
        let t = three();
        let mut c = camera();
        c.follow(PlayerId(99));
        let v = draw(&mut c, &t, 10.);
        assert_eq!(c.mode(), CameraMode::FreeFly);
        assert!(close(v.origin(), [0., 0., 0.]));
        let mut c = camera();
        c.next(&t, 10.);
        c.view(None, None, 10., 0.016, FlyInput::default());
        assert_eq!(c.mode(), CameraMode::FreeFly);
    }

    #[test]
    fn look_turns_only_the_free_fly() {
        let t = three();
        let mut c = camera();
        c.look(45., 0.);
        assert_eq!(draw(&mut c, &t, 10.).yaw(), 45.);
        c.next(&t, 10.);
        c.look(30., 10.);
        let v = draw(&mut c, &t, 10.);
        assert!(
            v.yaw().abs() < 1e-3 && v.pitch().abs() < 1e-3,
            "{} {}",
            v.yaw(),
            v.pitch()
        );
    }

    #[test]
    fn the_chase_pulls_in_at_a_wall_grows_by_wall_time_and_snaps_on_seek() {
        let t = three();
        let (a, b, cc, d) = (
            [50., -1000., -1000.],
            [50., 1000., -1000.],
            [50., 1000., 1000.],
            [50., -1000., 1000.],
        );
        let mesh = MapMesh::from_positions(vec![a, b, cc, a, cc, d]);
        let mut c = camera();
        c.next(&t, 10.);
        c.cycle(&t, 10.);
        let v = c.view(Some(&t), Some(&mesh), 10., 0.016, FlyInput::default());
        assert!(close(v.origin(), [56., 0., 132.]), "{:?}", v.origin());
        let v = c.view(Some(&t), None, 10., 0.5, FlyInput::default());
        assert!(close(v.origin(), [32., 0., 132.]), "{:?}", v.origin());
        c.snap();
        let v = c.view(Some(&t), None, 10., 0., FlyInput::default());
        assert!(close(v.origin(), [4., 0., 132.]), "{:?}", v.origin());
    }

    #[test]
    fn every_death_seen_from_first_person_snaps_the_chase() {
        let frames = (0..=130u32)
            .map(|k| {
                let up = k < 10 || (60..70).contains(&k);
                (
                    k,
                    if up {
                        vec![row(1, [100., 0., 64.], 0.)]
                    } else {
                        vec![]
                    },
                )
            })
            .collect();
        let t = tracks(frames);
        let (a, b, cc, d) = (
            [50., -1000., -1000.],
            [50., 1000., -1000.],
            [50., 1000., 1000.],
            [50., -1000., 1000.],
        );
        let mesh = MapMesh::from_positions(vec![a, b, cc, a, cc, d]);
        let mut c = camera();
        c.next(&t, 5.);
        let v = c.view(Some(&t), Some(&mesh), 30., 0.016, FlyInput::default());
        assert!(close(v.origin(), [56., 0., 132.]), "{:?}", v.origin());
        c.view(Some(&t), None, 65., 0.016, FlyInput::default());
        let v = c.view(Some(&t), None, 100., 0.016, FlyInput::default());
        assert!(close(v.origin(), [4., 0., 132.]), "{:?}", v.origin());
    }
}
