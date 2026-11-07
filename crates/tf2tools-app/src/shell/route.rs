//! The pages the sidebar routes between.

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
    HudStore,
    SoundStore,
    CrosshairStore,
    AddonStore,
    Maps,
    Settings,
    Logs,
}

impl Page {
    /// Every page in sidebar order: tools, stores, makers and Maps, footer.
    pub const ALL: [Page; 14] = [
        Page::Huds,
        Page::Sounds,
        Page::Crosshairs,
        Page::Profiles,
        Page::Addons,
        Page::HudStore,
        Page::SoundStore,
        Page::CrosshairStore,
        Page::AddonStore,
        Page::CrosshairMaker,
        Page::HudMaker,
        Page::Maps,
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
            Page::HudStore => "hud-store",
            Page::SoundStore => "sound-store",
            Page::CrosshairStore => "crosshair-store",
            Page::AddonStore => "addon-store",
            Page::Maps => "maps",
            Page::Settings => "settings",
            Page::Logs => "logs",
        }
    }

    /// The sidebar text, as v5's AppSidebar has it.
    pub fn label(self) -> &'static str {
        match self {
            Page::Huds => "HUDs",
            Page::Sounds => "Sounds",
            Page::Crosshairs => "Crosshairs",
            Page::CrosshairMaker => "Crosshair Maker",
            Page::HudMaker => "HUD Maker",
            Page::Profiles => "Profiles",
            Page::Addons => "Addons",
            Page::HudStore => "HUD Store",
            Page::SoundStore => "Sound Store",
            Page::CrosshairStore => "Crosshair Store",
            Page::AddonStore => "Addon Store",
            Page::Maps => "Maps",
            Page::Settings => "Settings",
            Page::Logs => "Logs",
        }
    }

    /// The header text, as v5's Header has it; only Logs differs from its label.
    pub fn title(self) -> &'static str {
        match self {
            Page::Logs => "Activity Log",
            other @ Page::Huds
            | other @ Page::Sounds
            | other @ Page::Crosshairs
            | other @ Page::CrosshairMaker
            | other @ Page::HudMaker
            | other @ Page::Profiles
            | other @ Page::Addons
            | other @ Page::HudStore
            | other @ Page::SoundStore
            | other @ Page::CrosshairStore
            | other @ Page::AddonStore
            | other @ Page::Maps
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
                | Page::Logs
                | Page::Profiles
                | Page::Maps
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
