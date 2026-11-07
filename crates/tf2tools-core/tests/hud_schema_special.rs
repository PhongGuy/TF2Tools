//! HUD options with special handling, using the bundled flawhud and hud-fixes schemas over
//! small in-memory copies: the transparent viewmodels switch is read and marked as needing the
//! addon, while a custom menu background stays left out.

use tf2tools_core::{
    Change, HudFiles, HudOptions, HudSchema, HudSchemas, OptionAction, OptionChange, OptionKind,
    OptionNeed, OptionValue, Platform,
};

const FLAW_VIEWMODELS: &str = "e:fh_toggle_transparent_viewmodels";
const FIXES_VIEWMODELS: &str = "e:tf_toggle_transparent_viewmodels";

fn bundled(name: &str) -> HudSchema {
    HudSchemas::bundled().for_name(name).unwrap().clone()
}

/// A copy installed as `folder` whose HUD layout reads the viewmodels file from `tf/cfg`.
fn copy(folder: &str) -> HudFiles {
    let layout = format!(
        "#base \"../../../cfg/{folder}/hud_transparent_viewmodels.txt\"\n\"Resource/HudLayout.res\" {{ }}\n"
    );
    HudFiles::from_memory(vec![vec![(
        "scripts/hudlayout.res".to_owned(),
        layout.into_bytes(),
    )]])
    .with_install_folder(folder)
}

fn choose(id: &str, on: bool) -> Change {
    Change::option(
        1,
        OptionChange::new(OptionChange::SCHEMA, id, OptionValue::Bool(on)),
    )
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
fn the_viewmodels_switch_is_read_and_needs_the_addon() {
    for (schema, folder, id) in [
        ("flawhud", "flawhud", FLAW_VIEWMODELS),
        ("hud-fixes", "hud-fixes", FIXES_VIEWMODELS),
    ] {
        let options = HudOptions::read(&copy(folder), Some(&bundled(schema)), Platform::Linux);
        let spec = (options.specs().iter())
            .find(|s| s.id() == id)
            .unwrap_or_else(|| panic!("{schema} offers its viewmodels switch"));
        assert_eq!(spec.label(), "Transparent Viewmodels");
        assert_eq!(*spec.kind(), OptionKind::Toggle);
        assert_eq!(*spec.default(), OptionValue::Bool(false));
        assert_eq!(spec.needs(), Some(OptionNeed::TransparentViewmodels));
        // The special adds no edit of its own: the HUD side is the settings file alone.
        assert!(matches!(spec.actions(), [OptionAction::WriteCfg { .. }]));
        // No other option of the HUD needs anything outside it.
        assert!(
            (options.specs().iter())
                .filter(|s| s.id() != id)
                .all(|s| s.needs().is_none())
        );
    }
}

#[test]
fn turning_the_viewmodels_switch_on_writes_only_its_settings_file() {
    let files = copy("flawhud");
    let options = HudOptions::read(&files, Some(&bundled("flawhud")), Platform::Linux);
    assert_eq!(
        written(&options, &files, &[choose(FLAW_VIEWMODELS, true)]),
        [(
            "flawhud/hud_transparent_viewmodels.txt".to_owned(),
            "#base ../../custom/flawhud/resource/ui/customizations/transparent_viewmodels/transparent_viewmodels.res"
                .to_owned()
        )]
    );
    assert!(written(&options, &files, &[choose(FLAW_VIEWMODELS, false)]).is_empty());

    let files = copy("hud-fixes");
    let options = HudOptions::read(&files, Some(&bundled("hud-fixes")), Platform::Linux);
    assert_eq!(
        written(&options, &files, &[choose(FIXES_VIEWMODELS, true)]),
        [(
            "hud-fixes/hud_transparent_viewmodels.txt".to_owned(),
            "x{TransparentViewmodel{visible 1}}".to_owned()
        )]
    );
}

#[test]
fn a_copy_that_never_reads_the_viewmodels_file_has_no_switch() {
    let files = HudFiles::from_memory(vec![vec![(
        "scripts/hudlayout.res".to_owned(),
        b"\"Resource/HudLayout.res\" { }\n".to_vec(),
    )]])
    .with_install_folder("flawhud");
    let options = HudOptions::read(&files, Some(&bundled("flawhud")), Platform::Linux);
    assert!(options.specs().iter().all(|s| s.id() != FLAW_VIEWMODELS));
}

#[test]
fn a_custom_menu_background_stays_left_out() {
    for (schema, folder, id) in [
        ("flawhud", "flawhud", "e:fh_custom_background"),
        ("hud-fixes", "hud-fixes", "e:tf_custom_background"),
    ] {
        let options = HudOptions::read(&copy(folder), Some(&bundled(schema)), Platform::Linux);
        assert!(options.specs().iter().all(|s| s.id() != id), "{schema}");
    }
}

#[test]
fn the_viewmodels_special_outside_a_switch_is_left_out() {
    let json = r#"{ "Controls": { "G": [
        { "Name": "pick", "Label": "Pick", "Type": "ComboBox", "Options": [
            { "Label": "A", "Value": "0", "Special": "TransparentViewmodels" }
        ] },
        { "Name": "only", "Label": "Only", "Type": "Checkbox", "Special": "TransparentViewmodels" }
    ] } }"#;
    let schema = HudSchema::parse("somehud", json.as_bytes()).unwrap();
    let options = schema.options(&copy("somehud"), Platform::Linux);
    assert!(options.specs().is_empty());
    assert_eq!(options.left_out(), 2);
}
