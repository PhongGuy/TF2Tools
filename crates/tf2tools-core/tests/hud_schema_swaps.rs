//! HUD options that swap folders and files into a HUD, pick its menu background, switch to
//! the game's stock backgrounds, or take free text, over small in-memory copies.

use tf2tools_core::{
    Change, HudFiles, HudOptions, HudSchema, HudSchemas, OptionChange, OptionKind, OptionValue,
    Platform,
};

fn bundled(name: &str) -> HudSchema {
    HudSchemas::bundled().for_name(name).unwrap().clone()
}

fn copy(files: &[(&str, &[u8])]) -> HudFiles {
    let layer = (files.iter())
        .map(|(p, t)| ((*p).to_owned(), t.to_vec()))
        .collect();
    HudFiles::from_memory(vec![layer]).with_install_folder("hud")
}

fn choose(id: &str, value: OptionValue) -> Change {
    Change::option(1, OptionChange::new(OptionChange::SCHEMA, id, value))
}

fn bytes(files: &HudFiles, path: &str) -> Option<Vec<u8>> {
    files.read(path).unwrap().map(|f| f.into_bytes())
}

/// A m0rehud-classic copy with two of its font folders under `customizations`.
fn m0rehud() -> HudFiles {
    copy(&[
        ("resource/clientscheme.res", b"surface"),
        (
            "customizations/fonts/[font] avenir/resource/clientscheme.res",
            b"avenir",
        ),
        ("customizations/fonts/[font] avenir/resource/ui/x.res", b"x"),
        ("customizations/fonts/[font] avenir/preview.png", b"png"),
        (
            "customizations/fonts/[font] lato/resource/clientscheme.res",
            b"lato",
        ),
    ])
}

const FONT: &str = "e:mh_font";

#[test]
fn a_folder_choice_copies_its_files_over_the_hud_and_undo_takes_them_back() {
    let files = m0rehud();
    let options = HudOptions::read(&files, Some(&bundled("m0rehud-classic")), Platform::Linux);
    let font = options.specs().iter().find(|s| s.id() == FONT).unwrap();
    let OptionKind::Choice(choices) = font.kind() else {
        panic!("the font is a pick");
    };
    assert!(choices.iter().any(|c| c.label() == "Avenir"));
    // Folders the copy lacks are no choice.
    assert!(choices.iter().all(|c| c.label() != "Roboto"));

    let avenir = choices
        .iter()
        .find(|c| c.label() == "Avenir")
        .unwrap()
        .key();
    let switched = options
        .switch(&files, &[choose(FONT, OptionValue::Pick(avenir.into()))])
        .unwrap();
    let view = switched.files();
    assert_eq!(bytes(view, "resource/clientscheme.res").unwrap(), b"avenir");
    assert_eq!(bytes(view, "resource/ui/x.res").unwrap(), b"x");
    // Preview pictures stay behind, and the option folder stays where it is.
    assert!(bytes(view, "preview.png").is_none());
    assert!(bytes(view, "customizations/fonts/[font] avenir/resource/ui/x.res").is_some());

    let undone = options.switch(&files, &[]).unwrap();
    assert_eq!(
        bytes(undone.files(), "resource/clientscheme.res").unwrap(),
        b"surface"
    );
    assert!(undone.files().switched_paths().0.is_empty());
}

#[test]
fn a_file_choice_moves_the_picked_file_in_and_the_others_back() {
    let files = copy(&[
        ("resource/ui/hudplayerhealth.res", b"\"x\" { }"),
        ("customizations/hudplayerhealth_broesel.res", b"broesel"),
        ("customizations/hudplayerhealth_cross.res", b"cross"),
    ]);
    let schema = r#"{ "CustomizationsFolder": "customizations", "EnabledFolder": "customizations//_enabled", "Controls": { "G": [
        { "Name": "hs", "Label": "Health Style", "Type": "ComboBox", "Value": "0",
          "ComboFiles": [ "hudplayerhealth_broesel.res", "hudplayerhealth_cross" ],
          "Options": [ { "Label": "Default", "Value": "0" },
                       { "Label": "Broesel", "Value": "1", "FileName": "hudplayerhealth_broesel.res" },
                       { "Label": "Cross", "Value": "2", "FileName": "hudplayerhealth_cross" } ] } ] } }"#;
    let schema = HudSchema::parse("one", schema.as_bytes()).unwrap();
    let options = HudOptions::read(&files, Some(&schema), Platform::Linux);
    assert_eq!(options.left_out(), 0);
    let switched = options
        .switch(&files, &[choose("e:hs", OptionValue::Pick("2".into()))])
        .unwrap();
    let view = switched.files();
    assert_eq!(
        bytes(view, "customizations/_enabled/hudplayerhealth_cross.res").unwrap(),
        b"cross"
    );
    assert!(bytes(view, "customizations/hudplayerhealth_cross.res").is_none());
    assert!(bytes(view, "customizations/hudplayerhealth_broesel.res").is_some());
}

#[test]
fn the_bundled_file_swaps_are_read() {
    let files = copy(&[
        ("#customizations/addons/health-ammo-style_ut/a.res", b"ut"),
        (
            "#customizations/addons/health-ammo_font_futura.res",
            b"futura",
        ),
    ]);
    let options = HudOptions::read(&files, Some(&bundled("zeeshud")), Platform::Linux);
    let ids: Vec<&str> = options.specs().iter().map(|s| s.id()).collect();
    assert!(ids.contains(&"e:zh_val_layout_style"), "{ids:?}");
    assert!(ids.contains(&"e:zh_val_layout_font"), "{ids:?}");
    let switched = options
        .switch(
            &files,
            &[choose(
                "e:zh_val_layout_font",
                OptionValue::Pick("1".into()),
            )],
        )
        .unwrap();
    assert!(
        bytes(
            switched.files(),
            "#customizations/addons/_enabled/health-ammo_font_futura.res"
        )
        .is_some()
    );
}

/// A rayshud copy that ships the classic background as its menu art.
fn rayshud() -> HudFiles {
    copy(&[
        ("materials/console/background_upward.vtf", b"classic"),
        ("materials/console/background_upward.vmt", b"vmt"),
        (
            "materials/console/background_upward_widescreen.vmt",
            b"wide vmt",
        ),
        (
            "materials/console/_disabled/background_modern.vtf",
            b"modern",
        ),
        (
            "materials/console/_disabled/background_classic.vtf",
            b"classic",
        ),
        ("scripts/chapterbackgrounds.txt", b"old"),
    ])
}

const MENU_BG: &str = "e:rh_val_main_menu_bg";

#[test]
fn the_menu_background_starts_at_the_shipped_art() {
    let files = rayshud();
    let options = HudOptions::read(&files, Some(&bundled("rayshud")), Platform::Linux);
    let spec = options.specs().iter().find(|s| s.id() == MENU_BG).unwrap();
    let OptionKind::Choice(choices) = spec.kind() else {
        panic!("the menu background is a pick");
    };
    let labels: Vec<&str> = choices.iter().map(|c| c.label()).collect();
    assert_eq!(labels, ["Modern", "Classic", "Default"]);
    assert_eq!(
        options.value(MENU_BG, &[]),
        Some(OptionValue::Pick("1".into()))
    );
}

#[test]
fn a_shipped_background_is_copied_into_the_menu_art() {
    let files = rayshud();
    let options = HudOptions::read(&files, Some(&bundled("rayshud")), Platform::Linux);
    let switched = options
        .switch(&files, &[choose(MENU_BG, OptionValue::Pick("0".into()))])
        .unwrap();
    let view = switched.files();
    assert_eq!(
        bytes(view, "materials/console/background_upward.vtf").unwrap(),
        b"modern"
    );
    let chapters =
        String::from_utf8(bytes(view, "scripts/chapterbackgrounds.txt").unwrap()).unwrap();
    assert!(chapters.contains("background_upward"));
    assert!(chapters.contains("BackgroundMaps"));
}

#[test]
fn the_stock_background_choice_hides_the_hud_art() {
    let files = rayshud();
    let options = HudOptions::read(&files, Some(&bundled("rayshud")), Platform::Linux);
    let switched = options
        .switch(&files, &[choose(MENU_BG, OptionValue::Pick("2".into()))])
        .unwrap();
    let view = switched.files();
    assert!(bytes(view, "materials/console/background_upward.vtf").is_none());
    assert!(bytes(view, "materials/console/background_upward_widescreen.vmt").is_none());
    // Shipped art the HUD keeps aside is no menu art, so it stays.
    assert!(bytes(view, "materials/console/_disabled/background_modern.vtf").is_some());
    let back = options.switch(&files, &[]).unwrap();
    assert!(back.files().switched_paths().1.is_empty());
}

#[test]
fn the_stock_background_switch_reads_its_start_and_hides_the_art_when_on() {
    let files = copy(&[
        ("materials/console/background_upward.vtf", b"art"),
        (
            "resource/ui/mainmenuoverride.res",
            b"\"x\" { \"Background\" { \"image\" \"x\" \"if_halloween\" { \"image\" \"\" } \"if_christmas\" { \"image\" \"\" } } }",
        ),
    ]);
    let options = HudOptions::read(&files, Some(&bundled("flawhud")), Platform::Linux);
    let id = "e:fh_toggle_background_stock";
    assert_eq!(options.value(id, &[]), Some(OptionValue::Bool(false)));
    let switched = options
        .switch(&files, &[choose(id, OptionValue::Bool(true))])
        .unwrap();
    assert!(bytes(switched.files(), "materials/console/background_upward.vtf").is_none());
}

const FAVOURITE: &str = "e:fh_favorite_server";

fn gamemenu() -> HudFiles {
    copy(&[(
        "resource/gamemenu.res",
        b"\"GameMenu\"\n{\n\t\"HomeServerButton\"\n\t{\n\t\t\"label\" \"Home\"\n\t\t\"command\" \"engine connect 1.2.3.4\"\n\t}\n}\n",
    )])
}

#[test]
fn the_text_box_writes_its_text_into_the_key() {
    let files = gamemenu();
    let options = HudOptions::read(&files, Some(&bundled("flawhud")), Platform::Linux);
    let spec = options
        .specs()
        .iter()
        .find(|s| s.id() == FAVOURITE)
        .unwrap();
    assert_eq!(*spec.kind(), OptionKind::Text);
    let next = options
        .set(
            FAVOURITE,
            OptionValue::Text("my.server:27015".into()),
            &[],
            1,
        )
        .unwrap();
    let switched = options.switch(&files, &next).unwrap();
    let text =
        String::from_utf8(bytes(switched.files(), "resource/gamemenu.res").unwrap()).unwrap();
    assert!(
        text.contains("\"engine connect my.server:27015\""),
        "{text}"
    );
}

#[test]
fn text_that_could_break_the_file_is_refused() {
    let files = gamemenu();
    let options = HudOptions::read(&files, Some(&bundled("flawhud")), Platform::Linux);
    for bad in ["a\"b", "a\nb", "a{b", &"x".repeat(300)] {
        assert!(
            options
                .set(FAVOURITE, OptionValue::Text(bad.into()), &[], 1)
                .is_err(),
            "{bad} was taken"
        );
    }
}
