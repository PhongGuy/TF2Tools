//! Kills as the parse reads them: raw names and weapon, the parties' team and class at the death.

use crate::hud_maker::Team;
use crate::weapons::TfClass;

/// One side of a kill, as the parse knew the player at the death's tick.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PartyRow {
    pub entity: u32,
    pub name: Option<String>,
    pub team: Option<Team>,
    pub class: Option<TfClass>,
}

/// One kill event, with the raw weapon string the game sent.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct KillRow {
    pub tick: u32,
    pub killer: Option<PartyRow>,
    pub assister: Option<PartyRow>,
    pub victim: PartyRow,
    pub weapon: String,
    pub critical: bool,
    pub feigned: bool,
}
