//! A borrowed part brings its events and the trigger lines that start them.

use crate::common;

use common::hud_anims::{ANIMS, ANIMS_PATH, anim_files, anim_model};
use common::hud_borrow::source;
use tf2tools_core::{
    Brings, Change, ChangeTarget, Fired, HudModel, LeftBehind, PartKey, Rgba, Screen,
};

const CROSS: &str = "HudPlayerStatus/HudPlayerClass/CustomCrosshair";
const SRC_CLASS: &str = r#""Resource/UI/HudPlayerClass.res" { HudPlayerClass { ControlName EditablePanel xpos 0 ypos 0 wide f0 tall 480 }
  CustomCrosshair { ControlName CExLabel xpos cs-0.5 ypos cs-0.5 wide 40 tall 40 labelText "<" fgcolor "255 255 255 255" } }"#;
const SRC_MANIFEST: &str = "hudanimations_manifest { file scripts/hudanimations_hit.txt }";
const HIT: &str = r#"event DamagedPlayer
{
    StopEvent HitMarker 0
    RunEvent HitMarker 0.00001
    RunEvent Unrelated 0
}
event HitMarker
{
    Animate CustomCrosshair FgColor "255 0 0 255" Linear 0 0.00001
    Animate CustomCrosshair FgColor "255 255 255 255" Linear 0.1 0.00001
}
event Unrelated { Animate Other Alpha "0" Linear 0 1 }
event Danger
{
    FireCommand 0 "engine quit"
    Animate CustomCrosshair Alpha "9" Linear 0 1
}
"#;

fn rayshud() -> tf2tools_core::BorrowSource {
    source(&[
        ("resource/ui/hudplayerclass.res", SRC_CLASS),
        ("scripts/hudanimations_manifest.txt", SRC_MANIFEST),
        ("scripts/hudanimations_hit.txt", HIT),
    ])
}

/// (shape, event, text) of each animation change.
fn anims(changes: &[Change]) -> Vec<(&'static str, String, String)> {
    changes
        .iter()
        .filter_map(|c| match c.target() {
            ChangeTarget::Animation(a) => {
                Some(("whole", a.event().to_owned(), a.body().to_owned()))
            }
            ChangeTarget::AnimationAppend(a) => {
                Some(("append", a.event().to_owned(), a.lines().to_owned()))
            }
            _ => None,
        })
        .collect()
}

fn borrow(target: &HudModel) -> tf2tools_core::Borrowed {
    target
        .borrow_part(&rayshud(), &anim_files(&[]), &PartKey::new(CROSS), 9)
        .unwrap()
}

#[test]
fn a_borrowed_hitmarker_fires_on_damaged_player() {
    let target = anim_model(&[]);
    let b = borrow(&target);
    let got = anims(b.changes());
    assert!(got.contains(&(
        "whole",
        "HitMarker".into(),
        "Animate \"CustomCrosshair\" \"FgColor\" \"255 0 0 255\" Linear 0 0.00001\nAnimate \"CustomCrosshair\" \"FgColor\" \"255 255 255 255\" Linear 0.1 0.00001".into()
    )));
    assert!(got.contains(&(
        "append",
        "DamagedPlayer".into(),
        "StopEvent \"HitMarker\" 0\nRunEvent \"HitMarker\" 0.00001".into()
    )));
    assert!(got.contains(&(
        "whole",
        "Danger".into(),
        "Animate \"CustomCrosshair\" \"Alpha\" \"9\" Linear 0 1".into()
    )));
    assert!(!got.iter().any(|(_, e, _)| e == "Unrelated"));
    assert!(b.left_behind().contains(&LeftBehind::Animations(1)));
    assert!(b.brings().contains(&Brings::Animation("HitMarker".into())));
    let m = target.with_changes(b.changes()).into_model();
    let layout = m.layout(Screen::new(1920, 1080));
    let hit = [Fired::new("DamagedPlayer", None)];
    let frame = m.animations().frame(&m, &layout, &hit, 0.05);
    assert_eq!(
        frame.get(&PartKey::new(CROSS)).unwrap().fg(),
        Some(Rgba::new(255, 0, 0, 255))
    );
}

#[test]
fn the_target_s_trigger_keeps_its_own_lines() {
    let theirs = ANIMS.replacen(
        "event DamagedPlayer\n{\n}",
        "event DamagedPlayer\n{\n    RunEvent Theirs 0\n}",
        1,
    );
    let target = anim_model(&[(ANIMS_PATH, Some(&theirs))]);
    let m = target.with_changes(borrow(&target).changes()).into_model();
    assert_eq!(
        m.animations().event("DamagedPlayer").unwrap().body(),
        "RunEvent \"Theirs\" 0\nStopEvent \"HitMarker\" 0\nRunEvent \"HitMarker\" 0.00001"
    );
}

#[test]
fn a_name_the_target_has_in_another_case_is_renamed_with_its_references() {
    let mine = format!("{ANIMS}event hitmarker {{ RunEvent Mine 0 }}\n");
    let target = anim_model(&[(ANIMS_PATH, Some(&mine))]);
    let got = anims(borrow(&target).changes());
    assert!(
        got.iter()
            .any(|(s, e, _)| *s == "whole" && e == "HitMarker_b1")
    );
    assert!(got.contains(&(
        "append",
        "DamagedPlayer".into(),
        "StopEvent \"HitMarker_b1\" 0\nRunEvent \"HitMarker_b1\" 0.00001".into()
    )));
    let m = target.with_changes(borrow(&target).changes()).into_model();
    assert_eq!(
        m.animations().event("hitmarker").unwrap().body(),
        "RunEvent \"Mine\" 0"
    );
}

#[test]
fn a_second_borrow_does_not_collide_with_the_first() {
    let target = anim_model(&[]);
    let first = borrow(&target);
    let after = target.with_changes(first.changes()).into_model();
    let second = after
        .borrow_part(&rayshud(), &anim_files(&[]), &PartKey::new(CROSS), 10)
        .unwrap();
    let got = anims(second.changes());
    assert!(
        got.iter()
            .any(|(s, e, _)| *s == "whole" && e == "HitMarker_b1")
    );
    assert!(got.iter().any(|(s, e, t)| *s == "append"
        && e == "DamagedPlayer"
        && t.contains("RunEvent \"HitMarker_b1\"")));
}

#[test]
fn a_trigger_s_own_console_commands_are_not_counted_as_the_part_s() {
    let hit = HIT.replacen(
        "    RunEvent Unrelated 0\n",
        "    RunEvent Unrelated 0\n    FireCommand 0 \"engine echo hit\"\n",
        1,
    );
    assert_ne!(hit, HIT);
    let src = source(&[
        ("resource/ui/hudplayerclass.res", SRC_CLASS),
        ("scripts/hudanimations_manifest.txt", SRC_MANIFEST),
        ("scripts/hudanimations_hit.txt", &hit),
    ]);
    let b = anim_model(&[])
        .borrow_part(&src, &anim_files(&[]), &PartKey::new(CROSS), 9)
        .unwrap();
    assert!(
        b.left_behind().contains(&LeftBehind::Animations(1)),
        "{:?}",
        b.left_behind()
    );
}

#[test]
fn a_target_trigger_that_runs_a_console_command_is_not_appended_to() {
    let theirs = ANIMS.replacen(
        "event DamagedPlayer\n{\n}",
        "event DamagedPlayer\n{\n    FireCommand 0 \"engine echo mine\"\n}",
        1,
    );
    let target = anim_model(&[(ANIMS_PATH, Some(&theirs))]);
    let b = borrow(&target);
    assert!(
        !anims(b.changes())
            .iter()
            .any(|(_, e, _)| e == "DamagedPlayer")
    );
    // Danger's console command, and the two DamagedPlayer lines that could not be added.
    assert!(
        b.left_behind().contains(&LeftBehind::Animations(3)),
        "{:?}",
        b.left_behind()
    );
}

#[test]
fn a_borrowed_trigger_line_cannot_run_the_player_s_console_command() {
    let evil = format!("{ANIMS}event Evil\n{{\n    FireCommand 0 \"quit\"\n}}\n");
    let target = anim_model(&[(ANIMS_PATH, Some(&evil))]);
    let hit = HIT.replacen("    RunEvent Unrelated 0\n", "    RunEvent Unrelated 0\n    RunEventChild CustomCrosshair Evil 0\n    RunEvent Evil 0.5\n", 1);
    assert_ne!(hit, HIT);
    let src = source(&[
        ("resource/ui/hudplayerclass.res", SRC_CLASS),
        ("scripts/hudanimations_manifest.txt", SRC_MANIFEST),
        ("scripts/hudanimations_hit.txt", &hit),
    ]);
    let b = target
        .borrow_part(&src, &anim_files(&[]), &PartKey::new(CROSS), 9)
        .unwrap();
    let got = anims(b.changes());
    assert!(
        !got.iter().any(|(_, _, text)| text.contains("Evil")),
        "{got:?}"
    );
    // Danger's console command, and the RunEventChild line that named the player's Evil.
    assert!(
        b.left_behind().contains(&LeftBehind::Animations(2)),
        "{:?}",
        b.left_behind()
    );
    let m = target.with_changes(b.changes()).into_model();
    let hit = [Fired::new("DamagedPlayer", None)];
    let layout = m.layout(Screen::new(1920, 1080));
    let _ = m.animations().frame(&m, &layout, &hit, 1.0);
    assert!(
        m.animations()
            .event("DamagedPlayer")
            .unwrap()
            .body()
            .contains("HitMarker")
    );
}
