//! The chase camera's distance behind a followed eye, kept out of walls.

use super::mesh::MapMesh;

pub(crate) const CHASE_MAX: f32 = 96.0;
pub(crate) const WALL_OFFSET: f32 = 6.0;
pub(crate) const GROW_SPEED: f32 = 48.0;

/// The chase camera's distance behind the eye: pulled in at once, grown back at GROW_SPEED.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ChaseDistance {
    current: f32,
}

impl ChaseDistance {
    pub(crate) fn new() -> Self {
        Self { current: CHASE_MAX }
    }

    pub(crate) fn current(&self) -> f32 {
        self.current
    }

    // Closer than wanted is wall-free growth over time; farther is a wall, so jump in.
    pub(crate) fn advance(&mut self, dt_seconds: f32, wanted: f32) {
        // The growth is capped at `wanted`, so a long frame needs no cap of its own.
        let dt = if dt_seconds.is_nan() {
            0.
        } else {
            dt_seconds.max(0.)
        };
        self.current = if self.current > wanted {
            wanted
        } else {
            (self.current + GROW_SPEED * dt).min(wanted)
        };
    }

    pub(crate) fn snap(&mut self, wanted: f32) {
        self.current = wanted;
    }
}

// The unit vector the view looks along, as free-fly computes it.
fn forward(yaw: f32, pitch: f32) -> [f32; 3] {
    let (sy, cy) = yaw.to_radians().sin_cos();
    let (sp, cp) = pitch.to_radians().sin_cos();
    [cp * cy, cp * sy, -sp]
}

/// CHASE_MAX, or the wall behind the view minus WALL_OFFSET, never below 0; CHASE_MAX with no mesh.
pub(crate) fn wanted_distance(mesh: Option<&MapMesh>, eye: [f32; 3], yaw: f32, pitch: f32) -> f32 {
    let Some(mesh) = mesh else { return CHASE_MAX };
    let back = forward(yaw, pitch).map(|v| -v);
    match mesh.ray_hit(eye, back, CHASE_MAX + WALL_OFFSET) {
        Some(hit) => (hit - WALL_OFFSET).clamp(0., CHASE_MAX),
        None => CHASE_MAX,
    }
}

/// The point `distance` behind `eye` against the view (forward is [cp*cy, cp*sy, -sp]).
pub(crate) fn chase_origin(eye: [f32; 3], yaw: f32, pitch: f32, distance: f32) -> [f32; 3] {
    let f = forward(yaw, pitch);
    [0, 1, 2].map(|i| eye[i] - f[i] * distance)
}

#[cfg(test)]
mod tests {
    use super::*;

    // A wall across x = `x`, wide enough for any test ray.
    fn wall(x: f32) -> MapMesh {
        let (a, b, c, d) = (
            [x, -1000., -1000.],
            [x, 1000., -1000.],
            [x, 1000., 1000.],
            [x, -1000., 1000.],
        );
        MapMesh::from_positions(vec![a, b, c, a, c, d])
    }

    fn close(a: [f32; 3], b: [f32; 3]) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-3)
    }

    #[test]
    fn the_chase_sits_behind_the_view() {
        assert!(close(
            chase_origin([0., 0., 100.], 0., 0., 96.),
            [-96., 0., 100.]
        ));
        assert!(close(
            chase_origin([0., 0., 100.], 90., 0., 96.),
            [0., -96., 100.]
        ));
        assert!(close(
            chase_origin([0., 0., 100.], 0., 90., 96.),
            [0., 0., 196.]
        ));
    }

    #[test]
    fn a_wall_behind_shortens_the_wanted_distance() {
        assert_eq!(wanted_distance(None, [0., 0., 0.], 0., 0.), CHASE_MAX);
        let mesh = wall(-50.);
        assert!((wanted_distance(Some(&mesh), [0., 0., 0.], 0., 0.) - 44.).abs() < 1e-3);
        assert_eq!(
            wanted_distance(Some(&mesh), [0., 0., 0.], 180., 0.),
            CHASE_MAX
        );
        assert_eq!(wanted_distance(Some(&wall(-3.)), [0., 0., 0.], 0., 0.), 0.);
    }

    #[test]
    fn the_distance_pulls_in_at_once_and_grows_back_by_time() {
        let mut d = ChaseDistance::new();
        assert_eq!(d.current(), CHASE_MAX);
        d.advance(0.016, 44.);
        assert_eq!(d.current(), 44.);
        d.advance(0.5, CHASE_MAX);
        assert!((d.current() - 68.).abs() < 1e-3);
        d.advance(0.25, CHASE_MAX);
        assert!((d.current() - 80.).abs() < 1e-3);
        d.advance(0.25, CHASE_MAX);
        d.advance(0.25, CHASE_MAX);
        assert_eq!(d.current(), CHASE_MAX);
    }

    #[test]
    fn a_snap_jumps_and_a_bad_frame_does_not_move() {
        let mut d = ChaseDistance::new();
        d.snap(30.);
        assert_eq!(d.current(), 30.);
        d.advance(-1., CHASE_MAX);
        d.advance(f32::NAN, CHASE_MAX);
        assert_eq!(d.current(), 30.);
    }
}
