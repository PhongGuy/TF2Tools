//! One studio animation: its descriptor and its per-bone tracks, sampled into bone poses.

use std::ops::Range;
use std::sync::Arc;

use crate::demo_view::studio::bytes::Bytes;
use crate::demo_view::studio::types::STUDIO_LOOPING;
use crate::demo_view::studio::values::{
    blend, channel_value, euler_quat, quat48, quat64, read_channel, vec48,
};
use crate::demo_view::studio::{
    MAX_CHANNEL_WORDS, MAX_FRAMES, MAX_MOVEMENTS, MAX_SECTIONS, MAX_TABLE, MAX_TRACKS,
};
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
/// `STUDIO_DELTA`: the animation layers on another pose instead of replacing it.
const STUDIO_DELTA: i32 = 0x4;
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

/// Track bit: the rotation is four floats in `fixed` at `rot[0]`.
const FIXED_ROT: u8 = 0x01;
/// Track bit: the rotation is three Euler channels in `words`.
const EULER_ROT: u8 = 0x02;
/// Track bit: the position is three floats in `fixed` at `pos[0]`.
const FIXED_POS: u8 = 0x04;
/// Track bit: the position is three channels in `words`.
const CHANNEL_POS: u8 = 0x08;
/// Track bit: values are deltas, so the file bone is never added and absent parts are identity.
const DELTA_TRACK: u8 = 0x10;
/// A pool slot a track leaves empty; as a channel it reads 0.
const NONE: u32 = u32::MAX;

/// One bone's track for one section, already moved onto its base bone. Its slots index the
/// animation's pools, so the many tracks of a class stay small.
#[derive(Debug, Clone, Copy)]
struct Track {
    base: u8,
    /// The file bone, an index into the animation's `bones`.
    bone: u8,
    flags: u8,
    rot: [u32; 3],
    pos: [u32; 3],
}

/// The value at `frame` of the channel at `off` in `words`; `NONE` or an offset past the pool
/// reads 0, as the engine's null value pointer does.
fn channel_at(words: &[u16], off: u32, frame: usize) -> i16 {
    usize::try_from(off)
        .ok()
        .and_then(|o| words.get(o..))
        .map_or(0, |w| channel_value(w, frame))
}

fn channels_at(words: &[u16], offs: [u32; 3], frame: usize) -> [i16; 3] {
    offs.map(|off| channel_at(words, off, frame))
}

/// The `N` floats at `at` in `fixed`, or `None` past the pool.
fn fixed_at<const N: usize>(fixed: &[f32], at: u32) -> Option<[f32; N]> {
    let at = usize::try_from(at).ok()?;
    fixed.get(at..at.checked_add(N)?)?.try_into().ok()
}

/// How much one class load may copy in all, per pool; going past any of them is `TooLarge`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AnimCaps {
    pub(crate) words: usize,
    pub(crate) tracks: usize,
    pub(crate) sections: usize,
    pub(crate) movements: usize,
}

impl AnimCaps {
    /// The caps a real class load runs under.
    pub(crate) const FULL: AnimCaps = AnimCaps {
        words: MAX_CHANNEL_WORDS,
        tracks: MAX_TRACKS,
        sections: MAX_SECTIONS,
        movements: MAX_MOVEMENTS,
    };
}

/// What every animation of one class load shares: the bones of the file being read, what the
/// whole load may still copy, and scratch pools each animation is copied out of at its exact
/// size.
pub(crate) struct AnimLoad {
    pub(crate) bones: Arc<[FileBone]>,
    left: AnimCaps,
    sections: Vec<Range<u32>>,
    tracks: Vec<Track>,
    words: Vec<u16>,
    fixed: Vec<f32>,
}

impl AnimLoad {
    /// A load reading `bones`' file that may copy at most `caps` in all.
    pub(crate) fn new(bones: Arc<[FileBone]>, caps: AnimCaps) -> AnimLoad {
        AnimLoad {
            bones,
            left: caps,
            sections: Vec::new(),
            tracks: Vec::new(),
            words: Vec::new(),
            fixed: Vec::new(),
        }
    }
}

/// Takes `n` from the budget `left`; a budget too small is `TooLarge`.
fn spend(bytes: &Bytes<'_>, left: &mut usize, n: usize) -> Result<(), RigLoadError> {
    *left = left.checked_sub(n).ok_or_else(|| bytes.too_large())?;
    Ok(())
}

/// Ends a section of `load`'s tracks, spending one from its section budget.
fn push_section(
    bytes: &Bytes<'_>,
    load: &mut AnimLoad,
    section: Range<u32>,
) -> Result<(), RigLoadError> {
    spend(bytes, &mut load.left.sections, 1)?;
    load.sections.push(section);
    Ok(())
}

/// A pool length as a slot index; a pool past `u32` is `TooLarge`.
fn slot(bytes: &Bytes<'_>, len: usize) -> Result<u32, RigLoadError> {
    u32::try_from(len)
        .ok()
        .filter(|&n| n != NONE)
        .ok_or_else(|| bytes.too_large())
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

/// One animation, its tracks kept compressed per section in flat pools.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Animation {
    name: String,
    fps: f32,
    flags: i32,
    frames: usize,
    section_frames: usize,
    /// Per section, its range of `tracks`.
    sections: Box<[Range<u32>]>,
    tracks: Box<[Track]>,
    /// Every channel's RLE words, each track holding offsets into it.
    words: Box<[u16]>,
    /// Fixed rotations and positions.
    fixed: Box<[f32]>,
    /// The file's bones, shared by every animation of the file.
    bones: Arc<[FileBone]>,
    movements: Vec<Movement>,
    rest: Arc<[BonePose]>,
}

impl Animation {
    /// Reads the descriptor at `at` with a load of its own, under [`AnimCaps::FULL`].
    #[cfg(test)]
    pub(crate) fn read(
        bytes: &Bytes<'_>,
        at: usize,
        ctx: &AnimContext<'_>,
    ) -> Result<Animation, RigLoadError> {
        let mut load = AnimLoad::new(Arc::from(ctx.bones), AnimCaps::FULL);
        Animation::read_in(bytes, at, ctx, &mut load)
    }

    /// Reads the 100-byte descriptor at `at`, its movements and every section's track block
    /// through `load`'s pools; `load.bones` must be the bones `ctx` names.
    pub(crate) fn read_in(
        bytes: &Bytes<'_>,
        at: usize,
        ctx: &AnimContext<'_>,
        load: &mut AnimLoad,
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
        let movements = read_movements(bytes, at, load)?;
        let section_frames = bytes.i32(at + 84)?;
        let section_frames = usize::try_from(section_frames).map_err(|_| {
            bytes.malformed(format!(
                "animation {name} has {section_frames} section frames"
            ))
        })?;
        load.sections.clear();
        load.tracks.clear();
        load.words.clear();
        load.fixed.clear();
        if section_frames == 0 {
            let block = bytes.rel(at, bytes.i32(at + 56)?)?;
            read_chain(bytes, block, frames, ctx, load)?;
        } else {
            read_sections(bytes, at, frames, section_frames, ctx, load)?;
        }
        Ok(Animation {
            name,
            fps,
            flags,
            frames,
            section_frames,
            sections: load.sections.as_slice().into(),
            tracks: load.tracks.as_slice().into(),
            words: load.words.as_slice().into(),
            fixed: load.fixed.as_slice().into(),
            bones: Arc::clone(&load.bones),
            movements,
            rest: Arc::clone(&ctx.rest),
        })
    }

    /// The animation's name in its file.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Frames per second.
    pub fn fps(&self) -> f32 {
        self.fps
    }

    /// How many frames it holds.
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// The raw studio animation flags.
    pub fn flags(&self) -> i32 {
        self.flags
    }

    /// Whether it loops (STUDIO_LOOPING).
    pub fn is_looping(&self) -> bool {
        self.flags & STUDIO_LOOPING != 0
    }

    /// Whether it is a delta layered on another pose (STUDIO_DELTA).
    pub fn is_delta(&self) -> bool {
        self.flags & STUDIO_DELTA != 0
    }

    /// The root motion pieces it carries, in frame order.
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
        for track in self.section_tracks(section) {
            let base = usize::from(track.base);
            if let Some(bone) = self.bones.get(usize::from(track.bone))
                && base < reach
            {
                out[base] = self.pose(track, bone, local, s);
            }
        }
    }

    /// The tracks of `section`, none for a section past the table.
    fn section_tracks(&self, section: usize) -> &[Track] {
        self.sections
            .get(section)
            .and_then(|r| self.tracks.get(r.start as usize..r.end as usize))
            .unwrap_or(&[])
    }

    /// `track`'s pose at section-local `frame`, `s` of the way to the next frame.
    fn pose(&self, track: &Track, bone: &FileBone, frame: usize, s: f32) -> BonePose {
        let delta = track.flags & DELTA_TRACK != 0;
        let quat = if track.flags & FIXED_ROT != 0 {
            fixed_at(&self.fixed, track.rot[0]).unwrap_or(BonePose::IDENTITY.quat)
        } else if track.flags & EULER_ROT != 0 {
            let base = if delta { [0.; 3] } else { bone.rot };
            let angles = |f| {
                let v = channels_at(&self.words, track.rot, f);
                std::array::from_fn(|j| f32::from(v[j]) * bone.rot_scale[j] + base[j])
            };
            // Like the engine, blend two whole quaternions rather than lerp the angles.
            let a1: [f32; 3] = angles(frame);
            let a2 = if s > 0. { angles(frame + 1) } else { a1 };
            if a1 == a2 {
                euler_quat(a1)
            } else {
                blend(euler_quat(a1), euler_quat(a2), s)
            }
        } else if delta {
            BonePose::IDENTITY.quat
        } else {
            bone.quat
        };
        let pos = if track.flags & FIXED_POS != 0 {
            fixed_at(&self.fixed, track.pos[0]).unwrap_or(BonePose::IDENTITY.pos)
        } else if track.flags & CHANNEL_POS != 0 {
            let v1 = channels_at(&self.words, track.pos, frame);
            // The next frame only matters with a weight, and may lie past the channel.
            let v2 = if s > 0. {
                channels_at(&self.words, track.pos, frame + 1)
            } else {
                v1
            };
            std::array::from_fn(|j| {
                let v = f32::from(v1[j]) * (1. - s) + f32::from(v2[j]) * s;
                v * bone.pos_scale[j] + if delta { 0. } else { bone.pos[j] }
            })
        } else if delta {
            BonePose::IDENTITY.pos
        } else {
            bone.pos
        };
        BonePose::new(pos, quat)
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
            .section_tracks(section)
            .iter()
            .find(|t| usize::from(t.base) == base_bone)?;
        let bone = self.bones.get(usize::from(track.bone))?;
        let rot = if track.flags & FIXED_ROT != 0 {
            RawRot::Fixed(fixed_at(&self.fixed, track.rot[0])?)
        } else if track.flags & EULER_ROT != 0 {
            RawRot::Euler {
                values: channels_at(&self.words, track.rot, local),
                scale: bone.rot_scale,
            }
        } else {
            RawRot::Default
        };
        let pos = if track.flags & FIXED_POS != 0 {
            RawPos::Fixed(fixed_at(&self.fixed, track.pos[0])?)
        } else if track.flags & CHANNEL_POS != 0 {
            RawPos::Channels {
                values: channels_at(&self.words, track.pos, local),
                scale: bone.pos_scale,
            }
        } else {
            RawPos::Default
        };
        Some(RawSample { rot, pos })
    }

    /// How many RLE channel words it copied.
    #[cfg(test)]
    pub(crate) fn channel_words(&self) -> usize {
        self.words.len()
    }

    /// How many tracks it copied, across all its sections.
    #[cfg(test)]
    pub(crate) fn track_count(&self) -> usize {
        self.tracks.len()
    }

    /// How many sections it copied.
    #[cfg(test)]
    pub(crate) fn section_count(&self) -> usize {
        self.sections.len()
    }

    /// How many root-motion movements it copied.
    #[cfg(test)]
    pub(crate) fn movement_count(&self) -> usize {
        self.movements.len()
    }

    /// Bytes it holds on the heap; the file's bones and rest pose are shared, so not counted.
    pub(crate) fn heap_bytes(&self) -> usize {
        self.name.capacity()
            + self.movements.capacity() * size_of::<Movement>()
            + self.sections.len() * size_of::<Range<u32>>()
            + self.tracks.len() * size_of::<Track>()
            + self.words.len() * size_of::<u16>()
            + self.fixed.len() * size_of::<f32>()
    }
}

/// The descriptor's root-motion blocks.
fn read_movements(
    bytes: &Bytes<'_>,
    at: usize,
    load: &mut AnimLoad,
) -> Result<Vec<Movement>, RigLoadError> {
    let table = bytes.table(
        at,
        bytes.i32(at + 24)?,
        bytes.i32(at + 20)?,
        MOVEMENT_BYTES,
        MAX_TABLE,
    )?;
    // Descriptors may alias one table, so the count comes out of the load's shared budget.
    spend(bytes, &mut load.left.movements, table.count)?;
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
    load: &mut AnimLoad,
) -> Result<(), RigLoadError> {
    let count = frames / sf + 2;
    let table = bytes.table(at, bytes.i32(at + 80)?, count as i32, 8, MAX_TABLE)?;
    let last = frames - 1;
    for k in 0..count {
        let covers = if k + 1 == count {
            usize::from(frames > sf)
        } else if k * sf > last {
            0
        } else {
            sf.min(last - k * sf) + 1
        };
        // Sections no frame reads hold nothing worth trusting, so they stay unread.
        if covers == 0 {
            let end = slot(bytes, load.tracks.len())?;
            push_section(bytes, load, end..end)?;
            continue;
        }
        let entry = table.at(k);
        if bytes.i32(entry)? != 0 {
            return Err(bytes.malformed(format!("section {k} lives in an animation block")));
        }
        let block = bytes.rel(at, bytes.i32(entry + 4)?)?;
        read_chain(bytes, block, covers, ctx, load)?;
    }
    Ok(())
}

/// Follows one block's chain of track headers from `at` until bone 255 or a zero link, adding
/// one section of tracks; each channel copies the `frames` the block covers.
fn read_chain(
    bytes: &Bytes<'_>,
    mut at: usize,
    frames: usize,
    ctx: &AnimContext<'_>,
    load: &mut AnimLoad,
) -> Result<(), RigLoadError> {
    let start = slot(bytes, load.tracks.len())?;
    let mut links = 0;
    loop {
        if links == MAX_LINKS {
            return Err(bytes.malformed(format!("a track chain links more than {MAX_LINKS} times")));
        }
        links += 1;
        let bone = bytes.u8(at)?;
        if bone == CHAIN_END {
            break;
        }
        let flags = bytes.u8(at + 1)?;
        let next = bytes.i16(at + 2)?;
        if next < 0 {
            return Err(bytes.malformed(format!("a track at {at} links backwards")));
        }
        if ctx.bones.get(usize::from(bone)).is_none() {
            return Err(bytes.malformed(format!(
                "the track at {at} is on bone {bone}, past the file's bones"
            )));
        }
        // A track on a bone the base model lacks plays no part.
        if let Some(base) = ctx.to_base.get(usize::from(bone)).copied().flatten() {
            // A track adds at most seven fixed floats, so this cap bounds the fixed pool too.
            spend(bytes, &mut load.left.tracks, 1)?;
            let track = read_track(bytes, at + 4, flags, bone, base, frames, load)?;
            load.tracks.push(track);
        }
        if next == 0 {
            break;
        }
        at = bytes.rel(at, i32::from(next))?;
    }
    let end = slot(bytes, load.tracks.len())?;
    push_section(bytes, load, start..end)
}

/// The track data at `data`: fixed values decode into `load.fixed` now, channels are copied
/// compressed into `load.words`.
fn read_track(
    bytes: &Bytes<'_>,
    data: usize,
    flags: u8,
    bone: u8,
    base: usize,
    frames: usize,
    load: &mut AnimLoad,
) -> Result<Track, RigLoadError> {
    let mut track = Track {
        base: u8::try_from(base).map_err(|_| bytes.too_large())?,
        bone,
        flags: if flags & DELTA != 0 { DELTA_TRACK } else { 0 },
        rot: [NONE; 3],
        pos: [NONE; 3],
    };
    let fixed_rot = if flags & RAW_ROT != 0 {
        Some(quat48(bytes.array(data)?))
    } else if flags & RAW_ROT2 != 0 {
        Some(quat64(bytes.array(data)?))
    } else {
        None
    };
    if let Some(q) = fixed_rot {
        track.rot[0] = push_fixed(bytes, load, &q)?;
        track.flags |= FIXED_ROT;
    } else if flags & ANIM_ROT != 0 {
        track.rot = read_channels(bytes, data, frames, load)?;
        track.flags |= EULER_ROT;
    }
    // The position sits past both rotation slots the flags name, as the engine lays it out.
    let skip = 6 * usize::from(flags & RAW_ROT != 0) + 8 * usize::from(flags & RAW_ROT2 != 0);
    if flags & RAW_POS != 0 {
        track.pos[0] = push_fixed(bytes, load, &vec48(bytes.array(data + skip)?))?;
        track.flags |= FIXED_POS;
    } else if flags & ANIM_POS != 0 {
        let ptr = data + 6 * usize::from(flags & ANIM_ROT != 0);
        track.pos = read_channels(bytes, ptr, frames, load)?;
        track.flags |= CHANNEL_POS;
    }
    Ok(track)
}

/// Appends `values` to the fixed pool and returns where they start.
fn push_fixed(bytes: &Bytes<'_>, load: &mut AnimLoad, values: &[f32]) -> Result<u32, RigLoadError> {
    let at = slot(bytes, load.fixed.len())?;
    load.fixed.extend_from_slice(values);
    Ok(at)
}

/// Copies the three channels of the value pointer at `ptr` into the word pool and returns their
/// offsets there; each u16 offset counts from the pointer and 0 means the channel reads 0.
fn read_channels(
    bytes: &Bytes<'_>,
    ptr: usize,
    frames: usize,
    load: &mut AnimLoad,
) -> Result<[u32; 3], RigLoadError> {
    let mut offs = [NONE; 3];
    for (j, off) in offs.iter_mut().enumerate() {
        let rel = bytes.u16(ptr + 2 * j)?;
        if rel != 0 {
            *off = slot(bytes, load.words.len())?;
            read_channel(
                bytes,
                ptr + usize::from(rel),
                frames,
                &mut load.words,
                &mut load.left.words,
            )?;
        }
    }
    Ok(offs)
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

    #[test]
    fn a_track_fits_in_32_bytes() {
        assert!(size_of::<Track>() <= 32, "{}", size_of::<Track>());
    }

    #[test]
    fn a_channel_offset_outside_the_pool_reads_zero() {
        let words = [0x0101, 7];
        assert_eq!(channel_at(&words, 0, 0), 7);
        assert_eq!(channel_at(&words, 5, 0), 0, "past the pool");
        assert_eq!(channel_at(&words, NONE, 0), 0, "no channel");
    }

    #[test]
    fn descriptors_aliasing_one_block_spend_the_shared_word_budget() {
        let bones = [FileBone {
            pos_scale: [1.; 3],
            ..bone([0.; 3], [0.; 3])
        }];
        let body = anim_body(None, Some([Some(rle(&[(&[1, 2, 3], 3)])), None, None]));
        let track = TrackBytes {
            bone: 0,
            flags: 0x04,
            body,
        };
        let mut blob = Blob::new();
        let specs = [
            AnimSpec::plain("a", 3, vec![track]),
            AnimSpec::plain("b", 3, Vec::new()),
        ];
        let at = write_anims(&mut blob, &specs);
        // Point b's block at a's track data, as a hostile file may.
        let rel = i32::from_le_bytes(blob.0[at + 56..at + 60].try_into().unwrap());
        blob.set_rel(at + 156, at + 100, at + rel as usize);
        let bytes = Bytes::new("a.mdl", &blob.0);
        let to_base = [Some(0)];
        let ctx = AnimContext {
            bones: &bones,
            to_base: &to_base,
            rest: rest(&bones),
        };
        let mut load = AnimLoad::new(
            Arc::from(bones.as_slice()),
            AnimCaps {
                words: 6,
                ..AnimCaps::FULL
            },
        );
        let a = Animation::read_in(&bytes, at, &ctx, &mut load).unwrap();
        assert_eq!(sample(&a, 2., 1)[0].pos()[0], 3.);
        assert!(matches!(
            Animation::read_in(&bytes, at + 100, &ctx, &mut load),
            Err(RigLoadError::TooLarge { .. })
        ));
    }

    #[test]
    fn fixed_only_tracks_aliased_across_descriptors_spend_the_shared_track_budget() {
        let bones = [bone([0.; 3], [0.; 3])];
        let mut body = q64_bytes(0, 0, 0, false).to_vec();
        body.extend_from_slice(&half_bytes([0; 3]));
        // A fixed rotation and position copy no channel words, so only the track cap stops them.
        let track = || TrackBytes {
            bone: 0,
            flags: RAW_ROT2 | RAW_POS,
            body: body.clone(),
        };
        let mut blob = Blob::new();
        let specs = [
            AnimSpec::plain("a", 3, vec![track(), track()]),
            AnimSpec::plain("b", 3, Vec::new()),
        ];
        let at = write_anims(&mut blob, &specs);
        let rel = i32::from_le_bytes(blob.0[at + 56..at + 60].try_into().unwrap());
        blob.set_rel(at + 156, at + 100, at + rel as usize);
        let bytes = Bytes::new("a.mdl", &blob.0);
        let to_base = [Some(0)];
        let ctx = AnimContext {
            bones: &bones,
            to_base: &to_base,
            rest: rest(&bones),
        };
        let mut load = AnimLoad::new(
            Arc::from(bones.as_slice()),
            AnimCaps {
                words: 0,
                tracks: 3,
                ..AnimCaps::FULL
            },
        );
        assert!(Animation::read_in(&bytes, at, &ctx, &mut load).is_ok());
        assert!(matches!(
            Animation::read_in(&bytes, at + 100, &ctx, &mut load),
            Err(RigLoadError::TooLarge { .. })
        ));
    }

    #[test]
    fn empty_sections_aliased_across_descriptors_spend_the_shared_section_budget() {
        let bones = [bone([0.; 3], [0.; 3])];
        // Three frames in one-frame sections make five sections, each an empty chain.
        let spec = |name| AnimSpec {
            section_frames: 1,
            blocks: (0..5).map(|_| Vec::new()).collect(),
            ..AnimSpec::plain(name, 3, Vec::new())
        };
        let mut blob = Blob::new();
        let at = write_anims(&mut blob, &[spec("a"), spec("b")]);
        // Every entry of a's table names one end marker; b reuses a's table, and its entries,
        // read from b's descriptor 100 bytes on, land on a second marker 100 bytes on.
        let marker = blob.push(&[255, 0, 0, 0]);
        blob.zeros(96);
        blob.push(&[255, 0, 0, 0]);
        let table = at + i32::from_le_bytes(blob.0[at + 80..at + 84].try_into().unwrap()) as usize;
        for k in 0..5 {
            blob.set_rel(table + 8 * k + 4, at, marker);
        }
        blob.set_rel(at + 180, at + 100, table);
        let bytes = Bytes::new("a.mdl", &blob.0);
        let to_base = [Some(0)];
        let ctx = AnimContext {
            bones: &bones,
            to_base: &to_base,
            rest: rest(&bones),
        };
        let mut load = AnimLoad::new(
            Arc::from(bones.as_slice()),
            AnimCaps {
                sections: 5,
                ..AnimCaps::FULL
            },
        );
        let a = Animation::read_in(&bytes, at, &ctx, &mut load).unwrap();
        assert_eq!(a.section_count(), 5);
        assert!(matches!(
            Animation::read_in(&bytes, at + 100, &ctx, &mut load),
            Err(RigLoadError::TooLarge { .. })
        ));
    }

    #[test]
    fn movements_aliased_across_descriptors_spend_the_shared_movement_budget() {
        let bones = [bone([0.; 3], [0.; 3])];
        let step = (9, 1, [0.; 9]);
        let specs = [
            AnimSpec {
                movements: vec![step, step],
                ..AnimSpec::plain("a", 10, Vec::new())
            },
            AnimSpec::plain("b", 10, Vec::new()),
        ];
        let mut blob = Blob::new();
        let at = write_anims(&mut blob, &specs);
        // b claims a's two movements by pointing its table at a's.
        let moves = at + i32::from_le_bytes(blob.0[at + 24..at + 28].try_into().unwrap()) as usize;
        blob.set_i32(at + 120, 2);
        blob.set_rel(at + 124, at + 100, moves);
        let bytes = Bytes::new("a.mdl", &blob.0);
        let to_base = [Some(0)];
        let ctx = AnimContext {
            bones: &bones,
            to_base: &to_base,
            rest: rest(&bones),
        };
        let mut load = AnimLoad::new(
            Arc::from(bones.as_slice()),
            AnimCaps {
                movements: 2,
                ..AnimCaps::FULL
            },
        );
        let a = Animation::read_in(&bytes, at, &ctx, &mut load).unwrap();
        assert_eq!(a.movement_count(), 2);
        assert!(matches!(
            Animation::read_in(&bytes, at + 100, &ctx, &mut load),
            Err(RigLoadError::TooLarge { .. })
        ));
    }
}
