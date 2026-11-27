//! The chase camera: back and above a followed eye along one ray, kept out of walls.

use super::mesh::MapMesh;

// TF2's own third-person distance, plus a lift so the target sits in the lower middle.
pub(crate) const CHASE_BACK: f32 = 150.0;
pub(crate) const CHASE_UP: f32 = 24.0;
pub(crate) const WALL_OFFSET: f32 = 6.0;
pub(crate) const GROW_SPEED: f32 = 76.0;

/// The chase camera's distance along its arm: pulled in at once, grown back at GROW_SPEED.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ChaseDistance {
    current: f32,
}

impl ChaseDistance {
    pub(crate) fn new() -> Self {
        Self {
            current: CHASE_BACK.hypot(CHASE_UP),
        }
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

// The arm from the eye to the full chase: back along the view, then lifted in world z.
fn offset(yaw: f32, pitch: f32) -> [f32; 3] {
    let f = forward(yaw, pitch);
    [
        -CHASE_BACK * f[0],
        -CHASE_BACK * f[1],
        -CHASE_BACK * f[2] + CHASE_UP,
    ]
}

/// The full chase distance for a look: the length of the back-and-up arm.
pub(crate) fn chase_length(yaw: f32, pitch: f32) -> f32 {
    offset(yaw, pitch).iter().map(|v| v * v).sum::<f32>().sqrt()
}

// The arm's unit direction, as the ray cast needs; the length never drops below about 126.
fn direction(yaw: f32, pitch: f32) -> [f32; 3] {
    let length = chase_length(yaw, pitch);
    offset(yaw, pitch).map(|v| v / length)
}

/// chase_length, or the first wall or ceiling on the arm minus WALL_OFFSET, never below 0.
pub(crate) fn wanted_distance(mesh: Option<&MapMesh>, eye: [f32; 3], yaw: f32, pitch: f32) -> f32 {
    let length = chase_length(yaw, pitch);
    let Some(mesh) = mesh else { return length };
    match mesh.ray_hit(eye, direction(yaw, pitch), length + WALL_OFFSET) {
        Some(hit) => (hit - WALL_OFFSET).clamp(0., length),
        None => length,
    }
}

/// The point `distance` from `eye` along the back-and-up arm.
pub(crate) fn chase_origin(eye: [f32; 3], yaw: f32, pitch: f32, distance: f32) -> [f32; 3] {
    let dir = direction(yaw, pitch);
    [0, 1, 2].map(|i| eye[i] + dir[i] * distance)
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

    // A ceiling across z = `z`, wide enough for any test ray.
    fn ceiling(z: f32) -> MapMesh {
        let (a, b, c, d) = (
            [-1000., -1000., z],
            [1000., -1000., z],
            [1000., 1000., z],
            [-1000., 1000., z],
        );
        MapMesh::from_positions(vec![a, b, c, a, c, d])
    }

    #[test]
    fn the_chase_sits_back_and_above_the_view() {
        let eye = [0., 0., 100.];
        assert!(close(
            chase_origin(eye, 0., 0., chase_length(0., 0.)),
            [-150., 0., 124.]
        ));
        assert!(close(
            chase_origin(eye, 90., 0., chase_length(90., 0.)),
            [0., -150., 124.]
        ));
        assert!(close(
            chase_origin(eye, 0., 90., chase_length(0., 90.)),
            [0., 0., 274.]
        ));
        // A shorter distance stays on the same line.
        assert!(close(
            chase_origin(eye, 0., 0., chase_length(0., 0.) / 2.),
            [-75., 0., 112.]
        ));
    }

    #[test]
    fn the_full_length_follows_the_pitch() {
        assert!((chase_length(0., 0.) - CHASE_BACK.hypot(CHASE_UP)).abs() < 1e-3);
        // Looking 70 up swings the back arm under the eye, against the lift.
        assert!((chase_length(0., -70.) - 127.71).abs() < 0.01);
        assert!((chase_length(0., 90.) - 174.).abs() < 1e-3);
    }

    #[test]
    fn a_wall_or_ceiling_shortens_the_wanted_distance() {
        let l = chase_length(0., 0.);
        assert_eq!(wanted_distance(None, [0., 0., 0.], 0., 0.), l);
        // The back-and-up ray reaches x -50 after 50 * l / 150 of its length.
        let w = wanted_distance(Some(&wall(-50.)), [0., 0., 0.], 0., 0.);
        assert!(
            (w - (50. * l / CHASE_BACK - WALL_OFFSET)).abs() < 1e-3,
            "{w}"
        );
        assert_eq!(
            wanted_distance(Some(&wall(-50.)), [0., 0., 0.], 180., 0.),
            l
        );
        assert_eq!(wanted_distance(Some(&wall(-3.)), [0., 0., 0.], 0., 0.), 0.);
        // A level look under a ceiling 10 up is stopped by the lift alone.
        let c = wanted_distance(Some(&ceiling(10.)), [0., 0., 0.], 0., 0.);
        assert!((c - (10. * l / CHASE_UP - WALL_OFFSET)).abs() < 1e-3, "{c}");
    }

    #[test]
    fn the_distance_pulls_in_at_once_and_grows_back_by_time() {
        let l = chase_length(0., 0.);
        let mut d = ChaseDistance::new();
        assert_eq!(d.current(), l);
        d.advance(0.016, 44.);
        assert_eq!(d.current(), 44.);
        d.advance(0.5, l);
        assert!((d.current() - 82.).abs() < 1e-3);
        d.advance(0.25, l);
        assert!((d.current() - 101.).abs() < 1e-3);
        d.advance(1., l);
        assert_eq!(d.current(), l);
    }

    #[test]
    fn a_snap_jumps_and_a_bad_frame_does_not_move() {
        let l = chase_length(0., 0.);
        let mut d = ChaseDistance::new();
        d.snap(30.);
        assert_eq!(d.current(), 30.);
        d.advance(-1., l);
        d.advance(f32::NAN, l);
        assert_eq!(d.current(), 30.);
    }
}
