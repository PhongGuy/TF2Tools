//! A file two scenes read: who owns it, and that one change list shows in both.

use crate::common;

use common::hud_stock::target_id_files;
use tf2tools_core::{ChangeTarget, HudModel, PartKey, Platform, Scene};

const TARGET: &str = "resource/ui/targetid.res";

fn k(text: &str) -> PartKey {
    PartKey::new(text)
}

#[test]
fn a_shared_file_belongs_to_the_open_scene_first() {
    assert_eq!(Scene::for_file(TARGET), Some(Scene::Hud));
    assert_eq!(
        Scene::for_file_near(TARGET, Scene::Spectator),
        Some(Scene::Spectator)
    );
    assert_eq!(
        Scene::for_file_near("Resource/UI/TargetID.res", Scene::Spectator),
        Some(Scene::Spectator)
    );
    assert_eq!(Scene::for_file_near(TARGET, Scene::Hud), Some(Scene::Hud));
    // A scene that does not read the file leaves it to the first one that does.
    assert_eq!(
        Scene::for_file_near(TARGET, Scene::WinPanel),
        Some(Scene::Hud)
    );
    assert_eq!(
        Scene::for_file_near("scripts/hudlayout.res", Scene::WinPanel),
        Some(Scene::WinPanel)
    );
    assert_eq!(
        Scene::for_file_near("scripts/hudlayout.res", Scene::Spectator),
        Some(Scene::Spectator)
    );
    assert_eq!(
        Scene::for_file_near("resource/ui/spectator.res", Scene::Hud),
        Some(Scene::Spectator)
    );
    assert_eq!(Scene::for_file_near("nope.res", Scene::Spectator), None);
}

#[test]
fn a_font_set_on_the_spectator_tab_shows_on_the_hud_tab_and_back() {
    let spec = HudModel::load_scene(&target_id_files(), Platform::Linux, Scene::Spectator).unwrap();
    let changes = spec
        .set_property(
            &k("CSpectatorTargetID/TargetNameLabel"),
            "font",
            "HudFontMedium",
            1,
        )
        .unwrap();
    for c in &changes {
        let ChangeTarget::Set(s) = c.target() else {
            panic!("not a set")
        };
        assert_eq!(s.file(), TARGET);
    }
    let font = |m: &HudModel, key: &str| {
        m.part(&k(key))
            .and_then(|p| p.value("font"))
            .map(|l| l.text().to_owned())
    };
    let edited = spec.with_changes(&changes);
    assert_eq!(edited.dropped(), 0);
    let spec_after = edited.into_model();
    let hud = spec.with_scene(Scene::Hud).with_changes(&changes);
    assert_eq!(hud.dropped(), 0);
    let hud_after = hud.into_model();
    assert_eq!(
        font(&spec_after, "CSpectatorTargetID/TargetNameLabel").as_deref(),
        Some("HudFontMedium")
    );
    assert_eq!(
        font(&hud_after, "CMainTargetID/TargetNameLabel").as_deref(),
        Some("HudFontMedium")
    );
    // The data label next to it is untouched in both.
    assert_eq!(
        font(&hud_after, "CMainTargetID/TargetDataLabel").as_deref(),
        Some("HudFontSmall")
    );
    // And the other way: a change made on the HUD tab replays on the Spectator tab.
    let back = hud_after
        .with_scene(Scene::Spectator)
        .with_changes(&changes)
        .into_model();
    assert_eq!(
        font(&back, "CSpectatorTargetID/TargetNameLabel").as_deref(),
        Some("HudFontMedium")
    );
}
