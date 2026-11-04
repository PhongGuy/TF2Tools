//! Starting points: small bundled looks built by code from the edit builders, so their changes
//! resolve against whatever default HUD the game ships.

use super::{Change, EditRefused, FontEdit, HudModel, OptionValue, PartKey, Recipe, Screen};
use crate::library::HudAssets;

/// The screen height font sizes are read and written at.
const SCREEN_TALL: u32 = 1080;
/// The screen box sizes are read and written at.
const SCREEN: Screen = Screen::new(1920, 1080);
/// The largest font height an edit accepts.
const MAX_TALL: i32 = 256;

/// A bundled look a player can start a HUD from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum StartingPoint {
    /// No class portrait and a short killfeed.
    CompetitiveMinimal,
    /// Larger health and ammo numbers.
    BigNumbers,
    /// Chat up top and no friends list.
    StreamerClean,
    /// Health and ammo beside the crosshair.
    CentreHud,
}

impl StartingPoint {
    /// Every starting point, in picker order.
    pub const ALL: [StartingPoint; 4] = [
        StartingPoint::CompetitiveMinimal,
        StartingPoint::BigNumbers,
        StartingPoint::StreamerClean,
        StartingPoint::CentreHud,
    ];

    /// The name shown in the picker.
    pub fn label(self) -> &'static str {
        match self {
            StartingPoint::CompetitiveMinimal => "Competitive minimal",
            StartingPoint::BigNumbers => "Big numbers",
            StartingPoint::StreamerClean => "Streamer clean",
            StartingPoint::CentreHud => "Centre HUD",
        }
    }

    /// One plain sentence for the picker.
    pub fn about(self) -> &'static str {
        match self {
            StartingPoint::CompetitiveMinimal => {
                "Hides the class portrait and keeps three killfeed rows."
            }
            StartingPoint::BigNumbers => "Makes the health and ammo numbers half again as big.",
            StartingPoint::StreamerClean => "Moves chat to the top and hides the friends list.",
            StartingPoint::CentreHud => "Moves health and ammo next to the crosshair.",
        }
    }
}

/// One builder call, run on the model as edited by the steps before it.
enum Step {
    Hide(&'static str),
    Option(&'static str, OptionValue),
    GrowFont(&'static str),
    /// Health and ammo beside the crosshair, their values' text on one line.
    CentreValues,
}

const HEALTH: &str = "HudPlayerStatus/HudPlayerHealth";
const HEALTH_VALUE: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";
const AMMO: &str = "HudWeaponAmmo";
const AMMO_VALUE: &str = "HudWeaponAmmo/AmmoInClip";
/// HUD units from the crosshair to each value's near edge, and down to the line they share.
const CENTRE_GAP: i32 = 20;
const CENTRE_LINE: i32 = 30;

impl Step {
    /// The list after this step, or the refusal that leaves it out.
    fn apply(&self, model: &HudModel, changes: &[Change]) -> Result<Vec<Change>, EditRefused> {
        let mut next = changes.to_vec();
        match self {
            Step::Hide(part) => next.extend(model.hide_part(&PartKey::new(*part), 0)?),
            // `set_option` already returns the whole next list.
            Step::Option(id, value) => return model.set_option(id, value.clone(), changes, 0),
            Step::GrowFont(part) => {
                let key = PartKey::new(*part);
                let size = font_tall(model, &key).ok_or(EditRefused::ReadOnly)?;
                let grown = (size.saturating_mul(3) / 2).min(MAX_TALL);
                next.extend(model.edit_font(
                    &key,
                    "font",
                    FontEdit::Size(grown),
                    SCREEN_TALL,
                    0,
                )?);
                // The box grows by the font's own ratio, so the bigger number is not clipped.
                if let Some(b) = model.layout(SCREEN).get(&key).filter(|_| size > 0) {
                    let grow = |px: i32| {
                        let scaled = i64::from(px) * i64::from(grown) / i64::from(size);
                        i32::try_from(scaled).unwrap_or(i32::MAX)
                    };
                    let (wide, tall) = (Some(grow(b.wide())), Some(grow(b.tall())));
                    next.extend(model.resize_part(&key, wide, tall, SCREEN, 0)?);
                }
            }
            Step::CentreValues => {
                let at = |part: &str, key, units: i32| {
                    let text = match units {
                        0.. => format!("c{units}"),
                        _ => format!("c-{}", units.unsigned_abs()),
                    };
                    model.set_key_text(&PartKey::new(part), key, &text, 0)
                };
                let health = value_in_panel(model, HEALTH, HEALTH_VALUE)?;
                let ammo = value_in_panel(model, AMMO, AMMO_VALUE)?;
                // Health ends left of the crosshair and ammo starts right of it; each panel
                // moves so its value's text lands on the shared line.
                let left = CENTRE_GAP
                    .saturating_add(health.x)
                    .saturating_add(health.wide);
                next.extend(at(HEALTH, "xpos", left.saturating_neg())?);
                next.extend(at(HEALTH, "ypos", CENTRE_LINE.saturating_sub(health.line))?);
                next.extend(at(AMMO, "xpos", CENTRE_GAP.saturating_sub(ammo.x))?);
                next.extend(at(AMMO, "ypos", CENTRE_LINE.saturating_sub(ammo.line))?);
            }
        }
        Ok(next)
    }
}

/// The height at 1080p of the font the part's own `font` line names. A part without one is
/// left alone, since its schema default is a guess about what the game draws.
fn font_tall(model: &HudModel, key: &PartKey) -> Option<i32> {
    let name = model.part(key)?.value("font")?.text();
    Some(model.scheme().font(name, SCREEN_TALL)?.tall())
}

// Where a value sits inside its panel, in HUD units: its left edge, width, and the line its
// text sits on (its box's middle plus half its font).
struct ValueSpot {
    x: i32,
    wide: i32,
    line: i32,
}

/// The value's spot inside `panel` as the layout places both at 1920x1080.
fn value_in_panel(model: &HudModel, panel: &str, value: &str) -> Result<ValueSpot, EditRefused> {
    let layout = model.layout(SCREEN);
    let (Some(p), Some(v)) = (
        layout.get(&PartKey::new(panel)),
        layout.get(&PartKey::new(value)),
    ) else {
        return Err(EditRefused::ReadOnly);
    };
    let tall = font_tall(model, &PartKey::new(value)).unwrap_or(0);
    let units = |px: i32| {
        let scaled = i64::from(px) * 480 / i64::from(SCREEN_TALL);
        i32::try_from(scaled).unwrap_or(i32::MAX)
    };
    let middle = units(v.y().saturating_sub(p.y()).saturating_add(v.tall() / 2));
    Ok(ValueSpot {
        x: units(v.x().saturating_sub(p.x())),
        wide: units(v.wide()),
        line: middle.saturating_add(tall / 2),
    })
}

/// The builder calls of a starting point, in the order they run.
fn steps(which: StartingPoint) -> Vec<Step> {
    match which {
        StartingPoint::CompetitiveMinimal => vec![
            Step::Hide("HudPlayerStatus/HudPlayerClass"),
            Step::Option("killfeed_rows", OptionValue::Int(3)),
        ],
        StartingPoint::BigNumbers => vec![Step::GrowFont(HEALTH_VALUE), Step::GrowFont(AMMO_VALUE)],
        StartingPoint::StreamerClean => vec![
            Step::Option("chat_top", OptionValue::Bool(true)),
            Step::Option("hide_friends", OptionValue::Bool(true)),
        ],
        StartingPoint::CentreHud => vec![Step::CentreValues],
    }
}

impl HudModel {
    /// The starting point as a recipe named by its label, its changes in group 0, built with
    /// the edit builders on this model in order; an edit the HUD refuses is left out.
    pub fn starting_point(&self, which: StartingPoint) -> Recipe {
        // Each step sees the model edited so far, so two font edits get two clone numbers.
        let mut changes = Vec::new();
        for step in steps(which) {
            let edited = self.with_changes(&changes).into_model();
            if let Ok(next) = step.apply(&edited, &changes) {
                changes = next;
            }
        }
        Recipe::new(which.label(), &changes, &HudAssets::new())
    }
}
