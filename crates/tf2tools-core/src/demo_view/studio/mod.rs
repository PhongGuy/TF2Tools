//! Readers for Source studio model files (MDL, VVD, VTX) and their animations, bounded by
//! caps so a hostile file fails with an error instead of a huge allocation or a panic.
#![allow(
    dead_code,
    reason = "wired to ClassRig::load in the load plan, which removes this allow"
)]

/// Most bones one model may declare.
pub(crate) const MAX_BONES: usize = 128;
/// Most vertices one model may carry.
pub(crate) const MAX_VERTICES: usize = 65_536;
/// Most frames one animation may hold.
pub(crate) const MAX_FRAMES: usize = 4_096;
/// Largest studio file read from the game.
pub(crate) const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
/// Most include models one model may pull in.
pub(crate) const MAX_INCLUDES: usize = 8;
/// Most entries in any other table of a studio file.
pub(crate) const MAX_TABLE: usize = 8_192;

mod anim;
mod bytes;
#[cfg(test)]
mod fixture;
mod mdl;
mod merge;
mod mesh;
#[cfg(test)]
mod real_install;
mod types;
mod values;
