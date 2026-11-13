//! The made-up class menu the class select scene previews, and what the game's code sets on it.

use crate::common;

use common::hud_stock::{class_select, stock_files, stock_files_with};
use tf2tools_core::{
    Backdrop, HudModel, PartKey, Platform, Rgba, SampleSet, Scene, SceneRule, Screen, Team, Vision,
};

#[test]
fn the_scene_previews_one_made_up_class_pick() {
    assert_eq!(
        SceneRule::get(Scene::ClassSelect).sample(),
        SampleSet::ClassPick
    );
    for team in [Team::Red, Team::Blue] {
        assert!(SampleSet::ClassPick.players(team).is_empty());
        assert_eq!(SampleSet::ClassPick.team_score(team), None);
    }
    assert!(SampleSet::ClassPick.killer().is_none());
    for name in ["scout", "Soldier", "heavyweapons", "random"] {
        assert!(SampleSet::ClassPick.is_class_button(name), "{name}");
    }
    assert!(!SampleSet::ClassPick.is_class_button("numScout"));
    assert!(!SampleSet::Casual.is_class_button("soldier"));
}

#[test]
fn the_counts_are_the_casual_sample_s_red_team_by_class() {
    let count = |class: &str| {
        SampleSet::Casual
            .players(Team::Red)
            .iter()
            .filter(|p| p.class() == class)
            .count()
    };
    let want = [
        ("%numScout%", "scout"),
        ("%numSoldier%", "soldier"),
        ("%numPyro%", "pyro"),
        ("%numDemoman%", "demoman"),
        ("%numHeavy%", "heavy"),
        ("%numEngineer%", "engineer"),
        ("%numMedic%", "medic"),
        ("%numSniper%", "sniper"),
        ("%numSpy%", "spy"),
    ];
    for (token, class) in want {
        assert_eq!(
            SampleSet::ClassPick.fill(token),
            count(class).to_string(),
            "{token}"
        );
    }
    assert_eq!(SampleSet::ClassPick.fill("%numsoldier%"), "3");
    // The local player is one of the twelve: eleven teammate pictures remain.
    assert_eq!(SampleSet::Casual.players(Team::Red).len(), 12);
    assert_eq!(SampleSet::Casual.fill("%numSoldier%"), "");
    assert_eq!(SampleSet::Empty.fill("%numSoldier%"), "");
}

#[test]
fn the_code_sets_the_selected_button_the_teammate_row_and_the_local_picture() {
    let image = |part: &str, parent: Option<&str>| SampleSet::ClassPick.code_image(part, parent);
    assert_eq!(
        image("SubImage", Some("soldier")),
        Some("class_sel_sm_soldier_red")
    );
    assert_eq!(
        image("subimage", Some("Soldier")),
        Some("class_sel_sm_soldier_red")
    );
    assert_eq!(image("SubImage", Some("scout")), None);
    assert_eq!(image("SubImage", None), None);
    assert_eq!(
        image("localPlayerImage", None),
        Some("class_sel_sm_soldier_red")
    );
    let row: Vec<_> = (0..11)
        .map(|i| image(&format!("countImage{i}"), None).unwrap())
        .collect();
    assert_eq!(
        row,
        [
            "class_sel_sm_scout_red",
            "class_sel_sm_soldier_red",
            "class_sel_sm_soldier_red",
            "class_sel_sm_pyro_red",
            "class_sel_sm_demo_red",
            "class_sel_sm_heavy_red",
            "class_sel_sm_engineer_red",
            "class_sel_sm_medic_red",
            "class_sel_sm_medic_red",
            "class_sel_sm_sniper_red",
            "class_sel_sm_spy_red",
        ]
    );
    assert_eq!(image("countImage11", None), None);
    assert_eq!(
        SampleSet::Casual.code_image("SubImage", Some("soldier")),
        None
    );
    assert_eq!(SampleSet::Empty.code_image("countImage0", None), None);
}

#[test]
fn the_backdrop_room_has_the_game_s_light_wall_under_a_dark_top_bar() {
    // Measured in game on the default HUD at 1920x1080: the wall about 181 174 168, the bar
    // above the class buttons 49 47 46 down to 111 of the screen's 480 rows.
    let wall = Rgba::new(181, 174, 168, 255);
    let bar = Rgba::new(49, 47, 46, 255);
    assert_eq!(SampleSet::ClassPick.code_fill("MenuBG"), Some(wall));
    assert_eq!(SampleSet::ClassPick.code_fill("menubg"), Some(wall));
    assert_eq!(
        SampleSet::ClassPick.code_band("MenuBG"),
        Some((bar, 111. / 480.))
    );
    assert_eq!(SampleSet::ClassPick.code_fill("ShadedBar"), None);
    assert_eq!(SampleSet::ClassPick.code_band("ShadedBar"), None);
    assert_eq!(SampleSet::Casual.code_fill("MenuBG"), None);
    assert_eq!(SampleSet::Casual.code_band("MenuBG"), None);
    assert_eq!(SampleSet::Spectating.code_band("topbar"), None);
    // The stock MenuBG has no colour key, so the room's wall is what the part fills with.
    let files = stock_files();
    let m = HudModel::load_scene(&files, Platform::Linux, Scene::ClassSelect).unwrap();
    let bg = m.part(&PartKey::new("class/MenuBG")).unwrap();
    assert_eq!(m.fill_colour(bg), Some(wall));
}

#[test]
fn the_counts_over_the_top_bar_are_judged_against_the_bar_not_the_wall() {
    // White count text is about 2.2 to 1 on the light wall but 12 to 1 on the dark bar. The
    // labels take the stock file's zpos, so they paint over the room.
    let class = class_select()
        .replace(
            "numScout { ControlName CExLabel",
            "numScout { ControlName CExLabel zpos 5",
        )
        .replace(
            "CountLabel { ControlName CExLabel",
            "CountLabel { ControlName CExLabel zpos 10",
        );
    assert_ne!(class, class_select());
    let files = stock_files_with(&[("resource/ui/classselection.res", Some(&class))]);
    let m = HudModel::load_scene(&files, Platform::Linux, Scene::ClassSelect).unwrap();
    let layout = m.layout(Screen::new(1920, 1080));
    let black = Backdrop::new(1, 1, vec![0, 0, 0, 255]).unwrap();
    let hard = m.hard_to_read(&layout, &black, Vision::Normal);
    assert!(!hard.contains(&PartKey::new("class/numScout")), "{hard:?}");
    assert!(hard.contains(&PartKey::new("class/CountLabel")), "{hard:?}");
}

#[test]
fn the_tips_box_is_drawn_in_game() {
    assert_eq!(
        SampleSet::ClassPick.game_filled("ClassTipsPanel"),
        Some("Class tips draw in game")
    );
    assert_eq!(SampleSet::ClassPick.game_filled("TFPlayerModel"), None);
    assert_eq!(SampleSet::Empty.game_filled("ClassTipsPanel"), None);
    // Each named part exists in the scene, so the lookups have something to draw on.
    let files = stock_files();
    let m = HudModel::load_scene(&files, Platform::Linux, Scene::ClassSelect).unwrap();
    for key in [
        "class/MenuBG",
        "class/ClassTipsPanel",
        "class/localPlayerImage",
        "class/countImage10",
    ] {
        assert!(m.part(&PartKey::new(key)).is_some(), "{key}");
    }
}

#[test]
fn only_the_class_menu_s_team_count_line_grows_to_its_text() {
    assert!(SampleSet::ClassPick.sizes_to_text("CountLabel"));
    assert!(!SampleSet::ClassPick.sizes_to_text("numScout"));
    assert!(!SampleSet::Empty.sizes_to_text("CountLabel"));
}
