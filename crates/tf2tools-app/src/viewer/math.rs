//! Camera math for the viewer in Source's frame (Z up, yaw counter-clockwise from +X, positive
//! pitch looks down), player box geometry, and the row unpadding the GPU readback needs.

use std::sync::Arc;

use tf2tools_core::{DemoSnapshot, MapMesh, PlayerId, ProjectileKind, ProjectileView, Team};

use super::{PlayerBox, ProjectileShot};

/// Degrees of turn per pixel of mouse drag.
pub const LOOK_DEG_PER_PX: f32 = 0.22;
/// How far above a spawn's origin the camera starts.
pub const EYE_HEIGHT: f32 = 64.0;
/// Near clip distance in map units.
pub const NEAR: f32 = 4.0;
/// Far clip distance in map units.
pub const FAR: f32 = 32768.0;
/// Half the vertical field of view as a tangent: 90 degrees horizontal at 4:3.
pub const TAN_HALF_VFOV: f32 = 0.75;

/// A player box's side in map units.
pub const BOX_WIDTH: f32 = 48.0;
/// A standing player box's height.
pub const BOX_HEIGHT: f32 = 82.0;
/// A crouched player box's height.
pub const BOX_DUCKED_HEIGHT: f32 = 62.0;
/// RED's box colour, sRGB `#B8383B`.
pub const RED: [f32; 3] = [184.0 / 255.0, 56.0 / 255.0, 59.0 / 255.0];
/// BLU's box colour, sRGB `#5885A2`.
pub const BLU: [f32; 3] = [88.0 / 255.0, 133.0 / 255.0, 162.0 / 255.0];
/// A long shot's length along its heading.
pub const SHOT_LONG: f32 = 24.0;
/// A long shot's width across its heading.
pub const SHOT_THIN: f32 = 6.0;
/// A round shot's size on every axis.
pub const SHOT_ROUND: f32 = 8.0;
/// The fewest logical pixels a shot's narrower side covers on screen, however far it is.
pub const MIN_SHOT_PX: f32 = 6.0;
/// In first person, shots nearer the eye than this are hidden, like the followed box.
pub const PROJECTILE_HIDE_NEAR: f32 = 32.0;
/// The steepest pitch the camera starts at, as `FreeFly` allows.
const MAX_PITCH: f32 = 89.0;

/// A box corner: map-space position and sRGB colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxVertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
}

/// wgpu's row alignment for texture-to-buffer copies.
const ROW_ALIGNMENT: u32 = 256;

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// The column-major view-projection for a camera at `eye`, mapping depth `NEAR..FAR` to `0..1`.
pub fn view_proj(eye: [f32; 3], yaw_deg: f32, pitch_deg: f32, aspect: f32) -> [[f32; 4]; 4] {
    let (sy, cy) = yaw_deg.to_radians().sin_cos();
    let (sp, cp) = pitch_deg.to_radians().sin_cos();
    let forward = [cp * cy, cp * sy, -sp];
    let right = [sy, -cy, 0.0];
    let up = cross(right, forward);
    // Each clip row is a scaled view axis dotted with (p - eye); w carries the view distance d,
    // so the divide yields `FAR / (FAR - NEAR) * (d - NEAR) / d`, 0 at NEAR and 1 at FAR.
    let sx = 1.0 / (TAN_HALF_VFOV * aspect);
    let sz = 1.0 / TAN_HALF_VFOV;
    let depth = FAR / (FAR - NEAR);
    let rows = [
        (right, sx, 0.0),
        (up, sz, 0.0),
        (forward, depth, -depth * NEAR),
        (forward, 1.0, 0.0),
    ];
    let mut m = [[0.0; 4]; 4];
    for (row, (axis, scale, offset)) in rows.into_iter().enumerate() {
        for col in 0..3 {
            m[col][row] = axis[col] * scale;
        }
        m[3][row] = -dot(axis, eye) * scale + offset;
    }
    m
}

/// The `(d_yaw, d_pitch)` in degrees for a drag: right turns right, down looks down, and
/// `scale` multiplies the turn per pixel.
pub fn drag_look(dx_px: f32, dy_px: f32, scale: f32) -> (f32, f32) {
    let per_px = LOOK_DEG_PER_PX * scale;
    (-dx_px * per_px, dy_px * per_px)
}

/// Bytes in one readback row: `width * 4` rounded up to wgpu's 256-byte copy alignment.
pub fn padded_row_bytes(width: u32) -> u32 {
    (width * 4).div_ceil(ROW_ALIGNMENT) * ROW_ALIGNMENT
}

/// Tightly packed `width * height` BGRA rows from a readback padded per `padded_row_bytes`.
/// Rows missing from a short `padded` are left out.
pub fn unpad_rows(padded: &[u8], width: u32, height: u32) -> Vec<u8> {
    let stride = padded_row_bytes(width) as usize;
    let row = width as usize * 4;
    // One slice copy per row; a per-byte iterator chain cost milliseconds at 1080p.
    let mut out = Vec::with_capacity(row * height as usize);
    for chunk in padded.chunks(stride).take(height as usize) {
        out.extend_from_slice(chunk.get(..row).unwrap_or(chunk));
    }
    out
}

/// Eye, yaw and pitch: the first RED player's eye (else the first player's), pitch clamped to
/// +-89; without players, the first spawn at eye height facing its yaw, else the middle of the
/// map's bounds looking along +X (the origin for an empty mesh), with pitch 0.
pub fn start_camera(mesh: &MapMesh, snapshot: Option<&DemoSnapshot>) -> ([f32; 3], f32, f32) {
    let players = snapshot.map_or(&[][..], DemoSnapshot::players);
    let chosen = players
        .iter()
        .find(|p| p.team() == Team::Red)
        .or_else(|| players.first());
    if let Some(p) = chosen {
        let [x, y, z] = p.feet();
        let pitch = p.pitch().clamp(-MAX_PITCH, MAX_PITCH);
        return ([x, y, z + p.eye_height()], p.yaw(), pitch);
    }
    if let Some((origin, yaw)) = mesh.start() {
        return ([origin[0], origin[1], origin[2] + EYE_HEIGHT], yaw, 0.0);
    }
    if mesh.triangle_count() == 0 {
        return ([0.0; 3], 0.0, 0.0);
    }
    let (min, max) = mesh.bounds();
    ([0, 1, 2].map(|i| (min[i] + max[i]) / 2.0), 0.0, 0.0)
}

/// The snapshot's players as boxes, in its order; none for `None`.
pub fn player_boxes(snapshot: Option<&DemoSnapshot>) -> Arc<[PlayerBox]> {
    player_boxes_except(snapshot, None)
}

/// The snapshot's players as boxes, in its order, leaving out `hidden` (the followed eye).
pub fn player_boxes_except(
    snapshot: Option<&DemoSnapshot>,
    hidden: Option<PlayerId>,
) -> Arc<[PlayerBox]> {
    snapshot
        .map_or(&[][..], DemoSnapshot::players)
        .iter()
        .filter(|p| Some(p.id()) != hidden)
        .map(|p| PlayerBox {
            feet: p.feet(),
            yaw_deg: p.yaw(),
            ducked: p.ducked(),
            team: p.team(),
        })
        .collect()
}

// Corner indices into `player_box_vertices`' corners: bit 0 is front (+X), bit 1 left (+Y),
// bit 2 top. Each face lists its corners counter-clockwise seen from outside.
const FACES: [[usize; 4]; 6] = [
    [1, 3, 7, 5], // front, +X
    [0, 4, 6, 2], // back, -X
    [2, 6, 7, 3], // left, +Y
    [0, 1, 5, 4], // right, -Y
    [4, 5, 7, 6], // top
    [0, 2, 3, 1], // bottom
];

/// 12 triangles wound counter-clockwise seen from outside; the face toward the yaw is 25 % lighter.
pub fn player_box_vertices(player: &PlayerBox) -> [BoxVertex; 36] {
    let half = BOX_WIDTH / 2.0;
    let height = if player.ducked {
        BOX_DUCKED_HEIGHT
    } else {
        BOX_HEIGHT
    };
    let (sin, cos) = player.yaw_deg.to_radians().sin_cos();
    // Corners in the box's own frame (+X forward, +Y left), turned by the yaw and set on the feet.
    let corners: [[f32; 3]; 8] = std::array::from_fn(|i| {
        let x = if i & 1 == 0 { -half } else { half };
        let y = if i & 2 == 0 { -half } else { half };
        let z = if i & 4 == 0 { 0.0 } else { height };
        let [fx, fy, fz] = player.feet;
        [fx + x * cos - y * sin, fy + x * sin + y * cos, fz + z]
    });
    let base = match player.team {
        Team::Red => RED,
        Team::Blue | _ => BLU,
    };
    let front = base.map(|c| c + (1.0 - c) * 0.25);
    let mut out = [BoxVertex {
        position: [0.0; 3],
        color: base,
    }; 36];
    for (face, quad) in FACES.iter().enumerate() {
        let color = if face == 0 { front } else { base };
        for (slot, corner) in [0, 1, 2, 0, 2, 3].into_iter().enumerate() {
            out[face * 6 + slot] = BoxVertex {
                position: corners[quad[corner]],
                color,
            };
        }
    }
    out
}

fn normalize(a: [f32; 3]) -> [f32; 3] {
    let len = dot(a, a).sqrt();
    a.map(|c| c / len)
}

fn team_color(team: Team) -> [f32; 3] {
    match team {
        Team::Red => RED,
        Team::Blue | _ => BLU,
    }
}

/// Where shots are seen from: the eye and the viewport's height in logical pixels, so a shot
/// covers the same size on a 2x display.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShotSight {
    pub eye: [f32; 3],
    pub height_px: u32,
}

/// The views as shots, in order; with `hide_near` (the first-person eye), shots closer than
/// `PROJECTILE_HIDE_NEAR` to it are left out. With a `sight`, each shot grows with its distance
/// so it covers at least `MIN_SHOT_PX`; without one (no measured viewport) it keeps its base size.
pub fn projectile_shots(
    views: &[ProjectileView],
    hide_near: Option<[f32; 3]>,
    sight: Option<ShotSight>,
) -> Arc<[ProjectileShot]> {
    views
        .iter()
        .filter(|view| {
            hide_near.is_none_or(|eye| {
                let d = [0, 1, 2].map(|i| view.position()[i] - eye[i]);
                dot(d, d) >= PROJECTILE_HIDE_NEAR * PROJECTILE_HIDE_NEAR
            })
        })
        .map(|view| {
            // Only kinds that fly nose first lie along their heading, and only once they moved.
            let shaped_long = match view.kind() {
                ProjectileKind::Rocket
                | ProjectileKind::Arrow
                | ProjectileKind::Flare
                | ProjectileKind::EnergyBall
                | ProjectileKind::EnergyRing => true,
                ProjectileKind::Pipe
                | ProjectileKind::Sticky
                | ProjectileKind::Thrown
                | ProjectileKind::Other
                | _ => false,
            };
            let long = shaped_long && view.heading().is_some();
            let narrow = if long { SHOT_THIN } else { SHOT_ROUND };
            ProjectileShot {
                position: view.position(),
                heading: view.heading(),
                long,
                team: view.team(),
                scale: sight.map_or(1.0, |sight| shot_scale(view.position(), narrow, sight)),
            }
        })
        .collect()
}

// How much a shot whose narrower side is `narrow` units must grow at `at` to cover
// `MIN_SHOT_PX`: one unit at distance d spans `height / (2 d tan(vfov / 2))` pixels.
fn shot_scale(at: [f32; 3], narrow: f32, sight: ShotSight) -> f32 {
    if sight.height_px == 0 {
        return 1.0;
    }
    let d = [0, 1, 2].map(|i| at[i] - sight.eye[i]);
    let units_per_px = 2.0 * dot(d, d).sqrt() * TAN_HALF_VFOV / sight.height_px as f32;
    (MIN_SHOT_PX * units_per_px / narrow).max(1.0)
}

/// 8 triangles wound counter-clockwise seen from outside, in the team colour mixed halfway to white.
pub fn projectile_vertices(shot: &ProjectileShot) -> [BoxVertex; 24] {
    // A right-handed frame with u along the heading (else +X), so every face's tips can be
    // ordered by the sign of its octant and still wind outward.
    let u = shot
        .heading
        .filter(|_| shot.long)
        .unwrap_or([1.0, 0.0, 0.0]);
    let helper = if u[2].abs() > 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let v = normalize(cross(helper, u));
    let w = cross(u, v);
    let (a, b) = if shot.long {
        (SHOT_LONG / 2.0, SHOT_THIN / 2.0)
    } else {
        (SHOT_ROUND / 2.0, SHOT_ROUND / 2.0)
    };
    let (a, b) = (a * shot.scale, b * shot.scale);
    let color = team_color(shot.team).map(|c| c + (1.0 - c) * 0.5);
    let tip = |axis: [f32; 3], scale: f32| [0, 1, 2].map(|i| shot.position[i] + axis[i] * scale);
    let mut out = [BoxVertex {
        position: shot.position,
        color,
    }; 24];
    for octant in 0..8 {
        let su = if octant & 1 == 0 { 1.0 } else { -1.0 };
        let sv = if octant & 2 == 0 { 1.0 } else { -1.0 };
        let sw = if octant & 4 == 0 { 1.0 } else { -1.0 };
        let mut face = [tip(u, su * a), tip(v, sv * b), tip(w, sw * b)];
        if su * sv * sw < 0.0 {
            face.swap(1, 2);
        }
        for (slot, position) in face.into_iter().enumerate() {
            out[octant * 3 + slot] = BoxVertex { position, color };
        }
    }
    out
}
