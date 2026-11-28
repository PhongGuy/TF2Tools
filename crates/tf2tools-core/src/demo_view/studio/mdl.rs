//! One MDL file: its header, bones, pose parameters, include models and animation descriptors.

use crate::demo_view::studio::anim::FileBone;
use crate::demo_view::studio::bytes::Bytes;
use crate::demo_view::studio::{MAX_BONES, MAX_INCLUDES, MAX_TABLE};
use crate::error::RigLoadError;

/// The only MDL version read; every TF2 class model is this one.
const VERSION: i32 = 48;
/// Size of the v48 header.
const HEADER_BYTES: usize = 408;
const BONE_BYTES: usize = 216;
const POSE_BYTES: usize = 20;
const INCLUDE_BYTES: usize = 8;
const ANIM_BYTES: usize = 100;

/// One bone: its name, parent, the values its tracks are relative to and its inverse bind matrix.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MdlBone {
    pub(crate) name: String,
    pub(crate) parent: Option<usize>,
    pub(crate) file: FileBone,
    pub(crate) pose_to_bone: [[f32; 4]; 3],
}

/// One pose parameter's name and range; `loop_range` is 0 for one that does not wrap.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PoseDesc {
    pub(crate) name: String,
    pub(crate) start: f32,
    pub(crate) end: f32,
    pub(crate) loop_range: f32,
}

/// One parsed MDL file, which keeps its bytes so animations decode from them later.
#[derive(Debug, Clone)]
pub(crate) struct MdlFile {
    pub(crate) path: String,
    pub(crate) data: Vec<u8>,
    pub(crate) checksum: i32,
    pub(crate) bones: Vec<MdlBone>,
    pub(crate) pose_params: Vec<PoseDesc>,
    /// Include model paths, lower case with '/' separators.
    pub(crate) includes: Vec<String>,
    /// Each local animation's name and descriptor offset.
    pub(crate) anims: Vec<(String, usize)>,
}

impl MdlFile {
    /// Parses a v48 MDL's header, bones, pose parameters, includes and animation names.
    /// Errors are `Malformed` for a bad header, table or parent and `TooLarge` over a cap.
    pub(crate) fn parse(path: &str, data: Vec<u8>) -> Result<MdlFile, RigLoadError> {
        let b = Bytes::new(path, &data);
        if b.len() < HEADER_BYTES || data[0..4] != *b"IDST" {
            return Err(b.malformed("not a studio model header"));
        }
        let version = b.i32(4)?;
        if version != VERSION {
            return Err(b.malformed(format!("version {version} is not {VERSION}")));
        }
        let length = b.i32(76)?;
        if usize::try_from(length).map_or(true, |l| !(HEADER_BYTES..=b.len()).contains(&l)) {
            return Err(b.malformed(format!("the header length {length} does not fit the file")));
        }
        let checksum = b.i32(8)?;
        let bones = read_bones(&b)?;

        let poses = b.table(0, b.i32(304)?, b.i32(300)?, POSE_BYTES, MAX_TABLE)?;
        let pose_params = (0..poses.count)
            .map(|i| {
                let at = poses.at(i);
                Ok(PoseDesc {
                    name: b.cstr(at, b.i32(at)?)?,
                    start: b.f32(at + 8)?,
                    end: b.f32(at + 12)?,
                    loop_range: b.f32(at + 16)?,
                })
            })
            .collect::<Result<_, RigLoadError>>()?;

        // Include paths are matched against pak paths, which are lower case with '/' separators.
        let incs = b.table(0, b.i32(340)?, b.i32(336)?, INCLUDE_BYTES, MAX_INCLUDES)?;
        let includes = (0..incs.count)
            .map(|i| {
                let at = incs.at(i);
                Ok(b.cstr(at, b.i32(at + 4)?)?
                    .to_lowercase()
                    .replace('\\', "/"))
            })
            .collect::<Result<_, RigLoadError>>()?;

        let table = b.table(0, b.i32(184)?, b.i32(180)?, ANIM_BYTES, MAX_TABLE)?;
        let anims = (0..table.count)
            .map(|i| {
                let at = table.at(i);
                Ok((b.cstr(at, b.i32(at + 4)?)?, at))
            })
            .collect::<Result<_, RigLoadError>>()?;

        Ok(MdlFile {
            path: path.to_owned(),
            checksum,
            bones,
            pose_params,
            includes,
            anims,
            data,
        })
    }

    /// A reader over this file's bytes, for its animation descriptors.
    pub(crate) fn bytes(&self) -> Bytes<'_> {
        Bytes::new(&self.path, &self.data)
    }

    /// The bones' track bases, in bone order.
    pub(crate) fn file_bones(&self) -> Vec<FileBone> {
        self.bones.iter().map(|bone| bone.file).collect()
    }
}

/// Reads the bone table; a parent must come before its child, which also rules out cycles.
fn read_bones(b: &Bytes<'_>) -> Result<Vec<MdlBone>, RigLoadError> {
    let table = b.table(0, b.i32(160)?, b.i32(156)?, BONE_BYTES, MAX_BONES)?;
    (0..table.count)
        .map(|i| {
            let at = table.at(i);
            let name = b.cstr(at, b.i32(at)?)?;
            let parent = match b.i32(at + 4)? {
                -1 => None,
                p => Some(
                    usize::try_from(p)
                        .ok()
                        .filter(|&p| p < i)
                        .ok_or_else(|| b.malformed(format!("bone {name} has parent {p}")))?,
                ),
            };
            let mut pose_to_bone = [[0.; 4]; 3];
            for (r, row) in pose_to_bone.iter_mut().enumerate() {
                for (c, v) in row.iter_mut().enumerate() {
                    *v = b.f32(at + 96 + 16 * r + 4 * c)?;
                }
            }
            Ok(MdlBone {
                name,
                parent,
                file: FileBone {
                    pos: b.vec3(at + 32)?,
                    quat: b.quat(at + 44)?,
                    rot: b.vec3(at + 60)?,
                    pos_scale: b.vec3(at + 72)?,
                    rot_scale: b.vec3(at + 84)?,
                },
                pose_to_bone,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::demo_view::studio::anim::{AnimContext, Animation, BonePose};
    use crate::demo_view::studio::fixture::{AnimSpec, BoneSpec, MdlSpec, TrackBytes, half_bytes};

    fn spec() -> MdlSpec {
        let track = TrackBytes {
            bone: 1,
            flags: 0x01,
            body: half_bytes([0x3c00; 3]).to_vec(),
        };
        MdlSpec {
            bones: vec![
                BoneSpec::new("bip_pelvis", -1, [0., 0., 40.]),
                BoneSpec::new("bip_spine_0", 0, [0., 0., 5.]),
            ],
            poses: vec![("body_pitch", -45., 45., 360.), ("move_x", -1., 1., 0.)],
            includes: vec!["Models\\Player\\Scout_Animations.mdl"],
            anims: vec![AnimSpec::plain("@ref", 2, vec![track])],
            ..MdlSpec::new()
        }
    }

    fn parse(bytes: Vec<u8>) -> Result<MdlFile, RigLoadError> {
        MdlFile::parse("models/player/scout.mdl", bytes)
    }

    fn patched(mut bytes: Vec<u8>, at: usize, v: i32) -> Vec<u8> {
        bytes[at..at + 4].copy_from_slice(&v.to_le_bytes());
        bytes
    }

    fn is_malformed(r: Result<MdlFile, RigLoadError>) -> bool {
        matches!(r, Err(RigLoadError::Malformed { .. }))
    }

    #[test]
    fn a_model_reads_its_bones_poses_includes_and_anims() {
        let file = parse(spec().bytes()).unwrap();
        assert_eq!(file.checksum, 7);
        let names: Vec<_> = file
            .bones
            .iter()
            .map(|b| (b.name.as_str(), b.parent))
            .collect();
        assert_eq!(names, [("bip_pelvis", None), ("bip_spine_0", Some(0))]);
        assert_eq!(file.bones[1].file.pos, [0., 0., 5.]);
        assert_eq!(file.bones[1].pose_to_bone[2], [0., 0., 1., 0.]);
        let poses: Vec<_> = file
            .pose_params
            .iter()
            .map(|p| (p.name.as_str(), p.start, p.end, p.loop_range))
            .collect();
        assert_eq!(
            poses,
            [("body_pitch", -45., 45., 360.), ("move_x", -1., 1., 0.)]
        );
        assert_eq!(file.includes, ["models/player/scout_animations.mdl"]);
        assert_eq!((file.anims.len(), file.anims[0].0.as_str()), (1, "@ref"));
    }

    #[test]
    fn its_animations_read_through_their_offsets() {
        let file = parse(spec().bytes()).unwrap();
        let bones = file.file_bones();
        let rest: Arc<[BonePose]> = bones.iter().map(|b| BonePose::new(b.pos, b.quat)).collect();
        let ctx = AnimContext {
            bones: &bones,
            to_base: &[Some(0), Some(1)],
            rest,
        };
        let anim = Animation::read(&file.bytes(), file.anims[0].1, &ctx).unwrap();
        let mut out = [BonePose::IDENTITY; 2];
        anim.sample(0., &mut out);
        assert_eq!(
            (anim.frames(), out[0].pos(), out[1].pos()),
            (2, [0., 0., 40.], [1., 1., 1.])
        );
    }

    #[test]
    fn bad_headers_and_tables_are_refused() {
        let good = spec().bytes();
        assert!(is_malformed(parse(good[..100].to_vec())), "short header");
        assert!(is_malformed(parse(patched(good.clone(), 0, 0))), "not IDST");
        assert!(is_malformed(parse(patched(good.clone(), 4, 44))), "version");
        assert!(
            is_malformed(parse(patched(good.clone(), 76, good.len() as i32 + 1))),
            "length past the file"
        );
        assert!(
            is_malformed(parse(patched(good.clone(), 160, good.len() as i32))),
            "bone table past the end"
        );
        let bone_at = i32::from_le_bytes(good[160..164].try_into().unwrap()) as usize;
        assert!(
            is_malformed(parse(patched(good.clone(), bone_at + 4, 1))),
            "the root's parent comes after it"
        );
        assert!(
            is_malformed(parse(patched(good.clone(), bone_at + 216 + 4, 1))),
            "a bone is its own parent"
        );
    }

    #[test]
    fn counts_over_the_caps_are_too_large() {
        let mut bones = spec();
        bones.bones = (0..129)
            .map(|i| BoneSpec::new(&format!("b{i}"), i - 1, [0.; 3]))
            .collect();
        assert!(matches!(
            parse(bones.bytes()),
            Err(RigLoadError::TooLarge { .. })
        ));
        let mut includes = spec();
        includes.includes = vec!["a.mdl"; 9];
        assert!(matches!(
            parse(includes.bytes()),
            Err(RigLoadError::TooLarge { .. })
        ));
        assert!(
            is_malformed(parse(patched(spec().bytes(), 180, -1))),
            "a negative count"
        );
    }
}
