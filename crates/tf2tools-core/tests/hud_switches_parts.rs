//! Options shown on the part they change: found and missing #base paths tie them.

mod common;

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
