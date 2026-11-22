//! Camera math for the viewer in Source's frame (Z up, yaw counter-clockwise from +X, positive
//! pitch looks down), and the row unpadding the GPU readback needs.

use tf2tools_core::MapMesh;

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

/// The `(d_yaw, d_pitch)` in degrees for a drag: right turns right, down looks down.
pub fn drag_look(dx_px: f32, dy_px: f32) -> (f32, f32) {
    (-dx_px * LOOK_DEG_PER_PX, dy_px * LOOK_DEG_PER_PX)
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

/// Where the camera starts: the first spawn at eye height facing its yaw, else the middle of
/// the map's bounds looking along +X (the origin for an empty mesh).
pub fn start_camera(mesh: &MapMesh) -> ([f32; 3], f32) {
    if let Some((origin, yaw)) = mesh.start() {
        return ([origin[0], origin[1], origin[2] + EYE_HEIGHT], yaw);
    }
    if mesh.triangle_count() == 0 {
        return ([0.0; 3], 0.0);
    }
    let (min, max) = mesh.bounds();
    ([0, 1, 2].map(|i| (min[i] + max[i]) / 2.0), 0.0)
}
