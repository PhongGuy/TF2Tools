//! The two animation change shapes: maker.json, replay order, validity, recipes.

use crate::common;

use common::hud_anims::{ANIMS, ANIMS_PATH, anim_model};
use std::collections::BTreeMap;
use tf2tools_core::{AnimationAppend, AnimationChange, Change, ChangeTarget, MakerFile, Recipe};

fn whole(group: u32, event: &str, body: &str) -> Change {
    Change::animation(group, AnimationChange::new(event, body))
}

fn append(group: u32, event: &str, lines: &str) -> Change {
    Change::animation_append(group, AnimationAppend::new(event, lines))
}

#[test]
fn both_shapes_round_trip_through_maker_json() {
    let changes = vec![
        whole(3, "HudHealthDyingPulse", "RunEvent X 0.5"),
        append(4, "DamagedPlayer", "RunEvent Y 0"),
    ];
    let file = MakerFile::new(BTreeMap::new(), changes.clone());
    let json: serde_json::Value = serde_json::from_slice(&file.to_json()).unwrap();
    assert_eq!(json["changes"][0]["animation"]["body"], "RunEvent X 0.5");
    assert_eq!(
        json["changes"][1]["animation_append"]["lines"],
        "RunEvent Y 0"
    );
    let (back, dropped) = MakerFile::parse(&file.to_json()).unwrap();
    assert_eq!((back.changes(), dropped), (changes.as_slice(), 0));
    assert!(
        matches!(back.changes()[1].target(), ChangeTarget::AnimationAppend(a) if a.event() == "DamagedPlayer")
    );
}

#[test]
fn wholes_apply_first_and_appends_follow_their_event() {
    let changes = [
        append(2, "HudHealthDyingPulse", "RunEvent Flash 0"),
        whole(1, "HudHealthDyingPulse", "RunEvent A 0"),
        whole(3, "hudhealthdyingpulse", "RunEvent B 0"),
        append(4, "Brand", "RunEvent C 0"),
    ];
    let edited = anim_model(&[]).with_changes(&changes);
    assert_eq!((edited.applied().len(), edited.dropped()), (4, 0));
    let a = edited.model().animations();
    let dying = a.event("HudHealthDyingPulse").unwrap();
    assert_eq!(dying.body(), "RunEvent \"B\" 0\nRunEvent \"Flash\" 0");
    assert_eq!(
        (dying.own_len(), dying.own_body()),
        (1, "RunEvent \"B\" 0".to_owned())
    );
    assert_eq!(dying.layer(), None);
    assert_eq!(a.event("Brand").unwrap().body(), "RunEvent \"C\" 0");
    assert!(
        edited.touched().is_empty(),
        "an animation edits no document"
    );
    let without: Vec<Change> = changes.iter().filter(|c| c.group() != 2).cloned().collect();
    let edited = anim_model(&[]).with_changes(&without);
    assert_eq!(
        edited
            .model()
            .animations()
            .event("HudHealthDyingPulse")
            .unwrap()
            .body(),
        "RunEvent \"B\" 0"
    );
}

#[test]
fn identical_appended_lines_keep_one_copy() {
    let both = [
        append(1, "DamagedPlayer", "RunEvent Hit 0"),
        append(2, "DamagedPlayer", "RunEvent Hit 0"),
    ];
    let m = anim_model(&[]).with_changes(&both).into_model();
    assert_eq!(
        m.animations().event("DamagedPlayer").unwrap().body(),
        "RunEvent \"Hit\" 0"
    );
    let m = anim_model(&[]).with_changes(&both[1..]).into_model();
    assert_eq!(
        m.animations().event("DamagedPlayer").unwrap().body(),
        "RunEvent \"Hit\" 0"
    );
}

#[test]
fn invalid_changes_drop_and_count() {
    let many = vec!["RunEvent A 0"; 257].join("\n");
    let bad = [
        whole(1, "Hud Pulse", "RunEvent A 0"),
        whole(1, "", "RunEvent A 0"),
        whole(1, "A//B", "RunEvent A 0"),
        whole(1, "Ok", "FireCommand 0 \"engine quit\""),
        append(1, "Ok", "FireCommand 0 \"engine quit\""),
        whole(1, "Ok", "Explode now"),
        whole(1, "Ok", "RunEvent A nan"),
        whole(1, "Ok", &many),
        whole(1, &"N".repeat(65), "RunEvent A 0"),
    ];
    let edited = anim_model(&[]).with_changes(&bad);
    assert_eq!((edited.applied().len(), edited.dropped()), (0, bad.len()));
    assert!(edited.model().animations().event("Ok").is_none());
}

/// The animation bodies a recipe applies onto `extra` events, and how many lines it left out.
fn recipe_onto(extra: &str, body: &str) -> (Vec<String>, usize) {
    let anims = format!("{ANIMS}{extra}");
    let model = anim_model(&[(ANIMS_PATH, Some(&anims))]);
    let text = format!(
        r#"{{ "version": 1, "kind": "recipe", "name": "Test", "made_for": null, "assets": {{}},
        "changes": [ {{ "group": 1, "animation": {{ "event": "DamagedPlayer", "body": {} }} }} ] }}"#,
        serde_json::Value::from(body)
    );
    let (recipe, skipped) = Recipe::parse(text.as_bytes()).unwrap();
    assert_eq!(skipped, 0);
    let applied = model.apply_recipe(&recipe, &[], 7);
    let bodies = applied
        .changes()
        .iter()
        .filter_map(|c| match c.target() {
            ChangeTarget::Animation(a) => Some(a.body().to_owned()),
            _ => None,
        })
        .collect();
    (bodies, applied.skipped())
}

#[test]
fn a_recipe_cannot_reach_a_console_command_through_a_chain() {
    let chain = "event Risky { FireCommand 0 \"engine quit\" }\nevent Middle { RunEventChild Panel Risky 0 }\nevent Top { RunEvent Middle 0 }\nevent Safe { RunEvent Fine 0 }\nevent Fine { }\n";
    let (bodies, left_out) = recipe_onto(
        chain,
        "RunEvent Top 0\nRunEventChild Panel Risky 0\nRunEvent Safe 0",
    );
    assert_eq!(bodies, ["RunEvent \"Safe\" 0"]);
    assert_eq!(left_out, 2);
}

#[test]
fn a_forty_link_chain_to_a_console_command_is_still_dropped() {
    let mut chain: String = (0..40)
        .map(|i| format!("event L{i} {{ RunEvent L{} 0 }}\n", i + 1))
        .collect();
    chain.push_str("event L40 { FireCommand 0 \"engine quit\" }\n");
    let (bodies, left_out) = recipe_onto(&chain, "RunEvent L0 0\nRunEvent Fine 0");
    assert_eq!(
        (bodies, left_out),
        (vec!["RunEvent \"Fine\" 0".to_owned()], 1)
    );
}

#[test]
fn with_scene_keeps_the_edited_events() {
    let edited = anim_model(&[])
        .with_changes(&[whole(1, "HudMedicCharged", "RunEvent Mine 0")])
        .into_model();
    let back = edited
        .with_scene(tf2tools_core::Scene::Scoreboard)
        .with_scene(tf2tools_core::Scene::Hud);
    assert_eq!(
        back.animations().event("HudMedicCharged").unwrap().body(),
        "RunEvent \"Mine\" 0"
    );
    let again = back.with_changes(&[]).into_model();
    assert!(
        again
            .animations()
            .event("HudMedicCharged")
            .unwrap()
            .body()
            .contains("ChargeLabel"),
        "a replay starts from the files"
    );
}

#[test]
fn a_cycle_ends_the_walk() {
    let cycle = "event A { RunEvent B 0 }\nevent B { RunEvent A 0 }\n";
    let (bodies, left_out) = recipe_onto(cycle, "RunEvent A 0");
    assert_eq!((bodies, left_out), (vec!["RunEvent A 0".to_owned()], 0));
}

/// A body whose quoted panel name hides a console command on its own line.
const PROBE: &str = "SetVisible \"x 1 0\nFireCommand 0 quit\nSetVisible y\" 1 0";

fn runs_a_command(model: &tf2tools_core::HudModel) -> bool {
    model.animations().events().iter().any(|e| {
        e.commands()
            .iter()
            .any(|c| matches!(c, tf2tools_core::AnimCommand::FireCommand { .. }))
    })
}

#[test]
fn a_smuggled_console_command_is_dropped_by_the_editor_replay() {
    let edited = anim_model(&[])
        .with_changes(&[whole(1, "Sneaky", PROBE), append(2, "DamagedPlayer", PROBE)]);
    assert_eq!((edited.applied().len(), edited.dropped()), (0, 2));
    assert!(!runs_a_command(edited.model()));
}

#[test]
fn a_smuggled_console_command_never_lands_from_a_recipe() {
    let model = anim_model(&[]);
    let text = format!(
        r#"{{ "version": 1, "kind": "recipe", "name": "Test", "made_for": null, "assets": {{}},
        "changes": [ {{ "group": 1, "animation": {{ "event": "DamagedPlayer", "body": {} }} }} ] }}"#,
        serde_json::Value::from(PROBE)
    );
    let (recipe, _) = Recipe::parse(text.as_bytes()).unwrap();
    let applied = model.apply_recipe(&recipe, &[], 7);
    let after = model.with_changes(applied.changes()).into_model();
    assert!(!runs_a_command(&after));
    assert!(
        applied
            .changes()
            .iter()
            .all(|c| !matches!(c.target(), ChangeTarget::Animation(_)))
    );
}

#[test]
fn an_event_that_runs_a_console_command_cannot_be_changed() {
    let anims = format!("{ANIMS}event Risky\n{{\n    FireCommand 0 \"engine quit\"\n}}\n");
    let model = anim_model(&[(ANIMS_PATH, Some(&anims))]);
    assert_eq!(
        model.can_change_animation("risky"),
        Err(tf2tools_core::EditRefused::RunsGameCommands)
    );
    assert_eq!(model.can_change_animation("HudMedicCharged"), Ok(()));
    let edited = model.with_changes(&[
        whole(1, "Risky", "RunEvent A 0"),
        append(2, "RISKY", "RunEvent B 0"),
    ]);
    assert_eq!((edited.applied().len(), edited.dropped()), (0, 2));
    let risky = edited.model().animations().event("Risky").unwrap();
    assert_eq!(risky.body(), "FireCommand 0 \"engine quit\"");
}

#[test]
fn a_loaded_event_s_lines_are_its_own_and_appends_follow_them() {
    let loaded = anim_model(&[]);
    let dying = loaded.animations().event("HudHealthDyingPulse").unwrap();
    assert_eq!(dying.own_len(), dying.commands().len());
    assert_eq!(dying.own_body(), dying.body());
    let edited = loaded.with_changes(&[append(1, "HudHealthDyingPulse", "RunEvent Flash 0")]);
    let appended = edited
        .model()
        .animations()
        .event("HudHealthDyingPulse")
        .unwrap();
    assert_eq!(
        appended.own_body(),
        dying.body(),
        "the append is not its own"
    );
    assert_eq!(appended.commands().len(), dying.commands().len() + 1);
}
