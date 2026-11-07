//! The made-up player the Spectator tab watches, and the colour the code gives its labels.

use crate::common;

use common::hud_stock::{TARGET_ID, target_id_texts};
use tf2tools_core::{HudModel, PartKey, Platform, Rgba, SampleSet, Scene, Team};

fn k(text: &str) -> PartKey {
    PartKey::new(text)
}

#[test]
fn the_sample_watches_a_red_medic() {
    let w = SampleSet::Spectating.watched().expect("watched");
    assert_eq!(
        (w.name(), w.class(), w.health(), w.max_health()),
        ("Dr. Pancake", "Medic", 118, 150)
    );
    assert_eq!(w.team(), Team::Red);
    assert_eq!(w.data(), "ÜberCharge: 42%");
    for set in [
        SampleSet::Empty,
        SampleSet::Casual,
        SampleSet::Killed,
        SampleSet::Won,
    ] {
        assert!(set.watched().is_none());
    }
}

#[test]
fn the_variables_carry_the_players_name_line_and_health() {
    let w = SampleSet::Spectating.watched().unwrap();
    let fill = |t| SampleSet::Spectating.fill(t);
    assert_eq!(fill("%targetname%"), w.name());
    assert_eq!(fill("%targetdata%"), w.data());
    assert_eq!(fill("%Health%"), w.health().to_string());
    assert_eq!(fill("%HEALTH%"), "118");
    // The other samples do not know the tokens, and the old ones still work.
    assert_eq!(SampleSet::Casual.fill("%targetname%"), "");
    assert_eq!(fill("[%jump%]"), "[SPACE]");
}

#[test]
fn the_labels_take_the_schemes_label_colour_whatever_the_file_says() {
    let red = TARGET_ID.replace(
        "labelText %targetname%",
        "fgcolor \"255 0 0 255\" fgcolor_override \"255 0 0 255\" labelText %targetname%",
    );
    assert_ne!(red, TARGET_ID);
    let mut texts = target_id_texts();
    texts.retain(|(p, _)| *p != "resource/ui/targetid.res");
    let mut over: Vec<(&str, Option<&str>)> =
        texts.iter().map(|(p, t)| (*p, Some(t.as_str()))).collect();
    over.push(("resource/ui/targetid.res", Some(&red)));
    let m = HudModel::load_scene(
        &common::hud_stock::stock_files_with(&over),
        Platform::Linux,
        Scene::Spectator,
    )
    .unwrap();
    // TanLight in the fixture scheme.
    let tan = Rgba::new(235, 226, 202, 255);
    assert_eq!(
        m.text_colour(m.part(&k("CSpectatorTargetID/TargetNameLabel")).unwrap()),
        tan
    );
    assert_eq!(
        m.text_colour(m.part(&k("CSpectatorTargetID/TargetDataLabel")).unwrap()),
        tan
    );
    // The HUD view of the same file draws the same colour: the game recolours those labels too.
    let hud = HudModel::load_scene(
        &common::hud_stock::stock_files_with(&over),
        Platform::Linux,
        Scene::Hud,
    )
    .unwrap();
    for name in ["TargetNameLabel", "TargetDataLabel"] {
        let key = k(&format!("CMainTargetID/{name}"));
        let owns = |slot| hud.part(&key).unwrap().code_toggled().unwrap().owns(slot);
        assert!(owns("fgcolor") && owns("fgcolor_override"), "{name}");
        assert!(!hud.capabilities(&key).unwrap().recolour(), "{name}");
        assert_eq!(hud.text_colour(hud.part(&key).unwrap()), tan, "{name}");
    }
}
