//! HUD options that comment and uncomment lines (animation `.txt` files and `#base` lines) or
//! write a whole file of text, using the bundled flawhud and rayshud schemas over small
//! in-memory copies.

use tf2tools_core::{
    Change, HudFiles, HudOptions, HudSchema, HudSchemas, OptionChange, OptionKind, OptionValue,
    Platform,
};

const UBER: &str = "e:rh_val_uber_animation";
const HITMARKER: &str = "e:rh_toggle_xhair_pulse";
const DISGUISE: &str = "e:fh_toggle_disguise_image";
const HEALTH: &str = "e:fh_val_health_style";

const RAYS_ANIMS: &str = "scripts/hudanimations_custom.txt";
const RAYS_HIT: &str = "scripts/hudanimations_hitmarker.txt";

fn bundled(name: &str) -> HudSchema {
    HudSchemas::bundled().for_name(name).unwrap().clone()
}

fn copy(files: &[(&str, &str)]) -> HudFiles {
    let layer = (files.iter())
        .map(|(p, t)| ((*p).to_owned(), t.as_bytes().to_vec()))
        .collect();
    HudFiles::from_memory(vec![layer]).with_install_folder("hud")
}

fn choose(id: &str, value: OptionValue) -> Change {
    Change::option(1, OptionChange::new(OptionChange::SCHEMA, id, value))
}

fn text(files: &HudFiles, path: &str) -> String {
    String::from_utf8(files.read(path).unwrap().unwrap().into_bytes()).unwrap()
}

/// A rayshud copy as it ships: the flash uber on, the hitmarker lines commented out.
fn rayshud() -> HudFiles {
    copy(&[
        (
            RAYS_ANIMS,
            "event HudMedicCharged\n{\n\tRunEvent HudMedicOrangePulseCharge \t0.0\n//\tRunEvent HudMedicSolidColorCharge\t0.0\n//\tRunEvent HudMedicRainbowCharged \t0.0\n}\n\nevent HudMedicRainbowCharged\n{\n}\n",
        ),
        (
            RAYS_HIT,
            "event DamagedPlayer\n{\n//\tStopEvent\tHitMarker \t0.0\n//\tRunEvent\tHitMarker \t0.00001\n}\n",
        ),
    ])
}

const FLAW_ANIMS: &str = "event HudHealthBonusPulse\n{\n\tRunEvent HudHealthBonusBox 0.0\n\t//RunEvent HudHealthBonusText 0.0\n\tRunEvent HudHealthDyingBox 0.0\n\t//RunEvent HudHealthDyingText 0.0\n\tRunEvent HudHealthStopBox 0.0\n\t//RunEvent HudHealthStopText 0.0\n\tRunEvent HudAmmoLowBox 0.0\n\t//RunEvent HudAmmoLowText 0.0\n\tRunEvent HudAmmoStopBox 0.0\n\t//RunEvent HudAmmoStopText 0.0\n}\nevent HudSpyDisguiseChanged\n{\n\tAnimate\tPlayerStatusSpyOutlineImage\tAlpha\t\"200\"\tLinear 0.0 0.2\n\tAnimate\tPlayerStatusSpyOutlineImage\tSize\t\"120 120\"\tLinear 0.0 0.2\n}\n";

const FLAW_HEALTH: &str = "\"Resource/UI/HudPlayerHealth.res\"\n{\n\t\"PlayerStatusHealthImage\" { \"visible\" \"1\" }\n\t\"PlayerStatusHealthImageBG\" { \"xpos\" \"18\" }\n\t\"HealthBG\" { \"bgcolor_override\" \"Transparent\" }\n\t\"PlayerStatusHealthBonusImage\" { \"xpos\" \"23\" }\n}\n";
const FLAW_AMMO: &str = "\"Resource/UI/HudAmmoWeapons.res\"\n{\n\t\"AmmoBG\" { \"bgcolor_override\" \"Transparent\" }\n}\n";

fn flawhud() -> HudFiles {
    copy(&[
        ("scripts/hudanimations_custom.txt", FLAW_ANIMS),
        ("resource/ui/hudplayerhealth_base.res", FLAW_HEALTH),
        ("resource/ui/hudammoweapons_base.res", FLAW_AMMO),
    ])
}

#[test]
fn the_uber_style_is_a_pick_that_starts_where_the_copy_is() {
    let files = rayshud();
    let options = HudOptions::read(&files, Some(&bundled("rayshud")), Platform::Linux);
    let uber = options.specs().iter().find(|s| s.id() == UBER).unwrap();
    let OptionKind::Choice(choices) = uber.kind() else {
        panic!("the uber style is a pick");
    };
    let labels: Vec<&str> = choices.iter().map(|c| c.label()).collect();
    assert_eq!(labels, ["Flash", "Solid", "Rainbow"]);
    assert_eq!(
        options.value(UBER, &[]),
        Some(OptionValue::Pick("0".into()))
    );
}

#[test]
fn picking_a_style_comments_the_others_and_undo_restores_the_file() {
    let files = rayshud();
    let options = HudOptions::read(&files, Some(&bundled("rayshud")), Platform::Linux);
    let next = options
        .set(UBER, OptionValue::Pick("2".into()), &[], 1)
        .unwrap();
    let switched = options.switch(&files, &next).unwrap();
    let got = text(switched.files(), RAYS_ANIMS);
    assert!(got.contains("\n\t//RunEvent HudMedicOrangePulseCharge \t0.0\n"));
    assert!(got.contains("\n//\tRunEvent HudMedicSolidColorCharge\t0.0\n"));
    assert!(got.contains("\n\tRunEvent HudMedicRainbowCharged \t0.0\n"));
    // The event's own name line holds no `RunEvent`, so it is left alone.
    assert!(got.contains("\nevent HudMedicRainbowCharged\n"));

    // Undo is the history without the choice: the file is as the HUD ships it.
    let back = options.switch(&files, &[]).unwrap();
    assert!(back.files().switched_paths().0.is_empty());
    // Picking the shipped style again stores nothing.
    let again = options
        .set(UBER, OptionValue::Pick("0".into()), &next, 2)
        .unwrap();
    assert!(again.is_empty());
}

#[test]
fn a_switch_reads_its_start_from_the_copy_and_uncomments_its_lines() {
    let files = rayshud();
    let options = HudOptions::read(&files, Some(&bundled("rayshud")), Platform::Linux);
    // The schema says on, but the shipped lines are commented out.
    assert_eq!(
        options.value(HITMARKER, &[]),
        Some(OptionValue::Bool(false))
    );
    let next = options
        .set(HITMARKER, OptionValue::Bool(true), &[], 1)
        .unwrap();
    assert_eq!(next.len(), 1);
    let switched = options.switch(&files, &next).unwrap();
    assert_eq!(
        text(switched.files(), RAYS_HIT),
        "event DamagedPlayer\n{\n\tStopEvent\tHitMarker \t0.0\n\tRunEvent\tHitMarker \t0.00001\n}\n"
    );
}

#[test]
fn a_switch_off_comments_what_on_uncomments() {
    let on = RAYS_HIT_ON;
    let files = copy(&[(RAYS_ANIMS, "x\n"), (RAYS_HIT, on)]);
    let options = HudOptions::read(&files, Some(&bundled("rayshud")), Platform::Linux);
    assert_eq!(options.value(HITMARKER, &[]), Some(OptionValue::Bool(true)));
    let switched = options
        .switch(&files, &[choose(HITMARKER, OptionValue::Bool(false))])
        .unwrap();
    assert_eq!(
        text(switched.files(), RAYS_HIT),
        "\t//StopEvent HitMarker 0.0\n\t//RunEvent HitMarker 0.1\n"
    );
}

const RAYS_HIT_ON: &str = "\tStopEvent HitMarker 0.0\n\tRunEvent HitMarker 0.1\n";

#[test]
fn hiding_the_disguise_outline_comments_every_line_naming_it() {
    let files = flawhud();
    let options = HudOptions::read(&files, Some(&bundled("flawhud")), Platform::Linux);
    assert_eq!(options.value(DISGUISE, &[]), Some(OptionValue::Bool(false)));
    let switched = options
        .switch(&files, &[choose(DISGUISE, OptionValue::Bool(true))])
        .unwrap();
    let got = text(switched.files(), "scripts/hudanimations_custom.txt");
    assert_eq!(
        got.matches("\t//Animate\tPlayerStatusSpyOutlineImage")
            .count(),
        2
    );
}

#[test]
fn a_health_style_writes_its_keys_and_its_animation_lines_together() {
    let files = flawhud();
    let options = HudOptions::read(&files, Some(&bundled("flawhud")), Platform::Linux);
    assert!(options.specs().iter().any(|s| s.id() == HEALTH));
    let switched = options
        .switch(&files, &[choose(HEALTH, OptionValue::Pick("0".into()))])
        .unwrap();
    let anims = text(switched.files(), "scripts/hudanimations_custom.txt");
    assert!(anims.contains("\t//RunEvent HudHealthBonusBox 0.0\n"));
    assert!(anims.contains("\tRunEvent HudHealthBonusText 0.0\n"));
    assert!(anims.contains("\tRunEvent HudAmmoStopText 0.0\n"));
    let health = text(switched.files(), "resource/ui/hudplayerhealth_base.res");
    assert!(health.contains("\"bgcolor_override\" \"BGBlack\""));
}

#[test]
fn a_line_the_copy_lacks_makes_the_option_a_misfit() {
    let files = copy(&[
        (RAYS_ANIMS, "x\n"),
        (RAYS_HIT, "event DamagedPlayer\n{\n}\n"),
    ]);
    let options = HudOptions::read(&files, Some(&bundled("rayshud")), Platform::Linux);
    assert!(options.specs().iter().all(|s| s.id() != HITMARKER));
    assert!(options.specs().iter().all(|s| s.id() != UBER));
    assert!(options.misfits() >= 2);
}

/// A schema of one switch acting through `action`, a JSON fragment.
fn one_control(action: &str) -> HudSchema {
    let json = format!(
        r#"{{ "Controls": {{ "G": [ {{ "Name": "x", "Label": "X", "Type": "Checkbox", "Value": "false", {action} }} ] }} }}"#
    );
    HudSchema::parse("one", json.as_bytes()).unwrap()
}

#[test]
fn a_comment_list_on_a_res_file_toggles_its_base_line() {
    let res = "#base \"../alternatives/FriendsList_Off.res\"\n\"x\" { }\n";
    let files = copy(&[("customizations/friendslist.res", res)]);
    let schema = one_control(
        r##""Files": { "customizations/FriendsList.res": { "comment": [ "#base \"../alternatives/FriendsList_Off.res\"" ] } }"##,
    );
    let options = HudOptions::read(&files, Some(&schema), Platform::Linux);
    let switched = options
        .switch(&files, &[choose("e:x", OptionValue::Bool(true))])
        .unwrap();
    assert_eq!(
        text(switched.files(), "customizations/friendslist.res"),
        format!("//{res}")
    );
}

#[test]
fn write_file_with_any_text_writes_each_side_inside_the_copy() {
    let files = copy(&[("resource/ui/x.res", "\"x\" { }\n")]);
    let schema = one_control(
        r#""WriteFile": { "FileName": "resource/ui/extra.txt", "TrueText": "on text", "FalseText": "off text" }"#,
    );
    let options = HudOptions::read(&files, Some(&schema), Platform::Linux);
    let on = options
        .switch(&files, &[choose("e:x", OptionValue::Bool(true))])
        .unwrap();
    assert_eq!(text(on.files(), "resource/ui/extra.txt"), "on text");
    let off = options
        .switch(&files, &[choose("e:x", OptionValue::Bool(false))])
        .unwrap();
    assert_eq!(text(off.files(), "resource/ui/extra.txt"), "off text");
    // With no choice stored the copy is as it ships.
    let none = options.switch(&files, &[]).unwrap();
    assert!(
        none.files()
            .read("resource/ui/extra.txt")
            .unwrap()
            .is_none()
    );
}

#[test]
fn write_file_outside_the_copy_or_to_a_script_is_left_out() {
    let files = copy(&[("resource/ui/x.res", "\"x\" { }\n")]);
    for bad in [
        "../x.txt",
        "/x.txt",
        "cfg/autoexec.txt",
        "resource/ui/x.cfg",
        "c:/x.txt",
    ] {
        let schema = one_control(&format!(
            r#""WriteFile": {{ "FileName": "{bad}", "TrueText": "on text", "FalseText": "" }}"#
        ));
        let options = HudOptions::read(&files, Some(&schema), Platform::Linux);
        assert!(options.specs().is_empty(), "{bad} was taken");
        assert_eq!(options.left_out(), 1, "{bad}");
    }
    // A folder the copy lacks is a misfit, as nothing in the HUD would read the file.
    let schema = one_control(
        r#""WriteFile": { "FileName": "nowhere/x.txt", "TrueText": "on text", "FalseText": "" }"#,
    );
    let options = HudOptions::read(&files, Some(&schema), Platform::Linux);
    assert_eq!(options.misfits(), 1);
}
