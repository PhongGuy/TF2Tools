//! Source's compressed animation values: packed quaternions, half-float vectors and the
//! run-length encoded value channels, plus the quaternion and matrix math the rig needs.

use crate::demo_view::studio::bytes::Bytes;
use crate::error::RigLoadError;

/// The w that makes `x, y, z` a unit quaternion, clamped at 0 so overlong input gives 0, not NaN.
fn unit_w(x: f32, y: f32, z: f32, negative: bool) -> f32 {
    let w = (1. - x * x - y * y - z * z).max(0.).sqrt();
    if negative { -w } else { w }
}

/// Quaternion48: x and y in 16 bits each, z in the low 15 bits of the third word, whose top
/// bit negates the derived w. Not normalised, as in the engine.
pub(crate) fn quat48(raw: [u8; 6]) -> [f32; 4] {
    let word = |i: usize| u16::from_le_bytes([raw[2 * i], raw[2 * i + 1]]);
    let x = (f32::from(word(0)) - 32_768.) / 32_768.;
    let y = (f32::from(word(1)) - 32_768.) / 32_768.;
    let z = (f32::from(word(2) & 0x7fff) - 16_384.) / 16_384.;
    [x, y, z, unit_w(x, y, z, word(2) & 0x8000 != 0)]
}

/// Quaternion64: one little-endian u64 with x, y, z in 21-bit fields and bit 63 negating w.
pub(crate) fn quat64(raw: [u8; 8]) -> [f32; 4] {
    let v = u64::from_le_bytes(raw);
    let field = |shift: u32| ((v >> shift) & 0x1f_ffff) as f32;
    let scale = 1. / 1_048_576.5f32;
    let x = (field(0) - 1_048_576.) * scale;
    let y = (field(21) - 1_048_576.) * scale;
    let z = (field(42) - 1_048_576.) * scale;
    [x, y, z, unit_w(x, y, z, v >> 63 != 0)]
}

/// Vector48: three half floats, with infinities clamped to ±65504 and NaN read as 0 like
/// Source's `float16`.
pub(crate) fn vec48(raw: [u8; 6]) -> [f32; 3] {
    std::array::from_fn(|i| {
        let v = half::f16::from_bits(u16::from_le_bytes([raw[2 * i], raw[2 * i + 1]])).to_f32();
        if v.is_nan() {
            0.
        } else if v.is_infinite() {
            65_504f32.copysign(v)
        } else {
            v
        }
    })
}

/// Source's AngleQuaternion(RadianEuler): q = qz(e[2]) * qy(e[1]) * qx(e[0]).
pub(crate) fn euler_quat(e: [f32; 3]) -> [f32; 4] {
    let (sr, cr) = (e[0] * 0.5).sin_cos();
    let (sp, cp) = (e[1] * 0.5).sin_cos();
    let (sy, cy) = (e[2] * 0.5).sin_cos();
    [
        sr * cp * cy - cr * sp * sy,
        cr * sp * cy + sr * cp * sy,
        cr * cp * sy - sr * sp * cy,
        cr * cp * cy + sr * sp * sy,
    ]
}

/// Source's QuaternionBlend: flip q when |p - q|² > |p + q|², lerp by t, normalise.
pub(crate) fn blend(p: [f32; 4], q: [f32; 4], t: f32) -> [f32; 4] {
    let apart: f32 = (0..4).map(|i| (p[i] - q[i]).powi(2)).sum();
    let together: f32 = (0..4).map(|i| (p[i] + q[i]).powi(2)).sum();
    let sign = if apart > together { -1. } else { 1. };
    let mixed: [f32; 4] = std::array::from_fn(|i| p[i] * (1. - t) + sign * q[i] * t);
    let len = mixed.iter().map(|v| v * v).sum::<f32>().sqrt();
    if len > 0. {
        mixed.map(|v| v / len)
    } else {
        mixed
    }
}

/// The 3x4 bone matrix for a rotation `quat` ([x, y, z, w]) followed by a move to `pos`.
pub(crate) fn pose_matrix(pos: [f32; 3], quat: [f32; 4]) -> [[f32; 4]; 3] {
    let [x, y, z, w] = quat;
    [
        [
            1. - 2. * y * y - 2. * z * z,
            2. * x * y - 2. * w * z,
            2. * x * z + 2. * w * y,
            pos[0],
        ],
        [
            2. * x * y + 2. * w * z,
            1. - 2. * x * x - 2. * z * z,
            2. * y * z - 2. * w * x,
            pos[1],
        ],
        [
            2. * x * z - 2. * w * y,
            2. * y * z + 2. * w * x,
            1. - 2. * x * x - 2. * y * y,
            pos[2],
        ],
    ]
}

/// a * b, both with an implied [0, 0, 0, 1] last row.
pub(crate) fn mul34(a: &[[f32; 4]; 3], b: &[[f32; 4]; 3]) -> [[f32; 4]; 3] {
    std::array::from_fn(|r| {
        std::array::from_fn(|c| {
            let rotated: f32 = (0..3).map(|k| a[r][k] * b[k][c]).sum();
            if c == 3 { rotated + a[r][3] } else { rotated }
        })
    })
}

/// One animation value channel: the RLE words copied from the file, as many as `frames` need.
pub(crate) struct Channel {
    words: Box<[u16]>,
}

impl Channel {
    /// Copies whole runs from `at` until their frame totals cover `frames` or a zero-frame run
    /// ends the data; a run cut short by the end of the file is `Malformed`.
    pub(crate) fn read(
        bytes: &Bytes<'_>,
        at: usize,
        frames: usize,
    ) -> Result<Channel, RigLoadError> {
        let mut words = Vec::new();
        let mut covered = 0usize;
        let mut pos = at;
        while covered < frames {
            let header = bytes.u16(pos)?;
            let [valid, total] = header.to_le_bytes();
            words.push(header);
            for i in 0..usize::from(valid) {
                words.push(bytes.u16(pos + 2 + 2 * i)?);
            }
            if total == 0 {
                break;
            }
            covered += usize::from(total);
            pos += 2 + 2 * usize::from(valid);
        }
        Ok(Channel {
            words: words.into_boxed_slice(),
        })
    }

    /// The value at `frame`: a run's values in order, then its last value repeated; a frame
    /// past the data, or behind a zero-frame run, reads 0.
    pub(crate) fn value(&self, frame: usize) -> i16 {
        let mut k = frame;
        let mut i = 0usize;
        while let Some(&header) = self.words.get(i) {
            let [valid, total] = header.to_le_bytes();
            let (valid, total) = (usize::from(valid), usize::from(total));
            if total == 0 {
                return 0;
            }
            if k < total {
                // An empty run's "last value" is its own header word, as the engine reads it.
                let at = if k < valid { i + 1 + k } else { i + valid };
                return self.words.get(at).map_or(0, |&w| w as i16);
            }
            k -= total;
            i += 1 + valid;
        }
        0
    }

    pub(crate) fn heap_bytes(&self) -> usize {
        self.words.len() * size_of::<u16>()
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::{FRAC_1_SQRT_2, FRAC_PI_2};

    use super::*;
    use crate::demo_view::studio::bytes::Bytes;
    use crate::demo_view::studio::fixture::{half_bytes, q64_bytes, rle};

    fn close(a: &[f32], b: &[f32], eps: f32) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() <= eps)
    }

    #[test]
    fn quat48_reads_16_16_15_bits_and_the_w_sign() {
        let mut raw = [0u8; 6];
        raw[0..2].copy_from_slice(&49_152u16.to_le_bytes());
        raw[2..4].copy_from_slice(&24_576u16.to_le_bytes());
        raw[4..6].copy_from_slice(&(20_480u16 | 0x8000).to_le_bytes());
        let q = quat48(raw);
        assert!(
            close(&q, &[0.5, -0.25, 0.25, -(0.625f32).sqrt()], 1e-6),
            "{q:?}"
        );
    }

    #[test]
    fn quat64_reads_21_bit_fields_and_the_w_sign() {
        let (x, y, z) = (
            1_048_576u64 + 104_857,
            1_048_576u64 - 209_715,
            1_048_576u64 + 314_573,
        );
        let q = quat64(q64_bytes(x, y, z, true));
        let d = 1_048_576.5f32;
        let (ex, ey, ez) = (104_857. / d, -209_715. / d, 314_573. / d);
        let ew = -(1. - ex * ex - ey * ey - ez * ez).sqrt();
        assert!(close(&q, &[ex, ey, ez, ew], 1e-6), "{q:?}");
    }

    #[test]
    fn a_quaternion_past_unit_length_gets_w_zero_not_nan() {
        let max = 0x1f_ffffu64;
        let q = quat64(q64_bytes(max, max, max, false));
        assert_eq!(q[3], 0.);
    }

    #[test]
    fn vector48_is_three_halves_with_sources_inf_and_nan_rules() {
        let halves = |h: [u16; 3]| vec48(half_bytes(h));
        assert_eq!(halves([0x3c00, 0xc000, 0x3800]), [1., -2., 0.5]);
        assert_eq!(halves([0x7c00, 0xfc00, 0x7e00]), [65_504., -65_504., 0.]);
    }

    #[test]
    fn euler_angles_turn_about_x_then_y_then_z() {
        let h = FRAC_1_SQRT_2;
        assert!(close(
            &euler_quat([0., 0., FRAC_PI_2]),
            &[0., 0., h, h],
            1e-6
        ));
        assert!(close(
            &euler_quat([FRAC_PI_2, 0., FRAC_PI_2]),
            &[0.5, 0.5, 0.5, 0.5],
            1e-6
        ));
        assert!(close(
            &euler_quat([FRAC_PI_2, FRAC_PI_2, 0.]),
            &[0.5, 0.5, -0.5, 0.5],
            1e-6
        ));
        let m = pose_matrix([0.; 3], euler_quat([0., 0., FRAC_PI_2]));
        assert!(
            close(&[m[0][0], m[1][0], m[2][0]], &[0., 1., 0.], 1e-6),
            "yaw turns +x to +y"
        );
    }

    #[test]
    fn blend_aligns_then_lerps_and_normalises() {
        let id = [0., 0., 0., 1.];
        assert!(
            close(&blend(id, [0., 0., 0., -1.], 0.5), &id, 1e-6),
            "-q is the same rotation"
        );
        let h = FRAC_1_SQRT_2;
        assert!(close(
            &blend(id, [0., 0., 1., 0.], 0.5),
            &[0., 0., h, h],
            1e-6
        ));
        assert!(close(&blend(id, [0., 0., 1., 0.], 0.), &id, 1e-6));
    }

    #[test]
    fn child_transforms_compose_under_their_parent() {
        let parent = pose_matrix([0., 0., 10.], euler_quat([0., 0., FRAC_PI_2]));
        let child = pose_matrix([1., 0., 0.], [0., 0., 0., 1.]);
        let world = mul34(&parent, &child);
        assert!(close(
            &[world[0][3], world[1][3], world[2][3]],
            &[0., 1., 10.],
            1e-6
        ));
    }

    #[test]
    fn rle_runs_repeat_their_last_value() {
        let d = rle(&[(&[10, 20], 3), (&[-5], 2)]);
        let c = Channel::read(&Bytes::new("a.mdl", &d), 0, 5).unwrap();
        let got: Vec<i16> = (0..6).map(|f| c.value(f)).collect();
        assert_eq!(got, [10, 20, 20, -5, -5, 0]);
    }

    #[test]
    fn rle_copy_stops_once_the_frames_are_covered() {
        let mut d = rle(&[(&[1, 2, 3], 3)]);
        d.extend_from_slice(&[0xff; 4]); // the next track's bytes, never a run
        let c = Channel::read(&Bytes::new("a.mdl", &d), 0, 3).unwrap();
        assert_eq!(c.heap_bytes(), 8, "one header and three values");
        assert_eq!((c.value(2), c.value(3)), (3, 0));
    }

    #[test]
    fn rle_edge_runs_follow_the_engine() {
        let d = rle(&[(&[7], 1), (&[], 0)]);
        let c = Channel::read(&Bytes::new("a.mdl", &d), 0, 4).unwrap();
        assert_eq!(
            (c.value(0), c.value(2)),
            (7, 0),
            "a zero-frame run ends the data"
        );
        let d = rle(&[(&[], 4)]);
        let c = Channel::read(&Bytes::new("a.mdl", &d), 0, 4).unwrap();
        assert_eq!(c.value(1), 0x0400, "an empty run reads its own header word");
    }

    #[test]
    fn a_cut_short_run_is_malformed() {
        let d = rle(&[(&[1, 2, 3], 3)]);
        let cut = &d[..5];
        assert!(matches!(
            Channel::read(&Bytes::new("a.mdl", cut), 0, 3),
            Err(RigLoadError::Malformed { .. })
        ));
    }
}
