//! The profile apply steps past crosshairs: Graphics now, Add-ons next. Each step returns
//! what the report records, so `apply_profile_discarding` only collects it.

use super::profile_slots::{ProfileGraphics, SlotMode};
use super::{Choices, Library, MastercomfigStatus};
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
