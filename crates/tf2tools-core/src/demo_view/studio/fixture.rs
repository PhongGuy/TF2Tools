//! Byte builders for synthetic studio files, for the parser tests.

/// Bytes that grow as a file is laid out, with in-place patches of earlier fields.
pub(crate) struct Blob(pub(crate) Vec<u8>);

impl Blob {
    pub(crate) fn new() -> Blob {
        Blob(Vec::new())
    }
    /// Appends `bytes`; returns where they start.
    pub(crate) fn push(&mut self, bytes: &[u8]) -> usize {
        let at = self.0.len();
        self.0.extend_from_slice(bytes);
        at
    }
    pub(crate) fn zeros(&mut self, n: usize) -> usize {
        self.push(&vec![0; n])
    }
    pub(crate) fn cstr(&mut self, s: &str) -> usize {
        let at = self.push(s.as_bytes());
        self.0.push(0);
        at
    }
    pub(crate) fn set_i32(&mut self, at: usize, v: i32) {
        self.0[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    pub(crate) fn set_i16(&mut self, at: usize, v: i16) {
        self.0[at..at + 2].copy_from_slice(&v.to_le_bytes());
    }
    pub(crate) fn set_f32s(&mut self, at: usize, v: &[f32]) {
        for (i, x) in v.iter().enumerate() {
            self.0[at + 4 * i..at + 4 * i + 4].copy_from_slice(&x.to_le_bytes());
        }
    }
    /// Sets the i32 at `field` to `target - base`.
    pub(crate) fn set_rel(&mut self, field: usize, base: usize, target: usize) {
        self.set_i32(field, target as i32 - base as i32);
    }
}

/// RLE animation values: each run is (values, frames it covers); a run's last value repeats.
pub(crate) fn rle(runs: &[(&[i16], u8)]) -> Vec<u8> {
    let mut out = Vec::new();
    for &(values, total) in runs {
        out.push(u8::try_from(values.len()).unwrap());
        out.push(total);
        for v in values {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out
}

pub(crate) fn q64_bytes(x: u64, y: u64, z: u64, w_neg: bool) -> [u8; 8] {
    (x | y << 21 | z << 42 | u64::from(w_neg) << 63).to_le_bytes()
}

pub(crate) fn half_bytes(h: [u16; 3]) -> [u8; 6] {
    let mut out = [0u8; 6];
    for (i, v) in h.iter().enumerate() {
        out[2 * i..2 * i + 2].copy_from_slice(&v.to_le_bytes());
    }
    out
}

/// One bone's track: its header fields and the bytes after the header.
pub(crate) struct TrackBytes {
    pub(crate) bone: u8,
    pub(crate) flags: u8,
    pub(crate) body: Vec<u8>,
}

/// One animation as `write_anims` lays it out.
pub(crate) struct AnimSpec {
    pub(crate) name: &'static str,
    pub(crate) fps: f32,
    pub(crate) flags: i32,
    pub(crate) frames: i32,
    /// 0 for one block; else the blocks are its sections, the last one the separate last frame.
    pub(crate) section_frames: i32,
    /// Track chains; an empty chain is a lone bone-255 end marker.
    pub(crate) blocks: Vec<Vec<TrackBytes>>,
    /// (endframe, motionflags, [v0, v1, angle, vector, position]).
    pub(crate) movements: Vec<(i32, i32, [f32; 9])>,
}

impl AnimSpec {
    /// `frames` frames at 30 fps, not sectioned, one block of `tracks`.
    pub(crate) fn plain(name: &'static str, frames: i32, tracks: Vec<TrackBytes>) -> AnimSpec {
        AnimSpec {
            name,
            fps: 30.,
            flags: 0,
            frames,
            section_frames: 0,
            blocks: vec![tracks],
            movements: Vec::new(),
        }
    }
}

/// Writes the 100-byte descriptors of `anims` as one array, then each one's name, movements,
/// section table and track blocks; returns where the array starts.
pub(crate) fn write_anims(blob: &mut Blob, anims: &[AnimSpec]) -> usize {
    let array = blob.zeros(100 * anims.len());
    for (k, spec) in anims.iter().enumerate() {
        let desc = array + 100 * k;
        let name = blob.cstr(spec.name);
        blob.set_rel(desc + 4, desc, name);
        blob.set_f32s(desc + 8, &[spec.fps]);
        blob.set_i32(desc + 12, spec.flags);
        blob.set_i32(desc + 16, spec.frames);
        blob.set_i32(desc + 20, spec.movements.len() as i32);
        let moves = blob.zeros(44 * spec.movements.len());
        blob.set_rel(desc + 24, desc, moves);
        for (i, (end, flags, floats)) in spec.movements.iter().enumerate() {
            blob.set_i32(moves + 44 * i, *end);
            blob.set_i32(moves + 44 * i + 4, *flags);
            blob.set_f32s(moves + 44 * i + 8, floats);
        }
        let table = blob.zeros(8 * spec.blocks.len());
        let mut starts = Vec::new();
        for block in &spec.blocks {
            starts.push(blob.0.len());
            if block.is_empty() {
                blob.push(&[255, 0, 0, 0]);
            }
            for (i, track) in block.iter().enumerate() {
                let next = if i + 1 == block.len() {
                    0
                } else {
                    4 + track.body.len() as i16
                };
                blob.push(&[track.bone, track.flags]);
                blob.push(&next.to_le_bytes());
                blob.push(&track.body);
            }
        }
        for (i, start) in starts.iter().enumerate() {
            blob.set_rel(table + 8 * i + 4, desc, *start);
        }
        if let Some(first) = starts.first() {
            blob.set_rel(desc + 56, desc, *first);
        }
        if spec.section_frames > 0 {
            blob.set_rel(desc + 80, desc, table);
            blob.set_i32(desc + 84, spec.section_frames);
        }
    }
    array
}

/// The body of an ANIMROT and/or ANIMPOS track: the value pointers (rotation first), then each
/// channel's RLE bytes; a None channel gets offset 0. Offsets count from their own pointer.
pub(crate) fn anim_body(
    rot: Option<[Option<Vec<u8>>; 3]>,
    pos: Option<[Option<Vec<u8>>; 3]>,
) -> Vec<u8> {
    let ptrs: Vec<[Option<Vec<u8>>; 3]> = rot.into_iter().chain(pos).collect();
    let mut body = vec![0u8; 6 * ptrs.len()];
    for (p, chans) in ptrs.iter().enumerate() {
        for (j, chan) in chans.iter().enumerate() {
            if let Some(bytes) = chan {
                let off = u16::try_from(body.len() - 6 * p).unwrap();
                body[6 * p + 2 * j..6 * p + 2 * j + 2].copy_from_slice(&off.to_le_bytes());
                body.extend_from_slice(bytes);
            }
        }
    }
    body
}

#[derive(Clone)]
pub(crate) struct BoneSpec {
    pub(crate) name: String,
    pub(crate) parent: i32,
    pub(crate) pos: [f32; 3],
    pub(crate) quat: [f32; 4],
    pub(crate) rot: [f32; 3],
    pub(crate) pos_scale: [f32; 3],
    pub(crate) rot_scale: [f32; 3],
    pub(crate) pose_to_bone: [[f32; 4]; 3],
}

impl BoneSpec {
    /// A bone at `pos`, unrotated, scales 1, `poseToBone` identity.
    pub(crate) fn new(name: &str, parent: i32, pos: [f32; 3]) -> BoneSpec {
        BoneSpec {
            name: name.to_owned(),
            parent,
            pos,
            quat: [0., 0., 0., 1.],
            rot: [0.; 3],
            pos_scale: [1.; 3],
            rot_scale: [1.; 3],
            pose_to_bone: [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]],
        }
    }
}

/// A v48 MDL as `bytes` lays it out.
pub(crate) struct MdlSpec {
    pub(crate) version: i32,
    pub(crate) checksum: i32,
    pub(crate) bones: Vec<BoneSpec>,
    pub(crate) poses: Vec<(&'static str, f32, f32, f32)>,
    pub(crate) includes: Vec<&'static str>,
    pub(crate) anims: Vec<AnimSpec>,
}

impl MdlSpec {
    pub(crate) fn new() -> MdlSpec {
        MdlSpec {
            version: 48,
            checksum: 7,
            bones: Vec::new(),
            poses: Vec::new(),
            includes: Vec::new(),
            anims: Vec::new(),
        }
    }

    pub(crate) fn bytes(&self) -> Vec<u8> {
        let mut b = Blob::new();
        b.zeros(408);
        b.0[0..4].copy_from_slice(b"IDST");
        b.set_i32(4, self.version);
        b.set_i32(8, self.checksum);
        let bones = b.zeros(216 * self.bones.len());
        b.set_i32(156, self.bones.len() as i32);
        b.set_i32(160, bones as i32);
        for (i, bone) in self.bones.iter().enumerate() {
            let at = bones + 216 * i;
            let name = b.cstr(&bone.name);
            b.set_rel(at, at, name);
            b.set_i32(at + 4, bone.parent);
            b.set_f32s(at + 32, &bone.pos);
            b.set_f32s(at + 44, &bone.quat);
            b.set_f32s(at + 60, &bone.rot);
            b.set_f32s(at + 72, &bone.pos_scale);
            b.set_f32s(at + 84, &bone.rot_scale);
            b.set_f32s(at + 96, bone.pose_to_bone.as_flattened());
        }
        let anims = write_anims(&mut b, &self.anims);
        b.set_i32(180, self.anims.len() as i32);
        b.set_i32(184, anims as i32);
        let poses = b.zeros(20 * self.poses.len());
        b.set_i32(300, self.poses.len() as i32);
        b.set_i32(304, poses as i32);
        for (i, (name, start, end, looping)) in self.poses.iter().enumerate() {
            let at = poses + 20 * i;
            let n = b.cstr(name);
            b.set_rel(at, at, n);
            b.set_f32s(at + 8, &[*start, *end, *looping]);
        }
        let includes = b.zeros(8 * self.includes.len());
        b.set_i32(336, self.includes.len() as i32);
        b.set_i32(340, includes as i32);
        for (i, path) in self.includes.iter().enumerate() {
            let at = includes + 8 * i;
            let n = b.cstr(path);
            b.set_rel(at + 4, at, n);
        }
        let len = b.0.len() as i32;
        b.set_i32(76, len);
        b.0
    }
}
