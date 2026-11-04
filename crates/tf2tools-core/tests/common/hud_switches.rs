//! A synthetic HUD copy holding one switch of each detected form.

use std::path::{Path, PathBuf};

use tf2tools_core::HudFiles;

use super::hud_stock::{AMMO, BASECHAT, HEALTH, SCOREBOARD, stock};

pub const DAMAGE_SWITCH: &str = "// DAMAGE STYLE\n// IMPORTANT: MAKE SURE ONLY ONE LINE STAYS UNCOMMENTED!!\n\n//#base \"../resource/ui/damage_small.res\"\t\t// SMALL\n#base \"../resource/ui/damage_medium.res\"\t\t// MEDIUM\n//#base \"../resource/ui/damage_big.res\"\n\n// EXTRAS\n//#base \"../resource/ui/damage_last.res\"\t// LAST DAMAGE\n";

pub fn damage(font: &str) -> String {
    format!(
        "\"Resource/UI/HudDamageAccount.res\" {{ CDamageAccountPanel {{ delta_item_font {font} }} }}\n"
    )
}

/// The copy's files: the stock HUD, one switch of each form, then `extra` (path, text) replacing or adding.
pub fn switch_layer(extra: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut files = stock();
    let mut put = |path: &str, text: String| {
        files.retain(|(p, _)| p != path);
        files.push((path.to_owned(), text));
    };
    put("customizations/damage.res", DAMAGE_SWITCH.to_owned());
    put(
        "resource/ui/huddamageaccount.res",
        format!(
            "#base \"../../customizations/damage.res\"\n{}",
            damage("HudFontMedium")
        ),
    );
    for (name, font) in [
        ("small", "HudFontSmall"),
        ("medium", "HudFontMedium"),
        ("big", "HudFontGiant"),
        ("last", "HudFontSmall"),
    ] {
        put(&format!("resource/ui/damage_{name}.res"), damage(font));
    }
    put(
        "resource/ui/hudplayerhealth.res",
        format!("#base \"../../#customization/_enabled/bh_player_largehealth.res\"\n{HEALTH}"),
    );
    put(
        "#customization/bh_player_largehealth.res",
        "\"Resource/UI/HudPlayerHealth.res\" { PlayerStatusHealthValue { textAlignment east } }\n"
            .to_owned(),
    );
    put(
        "resource/ui/basechat.res",
        format!("#base \"../../#customization/_enabled/bh_chat_top.res\"\n{BASECHAT}"),
    );
    put(
        "#customization/_enabled/bh_chat_top.res",
        "\"Resource/UI/BaseChat.res\" { HudChat { ypos 0 } }\n".to_owned(),
    );
    put(
        "resource/ui/hudammoweapons.res",
        format!("#base \"../../customizations/streamermode/ammo.res\"\n{AMMO}"),
    );
    put(
        "customizations/streamermode_/ammo.res",
        "\"Resource/UI/HudAmmoWeapons.res\" { AmmoInClip { visible 0 } }\n".to_owned(),
    );
    put(
        "resource/ui/scoreboard.res",
        format!(
            "#base \"../../#customization/24p scoreboard/scoreboard.res\"\n#base \"../../#customization/32p scoreboard/scoreboard.res\"\n{SCOREBOARD}"
        ),
    );
    for size in ["24p", "32p"] {
        put(
            &format!("#customization/_disabled/! scoreboards/{size} scoreboard/scoreboard.res"),
            "\"Resource/UI/Scoreboard.res\" { }\n".to_owned(),
        );
    }
    for (path, text) in extra {
        put(path, (*text).to_owned());
    }
    files
}

/// `switch_layer(extra)` as an in-memory copy installed as `somehud`.
pub fn switch_files(extra: &[(&str, &str)]) -> HudFiles {
    let layer = switch_layer(extra)
        .into_iter()
        .map(|(p, t)| (p, t.into_bytes()))
        .collect();
    HudFiles::from_memory(vec![layer]).with_install_folder("somehud")
}

pub const GROUP: &str = "f:customizations/damage.res:0";
pub const SLOT: &str = "b:#customization/bh_player_largehealth.res";
pub const ADDED: &str = "#customization/_enabled/bh_player_largehealth.res";

pub fn copy_with(env: &super::LibraryEnv, layer: &[(String, String)]) -> tf2tools_core::HudEntry {
    let extra: Vec<(&str, &str)> = layer
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    let base =
        super::hud_copy::import_stock_hud(env, "somehud", &extra, tf2tools_core::Provenance::Local);
    env.lib.copy_hud(base.id()).unwrap()
}
pub fn dir(env: &super::LibraryEnv, hud: &tf2tools_core::HudEntry) -> PathBuf {
    env.dir
        .library_dir(tf2tools_core::LibraryKind::Huds)
        .join(hud.folder_name())
}
pub fn save(
    env: &super::LibraryEnv,
    tf: &Path,
    copy: &tf2tools_core::HudEntry,
    changes: &[tf2tools_core::Change],
) {
    let files = env.lib.made_hud_files(copy.id(), tf).unwrap();
    let target = tf2tools_core::MakeTarget::Existing(copy.id().clone());
    env.lib
        .save_made_hud(
            &target,
            copy.name(),
            &files,
            tf2tools_core::Platform::Linux,
            changes,
        )
        .unwrap();
}
pub fn choices() -> Vec<tf2tools_core::Change> {
    use tf2tools_core::{Change, OptionChange, OptionValue};
    vec![
        Change::option(
            1,
            OptionChange::new(
                OptionChange::HUD,
                GROUP,
                OptionValue::Pick("resource/ui/damage_big.res".into()),
            ),
        ),
        Change::option(
            2,
            OptionChange::new(OptionChange::HUD, SLOT, OptionValue::Bool(true)),
        ),
    ]
}
pub fn sha(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
