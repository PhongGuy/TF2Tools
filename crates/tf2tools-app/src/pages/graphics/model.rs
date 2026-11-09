//! The Graphics page's words: the status card, the preset cards and failures in player words.

use tf2tools_core::{
    ApplyPresetError, InstallMastercomfigError, MastercomfigStatus, MastercomfigStatusError,
    Preset, RemoveMastercomfigError, TakeOverError,
};

use crate::local_messages::{GAME_MOVED, LIBRARY_OPEN, LocalFailure, TF2_WRITE};
use crate::store_messages::LIBRARY_UPDATE;

const GITHUB_DOWN: &str = "Couldn't reach GitHub. Check your internet connection and try again.";
const GITHUB_BUSY: &str = "GitHub is busy right now. Wait a few minutes, then try again.";
const GITHUB_UNREADABLE: &str = "GitHub sent something TF2Tools couldn't read. Try again later.";
const NO_RELEASE: &str =
    "GitHub has no mastercomfig release TF2Tools can install right now. Try again later.";
const STALE_PAGE: &str =
    "mastercomfig changed since this page opened. Open the Graphics page again and try once more.";

/// What the status card shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusCard {
    NotInstalled,
    /// Managed by TF2Tools; `latest` is the release the last addon-store sync recorded.
    Installed {
        version: String,
        latest: Option<String>,
    },
    /// Installed by hand or another tool.
    Outside {
        version: Option<String>,
    },
}

/// The card for `status`, given the latest recorded release tag.
pub fn status_card(status: &MastercomfigStatus, latest: Option<&str>) -> StatusCard {
    match status {
        MastercomfigStatus::Managed { version, .. } => StatusCard::Installed {
            version: version.clone(),
            latest: latest.map(str::to_owned),
        },
        MastercomfigStatus::Outside { version } => StatusCard::Outside {
            version: version.clone(),
        },
        MastercomfigStatus::NotInstalled | _ => StatusCard::NotInstalled,
    }
}

/// The card's title line.
pub fn headline(card: &StatusCard) -> String {
    match card {
        StatusCard::NotInstalled => "mastercomfig isn't installed".to_owned(),
        StatusCard::Installed { version, .. }
        | StatusCard::Outside {
            version: Some(version),
        } => format!("mastercomfig {version}"),
        StatusCard::Outside { version: None } => "mastercomfig".to_owned(),
    }
}

/// The line under the title; None for a managed install when no release is recorded.
pub fn detail(card: &StatusCard) -> Option<String> {
    match card {
        StatusCard::NotInstalled => Some("Install it to pick a graphics preset.".to_owned()),
        StatusCard::Installed { latest: None, .. } => None,
        StatusCard::Installed {
            latest: Some(_), ..
        } => Some(match update_to(card) {
            Some(tag) => format!("Update to {tag}"),
            None => "Up to date".to_owned(),
        }),
        StatusCard::Outside { .. } => Some("Managed outside TF2Tools".to_owned()),
    }
}

/// The recorded release tag when it is newer than a managed install's version. An install
/// whose version does not parse (such as "unknown") is offered any tag that does.
pub fn update_to(card: &StatusCard) -> Option<&str> {
    match card {
        StatusCard::Installed {
            version,
            latest: Some(latest),
        } => {
            let newest = version_parts(latest)?;
            match version_parts(version) {
                Some(installed) if installed >= newest => None,
                _ => Some(latest),
            }
        }
        StatusCard::Installed { .. } | StatusCard::NotInstalled | StatusCard::Outside { .. } => {
            None
        }
    }
}

// A dotted numeric version as numbers, so 9.100.1 sorts after 9.99.3; None if any part isn't one.
fn version_parts(tag: &str) -> Option<Vec<u64>> {
    tag.split('.').map(|part| part.parse().ok()).collect()
}

/// The preset card's title.
pub fn preset_label(preset: Preset) -> &'static str {
    match preset {
        Preset::Ultra => "Ultra",
        Preset::High => "High",
        Preset::Low => "Low",
        Preset::Custom => "Custom",
        Preset::Medium | _ => "Medium",
    }
}

/// The preset card's line, as comfig-app words it.
pub fn preset_description(preset: Preset) -> &'static str {
    match preset {
        Preset::Ultra => "Absolute maximum graphical quality",
        Preset::High => "High graphical quality",
        Preset::Low => "Low graphical quality",
        Preset::Custom => "Skip setting quality options unless set by you",
        Preset::Medium | _ => "Medium graphical quality",
    }
}

/// A failed read of mastercomfig's install state in player words.
pub fn describe_status(err: &MastercomfigStatusError) -> LocalFailure {
    let message = match err {
        MastercomfigStatusError::Database { .. } | _ => LIBRARY_OPEN,
    };
    LocalFailure::new(message, err.to_string())
}

/// A failed install or update of mastercomfig in player words.
pub fn describe_install(err: &InstallMastercomfigError) -> LocalFailure {
    let message = match err {
        InstallMastercomfigError::NoRelease => NO_RELEASE,
        InstallMastercomfigError::GameMissing { .. } => GAME_MOVED,
        InstallMastercomfigError::RateLimited { .. } => GITHUB_BUSY,
        InstallMastercomfigError::Network { .. } => GITHUB_DOWN,
        InstallMastercomfigError::Io { .. } => TF2_WRITE,
        InstallMastercomfigError::Database { .. } => LIBRARY_UPDATE,
        InstallMastercomfigError::NotAVpk { .. }
        | InstallMastercomfigError::Http { .. }
        | InstallMastercomfigError::TooLarge { .. }
        | _ => GITHUB_UNREADABLE,
    };
    LocalFailure::new(message, err.to_string())
}

/// A failed Remove in player words.
pub fn describe_remove(err: &RemoveMastercomfigError) -> LocalFailure {
    let message = match err {
        RemoveMastercomfigError::NotManaged => STALE_PAGE,
        RemoveMastercomfigError::GameMissing { .. } => GAME_MOVED,
        RemoveMastercomfigError::Io { .. } => TF2_WRITE,
        RemoveMastercomfigError::Database { .. } | _ => LIBRARY_UPDATE,
    };
    LocalFailure::new(message, err.to_string())
}

/// A preset that could not be applied, in player words.
pub fn describe_apply(err: &ApplyPresetError) -> LocalFailure {
    let message = match err {
        ApplyPresetError::NotManaged => STALE_PAGE,
        ApplyPresetError::GameMissing { .. } => GAME_MOVED,
        ApplyPresetError::Io { .. } => TF2_WRITE,
        ApplyPresetError::Database { .. } | _ => LIBRARY_UPDATE,
    };
    LocalFailure::new(message, err.to_string())
}

/// A failed take-over of a hand-installed mastercomfig, in player words.
pub fn describe_take_over(err: &TakeOverError) -> LocalFailure {
    let message = match err {
        // The page was showing an install that has since gone or changed hands.
        TakeOverError::NotInstalled | TakeOverError::AlreadyManaged => STALE_PAGE,
        TakeOverError::Database { .. } | _ => LIBRARY_UPDATE,
    };
    LocalFailure::new(message, err.to_string())
}
