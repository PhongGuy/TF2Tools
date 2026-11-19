//! The pages the sidebar routes between.

use gpui_kit::SharedString;

use crate::i18n::tr;

/// One sidebar destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Page {
    Huds,
    Sounds,
    Crosshairs,
    CrosshairMaker,
    HudMaker,
    Profiles,
    Addons,
    Graphics,
    HudStore,
    SoundStore,
    CrosshairStore,
    AddonStore,
    Maps,
    LogsTf,
    Settings,
    Logs,
}

impl Page {
    /// Every page in sidebar order: tools, stores, makers, Maps and logs.tf, footer.
    pub const ALL: [Page; 16] = [
        Page::Huds,
        Page::Sounds,
        Page::Crosshairs,
        Page::Addons,
        Page::Graphics,
        Page::Profiles,
        Page::HudStore,
        Page::SoundStore,
        Page::CrosshairStore,
        Page::AddonStore,
        Page::CrosshairMaker,
        Page::HudMaker,
        Page::Maps,
        Page::LogsTf,
        Page::Settings,
        Page::Logs,
    ];

    /// The kebab-case key the pilot ids carry.
    pub fn slug(self) -> &'static str {
        match self {
            Page::Huds => "huds",
            Page::Sounds => "sounds",
            Page::Crosshairs => "crosshairs",
            Page::CrosshairMaker => "crosshair-maker",
            Page::HudMaker => "hud-maker",
            Page::Profiles => "profiles",
            Page::Addons => "addons",
            Page::Graphics => "graphics",
            Page::HudStore => "hud-store",
            Page::SoundStore => "sound-store",
            Page::CrosshairStore => "crosshair-store",
            Page::AddonStore => "addon-store",
            Page::Maps => "maps",
            Page::LogsTf => "logs-tf",
            Page::Settings => "settings",
            Page::Logs => "logs",
        }
    }

    /// The sidebar text, as v5's AppSidebar has it, in the current language.
    pub fn label(self) -> SharedString {
        match self {
            Page::Huds => tr!("nav.huds"),
            Page::Sounds => tr!("nav.sounds"),
            Page::Crosshairs => tr!("nav.crosshairs"),
            Page::CrosshairMaker => tr!("nav.crosshair_maker"),
            Page::HudMaker => tr!("nav.hud_maker"),
            Page::Profiles => tr!("nav.profiles"),
            Page::Addons => tr!("nav.addons"),
            Page::Graphics => tr!("nav.graphics"),
            Page::HudStore => tr!("nav.hud_store"),
            Page::SoundStore => tr!("nav.sound_store"),
            Page::CrosshairStore => tr!("nav.crosshair_store"),
            Page::AddonStore => tr!("nav.addon_store"),
            Page::Maps => tr!("nav.maps"),
            Page::LogsTf => tr!("nav.logs_tf"),
            Page::Settings => tr!("nav.settings"),
            Page::Logs => tr!("nav.logs"),
        }
    }

    /// The header text, as v5's Header has it; only Logs differs from its label.
    pub fn title(self) -> SharedString {
        match self {
            Page::Logs => tr!("nav.activity_log"),
            other @ Page::Huds
            | other @ Page::Sounds
            | other @ Page::Crosshairs
            | other @ Page::CrosshairMaker
            | other @ Page::HudMaker
            | other @ Page::Profiles
            | other @ Page::Addons
            | other @ Page::Graphics
            | other @ Page::HudStore
            | other @ Page::SoundStore
            | other @ Page::CrosshairStore
            | other @ Page::AddonStore
            | other @ Page::Maps
            | other @ Page::LogsTf
            | other @ Page::Settings => other.label(),
        }
    }

    /// Whether the page still renders the "coming later" placeholder; each page flips its arm once it exists.
    pub fn is_placeholder(self) -> bool {
        !matches!(
            self,
            Page::Settings
                | Page::Huds
                | Page::Sounds
                | Page::Crosshairs
                | Page::CrosshairMaker
                | Page::HudMaker
                | Page::HudStore
                | Page::CrosshairStore
                | Page::SoundStore
                | Page::AddonStore
                | Page::Addons
                | Page::Graphics
                | Page::Logs
                | Page::Profiles
                | Page::Maps
                | Page::LogsTf
        )
    }

    /// Whether the page shows the floating sound player while it is idle: Sounds and the Sound
    /// Store. A playing player shows on every page.
    pub fn shows_player(self) -> bool {
        matches!(self, Page::Sounds | Page::SoundStore)
    }
}

/// Asks the shell to show `0`; emitted by pages that link to another page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NavigateTo(pub Page);
