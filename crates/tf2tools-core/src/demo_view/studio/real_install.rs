//! Checks against the installed game; run with `--ignored` and TF2TOOLS_TF2_GAME set to the game folder.

use std::path::PathBuf;
use std::sync::Arc;

use super::ClassRig;
use super::anim::{AnimContext, Animation, BonePose, RawPos, RawRot, RawSample};
use super::mdl::MdlFile;
use super::values::{mul34, pose_matrix};
use crate::pak::{open_vpk, read_entry};
use crate::weapons::TfClass;

const STEMS: [&str; 9] = [
    "scout", "soldier", "pyro", "demo", "heavy", "engineer", "medic", "sniper", "spy",
];

fn misc_vpk() -> PathBuf {
    PathBuf::from(std::env::var("TF2TOOLS_TF2_GAME").expect("TF2TOOLS_TF2_GAME"))
        .join("tf/tf2_misc_dir.vpk")
}

/// `stem`'s base model and each include the pak holds, as (path, bytes).
fn chain(pak: &vpk::VPK, stem: &str) -> Vec<(String, Vec<u8>)> {
    let path = misc_vpk();
    let read = |name: &str| {
        pak.tree
            .get(name)
            .map(|entry| read_entry(&path, name, entry).unwrap())
    };
    let base_name = format!("models/player/{stem}.mdl");
    let base = read(&base_name).expect("base model");
    let includes = MdlFile::parse(&base_name, base.clone()).unwrap().includes;
    let mut out = vec![(base_name, base)];
    out.extend(
        includes
            .into_iter()
            .filter_map(|name| read(&name).map(|bytes| (name, bytes))),
    );
    out
}

/// The file's own animations, each read with the file as its own skeleton.
fn own_animations(file: &MdlFile) -> Vec<Animation> {
    let bones = file.file_bones();
    let to_base: Vec<Option<usize>> = (0..bones.len()).map(Some).collect();
    let rest: Arc<[BonePose]> = bones.iter().map(|b| BonePose::new(b.pos, b.quat)).collect();
    let ctx = AnimContext {
        bones: &bones,
        to_base: &to_base,
        rest,
    };
    file.anims
        .iter()
        .map(|(_, at)| Animation::read(&file.bytes(), *at, &ctx).unwrap())
        .collect()
}

fn close(a: &[f32], b: &[f32], eps: f32) -> bool {
    a.iter()
        .zip(b)
        .all(|(x, y)| (x - y).abs() <= eps * x.abs().max(1.))
}

fn normal(q: [f32; 4]) -> [f32; 4] {
    let n = q.iter().map(|v| v * v).sum::<f32>().sqrt();
    q.map(|v| v / n)
}

// vmdl 0.2.0 reads animation values as u16, so a negative value comes back 65536 higher.
fn as_vmdl(values: [i16; 3], scale: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|j| f32::from(values[j] as u16) * scale[j])
}

#[test]
#[ignore = "needs a TF2 game folder in TF2TOOLS_TF2_GAME"]
fn decoded_tracks_match_vmdl_below_frame_30() {
    let pak = open_vpk(&misc_vpk()).unwrap();
    for stem in STEMS {
        let mut compared = 0usize;
        for (name, bytes) in chain(&pak, stem) {
            let required = name == format!("models/player/{stem}.mdl")
                || name == format!("models/player/{stem}_animations.mdl");
            let file = MdlFile::parse(&name, bytes.clone()).unwrap();
            let ours = own_animations(&file);
            let Ok(Ok(theirs)) = std::panic::catch_unwind(|| vmdl::Mdl::read(&bytes)) else {
                assert!(!required, "vmdl must read {name}");
                eprintln!("{name}: vmdl cannot read it, skipped");
                continue;
            };
            assert_eq!(ours.len(), theirs.local_animations.len(), "{name}");
            for (anim, desc) in ours.iter().zip(&theirs.local_animations) {
                assert_eq!(
                    (anim.name(), anim.frames()),
                    (desc.name.as_str(), desc.frame_count),
                    "{name}"
                );
                for track in desc
                    .animations
                    .iter()
                    .filter(|t| usize::from(t.bone) < file.bones.len())
                {
                    let bone = usize::from(track.bone);
                    for frame in 0..anim.frames().min(30) {
                        let at = format!("{name} {} bone {bone} frame {frame}", anim.name());
                        let Some(RawSample { rot, pos }) = anim.raw(bone, frame) else {
                            panic!("{at}: no track")
                        };
                        let q = track.rotation(frame);
                        let got = [q.x, q.y, q.z, q.w];
                        match rot {
                            RawRot::Fixed(want) => {
                                assert!(close(&normal(want), &normal(got), 1e-5), "{at}")
                            }
                            RawRot::Euler { values, scale } => {
                                let e = as_vmdl(values, scale);
                                let w = vmdl::Quaternion::from(vmdl::RadianEuler {
                                    x: e[2],
                                    y: e[0],
                                    z: e[1],
                                });
                                assert!(close(&[w.x, w.y, w.z, w.w], &got, 1e-4), "{at}");
                            }
                            RawRot::Default => {}
                        }
                        let p = track.position(frame);
                        let got = [p.x, p.y, p.z];
                        match pos {
                            RawPos::Fixed(want) if want.iter().all(|v| v.abs() < 65_504.) => {
                                assert!(close(&want, &got, 1e-4), "{at}");
                            }
                            RawPos::Channels { values, scale } => {
                                assert!(close(&as_vmdl(values, scale), &got, 1e-3), "{at}");
                            }
                            RawPos::Fixed(_) | RawPos::Default => {}
                        }
                        compared += 1;
                    }
                }
            }
        }
        assert!(compared > 1_000, "{stem}: only {compared} samples compared");
    }
}

#[test]
#[ignore = "needs a TF2 game folder in TF2TOOLS_TF2_GAME"]
fn the_bind_pose_inverts_pose_to_bone() {
    let pak = open_vpk(&misc_vpk()).unwrap();
    for stem in STEMS {
        let (name, bytes) = chain(&pak, stem).swap_remove(0);
        let file = MdlFile::parse(&name, bytes).unwrap();
        let mut world: Vec<[[f32; 4]; 3]> = Vec::new();
        for bone in &file.bones {
            let local = pose_matrix(bone.file.pos, bone.file.quat);
            let m = bone.parent.map_or(local, |p| mul34(&world[p], &local));
            let id = mul34(&m, &bone.pose_to_bone);
            for (r, row) in id.iter().enumerate() {
                for (c, v) in row.iter().enumerate() {
                    let want = if r == c { 1. } else { 0. };
                    assert!(
                        (v - want).abs() <= 1e-3,
                        "{stem} {}: [{r}][{c}] = {v}",
                        bone.name
                    );
                }
            }
            world.push(m);
        }
    }
}

#[test]
#[ignore = "needs a TF2 game folder in TF2TOOLS_TF2_GAME"]
fn standing_with_a_melee_weapon_stands_upright() {
    let tf = misc_vpk().parent().unwrap().to_path_buf();
    for class in TfClass::ALL {
        let rig = ClassRig::load(&tf, class).unwrap();
        let stand = rig
            .sequences()
            .iter()
            .find(|s| s.activity().eq_ignore_ascii_case("ACT_MP_STAND_MELEE"))
            .unwrap();
        let anim = &rig.animations()[stand.animation(0, 0).unwrap()];
        let mut local = vec![BonePose::IDENTITY; rig.skeleton().bones().len()];
        anim.sample(0., &mut local);
        let mut world: Vec<[[f32; 4]; 3]> = Vec::new();
        for (bone, pose) in rig.skeleton().bones().iter().zip(&local) {
            let m = pose_matrix(pose.pos(), pose.quat());
            let placed = bone.parent().map_or(m, |p| mul34(&world[p], &m));
            world.push(placed);
        }
        let z = |name: &str| world[rig.skeleton().find(name).unwrap()][2][3];
        let (pelvis, head, left, right) = (
            z("bip_pelvis"),
            z("bip_head"),
            z("bip_foot_L"),
            z("bip_foot_R"),
        );
        assert!(
            (30. ..=50.).contains(&pelvis) && (55. ..=80.).contains(&head),
            "{class:?}: pelvis {pelvis}, head {head}"
        );
        assert!(
            (0. ..=12.).contains(&left) && (0. ..=12.).contains(&right),
            "{class:?}: feet {left} {right}"
        );
    }
}
