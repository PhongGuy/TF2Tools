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
    Preset, RemoveMastercomfigError, TakeOverError,
};

use crate::i18n::mastercomfig::{choice_label, module_label_by_name};
use crate::i18n::tr;
use crate::ids;
use crate::local_messages::{LocalFailure, game_moved, library_open, tf2_write};
use crate::store_messages::library_update;

/// The site mastercomfig's releases come from, named in the shared source sentences.
const GITHUB: &str = "GitHub";

/// GitHub could not be reached while fetching mastercomfig.
fn github_down() -> SharedString {
    tr!("messages.shared.source_unreachable", source = GITHUB)
}

/// GitHub rate-limited the mastercomfig fetch.
fn github_busy() -> SharedString {
    tr!("messages.shared.source_busy", source = GITHUB)
}

/// GitHub answered with something the mastercomfig parser could not read.
fn github_unreadable() -> SharedString {
    tr!("messages.shared.source_unreadable", source = GITHUB)
}

/// A modules.cfg the player wrote, which TF2Tools does not overwrite unasked.
pub(crate) fn foreign_modules_cfg() -> SharedString {
    tr!("messages.mastercomfig.foreign_modules_cfg")
}

/// Added to the install or update toast when a player's own modules.cfg kept the saved settings out.
pub fn own_modules_cfg_kept() -> SharedString {
    tr!("graphics.toast.own_kept")
}

/// mastercomfig changed after the Graphics page loaded, so the action must be retried.
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

/// The status row's headline: whether mastercomfig is installed, which version, and by whom.
pub fn headline(card: &StatusCard) -> SharedString {
    match card {
        StatusCard::NotInstalled => tr!("graphics.status.not_installed"),
        StatusCard::Installed { version, .. } if card_is_current(card) => {
            tr!("graphics.status.up_to_date", version = version)
        }
        StatusCard::Installed { version, .. } => {
            tr!("graphics.status.installed", version = version)
        }
        StatusCard::Outside {
            version: Some(version),
        } => tr!("graphics.status.outside", version = version),
        StatusCard::Outside { version: None } => tr!("graphics.status.outside_unknown"),
    }
}

/// The line under the headline, saying what the player can do in this state.
pub fn detail(card: &StatusCard) -> SharedString {
    match card {
        StatusCard::NotInstalled => tr!("graphics.status.install_hint"),
        StatusCard::Installed { .. } => tr!("graphics.status.saves"),
        StatusCard::Outside { .. } => tr!("graphics.status.take_over_hint"),
    }
}

// A managed install is current only when a release is recorded and none is newer.
fn card_is_current(card: &StatusCard) -> bool {
    matches!(
        card,
        StatusCard::Installed {
            latest: Some(_),
            ..
        }
    ) && update_to(card).is_none()
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

/// The preset's name in the current language, as the cards, the strip and the rows show it.
pub fn preset_label(preset: Preset) -> SharedString {
    match preset {
        Preset::Ultra => tr!("graphics.preset.ultra.label"),
        Preset::High => tr!("graphics.preset.high.label"),
        Preset::Low => tr!("graphics.preset.low.label"),
        Preset::Custom => tr!("graphics.preset.custom.label"),
        Preset::Medium | _ => tr!("graphics.preset.medium.label"),
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

/// A row's Select entries: Let TF2 decide first, then each value in catalog order.
pub fn value_options(module: &ComfigModule) -> Vec<ValueOption> {
    std::iter::once(ModuleChoice::NotSet)
        .chain(
            module
                .values()
                .iter()
                .map(|v| ModuleChoice::Value(v.value().to_owned())),
        )
        .map(|choice| ValueOption {
            id: ids::graphics_value(module.name(), &choice),
            label: choice_label(module, &choice),
            choice,
        })
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

/// The note's sentence, worded in the current language.
pub fn settings_note_text(note: SettingsNote) -> SharedString {
    match note {
        SettingsNote::Loading => tr!("graphics.settings_note.loading"),
        SettingsNote::Offline => tr!("graphics.settings_note.offline"),
        SettingsNote::Unsupported => tr!("graphics.settings_note.unsupported"),
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
pub fn dropped_note(names: &[String]) -> Option<SharedString> {
    if names.is_empty() {
        return None;
    }
    let names: Vec<String> = names
        .iter()
        .map(|n| module_label_by_name(n).to_string())
        .collect();
    Some(tr!("graphics.toast.dropped", names = names.join(", ")))
}
