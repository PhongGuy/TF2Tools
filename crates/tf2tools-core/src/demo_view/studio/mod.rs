//! Readers for Source studio model files (MDL, VVD, VTX) and their animations, bounded by
//! caps so a hostile file fails with an error instead of a huge allocation or a panic.

/// Most bones one model may declare.
pub(crate) const MAX_BONES: usize = 128;
/// Most vertices one model may carry.
pub(crate) const MAX_VERTICES: usize = 65_536;
/// Most frames one animation may hold.
pub(crate) const MAX_FRAMES: usize = 4_096;
/// Largest studio file read from the game.
pub(crate) const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
/// Most include models one model may pull in.
pub(crate) const MAX_INCLUDES: usize = 8;
/// Most entries in any other table of a studio file.
pub(crate) const MAX_TABLE: usize = 8_192;

mod anim;
mod bytes;
mod files;
#[cfg(test)]
mod fixture;
mod mdl;
mod merge;
mod mesh;
#[cfg(test)]
mod real_install;
mod types;
mod values;

use std::path::Path;

use files::PakFiles;
use mdl::MdlFile;
use merge::merge;
use mesh::read_mesh;

pub use anim::{Animation, BonePose, Movement};
pub use mesh::{MeshGroup, RigMesh};
pub use types::{AutoLayer, PoseParam, RigBone, Sequence, Skeleton};

use crate::error::RigLoadError;
use crate::panic_mark::catching;
use crate::weapons::TfClass;

/// The file stem of `class`'s model under `models/player/`.
pub(crate) fn stem(class: TfClass) -> &'static str {
    match class {
        TfClass::Scout => "scout",
        TfClass::Soldier => "soldier",
        TfClass::Pyro => "pyro",
        TfClass::Demoman => "demo",
        TfClass::Heavy => "heavy",
        TfClass::Engineer => "engineer",
        TfClass::Medic => "medic",
        TfClass::Sniper => "sniper",
        TfClass::Spy => "spy",
    }
}

/// One class's animated model, read from the game's own files.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ClassRig {
    class: TfClass,
    skeleton: Skeleton,
    mesh: RigMesh,
    pose_params: Vec<PoseParam>,
    sequences: Vec<Sequence>,
    animations: Vec<Animation>,
}

impl ClassRig {
    /// Reads `models/player/<class>.mdl`, `.vvd`, `.dx90.vtx` and the includes it lists from
    /// `<tf_dir>/tf2_misc_dir.vpk`, never `tf/custom`. Blocking; a parser panic is Malformed.
    pub fn load(tf_dir: &Path, class: TfClass) -> Result<ClassRig, RigLoadError> {
        let files = PakFiles::open(tf_dir)?;
        load_with(class, &mut |name| files.read(name, MAX_FILE_BYTES))
    }

    /// The class this rig belongs to.
    pub fn class(&self) -> TfClass {
        self.class
    }

    /// The base model's bones.
    pub fn skeleton(&self) -> &Skeleton {
        &self.skeleton
    }

    /// The LOD 0 mesh of the default body group.
    pub fn mesh(&self) -> &RigMesh {
        &self.mesh
    }

    /// The pose parameters merged across the include chain.
    pub fn pose_params(&self) -> &[PoseParam] {
        &self.pose_params
    }

    /// The index of the pose parameter `name`, matched ASCII case-insensitively.
    pub fn pose_param(&self, name: &str) -> Option<usize> {
        self.pose_params
            .iter()
            .position(|p| p.name().eq_ignore_ascii_case(name))
    }

    /// The sequences merged across the include chain.
    pub fn sequences(&self) -> &[Sequence] {
        &self.sequences
    }

    /// The decoded animations the sequences index.
    pub fn animations(&self) -> &[Animation] {
        &self.animations
    }

    /// Bytes this rig holds on the heap: its mesh, animation channels and sequence tables.
    pub fn heap_bytes(&self) -> usize {
        let sequences: usize = self
            .sequences
            .iter()
            .map(|s| {
                s.label.capacity()
                    + s.activity.capacity()
                    + s.animations.capacity() * size_of::<usize>()
                    + s.autolayers.capacity() * size_of::<types::AutoLayer>()
                    + s.bone_weights.capacity() * size_of::<f32>()
                    + s.pose_keys
                        .as_ref()
                        .map_or(0, |[x, y]| (x.capacity() + y.capacity()) * size_of::<f32>())
            })
            .sum();
        self.mesh.heap_bytes()
            + self
                .animations
                .iter()
                .map(Animation::heap_bytes)
                .sum::<usize>()
            + self.sequences.capacity() * size_of::<Sequence>()
            + sequences
    }
}

/// A file source for the loader: the entry at a pak path, or None when the pak lacks it.
pub(crate) type ReadFile<'a> = dyn FnMut(&str) -> Result<Option<Vec<u8>>, RigLoadError> + 'a;

/// The loader over any file source: `read(name)` returns the entry or None. Runs under `panic_mark::catching`.
pub(crate) fn load_with(class: TfClass, read: &mut ReadFile<'_>) -> Result<ClassRig, RigLoadError> {
    let base = format!("models/player/{}.mdl", stem(class));
    catching(|| read_rig(class, &base, read)).unwrap_or_else(|_| {
        Err(RigLoadError::Malformed {
            file: base.clone(),
            what: "the model reader panicked".to_owned(),
        })
    })
}

/// Reads the base model's three files, then each include the pak holds, and builds the rig.
fn read_rig(class: TfClass, base: &str, read: &mut ReadFile<'_>) -> Result<ClassRig, RigLoadError> {
    let dir = format!("models/player/{}", stem(class));
    let mut need = |path: String| read(&path)?.ok_or(RigLoadError::Missing { path });
    let mdl = need(base.to_owned())?;
    let vvd = need(format!("{dir}.vvd"))?;
    let vtx = need(format!("{dir}.dx90.vtx"))?;
    let base_file = MdlFile::parse(base, mdl)?;

    // Includes of includes are not followed; a listed include the pak lacks is skipped.
    let mut files = Vec::with_capacity(1 + base_file.includes.len());
    for include in &base_file.includes {
        check_include(base, include)?;
        if let Some(data) = read(include)? {
            files.push(MdlFile::parse(include, data)?);
        }
    }
    let mesh = read_mesh(&base_file, &vvd, &vtx)?;
    files.insert(0, base_file);
    let merged = merge(&files)?;
    Ok(ClassRig {
        class,
        skeleton: merged.skeleton,
        mesh,
        pose_params: merged.pose_params,
        sequences: merged.sequences,
        animations: merged.animations,
    })
}

/// An include name is only ever looked up among pak entries, so one that cannot name a model
/// entry (empty, absolute, a drive, an empty or dot segment) is refused before any lookup.
fn check_include(base: &str, include: &str) -> Result<(), RigLoadError> {
    let entry = include.ends_with(".mdl")
        && !include.contains(':')
        && include
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..");
    if entry {
        Ok(())
    } else {
        Err(RigLoadError::Malformed {
            file: base.to_owned(),
            what: format!("the include {include:?} names no model in the game files"),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::fixture::{
        AnimSpec, BoneSpec, GroupSpec, MdlSpec, MeshSpec, SeqSpec, VertSpec, vtx_bytes, vvd_bytes,
    };
    use super::*;

    fn game() -> HashMap<String, Vec<u8>> {
        let base = MdlSpec {
            bones: vec![BoneSpec::new("root", -1, [0.; 3])],
            includes: vec![
                "models/player/scout_user_animations.mdl",
                "Models/Player/Scout_Animations.mdl",
            ],
            seqs: vec![SeqSpec {
                flags: 0x800,
                ..SeqSpec::new("run", "", None)
            }],
            textures: vec!["scout_red"],
            parts: vec![vec![(
                0,
                vec![MeshSpec {
                    material: 0,
                    vertex_offset: 0,
                    vertex_count: 3,
                    material_type: 0,
                }],
            )]],
            ..MdlSpec::new()
        };
        let anims = MdlSpec {
            bones: vec![BoneSpec::new("root", -1, [0.; 3])],
            poses: vec![("move_x", -1., 1., 0.)],
            anims: vec![AnimSpec::plain("a_run", 4, Vec::new())],
            seqs: vec![SeqSpec::new("run", "ACT_MP_RUN_PRIMARY", Some(0))],
            ..MdlSpec::new()
        };
        let verts: Vec<VertSpec> = (0..3).map(|i| VertSpec::at(i as f32)).collect();
        let vtx = vtx_bytes(
            7,
            &[Some(vec![vec![GroupSpec::list(
                vec![0, 1, 2],
                vec![0, 1, 2],
            )]])],
        );
        HashMap::from([
            ("models/player/scout.mdl".to_owned(), base.bytes()),
            (
                "models/player/scout.vvd".to_owned(),
                vvd_bytes(7, 3, &verts, &[]),
            ),
            ("models/player/scout.dx90.vtx".to_owned(), vtx),
            (
                "models/player/scout_animations.mdl".to_owned(),
                anims.bytes(),
            ),
        ])
    }

    fn load(files: &HashMap<String, Vec<u8>>) -> Result<ClassRig, RigLoadError> {
        load_with(TfClass::Scout, &mut |name: &str| {
            Ok(files.get(name).cloned())
        })
    }

    #[test]
    fn a_rig_merges_its_includes_and_skips_one_the_game_lacks() {
        let rig = load(&game()).unwrap();
        assert_eq!(rig.class(), TfClass::Scout);
        assert_eq!(rig.skeleton().bones().len(), 1);
        assert_eq!(
            (rig.sequences().len(), rig.sequences()[0].activity()),
            (1, "ACT_MP_RUN_PRIMARY")
        );
        assert_eq!(
            (rig.animations().len(), rig.pose_param("MOVE_X")),
            (1, Some(0))
        );
        assert_eq!(
            (rig.mesh().triangle_count(), rig.mesh().vertex_count()),
            (1, 3)
        );
        assert!(rig.heap_bytes() >= 3 * 12, "{}", rig.heap_bytes());
    }

    #[test]
    fn a_base_file_the_game_lacks_is_missing() {
        let mut files = game();
        files.remove("models/player/scout.dx90.vtx");
        assert!(
            matches!(load(&files), Err(RigLoadError::Missing { path }) if path == "models/player/scout.dx90.vtx")
        );
    }

    #[test]
    fn a_panic_while_reading_is_malformed() {
        let result = load_with(TfClass::Scout, &mut |_: &str| -> Result<
            Option<Vec<u8>>,
            RigLoadError,
        > { panic!("hostile bytes") });
        assert!(matches!(result, Err(RigLoadError::Malformed { .. })));
    }

    #[test]
    fn each_class_reads_its_own_files() {
        let stems: Vec<_> = TfClass::ALL.iter().map(|c| stem(*c)).collect();
        assert_eq!(
            stems,
            [
                "scout", "soldier", "pyro", "demo", "heavy", "engineer", "medic", "sniper", "spy"
            ]
        );
    }

    // Include names come from the model's bytes, so a name that cannot be a pak entry
    // (empty, absolute, or climbing out with "..") fails the load instead of being looked up.
    #[test]
    fn an_include_that_names_no_pak_entry_is_malformed() {
        for bad in [
            "",
            "/models/player/x.mdl",
            "models/../../x.mdl",
            "c:/x.mdl",
            "models/player/",
        ] {
            let mut files = game();
            let base = MdlSpec {
                bones: vec![BoneSpec::new("root", -1, [0.; 3])],
                includes: vec![bad],
                ..MdlSpec::new()
            };
            files.insert("models/player/scout.mdl".to_owned(), base.bytes());
            let mut asked = Vec::new();
            let result = load_with(TfClass::Scout, &mut |name: &str| {
                asked.push(name.to_owned());
                Ok(files.get(name).cloned())
            });
            assert!(
                matches!(result, Err(RigLoadError::Malformed { ref what, .. }) if what.contains("include")),
                "{bad:?}: {result:?}"
            );
            assert_eq!(asked.len(), 3, "{bad:?} was looked up: {asked:?}");
        }
    }
}
