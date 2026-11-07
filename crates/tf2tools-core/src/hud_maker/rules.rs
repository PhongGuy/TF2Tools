//! The Stage 1 vocabulary: which HUD parts the maker offers, how a part is hidden, the
//! engine's pin corners, and each part's panel rule.

use super::probe;

/// A HUD part the Stage 1 maker shows and edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PartId {
    /// The player's health cross.
    Health,
    /// The class portrait beside the health.
    ClassPortrait,
    /// The weapon's clip and reserve ammo.
    Ammo,
    /// The Medic's ÜberCharge meter.
    UberMeter,
    /// The floating heal numbers.
    HealNumbers,
    /// The floating damage numbers.
    DamageNumbers,
    /// The name and health of the player under the crosshair.
    TargetId,
    /// The meter for a charged item (Razorback, Sandvich and the like).
    ItemMeter,
    /// The round timer.
    RoundTimer,
    /// The killfeed, HudDeathNotice.
    Killfeed,
    /// The chat box, HudChat.
    Chat,
    /// The two team timers on King of the Hill maps.
    KothTimer,
    /// The scoreboard's root panel.
    Scoreboard,
    /// The main menu's root panel.
    MainMenu,
    /// The killed-by panel's root, FreezePanel.
    FreezeCam,
    /// The end-of-round panel's root, WinPanel.
    WinPanel,
    /// The class menu's root panel.
    ClassSelect,
    /// The spectator screen's root panel.
    Spectator,
    /// The watched player's target ID on the spectator screen.
    SpectatorTargetId,
}

/// How the maker hides a part, as the keys it writes into the part's block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum HideTechnique {
    /// `visible 0`.
    VisibleZero,
    /// Moved below the screen.
    OffScreenY,
    /// Moved right of the screen.
    OffScreenX,
    /// Shrunk to nothing.
    SizeZero,
    /// Its height set to zero, for a panel whose width the game sets itself.
    TallZero,
    /// The part cannot be hidden from its file.
    NotOffered,
}

impl HideTechnique {
    /// The `(key, value)` pairs this technique writes; empty for `NotOffered`.
    pub fn writes(self) -> &'static [(&'static str, &'static str)] {
        match self {
            HideTechnique::VisibleZero => &[("visible", "0")],
            HideTechnique::OffScreenY => &[("ypos", "9999")],
            HideTechnique::OffScreenX => &[("xpos", "9999")],
            HideTechnique::SizeZero => &[("wide", "0"), ("tall", "0")],
            HideTechnique::TallZero => &[("tall", "0")],
            HideTechnique::NotOffered => &[],
        }
    }
}

/// A corner or edge midpoint a panel pins by, in the engine's `PinCorner_e` order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PinCorner {
    /// `PIN_TOPLEFT`, index 0.
    TopLeft,
    /// `PIN_TOPRIGHT`, index 1.
    TopRight,
    /// `PIN_BOTTOMLEFT`, index 2.
    BottomLeft,
    /// `PIN_BOTTOMRIGHT`, index 3.
    BottomRight,
    /// `PIN_CENTER_TOP`, index 4.
    CenterTop,
    /// `PIN_CENTER_RIGHT`, index 5.
    CenterRight,
    /// `PIN_CENTER_BOTTOM`, index 6.
    CenterBottom,
    /// `PIN_CENTER_LEFT`, index 7.
    CenterLeft,
}

impl PinCorner {
    /// Every corner in index order 0..7.
    pub const ALL: [PinCorner; 8] = [
        PinCorner::TopLeft,
        PinCorner::TopRight,
        PinCorner::BottomLeft,
        PinCorner::BottomRight,
        PinCorner::CenterTop,
        PinCorner::CenterRight,
        PinCorner::CenterBottom,
        PinCorner::CenterLeft,
    ];

    const NAMES: [&'static str; 8] = [
        "PIN_TOPLEFT",
        "PIN_TOPRIGHT",
        "PIN_BOTTOMLEFT",
        "PIN_BOTTOMRIGHT",
        "PIN_CENTER_TOP",
        "PIN_CENTER_RIGHT",
        "PIN_CENTER_BOTTOM",
        "PIN_CENTER_LEFT",
    ];

    /// Reads a pin-corner value like the engine's `GetPinCornerFromString`: a missing or
    /// unknown value is `TopLeft`.
    pub fn parse(text: Option<&str>) -> PinCorner {
        let Some(text) = text else {
            return PinCorner::TopLeft;
        };
        // The engine reads a one-character value as an `atoi` index; a non-digit is 0 and
        // an index past the table falls back to top-left here.
        if text.len() == 1 {
            let index = text
                .bytes()
                .next()
                .filter(u8::is_ascii_digit)
                .map(|b| b - b'0');
            return index
                .and_then(|i| Self::ALL.get(usize::from(i)).copied())
                .unwrap_or(PinCorner::TopLeft);
        }
        Self::NAMES
            .iter()
            .position(|name| name.eq_ignore_ascii_case(text))
            .map_or(PinCorner::TopLeft, |i| Self::ALL[i])
    }

    /// The corner as the file writes it by name, such as `PIN_TOPLEFT`.
    pub fn key_text(self) -> &'static str {
        Self::NAMES[Self::ALL.iter().position(|c| *c == self).unwrap_or(0)]
    }

    /// This corner's point inside a `wide` x `tall` box, halves rounded down.
    pub fn point(self, wide: i32, tall: i32) -> (i32, i32) {
        let (mid_x, mid_y) = (wide / 2, tall / 2);
        match self {
            PinCorner::TopLeft => (0, 0),
            PinCorner::TopRight => (wide, 0),
            PinCorner::BottomLeft => (0, tall),
            PinCorner::BottomRight => (wide, tall),
            PinCorner::CenterTop => (mid_x, 0),
            PinCorner::CenterRight => (wide, mid_y),
            PinCorner::CenterBottom => (mid_x, tall),
            PinCorner::CenterLeft => (0, mid_y),
        }
    }
}

const HUDLAYOUT: &str = "scripts/hudlayout.res";

/// Where a Stage 1 part's settings live and which of them the game's code owns.
#[derive(Debug)]
#[non_exhaustive]
pub struct PanelRule {
    id: PartId,
    label: &'static str,
    shows_when: &'static str,
    panel: &'static str,
    parent: Option<&'static str>,
    geometry: &'static [(&'static str, &'static str)],
    children_file: &'static str,
    ignores: &'static [&'static str],
    centres_x: bool,
    centres_y: bool,
    children: bool,
    clickable: bool,
    fills_screen: bool,
}

// The survey's PanelRules table, in `PartId` order so `get` can index by discriminant.
pub(super) static RULES: [PanelRule; 12] = [
    PanelRule {
        id: PartId::Health,
        label: "Health",
        shows_when: "Shows while you are alive",
        panel: "HudPlayerHealth",
        parent: Some("HudPlayerStatus"),
        geometry: &[],
        children_file: "resource/ui/hudplayerhealth.res",
        ignores: &[],
        centres_x: false,
        centres_y: false,
        children: true,
        clickable: true,
        fills_screen: false,
    },
    PanelRule {
        id: PartId::ClassPortrait,
        label: "Class portrait",
        shows_when: "Shows while you are alive",
        panel: "HudPlayerClass",
        parent: Some("HudPlayerStatus"),
        geometry: &[],
        children_file: "resource/ui/hudplayerclass.res",
        ignores: &[],
        centres_x: false,
        centres_y: false,
        children: true,
        clickable: true,
        fills_screen: false,
    },
    PanelRule {
        id: PartId::Ammo,
        label: "Ammo",
        shows_when: "Shows when your weapon uses ammo",
        panel: "HudWeaponAmmo",
        parent: None,
        geometry: &[(HUDLAYOUT, "HudWeaponAmmo")],
        children_file: "resource/ui/hudammoweapons.res",
        ignores: &["visible"],
        centres_x: false,
        centres_y: false,
        children: true,
        clickable: true,
        fills_screen: false,
    },
    PanelRule {
        id: PartId::UberMeter,
        label: "Uber meter",
        shows_when: "Shows when you hold a Medi Gun",
        panel: "HudMedicCharge",
        parent: None,
        geometry: &[(HUDLAYOUT, "HudMedicCharge")],
        children_file: "resource/ui/hudmediccharge.res",
        ignores: &["visible"],
        centres_x: false,
        centres_y: false,
        children: true,
        clickable: true,
        fills_screen: false,
    },
    PanelRule {
        id: PartId::HealNumbers,
        label: "Heal numbers",
        shows_when: "Shows when you are healed",
        panel: "CHealthAccountPanel",
        parent: None,
        geometry: &[(HUDLAYOUT, "CHealthAccountPanel")],
        children_file: "resource/ui/hudhealthaccount.res",
        ignores: &["visible"],
        centres_x: false,
        centres_y: false,
        children: true,
        clickable: true,
        fills_screen: false,
    },
    PanelRule {
        id: PartId::DamageNumbers,
        label: "Damage numbers",
        shows_when: "Shows when you damage an enemy",
        panel: "CDamageAccountPanel",
        parent: None,
        geometry: &[(HUDLAYOUT, "CDamageAccountPanel")],
        children_file: "resource/ui/huddamageaccount.res",
        ignores: &["xpos", "ypos", "wide", "tall", "visible"],
        centres_x: false,
        centres_y: false,
        children: true,
        clickable: true,
        fills_screen: false,
    },
    PanelRule {
        id: PartId::TargetId,
        label: "Target ID",
        shows_when: "Shows when you aim at a teammate",
        panel: "CMainTargetID",
        parent: None,
        geometry: &[(HUDLAYOUT, "CMainTargetID")],
        children_file: "resource/ui/targetid.res",
        ignores: &["xpos", "wide", "visible"],
        centres_x: true,
        centres_y: false,
        children: true,
        clickable: true,
        fills_screen: false,
    },
    PanelRule {
        id: PartId::ItemMeter,
        label: "Item meter",
        shows_when: "Shows when you carry an item with a meter",
        panel: "HudItemEffectMeter",
        parent: None,
        geometry: &[(HUDLAYOUT, "HudItemEffectMeter")],
        children_file: "resource/ui/huditemeffectmeter.res",
        ignores: &["visible"],
        centres_x: false,
        centres_y: false,
        children: true,
        clickable: true,
        fills_screen: false,
    },
    PanelRule {
        id: PartId::RoundTimer,
        label: "Round timer",
        shows_when: "Shows in a timed round",
        panel: "ObjectiveStatusTimePanel",
        parent: Some("HudMatchStatus"),
        geometry: &[("resource/ui/hudmatchstatus.res", "ObjectiveStatusTimePanel")],
        children_file: "resource/ui/hudobjectivetimepanel.res",
        ignores: &["visible"],
        centres_x: false,
        centres_y: false,
        children: true,
        clickable: true,
        fills_screen: false,
    },
    // HudDeathNotice draws its notices in code, so its block's ControlName siblings in
    // hudlayout.res are not its children; its box spans most of the screen, so the canvas
    // selects it through its drawn lines instead of a click on the box.
    PanelRule {
        id: PartId::Killfeed,
        label: "Killfeed",
        shows_when: "Shows when someone is killed",
        panel: "HudDeathNotice",
        parent: None,
        geometry: &[(HUDLAYOUT, "HudDeathNotice")],
        children_file: HUDLAYOUT,
        ignores: &["visible"],
        centres_x: false,
        centres_y: false,
        children: false,
        clickable: false,
        fills_screen: false,
    },
    // basechat.res's own HudChat block applies after hudlayout's, and its top-level siblings
    // (input line, filters button, history) are the chat's children.
    PanelRule {
        id: PartId::Chat,
        label: "Chat",
        shows_when: "Shows when someone sends a chat message",
        panel: "HudChat",
        parent: None,
        geometry: &[(HUDLAYOUT, "HudChat")],
        children_file: "resource/ui/basechat.res",
        ignores: &["visible"],
        centres_x: false,
        centres_y: false,
        children: true,
        clickable: true,
        fills_screen: false,
    },
    // On a King of the Hill map the game draws these two team timers in the round timer's place.
    PanelRule {
        id: PartId::KothTimer,
        label: "KOTH timer",
        shows_when: "Shows on King of the Hill maps",
        panel: "HudKothTimeStatus",
        parent: None,
        geometry: &[(HUDLAYOUT, "HudKothTimeStatus")],
        children_file: "resource/ui/hudobjectivekothtimepanel.res",
        ignores: &["visible"],
        centres_x: false,
        centres_y: false,
        children: true,
        clickable: true,
        fills_screen: false,
    },
];

/// When the scoreboard shows, shared by its part rule and its code-toggled root.
pub(super) const HOLD_SCORES: &str = "Shows while you hold the scoreboard key";

// The game re-centres the scoreboard root on both axes every update, so its position is
// code-owned; only its size and alpha can hide it.
pub(super) static SCOREBOARD: PanelRule = PanelRule {
    id: PartId::Scoreboard,
    label: "Scoreboard",
    shows_when: HOLD_SCORES,
    panel: "scores",
    parent: None,
    geometry: &[],
    children_file: "resource/ui/scoreboard.res",
    ignores: &["xpos", "ypos", "visible"],
    centres_x: true,
    centres_y: true,
    children: true,
    clickable: true,
    fills_screen: false,
};

// The main menu fills the screen and is the panel every menu control hangs off, so it is
// neither clickable nor hideable; its file reads `if_wider` on a wide screen.
pub(super) static MAIN_MENU: PanelRule = PanelRule {
    id: PartId::MainMenu,
    label: "Main menu",
    shows_when: "Shows at the main menu",
    panel: "MainMenuOverride",
    parent: None,
    geometry: &[],
    children_file: "resource/ui/mainmenuoverride.res",
    ignores: &[],
    centres_x: false,
    centres_y: false,
    children: true,
    clickable: false,
    fills_screen: false,
};

// The freeze cam panel fills the screen by hudlayout's box and everything hangs off it, so it
// is neither clickable nor hideable.
pub(super) static FREEZE_PANEL: PanelRule = PanelRule {
    id: PartId::FreezeCam,
    label: "Freeze cam",
    shows_when: "Shows while you watch your killer",
    panel: "FreezePanel",
    parent: None,
    geometry: &[(HUDLAYOUT, "FreezePanel")],
    children_file: "resource/ui/freezepanel_basic.res",
    ignores: &[],
    centres_x: false,
    centres_y: false,
    children: true,
    clickable: false,
    fills_screen: false,
};

// The win panel's box comes from hudlayout and everything hangs off it, so it is neither
// clickable nor hideable.
pub(super) static WIN_PANEL: PanelRule = PanelRule {
    id: PartId::WinPanel,
    label: "Win panel",
    shows_when: "Shows when a round ends",
    panel: "WinPanel",
    parent: None,
    geometry: &[(HUDLAYOUT, "WinPanel")],
    children_file: "resource/ui/winpanel.res",
    ignores: &[],
    centres_x: false,
    centres_y: false,
    children: true,
    clickable: false,
    fills_screen: false,
};

// The class menu's own box is its file's first block, `class`, so it is neither clickable
// nor hideable.
pub(super) static CLASS_SELECT: PanelRule = PanelRule {
    id: PartId::ClassSelect,
    label: "Class select",
    shows_when: "Shows when you pick a class",
    panel: "class",
    parent: None,
    geometry: &[],
    children_file: "resource/ui/classselection.res",
    ignores: &[],
    centres_x: false,
    centres_y: false,
    children: true,
    clickable: false,
    fills_screen: false,
};

// The spectator screen is the file's first block, `Spectator`; the game sizes it to the whole
// screen whatever the file says, so it is neither clickable nor hideable and ignores its box.
pub(super) static SPECTATOR_PANELS: [PanelRule; 2] = [
    PanelRule {
        id: PartId::Spectator,
        label: "Spectator",
        shows_when: "Shows while you watch other players",
        panel: "Spectator",
        parent: None,
        geometry: &[],
        children_file: "resource/ui/spectator.res",
        ignores: &["xpos", "ypos", "wide", "tall", "visible"],
        centres_x: false,
        centres_y: false,
        children: true,
        clickable: false,
        fills_screen: true,
    },
    PanelRule {
        id: PartId::SpectatorTargetId,
        label: "Spectator target ID",
        shows_when: "Shows while you watch a player in first person",
        panel: "CSpectatorTargetID",
        parent: None,
        geometry: &[(HUDLAYOUT, "CSpectatorTargetID")],
        children_file: "resource/ui/targetid.res",
        ignores: &["xpos", "wide", "visible"],
        centres_x: true,
        centres_y: false,
        children: true,
        clickable: true,
        fills_screen: false,
    },
];

impl PanelRule {
    /// Every HUD-scene rule, in `PartId` order.
    pub fn all() -> &'static [PanelRule] {
        &RULES
    }

    /// The rule for `id`.
    pub fn get(id: PartId) -> &'static PanelRule {
        match id {
            PartId::Scoreboard => &SCOREBOARD,
            PartId::MainMenu => &MAIN_MENU,
            PartId::FreezeCam => &FREEZE_PANEL,
            PartId::WinPanel => &WIN_PANEL,
            PartId::ClassSelect => &CLASS_SELECT,
            PartId::Spectator => &SPECTATOR_PANELS[0],
            PartId::SpectatorTargetId => &SPECTATOR_PANELS[1],
            PartId::Health
            | PartId::ClassPortrait
            | PartId::Ammo
            | PartId::UberMeter
            | PartId::HealNumbers
            | PartId::DamageNumbers
            | PartId::TargetId
            | PartId::ItemMeter
            | PartId::RoundTimer
            | PartId::Killfeed
            | PartId::Chat
            | PartId::KothTimer => &RULES[id as usize],
        }
    }

    /// The part this rule describes.
    pub fn id(&self) -> PartId {
        self.id
    }

    /// The part's name as players see it.
    pub fn label(&self) -> &'static str {
        self.label
    }

    /// When the part appears in game, in player words.
    pub fn shows_when(&self) -> &'static str {
        self.shows_when
    }

    /// The panel's name, e.g. `HudPlayerHealth`.
    pub fn panel(&self) -> &'static str {
        self.panel
    }

    /// The hudlayout container element the panel sits in, if any.
    pub fn parent(&self) -> Option<&'static str> {
        self.parent
    }

    /// Blocks applied first, in order: (lowercase game path, block name).
    pub fn geometry(&self) -> &'static [(&'static str, &'static str)] {
        self.geometry
    }

    /// The file the panel loads its children from. Its root block's top-level blocks are the
    /// children, except one named like `panel`, which is the panel's own block, applied after
    /// `geometry`.
    pub fn children_file(&self) -> &'static str {
        self.children_file
    }

    /// The `if_*` sub-blocks active for this part in the default situation.
    pub fn conditions(&self) -> &'static [&'static str] {
        probe::active_conditions(self.id)
    }

    /// How the maker hides this part.
    pub fn hide(&self) -> HideTechnique {
        probe::hide_technique(self.id)
    }

    /// Whether the game's code owns `key` for this panel, so the editor must not change it.
    /// Layout skips an ignored position or visibility key but still sizes the panel from the
    /// file's `wide` and `tall`. ASCII case-insensitive.
    pub fn ignores(&self, key: &str) -> bool {
        self.ignores.iter().any(|k| k.eq_ignore_ascii_case(key))
    }

    /// Whether the game centres the panel across the screen: x = (screen wide - wide) / 2.
    pub fn centres_x(&self) -> bool {
        self.centres_x
    }

    /// Whether the game centres the panel down the screen: y = (screen tall - tall) / 2.
    pub fn centres_y(&self) -> bool {
        self.centres_y
    }

    /// Whether `children_file`'s top-level blocks are the panel's children.
    pub fn has_children(&self) -> bool {
        self.children
    }

    /// Whether a click on the panel's box over the canvas selects it.
    pub fn clickable(&self) -> bool {
        self.clickable
    }

    /// Whether the game sizes the panel to the whole screen, whatever its file says.
    pub fn fills_screen(&self) -> bool {
        self.fills_screen
    }
}
