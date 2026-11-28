//! One studio animation: its descriptor and its per-bone tracks, sampled into bone poses.

use std::sync::Arc;

use crate::demo_view::studio::MAX_FRAMES;
use crate::demo_view::studio::bytes::Bytes;
use crate::demo_view::studio::values::{quat48, quat64, vec48};
use crate::error::RigLoadError;

/// Track flag: a Vector48 position follows the rotation.
const RAW_POS: u8 = 0x01;
/// Track flag: a Quaternion48 rotation.
const RAW_ROT: u8 = 0x02;
/// Track flag: an RLE-compressed position.
const ANIM_POS: u8 = 0x04;
/// Track flag: RLE-compressed Euler angles.
const ANIM_ROT: u8 = 0x08;
/// Track flag: a Quaternion64 rotation.
const RAW_ROT2: u8 = 0x20;
/// Bone index that ends a track chain.
const CHAIN_END: u8 = 255;
/// Most track headers one chain may link.
const MAX_LINKS: usize = 256;

/// One bone of the file an animation lives in: what its tracks are relative to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FileBone {
    pub(crate) pos: [f32; 3],
    pub(crate) quat: [f32; 4],
    pub(crate) rot: [f32; 3],
    pub(crate) pos_scale: [f32; 3],
    pub(crate) rot_scale: [f32; 3],
}

/// A bone's transform relative to its parent.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct BonePose {
    pub(crate) pos: [f32; 3],
    pub(crate) quat: [f32; 4],
}

impl BonePose {
    /// No offset and no turn.
    pub const IDENTITY: BonePose = BonePose {
        pos: [0.; 3],
        quat: [0., 0., 0., 1.],
    };

    pub(crate) fn new(pos: [f32; 3], quat: [f32; 4]) -> BonePose {
        BonePose { pos, quat }
    }

    /// The offset from the parent bone.
    pub fn pos(&self) -> [f32; 3] {
        self.pos
    }

    /// The rotation relative to the parent bone, as x, y, z, w.
    pub fn quat(&self) -> [f32; 4] {
        self.quat
    }
}

/// What an animation needs from the model it is read for.
pub(crate) struct AnimContext<'a> {
    /// The file's own bones.
    pub(crate) bones: &'a [FileBone],
    /// File bone to base bone; `None` for a bone the base lacks.
    pub(crate) to_base: &'a [Option<usize>],
    /// Per base bone: the pose of a bone the animation leaves untracked.
    pub(crate) rest: Arc<[BonePose]>,
}

/// One bone's decoded track, already moved onto its base bone.
#[derive(Debug, Clone, Copy)]
struct Track {
    base: usize,
    quat: [f32; 4],
    pos: [f32; 3],
}

/// One animation, its tracks kept compressed per section.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Animation {
    name: String,
    fps: f32,
    flags: i32,
    frames: usize,
    section_frames: usize,
    sections: Vec<Vec<Track>>,
    rest: Arc<[BonePose]>,
}

impl Animation {
    /// Reads the 100-byte descriptor at `at` and its first track block.
    pub(crate) fn read(
        bytes: &Bytes<'_>,
        at: usize,
        ctx: &AnimContext<'_>,
    ) -> Result<Animation, RigLoadError> {
        let name = bytes.cstr(at, bytes.i32(at + 4)?)?;
        let fps = bytes.f32(at + 8)?;
        let flags = bytes.i32(at + 12)?;
        let frames = bytes.i32(at + 16)?;
        let frames = usize::try_from(frames)
            .map_err(|_| bytes.malformed(format!("animation {name} has {frames} frames")))?;
        if frames > MAX_FRAMES {
            return Err(bytes.too_large());
        }
        let block = bytes.rel(at, bytes.i32(at + 56)?)?;
        let tracks = read_chain(bytes, block, ctx)?;
        Ok(Animation {
            name,
            fps,
            flags,
            frames,
            section_frames: 0,
            sections: vec![tracks],
            rest: Arc::clone(&ctx.rest),
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn fps(&self) -> f32 {
        self.fps
    }

    pub fn frames(&self) -> usize {
        self.frames
    }

    pub fn flags(&self) -> i32 {
        self.flags
    }

    pub fn is_looping(&self) -> bool {
        self.flags & 0x1 != 0
    }

    pub fn is_delta(&self) -> bool {
        self.flags & 0x4 != 0
    }

    /// Writes the pose at `frame` (clamped to the animation; non-finite reads frame 0) into
    /// `out`, one entry per base bone, as far as both `out` and the rest pose reach.
    pub fn sample(&self, _frame: f32, out: &mut [BonePose]) {
        let reach = out.len().min(self.rest.len());
        for (slot, rest) in out.iter_mut().zip(self.rest.iter()) {
            *slot = if self.is_delta() {
                BonePose::IDENTITY
            } else {
                *rest
            };
        }
        // Raw tracks hold one value for the whole animation, so any frame reads the same.
        for track in self.sections.first().into_iter().flatten() {
            if track.base < reach {
                out[track.base] = BonePose::new(track.pos, track.quat);
            }
        }
    }

    pub(crate) fn heap_bytes(&self) -> usize {
        self.name.capacity()
            + self.sections.capacity() * size_of::<Vec<Track>>()
            + self
                .sections
                .iter()
                .map(|s| s.capacity() * size_of::<Track>())
                .sum::<usize>()
    }
}

/// Follows one block's chain of track headers from `at` until bone 255 or a zero link.
fn read_chain(
    bytes: &Bytes<'_>,
    mut at: usize,
    ctx: &AnimContext<'_>,
) -> Result<Vec<Track>, RigLoadError> {
    let mut tracks = Vec::new();
    for _ in 0..MAX_LINKS {
        let bone = bytes.u8(at)?;
        if bone == CHAIN_END {
            return Ok(tracks);
        }
        let flags = bytes.u8(at + 1)?;
        let next = bytes.i16(at + 2)?;
        if next < 0 {
            return Err(bytes.malformed(format!("a track at {at} links backwards")));
        }
        if flags & (ANIM_POS | ANIM_ROT) != 0 {
            return Err(bytes.malformed(format!("the track at {at} is compressed")));
        }
        // A track on a bone the base model lacks, or past the file's bones, plays no part.
        let file_bone = ctx.bones.get(usize::from(bone));
        let base = ctx.to_base.get(usize::from(bone)).copied().flatten();
        if let (Some(file_bone), Some(base)) = (file_bone, base) {
            tracks.push(read_raw(bytes, at + 4, flags, file_bone, base)?);
        }
        if next == 0 {
            return Ok(tracks);
        }
        at = bytes.rel(at, i32::from(next))?;
    }
    Err(bytes.malformed(format!("a track chain links more than {MAX_LINKS} times")))
}

/// The raw rotation and position at `data`; what the track lacks comes from the file bone.
fn read_raw(
    bytes: &Bytes<'_>,
    data: usize,
    flags: u8,
    bone: &FileBone,
    base: usize,
) -> Result<Track, RigLoadError> {
    let quat = if flags & RAW_ROT != 0 {
        quat48(bytes.array(data)?)
    } else if flags & RAW_ROT2 != 0 {
        quat64(bytes.array(data)?)
    } else {
        bone.quat
    };
    // The position sits past both rotation slots the flags name, as the engine lays it out.
    let skip = 6 * usize::from(flags & RAW_ROT != 0) + 8 * usize::from(flags & RAW_ROT2 != 0);
    let pos = if flags & RAW_POS != 0 {
        vec48(bytes.array(data + skip)?)
    } else {
        bone.pos
    };
    Ok(Track { base, quat, pos })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::demo_view::studio::bytes::Bytes;
    use crate::demo_view::studio::fixture::{
        AnimSpec, Blob, TrackBytes, half_bytes, q64_bytes, write_anims,
    };
    use crate::demo_view::studio::values::{euler_quat, quat64};

    const ID: [f32; 4] = [0., 0., 0., 1.];

    fn bone(pos: [f32; 3], rot: [f32; 3]) -> FileBone {
        FileBone {
            pos,
            quat: euler_quat(rot),
            rot,
            pos_scale: [0.5; 3],
            rot_scale: [0.01; 3],
        }
    }

    fn rest(bones: &[FileBone]) -> Arc<[BonePose]> {
        bones.iter().map(|b| BonePose::new(b.pos, b.quat)).collect()
    }

    fn read(
        spec: AnimSpec,
        bones: &[FileBone],
        to_base: &[Option<usize>],
        rest: Arc<[BonePose]>,
    ) -> Result<Animation, RigLoadError> {
        let mut blob = Blob::new();
        let at = write_anims(&mut blob, &[spec]);
        Animation::read(
            &Bytes::new("a.mdl", &blob.0),
            at,
            &AnimContext {
                bones,
                to_base,
                rest,
            },
        )
    }

    fn sample(anim: &Animation, frame: f32, bones: usize) -> Vec<BonePose> {
        let mut out = vec![BonePose::IDENTITY; bones];
        anim.sample(frame, &mut out);
        out
    }

    fn close(a: &[f32], b: &[f32]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() <= 1e-5)
    }

    #[test]
    fn a_raw_track_holds_one_quaternion_and_position_for_every_frame() {
        let bones = [bone([1., 2., 3.], [0.; 3])];
        let q = q64_bytes(1_048_576 + 524_288, 1_048_576, 1_048_576, false);
        let body = [
            q.as_slice(),
            half_bytes([0x3c00, 0x4000, 0x4200]).as_slice(),
        ]
        .concat();
        let spec = AnimSpec::plain(
            "idle",
            10,
            vec![TrackBytes {
                bone: 0,
                flags: 0x21,
                body,
            }],
        );
        let anim = read(spec, &bones, &[Some(0)], rest(&bones)).unwrap();
        assert_eq!((anim.name(), anim.frames(), anim.fps()), ("idle", 10, 30.));
        for frame in [0., 3.5, 9.] {
            let out = sample(&anim, frame, 1);
            assert!(close(&out[0].quat(), &quat64(q)), "frame {frame}");
            assert!(close(&out[0].pos(), &[1., 2., 3.]), "frame {frame}");
        }
    }

    #[test]
    fn untracked_bones_take_the_rest_pose_and_unmapped_tracks_are_dropped() {
        let file = [bone([1., 0., 0.], [0.; 3]), bone([2., 0., 0.], [0.; 3])];
        let base_rest: Arc<[BonePose]> = [[7., 0., 0.], [8., 0., 0.], [9., 0., 0.]]
            .map(|p| BonePose::new(p, ID))
            .into();
        let body = half_bytes([0x3c00, 0x3c00, 0x3c00]).to_vec();
        let spec = AnimSpec::plain(
            "x",
            3,
            vec![TrackBytes {
                bone: 1,
                flags: 0x01,
                body,
            }],
        );
        let anim = read(spec, &file, &[Some(2), None], Arc::clone(&base_rest)).unwrap();
        let out = sample(&anim, 1., 3);
        assert_eq!(
            out.as_slice(),
            &base_rest[..],
            "the only track is on a bone the base lacks"
        );
        let mut short = [BonePose::IDENTITY; 1];
        anim.sample(1., &mut short);
        assert_eq!(
            short[0], base_rest[0],
            "a short buffer is filled as far as it goes"
        );
    }

    #[test]
    fn bone_255_ends_the_track_chain() {
        let bones = [bone([1., 0., 0.], [0.; 3]), bone([2., 0., 0.], [0.; 3])];
        let raw = |bone| TrackBytes {
            bone,
            flags: 0x01,
            body: half_bytes([0x4900, 0, 0]).to_vec(),
        };
        let end = TrackBytes {
            bone: 255,
            flags: 0,
            body: Vec::new(),
        };
        let spec = AnimSpec::plain("x", 1, vec![raw(0), end, raw(1)]);
        let anim = read(spec, &bones, &[Some(0), Some(1)], rest(&bones)).unwrap();
        let out = sample(&anim, 0., 2);
        assert_eq!((out[0].pos()[0], out[1].pos()[0]), (10., 2.));
    }
}
