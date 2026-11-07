//! HUD options that write a game settings file the HUD's `#base` lines read, using the
//! bundled darhud and flawhud schemas over small in-memory copies.

use tf2tools_core::{
    CfgFile, Change, HudFiles, HudOptions, HudSchema, HudSchemas, OptionAction, OptionChange,
    OptionKind, OptionValue, Platform,
};

const UBER: &str = "e:dh_toggle_centered_ubercharge";
const METERS: &str = "e:dh_toggle_centered_item_meters";
const SPEED: &str = "e:fh_speedometer";

fn bundled(name: &str) -> HudSchema {
    HudSchemas::bundled().for_name(name).unwrap().clone()
}

fn copy(folder: &str, files: &[(&str, &str)]) -> HudFiles {
    let layer = (files.iter())
        .map(|(p, t)| ((*p).to_owned(), t.as_bytes().to_vec()))
        .collect();
    HudFiles::from_memory(vec![layer]).with_install_folder(folder)
}

/// A darhud copy whose files `#base` the uber and meter files in `tf/cfg/darhud/`.
fn darhud() -> HudFiles {
    copy(
        "darhud",
        &[
            (
                "resource/ui/hudmediccharge.res",
                "#base \"../../../../cfg/darhud/ubercharge.txt\"\n\"Resource/UI/HudMedicCharge.res\" { }\n",
            ),
            (
                "resource/ui/huditemeffectmeter_scout.res",
                "#base \"../../../../cfg/darhud/top_meter.txt\"\n#base \"../../../../cfg/darhud/middle_meter.txt\"\n#base \"../../../../cfg/darhud/bottom_meter.txt\"\n\"x\" { }\n",
            ),
        ],
    )
}

fn flawhud() -> HudFiles {
    copy(
        "flawhud",
        &[(
            "scripts/hudlayout.res",
            "#base \"../../../cfg/flawhud/hud_speedometer.txt\"\n#base \"../../../cfg/flawhud/hud_crosshair_animation.txt\"\n\"Resource/HudLayout.res\" { }\n",
        )],
    )
}

fn choose(id: &str, value: OptionValue) -> Change {
    Change::option(1, OptionChange::new(OptionChange::SCHEMA, id, value))
}

fn written(options: &HudOptions, files: &HudFiles, changes: &[Change]) -> Vec<(String, String)> {
    let switched = options.switch(files, changes).unwrap();
    (switched.files().switched_cfg().iter())
        .map(|f| {
            (
                f.path().to_owned(),
                String::from_utf8(f.bytes().to_vec()).unwrap(),
            )
        })
        .collect()
}

#[test]
fn settings_file_options_are_read_with_their_label_and_tooltip() {
    let options = HudOptions::read(&darhud(), Some(&bundled("darhud")), Platform::Linux);
    let uber = options.specs().iter().find(|s| s.id() == UBER).unwrap();
    assert_eq!(uber.label(), "Centered UberCharge");
    assert_eq!(*uber.kind(), OptionKind::Toggle);
    assert!(matches!(uber.actions(), [OptionAction::WriteCfg { .. }]));
    let meters = options.specs().iter().find(|s| s.id() == METERS).unwrap();
    let OptionKind::Choice(choices) = meters.kind() else {
        panic!("the meter position is a pick");
    };
    let labels: Vec<&str> = choices.iter().map(|c| c.label()).collect();
    assert_eq!(labels, ["Top", "Middle", "Bottom"]);
    // A settings file no `#base` in the copy names would change nothing, so it is no option.
    assert!(
        (options.specs().iter()).all(|s| s.id() != "e:dh_toggle_centered_metal"),
        "the metal counter's file is named nowhere"
    );

    let options = HudOptions::read(&flawhud(), Some(&bundled("flawhud")), Platform::Linux);
    let speed = options.specs().iter().find(|s| s.id() == SPEED).unwrap();
    assert_eq!(speed.label(), "Display Speedometer");
    assert_eq!(
        speed.tooltip(),
        Some("Displays a speedometer under the crosshair")
    );
}

#[test]
fn turning_an_option_on_writes_only_its_named_file() {
    let files = darhud();
    let options = HudOptions::read(&files, Some(&bundled("darhud")), Platform::Linux);
    assert!(written(&options, &files, &[]).is_empty());
    let on = [choose(UBER, OptionValue::Bool(true))];
    assert_eq!(
        written(&options, &files, &on),
        [(
            "darhud/ubercharge.txt".to_owned(),
            "#base ../../custom/darhud/resource/ui/customizations/ubercharge/centered_ubercharge.res"
                .to_owned()
        )]
    );
    // Off writes the empty off text, which is no file at all.
    let off = [choose(UBER, OptionValue::Bool(false))];
    assert!(written(&options, &files, &off).is_empty());
    // The copy's own files are untouched.
    let switched = options.switch(&files, &on).unwrap();
    assert!(switched.files().switched_paths().0.is_empty());
}

#[test]
fn a_pick_writes_the_picked_file_and_none_of_the_others() {
    let files = darhud();
    let options = HudOptions::read(&files, Some(&bundled("darhud")), Platform::Linux);
    let middle = [choose(METERS, OptionValue::Pick("1".into()))];
    let got = written(&options, &files, &middle);
    let paths: Vec<&str> = got.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(paths, ["darhud/middle_meter.txt"]);
    assert!(got[0].1.ends_with("centered_middle_meter.res"));
}

#[test]
fn the_flawhud_speedometer_writes_into_its_cfg_folder() {
    let files = flawhud();
    let options = HudOptions::read(&files, Some(&bundled("flawhud")), Platform::Linux);
    let got = written(&options, &files, &[choose(SPEED, OptionValue::Bool(true))]);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, "flawhud/hud_speedometer.txt");
    assert!(got[0].1.contains("speedometer_on.res"));
}

#[test]
fn the_written_file_reaches_the_preview_through_the_base_line() {
    let files = flawhud();
    let options = HudOptions::read(&files, Some(&bundled("flawhud")), Platform::Linux);
    let switched = options
        .switch(&files, &[choose(SPEED, OptionValue::Bool(true))])
        .unwrap();
    let file = switched
        .files()
        .read_tf_cfg("flawhud/hud_speedometer.txt")
        .unwrap()
        .unwrap();
    assert!(String::from_utf8_lossy(file.bytes()).contains("speedometer_on.res"));
}

/// A schema whose one WriteCfg control names `file`.
fn one_control(file: &str) -> HudSchema {
    let json = format!(
        r#"{{ "Controls": {{ "G": [ {{ "Name": "x", "Label": "X", "Type": "Checkbox", "Value": "false",
          "WriteCfg": {{ "FileName": "{file}", "TrueText": "x{{a{{b 1}}}}", "FalseText": "" }} }} ] }} }}"#
    );
    HudSchema::parse("one", json.as_bytes()).unwrap()
}

#[test]
fn a_file_name_that_is_no_plain_settings_name_is_left_out() {
    for bad in [
        "../x.txt",
        "sub/x.txt",
        "sub\\\\x.txt",
        "autoexec.cfg",
        "x.exe",
        ".tf2tools",
        "",
    ] {
        let base = format!("#base \"../../../cfg/one/{bad}\"\n\"x\" {{ }}\n");
        let files = copy("one", &[("scripts/hudlayout.res", &base)]);
        let options = HudOptions::read(&files, Some(&one_control(bad)), Platform::Linux);
        assert!(options.specs().is_empty(), "{bad} was taken");
        assert_eq!(options.left_out(), 1, "{bad}");
    }
}

#[test]
fn a_base_line_that_lands_outside_cfg_and_the_copy_misfits() {
    let files = copy(
        "one",
        &[(
            "scripts/hudlayout.res",
            "#base \"../../../../../x.txt\"\n\"x\" { }\n",
        )],
    );
    let options = HudOptions::read(&files, Some(&one_control("x.txt")), Platform::Linux);
    assert!(options.specs().is_empty());
    assert_eq!(options.misfits(), 1);
}

#[test]
fn a_file_the_copy_bases_from_its_own_folder_is_written_in_the_copy() {
    let files = copy(
        "one",
        &[(
            "scripts/hudlayout.res",
            "#base \"settings/x.txt\"\n\"x\" { }\n",
        )],
    );
    let options = HudOptions::read(&files, Some(&one_control("x.txt")), Platform::Linux);
    let switched = options
        .switch(&files, &[choose("e:x", OptionValue::Bool(true))])
        .unwrap();
    assert!(switched.files().switched_cfg().is_empty());
    let text = switched
        .files()
        .read("scripts/settings/x.txt")
        .unwrap()
        .unwrap();
    assert_eq!(text.bytes(), b"x{a{b 1}}");
}

#[test]
fn cfg_files_round_trip_through_their_record() {
    let file = CfgFile::new("darhud/ubercharge.txt", b"#base x.res".to_vec()).unwrap();
    assert_eq!(file.path(), "darhud/ubercharge.txt");
    assert!(CfgFile::new("../autoexec.cfg", Vec::new()).is_none());
    assert!(CfgFile::new("autoexec.cfg", Vec::new()).is_none());
}

#[test]
fn a_switch_whose_off_side_is_no_file_starts_off() {
    // The schema starts the hitmarker on, but without its file the game has it off.
    let files = flawhud();
    let options = HudOptions::read(&files, Some(&bundled("flawhud")), Platform::Linux);
    let pulse = "e:fh_toggle_xhair_pulse";
    assert_eq!(options.value(pulse, &[]), Some(OptionValue::Bool(false)));
    let next = options.set(pulse, OptionValue::Bool(true), &[], 1).unwrap();
    assert_eq!(next.len(), 1);
    assert_eq!(written(&options, &files, &next).len(), 1);
}
