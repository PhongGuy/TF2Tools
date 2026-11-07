//! A library HUD copy holding a damage-style switch and an enabled slot.

use std::path::Path;

use tf2tools_app::services::Services;
use tf2tools_core::{HudEntry, HudFiles, HudSchema, HudSchemas, LibraryKind};

use super::hud_stock::{HEALTH, stock};

pub const GROUP: &str = "f:customizations/damage.res:0";
pub const SLOT: &str = "b:#customization/bh_player_largehealth.res";
pub const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";
pub const SWITCH: &str = "// DAMAGE STYLE\n// ONLY ONE LINE\n\n//#base \"../resource/ui/damage_small.res\"\t// SMALL\n#base \"../resource/ui/damage_medium.res\"\t// MEDIUM\n";

/// The switch files added to the stock HUD, as (path, text).
pub fn extra() -> Vec<(String, String)> {
    let damage = |font: &str| {
        format!(
            "\"Resource/UI/HudDamageAccount.res\" {{ CDamageAccountPanel {{ delta_item_font {font} }} }}\n"
        )
    };
    vec![
        ("customizations/damage.res".into(), SWITCH.into()),
        ("resource/ui/huddamageaccount.res".into(), format!("#base \"../../customizations/damage.res\"\n{}", damage("HudFontMedium"))),
        ("resource/ui/damage_small.res".into(), damage("HudFontSmall")),
        ("resource/ui/damage_medium.res".into(), damage("HudFontMedium")),
        ("resource/ui/hudplayerhealth.res".into(), format!("#base \"../../#customization/_enabled/bh_player_largehealth.res\"\n{HEALTH}")),
        ("#customization/bh_player_largehealth.res".into(), "\"Resource/UI/HudPlayerHealth.res\" { PlayerStatusHealthValue { textAlignment east } }\n".into()),
    ]
}

/// The stock HUD with the switch files, in memory, installed as `somehud`.
pub fn switch_files() -> HudFiles {
    let mut files = stock();
    for (path, text) in extra() {
        files.retain(|(p, _)| *p != path);
        files.push((path, text));
    }
    HudFiles::from_memory(vec![
        files
            .into_iter()
            .map(|(p, t)| (p, t.into_bytes()))
            .collect(),
    ])
    .with_install_folder("somehud")
}

/// Imports the switch HUD as `somehud` and copies it.
pub fn seed_switch_copy(services: &Services, dir: &Path) -> HudEntry {
    let extra = extra();
    let refs: Vec<(&str, &str)> = extra
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    let base = super::hud_copies::import_stock_hud(
        services,
        dir,
        "somehud",
        &refs,
        super::hud_copies::store_item(),
    );
    services.library.copy_hud(base.id()).unwrap()
}

/// A HUD option schema for `somehud` with one colour, the scheme's TanLight.
pub const COLOUR_SCHEMA: &str = r#"{
  "Links": { "GitHub": "https://github.com/Someone/SomeHUD" },
  "Controls": {
    "Colours": [
      { "Name": "sh_tan", "Label": "Tan", "Type": "ColorPicker", "Value": "1 2 3 255",
        "Files": { "resource/clientscheme.res": { "Scheme": { "Colors": { "TanLight": "$value" } } } } }
    ]
  }
}"#;

/// The schema copies of `Someone/SomeHUD` open with, for `shell_window_tweaked`.
pub fn with_colour_schema(services: &mut Services) {
    let schema = HudSchema::parse("somehud", COLOUR_SCHEMA.as_bytes()).unwrap();
    (services.library).set_hud_schemas(HudSchemas::bundled().with(schema));
}

/// Records `Someone/SomeHUD` as the copy's repo, as a store copy would have, so it opens with
/// `COLOUR_SCHEMA`.
pub fn give_colour_repo(services: &Services, copy: &HudEntry) {
    let maker = (services.data.library_dir(LibraryKind::Huds))
        .join(copy.folder_name())
        .join(".tf2tools/maker.json");
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&maker).unwrap()).unwrap();
    json["base"]["repo"] = "https://github.com/Someone/SomeHUD".into();
    std::fs::write(&maker, serde_json::to_vec(&json).unwrap()).unwrap();
}
