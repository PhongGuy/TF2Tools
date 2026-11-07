//! Schema options with detected switches: overlap, values and key edits in the switch layer.

mod common;

use common::hud_schema::schema;
use common::hud_stock::SCHEME;
use common::hud_switches::switch_files;
use tf2tools_core::{Change, HudFiles, HudOptions, OptionChange, OptionValue, Platform};

fn editor(id: &str, value: OptionValue) -> Change {
    Change::option(1, OptionChange::new(OptionChange::SCHEMA, id, value))
}
fn text(files: &HudFiles, path: &str) -> String {
    String::from_utf8(files.read(path).unwrap().unwrap().into_bytes()).unwrap()
}

#[test]
fn a_schema_control_hides_the_detected_switch_on_the_same_path() {
    let files = switch_files(&[]);
    let options = HudOptions::read(&files, Some(&schema()), Platform::Linux);
    let ids: Vec<&str> = options.specs().iter().map(|s| s.id()).collect();
    assert!(
        ids.contains(&"e:sh_big") && !ids.contains(&"b:#customization/bh_player_largehealth.res")
    );
    assert!(!ids.contains(&"c:customizations/streamermode_"));
    assert!(ids.contains(&"f:customizations/damage.res:0"));
    assert_eq!((options.misfits(), options.left_out()), (2, 6));
    assert!(
        HudOptions::read(&files, None, Platform::Linux)
            .specs()
            .iter()
            .any(|s| s.id() == "c:customizations/streamermode_")
    );
}

#[test]
fn values_start_from_the_copy() {
    let files = switch_files(&[]);
    let options = HudOptions::read(&files, Some(&schema()), Platform::Linux);
    assert_eq!(
        options.value("e:sh_tan", &[]),
        Some(OptionValue::Rgba([235, 226, 202, 255]))
    );
    assert_eq!(
        options.value("e:sh_chat_top", &[]),
        Some(OptionValue::Bool(false))
    );
    assert_eq!(options.value("e:sh_rows", &[]), Some(OptionValue::Int(4)));
}

#[test]
fn key_edits_land_in_the_switch_layer_keeping_the_file() {
    let shadowed = SCHEME.replace(
        "    TanDark",
        "    TanLightShadow \"0 0 0 255\"\n    TanLightPulse \"9 9 9 255\"\n    TanDark",
    );
    let files = switch_files(&[("resource/clientscheme.res", &shadowed)]);
    let options = HudOptions::read(&files, Some(&schema()), Platform::Linux);
    let switched = options
        .switch(
            &files,
            &[
                editor("e:sh_tan", OptionValue::Rgba([100, 50, 10, 255])),
                editor("e:sh_chat_top", OptionValue::Bool(true)),
                editor("e:sh_rows", OptionValue::Int(20)),
            ],
        )
        .unwrap();
    let scheme = text(switched.files(), "resource/clientscheme.res");
    assert_eq!(
        scheme,
        shadowed
            .replace("TanLight \"235 226 202 255\"", "TanLight \"100 50 10 255\"")
            .replace(
                "TanLightPulse \"9 9 9 255\"",
                "TanLightPulse \"100 50 10 205\""
            )
    );
    assert!(text(switched.files(), "resource/ui/basechat.res").contains("    ypos 0\n"));
    assert!(text(switched.files(), "scripts/hudlayout.res").contains("MaxDeathNotices 20"));
    assert_eq!(switched.dropped(), 0);
}

#[test]
fn a_combo_choice_runs_its_move_and_off_values_restore_pristine() {
    let files = switch_files(&[]);
    let options = HudOptions::read(&files, Some(&schema()), Platform::Linux);
    let switched = options
        .switch(
            &files,
            &[editor("e:sh_style", OptionValue::Pick("1".into()))],
        )
        .unwrap();
    assert!(
        switched
            .files()
            .contains("customizations/streamermode/ammo.res")
    );
    let off = options
        .switch(&files, &[editor("e:sh_chat_top", OptionValue::Bool(false))])
        .unwrap();
    assert_eq!(off.files().switched_paths(), (Vec::new(), Vec::new()));
}

#[test]
fn a_combo_pick_writes_its_own_literal() {
    let json = r##"{ "Controls": { "A": [ { "Name": "c", "Label": "C", "Type": "ComboBox", "Value": "0", "Options": [
      { "Label": "Low", "Value": "0", "Files": { "resource/ui/basechat.res": { "HudChat": { "ypos": "275" } } } },
      { "Label": "Mid", "Value": "2", "Files": { "resource/ui/basechat.res": { "HudChat": { "ypos": "100" } } } } ] } ] } }"##;
    let schema = tf2tools_core::HudSchema::parse("x", json.as_bytes()).unwrap();
    let files = switch_files(&[]);
    let options = HudOptions::read(&files, Some(&schema), Platform::Linux);
    let mid = options
        .switch(&files, &[editor("e:c", OptionValue::Pick("2".into()))])
        .unwrap();
    assert!(text(mid.files(), "resource/ui/basechat.res").contains("    ypos 100\n"));
    let low = options
        .switch(&files, &[editor("e:c", OptionValue::Pick("0".into()))])
        .unwrap();
    assert_eq!(low.files().switched_paths(), (Vec::new(), Vec::new()));
}

#[test]
fn a_shadow_flag_writes_the_colour_darkened_where_the_scheme_names_it() {
    let json = r##"{ "Controls": { "A": [ { "Name": "s", "Label": "S", "Type": "ColorPicker", "Value": "1 2 3 255", "Shadow": true,
      "Files": { "resource/clientscheme.res": { "Scheme": { "Colors": { "TanLight": "$value" } } } } } ] } }"##;
    let schema = tf2tools_core::HudSchema::parse("x", json.as_bytes()).unwrap();
    let shadowed = SCHEME.replace(
        "    TanDark",
        "    TanLightShadow \"0 0 0 255\"\n    TanDark",
    );
    let files = switch_files(&[("resource/clientscheme.res", &shadowed)]);
    let options = HudOptions::read(&files, Some(&schema), Platform::Linux);
    let switched = options
        .switch(
            &files,
            &[editor("e:s", OptionValue::Rgba([100, 50, 10, 128]))],
        )
        .unwrap();
    let want = shadowed
        .replace("TanLight \"235 226 202 255\"", "TanLight \"100 50 10 128\"")
        .replace(
            "TanLightShadow \"0 0 0 255\"",
            "TanLightShadow \"60 30 6 255\"",
        );
    assert_eq!(text(switched.files(), "resource/clientscheme.res"), want);
}
