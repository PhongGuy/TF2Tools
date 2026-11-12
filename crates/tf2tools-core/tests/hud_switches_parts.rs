//! Options shown on the part they change: found and missing #base paths tie them.

use crate::common;

use common::hud_switches::switch_files;
use tf2tools_core::{HudModel, HudOptions, PartKey, Platform};

const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";

fn ids(options: &HudOptions, model: &HudModel, part: &str) -> Vec<String> {
    options
        .for_part(model, &PartKey::new(part))
        .iter()
        .map(|s| s.id().to_owned())
        .collect()
}

#[test]
fn a_part_lists_its_files_and_its_missing_bases() {
    let files = switch_files(&[]);
    let model = HudModel::load(&files, Platform::Linux).unwrap();
    let got = model.part_files(&PartKey::new(HV));
    assert!(
        got.contains(&"resource/ui/hudplayerhealth.res".to_owned()),
        "{got:?}"
    );
    assert!(
        got.contains(&"#customization/_enabled/bh_player_largehealth.res".to_owned()),
        "{got:?}"
    );
    assert!(model.part_files(&PartKey::new("Nope/Nothing")).is_empty());
}

#[test]
fn an_off_slot_ties_to_the_part_whose_chain_names_it() {
    let files = switch_files(&[]);
    let model = HudModel::load(&files, Platform::Linux).unwrap();
    let options = HudOptions::detected(&files, Platform::Linux);
    assert_eq!(
        ids(&options, &model, HV),
        ["b:#customization/bh_player_largehealth.res"]
    );
}

#[test]
fn an_f_file_ties_through_the_base_that_includes_it() {
    let files = switch_files(&[]);
    let model = HudModel::load(&files, Platform::Linux).unwrap();
    let options = HudOptions::detected(&files, Platform::Linux);
    let part = model
        .parts()
        .iter()
        .find(|p| p.name() == "CDamageAccountPanel")
        .map(|p| p.key().as_str().to_owned())
        .expect("the damage part");
    let tied = ids(&options, &model, &part);
    assert!(
        tied.contains(&"f:customizations/damage.res:0".to_owned()),
        "{tied:?}"
    );
    assert!(
        tied.contains(&"f:customizations/damage.res:resource/ui/damage_last.res".to_owned()),
        "{tied:?}"
    );
}

#[test]
fn a_folder_move_covers_the_files_under_it() {
    let files = switch_files(&[]);
    let options = HudOptions::detected(&files, Platform::Linux);
    let touched = options.touched("c:customizations/streamermode_");
    assert_eq!(
        touched,
        [
            "customizations/streamermode",
            "customizations/streamermode_"
        ]
    );
    let model = HudModel::load(&files, Platform::Linux).unwrap();
    let ammo = model
        .parts()
        .iter()
        .find(|p| p.name() == "AmmoInClip")
        .map(|p| p.key().as_str().to_owned())
        .expect("the ammo part");
    assert_eq!(
        ids(&options, &model, &ammo),
        ["c:customizations/streamermode_"]
    );
}

/// A budhud-like copy: the health file is a `#base`-only stub whose blocks live in a base
/// folder, and the cross option's file sits beside `_enabled` (off) or inside it (on).
fn stub_health_files(on: bool) -> tf2tools_core::HudFiles {
    use common::hud_stock::HEALTH;
    let slot = if on {
        "#customization/_enabled/bh_player_healthcross.res"
    } else {
        "#customization/bh_player_healthcross.res"
    };
    let stub = "#base \"../../#customization/_enabled/bh_player_healthcross.res\"\n#base \"../../_base/resource/ui/hudplayerhealth.res\"\n";
    let cross = "\"Resource/UI/HudPlayerHealth.res\" { PlayerStatusHealthValue { xpos 9 } }\n";
    switch_files(&[
        ("resource/ui/hudplayerhealth.res", stub),
        ("_base/resource/ui/hudplayerhealth.res", HEALTH),
        (slot, cross),
    ])
}

const CROSS_SCHEMA: &str = r##"{ "CustomizationsFolder": "#customization", "EnabledFolder": "#customization//_enabled",
  "Controls": { "Health": [
    { "Name": "bh_cross", "Label": "Health Cross", "Type": "Checkbox", "Value": "false", "FileName": "bh_player_healthcross.res" } ] } }"##;

fn cross_ids(on: bool) -> Vec<String> {
    let files = stub_health_files(on);
    let model = HudModel::load(&files, Platform::Linux).unwrap();
    let schema = tf2tools_core::HudSchema::parse("somehud", CROSS_SCHEMA.as_bytes()).unwrap();
    let options = HudOptions::read(&files, Some(&schema), Platform::Linux);
    ids(&options, &model, HV)
}

#[test]
fn an_off_option_is_offered_on_the_part_a_stub_chain_reaches() {
    let tied = cross_ids(false);
    assert!(tied.contains(&"e:bh_cross".to_owned()), "{tied:?}");
}

#[test]
fn an_on_option_is_still_offered_on_its_part() {
    let tied = cross_ids(true);
    assert!(tied.contains(&"e:bh_cross".to_owned()), "{tied:?}");
}
