//! Merges a class model's include chain into one rig, following the engine's virtual model.

use std::sync::Arc;

use super::anim::{AnimContext, AnimLoad, Animation, BonePose, FileBone};
use super::mdl::MdlFile;
use super::types::{AutoLayer, PoseParam, RigBone, Sequence, Skeleton};
use super::{MAX_CHANNEL_WORDS, MAX_TABLE};
use crate::error::RigLoadError;

/// The parts of a class rig taken from the base model and its includes.
#[derive(Debug)]
pub(crate) struct Merged {
    pub(crate) skeleton: Skeleton,
    pub(crate) pose_params: Vec<PoseParam>,
    pub(crate) sequences: Vec<Sequence>,
    pub(crate) animations: Vec<Animation>,
}

/// `STUDIO_OVERRIDE`: a forward declaration that a later file's same-named sequence replaces.
const STUDIO_OVERRIDE: i32 = 0x800;

/// `files[0]` is the base model, the rest its includes in order. Pose parameters widen on a
/// duplicate name, animations keep the first of a name, and a sequence replaces an earlier one
/// of its label only when that one is a forward declaration. Every index is remapped into the
/// merged tables; an index or grid that does not fit its file is `Malformed`.
pub(crate) fn merge(files: &[MdlFile]) -> Result<Merged, RigLoadError> {
    merge_within(files, MAX_CHANNEL_WORDS)
}

/// [`merge`], copying at most `word_cap` channel words across the whole chain; more is
/// `TooLarge`, however the descriptors alias their track data.
pub(crate) fn merge_within(files: &[MdlFile], word_cap: usize) -> Result<Merged, RigLoadError> {
    let Some(base) = files.first() else {
        return Ok(Merged {
            skeleton: Skeleton { bones: Vec::new() },
            pose_params: Vec::new(),
            sequences: Vec::new(),
            animations: Vec::new(),
        });
    };
    let skeleton = Skeleton {
        bones: base
            .bones
            .iter()
            .map(|b| RigBone {
                name: b.name.clone(),
                parent: b.parent,
                bind: BonePose::new(b.file.pos, b.file.quat),
                pose_to_bone: b.pose_to_bone,
            })
            .collect(),
    };
    let mut merged = Merged {
        skeleton,
        pose_params: Vec::new(),
        sequences: Vec::new(),
        animations: Vec::new(),
    };
    let mut load = AnimLoad::new(Arc::from([]), word_cap);
    for (f, file) in files.iter().enumerate() {
        append_file(&mut merged, file, f == 0, &mut load)?;
    }
    Ok(merged)
}

/// Appends one file's pose parameters, animations and sequences. Like the engine's `numCheck`,
/// a file is compared only against entries that existed before it.
fn append_file(
    m: &mut Merged,
    file: &MdlFile,
    is_base: bool,
    load: &mut AnimLoad,
) -> Result<(), RigLoadError> {
    let b = file.bytes();
    let base_bones = &m.skeleton.bones;

    // The base maps onto itself; an include maps each bone to the first base bone of its name.
    let to_base: Vec<Option<usize>> = if is_base {
        (0..file.bones.len()).map(Some).collect()
    } else {
        let find = |name: &str| {
            base_bones
                .iter()
                .position(|x| x.name.eq_ignore_ascii_case(name))
        };
        file.bones.iter().map(|bone| find(&bone.name)).collect()
    };
    // Per base bone, the first file bone mapped to it, which supplies its rest pose and weight.
    let mut from_file = vec![None; base_bones.len()];
    for (j, target) in to_base.iter().enumerate() {
        if let Some(slot) = target.and_then(|i| from_file.get_mut(i)) {
            slot.get_or_insert(j);
        }
    }
    let rest: Arc<[BonePose]> = base_bones
        .iter()
        .zip(&from_file)
        .map(|(base, own)| {
            own.map_or(base.bind, |j| {
                BonePose::new(file.bones[j].file.pos, file.bones[j].file.quat)
            })
        })
        .collect();

    let poses_before = m.pose_params.len();
    let mut pose_map = Vec::with_capacity(file.pose_params.len());
    for p in &file.pose_params {
        let found = m.pose_params[..poses_before]
            .iter()
            .position(|q| q.name.eq_ignore_ascii_case(&p.name));
        pose_map.push(match found {
            Some(k) => {
                let q = &mut m.pose_params[k];
                (q.start, q.end) = (
                    q.start.min(q.end).min(p.start).min(p.end),
                    q.start.max(q.end).max(p.start).max(p.end),
                );
                k
            }
            None => {
                m.pose_params.push(PoseParam {
                    name: p.name.clone(),
                    start: p.start,
                    end: p.end,
                    loop_range: p.loop_range,
                });
                m.pose_params.len() - 1
            }
        });
    }

    let anims_before = m.animations.len();
    let bones: Arc<[FileBone]> = file.file_bones().into();
    load.bones = Arc::clone(&bones);
    let ctx = AnimContext {
        bones: &bones,
        to_base: &to_base,
        rest,
    };
    let mut anim_map = Vec::with_capacity(file.anims.len());
    for (name, at) in &file.anims {
        let found = m.animations[..anims_before]
            .iter()
            .position(|a| a.name().eq_ignore_ascii_case(name));
        anim_map.push(match found {
            Some(k) => k,
            None => {
                m.animations.push(Animation::read_in(&b, *at, &ctx, load)?);
                m.animations.len() - 1
            }
        });
    }

    // The whole sequence map is built before any layer is remapped, since a layer may name a
    // later sequence of the same file. `None` marks a sequence this file does not supply.
    let seqs_before = m.sequences.len();
    let mut seq_map = Vec::with_capacity(file.seqs.len());
    let mut placed = Vec::with_capacity(file.seqs.len());
    let mut appended = seqs_before;
    for s in &file.seqs {
        let found = m.sequences[..seqs_before]
            .iter()
            .position(|q| q.label.eq_ignore_ascii_case(&s.label));
        let (at, supplies) = match found {
            Some(k) => (k, m.sequences[k].flags & STUDIO_OVERRIDE != 0),
            None => {
                appended += 1;
                (appended - 1, true)
            }
        };
        seq_map.push(at);
        placed.push(supplies.then_some(at));
    }

    let local = |map: &[usize], i: usize, what: &str| {
        map.get(i)
            .copied()
            .ok_or_else(|| b.malformed(format!("{what} {i} is not in the file")))
    };
    for (s, place) in file.seqs.iter().zip(placed) {
        let Some(place) = place else { continue };
        let cells = s.grid[0].checked_mul(s.grid[1]);
        if cells != Some(s.anims.len()) || s.anims.len() > MAX_TABLE {
            return Err(b.malformed(format!(
                "sequence {} has a {}x{} grid over {} cells",
                s.label,
                s.grid[0],
                s.grid[1],
                s.anims.len()
            )));
        }
        let pose = |p: Option<usize>| p.map(|i| local(&pose_map, i, "pose parameter")).transpose();
        let sequence = Sequence {
            label: s.label.clone(),
            activity: s.activity.clone(),
            flags: s.flags,
            actweight: s.actweight,
            fade_in: s.fade_in,
            fade_out: s.fade_out,
            grid: s.grid,
            params: [pose(s.params[0])?, pose(s.params[1])?],
            param_start: s.param_start,
            param_end: s.param_end,
            pose_keys: s.pose_keys.clone(),
            animations: s
                .anims
                .iter()
                .map(|&a| local(&anim_map, a, "animation"))
                .collect::<Result<_, _>>()?,
            autolayers: s
                .layers
                .iter()
                .map(|l| {
                    Ok(AutoLayer {
                        sequence: local(&seq_map, l.seq, "layer sequence")?,
                        pose: pose(l.pose)?,
                        flags: l.flags,
                        start: l.start,
                        peak: l.peak,
                        tail: l.tail,
                        end: l.end,
                    })
                })
                .collect::<Result<_, RigLoadError>>()?,
            bone_weights: from_file
                .iter()
                .map(|own| own.and_then(|j| s.weights.get(j).copied()).unwrap_or(0.))
                .collect(),
        };
        if place < m.sequences.len() {
            m.sequences[place] = sequence;
        } else {
            m.sequences.push(sequence);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo_view::studio::anim::BonePose;
    use crate::demo_view::studio::fixture::{
        AnimSpec, BoneSpec, MdlSpec, SeqSpec, TrackBytes, anim_body, half_bytes, rle,
    };
    use crate::demo_view::studio::mdl::MdlFile;

    fn file(path: &str, spec: MdlSpec) -> MdlFile {
        MdlFile::parse(path, spec.bytes()).unwrap()
    }

    fn base() -> MdlSpec {
        MdlSpec {
            bones: vec![
                BoneSpec::new("root", -1, [0., 0., 40.]),
                BoneSpec::new("spine", 0, [0., 0., 5.]),
            ],
            poses: vec![("body_pitch", -45., 45., 360.)],
            includes: vec!["anims.mdl"],
            anims: vec![AnimSpec::plain("@ref", 1, Vec::new())],
            seqs: vec![
                SeqSpec::new("ref", "", Some(0)),
                SeqSpec {
                    flags: 0x800,
                    ..SeqSpec::new("run", "", None)
                },
            ],
            ..MdlSpec::new()
        }
    }

    /// An include with an extra bone first, a run that fills the base's forward declaration and
    /// an idle layered over the run.
    fn anims() -> MdlSpec {
        let spine_x = TrackBytes {
            bone: 2,
            flags: 0x01,
            body: half_bytes([0x3c00, 0, 0]).to_vec(),
        };
        MdlSpec {
            bones: vec![
                BoneSpec::new("extra", -1, [0.; 3]),
                BoneSpec::new("root", -1, [0., 0., 41.]),
                BoneSpec::new("SPINE", 1, [0., 0., 6.]),
            ],
            poses: vec![("body_pitch", -45., 90., 360.), ("move_x", -1., 1., 0.)],
            anims: vec![
                AnimSpec::plain("a_run", 1, vec![spine_x]),
                AnimSpec::plain("@REF", 1, Vec::new()),
                AnimSpec::plain("a_idle", 1, Vec::new()),
            ],
            seqs: vec![
                SeqSpec {
                    params: [1, -1],
                    weights: Some(vec![9., 0.75, 0.25]),
                    ..SeqSpec::new("RUN", "ACT_MP_RUN_PRIMARY", Some(0))
                },
                SeqSpec {
                    layers: vec![(0, 1, 0x40, [0.; 4])],
                    ..SeqSpec::new("idle", "ACT_MP_STAND_PRIMARY", Some(2))
                },
            ],
            ..MdlSpec::new()
        }
    }

    #[test]
    fn includes_fill_forward_declarations_and_remap_their_indices() {
        let m = merge(&[file("base.mdl", base()), file("anims.mdl", anims())]).unwrap();
        let labels: Vec<_> = m.sequences.iter().map(Sequence::label).collect();
        assert_eq!(labels, ["ref", "RUN", "idle"]);
        let names: Vec<_> = m.animations.iter().map(Animation::name).collect();
        assert_eq!(
            names,
            ["@ref", "a_run", "a_idle"],
            "the include's @REF is the base's @ref"
        );
        let run = &m.sequences[1];
        assert_eq!(
            (run.activity(), run.params()),
            ("ACT_MP_RUN_PRIMARY", [Some(1), None])
        );
        assert_eq!(run.animations(), [1]);
        assert_eq!(
            run.bone_weights(),
            [0.75, 0.25],
            "weights follow the base skeleton"
        );
        let idle = &m.sequences[2];
        assert_eq!(idle.animations(), [2]);
        let layer = &idle.autolayers()[0];
        assert_eq!(
            (layer.sequence(), layer.pose(), layer.flags()),
            (1, Some(1), 0x40)
        );
        let bones: Vec<_> = m.skeleton.bones().iter().map(RigBone::name).collect();
        assert_eq!(bones, ["root", "spine"]);
        assert_eq!(m.skeleton.find("SPINE"), Some(1));
    }

    #[test]
    fn duplicate_pose_parameters_widen_to_cover_both_ranges() {
        let m = merge(&[file("base.mdl", base()), file("anims.mdl", anims())]).unwrap();
        let poses: Vec<_> = m
            .pose_params
            .iter()
            .map(|p| (p.name(), p.start(), p.end()))
            .collect();
        assert_eq!(poses, [("body_pitch", -45., 90.), ("move_x", -1., 1.)]);
    }

    #[test]
    fn a_later_definition_never_replaces_one_that_is_not_forward_declared() {
        let again = MdlSpec {
            bones: vec![BoneSpec::new("root", -1, [0.; 3])],
            anims: vec![
                AnimSpec::plain("a_idle", 5, Vec::new()),
                AnimSpec::plain("a_new", 1, Vec::new()),
            ],
            seqs: vec![
                SeqSpec::new("IDLE", "ACT_OTHER", Some(0)),
                SeqSpec::new("new", "", Some(1)),
            ],
            ..MdlSpec::new()
        };
        let m = merge(&[
            file("base.mdl", base()),
            file("anims.mdl", anims()),
            file("again.mdl", again),
        ])
        .unwrap();
        let labels: Vec<_> = m.sequences.iter().map(Sequence::label).collect();
        assert_eq!(labels, ["ref", "RUN", "idle", "new"]);
        assert_eq!(m.sequences[2].activity(), "ACT_MP_STAND_PRIMARY");
        assert_eq!(m.animations[2].frames(), 1, "the first a_idle stays");
        assert_eq!(
            m.sequences[3].animations(),
            [3],
            "a_new follows the first three"
        );
    }

    #[test]
    fn a_layer_may_name_a_later_sequence_of_its_file() {
        let taunt = MdlSpec {
            bones: vec![BoneSpec::new("root", -1, [0.; 3])],
            anims: vec![
                AnimSpec::plain("a_taunt", 1, Vec::new()),
                AnimSpec::plain("a_wheel", 1, Vec::new()),
            ],
            seqs: vec![
                SeqSpec {
                    layers: vec![(1, -1, 0, [0.; 4])],
                    ..SeqSpec::new("taunt", "", Some(0))
                },
                SeqSpec::new("layer_wheel", "", Some(1)),
            ],
            ..MdlSpec::new()
        };
        let m = merge(&[
            file("base.mdl", base()),
            file("anims.mdl", anims()),
            file("taunt.mdl", taunt),
        ])
        .unwrap();
        let labels: Vec<_> = m.sequences.iter().map(Sequence::label).collect();
        assert_eq!(labels, ["ref", "RUN", "idle", "taunt", "layer_wheel"]);
        let layer = &m.sequences[3].autolayers()[0];
        assert_eq!(
            (layer.sequence(), layer.pose()),
            (4, None),
            "the layer points forward to layer_wheel"
        );
        assert_eq!(
            m.sequences[4].bone_weights(),
            [1., 0.],
            "no weight list: full weight on the file's bones, none on bones it lacks"
        );
    }

    #[test]
    fn untracked_bones_rest_at_their_files_bind_pose_and_absent_bones_at_the_base() {
        let more = MdlSpec {
            bones: vec![BoneSpec::new("root", -1, [0., 0., 42.])],
            anims: vec![AnimSpec::plain("a_more", 1, Vec::new())],
            ..MdlSpec::new()
        };
        let m = merge(&[
            file("base.mdl", base()),
            file("anims.mdl", anims()),
            file("more.mdl", more),
        ])
        .unwrap();
        let mut out = [BonePose::IDENTITY; 2];
        m.animations[1].sample(0., &mut out);
        assert_eq!(
            (out[0].pos(), out[1].pos()),
            ([0., 0., 41.], [1., 0., 0.]),
            "the include's root, the tracked spine"
        );
        let more_anim = m.animations.iter().find(|a| a.name() == "a_more").unwrap();
        more_anim.sample(0., &mut out);
        assert_eq!((out[0].pos(), out[1].pos()), ([0., 0., 42.], [0., 0., 5.]));
    }

    #[test]
    fn a_grid_that_does_not_match_its_cells_is_malformed() {
        let mut include = file("anims.mdl", anims());
        include.seqs[0].grid = [2, 1];
        let r = merge(&[file("base.mdl", base()), include.clone()]);
        assert!(matches!(r, Err(RigLoadError::Malformed { .. })), "{r:?}");
        include.seqs[0].grid = [usize::MAX, 2];
        let r = merge(&[file("base.mdl", base()), include]);
        assert!(
            matches!(r, Err(RigLoadError::Malformed { .. })),
            "an overflowing grid: {r:?}"
        );
    }

    #[test]
    fn channel_words_are_budgeted_across_the_chain_and_aliased_descriptors() {
        let chan = || TrackBytes {
            bone: 0,
            flags: 0x04,
            body: anim_body(None, Some([Some(rle(&[(&[1, 2, 3], 3)])), None, None])),
        };
        let base = MdlSpec {
            anims: vec![AnimSpec::plain("@ref", 3, vec![chan()])],
            ..base()
        };
        let include = MdlSpec {
            bones: vec![BoneSpec::new("root", -1, [0.; 3])],
            anims: vec![
                AnimSpec::plain("b", 3, vec![chan()]),
                AnimSpec::plain("c", 3, Vec::new()),
            ],
            ..MdlSpec::new()
        };
        // Point c's block at b's track data, so one copy in the file decodes twice.
        let mut bytes = include.bytes();
        let parsed = MdlFile::parse("anims.mdl", bytes.clone()).unwrap();
        let (b, c) = (parsed.anims[0].1, parsed.anims[1].1);
        let rel = i32::from_le_bytes(bytes[b + 56..b + 60].try_into().unwrap());
        let aliased = (b + rel as usize - c) as i32;
        bytes[c + 56..c + 60].copy_from_slice(&aliased.to_le_bytes());
        let files = [
            file("base.mdl", base),
            MdlFile::parse("anims.mdl", bytes).unwrap(),
        ];
        assert!(merge_within(&files, 12).is_ok(), "three four-word channels");
        assert!(matches!(
            merge_within(&files, 11),
            Err(RigLoadError::TooLarge { .. })
        ));
    }
}
