//! Loading a demo's map for the viewer: read the demo's header, find its map in the TF2 install
//! and build the mesh, each failure mapped to the line the page shows.

use std::io::Read as _;
use std::path::Path;

use gpui_kit::SharedString;
use tf2tools_core::{
    DemoHeader, DemoHeaderError, MapMesh, Tf2Install, find_installed_map, load_map_file,
};

use crate::i18n::tr;

/// The most of a demo the loader reads: its fixed-size header.
const HEADER_BYTES: u64 = 1072;

/// Why a demo's map cannot be shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    NotADemo,
    Damaged,
    /// The header names a map that is not a plain file name, such as a Workshop map.
    MapNotLoadable(String),
    NoTf2,
    MapMissing(String),
    MapUnreadable,
}

impl LoadError {
    /// The player's line for this failure.
    pub fn message(&self) -> SharedString {
        match self {
            Self::NotADemo => tr!("demo_viewer.error.not_a_demo"),
            Self::Damaged => tr!("demo_viewer.error.damaged"),
            Self::MapNotLoadable(map) => tr!("demo_viewer.error.map_not_loadable", map = map),
            Self::NoTf2 => tr!("demo_viewer.error.no_tf2"),
            Self::MapMissing(map) => tr!("demo_viewer.error.map_missing", map = map),
            Self::MapUnreadable => tr!("demo_viewer.error.map_unreadable"),
        }
    }
}

/// Seconds per tick when the header has no tick count, as an unfinished recording's does.
const DEFAULT_SECONDS_PER_TICK: f32 = 0.015;

/// A demo's map ready to show: its name and mesh, and how long one tick lasts.
pub struct DemoScene {
    pub map: String,
    pub mesh: MapMesh,
    pub seconds_per_tick: f32,
}

/// Reads `dem`'s header and loads the map it names from `install`. Blocking. Seconds per tick
/// is duration over ticks, or 0.015 for 0 ticks.
pub fn load_demo_scene(dem: &Path, install: Option<&Tf2Install>) -> Result<DemoScene, LoadError> {
    let (header, map, mesh) = load_header_and_map(dem, install)?;
    let seconds_per_tick = match header.ticks() {
        0 => DEFAULT_SECONDS_PER_TICK,
        ticks => header.duration_seconds() / ticks as f32,
    };
    Ok(DemoScene {
        map,
        mesh,
        seconds_per_tick,
    })
}

/// `tick * seconds_per_tick`, floored to whole seconds, as m:ss.
pub fn snapshot_time(tick: u32, seconds_per_tick: f32) -> String {
    let seconds = (f64::from(tick) * f64::from(seconds_per_tick)).floor() as u64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

// Reads the header and meshes the map it names.
fn load_header_and_map(
    dem: &Path,
    install: Option<&Tf2Install>,
) -> Result<(DemoHeader, String, MapMesh), LoadError> {
    let mut head = Vec::new();
    std::fs::File::open(dem)
        .and_then(|file| file.take(HEADER_BYTES).read_to_end(&mut head))
        .map_err(|_| LoadError::Damaged)?;
    let header = tf2tools_core::read_demo_header(&head).map_err(|e| match e {
        DemoHeaderError::NotADemo => LoadError::NotADemo,
        DemoHeaderError::BadMapName { name } => LoadError::MapNotLoadable(name),
        DemoHeaderError::Truncated | _ => LoadError::Damaged,
    })?;
    let map = header.map().to_owned();
    let install = install.ok_or(LoadError::NoTf2)?;
    let path =
        find_installed_map(install, &map).ok_or_else(|| LoadError::MapMissing(map.clone()))?;
    // A map that will not mesh is the player's dead end either way; the reason is for the log.
    let mesh = load_map_file(&path).map_err(|error| {
        tracing::warn!(%error, map = %map, "the demo viewer could not load a map");
        LoadError::MapUnreadable
    })?;
    Ok((header, map, mesh))
}
