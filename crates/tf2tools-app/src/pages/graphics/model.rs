//! The Graphics page's words: the status card, the preset cards, the settings list and
//! failures in player words.

use gpui_kit::component::select::SelectItem;
use gpui_kit::{
    App, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    TestSupportExt as _, Window, div,
};
use tf2tools_core::{
    ApplyModulesError, ApplyPresetError, Choices, ComfigModule, InstallMastercomfigError,
    MastercomfigStatus, MastercomfigStatusError, ModuleCatalog, ModuleCatalogError, ModuleChoice,
    Preset, RemoveMastercomfigError, TakeOverError, readable_name,
};

use crate::i18n::tr;
use crate::ids;
use crate::local_messages::{LocalFailure, game_moved, library_open, tf2_write};
use crate::store_messages::library_update;

/// The site mastercomfig's releases come from, named in the shared source sentences.
const GITHUB: &str = "GitHub";

fn github_down() -> SharedString {
    tr!("messages.shared.source_unreachable", source = GITHUB)
}

fn github_busy() -> SharedString {
    tr!("messages.shared.source_busy", source = GITHUB)
}

fn github_unreadable() -> SharedString {
    tr!("messages.shared.source_unreadable", source = GITHUB)
}

/// A modules.cfg the player wrote, which TF2Tools does not overwrite unasked.
pub(crate) fn foreign_modules_cfg() -> SharedString {
    tr!("messages.mastercomfig.foreign_modules_cfg")
}

/// Added to the install or update toast when a player's own modules.cfg kept the saved settings out.
pub const OWN_MODULES_CFG_KEPT: &str = "Your own modules.cfg was kept, so the settings below aren't in use. Change one to use them instead.";

fn stale_page() -> SharedString {
    tr!("messages.mastercomfig.stale_page")
}

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

// A dotted numeric version as numbers, so 9.100.1 sorts after 9.99.3; a leading "v" is
// ignored and None means some part isn't a number.
fn version_parts(tag: &str) -> Option<Vec<u64>> {
    let tag = tag.strip_prefix('v').unwrap_or(tag);
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
        MastercomfigStatusError::Database { .. } | _ => library_open(),
    };
    LocalFailure::new(message, err.to_string())
}

/// A failed install or update of mastercomfig in player words.
pub fn describe_install(err: &InstallMastercomfigError) -> LocalFailure {
    let message = match err {
        InstallMastercomfigError::NoRelease => tr!("messages.mastercomfig.no_release"),
        InstallMastercomfigError::GameMissing { .. } => game_moved(),
        InstallMastercomfigError::RateLimited { .. } => github_busy(),
        InstallMastercomfigError::Network { .. } => github_down(),
        InstallMastercomfigError::Io { .. } => tf2_write(),
        InstallMastercomfigError::Database { .. } => library_update(),
        InstallMastercomfigError::NotAVpk { .. }
        | InstallMastercomfigError::Http { .. }
        | InstallMastercomfigError::TooLarge { .. }
        | _ => github_unreadable(),
    };
    LocalFailure::new(message, err.to_string())
}

/// A failed Remove in player words.
pub fn describe_remove(err: &RemoveMastercomfigError) -> LocalFailure {
    let message = match err {
        RemoveMastercomfigError::NotManaged => stale_page(),
        RemoveMastercomfigError::GameMissing { .. } => game_moved(),
        RemoveMastercomfigError::Io { .. } => tf2_write(),
        RemoveMastercomfigError::Database { .. } | _ => library_update(),
    };
    LocalFailure::new(message, err.to_string())
}

/// A preset that could not be applied, in player words.
pub fn describe_apply(err: &ApplyPresetError) -> LocalFailure {
    let message = match err {
        ApplyPresetError::NotManaged => stale_page(),
        ApplyPresetError::GameMissing { .. } => game_moved(),
        ApplyPresetError::Io { .. } => tf2_write(),
        ApplyPresetError::Database { .. } | _ => library_update(),
    };
    LocalFailure::new(message, err.to_string())
}

/// A setting that could not be saved, in player words.
pub fn describe_apply_modules(err: &ApplyModulesError) -> LocalFailure {
    let message = match err {
        ApplyModulesError::NotManaged => stale_page(),
        ApplyModulesError::ForeignModulesCfg { .. } => foreign_modules_cfg(),
        ApplyModulesError::GameMissing { .. } => game_moved(),
        ApplyModulesError::Io { .. } => tf2_write(),
        ApplyModulesError::Database { .. } => library_update(),
        ApplyModulesError::RateLimited { .. } => github_busy(),
        ApplyModulesError::Network { .. } => github_down(),
        ApplyModulesError::NoRelease => tr!("messages.mastercomfig.no_module_url"),
        ApplyModulesError::UpdateWaiting { .. } => tr!("messages.mastercomfig.update_waiting"),
        ApplyModulesError::NotAVpk { .. }
        | ApplyModulesError::Http { .. }
        | ApplyModulesError::TooLarge { .. }
        | _ => github_unreadable(),
    };
    LocalFailure::new(message, err.to_string())
}

/// A failed take-over of a hand-installed mastercomfig, in player words.
pub fn describe_take_over(err: &TakeOverError) -> LocalFailure {
    let message = match err {
        // The page was showing an install that has since gone or changed hands.
        TakeOverError::NotInstalled | TakeOverError::AlreadyManaged => stale_page(),
        TakeOverError::Database { .. } | _ => library_update(),
    };
    LocalFailure::new(message, err.to_string())
}

/// The Select entry that leaves a module unset, so the game's own value applies.
pub const GAME_DEFAULT: &str = "Game default";

/// One entry of a settings row's Select.
#[derive(Debug, Clone, PartialEq)]
pub struct ValueOption {
    label: SharedString,
    choice: ModuleChoice,
    id: ElementId,
}

impl ValueOption {
    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn choice(&self) -> &ModuleChoice {
        &self.choice
    }
}

// Each entry carries its pilot id, since the kit's rows have none.
impl SelectItem for ValueOption {
    type Value = ModuleChoice;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    fn render(&self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .id(self.id.clone())
            .test_support()
            .child(self.label.clone())
    }

    fn value(&self) -> &ModuleChoice {
        &self.choice
    }
}

/// A row's Select entries: Game default first, then each value in catalog order.
pub fn value_options(module: &ComfigModule) -> Vec<ValueOption> {
    let option = |label: &str, choice: ModuleChoice| ValueOption {
        id: ids::graphics_value(module.name(), &choice),
        label: SharedString::from(label.to_owned()),
        choice,
    };
    std::iter::once(option(GAME_DEFAULT, ModuleChoice::NotSet))
        .chain(
            module
                .values()
                .iter()
                .map(|v| option(v.label(), ModuleChoice::Value(v.value().to_owned()))),
        )
        .collect()
}

/// The value a row shows: the saved choice, else the preset's resolved value, else NotSet.
pub fn shown_choice(catalog: &ModuleCatalog, choices: &Choices, module: &str) -> ModuleChoice {
    if let Some(saved) = choices.get(module) {
        return saved.clone();
    }
    catalog
        .resolve(module, choices.preset())
        .map_or(ModuleChoice::NotSet, |v| ModuleChoice::Value(v.to_owned()))
}

/// What shows in place of the settings list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsNote {
    Loading,
    Offline,
    Unsupported,
}

/// The note's sentence.
pub fn settings_note_text(note: SettingsNote) -> &'static str {
    match note {
        SettingsNote::Loading => "Loading the settings list...",
        SettingsNote::Offline => {
            "The settings list needs an internet connection the first time. Connect, then open this page again."
        }
        SettingsNote::Unsupported => {
            "This mastercomfig version isn't supported yet. The presets above still work."
        }
    }
}

/// The note for a catalog that could not be read: a connection problem reads as offline, and a
/// release without a readable catalog (a 404 among them) as unsupported.
pub fn catalog_note(err: &ModuleCatalogError) -> SettingsNote {
    match err {
        ModuleCatalogError::Http { status: 404, .. } => SettingsNote::Unsupported,
        ModuleCatalogError::Network { .. }
        | ModuleCatalogError::RateLimited { .. }
        | ModuleCatalogError::Http { .. } => SettingsNote::Offline,
        ModuleCatalogError::Format { .. }
        | ModuleCatalogError::UnsupportedTag { .. }
        | ModuleCatalogError::TooLarge { .. }
        | _ => SettingsNote::Unsupported,
    }
}

/// The sentence naming settings an update dropped; None when nothing was dropped.
pub fn dropped_note(names: &[String]) -> Option<String> {
    if names.is_empty() {
        return None;
    }
    let names: Vec<String> = names.iter().map(|n| readable_name(n)).collect();
    Some(format!(
        "These settings aren't in this mastercomfig version and went back to the preset: {}.",
        names.join(", ")
    ))
}
