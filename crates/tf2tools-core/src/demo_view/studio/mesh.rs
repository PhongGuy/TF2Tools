//! A class model's LOD 0 mesh: vertices from its VVD, triangles from its VTX.

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

/// The only VTX version read; every TF2 class model is this one.
const VTX_VERSION: i32 = 7;
const VTX_HEADER_BYTES: usize = 36;
const VTX_PART_BYTES: usize = 8;
const VTX_MODEL_BYTES: usize = 8;
const VTX_LOD_BYTES: usize = 12;
const VTX_MESH_BYTES: usize = 9;
const VTX_GROUP_BYTES: usize = 25;
const VTX_STRIP_BYTES: usize = 27;
const VTX_VERTEX_BYTES: usize = 9;
/// `STRIP_IS_TRILIST`, the only strip kind read.
const STRIP_IS_TRILIST: u8 = 0x1;
/// Most triangle indices one model may yield; strips and groups may alias one index table.
const MAX_INDICES: usize = 8 * MAX_VERTICES;

/// LOD 0 of the default body group: every LOD 0 vertex, and triangles grouped by material.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct RigMesh {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    bone_indices: Vec<[u8; 3]>,
    bone_weights: Vec<[f32; 3]>,
    indices: Vec<u16>,
    groups: Vec<MeshGroup>,
    materials: Vec<String>,
}

impl RigMesh {
    /// Every vertex position, in model space.
    pub fn positions(&self) -> &[[f32; 3]] {
        &self.positions
    }

    /// Every vertex normal, in model space.
    pub fn normals(&self) -> &[[f32; 3]] {
        &self.normals
    }

    /// Every vertex texture coordinate.
    pub fn uvs(&self) -> &[[f32; 2]] {
        &self.uvs
    }

    /// Each vertex's skeleton bones; unused slots are 0.
    pub fn bone_indices(&self) -> &[[u8; 3]] {
        &self.bone_indices
    }

    /// Each vertex's bone weights; unused slots are 0.
    pub fn bone_weights(&self) -> &[[f32; 3]] {
        &self.bone_weights
    }

    /// The triangle list, three vertex indices per triangle in stored winding.
    pub fn indices(&self) -> &[u16] {
        &self.indices
    }

    /// Runs of `indices()` that share a material.
    pub fn groups(&self) -> &[MeshGroup] {
        &self.groups
    }

    /// The model's texture names, which `MeshGroup::material` indexes.
    pub fn materials(&self) -> &[String] {
        &self.materials
    }

    /// How many vertices the mesh has.
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    /// How many triangles the mesh has.
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Heap bytes the mesh holds.
    pub(crate) fn heap_bytes(&self) -> usize {
        size_of_val(self.positions.as_slice())
            + size_of_val(self.normals.as_slice())
            + size_of_val(self.uvs.as_slice())
            + size_of_val(self.bone_indices.as_slice())
            + size_of_val(self.bone_weights.as_slice())
            + size_of_val(self.indices.as_slice())
            + size_of_val(self.groups.as_slice())
            + self.materials.iter().map(String::capacity).sum::<usize>()
            + size_of_val(self.materials.as_slice())
    }
}

/// A run of a mesh's triangles drawn with one material.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct MeshGroup {
    material: usize,
    first: usize,
    count: usize,
    eyeball: bool,
}

impl MeshGroup {
    /// The index of its texture in `RigMesh::materials`.
    pub fn material(&self) -> usize {
        self.material
    }

    /// Where its run starts in `RigMesh::indices`, a multiple of 3.
    pub fn first(&self) -> usize {
        self.first
    }

    /// How many indices its run holds, a multiple of 3.
    pub fn count(&self) -> usize {
        self.count
    }

    /// Whether it draws an eyeball, which needs the eye shader.
    pub fn eyeball(&self) -> bool {
        self.eyeball
    }
}

/// Builds LOD 0 of the default body group from `mdl`, its VVD and its VTX. The VTX must be
/// version 7 with the MDL's checksum, body part and mesh counts, and only triangle-list strips;
/// every index must reach a vertex of its mesh within LOD 0. Otherwise `Malformed`, or
/// `TooLarge` over the caps.
pub(crate) fn read_mesh(mdl: &MdlFile, vvd: &[u8], vtx: &[u8]) -> Result<RigMesh, RigLoadError> {
    let lod0 = read_lod0(mdl, vvd)?;
    let name = vtx_path(&mdl.path);
    let b = Bytes::new(&name, vtx);
    if b.len() < VTX_HEADER_BYTES {
        return Err(b.malformed("not a strip file header"));
    }
    let version = b.i32(0)?;
    if version != VTX_VERSION {
        return Err(b.malformed(format!("version {version} is not {VTX_VERSION}")));
    }
    let checksum = b.i32(16)?;
    if checksum != mdl.checksum {
        return Err(b.malformed(format!(
            "checksum {checksum} does not match the model's {}",
            mdl.checksum
        )));
    }
    let parts = b.table(0, b.i32(32)?, b.i32(28)?, VTX_PART_BYTES, MAX_TABLE)?;
    if parts.count != mdl.parts.len() {
        return Err(b.malformed(format!(
            "{} body parts where the model has {}",
            parts.count,
            mdl.parts.len()
        )));
    }

    // Triangles collect per (texture, eyeball) in first-seen order, then concatenate.
    let mut walk = Walk {
        b: &b,
        vertex_count: lod0.positions.len(),
        groups: 0,
        strips: 0,
        indices: 0,
    };
    let mut keyed: Vec<((usize, bool), Vec<u16>)> = Vec::new();
    for (p, part) in mdl.parts.iter().enumerate() {
        let Some(part) = part else { continue };
        let at = parts.at(p);
        let models = b.table(at, b.i32(at + 4)?, b.i32(at)?, VTX_MODEL_BYTES, MAX_TABLE)?;
        if models.count == 0 {
            return Err(b.malformed(format!("body part {p} has no models")));
        }
        let model = models.at(0);
        let lods = b.table(
            model,
            b.i32(model + 4)?,
            b.i32(model)?,
            VTX_LOD_BYTES,
            MAX_TABLE,
        )?;
        if lods.count == 0 {
            return Err(b.malformed(format!("body part {p} has no LODs")));
        }
        let lod = lods.at(0);
        let meshes = b.table(lod, b.i32(lod + 4)?, b.i32(lod)?, VTX_MESH_BYTES, MAX_TABLE)?;
        if meshes.count != part.meshes.len() {
            return Err(b.malformed(format!(
                "body part {p} has {} meshes where the model has {}",
                meshes.count,
                part.meshes.len()
            )));
        }
        for (k, desc) in part.meshes.iter().enumerate() {
            let texture = mdl
                .skin0
                .get(desc.material)
                .copied()
                .unwrap_or(desc.material);
            if texture >= mdl.materials.len() {
                return Err(b.malformed(format!("body part {p} mesh {k} names texture {texture}")));
            }
            let key = (texture, desc.eyeball);
            let slot = match keyed.iter().position(|(k, _)| *k == key) {
                Some(slot) => slot,
                None => {
                    keyed.push((key, Vec::new()));
                    keyed.len() - 1
                }
            };
            let base = part
                .model_vertex
                .checked_add(desc.vertex_offset)
                .ok_or_else(|| b.malformed(format!("body part {p} mesh {k} starts past LOD 0")))?;
            walk.mesh(meshes.at(k), base, desc.vertex_count, &mut keyed[slot].1)?;
        }
    }

    let mut indices = Vec::with_capacity(walk.indices);
    let mut groups = Vec::with_capacity(keyed.len());
    for ((material, eyeball), list) in keyed {
        groups.push(MeshGroup {
            material,
            first: indices.len(),
            count: list.len(),
            eyeball,
        });
        indices.extend_from_slice(&list);
    }
    Ok(RigMesh {
        positions: lod0.positions,
        normals: lod0.normals,
        uvs: lod0.uvs,
        bone_indices: lod0.bone_indices,
        bone_weights: lod0.bone_weights,
        indices,
        groups,
        materials: mdl.materials.clone(),
    })
}

/// Running totals across the whole VTX, capped because meshes, groups and strips may all
/// point at one table and so multiply the work a small file asks for.
struct Walk<'a, 'b> {
    b: &'a Bytes<'b>,
    vertex_count: usize,
    groups: usize,
    strips: usize,
    indices: usize,
}

impl Walk<'_, '_> {
    /// Appends the triangles of the VTX mesh at `at`, whose MDL mesh starts at LOD 0 vertex
    /// `base` and holds `len` vertices. Each index is checked at every hop: into the group's
    /// vertices, into the mesh, then into LOD 0.
    fn mesh(
        &mut self,
        at: usize,
        base: usize,
        len: usize,
        out: &mut Vec<u16>,
    ) -> Result<(), RigLoadError> {
        let b = self.b;
        let groups = b.table(at, b.i32(at + 4)?, b.i32(at)?, VTX_GROUP_BYTES, MAX_TABLE)?;
        self.groups = self.groups.saturating_add(groups.count);
        if self.groups > MAX_TABLE {
            return Err(b.too_large());
        }
        for g in 0..groups.count {
            let gh = groups.at(g);
            let verts = b.table(
                gh,
                b.i32(gh + 4)?,
                b.i32(gh)?,
                VTX_VERTEX_BYTES,
                MAX_VERTICES,
            )?;
            let idx = b.table(gh, b.i32(gh + 12)?, b.i32(gh + 8)?, 2, MAX_INDICES)?;
            let strips = b.table(
                gh,
                b.i32(gh + 20)?,
                b.i32(gh + 16)?,
                VTX_STRIP_BYTES,
                MAX_TABLE,
            )?;
            self.strips = self.strips.saturating_add(strips.count);
            if self.strips > MAX_TABLE {
                return Err(b.too_large());
            }
            for s in 0..strips.count {
                let sh = strips.at(s);
                if b.u8(sh + 18)? & STRIP_IS_TRILIST == 0 {
                    return Err(
                        b.malformed(format!("strip {s} of group {g} is not a triangle list"))
                    );
                }
                let (count, offset) = (b.i32(sh)?, b.i32(sh + 4)?);
                let (Ok(count), Ok(offset)) = (usize::try_from(count), usize::try_from(offset))
                else {
                    return Err(b.malformed(format!("strip {s} of group {g} has a negative field")));
                };
                if count % 3 != 0 {
                    return Err(b.malformed(format!("strip {s} of group {g} has {count} indices")));
                }
                offset
                    .checked_add(count)
                    .filter(|&end| end <= idx.count)
                    .ok_or_else(|| {
                        b.malformed(format!("strip {s} of group {g} runs past its indices"))
                    })?;
                self.indices = self.indices.saturating_add(count);
                if self.indices > MAX_INDICES {
                    return Err(b.too_large());
                }
                for i in offset..offset + count {
                    let v = usize::from(b.u16(idx.at(i))?);
                    if v >= verts.count {
                        return Err(b.malformed(format!("index {i} of group {g} names vertex {v}")));
                    }
                    let orig = usize::from(b.u16(verts.at(v) + 4)?);
                    if orig >= len {
                        return Err(b.malformed(format!(
                            "vertex {v} of group {g} names mesh vertex {orig} of {len}"
                        )));
                    }
                    let vertex = base
                        .checked_add(orig)
                        .filter(|&x| x < self.vertex_count)
                        .and_then(|x| u16::try_from(x).ok())
                        .ok_or_else(|| {
                            b.malformed(format!("vertex {v} of group {g} is past LOD 0"))
                        })?;
                    out.push(vertex);
                }
            }
        }
        Ok(())
    }
}

/// The DX9 strip file beside an MDL path.
fn vtx_path(mdl: &str) -> String {
    format!("{}.dx90.vtx", mdl.strip_suffix(".mdl").unwrap_or(mdl))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo_view::studio::fixture::{
        BoneSpec, GroupSpec, MdlSpec, MeshSpec, VertSpec, vtx_bytes, vvd_bytes,
    };

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

    fn list(verts: Vec<u16>, indices: Vec<u16>) -> Vec<GroupSpec> {
        vec![GroupSpec::list(verts, indices)]
    }

    fn vtx_two(checksum: i32) -> Vec<u8> {
        vtx_bytes(
            checksum,
            &[
                Some(vec![
                    list(vec![0, 1, 2], vec![0, 1, 2]),
                    list(vec![2, 1, 0], vec![0, 1, 2]),
                ]),
                None,
            ],
        )
    }

    #[test]
    fn lod0_triangles_index_the_default_body_group_by_material() {
        let mesh = read_mesh(&two_meshes(), &vvd_bytes(7, 6, &six(), &[]), &vtx_two(7)).unwrap();
        assert_eq!((mesh.vertex_count(), mesh.triangle_count()), (6, 2));
        assert_eq!(mesh.indices(), [0, 1, 2, 5, 4, 3]);
        let groups: Vec<_> = mesh
            .groups()
            .iter()
            .map(|g| {
                (
                    mesh.materials()[g.material()].as_str(),
                    g.first(),
                    g.count(),
                    g.eyeball(),
                )
            })
            .collect();
        assert_eq!(
            groups,
            [("scout_red", 0, 3, false), ("eyeball_l", 3, 3, true)]
        );
        assert_eq!(
            (mesh.positions()[4], mesh.bone_weights()[4]),
            ([4., 0., 0.], [1., 0., 0.]),
            "vertices come from LOD 0"
        );
    }

    #[test]
    fn bad_strip_files_are_refused() {
        let file = two_meshes();
        let vvd = vvd_bytes(7, 6, &six(), &[]);
        let malformed = |vtx: &[u8]| {
            matches!(
                read_mesh(&file, &vvd, vtx),
                Err(RigLoadError::Malformed { .. })
            )
        };
        assert!(malformed(&vtx_two(8)), "vtx checksum");
        let strip = GroupSpec {
            strips: vec![(0, 3, 2)],
            ..GroupSpec::list(vec![0, 1, 2], vec![0, 1, 2])
        };
        assert!(
            malformed(&vtx_bytes(
                7,
                &[
                    Some(vec![vec![strip], list(vec![0, 1, 2], vec![0, 1, 2])]),
                    None
                ]
            )),
            "triangle strips"
        );
        let far_index = vtx_bytes(
            7,
            &[
                Some(vec![
                    list(vec![0, 1, 2], vec![0, 1, 3]),
                    list(vec![0, 1, 2], vec![0, 1, 2]),
                ]),
                None,
            ],
        );
        assert!(malformed(&far_index), "an index past the group's vertices");
        let far_vertex = vtx_bytes(
            7,
            &[
                Some(vec![
                    list(vec![0, 1, 3], vec![0, 1, 2]),
                    list(vec![0, 1, 2], vec![0, 1, 2]),
                ]),
                None,
            ],
        );
        assert!(malformed(&far_vertex), "a vertex past its mesh");
        assert!(malformed(&vtx_bytes(7, &[None])), "body part counts differ");
    }

    #[test]
    fn strips_repeating_one_index_run_are_too_large() {
        // Every strip names the same 30,000 indices, so the file stays small while the
        // triangles it asks for pass the cap.
        let run = vec![0u16; 30_000];
        let aliased = GroupSpec {
            strips: vec![(0, 30_000, 1); 20],
            ..GroupSpec::list(vec![0, 1, 2], run)
        };
        let vtx = vtx_bytes(
            7,
            &[
                Some(vec![vec![aliased], list(vec![0, 1, 2], vec![0, 1, 2])]),
                None,
            ],
        );
        let r = read_mesh(&two_meshes(), &vvd_bytes(7, 6, &six(), &[]), &vtx);
        assert!(
            matches!(r, Err(RigLoadError::TooLarge { .. })),
            "{:?}",
            r.err()
        );
    }
}
