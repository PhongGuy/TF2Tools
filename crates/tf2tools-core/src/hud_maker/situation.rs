//! A game situation: the moment the preview shows. It decides which code-toggled parts
//! show, the sample numbers, and the events the game would fire at that moment.

use std::collections::BTreeSet;

use super::{Fired, HudModel, Part, PartKey, Rgba};
use crate::weapons::TfClass;

/// The health element, where the game starts the health pulses.
const HEALTH_PANEL: &str = "HudPlayerStatus/HudPlayerHealth";
/// The ammo element, where the game starts the low ammo pulse.
const AMMO_PANEL: &str = "HudWeaponAmmo";
/// The Medic's charge meter, where the game starts the charged pulse.
const CHARGE_PANEL: &str = "HudMedicCharge";
/// The share of max health under which the dying pulse runs when the HUD sets none.
pub(super) const DEATH_WARNING: f32 = 0.49;
/// The health pulse image, which the health element sizes and colours itself.
pub(super) const PULSE_IMAGE: &str = "PlayerStatusHealthBonusImage";
/// How far the pulse image grows on each side at its fullest when the HUD sets no
/// `HealthBonusPosAdj`.
const PULSE_POS_ADJ: f32 = 25.;
/// Below this ammo percent the low ammo pulse runs.
const LOW_AMMO: u8 = 40;

/// The kind of weapon held.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WeaponKind {
    /// A weapon with a clip, such as the Rocket Launcher.
    #[default]
    Clip,
    /// A weapon without one, such as the Flame Thrower.
    NoClip,
}

/// A yes/no fact about the moment.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SituationSwitch {
    /// Holding a weapon with a meter.
    Meter,
    /// Aiming at a player.
    Aiming,
    /// An effect such as bleeding is on you.
    Effect,
    /// A Mannpower powerup is on you.
    Powerup,
    /// Typing a chat message.
    Typing,
    /// The 2D class portrait instead of the 3D model.
    Portrait,
    /// Just hit someone.
    JustHit,
    /// On a King of the Hill map.
    Koth,
}

/// The moment the preview shows; the default is a Soldier at 150 health with a full clip.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameSituation {
    class: TfClass,
    health: u16,
    ammo_percent: u8,
    charge: u8,
    weapon: WeaponKind,
    switches: BTreeSet<SituationSwitch>,
}

impl Default for GameSituation {
    fn default() -> GameSituation {
        GameSituation {
            class: TfClass::Soldier,
            health: 150,
            ammo_percent: 100,
            charge: 0,
            weapon: WeaponKind::Clip,
            switches: BTreeSet::new(),
        }
    }
}

impl GameSituation {
    /// The class played.
    pub fn class(&self) -> TfClass {
        self.class
    }

    /// The health shown.
    pub fn health(&self) -> u16 {
        self.health
    }

    /// The ammo left, as a percent of a full load.
    pub fn ammo_percent(&self) -> u8 {
        self.ammo_percent
    }

    /// The Medic's charge percent.
    pub fn charge(&self) -> u8 {
        self.charge
    }

    /// The kind of weapon held.
    pub fn weapon(&self) -> WeaponKind {
        self.weapon
    }

    /// Whether `s` is on.
    pub fn is_on(&self, s: SituationSwitch) -> bool {
        self.switches.contains(&s)
    }

    /// The class's max health.
    pub fn max_health(&self) -> u16 {
        match self.class {
            TfClass::Soldier => 200,
            TfClass::Pyro | TfClass::Demoman => 175,
            TfClass::Heavy => 300,
            TfClass::Medic => 150,
            TfClass::Scout | TfClass::Engineer | TfClass::Sniper | TfClass::Spy => 125,
        }
    }

    /// The most health overheal gives: 150% of max, rounded down to 5.
    pub fn overheal_cap(&self) -> u16 {
        let cap = u32::from(self.max_health()) * 3 / 2;
        u16::try_from(cap / 5 * 5).unwrap_or(u16::MAX)
    }

    /// The same moment as `c`, health clamped to the new class's overheal.
    pub fn with_class(mut self, c: TfClass) -> Self {
        self.class = c;
        let health = self.health;
        self.with_health(health)
    }

    /// The same moment at health `h`, clamped to 1 and the overheal cap.
    pub fn with_health(mut self, h: u16) -> Self {
        self.health = h.clamp(1, self.overheal_cap());
        self
    }

    /// The same moment with `p` percent ammo, at most 100.
    pub fn with_ammo_percent(mut self, p: u8) -> Self {
        self.ammo_percent = p.min(100);
        self
    }

    /// The same moment with charge `c` percent, at most 100.
    pub fn with_charge(mut self, c: u8) -> Self {
        self.charge = c.min(100);
        self
    }

    /// The same moment holding a `w` weapon.
    pub fn with_weapon(mut self, w: WeaponKind) -> Self {
        self.weapon = w;
        self
    }

    /// The same moment with `s` turned on or off.
    pub fn with_switch(mut self, s: SituationSwitch, on: bool) -> Self {
        if on {
            self.switches.insert(s);
        } else {
            self.switches.remove(&s);
        }
        self
    }

    /// Whether ammo is low enough for the low ammo warning.
    pub fn low_ammo(&self) -> bool {
        self.ammo_percent < LOW_AMMO
    }

    /// Whether the game shows the health pulse image: overhealed, or under `model`'s dying
    /// threshold.
    pub fn health_pulse(&self, model: &HudModel) -> bool {
        let health = PartKey::new(HEALTH_PANEL);
        self.health_pulse_under(death_warning(model, &health))
    }

    // Overhealed, or below `warning` of max health.
    pub(super) fn health_pulse_under(&self, warning: f32) -> bool {
        self.health > self.max_health() || self.dying_under(warning)
    }

    fn dying_under(&self, warning: f32) -> bool {
        f32::from(self.health) < f32::from(self.max_health()) * warning
    }

    /// Whether `part` is the health element's pulse image, which the game sizes and colours.
    pub(super) fn is_pulse_image(part: &Part) -> bool {
        part.name().eq_ignore_ascii_case(PULSE_IMAGE)
            && part.parent() == Some(&PartKey::new(HEALTH_PANEL))
    }

    /// How many pixels the game grows the pulse image by on each side in `model`.
    /// `CTFHudPlayerHealth::SetHealth`: adj = round(share * HealthBonusPosAdj), where share is
    /// the overheal over the cap's room (at most 1) or how far under the dying line health is.
    pub(super) fn health_pulse_grow(&self, model: &HudModel) -> i32 {
        let panel = model.part(&PartKey::new(HEALTH_PANEL));
        let pos_adj = (panel.and_then(|p| p.value("HealthBonusPosAdj")))
            .map_or(PULSE_POS_ADJ, |l| {
                super::expr::atoi(l.text().as_bytes()) as f32
            });
        let (health, max) = (f32::from(self.health), f32::from(self.max_health()));
        let warning = death_warning(model, &PartKey::new(HEALTH_PANEL));
        let share = if self.health > self.max_health() {
            let room = f32::from(self.overheal_cap()) - max;
            ((health - max) / room).min(1.)
        } else if self.dying_under(warning) {
            (max * warning - health) / (max * warning)
        } else {
            return 0;
        };
        // RoundFloatToInt converts in the default rounding mode, which breaks ties to even.
        (share * pos_adj).round_ties_even() as i32
    }

    /// The colour the game draws the pulse image in: `HealthDeathWarningColor` (default the
    /// scheme's `HUDDeathWarning`) while dying, else plain white.
    pub(super) fn pulse_colour(&self, model: &HudModel) -> Rgba {
        let health = PartKey::new(HEALTH_PANEL);
        if self.health > self.max_health() || !self.dying_under(death_warning(model, &health)) {
            return Rgba::WHITE;
        }
        let colour = (model.part(&health))
            .and_then(|p| p.value("HealthDeathWarningColor"))
            .map_or("HUDDeathWarning", |l| l.text().trim());
        model.scheme().colour(colour)
    }

    /// The events the game fires at this moment, in order: health, ammo, Medic charge, hit.
    pub fn fired(&self, model: &HudModel) -> Vec<Fired> {
        let health = PartKey::new(HEALTH_PANEL);
        let max = self.max_health();
        let mut fired = Vec::new();
        if self.health > max {
            fired.push(Fired::new("HudHealthBonusPulse", Some(health)));
        } else if self.dying_under(death_warning(model, &health)) {
            fired.push(Fired::new("HudHealthDyingPulse", Some(health)));
        }
        if self.low_ammo() {
            fired.push(Fired::new(
                "HudLowAmmoPulse",
                Some(PartKey::new(AMMO_PANEL)),
            ));
        }
        if self.class == TfClass::Medic && self.charge >= 100 {
            fired.push(Fired::new(
                "HudMedicCharged",
                Some(PartKey::new(CHARGE_PANEL)),
            ));
        }
        if self.is_on(SituationSwitch::JustHit) {
            fired.push(Fired::new("DamagedPlayer", None));
        }
        fired
    }
}

/// The health panel's `HealthDeathWarning`, else the game's default.
fn death_warning(model: &HudModel, health: &PartKey) -> f32 {
    (model.part(health))
        .and_then(|p| p.value("HealthDeathWarning"))
        .map(|l| super::animations::atof(l.text()))
        .filter(|w| w.is_finite())
        .unwrap_or(DEATH_WARNING)
}
