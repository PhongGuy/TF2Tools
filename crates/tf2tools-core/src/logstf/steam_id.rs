//! SteamID64 values and the player text the page accepts.

use std::fmt;

use super::STEAM_ID64_BASE;
use crate::error::PlayerInputError;

/// A public-universe individual account, held as its SteamID64.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SteamId(u64);

impl SteamId {
    /// The ID for a SteamID64 inside the public individual range, else `None`.
    pub fn from_id64(id64: u64) -> Option<SteamId> {
        let top = STEAM_ID64_BASE + u64::from(u32::MAX);
        (STEAM_ID64_BASE..=top)
            .contains(&id64)
            .then_some(SteamId(id64))
    }

    /// The ID for exactly `[U:1:<account>]`, else `None`.
    pub fn from_steam3(text: &str) -> Option<SteamId> {
        let digits = text.strip_prefix("[U:1:")?.strip_suffix(']')?;
        SteamId::from_account(digits.parse::<u32>().ok()?)
    }

    fn from_account(account: u32) -> Option<SteamId> {
        SteamId::from_id64(STEAM_ID64_BASE + u64::from(account))
    }

    /// The SteamID64.
    pub fn id64(self) -> u64 {
        self.0
    }

    /// The Steam3 form, such as `[U:1:231702]`.
    pub fn steam3(self) -> String {
        format!("[U:1:{}]", self.0 - STEAM_ID64_BASE)
    }
}

impl fmt::Display for SteamId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// What the player box text means.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PlayerInput {
    /// A Steam ID, ready to use.
    Id(SteamId),
    /// A vanity name that still needs resolving through Steam.
    Vanity(String),
}

/// Reads a profile link, any Steam ID form, or a bare vanity name.
///
/// Fails with `NotSteam` for anything else.
pub fn parse_player_input(input: &str) -> Result<PlayerInput, PlayerInputError> {
    let text = input.trim();
    let text = text.strip_suffix('/').unwrap_or(text);
    let id = |id: Option<SteamId>| id.map(PlayerInput::Id).ok_or(PlayerInputError::NotSteam);

    if text.starts_with("[U:1:") {
        return id(SteamId::from_steam3(text));
    }
    if let Some(rest) = text.strip_prefix("STEAM_") {
        return id(legacy(rest));
    }
    if text.contains('/') {
        return link(text);
    }
    if is_id64_shaped(text) {
        return id(text.parse().ok().and_then(SteamId::from_id64));
    }
    vanity(text)
}

// `X:Y:Z` after `STEAM_`: any universe digit, Y is 0 or 1, account = 2Z + Y.
fn legacy(rest: &str) -> Option<SteamId> {
    let mut parts = rest.split(':');
    let (x, y, z) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || x.len() != 1 || !x.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let y: u32 = match y {
        "0" => 0,
        "1" => 1,
        _ => return None,
    };
    let z: u32 = z.parse().ok()?;
    SteamId::from_account(z.checked_mul(2)?.checked_add(y)?)
}

fn is_id64_shaped(text: &str) -> bool {
    text.len() == 17 && text.starts_with("7656") && text.bytes().all(|b| b.is_ascii_digit())
}

// A steamcommunity.com or logs.tf link; anything on another host is not Steam.
// Query, fragment and trailing path are dropped, and scheme and host ignore case.
fn link(text: &str) -> Result<PlayerInput, PlayerInputError> {
    let text = text.split(['?', '#']).next().unwrap_or(text);
    let lower = text.to_ascii_lowercase();
    let skip = ["https://", "http://"]
        .iter()
        .find(|scheme| lower.starts_with(*scheme))
        .map_or(0, |scheme| scheme.len());
    let text = &text[skip..];
    let (host, path) = text.split_once('/').ok_or(PlayerInputError::NotSteam)?;
    let host = host.to_ascii_lowercase();
    let host = host
        .strip_prefix("www.")
        .or_else(|| host.strip_prefix("m."))
        .unwrap_or(&host);
    let mut parts = path.split('/');
    let (kind, value) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    match (host, kind) {
        ("steamcommunity.com", "profiles") | ("logs.tf", "profile") => id64_text(value),
        ("steamcommunity.com", "id") => vanity(value),
        _ => Err(PlayerInputError::NotSteam),
    }
}

fn id64_text(text: &str) -> Result<PlayerInput, PlayerInputError> {
    text.parse::<u64>()
        .ok()
        .and_then(SteamId::from_id64)
        .map(PlayerInput::Id)
        .ok_or(PlayerInputError::NotSteam)
}

fn vanity(text: &str) -> Result<PlayerInput, PlayerInputError> {
    let ok = (2..=32).contains(&text.len())
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if ok {
        Ok(PlayerInput::Vanity(text.to_owned()))
    } else {
        Err(PlayerInputError::NotSteam)
    }
}
