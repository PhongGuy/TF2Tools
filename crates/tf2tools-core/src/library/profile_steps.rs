//! The profile apply steps that follow crosshairs, Graphics and Add-ons. Each step returns
//! what the report records, so `apply_profile_discarding` only collects it.

use super::profile_slots::{ProfileAddons, ProfileGraphics, SlotMode};
use super::profiles::{MissingItem, MissingKind};
use super::{AddonName, Choices, Library, MastercomfigStatus, MovementKeys};
use crate::tf2::Tf2Install;

/// Why a profile step did not run although its slot asked for a change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ApplySkipReason {
    /// The slot needs mastercomfig and none is installed.
    MastercomfigNotInstalled,
    /// mastercomfig is installed, but by hand or another tool, so TF2Tools leaves it alone.
    MastercomfigNotManaged,
}

/// What the Graphics step did. `outcome` is None when the slot asked for nothing to change.
#[derive(Debug, Default)]
pub(super) struct GraphicsStep {
    pub(super) outcome: Option<Result<(), String>>,
    pub(super) skip: Option<ApplySkipReason>,
    pub(super) pending_modules: Option<Choices>,
    pub(super) binds_restored: bool,
}

impl Library {
    /// Set writes the preset over a managed mastercomfig and leaves its module choices to the
    /// app; None removes a managed one. A foreign install is skipped, never touched.
    pub(super) fn apply_profile_graphics(
        &self,
        slot: &SlotMode<ProfileGraphics>,
        tf2: &Tf2Install,
    ) -> GraphicsStep {
        let mut step = GraphicsStep::default();
        if matches!(slot, SlotMode::Leave) {
            return step;
        }
        let status = match self.mastercomfig_status(tf2) {
            Ok(status) => status,
            Err(e) => {
                step.outcome = Some(Err(e.to_string()));
                return step;
            }
        };
        match (slot, status) {
            (SlotMode::Leave, _) | (SlotMode::None, MastercomfigStatus::NotInstalled) => {}
            (SlotMode::Set(_), MastercomfigStatus::NotInstalled) => {
                step.skip = Some(ApplySkipReason::MastercomfigNotInstalled);
            }
            (_, MastercomfigStatus::Outside { .. }) => {
                step.skip = Some(ApplySkipReason::MastercomfigNotManaged);
            }
            (SlotMode::Set(graphics), MastercomfigStatus::Managed { .. }) => {
                let result = self.apply_preset(tf2, graphics.choices.preset());
                if result.is_ok() {
                    step.pending_modules = Some(graphics.choices.clone());
                }
                step.outcome = Some(result.map_err(|e| e.to_string()));
            }
            (SlotMode::None, MastercomfigStatus::Managed { .. }) => {
                let result = self.remove_mastercomfig(tf2);
                step.binds_restored = matches!(result, Ok(true));
                step.outcome = Some(result.map(|_| ()).map_err(|e| e.to_string()));
            }
        }
        step
    }
}

/// What the Add-ons step did. `ran` is false for Leave; `failures` holds each toggle's error.
#[derive(Debug, Default)]
pub(super) struct AddonsStep {
    pub(super) ran: bool,
    pub(super) failures: Vec<String>,
    pub(super) skip: Option<ApplySkipReason>,
    pub(super) movement: Option<MovementKeys>,
    pub(super) movement_off: bool,
}

impl Library {
    /// None turns off every installed library add-on; Set turns on the ticked ones that are
    /// present and not yet installed and, with Only these, turns off the rest. Turn-offs
    /// always run; turn-ons need mastercomfig as the Graphics step left it.
    pub(super) fn apply_profile_addons(
        &self,
        slot: &SlotMode<ProfileAddons>,
        missing: &[MissingItem],
        tf2: &Tf2Install,
    ) -> AddonsStep {
        let mut step = AddonsStep::default();
        let (wanted, only_these): (&[AddonName], bool) = match slot {
            SlotMode::Leave => return step,
            SlotMode::None => (&[], true),
            SlotMode::Set(addons) => (&addons.on, addons.only_these),
        };
        step.ran = true;
        // `list_addons` leaves out override VPKs, so only real add-ons are turned off.
        let listed = match self.list_addons(Some(tf2)) {
            Ok(listed) => listed,
            Err(e) => {
                step.failures.push(e.to_string());
                return step;
            }
        };
        if only_these {
            for addon in listed
                .iter()
                .filter(|a| a.is_installed() && !wanted.contains(a.name()))
            {
                match self.uninstall_addon(addon.name(), tf2) {
                    Ok(()) => step.movement_off |= addon.name().is_null_movement(),
                    Err(e) => step.failures.push(e.to_string()),
                }
            }
        }

        let is_missing = |name: &AddonName| {
            missing
                .iter()
                .any(|m| matches!(m.kind(), MissingKind::Addon { name: n, .. } if n == name))
        };
        // Installed is read from `listed`, the same fact the turn-offs above went by.
        let installed =
            |name: &AddonName| listed.iter().any(|a| a.name() == name && a.is_installed());
        let turn_ons: Vec<&AddonName> = wanted
            .iter()
            .filter(|name| !is_missing(name) && !installed(name))
            .collect();
        if turn_ons.is_empty() {
            return step;
        }
        match self.mastercomfig_status(tf2) {
            Ok(MastercomfigStatus::NotInstalled) => {
                step.skip = Some(ApplySkipReason::MastercomfigNotInstalled);
            }
            Ok(_) => {
                for name in turn_ons {
                    match self.install_addon(name, tf2) {
                        Ok(Some(keys)) => step.movement = Some(keys),
                        Ok(None) => {}
                        Err(e) => step.failures.push(e.to_string()),
                    }
                }
            }
            Err(e) => step.failures.push(e.to_string()),
        }
        step
    }
}
