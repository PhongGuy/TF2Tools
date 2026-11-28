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
const TEXTURE_BYTES: usize = 64;
const PART_BYTES: usize = 16;
const MODEL_BYTES: usize = 148;
const MESH_BYTES: usize = 116;
/// Size of one VVD vertex, the unit of a model's `vertexindex`.
const VERTEX_BYTES: usize = 48;
/// A mesh `materialtype` that marks an eyeball.
const EYEBALL: i32 = 1;
const SEQ_BYTES: usize = 212;
const LAYER_BYTES: usize = 24;
/// Most grid cells, pose keys and layers across all of a file's sequences, so sequences that
/// alias one large table cannot multiply its size in memory.
const MAX_SEQ_ENTRIES: usize = 16 * MAX_TABLE;

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

/// One mesh of a model: its material slot, its vertex range within the model and whether it
/// is an eyeball.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MeshDesc {
    pub(crate) material: usize,
    pub(crate) vertex_offset: usize,
    pub(crate) vertex_count: usize,
    pub(crate) eyeball: bool,
}

/// Model 0 of a body part, the default body group; `model_vertex` is its first vertex.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PartDesc {
    pub(crate) model_vertex: usize,
    pub(crate) meshes: Vec<MeshDesc>,
}

/// One auto layer of a sequence; `seq` and `pose` are local to the file.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LayerDesc {
    pub(crate) seq: usize,
    pub(crate) pose: Option<usize>,
    pub(crate) flags: i32,
    pub(crate) start: f32,
    pub(crate) peak: f32,
    pub(crate) tail: f32,
    pub(crate) end: f32,
}

/// One sequence: its blend grid over up to two pose parameters, its auto layers and its
/// per-bone weights. Indices are local to the file.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SeqDesc {
    pub(crate) label: String,
    pub(crate) activity: String,
    pub(crate) flags: i32,
    pub(crate) actweight: i32,
    pub(crate) fade_in: f32,
    pub(crate) fade_out: f32,
    pub(crate) grid: [usize; 2],
    pub(crate) params: [Option<usize>; 2],
    pub(crate) param_start: [f32; 2],
    pub(crate) param_end: [f32; 2],
    /// The keys of `grid[0]` then of `grid[1]`, in pose units.
    pub(crate) pose_keys: Option<[Vec<f32>; 2]>,
    /// Local animation per cell, index `y * grid[0] + x`.
    pub(crate) anims: Vec<usize>,
    pub(crate) layers: Vec<LayerDesc>,
    /// One weight per file bone.
    pub(crate) weights: Vec<f32>,
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
    /// Each body part's default model; None for a part with no models.
    pub(crate) parts: Vec<Option<PartDesc>>,
    /// Texture names in file order.
    pub(crate) materials: Vec<String>,
    /// Skin family 0: material slot to texture index.
    pub(crate) skin0: Vec<usize>,
    pub(crate) seqs: Vec<SeqDesc>,
}

impl MdlFile {
    /// Parses a v48 MDL's header, bones, pose parameters, includes, animation names, textures,
    /// skin family 0, each body part's default model and the sequences.
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
            .collect::<Result<Vec<_>, RigLoadError>>()?;

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
            .collect::<Result<Vec<_>, RigLoadError>>()?;

        let (materials, skin0) = read_skins(&b)?;
        let parts = read_parts(&b)?;
        let seqs = read_seqs(&b, bones.len(), pose_params.len(), anims.len())?;

        Ok(MdlFile {
            path: path.to_owned(),
            checksum,
            bones,
            pose_params,
            includes,
            anims,
            parts,
            materials,
            skin0,
            seqs,
            data,
        })
    }

    /// A reader over this file's bytes, for its animation descriptors.
    pub(crate) fn bytes(&self) -> Bytes<'_> {
        Bytes::new(&self.path, &self.data)
    }

    /// The texture of `material` through skin family 0, or the texture of that index.
    pub(crate) fn material_name(&self, material: usize) -> Option<&str> {
        let texture = self.skin0.get(material).copied().unwrap_or(material);
        self.materials.get(texture).map(String::as_str)
    }

    /// The bones' track bases, in bone order.
    pub(crate) fn file_bones(&self) -> Vec<FileBone> {
        self.bones.iter().map(|bone| bone.file).collect()
    }
}

/// Reads the texture names and skin family 0, whose entries must name an existing texture.
fn read_skins(b: &Bytes<'_>) -> Result<(Vec<String>, Vec<usize>), RigLoadError> {
    let textures = b.table(0, b.i32(208)?, b.i32(204)?, TEXTURE_BYTES, MAX_TABLE)?;
    let materials: Vec<String> = (0..textures.count)
        .map(|i| {
            let at = textures.at(i);
            b.cstr(at, b.i32(at)?)
        })
        .collect::<Result<_, _>>()?;

    // Family 0 is the first row of a families-by-refs i16 grid; no family means no remap.
    let families = b.i32(224)?;
    if families < 0 {
        return Err(b.malformed(format!("a negative skin family count {families}")));
    }
    let refs = if families == 0 { 0 } else { b.i32(220)? };
    let row = b.table(0, b.i32(228)?, refs, 2, MAX_TABLE)?;
    let skin0 = (0..row.count)
        .map(|i| {
            let s = b.i16(row.at(i))?;
            usize::try_from(s)
                .ok()
                .filter(|&t| t < materials.len())
                .ok_or_else(|| b.malformed(format!("skin entry {i} names texture {s}")))
        })
        .collect::<Result<_, _>>()?;
    Ok((materials, skin0))
}

/// Reads model 0 of every body part with its meshes; more than `MAX_TABLE` meshes in all is
/// `TooLarge`, since parts may alias one mesh table.
fn read_parts(b: &Bytes<'_>) -> Result<Vec<Option<PartDesc>>, RigLoadError> {
    let parts = b.table(0, b.i32(236)?, b.i32(232)?, PART_BYTES, MAX_TABLE)?;
    let mut total_meshes = 0usize;
    (0..parts.count)
        .map(|p| {
            let part = parts.at(p);
            let models = b.table(
                part,
                b.i32(part + 12)?,
                b.i32(part + 4)?,
                MODEL_BYTES,
                MAX_TABLE,
            )?;
            if models.count == 0 {
                return Ok(None);
            }
            let model = models.at(0);
            let vertex_index = b.i32(model + 84)?;
            let model_vertex = usize::try_from(vertex_index)
                .ok()
                .filter(|v| v % VERTEX_BYTES == 0)
                .ok_or_else(|| {
                    b.malformed(format!("body part {p} has vertexindex {vertex_index}"))
                })?
                / VERTEX_BYTES;
            let meshes = b.table(
                model,
                b.i32(model + 76)?,
                b.i32(model + 72)?,
                MESH_BYTES,
                MAX_TABLE,
            )?;
            total_meshes += meshes.count;
            if total_meshes > MAX_TABLE {
                return Err(b.too_large());
            }
            let meshes = (0..meshes.count)
                .map(|k| {
                    let at = meshes.at(k);
                    let field = |off: usize, what: &str| -> Result<usize, RigLoadError> {
                        let v = b.i32(at + off)?;
                        usize::try_from(v).map_err(|_| {
                            b.malformed(format!("body part {p} mesh {k} has {what} {v}"))
                        })
                    };
                    Ok(MeshDesc {
                        material: field(0, "material")?,
                        vertex_count: field(8, "vertex count")?,
                        vertex_offset: field(12, "vertex offset")?,
                        eyeball: b.i32(at + 24)? == EYEBALL,
                    })
                })
                .collect::<Result<_, RigLoadError>>()?;
            Ok(Some(PartDesc {
                model_vertex,
                meshes,
            }))
        })
        .collect()
}

/// Reads the sequence table, checking every animation, pose and layer index against the file.
/// A grid axis or the running total of cells, keys and layers over its cap is `TooLarge`.
fn read_seqs(
    b: &Bytes<'_>,
    bones: usize,
    poses: usize,
    anims: usize,
) -> Result<Vec<SeqDesc>, RigLoadError> {
    let table = b.table(0, b.i32(192)?, b.i32(188)?, SEQ_BYTES, MAX_TABLE)?;
    let mut total = 0usize;
    let mut charge = |n: usize| {
        total += n;
        if total > MAX_SEQ_ENTRIES {
            Err(b.too_large())
        } else {
            Ok(())
        }
    };
    // An index must lie within `0..count`; an optional one may also be -1 for none.
    let index = |v: i32, count: usize, what: &str, seq: usize| -> Result<usize, RigLoadError> {
        usize::try_from(v)
            .ok()
            .filter(|&i| i < count)
            .ok_or_else(|| b.malformed(format!("sequence {seq} names {what} {v}")))
    };
    let optional = |v: i32, count: usize, what: &str, seq: usize| {
        if v == -1 {
            Ok(None)
        } else {
            index(v, count, what, seq).map(Some)
        }
    };
    (0..table.count)
        .map(|i| {
            let at = table.at(i);
            let label = b.cstr(at, b.i32(at + 4)?)?;
            let activity = b.cstr(at, b.i32(at + 8)?)?;

            // The grid is sized before any cell or key is read, so a huge one fails first.
            let mut grid = [0usize; 2];
            for (axis, g) in grid.iter_mut().enumerate() {
                let v = b.i32(at + 68 + 4 * axis)?;
                *g = usize::try_from(v)
                    .map_err(|_| b.malformed(format!("sequence {i} has group size {v}")))?;
                if *g > MAX_TABLE {
                    return Err(b.too_large());
                }
            }
            let cells_count = grid[0] * grid[1];
            if cells_count > MAX_TABLE {
                return Err(b.too_large());
            }
            charge(cells_count)?;
            let cells = b.table(at, b.i32(at + 60)?, cells_count as i32, 2, MAX_TABLE)?;
            let anims = (0..cells.count)
                .map(|k| index(i32::from(b.i16(cells.at(k))?), anims, "animation", i))
                .collect::<Result<_, _>>()?;

            let params = [
                optional(b.i32(at + 76)?, poses, "pose parameter", i)?,
                optional(b.i32(at + 80)?, poses, "pose parameter", i)?,
            ];

            // Both axes stride by grid[0], so axis 1's keys follow axis 0's.
            let keys_at = b.i32(at + 160)?;
            let pose_keys = if keys_at == 0 {
                None
            } else {
                let n = grid[0] + grid[1];
                charge(n)?;
                let keys = b.table(at, keys_at, n as i32, 4, 2 * MAX_TABLE)?;
                let axis = |from: usize, count: usize| {
                    (from..from + count)
                        .map(|k| b.f32(keys.at(k)))
                        .collect::<Result<Vec<_>, _>>()
                };
                Some([axis(0, grid[0])?, axis(grid[0], grid[1])?])
            };

            let layer_table = b.table(
                at,
                b.i32(at + 152)?,
                b.i32(at + 148)?,
                LAYER_BYTES,
                MAX_TABLE,
            )?;
            charge(layer_table.count)?;
            let layers = (0..layer_table.count)
                .map(|k| {
                    let l = layer_table.at(k);
                    Ok(LayerDesc {
                        seq: index(i32::from(b.i16(l)?), table.count, "layer sequence", i)?,
                        pose: optional(i32::from(b.i16(l + 2)?), poses, "layer pose", i)?,
                        flags: b.i32(l + 4)?,
                        start: b.f32(l + 8)?,
                        peak: b.f32(l + 12)?,
                        tail: b.f32(l + 16)?,
                        end: b.f32(l + 20)?,
                    })
                })
                .collect::<Result<_, RigLoadError>>()?;

            // No weight list weighs every bone fully.
            let weights_at = b.i32(at + 156)?;
            let weights = if weights_at == 0 {
                vec![1.; bones]
            } else {
                let w = b.table(at, weights_at, bones as i32, 4, MAX_BONES)?;
                (0..w.count)
                    .map(|k| b.f32(w.at(k)))
                    .collect::<Result<_, _>>()?
            };

            Ok(SeqDesc {
                label,
                activity,
                flags: b.i32(at + 12)?,
                actweight: b.i32(at + 20)?,
                fade_in: b.f32(at + 104)?,
                fade_out: b.f32(at + 108)?,
                grid,
                params,
                param_start: [b.f32(at + 84)?, b.f32(at + 88)?],
                param_end: [b.f32(at + 92)?, b.f32(at + 96)?],
                pose_keys,
                anims,
                layers,
                weights,
            })
        })
        .collect()
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
    use crate::demo_view::studio::fixture::{
        AnimSpec, BoneSpec, MdlSpec, MeshSpec, SeqSpec, TrackBytes, half_bytes,
    };

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

    fn mesh(material: i32, vertex_offset: i32, vertex_count: i32, material_type: i32) -> MeshSpec {
        MeshSpec {
            material,
            vertex_offset,
            vertex_count,
            material_type,
        }
    }
    fn parts_spec() -> MdlSpec {
        MdlSpec {
            textures: vec!["scout_red", "eyeball_l"],
            skin0: vec![1, 0],
            parts: vec![
                vec![(96, vec![mesh(1, 0, 4, 0), mesh(0, 4, 2, 1)])],
                Vec::new(),
            ],
            ..spec()
        }
    }

    #[test]
    fn a_model_reads_its_default_body_group_and_skin() {
        let file = parse(parts_spec().bytes()).unwrap();
        let Some(Some(part)) = file.parts.first() else {
            panic!("part 0 has a model")
        };
        assert_eq!(
            part.model_vertex, 2,
            "vertexindex counts bytes of 48-byte vertices"
        );
        let meshes: Vec<_> = part
            .meshes
            .iter()
            .map(|m| (m.material, m.vertex_offset, m.vertex_count, m.eyeball))
            .collect();
        assert_eq!(meshes, [(1, 0, 4, false), (0, 4, 2, true)]);
        assert!(
            file.parts[1].is_none(),
            "a part with no models draws nothing"
        );
        assert_eq!(
            (
                file.material_name(0),
                file.material_name(1),
                file.material_name(5)
            ),
            (Some("eyeball_l"), Some("scout_red"), None)
        );
    }

    #[test]
    fn bad_parts_and_skins_are_refused() {
        let mut odd = parts_spec();
        odd.parts[0][0].0 = 50;
        assert!(
            is_malformed(parse(odd.bytes())),
            "vertexindex not a whole vertex"
        );
        let mut skin = parts_spec();
        skin.skin0 = vec![2, 0];
        assert!(
            is_malformed(parse(skin.bytes())),
            "a skin names a missing texture"
        );
    }

    #[test]
    fn mesh_tables_aliased_across_parts_are_too_large() {
        // Two parts each hold 4097 meshes, within the per-table cap; part 1's table is then
        // pointed at part 0's, so the file stays small while the total passes MAX_TABLE.
        let meshes = vec![mesh(0, 0, 1, 0); MAX_TABLE / 2 + 1];
        let aliased = MdlSpec {
            textures: vec!["scout_red"],
            parts: vec![vec![(0, meshes.clone())], vec![(0, meshes)]],
            ..spec()
        };
        let bytes = aliased.bytes();
        let i32_at = |at: usize| i32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        let parts = i32_at(236) as usize;
        let model_of = |p: usize| (parts + 16 * p) as i32 + i32_at(parts + 16 * p + 12);
        let (model0, model1) = (model_of(0), model_of(1));
        let mesh0 = model0 + i32_at(model0 as usize + 76);
        let bytes = patched(bytes.clone(), model1 as usize + 76, mesh0 - model1);
        assert!(matches!(parse(bytes), Err(RigLoadError::TooLarge { .. })));
    }

    fn seq_spec() -> MdlSpec {
        let anim = |name| AnimSpec::plain(name, 1, Vec::new());
        let run = SeqSpec {
            flags: 1,
            actweight: 2,
            fade: [0.3, 0.4],
            grid: [2, 2],
            params: [1, 0],
            param_start: [-1., -1.],
            param_end: [1., 1.],
            pose_keys: vec![-1., 1., 90., -45.],
            anims: vec![0, 1, 2, 3],
            layers: vec![(1, 0, 0x4040, [0., 0.2, 0.8, 1.]), (1, -1, 0, [0.; 4])],
            weights: Some(vec![1., 0.5]),
            ..SeqSpec::new("run_PRIMARY", "ACT_MP_RUN_primary", None)
        };
        let later = SeqSpec {
            flags: 0x800,
            ..SeqSpec::new("later", "ACT_MP_STAND_MELEE", None)
        };
        MdlSpec {
            seqs: vec![run, SeqSpec::new("aim", "", Some(1)), later],
            anims: vec![anim("a"), anim("b"), anim("c"), anim("d")],
            ..spec()
        }
    }

    #[test]
    fn a_sequence_reads_its_grid_params_keys_layers_and_weights() {
        let file = parse(seq_spec().bytes()).unwrap();
        let [run, aim, later] = file.seqs.as_slice() else {
            panic!("three sequences")
        };
        assert_eq!(
            (
                run.label.as_str(),
                run.activity.as_str(),
                run.flags,
                run.actweight
            ),
            ("run_PRIMARY", "ACT_MP_RUN_primary", 1, 2)
        );
        assert_eq!(
            (run.fade_in, run.fade_out, run.grid, run.params),
            (0.3, 0.4, [2, 2], [Some(1), Some(0)])
        );
        assert_eq!((run.param_start, run.param_end), ([-1., -1.], [1., 1.]));
        assert_eq!(run.pose_keys, Some([vec![-1., 1.], vec![90., -45.]]));
        assert_eq!(run.anims, [0, 1, 2, 3]);
        let layers: Vec<_> = run
            .layers
            .iter()
            .map(|l| (l.seq, l.pose, l.flags, [l.start, l.peak, l.tail, l.end]))
            .collect();
        assert_eq!(
            layers,
            [
                (1, Some(0), 0x4040, [0., 0.2, 0.8, 1.]),
                (1, None, 0, [0.; 4])
            ]
        );
        assert_eq!(run.weights, [1., 0.5]);
        assert_eq!(
            (aim.activity.as_str(), aim.grid, aim.params),
            ("", [1, 1], [None, None])
        );
        assert_eq!(aim.anims, [1]);
        assert!(aim.pose_keys.is_none());
        assert_eq!(
            aim.weights,
            [1., 1.],
            "no weight list weighs every bone fully"
        );
        assert_eq!(
            (later.grid, later.anims.len(), later.flags),
            ([0, 0], 0, 0x800),
            "a forward declaration"
        );
    }

    #[test]
    fn bad_sequences_are_refused() {
        let with = |edit: fn(&mut SeqSpec)| {
            let mut s = seq_spec();
            edit(&mut s.seqs[0]);
            parse(s.bytes())
        };
        assert!(
            is_malformed(with(|s| s.anims[3] = 4)),
            "an animation the file lacks"
        );
        assert!(
            is_malformed(with(|s| s.anims[3] = -1)),
            "a negative animation"
        );
        assert!(
            is_malformed(with(|s| s.layers[0].0 = 3)),
            "a layer sequence the file lacks"
        );
        assert!(
            is_malformed(with(|s| s.layers[0].1 = 2)),
            "a layer pose the file lacks"
        );
        assert!(
            is_malformed(with(|s| s.params[0] = 2)),
            "a pose parameter the file lacks"
        );
        assert!(is_malformed(with(|s| s.grid = [-1, 2])), "a negative grid");
        assert!(
            matches!(
                with(|s| s.grid = [100, 100]),
                Err(RigLoadError::TooLarge { .. })
            ),
            "a grid over the cap"
        );
    }

    #[test]
    fn sequence_entries_past_the_file_total_are_too_large() {
        // Each grid is within the per-sequence cap; seventeen of them pass the file total.
        let full = SeqSpec {
            grid: [MAX_TABLE as i32, 1],
            anims: vec![0; MAX_TABLE],
            ..SeqSpec::new("full", "", None)
        };
        let mut many = seq_spec();
        many.seqs = vec![full; MAX_SEQ_ENTRIES / MAX_TABLE + 1];
        assert!(matches!(
            parse(many.bytes()),
            Err(RigLoadError::TooLarge { .. })
        ));
        many.seqs.pop();
        assert_eq!(
            parse(many.bytes()).unwrap().seqs.len(),
            16,
            "exactly the total parses"
        );
    }
}
