//! The HUD's animations: the manifest's files in order, first event of a name wins.

mod common;

use common::hud_anims::{ANIMS, MANIFEST_PATH, anim_model};
use common::hud_stock::stock;
use tf2tools_core::{HudAnimations, HudFiles, HudModel, Platform, Scene};

fn layered(top: &[(&str, &str)], with_stock_anims: bool) -> HudFiles {
    let top = top
        .iter()
        .map(|(p, t)| ((*p).to_owned(), t.as_bytes().to_vec()))
        .collect();
    let mut base: Vec<(String, Vec<u8>)> = stock()
        .into_iter()
        .map(|(p, t)| (p, t.into_bytes()))
        .collect();
    if with_stock_anims {
        base.push((
            MANIFEST_PATH.to_owned(),
            common::hud_anims::MANIFEST.as_bytes().to_vec(),
        ));
        base.push((
            "scripts/hudanimations_tf.txt".to_owned(),
            ANIMS.as_bytes().to_vec(),
        ));
    }
    HudFiles::from_memory(vec![top, base])
}

#[test]
fn the_model_reads_the_manifest_s_files() {
    let model = anim_model(&[]);
    let a = model.animations();
    assert!(a.event("hudhealthdyingpulse").is_some());
    assert_eq!(a.events().len(), 7);
    assert_eq!(
        a.event("HudLowAmmoPulse").unwrap().file(),
        "scripts/hudanimations_tf.txt"
    );
    assert!(
        a.problems()
            .iter()
            .any(|p| p.contains("scripts/hudanimations.txt"))
    );
}

#[test]
fn a_file_listed_first_wins_and_events_know_their_layer() {
    let manifest = "hudanimations_manifest { file scripts/hudanimations_mine.txt file scripts/hudanimations_tf.txt }";
    let mine = "event HudLowAmmoPulse { RunEvent Mine 0 }";
    let files = layered(
        &[
            (MANIFEST_PATH, manifest),
            ("scripts/hudanimations_mine.txt", mine),
        ],
        true,
    );
    let a = HudAnimations::load(&files, Platform::Linux);
    let low = a.event("HudLowAmmoPulse").unwrap();
    assert_eq!(low.body(), "RunEvent \"Mine\" 0");
    assert_eq!(low.layer(), Some(0));
    assert_eq!(a.event("HudMedicCharged").unwrap().layer(), Some(1));
}

#[test]
fn naming_finds_the_events_that_animate_a_part() {
    let model = anim_model(&[]);
    let names: Vec<&str> = model
        .animations()
        .naming("playerstatushealthvalue")
        .iter()
        .map(|e| e.name())
        .collect();
    assert_eq!(names, ["HudHealthDyingPulse"]);
}

#[test]
fn no_manifest_means_no_animations() {
    let model = HudModel::load(&common::hud_stock::stock_files(), Platform::Linux).unwrap();
    assert!(model.animations().events().is_empty());
}

#[test]
fn a_bad_file_keeps_its_good_part_and_says_so() {
    let broken = "event A { RunEvent B 0 }\nevent C { Explode }\nevent D { RunEvent E 0 }";
    let files = layered(
        &[
            (MANIFEST_PATH, "m { file scripts/broken.txt }"),
            ("scripts/broken.txt", broken),
        ],
        false,
    );
    let a = HudAnimations::load(&files, Platform::Linux);
    let names: Vec<&str> = a.events().iter().map(|e| e.name()).collect();
    assert_eq!(names, ["A", "C"]);
    assert!(
        a.problems()
            .iter()
            .any(|p| p.contains("scripts/broken.txt"))
    );
}

#[test]
fn a_manifest_reads_at_most_64_files() {
    let list: String = (0..65)
        .map(|i| format!("file scripts/a{i}.txt\n"))
        .collect();
    let manifest = format!("m {{\n{list}}}");
    let mut top: Vec<(String, String)> = vec![(MANIFEST_PATH.to_owned(), manifest)];
    for i in 0..65 {
        top.push((format!("scripts/a{i}.txt"), format!("event E{i} {{ }}")));
    }
    let top: Vec<(&str, &str)> = top.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
    let a = HudAnimations::load(&layered(&top, false), Platform::Linux);
    assert_eq!((a.events().len(), a.skipped()), (64, 1));
    assert!(a.event("E64").is_none());
}

#[test]
fn only_the_top_copy_of_a_file_is_read() {
    let mine = "event HudMedicCharged { RunEvent Top 0 }";
    let files = layered(&[("scripts/hudanimations_tf.txt", mine)], true);
    let a = HudAnimations::load(&files, Platform::Linux);
    assert_eq!(
        a.event("HudMedicCharged").unwrap().body(),
        "RunEvent \"Top\" 0"
    );
    assert!(
        a.event("HudLowAmmoPulse").is_none(),
        "the lower copy is not read"
    );
}

#[test]
fn animations_survive_a_scene_round_trip() {
    let model = anim_model(&[]);
    let back = model.with_scene(Scene::Scoreboard).with_scene(Scene::Hud);
    assert_eq!(back.animations().events().len(), 7);
}
