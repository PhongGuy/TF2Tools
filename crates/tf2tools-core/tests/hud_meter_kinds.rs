//! The meters the HUD tab can preview: one kind per file the game loads, all read by the HUD.

use tf2tools_core::{MeterKind, Scene, SceneRule};

#[test]
fn every_kind_has_a_label_and_its_own_file() {
    assert_eq!(MeterKind::ALL.len(), 21);
    assert_eq!(MeterKind::default(), MeterKind::Plain);
    assert_eq!(MeterKind::ALL[0], MeterKind::Plain);
    assert_eq!(
        MeterKind::Plain.file(),
        "resource/ui/huditemeffectmeter.res"
    );
    assert_eq!(
        MeterKind::Sword.file(),
        "resource/ui/huditemeffectmeter_demoman.res"
    );
    assert_eq!(
        MeterKind::RocketPack.file(),
        "resource/ui/hudrocketpack.res"
    );
    assert_eq!(
        MeterKind::Canteen.file(),
        "resource/ui/huditemeffectmeter_powerupbottle.res"
    );
    assert_eq!(MeterKind::Sword.label(), "Eyelander heads and Air Strike");
    assert_eq!(
        MeterKind::Plain.label(),
        "Sandman, Jarate, banners and most items"
    );
    let mut files: Vec<&str> = MeterKind::ALL.iter().map(|k| k.file()).collect();
    files.sort_unstable();
    files.dedup();
    assert_eq!(files.len(), 21, "no two kinds share a file");
    for kind in MeterKind::ALL {
        assert!(!kind.label().is_empty(), "{kind:?}");
        assert_eq!(kind.file(), kind.file().to_ascii_lowercase(), "{kind:?}");
        assert_eq!(MeterKind::for_file(kind.file()), Some(kind));
    }
    assert_eq!(
        MeterKind::for_file("Resource/UI/HudItemEffectMeter_Demoman.res"),
        Some(MeterKind::Sword)
    );
    assert_eq!(
        MeterKind::for_file("resource/ui/huditemeffectmeter_action.res"),
        None
    );
    assert_eq!(MeterKind::for_file("scripts/hudlayout.res"), None);
}

#[test]
fn the_hud_reads_every_meter_file_and_none_is_a_problem() {
    let hud = SceneRule::get(Scene::Hud);
    for kind in MeterKind::ALL {
        assert!(hud.files().contains(&kind.file()), "{kind:?}");
        assert_eq!(Scene::for_file(kind.file()), Some(Scene::Hud), "{kind:?}");
        let variant = kind != MeterKind::Plain;
        assert_eq!(hud.extras().contains(&kind.file()), variant, "{kind:?}");
        assert_eq!(
            hud.problem_files().contains(&kind.file()),
            !variant,
            "{kind:?}"
        );
    }
}

#[test]
fn the_drink_meter_leaves_out_the_medi_gun_which_has_its_own_charge_meter() {
    assert_eq!(MeterKind::Drink.label(), "Bonk!, Crit-a-Cola and Mad Milk");
}
