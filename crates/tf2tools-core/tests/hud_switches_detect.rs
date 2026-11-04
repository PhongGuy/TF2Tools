//! Detected switches: F groups and toggles, manifest lines, C renames, B and B' slots.

mod common;

use common::hud_stock::stock_files;
use common::hud_switches::switch_files;
use tf2tools_core::{HudOptions, OptionKind, OptionSource, OptionSpec, OptionValue, Platform};

fn spec<'a>(options: &'a HudOptions, id: &str) -> &'a OptionSpec {
    options
        .specs()
        .iter()
        .find(|s| s.id() == id)
        .unwrap_or_else(|| panic!("no {id}"))
}

#[test]
fn a_group_that_says_only_one_is_a_choice_labelled_by_its_comments() {
    let options = HudOptions::detected(&switch_files(&[]), Platform::Linux);
    let s = spec(&options, "f:customizations/damage.res:0");
    assert_eq!(
        (s.label(), s.group(), s.source()),
        ("Damage style", "Damage", &OptionSource::Detected)
    );
    let OptionKind::Choice(choices) = s.kind() else {
        panic!("not a choice")
    };
    let shown: Vec<(&str, &str)> = choices.iter().map(|c| (c.key(), c.label())).collect();
    assert_eq!(
        shown,
        [
            ("resource/ui/damage_small.res", "Small"),
            ("resource/ui/damage_medium.res", "Medium"),
            ("resource/ui/damage_big.res", "Damage big"),
        ]
    );
    assert_eq!(
        s.default(),
        &OptionValue::Pick("resource/ui/damage_medium.res".into())
    );
}

#[test]
fn a_group_without_only_one_gives_toggles() {
    let options = HudOptions::detected(&switch_files(&[]), Platform::Linux);
    let s = spec(
        &options,
        "f:customizations/damage.res:resource/ui/damage_last.res",
    );
    assert_eq!(
        (s.label(), s.group(), s.kind()),
        ("Last damage", "Damage", &OptionKind::Toggle)
    );
    assert_eq!(s.default(), &OptionValue::Bool(false));
}

#[test]
fn a_one_of_group_with_nothing_on_starts_with_none() {
    let all_off = common::hud_switches::DAMAGE_SWITCH.replace("\n#base", "\n//#base");
    let options = HudOptions::detected(
        &switch_files(&[("customizations/damage.res", &all_off)]),
        Platform::Linux,
    );
    let s = spec(&options, "f:customizations/damage.res:0");
    let OptionKind::Choice(choices) = s.kind() else {
        panic!("not a choice")
    };
    assert_eq!((choices[0].key(), choices[0].label()), ("", "None"));
    assert_eq!(s.default(), &OptionValue::Pick(String::new()));
}

#[test]
fn commented_lines_outside_customization_folders_or_with_missing_targets_are_not_switches() {
    let health = format!("//#base \"damage_big.res\"\n{}", common::hud_stock::HEALTH);
    let files = switch_files(&[
        ("resource/ui/hudplayerhealth.res", &health),
        (
            "customizations/extra.res",
            "//#base \"../resource/ui/gone.res\"\n",
        ),
    ]);
    let options = HudOptions::detected(&files, Platform::Linux);
    assert!(
        options
            .specs()
            .iter()
            .all(|s| !s.id().starts_with("f:resource/")
                && !s.id().starts_with("f:customizations/extra.res"))
    );
}

#[test]
fn a_commented_manifest_line_whose_file_exists_is_a_toggle() {
    let manifest = "hudanimations_manifest\n{\n\t\"file\"\t\"scripts/hudanimations_tf.txt\"\n\t//\"file\"\t\"scripts/hudanimations_hitmarker.txt\"\n\t//\"file\"\t\"scripts/gone.txt\"\n}\n";
    let files = switch_files(&[
        ("scripts/hudanimations_manifest.txt", manifest),
        (
            "scripts/hudanimations_hitmarker.txt",
            "event HitMarker\n{\n}\n",
        ),
    ]);
    let options = HudOptions::detected(&files, Platform::Linux);
    let s = spec(&options, "m:scripts/hudanimations_hitmarker.txt");
    assert_eq!(
        (s.label(), s.group(), s.default()),
        (
            "Hudanimations hitmarker",
            "Switches",
            &OptionValue::Bool(false)
        )
    );
    assert!(
        options
            .specs()
            .iter()
            .all(|s| s.id() != "m:scripts/gone.txt")
    );
}

#[test]
fn files_that_are_no_copy_have_no_switches() {
    assert!(
        HudOptions::detected(&stock_files(), Platform::Linux)
            .specs()
            .is_empty()
    );
}

fn moves(s: &OptionSpec) -> Vec<(String, String)> {
    let actions = match s.kind() {
        OptionKind::Choice(choices) => choices.iter().flat_map(|c| c.actions().to_vec()).collect(),
        _ => s.actions().to_vec(),
    };
    actions
        .into_iter()
        .filter_map(|a| match a {
            tf2tools_core::OptionAction::Switch(tf2tools_core::SwitchAction::Move { from, to }) => {
                Some((from, to))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn an_enabled_slot_moves_its_option_in_and_one_shipped_on_starts_on() {
    let options = HudOptions::detected(&switch_files(&[]), Platform::Linux);
    let off = spec(&options, "b:#customization/bh_player_largehealth.res");
    assert_eq!(
        (off.label(), off.group(), off.default()),
        (
            "Bh player largehealth",
            "Switches",
            &OptionValue::Bool(false)
        )
    );
    assert_eq!(
        moves(off),
        [(
            "#customization/bh_player_largehealth.res".into(),
            "#customization/_enabled/bh_player_largehealth.res".into()
        )]
    );
    let on = spec(&options, "b:#customization/bh_chat_top.res");
    assert_eq!(on.default(), &OptionValue::Bool(true));
}

#[test]
fn an_underscore_folder_named_without_it_is_a_rename() {
    let options = HudOptions::detected(&switch_files(&[]), Platform::Linux);
    let s = spec(&options, "c:customizations/streamermode_");
    assert_eq!(
        (s.label(), s.default()),
        ("Streamermode", &OptionValue::Bool(false))
    );
    assert_eq!(
        moves(s),
        [(
            "customizations/streamermode_".into(),
            "customizations/streamermode".into()
        )]
    );
}

#[test]
fn a_bang_folder_under_disabled_is_one_of() {
    let options = HudOptions::detected(&switch_files(&[]), Platform::Linux);
    let s = spec(&options, "b:#customization/_disabled/! scoreboards");
    assert_eq!(s.label(), "Scoreboards");
    let OptionKind::Choice(choices) = s.kind() else {
        panic!("not a choice")
    };
    let keys: Vec<&str> = choices.iter().map(|c| c.key()).collect();
    assert_eq!(
        keys,
        [
            "",
            "#customization/_disabled/! scoreboards/24p scoreboard",
            "#customization/_disabled/! scoreboards/32p scoreboard"
        ]
    );
    assert_eq!(s.default(), &OptionValue::Pick(String::new()));
}

#[test]
fn a_climbing_target_is_never_a_switch() {
    let ammo = format!(
        "#base \"../../../../cfg/somehud/ammo.res\"\n{}",
        common::hud_stock::AMMO
    );
    let files = switch_files(&[
        ("resource/ui/hudammoweapons.res", &ammo),
        ("cfg/somehud_/ammo.res", "x"),
    ]);
    let options = HudOptions::detected(&files, Platform::Linux);
    assert!(options.specs().iter().all(|s| !s.id().contains("cfg")));
}

#[test]
fn detection_stops_at_the_option_cap_and_counts_the_rest() {
    let mut extra = Vec::new();
    for i in 0..600 {
        extra.push((
            format!("customizations/many_{i}.res"),
            format!("//#base \"../resource/ui/damage_big.res\" // OPTION {i}\n"),
        ));
    }
    let refs: Vec<(&str, &str)> = extra
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    let options = HudOptions::detected(&switch_files(&refs), Platform::Linux);
    assert_eq!(options.specs().len(), 512);
    // At least the 600 added toggles minus the cap are refused, whatever the fixture adds.
    assert!(options.refused() >= 600 - 512);
}

#[test]
fn a_rename_into_the_metadata_folder_is_never_a_switch() {
    // `.tf2tools/maker.res` is missing and `.tf2tools_` exists: a C rename would move the folder
    // `.tf2tools_` to `.tf2tools` (no trailing slash); `_.tf2tools` would do the same in any case.
    let health = format!(
        "#base \"../../.tf2tools/maker.res\"\n#base \"../../.TF2Tools/x/a.res\"\n{}",
        common::hud_stock::HEALTH
    );
    let files = switch_files(&[
        ("resource/ui/hudplayerhealth.res", &health),
        (".tf2tools_/maker.res", "\"R\" { }"),
        ("_.tf2tools/x/a.res", "\"R\" { }"),
        (
            "customizations/meta.res",
            "//#base \"../.tf2tools/maker.res\"\n",
        ),
    ]);
    let options = HudOptions::detected(&files, Platform::Linux);
    let meta = |path: &str| {
        path.split('/')
            .any(|seg| seg.eq_ignore_ascii_case(".tf2tools"))
    };
    for s in options.specs() {
        for (from, to) in moves(s) {
            assert!(!meta(&from) && !meta(&to), "{}: {from} -> {to}", s.id());
        }
    }
    assert!(
        options
            .specs()
            .iter()
            .all(|s| !s.id().starts_with("f:customizations/meta.res"))
    );
}

#[test]
fn files_past_the_scan_caps_are_refused_and_counted() {
    // 8,200 empty files take the copy past the 8,192-file scan cap.
    let many: Vec<(String, String)> = (0..8_200)
        .map(|i| (format!("scripts/many/f{i:05}.txt"), String::new()))
        .collect();
    let refs: Vec<(&str, &str)> = many.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
    let options = HudOptions::detected(&switch_files(&refs), Platform::Linux);
    assert!(options.refused() >= 8, "refused {}", options.refused());

    // Four files of 22 MiB take it past the 64 MiB scan cap.
    let big = "x".repeat(22 * 1024 * 1024);
    let names: Vec<String> = (0..4).map(|i| format!("scripts/big{i}.txt")).collect();
    let refs: Vec<(&str, &str)> = names.iter().map(|p| (p.as_str(), big.as_str())).collect();
    let options = HudOptions::detected(&switch_files(&refs), Platform::Linux);
    assert!(options.refused() >= 1, "refused {}", options.refused());
}
