//! What is installed now, read as the five profile slots, and saving it as a new profile.

use super::profile_slots::{ProfileGraphics, ProfileSounds, SlotMode};
use super::profiles::{
    Profile, ProfileCrosshair, ProfileDraft, ProfileItem, assigned_crosshairs, hud_item,
    sound_item, validated_name,
};
use super::{AddonName, HudStatus, Library, MastercomfigStatus, SoundKind, profile_db};
use crate::error::{MastercomfigStatusError, ProfileError};
use crate::tf2::Tf2Install;

/// The game's setup as the library can name it, slot by slot.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CurrentSetup {
    /// Set: the managed HUD; None: no HUD; Leave: an unmanaged HUD or several.
    pub hud: SlotMode<ProfileItem>,
    /// Each managed sound, else `None`.
    pub sounds: ProfileSounds,
    /// The crosshair assignments, sorted by weapon script.
    pub crosshairs: Vec<ProfileCrosshair>,
    /// Set: the managed choices; None: mastercomfig not installed; Leave: not managed.
    pub graphics: SlotMode<ProfileGraphics>,
    /// The installed library mastercomfig add-ons, sorted.
    pub addons: Vec<AddonName>,
}

fn status_db(e: MastercomfigStatusError) -> ProfileError {
    match e {
        MastercomfigStatusError::Database { message } => ProfileError::Database { message },
    }
}

impl Library {
    /// Scans `tf/custom` and the library for the installed HUD, sounds, crosshairs,
    /// mastercomfig choices and add-ons. Fails with `Library` when a scan fails and
    /// `Database` when a library read does. Blocking.
    pub fn current_setup(&self, tf2: &Tf2Install) -> Result<CurrentSetup, ProfileError> {
        // The HUD scan goes first, so a broken `tf/custom` fails as that scan's error.
        let hud = match self.hud_status(tf2)? {
            HudStatus::Managed { id, .. } => {
                let item = self.db.read(|conn| Ok(hud_item(conn, &id)));
                SlotMode::Set(item.map_err(profile_db)??)
            }
            HudStatus::NotInstalled => SlotMode::None,
            HudStatus::Unmanaged { .. } | HudStatus::Multiple { .. } => SlotMode::Leave,
        };
        let installed = self.installed_sounds(tf2)?;
        let sound = |kind| -> Result<Option<ProfileItem>, ProfileError> {
            let Some(id) = installed.slot(kind).managed() else {
                return Ok(None);
            };
            let item = self.db.read(|conn| Ok(sound_item(conn, id)));
            item.map_err(profile_db)?.map(Some)
        };
        let sounds = ProfileSounds {
            hit: sound(SoundKind::Hitsound)?,
            kill: sound(SoundKind::Killsound)?,
        };
        let assignments = self.assignments()?;
        let crosshairs = self
            .db
            .read(|conn| Ok(assigned_crosshairs(conn, &assignments)))
            .map_err(profile_db)??;
        let graphics = match self.mastercomfig_status(tf2).map_err(status_db)? {
            MastercomfigStatus::NotInstalled => SlotMode::None,
            MastercomfigStatus::Managed { .. } => match self.mastercomfig_choices() {
                Ok(Some(choices)) => SlotMode::Set(ProfileGraphics { choices }),
                Ok(None) => SlotMode::Leave,
                Err(e) => return Err(status_db(e)),
            },
            MastercomfigStatus::Outside { .. } => SlotMode::Leave,
        };
        let mut addons: Vec<AddonName> = self
            .list_addons(Some(tf2))?
            .into_iter()
            .filter(|a| a.is_installed())
            .map(|a| a.name().clone())
            .collect();
        addons.sort();
        Ok(CurrentSetup {
            hud,
            sounds,
            crosshairs,
            graphics,
            addons,
        })
    }

    /// Creates a profile named `name` from `current_setup`, filled as
    /// `ProfileDraft::from_current` does. A blank name is refused before anything is scanned.
    pub fn save_current_profile(
        &self,
        name: &str,
        tf2: &Tf2Install,
    ) -> Result<Profile, ProfileError> {
        validated_name(&ProfileDraft::new(name))?;
        let setup = self.current_setup(tf2)?;
        self.create_profile(&ProfileDraft::from_current(name, &setup))
    }
}
