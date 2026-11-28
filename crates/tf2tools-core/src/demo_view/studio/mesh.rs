//! A class model's LOD 0 mesh, read from its VVD vertices.

use crate::demo_view::studio::bytes::Bytes;
use crate::demo_view::studio::mdl::MdlFile;
use crate::demo_view::studio::{MAX_TABLE, MAX_VERTICES};
use crate::error::RigLoadError;

/// The only VVD version read; every TF2 class model is this one.
const VVD_VERSION: i32 = 4;
const VVD_HEADER_BYTES: usize = 64;
const FIXUP_BYTES: usize = 12;
const VERTEX_BYTES: usize = 48;
/// Most bones one vertex may be weighted to.
const MAX_VERTEX_BONES: u8 = 3;

/// Every LOD 0 vertex of a model, fixups applied.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Lod0 {
    pub(crate) positions: Vec<[f32; 3]>,
    pub(crate) normals: Vec<[f32; 3]>,
    pub(crate) uvs: Vec<[f32; 2]>,
    /// Unused slots are 0.
    pub(crate) bone_indices: Vec<[u8; 3]>,
    /// Unused slots are 0.
    pub(crate) bone_weights: Vec<[f32; 3]>,
}

/// Reads the LOD 0 vertices of `mdl` from its VVD bytes. The VVD must be version 4 with the
/// MDL's checksum, its fixups must add up to LOD 0, every used bone must exist in the MDL and
/// every mesh's vertex range must lie within LOD 0; otherwise `Malformed`, or `TooLarge` over
/// `MAX_VERTICES`.
pub(crate) fn read_lod0(mdl: &MdlFile, vvd: &[u8]) -> Result<Lod0, RigLoadError> {
    let name = vvd_path(&mdl.path);
    let b = Bytes::new(&name, vvd);
    if b.len() < VVD_HEADER_BYTES || vvd[0..4] != *b"IDSV" {
        return Err(b.malformed("not a vertex file header"));
    }
    let version = b.i32(4)?;
    if version != VVD_VERSION {
        return Err(b.malformed(format!("version {version} is not {VVD_VERSION}")));
    }
    let checksum = b.i32(8)?;
    if checksum != mdl.checksum {
        return Err(b.malformed(format!(
            "checksum {checksum} does not match the model's {}",
            mdl.checksum
        )));
    }
    let count = b.i32(16)?;
    let count = usize::try_from(count)
        .map_err(|_| b.malformed(format!("a negative LOD 0 vertex count {count}")))?;
    if count > MAX_VERTICES {
        return Err(b.too_large());
    }
    check_meshes(&b, mdl, count)?;

    // Without fixups LOD 0 is the head of the vertex block; with them it is every run whose
    // lod reaches 0, in table order. Runs may overlap, so the running total is capped first.
    let data = b.i32(56)?;
    let fixups = b.table(0, b.i32(52)?, b.i32(48)?, FIXUP_BYTES, MAX_TABLE)?;
    let mut runs = Vec::new();
    if fixups.count == 0 {
        runs.push((0, count));
    } else {
        let mut total = 0usize;
        for i in 0..fixups.count {
            let at = fixups.at(i);
            if b.i32(at)? < 0 {
                continue;
            }
            let (source, len) = (b.i32(at + 4)?, b.i32(at + 8)?);
            let (Ok(source), Ok(len)) = (usize::try_from(source), usize::try_from(len)) else {
                return Err(b.malformed(format!("fixup {i} has a negative field")));
            };
            total = total
                .checked_add(len)
                .filter(|&t| t <= count)
                .ok_or_else(|| b.malformed("fixups run past the LOD 0 vertex count"))?;
            runs.push((source, len));
        }
        if total != count {
            return Err(b.malformed(format!("fixups gather {total} of {count} vertices")));
        }
    }

    let mut lod0 = Lod0 {
        positions: Vec::with_capacity(count),
        normals: Vec::with_capacity(count),
        uvs: Vec::with_capacity(count),
        bone_indices: Vec::with_capacity(count),
        bone_weights: Vec::with_capacity(count),
    };
    let start = b.rel(0, data)?;
    for (source, len) in runs {
        let run = source
            .checked_mul(VERTEX_BYTES)
            .and_then(|off| start.checked_add(off))
            .filter(|&at| at <= b.len())
            .ok_or_else(|| b.malformed(format!("vertex {source} is outside the file")))?;
        let run = b.table(run, 0, len as i32, VERTEX_BYTES, MAX_VERTICES)?;
        for k in 0..run.count {
            read_vertex(&b, mdl, run.at(k), &mut lod0)?;
        }
    }
    Ok(lod0)
}

/// Appends the vertex at `at`, zeroing the bone slots it does not use.
fn read_vertex(
    b: &Bytes<'_>,
    mdl: &MdlFile,
    at: usize,
    lod0: &mut Lod0,
) -> Result<(), RigLoadError> {
    let used = b.u8(at + 15)?;
    if used > MAX_VERTEX_BONES {
        return Err(b.malformed(format!("the vertex at {at} has {used} bones")));
    }
    let raw = b.vec3(at)?;
    let ids: [u8; 3] = b.array(at + 12)?;
    let mut bones = [0u8; 3];
    let mut weights = [0f32; 3];
    for slot in 0..usize::from(used) {
        if usize::from(ids[slot]) >= mdl.bones.len() {
            return Err(b.malformed(format!("the vertex at {at} names bone {}", ids[slot])));
        }
        bones[slot] = ids[slot];
        weights[slot] = raw[slot];
    }
    let uv = [b.f32(at + 40)?, b.f32(at + 44)?];
    lod0.positions.push(b.vec3(at + 16)?);
    lod0.normals.push(b.vec3(at + 28)?);
    lod0.uvs.push(uv);
    lod0.bone_indices.push(bones);
    lod0.bone_weights.push(weights);
    Ok(())
}

/// Checks that every default model's meshes index within the `count` LOD 0 vertices, since the
/// MDL bounds neither a mesh's offset nor its length.
fn check_meshes(b: &Bytes<'_>, mdl: &MdlFile, count: usize) -> Result<(), RigLoadError> {
    for (p, part) in mdl.parts.iter().enumerate() {
        let Some(part) = part else { continue };
        for (k, mesh) in part.meshes.iter().enumerate() {
            part.model_vertex
                .checked_add(mesh.vertex_offset)
                .and_then(|v| v.checked_add(mesh.vertex_count))
                .filter(|&end| end <= count)
                .ok_or_else(|| {
                    b.malformed(format!("body part {p} mesh {k} runs past {count} vertices"))
                })?;
        }
    }
    Ok(())
}

/// The VVD beside an MDL path.
fn vvd_path(mdl: &str) -> String {
    format!("{}.vvd", mdl.strip_suffix(".mdl").unwrap_or(mdl))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo_view::studio::fixture::{BoneSpec, MdlSpec, MeshSpec, VertSpec, vvd_bytes};

    fn mesh(material: i32, vertex_offset: i32, vertex_count: i32, material_type: i32) -> MeshSpec {
        MeshSpec {
            material,
            vertex_offset,
            vertex_count,
            material_type,
        }
    }

    fn model(meshes: Vec<MeshSpec>) -> MdlFile {
        let spec = MdlSpec {
            bones: vec![
                BoneSpec::new("root", -1, [0.; 3]),
                BoneSpec::new("head", 0, [0.; 3]),
            ],
            textures: vec!["scout_red", "eyeball_l"],
            skin0: vec![0, 1],
            parts: vec![vec![(0, meshes)], Vec::new()],
            ..MdlSpec::new()
        };
        MdlFile::parse("models/player/scout.mdl", spec.bytes()).unwrap()
    }

    fn two_meshes() -> MdlFile {
        model(vec![mesh(0, 0, 3, 0), mesh(1, 3, 3, 1)])
    }

    fn six() -> Vec<VertSpec> {
        (0..6).map(|i| VertSpec::at(i as f32)).collect()
    }

    #[test]
    fn lod0_vertices_keep_their_attributes_and_zero_unused_slots() {
        let mut verts = six();
        verts[4] = VertSpec {
            bones: [1, 0, 0],
            weights: [0.25, 0.75, 0.],
            count: 2,
            ..VertSpec::at(4.)
        };
        verts[5] = VertSpec {
            bones: [1, 1, 1],
            weights: [1., 0.5, 0.5],
            count: 1,
            ..VertSpec::at(5.)
        };
        let lod0 = read_lod0(&two_meshes(), &vvd_bytes(7, 6, &verts, &[])).unwrap();
        assert_eq!(lod0.positions.len(), 6);
        assert_eq!(
            (lod0.positions[4], lod0.normals[4], lod0.uvs[4]),
            ([4., 0., 0.], [0., 0., 1.], [4., 0.])
        );
        assert_eq!(
            (lod0.bone_indices[4], lod0.bone_weights[4]),
            ([1, 0, 0], [0.25, 0.75, 0.])
        );
        assert_eq!(
            (lod0.bone_indices[5], lod0.bone_weights[5]),
            ([1, 0, 0], [1., 0., 0.]),
            "slots past numbones are zero"
        );
    }

    #[test]
    fn fixups_gather_the_lod0_vertices() {
        let file = model(vec![mesh(0, 0, 3, 0)]);
        let lod0 = read_lod0(&file, &vvd_bytes(7, 3, &six(), &[(0, 4, 2), (2, 1, 1)])).unwrap();
        let xs: Vec<f32> = lod0.positions.iter().map(|p| p[0]).collect();
        assert_eq!(xs, [4., 5., 1.]);
        let short = vvd_bytes(7, 4, &six(), &[(0, 4, 2), (2, 1, 1)]);
        assert!(
            matches!(
                read_lod0(&file, &short),
                Err(RigLoadError::Malformed { .. })
            ),
            "fixups must add up to LOD 0"
        );
    }

    #[test]
    fn bad_vertex_files_are_refused() {
        let file = two_meshes();
        let vvd = vvd_bytes(7, 6, &six(), &[]);
        let malformed =
            |vvd: &[u8]| matches!(read_lod0(&file, vvd), Err(RigLoadError::Malformed { .. }));
        assert!(malformed(&vvd_bytes(8, 6, &six(), &[])), "vvd checksum");
        assert!(malformed(&vvd[..100]), "vertex block past the end");
        let mut far_bone = six();
        far_bone[0].bones = [2, 0, 0];
        assert!(
            malformed(&vvd_bytes(7, 6, &far_bone, &[])),
            "a bone the model lacks"
        );
        let mut four = six();
        four[0].count = 4;
        assert!(
            malformed(&vvd_bytes(7, 6, &four, &[])),
            "four bones on one vertex"
        );
        let huge = vvd_bytes(7, 70_000, &six(), &[]);
        assert!(matches!(
            read_lod0(&file, &huge),
            Err(RigLoadError::TooLarge { .. })
        ));
    }

    #[test]
    fn a_mesh_past_lod0_is_refused() {
        let past = model(vec![mesh(0, 4, 3, 0)]);
        let r = read_lod0(&past, &vvd_bytes(7, 6, &six(), &[]));
        assert!(
            matches!(r, Err(RigLoadError::Malformed { .. })),
            "{:?}",
            r.err()
        );
        let huge = model(vec![mesh(0, i32::MAX, i32::MAX, 0)]);
        assert!(read_lod0(&huge, &vvd_bytes(7, 6, &six(), &[])).is_err());
    }
}
