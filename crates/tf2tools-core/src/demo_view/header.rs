//! The HL2DEMO header: the map and the length, read from the first 1072 bytes.

use crate::error::DemoHeaderError;
use crate::store::is_map_name;

const MAGIC: &[u8; 8] = b"HL2DEMO\0";
const HEADER_LEN: usize = 1072;
const MAP_OFFSET: usize = 536;
const MAP_FIELD: usize = 260;
const DURATION_OFFSET: usize = 1056;

/// What the viewer needs from a demo's header.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct DemoHeader {
    map: String,
    duration_seconds: f32,
}

impl DemoHeader {
    /// The map the demo was recorded on, always a plain file name.
    pub fn map(&self) -> &str {
        &self.map
    }

    /// The recording's length in seconds, as the header states it.
    pub fn duration_seconds(&self) -> f32 {
        self.duration_seconds
    }
}

/// Reads the header from the start of a `.dem`; later bytes are never looked at.
/// Fails when the magic is wrong, the file is shorter than a header, or the map name is
/// not a plain file name.
pub fn read_demo_header(dem: &[u8]) -> Result<DemoHeader, DemoHeaderError> {
    let seen = dem.len().min(MAGIC.len());
    if dem[..seen] != MAGIC[..seen] {
        return Err(DemoHeaderError::NotADemo);
    }
    if dem.len() < HEADER_LEN {
        return Err(DemoHeaderError::Truncated);
    }
    let field = &dem[MAP_OFFSET..MAP_OFFSET + MAP_FIELD];
    let text = &field[..field.iter().position(|&b| b == 0).unwrap_or(MAP_FIELD)];
    let map: String = String::from_utf8_lossy(text)
        .chars()
        .filter(|c| !c.is_control())
        .collect();
    if !is_map_name(&map) {
        return Err(DemoHeaderError::BadMapName { name: map });
    }
    let mut bytes = [0u8; 4];
    bytes.copy_from_slice(&dem[DURATION_OFFSET..DURATION_OFFSET + 4]);
    Ok(DemoHeader {
        map,
        duration_seconds: f32::from_le_bytes(bytes),
    })
}
