//! One studio animation: its descriptor and its per-bone tracks, sampled into bone poses.

use std::sync::Arc;

use crate::demo_view::studio::bytes::Bytes;
use crate::demo_view::studio::values::{Channel, blend, euler_quat, quat48, quat64, vec48};
use crate::demo_view::studio::{MAX_FRAMES, MAX_TABLE};
use crate::error::RigLoadError;

/// Track flag: a Vector48 position follows the rotation.
const RAW_POS: u8 = 0x01;
/// Track flag: a Quaternion48 rotation.
const RAW_ROT: u8 = 0x02;
/// Track flag: an RLE-compressed position.
const ANIM_POS: u8 = 0x04;
/// Track flag: RLE-compressed Euler angles.
const ANIM_ROT: u8 = 0x08;
/// Track flag: values are deltas, so the file bone is never added and absent parts are identity.
const DELTA: u8 = 0x10;
/// Track flag: a Quaternion64 rotation.
const RAW_ROT2: u8 = 0x20;
/// Bone index that ends a track chain.
const CHAIN_END: u8 = 255;
/// Most track headers one chain may link.
const MAX_LINKS: usize = 256;
/// Size of one movement entry.
const MOVEMENT_BYTES: usize = 44;

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

/// One block of an animation's root motion.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct Movement {
    end_frame: i32,
    motion_flags: i32,
    v0: f32,
    v1: f32,
    angle: f32,
    vector: [f32; 3],
    position: [f32; 3],
}

impl Movement {
    /// The frame this block of motion ends at.
    pub fn end_frame(&self) -> i32 {
        self.end_frame
    }

    /// Which axes the motion moves along, as the engine's `STUDIO_X` family of bits.
    pub fn motion_flags(&self) -> i32 {
        self.motion_flags
    }

    /// Speed at the block's start.
    pub fn v0(&self) -> f32 {
        self.v0
    }

    /// Speed at the block's end.
    pub fn v1(&self) -> f32 {
        self.v1
    }

    /// Yaw turned over the block, in degrees.
    pub fn angle(&self) -> f32 {
        self.angle
    }

    /// Direction of travel.
    pub fn vector(&self) -> [f32; 3] {
        self.vector
    }

    /// Offset reached at the block's end.
    pub fn position(&self) -> [f32; 3] {
        self.position
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

/// A track's rotation: one fixed value, three Euler channels, or none.
#[derive(Debug, Clone)]
enum Rot {
    Fixed([f32; 4]),
    Euler([Option<Channel>; 3]),
    Absent,
}

/// A track's position: one fixed value, three channels, or none.
#[derive(Debug, Clone)]
enum Pos {
    Fixed([f32; 3]),
    Channels([Option<Channel>; 3]),
    Absent,
}

/// One bone's track for one section, already moved onto its base bone.
#[derive(Debug, Clone)]
struct Track {
    base: usize,
    bone: FileBone,
    delta: bool,
    rot: Rot,
    pos: Pos,
}

/// A missing channel reads 0, as the engine's null value pointer does.
fn channel_values(chans: &[Option<Channel>; 3], frame: usize) -> [i16; 3] {
    std::array::from_fn(|j| chans[j].as_ref().map_or(0, |c| c.value(frame)))
}

impl Track {
    /// The pose at section-local `frame`, `s` of the way to the next frame.
    fn pose(&self, frame: usize, s: f32) -> BonePose {
        let quat = match &self.rot {
            Rot::Fixed(q) => *q,
            Rot::Absent if self.delta => BonePose::IDENTITY.quat,
            Rot::Absent => self.bone.quat,
            Rot::Euler(chans) => {
                let angles = |f| {
                    let v = channel_values(chans, f);
                    std::array::from_fn(|j| {
                        f32::from(v[j]) * self.bone.rot_scale[j] + self.rot_base(j)
                    })
                };
                // Like the engine, blend two whole quaternions rather than lerp the angles.
                let a1: [f32; 3] = angles(frame);
                let a2 = if s > 0. { angles(frame + 1) } else { a1 };
                if a1 == a2 {
                    euler_quat(a1)
                } else {
                    blend(euler_quat(a1), euler_quat(a2), s)
                }
            }
        };
        let pos = match &self.pos {
            Pos::Fixed(p) => *p,
            Pos::Absent if self.delta => BonePose::IDENTITY.pos,
            Pos::Absent => self.bone.pos,
            Pos::Channels(chans) => {
                let (v1, v2) = (
                    channel_values(chans, frame),
                    channel_values(chans, frame + 1),
                );
                std::array::from_fn(|j| {
                    let v = f32::from(v1[j]) * (1. - s) + f32::from(v2[j]) * s;
                    v * self.bone.pos_scale[j] + if self.delta { 0. } else { self.bone.pos[j] }
                })
            }
        };
        BonePose::new(pos, quat)
    }

    fn rot_base(&self, axis: usize) -> f32 {
        if self.delta { 0. } else { self.bone.rot[axis] }
    }

    fn heap_bytes(&self) -> usize {
        let rot: &[Option<Channel>] = match &self.rot {
            Rot::Euler(c) => c,
            Rot::Fixed(_) | Rot::Absent => &[],
        };
        let pos: &[Option<Channel>] = match &self.pos {
            Pos::Channels(c) => c,
            Pos::Fixed(_) | Pos::Absent => &[],
        };
        rot.iter()
            .chain(pos)
            .flatten()
            .map(Channel::heap_bytes)
            .sum()
    }
}

/// The undecoded track of one bone at one frame, for the vmdl oracle.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RawSample {
    pub(crate) rot: RawRot,
    pub(crate) pos: RawPos,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum RawRot {
    Fixed([f32; 4]),
    Euler { values: [i16; 3], scale: [f32; 3] },
    Default,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum RawPos {
    Fixed([f32; 3]),
    Channels { values: [i16; 3], scale: [f32; 3] },
    Default,
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
    movements: Vec<Movement>,
    rest: Arc<[BonePose]>,
}

impl Animation {
    /// Reads the 100-byte descriptor at `at`, its movements and every section's track block.
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
            .ok()
            .filter(|&f| f > 0)
            .ok_or_else(|| bytes.malformed(format!("animation {name} has {frames} frames")))?;
        if frames > MAX_FRAMES {
            return Err(bytes.too_large());
        }
        if bytes.i32(at + 52)? != 0 {
            return Err(bytes.malformed(format!("animation {name} lives in animation blocks")));
        }
        let movements = read_movements(bytes, at)?;
        let section_frames = bytes.i32(at + 84)?;
        let section_frames = usize::try_from(section_frames).map_err(|_| {
            bytes.malformed(format!(
                "animation {name} has {section_frames} section frames"
            ))
        })?;
        let sections = if section_frames == 0 {
            let block = bytes.rel(at, bytes.i32(at + 56)?)?;
            vec![read_chain(bytes, block, frames, ctx)?]
        } else {
            read_sections(bytes, at, frames, section_frames, ctx)?
        };
        Ok(Animation {
            name,
            fps,
            flags,
            frames,
            section_frames,
            sections,
            movements,
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

    pub fn movements(&self) -> &[Movement] {
        &self.movements
    }

    /// Writes the pose at `frame` (clamped to the animation; non-finite reads frame 0) into
    /// `out`, one entry per base bone, as far as both `out` and the rest pose reach.
    pub fn sample(&self, frame: f32, out: &mut [BonePose]) {
        let reach = out.len().min(self.rest.len());
        for (slot, rest) in out.iter_mut().zip(self.rest.iter()) {
            *slot = if self.is_delta() {
                BonePose::IDENTITY
            } else {
                *rest
            };
        }
        let last = self.frames - 1;
        let frame = if frame.is_finite() {
            frame.clamp(0., last as f32)
        } else {
            0.
        };
        let lower = (frame.floor() as usize).min(last);
        let s = if lower == last {
            0.
        } else {
            frame - lower as f32
        };
        let (section, local) = self.locate(lower);
        for track in self.sections.get(section).into_iter().flatten() {
            if track.base < reach {
                out[track.base] = track.pose(local, s);
            }
        }
    }

    /// The section holding integer `frame` and the frame's index within it. A long animation's
    /// last frame has a section of its own (`studio.cpp`, `mstudioanimdesc_t::pAnim`).
    fn locate(&self, frame: usize) -> (usize, usize) {
        let sf = self.section_frames;
        if sf == 0 {
            (0, frame)
        } else if self.frames > sf && frame == self.frames - 1 {
            (self.sections.len() - 1, 0)
        } else {
            (frame / sf, frame % sf)
        }
    }

    /// The undecoded track of `base_bone` at integer `frame`, for the vmdl oracle.
    #[cfg(test)]
    pub(crate) fn raw(&self, base_bone: usize, frame: usize) -> Option<RawSample> {
        if frame >= self.frames {
            return None;
        }
        let (section, local) = self.locate(frame);
        let track = self
            .sections
            .get(section)?
            .iter()
            .find(|t| t.base == base_bone)?;
        let rot = match &track.rot {
            Rot::Fixed(q) => RawRot::Fixed(*q),
            Rot::Euler(chans) => RawRot::Euler {
                values: channel_values(chans, local),
                scale: track.bone.rot_scale,
            },
            Rot::Absent => RawRot::Default,
        };
        let pos = match &track.pos {
            Pos::Fixed(p) => RawPos::Fixed(*p),
            Pos::Channels(chans) => RawPos::Channels {
                values: channel_values(chans, local),
                scale: track.bone.pos_scale,
            },
            Pos::Absent => RawPos::Default,
        };
        Some(RawSample { rot, pos })
    }

    pub(crate) fn heap_bytes(&self) -> usize {
        self.name.capacity()
            + self.movements.capacity() * size_of::<Movement>()
            + self.sections.capacity() * size_of::<Vec<Track>>()
            + self
                .sections
                .iter()
                .map(|s| {
                    s.capacity() * size_of::<Track>()
                        + s.iter().map(Track::heap_bytes).sum::<usize>()
                })
                .sum::<usize>()
    }
}

/// The descriptor's root-motion blocks.
fn read_movements(bytes: &Bytes<'_>, at: usize) -> Result<Vec<Movement>, RigLoadError> {
    let table = bytes.table(
        at,
        bytes.i32(at + 24)?,
        bytes.i32(at + 20)?,
        MOVEMENT_BYTES,
        MAX_TABLE,
    )?;
    (0..table.count)
        .map(|i| {
            let m = table.at(i);
            Ok(Movement {
                end_frame: bytes.i32(m)?,
                motion_flags: bytes.i32(m + 4)?,
                v0: bytes.f32(m + 8)?,
                v1: bytes.f32(m + 12)?,
                angle: bytes.f32(m + 16)?,
                vector: bytes.vec3(m + 20)?,
                position: bytes.vec3(m + 32)?,
            })
        })
        .collect()
}

/// Reads the section table: `frames / sf + 2` entries, each covering up to `sf + 1` frames
/// (its first frame through the next section's first), the last holding the lone last frame.
fn read_sections(
    bytes: &Bytes<'_>,
    at: usize,
    frames: usize,
    sf: usize,
    ctx: &AnimContext<'_>,
) -> Result<Vec<Vec<Track>>, RigLoadError> {
    let count = frames / sf + 2;
    let table = bytes.table(at, bytes.i32(at + 80)?, count as i32, 8, MAX_TABLE)?;
    let last = frames - 1;
    (0..count)
        .map(|k| {
            let covers = if k + 1 == count {
                usize::from(frames > sf)
            } else if k * sf > last {
                0
            } else {
                sf.min(last - k * sf) + 1
            };
            // Sections no frame reads hold nothing worth trusting, so they stay unread.
            if covers == 0 {
                return Ok(Vec::new());
            }
            let entry = table.at(k);
            if bytes.i32(entry)? != 0 {
                return Err(bytes.malformed(format!("section {k} lives in an animation block")));
            }
            let block = bytes.rel(at, bytes.i32(entry + 4)?)?;
            read_chain(bytes, block, covers, ctx)
        })
        .collect()
}

/// Follows one block's chain of track headers from `at` until bone 255 or a zero link; each
/// channel copies the `frames` the block covers.
fn read_chain(
    bytes: &Bytes<'_>,
    mut at: usize,
    frames: usize,
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
        let file_bone = ctx.bones.get(usize::from(bone)).ok_or_else(|| {
            bytes.malformed(format!(
                "the track at {at} is on bone {bone}, past the file's bones"
            ))
        })?;
        // A track on a bone the base model lacks plays no part.
        if let Some(base) = ctx.to_base.get(usize::from(bone)).copied().flatten() {
            tracks.push(read_track(bytes, at + 4, flags, file_bone, base, frames)?);
        }
        if next == 0 {
            return Ok(tracks);
        }
        at = bytes.rel(at, i32::from(next))?;
    }
    Err(bytes.malformed(format!("a track chain links more than {MAX_LINKS} times")))
}

/// The track data at `data`: fixed values decode now, channels stay compressed.
fn read_track(
    bytes: &Bytes<'_>,
    data: usize,
    flags: u8,
    bone: &FileBone,
    base: usize,
    frames: usize,
) -> Result<Track, RigLoadError> {
    let rot = if flags & RAW_ROT != 0 {
        Rot::Fixed(quat48(bytes.array(data)?))
    } else if flags & RAW_ROT2 != 0 {
        Rot::Fixed(quat64(bytes.array(data)?))
    } else if flags & ANIM_ROT != 0 {
        Rot::Euler(read_channels(bytes, data, frames)?)
    } else {
        Rot::Absent
    };
    // The position sits past both rotation slots the flags name, as the engine lays it out.
    let skip = 6 * usize::from(flags & RAW_ROT != 0) + 8 * usize::from(flags & RAW_ROT2 != 0);
    let pos = if flags & RAW_POS != 0 {
        Pos::Fixed(vec48(bytes.array(data + skip)?))
    } else if flags & ANIM_POS != 0 {
        let ptr = data + 6 * usize::from(flags & ANIM_ROT != 0);
        Pos::Channels(read_channels(bytes, ptr, frames)?)
    } else {
        Pos::Absent
    };
    Ok(Track {
        base,
        bone: *bone,
        delta: flags & DELTA != 0,
        rot,
        pos,
    })
}

/// The three channels of the value pointer at `ptr`; each u16 offset counts from the pointer
/// and 0 means the channel reads 0.
fn read_channels(
    bytes: &Bytes<'_>,
    ptr: usize,
    frames: usize,
) -> Result<[Option<Channel>; 3], RigLoadError> {
    let mut chans = [None, None, None];
    for (j, chan) in chans.iter_mut().enumerate() {
        let off = bytes.u16(ptr + 2 * j)?;
        if off != 0 {
            *chan = Some(Channel::read(bytes, ptr + usize::from(off), frames)?);
        }
    }
    Ok(chans)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::demo_view::studio::bytes::Bytes;
    use crate::demo_view::studio::fixture::{
        AnimSpec, Blob, TrackBytes, anim_body, half_bytes, q64_bytes, rle, write_anims,
    };
    use crate::demo_view::studio::values::{blend, euler_quat, quat64};

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

    #[test]
    fn animated_channels_scale_then_add_the_bone_base() {
        let bones = [bone([1., 0., 0.], [0., 0., 0.5])];
        let body = anim_body(
            Some([None, None, Some(rle(&[(&[0, 100], 2)]))]),
            Some([Some(rle(&[(&[10, 20], 2)])), None, None]),
        );
        let spec = AnimSpec::plain(
            "turn",
            2,
            vec![TrackBytes {
                bone: 0,
                flags: 0x0c,
                body,
            }],
        );
        let anim = read(spec, &bones, &[Some(0)], rest(&bones)).unwrap();
        let (q0, q1) = (euler_quat([0., 0., 0.5]), euler_quat([0., 0., 1.5]));
        let out = sample(&anim, 0., 1);
        assert!(
            close(&out[0].quat(), &q0) && close(&out[0].pos(), &[6., 0., 0.]),
            "{out:?}"
        );
        let out = sample(&anim, 1., 1);
        assert!(
            close(&out[0].quat(), &q1) && close(&out[0].pos(), &[11., 0., 0.]),
            "{out:?}"
        );
        let out = sample(&anim, 0.5, 1);
        assert!(
            close(&out[0].quat(), &blend(q0, q1, 0.5)) && close(&out[0].pos(), &[8.5, 0., 0.]),
            "{out:?}"
        );
        let Some(RawSample {
            rot: RawRot::Euler { values, scale },
            pos: RawPos::Channels { values: pv, .. },
        }) = anim.raw(0, 1)
        else {
            panic!("raw channels")
        };
        assert_eq!((values, scale, pv), ([0, 0, 100], [0.01; 3], [20, 0, 0]));
    }

    #[test]
    fn delta_tracks_skip_the_base_and_untracked_bones_stay_identity() {
        let bones = [
            bone([1., 0., 0.], [0., 0., 0.5]),
            bone([5., 5., 5.], [0.3, 0., 0.]),
        ];
        let body = anim_body(
            Some([None, None, Some(rle(&[(&[100], 1)]))]),
            Some([Some(rle(&[(&[10], 1)])), None, None]),
        );
        let spec = AnimSpec {
            flags: 0x4,
            ..AnimSpec::plain(
                "aim",
                1,
                vec![TrackBytes {
                    bone: 0,
                    flags: 0x1c,
                    body,
                }],
            )
        };
        let anim = read(spec, &bones, &[Some(0), Some(1)], rest(&bones)).unwrap();
        assert!(anim.is_delta());
        let out = sample(&anim, 0., 2);
        assert!(
            close(&out[0].quat(), &euler_quat([0., 0., 1.])) && close(&out[0].pos(), &[5., 0., 0.]),
            "{out:?}"
        );
        assert_eq!(out[1], BonePose::IDENTITY);
    }

    #[test]
    fn a_raw_delta_track_without_a_rotation_takes_identity_not_the_bone() {
        let bones = [bone([1., 2., 3.], [0.3, 0.2, 0.1])];
        let body = half_bytes([0x3c00, 0, 0]).to_vec();
        let spec = AnimSpec::plain(
            "x",
            2,
            vec![TrackBytes {
                bone: 0,
                flags: 0x11,
                body,
            }],
        );
        let anim = read(spec, &bones, &[Some(0)], rest(&bones)).unwrap();
        assert!(!anim.is_delta());
        let out = sample(&anim, 0., 1);
        assert_eq!((out[0].quat(), out[0].pos()), (ID, [1., 0., 0.]));
        assert!(
            matches!(
                anim.raw(0, 0),
                Some(RawSample {
                    rot: RawRot::Default,
                    pos: RawPos::Fixed([1., 0., 0.])
                })
            ),
            "the absent rotation is reported as absent"
        );
    }

    #[test]
    fn sectioned_frames_read_their_own_section_and_the_last_frame_its_own() {
        let bones = [FileBone {
            pos_scale: [1.; 3],
            ..bone([0.; 3], [0.; 3])
        }];
        let block = |values: Vec<i16>| {
            let n = u8::try_from(values.len()).unwrap();
            let body = anim_body(
                None,
                Some([Some(rle(&[(values.as_slice(), n)])), None, None]),
            );
            vec![TrackBytes {
                bone: 0,
                flags: 0x04,
                body,
            }]
        };
        let spec = AnimSpec {
            section_frames: 30,
            blocks: vec![
                block((0..=30).collect()),
                block((30..=60).collect()),
                block((60..=64).collect()),
                block(vec![999]),
            ],
            ..AnimSpec::plain("long", 65, Vec::new())
        };
        let anim = read(spec, &bones, &[Some(0)], rest(&bones)).unwrap();
        let cases = [
            (0., 0.),
            (29., 29.),
            (30., 30.),
            (45., 45.),
            (61., 61.),
            (63.5, 63.5),
            (64., 999.),
        ];
        for (frame, want) in cases {
            assert_eq!(sample(&anim, frame, 1)[0].pos()[0], want, "frame {frame}");
        }
    }

    #[test]
    fn frames_clamp_to_the_animation_and_non_finite_frames_read_frame_zero() {
        let bones = [FileBone {
            pos_scale: [1.; 3],
            ..bone([0.; 3], [0.; 3])
        }];
        let body = anim_body(None, Some([Some(rle(&[(&[4, 5, 6], 3)])), None, None]));
        let spec = AnimSpec::plain(
            "x",
            3,
            vec![TrackBytes {
                bone: 0,
                flags: 0x04,
                body,
            }],
        );
        let anim = read(spec, &bones, &[Some(0)], rest(&bones)).unwrap();
        for (frame, want) in [(-3., 4.), (7., 6.), (f32::NAN, 4.), (f32::INFINITY, 4.)] {
            assert_eq!(sample(&anim, frame, 1)[0].pos()[0], want, "frame {frame}");
        }
    }

    #[test]
    fn bad_animations_are_refused() {
        let bones = [bone([0.; 3], [0.; 3])];
        let to_base = [Some(0)];
        let raw = |bone| TrackBytes {
            bone,
            flags: 0x01,
            body: half_bytes([0; 3]).to_vec(),
        };
        let refused = |spec: AnimSpec| read(spec, &bones, &to_base, rest(&bones)).err();
        assert!(
            matches!(
                refused(AnimSpec::plain("x", 0, vec![raw(0)])),
                Some(RigLoadError::Malformed { .. })
            ),
            "no frames"
        );
        assert!(
            matches!(
                refused(AnimSpec::plain("x", 5_000, vec![raw(0)])),
                Some(RigLoadError::TooLarge { .. })
            ),
            "frame cap"
        );
        assert!(
            matches!(
                refused(AnimSpec::plain("x", 1, vec![raw(3)])),
                Some(RigLoadError::Malformed { .. })
            ),
            "a track on a bone the file lacks"
        );
        let past_end = anim_body(None, Some([Some(Vec::new()), None, None]));
        let channel = TrackBytes {
            bone: 0,
            flags: 0x04,
            body: past_end,
        };
        assert!(
            matches!(
                refused(AnimSpec::plain("x", 1, vec![channel])),
                Some(RigLoadError::Malformed { .. })
            ),
            "a channel past the end"
        );
        let mut blob = Blob::new();
        let at = write_anims(&mut blob, &[AnimSpec::plain("x", 1, vec![raw(0)])]);
        blob.set_i32(at + 52, 1);
        let ctx = AnimContext {
            bones: &bones,
            to_base: &to_base,
            rest: rest(&bones),
        };
        assert!(
            matches!(
                Animation::read(&Bytes::new("a.mdl", &blob.0), at, &ctx),
                Err(RigLoadError::Malformed { .. })
            ),
            "animation blocks"
        );
    }

    #[test]
    fn movements_carry_their_root_motion() {
        let bones = [bone([0.; 3], [0.; 3])];
        let spec = AnimSpec {
            movements: vec![(9, 1, [100., 120., 0., 40., 0., 0., 40., 0., 0.])],
            ..AnimSpec::plain("run", 10, Vec::new())
        };
        let anim = read(spec, &bones, &[Some(0)], rest(&bones)).unwrap();
        let m = anim.movements()[0];
        assert_eq!(
            (m.end_frame(), m.motion_flags(), m.v0(), m.v1()),
            (9, 1, 100., 120.)
        );
        assert_eq!((m.vector(), m.position()), ([40., 0., 0.], [40., 0., 0.]));
    }
}
