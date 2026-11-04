//! Synthetic hudanimations files for the HUD Maker tests; made-up values, no Valve bytes.

use super::hud_stock::{HEALTH, block, stock_files_with, with_blocks};
use tf2tools_core::{HudFiles, HudModel, Platform};

pub const MANIFEST_PATH: &str = "scripts/hudanimations_manifest.txt";
pub const ANIMS_PATH: &str = "scripts/hudanimations_tf.txt";
pub const HEALTH_RES: &str = "resource/ui/hudplayerhealth.res";
pub const HEALTH_KEY: &str = "HudPlayerStatus/HudPlayerHealth";
pub const VALUE: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";
pub const BONUS: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthBonusImage";

pub const MANIFEST: &str = "hudanimations_manifest\n{\n\t\"file\"\t\"scripts/hudanimations.txt\"\n\t\"file\"\t\"scripts/hudanimations_tf.txt\"\n}\n";

pub const ANIMS: &str = r#"event HudHealthBonusPulse
{
    Animate PlayerStatusHealthBonusImage Alpha "200" Linear 0.0 0.2
    Animate PlayerStatusHealthBonusImage Alpha "0" Linear 0.2 0.2
    RunEvent HudHealthBonusPulseLoop 0.4
}
event HudHealthBonusPulseLoop
{
    RunEvent HudHealthBonusPulse 0.0
}
event HudHealthDyingPulse
{
    Animate PlayerStatusHealthValue FgColor "HUDRedTeamSolid" Linear 0.0 0.1
    Animate PlayerStatusHealthValue FgColor "TanDark" Accel 0.1 0.1
    RunEvent HudHealthDyingPulseLoop 0.25
}
event HudHealthDyingPulseLoop
{
    RunEvent HudHealthDyingPulse 0.0
}
event HudLowAmmoPulse
{
    Animate AmmoInClip FgColor "255 0 0 255" Linear 0.0 0.1
}
event HudMedicCharged
{
    Animate ChargeLabel FgColor "TanLight" Linear 0.0 0.1
}
event DamagedPlayer
{
}
"#;

/// The health file with a bonus image at alpha 0, so the bonus pulse has a panel to animate.
pub fn health_with_bonus() -> String {
    let keys = [
        ("xpos", "60"),
        ("ypos", "20"),
        ("wide", "80"),
        ("tall", "80"),
        ("alpha", "0"),
    ];
    with_blocks(
        HEALTH,
        &block("PlayerStatusHealthBonusImage", "ImagePanel", &keys),
    )
}

/// The stock files plus MANIFEST and ANIMS, then `extra` overrides.
pub fn anim_files(extra: &[(&str, Option<&str>)]) -> HudFiles {
    let mut all: Vec<(&str, Option<&str>)> =
        vec![(MANIFEST_PATH, Some(MANIFEST)), (ANIMS_PATH, Some(ANIMS))];
    all.extend_from_slice(extra);
    stock_files_with(&all)
}

pub fn anim_model(extra: &[(&str, Option<&str>)]) -> HudModel {
    HudModel::load(&anim_files(extra), Platform::Linux).unwrap()
}
