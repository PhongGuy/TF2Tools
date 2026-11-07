//! The added crosshair and its flash when you hit someone.

use crate::common;

use common::hud_anims::{ANIMS, ANIMS_PATH, VALUE, anim_model};
use tf2tools_core::{
    AnimationChange, Change, ChangeTarget, EditRefused, Fired, HudModel, NewPart, PartKey, Rgba,
    Screen,
};

const HIT_RED: &str = "Animate \"TF2ToolsCrosshair\" \"FgColor\" \"255 0 0 255\" Linear 0 0.00001\nAnimate \"TF2ToolsCrosshair\" \"FgColor\" \"255 255 255 255\" Linear 0.1 0.00001";

const HD: Screen = Screen::new(1920, 1080);
const CROSS: &str = "HudPlayerStatus/HudPlayerClass/TF2ToolsCrosshair";
const OURS: &str = "StopEvent \"TF2ToolsHitMarker\" 0\nRunEvent \"TF2ToolsHitMarker\" 0.00001";

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

fn with_cross(base: &HudModel) -> (PartKey, Vec<Change>) {
    base.add_part(NewPart::Crosshair, &PartKey::new(VALUE), 1)
        .unwrap()
}

#[test]
fn the_crosshair_sits_at_the_screen_s_centre_once() {
    let base = anim_model(&[]);
    let (key, add) = with_cross(&base);
    assert_eq!(key.as_str(), CROSS);
    let m = base.with_changes(&add).into_model();
    let l = m.layout(HD);
    let b = l.get(&key).unwrap();
    assert!(((b.x() + b.wide() / 2) - 960).abs() <= 1 && ((b.y() + b.tall() / 2) - 540).abs() <= 1);
    assert_eq!(
        m.part(&key).unwrap().value("labelText").unwrap().text(),
        "+"
    );
    assert_eq!(
        m.add_part(NewPart::Crosshair, &PartKey::new(VALUE), 2)
            .unwrap(),
        (key, vec![])
    );
}

#[test]
fn the_hit_flash_writes_the_rayshud_pattern_and_plays() {
    let base = anim_model(&[]);
    let (key, add) = with_cross(&base);
    let m = base.with_changes(&add).into_model();
    let flash = m.hit_flash(&key, "255 0 0 255", 2).unwrap();
    assert!(flash.iter().all(|c| c.group() == 2));
    assert_eq!(
        anims(&flash),
        [
            ("append", "TF2ToolsHitMarker".to_owned(), HIT_RED.to_owned()),
            ("append", "DamagedPlayer".to_owned(), OURS.to_owned()),
        ]
    );
    let all = base.with_changes(&[add, flash].concat()).into_model();
    let layout = all.layout(HD);
    let hit = [Fired::new("DamagedPlayer", None)];
    let red = all.animations().frame(&all, &layout, &hit, 0.05);
    assert_eq!(red.get(&key).unwrap().fg(), Some(Rgba::new(255, 0, 0, 255)));
    let back = all.animations().frame(&all, &layout, &hit, 0.2);
    assert_eq!(
        back.get(&key).unwrap().fg(),
        Some(Rgba::new(255, 255, 255, 255))
    );
}

#[test]
fn the_hud_s_own_hit_lines_stay_and_ours_are_added_once() {
    let theirs = ANIMS.replacen(
        "event DamagedPlayer\n{\n}",
        "event DamagedPlayer\n{\n    StopEvent Theirs 0\n    RunEvent Theirs 0\n}",
        1,
    );
    assert_ne!(theirs, ANIMS);
    let base = anim_model(&[(ANIMS_PATH, Some(&theirs))]);
    let (key, add) = with_cross(&base);
    let first = base
        .with_changes(&add)
        .into_model()
        .hit_flash(&key, "255 0 0 255", 2)
        .unwrap();
    let all = [add.clone(), first].concat();
    let m = base.with_changes(&all).into_model();
    assert_eq!(
        m.animations().event("DamagedPlayer").unwrap().body(),
        format!("StopEvent \"Theirs\" 0\nRunEvent \"Theirs\" 0\n{OURS}")
    );
    assert!(
        m.hit_flash(&key, "0 255 0 255", 3).unwrap().is_empty(),
        "already flashing"
    );
}

#[test]
fn two_labels_flash_and_turning_one_off_leaves_the_other() {
    let base = anim_model(&[]);
    let (cross, add) = with_cross(&base);
    let value = PartKey::new(VALUE);
    let one = base
        .with_changes(&add)
        .into_model()
        .hit_flash(&cross, "255 0 0 255", 2)
        .unwrap();
    let both_before = [add.clone(), one.clone()].concat();
    let two = base
        .with_changes(&both_before)
        .into_model()
        .hit_flash(&value, "0 0 255 255", 3)
        .unwrap();
    let all = [both_before, two].concat();
    let m = base.with_changes(&all).into_model();
    assert_eq!(
        m.animations()
            .event("TF2ToolsHitMarker")
            .unwrap()
            .commands()
            .len(),
        4
    );
    assert_eq!(
        m.animations().event("DamagedPlayer").unwrap().body(),
        OURS,
        "one copy of the trigger lines"
    );
    let layout = m.layout(HD);
    let hit = [Fired::new("DamagedPlayer", None)];
    let frame = m.animations().frame(&m, &layout, &hit, 0.05);
    assert_eq!(
        frame.get(&cross).unwrap().fg(),
        Some(Rgba::new(255, 0, 0, 255))
    );
    assert_eq!(
        frame.get(&value).unwrap().fg(),
        Some(Rgba::new(0, 0, 255, 255))
    );
    let off: Vec<Change> = all.into_iter().filter(|c| c.group() != 2).collect();
    let m = base.with_changes(&off).into_model();
    assert_eq!(m.animations().event("DamagedPlayer").unwrap().body(), OURS);
    let layout = m.layout(HD);
    let frame = m.animations().frame(&m, &layout, &hit, 0.05);
    assert_eq!(
        frame.get(&value).unwrap().fg(),
        Some(Rgba::new(0, 0, 255, 255))
    );
    assert!(
        frame
            .get(&cross)
            .is_none_or(|a| a.fg() != Some(Rgba::new(255, 0, 0, 255)))
    );
}

#[test]
fn turning_the_flash_off_keeps_a_later_edit_of_damaged_player() {
    let base = anim_model(&[]);
    let (key, add) = with_cross(&base);
    let flash = base
        .with_changes(&add)
        .into_model()
        .hit_flash(&key, "255 0 0 255", 2)
        .unwrap();
    let mine = Change::animation(3, AnimationChange::new("DamagedPlayer", "RunEvent Mine 0"));
    let all = [add, flash, vec![mine]].concat();
    let on = base.with_changes(&all).into_model();
    assert_eq!(
        on.animations().event("DamagedPlayer").unwrap().body(),
        format!("RunEvent \"Mine\" 0\n{OURS}")
    );
    let off: Vec<Change> = all.into_iter().filter(|c| c.group() != 2).collect();
    let off = base.with_changes(&off).into_model();
    assert_eq!(
        off.animations().event("DamagedPlayer").unwrap().body(),
        "RunEvent \"Mine\" 0"
    );
}

#[test]
fn the_flash_is_refused_where_it_cannot_be_written() {
    let plain = anim_model(&[]);
    let image = PartKey::new("HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthImage");
    assert_eq!(
        plain.hit_flash(&image, "255 0 0 255", 2),
        Err(EditRefused::NotOffered)
    );
    assert_eq!(
        plain.hit_flash(&PartKey::new(VALUE), "1 2 \"3", 2),
        Err(EditRefused::BadValue)
    );
    assert_eq!(
        plain.hit_flash(&PartKey::new("Nope"), "255 0 0 255", 2),
        Err(EditRefused::UnknownPart)
    );
}

#[test]
fn a_hud_whose_hit_event_runs_game_commands_takes_no_flash() {
    let theirs = ANIMS.replacen(
        "event DamagedPlayer\n{\n}",
        "event DamagedPlayer\n{\n    FireCommand 0 \"engine echo\"\n}",
        1,
    );
    assert_ne!(theirs, ANIMS);
    let base = anim_model(&[(ANIMS_PATH, Some(&theirs))]);
    let (key, add) = with_cross(&base);
    let m = base.with_changes(&add).into_model();
    assert_eq!(
        m.hit_flash(&key, "255 0 0 255", 2),
        Err(EditRefused::RunsGameCommands)
    );
}
