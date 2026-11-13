//! Scenes: the screens the maker previews, which files each reads, its part rules and
//! the panels the game's code shows or hides there.

use super::probe;
use super::rules::{
    CLASS_SELECT, FREEZE_PANELS, HOLD_SCORES, MAIN_MENU, RULES, SCOREBOARD, SPECTATOR_PANELS,
    WIN_PANEL,
};
use super::{
    GameSituation, HideTechnique, HudModel, PanelRule, SampleSet, SituationSwitch, WeaponKind,
};

/// A screen the maker previews and edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Scene {
    /// The in-game HUD.
    Hud,
    /// The scoreboard held open over the game.
    Scoreboard,
    /// The main menu.
    MainMenu,
    /// The killed-by panel and freeze cam.
    FreezeCam,
    /// The end-of-round win panel.
    WinPanel,
    /// The class select menu.
    ClassSelect,
    /// The spectator screen's bars.
    Spectator,
}

impl Scene {
    /// Every scene, in the order the maker lists them.
    pub const ALL: [Scene; 7] = [
        Scene::Hud,
        Scene::Scoreboard,
        Scene::MainMenu,
        Scene::FreezeCam,
        Scene::WinPanel,
        Scene::ClassSelect,
        Scene::Spectator,
    ];

    /// The scene whose files include `path`, compared ASCII case-insensitively.
    pub fn for_file(path: &str) -> Option<Scene> {
        SceneRule::every()
            .iter()
            .find(|rule| rule.files().iter().any(|f| f.eq_ignore_ascii_case(path)))
            .map(|rule| rule.scene())
    }

    /// The scene whose files include `path`, preferring `open` when it reads it, so an edit to
    /// a file two scenes share belongs to the tab it was made on.
    pub fn for_file_near(path: &str, open: Scene) -> Option<Scene> {
        let reads = SceneRule::get(open)
            .files()
            .iter()
            .any(|f| f.eq_ignore_ascii_case(path));
        if reads {
            Some(open)
        } else {
            Scene::for_file(path)
        }
    }
}

/// Which match the Scoreboard scene previews: the dialog is one panel, and the match decides
/// what it shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum BoardMode {
    /// A casual match.
    #[default]
    Casual,
    /// A tournament match with team names.
    Competitive,
    /// Mann vs. Machine, with its own table.
    Mvm,
}

impl BoardMode {
    /// Every mode, in the order the dropdown lists them.
    pub const ALL: [BoardMode; 3] = [BoardMode::Casual, BoardMode::Competitive, BoardMode::Mvm];

    /// The mode's name as players see it.
    pub fn label(self) -> &'static str {
        match self {
            BoardMode::Casual => "Casual",
            BoardMode::Competitive => "Competitive",
            BoardMode::Mvm => "MvM",
        }
    }

    /// The mode that owns `path` alone, compared ASCII case-insensitively: only a file no
    /// other mode reads.
    pub fn for_file(path: &str) -> Option<BoardMode> {
        let shared = SceneRule::for_mode(Scene::Scoreboard, BoardMode::Casual).files();
        BoardMode::ALL.into_iter().find(|&mode| {
            SceneRule::for_mode(Scene::Scoreboard, mode)
                .files()
                .iter()
                .filter(|f| !shared.contains(f))
                .any(|f| f.eq_ignore_ascii_case(path))
        })
    }
}

/// What the game's code checks before it shows a code-toggled HUD part, so a later game
/// situation can test it; the HUD's default situation is the preview's sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ShowsFor {
    /// While you play this class, such as `"Medic"`.
    Class(&'static str),
    /// While the weapon you hold is of this kind: `"clip"`, `"no clip"`, `"meter"`, `"low ammo"`.
    Weapon(&'static str),
    /// While an effect is on you: `"condition"` or `"powerup"`.
    Effect(&'static str),
    /// While a mode or setting is on: `"3D model"`, `"2D portrait"`, `"typing"` (a chat message)
    /// or `"full hud"` (Minimal HUD off).
    Mode(&'static str),
    /// While you aim at something: `"player"`.
    Aim(&'static str),
    /// While your health is at a mark: `"pulse"` (overhealed or under the dying threshold).
    Health(&'static str),
}

impl ShowsFor {
    /// Whether it holds for a RED Soldier holding a rocket launcher (a clip weapon with no
    /// meter and ammo to spare), no effects on him, the 3D class model on, aiming at no one.
    pub fn in_default(self) -> bool {
        self.holds(&GameSituation::default())
    }

    /// Whether it holds in situation `s` for HUD `model`, whose `HealthDeathWarning` sets the
    /// dying threshold.
    pub fn holds_in(self, s: &GameSituation, model: &HudModel) -> bool {
        match self {
            ShowsFor::Health("pulse") => s.health_pulse(model),
            ShowsFor::Mode("full hud") => !model.minmode(),
            ShowsFor::Class(_)
            | ShowsFor::Weapon(_)
            | ShowsFor::Effect(_)
            | ShowsFor::Mode(_)
            | ShowsFor::Aim(_)
            | ShowsFor::Health(_) => self.holds(s),
        }
    }

    /// Whether it holds in situation `s`. `Health` uses the game's default dying threshold and
    /// ignores the HUD's own `HealthDeathWarning`; use `holds_in` for that.
    pub fn holds(self, s: &GameSituation) -> bool {
        match self {
            ShowsFor::Class(class) => s.class().name() == class,
            ShowsFor::Weapon("clip") => s.weapon() == WeaponKind::Clip,
            ShowsFor::Weapon("no clip") => s.weapon() == WeaponKind::NoClip,
            ShowsFor::Weapon("meter") => s.is_on(SituationSwitch::Meter),
            ShowsFor::Weapon("low ammo") => s.low_ammo(),
            ShowsFor::Effect("condition") => s.is_on(SituationSwitch::Effect),
            ShowsFor::Effect("powerup") => s.is_on(SituationSwitch::Powerup),
            ShowsFor::Mode("3D model") => !s.is_on(SituationSwitch::Portrait),
            ShowsFor::Mode("2D portrait") => s.is_on(SituationSwitch::Portrait),
            ShowsFor::Mode("typing") => s.is_on(SituationSwitch::Typing),
            ShowsFor::Mode("koth") => s.is_on(SituationSwitch::Koth),
            ShowsFor::Mode("not koth") => !s.is_on(SituationSwitch::Koth),
            // The sample's HUD is the full one; only `holds_in` knows the model's mode.
            ShowsFor::Mode("full hud") => true,
            ShowsFor::Aim(_) => s.is_on(SituationSwitch::Aiming),
            ShowsFor::Health("pulse") => s.health_pulse_under(super::situation::DEATH_WARNING),
            ShowsFor::Weapon(_) | ShowsFor::Effect(_) | ShowsFor::Mode(_) | ShowsFor::Health(_) => {
                false
            }
        }
    }
}

/// A panel the game's code shows or hides in a scene, whatever its file says.
#[derive(Debug)]
#[non_exhaustive]
pub struct CodeToggled {
    panel: &'static str,
    shown: bool,
    shows_for: Option<ShowsFor>,
    shows_when: &'static str,
    owned: &'static [&'static str],
    hide_as: Option<HideTechnique>,
    fit: Fit,
}

/// How the game fits a code-sized bar to the screen, read by the layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Fit {
    /// The file's box stands.
    None,
    /// At the origin, the screen wide, the file's height.
    TopBar,
    /// At the file's place, the screen wide, down to the screen's bottom.
    BottomBar,
    /// At the x the game's code gives it: `gap` 640-wide units, scaled by screen width, plus the
    /// width of the sibling panel.
    AfterSibling { sibling: &'static str, gap: i32 },
    /// The chat's input line: its file's ypos and tall give way to the chat font's height.
    ChatInput,
}

impl CodeToggled {
    /// The panel's name in the scene's file.
    pub fn panel(&self) -> &'static str {
        self.panel
    }

    /// Whether the preview shows the panel under the scene's sample.
    pub fn shown_in_sample(&self) -> bool {
        self.shows_for.map_or(self.shown, ShowsFor::in_default)
    }

    /// Whether the game shows the panel in situation `s` for HUD `model`.
    pub fn shown_in(&self, s: &GameSituation, model: &HudModel) -> bool {
        self.shows_for.map_or(self.shown, |f| f.holds_in(s, model))
    }

    /// What the game checks before it shows the panel; `None` for a scoreboard panel, whose
    /// preview default is fixed.
    pub fn shows_for(&self) -> Option<ShowsFor> {
        self.shows_for
    }

    /// When the game shows the panel, in player words.
    pub fn shows_when(&self) -> &'static str {
        self.shows_when
    }

    /// How the maker hides the panel.
    pub fn hide(&self) -> HideTechnique {
        self.hide_as
            .unwrap_or_else(|| probe::code_toggled_hide(self.panel))
    }

    /// Whether the game's code sets `key` on this panel, so the maker never edits it.
    pub fn owns(&self, key: &str) -> bool {
        self.owned.iter().any(|k| k.eq_ignore_ascii_case(key))
    }

    /// How the layout fits the panel to the screen.
    pub(super) fn fit(&self) -> Fit {
        self.fit
    }
}

/// One scene's files, part rules, code-toggled panels and preview sample.
#[derive(Debug)]
#[non_exhaustive]
pub struct SceneRule {
    scene: Scene,
    label: &'static str,
    files: &'static [&'static str],
    /// Files only a secondary root reads: missing, they leave those parts unreadable and the
    /// scene still draws.
    extras: &'static [&'static str],
    read_only: &'static [&'static str],
    root: Option<&'static str>,
    rules: &'static [PanelRule],
    code_toggled: &'static [CodeToggled],
    code_classes: &'static [(&'static str, &'static str)],
    toggle_any_depth: bool,
    nested: &'static [(&'static str, &'static str)],
    hides_hud: bool,
    sample: SampleSet,
    mode: BoardMode,
    conditions: &'static [&'static str],
}

const fn toggled(panel: &'static str, shown: bool, shows_when: &'static str) -> CodeToggled {
    CodeToggled {
        panel,
        shown,
        shows_for: None,
        shows_when,
        owned: &[],
        hide_as: None,
        fit: Fit::None,
    }
}

// A toggled panel whose box the game's code fits: it owns `owned`, hides only as `hide_as`.
const fn fitted(
    base: CodeToggled,
    owned: &'static [&'static str],
    hide_as: HideTechnique,
    fit: Fit,
) -> CodeToggled {
    CodeToggled {
        owned,
        hide_as: Some(hide_as),
        fit,
        ..base
    }
}

// A HUD panel the game shows only while `shows_for` holds; the sample decides from it.
const fn when(panel: &'static str, shows_for: ShowsFor, shows_when: &'static str) -> CodeToggled {
    CodeToggled {
        panel,
        shown: false,
        shows_for: Some(shows_for),
        shows_when,
        owned: &[],
        hide_as: None,
        fit: Fit::None,
    }
}

const CLIP: ShowsFor = ShowsFor::Weapon("clip");
const CLIP_WHEN: &str = "Shows for weapons with a clip";
const NO_CLIP: ShowsFor = ShowsFor::Weapon("no clip");
const NO_CLIP_WHEN: &str = "Shows for weapons without a clip";
const MODEL: ShowsFor = ShowsFor::Mode("3D model");
const MODEL_WHEN: &str = "Shows while the 3D class model is on";
const PORTRAIT: ShowsFor = ShowsFor::Mode("2D portrait");
const PORTRAIT_WHEN: &str = "Shows when the 3D class model is off";
const CONDITION: ShowsFor = ShowsFor::Effect("condition");
const CONDITION_WHEN: &str = "Shows while an effect such as bleeding or a buff is on you";
const POWERUP: ShowsFor = ShowsFor::Effect("powerup");
const POWERUP_WHEN: &str = "Shows while you carry a Mannpower powerup";
const TYPING: ShowsFor = ShowsFor::Mode("typing");
const TYPING_WHEN: &str = "Shows while you type a chat message";

// The KOTH panel shows only on a King of the Hill map, in the HUD and the freeze cam alike.
const KOTH_TOGGLED: CodeToggled = fitted(
    when(
        "HudKothTimeStatus",
        ShowsFor::Mode("koth"),
        "Shows on King of the Hill maps",
    ),
    &[],
    HideTechnique::OffScreenY,
    Fit::None,
);

// What the game checks before it shows each HUD part its code toggles; the preview shows the
// default situation, `ShowsFor::in_default`.
static HUD_TOGGLED: [CodeToggled; 55] = [
    when(
        "CMainTargetID",
        ShowsFor::Aim("player"),
        "Shows while you aim at a player",
    ),
    fitted(
        when(
            "TargetNameLabel",
            ShowsFor::Aim("player"),
            "Shows the name of the player you aim at",
        ),
        LABEL_COLOUR_OWNED,
        HideTechnique::SizeZero,
        Fit::None,
    ),
    fitted(
        when(
            "TargetDataLabel",
            ShowsFor::Aim("player"),
            "Shows a line about the player you aim at, such as a Medic's charge",
        ),
        LABEL_COLOUR_OWNED,
        HideTechnique::SizeZero,
        Fit::None,
    ),
    when(
        "HudMedicCharge",
        ShowsFor::Class("Medic"),
        "Shows when you hold a Medi Gun",
    ),
    when(
        "HudItemEffectMeter",
        ShowsFor::Weapon("meter"),
        "Shows with weapons that have a meter, such as the Sandman",
    ),
    when("AmmoInClip", CLIP, CLIP_WHEN),
    when("AmmoInClipShadow", CLIP, CLIP_WHEN),
    when("AmmoInReserve", CLIP, CLIP_WHEN),
    when("AmmoInReserveShadow", CLIP, CLIP_WHEN),
    when("AmmoNoClip", NO_CLIP, NO_CLIP_WHEN),
    when("AmmoNoClipShadow", NO_CLIP, NO_CLIP_WHEN),
    when(
        "PlayerStatusHealthBonusImage",
        ShowsFor::Health("pulse"),
        "Shows while you are overhealed or low on health",
    ),
    when(
        "HudWeaponLowAmmoImage",
        ShowsFor::Weapon("low ammo"),
        "Shows when your ammo runs low",
    ),
    when("PlayerStatusClassImage", PORTRAIT, PORTRAIT_WHEN),
    when("PlayerStatusClassImageBG", PORTRAIT, PORTRAIT_WHEN),
    when("classmodelpanel", MODEL, MODEL_WHEN),
    when("classmodelpanelBG", MODEL, MODEL_WHEN),
    when(
        "PlayerStatusSpyImage",
        ShowsFor::Class("Spy"),
        "Shows while you are disguised",
    ),
    when(
        "CarryingWeapon",
        ShowsFor::Class("Engineer"),
        "Shows while you carry a building",
    ),
    when(
        "MoveableSubPanel",
        ShowsFor::Class("Engineer"),
        "Shows when you aim at a building you can move",
    ),
    when("PlayerStatusBleedImage", CONDITION, CONDITION_WHEN),
    when("PlayerStatusHookBleedImage", CONDITION, CONDITION_WHEN),
    when("PlayerStatusMilkImage", CONDITION, CONDITION_WHEN),
    when("PlayerStatusGasImage", CONDITION, CONDITION_WHEN),
    when("PlayerStatusMarkedForDeathImage", CONDITION, CONDITION_WHEN),
    when(
        "PlayerStatusMarkedForDeathSilentImage",
        CONDITION,
        CONDITION_WHEN,
    ),
    when(
        "PlayerStatus_MedicUberBulletResistImage",
        CONDITION,
        CONDITION_WHEN,
    ),
    when(
        "PlayerStatus_MedicUberBlastResistImage",
        CONDITION,
        CONDITION_WHEN,
    ),
    when(
        "PlayerStatus_MedicUberFireResistImage",
        CONDITION,
        CONDITION_WHEN,
    ),
    when(
        "PlayerStatus_MedicSmallBulletResistImage",
        CONDITION,
        CONDITION_WHEN,
    ),
    when(
        "PlayerStatus_MedicSmallBlastResistImage",
        CONDITION,
        CONDITION_WHEN,
    ),
    when(
        "PlayerStatus_MedicSmallFireResistImage",
        CONDITION,
        CONDITION_WHEN,
    ),
    when("PlayerStatus_WheelOfDoom", CONDITION, CONDITION_WHEN),
    when("PlayerStatus_SoldierOffenseBuff", CONDITION, CONDITION_WHEN),
    when("PlayerStatus_SoldierDefenseBuff", CONDITION, CONDITION_WHEN),
    when(
        "PlayerStatus_SoldierHealOnHitBuff",
        CONDITION,
        CONDITION_WHEN,
    ),
    when("PlayerStatus_SpyMarked", CONDITION, CONDITION_WHEN),
    when("PlayerStatus_Parachute", CONDITION, CONDITION_WHEN),
    when("PlayerStatusSlowed", CONDITION, CONDITION_WHEN),
    when("PlayerStatus_RuneStrength", POWERUP, POWERUP_WHEN),
    when("PlayerStatus_RuneHaste", POWERUP, POWERUP_WHEN),
    when("PlayerStatus_RuneRegen", POWERUP, POWERUP_WHEN),
    when("PlayerStatus_RuneResist", POWERUP, POWERUP_WHEN),
    when("PlayerStatus_RuneVampire", POWERUP, POWERUP_WHEN),
    when("PlayerStatus_RuneReflect", POWERUP, POWERUP_WHEN),
    when("PlayerStatus_RunePrecision", POWERUP, POWERUP_WHEN),
    when("PlayerStatus_RuneAgility", POWERUP, POWERUP_WHEN),
    when("PlayerStatus_RuneKnockout", POWERUP, POWERUP_WHEN),
    when("PlayerStatus_RuneKing", POWERUP, POWERUP_WHEN),
    when("PlayerStatus_RunePlague", POWERUP, POWERUP_WHEN),
    when("PlayerStatus_RuneSupernova", POWERUP, POWERUP_WHEN),
    // CBaseHudChat::OnTick sets the input line's ypos and tall itself, so the file's
    // off-screen ypos never hides it.
    CodeToggled {
        owned: &["ypos", "tall"],
        fit: Fit::ChatInput,
        ..when("ChatInputLine", TYPING, TYPING_WHEN)
    },
    when("ChatFiltersButton", TYPING, TYPING_WHEN),
    // On a King of the Hill map the KOTH timers take the round timer's place; both keep the
    // round timer's off-screen hide.
    KOTH_TOGGLED,
    fitted(
        when(
            "ObjectiveStatusTimePanel",
            ShowsFor::Mode("not koth"),
            "Shows in a timed round, except on King of the Hill maps",
        ),
        &[],
        HideTechnique::OffScreenY,
        Fit::None,
    ),
];

/// The round timer's children file, which each KOTH team timer loads too.
const TIME_PANEL: &str = "resource/ui/hudobjectivetimepanel.res";

// The HUD scene reads hudlayout, then each rule's geometry and children files in rule order.
static HUD: SceneRule = SceneRule {
    scene: Scene::Hud,
    label: "HUD",
    files: &[
        "scripts/hudlayout.res",
        "resource/ui/hudplayerhealth.res",
        "resource/ui/hudplayerclass.res",
        "resource/ui/hudammoweapons.res",
        "resource/ui/hudmediccharge.res",
        "resource/ui/hudhealthaccount.res",
        "resource/ui/huddamageaccount.res",
        "resource/ui/targetid.res",
        "resource/ui/huditemeffectmeter.res",
        "resource/ui/hudmatchstatus.res",
        "resource/ui/hudobjectivetimepanel.res",
        "resource/ui/basechat.res",
        "resource/ui/hudobjectivekothtimepanel.res",
        // Each weapon's own meter file, in `MeterKind::ALL` order after Plain.
        "resource/ui/huditemeffectmeter_scout.res",
        "resource/ui/huditemeffectmeter_sodapopper.res",
        "resource/ui/huditemeffectmeter_cleaver.res",
        "resource/ui/huditemeffectmeter_heavy.res",
        "resource/ui/huditemeffectmeter_sniper.res",
        "resource/ui/huditemeffectmeter_sniperfocus.res",
        "resource/ui/huditemeffectmeter_demoman.res",
        "resource/ui/huditemeffectmeter_particlecannon.res",
        "resource/ui/huditemeffectmeter_raygun.res",
        "resource/ui/huditemeffectmeter_spyknife.res",
        "resource/ui/huditemeffectmeter_sapper.res",
        "resource/ui/huditemeffectmeter_spy.res",
        "resource/ui/huditemeffectmeter_engineer.res",
        "resource/ui/huditemeffectmeter_pomson.res",
        "resource/ui/huditemeffectmeter_pyro.res",
        "resource/ui/hudrocketpack.res",
        "resource/ui/huditemeffectmeter_organs.res",
        "resource/ui/huditemeffectmeter_killstreak.res",
        "resource/ui/huditemeffectmeter_kartcharge.res",
        "resource/ui/huditemeffectmeter_powerupbottle.res",
    ],
    // A meter file the HUD and the game both lack only leaves that meter unreadable.
    extras: &[
        "resource/ui/huditemeffectmeter_scout.res",
        "resource/ui/huditemeffectmeter_sodapopper.res",
        "resource/ui/huditemeffectmeter_cleaver.res",
        "resource/ui/huditemeffectmeter_heavy.res",
        "resource/ui/huditemeffectmeter_sniper.res",
        "resource/ui/huditemeffectmeter_sniperfocus.res",
        "resource/ui/huditemeffectmeter_demoman.res",
        "resource/ui/huditemeffectmeter_particlecannon.res",
        "resource/ui/huditemeffectmeter_raygun.res",
        "resource/ui/huditemeffectmeter_spyknife.res",
        "resource/ui/huditemeffectmeter_sapper.res",
        "resource/ui/huditemeffectmeter_spy.res",
        "resource/ui/huditemeffectmeter_engineer.res",
        "resource/ui/huditemeffectmeter_pomson.res",
        "resource/ui/huditemeffectmeter_pyro.res",
        "resource/ui/hudrocketpack.res",
        "resource/ui/huditemeffectmeter_organs.res",
        "resource/ui/huditemeffectmeter_killstreak.res",
        "resource/ui/huditemeffectmeter_kartcharge.res",
        "resource/ui/huditemeffectmeter_powerupbottle.res",
    ],
    read_only: &[],
    root: None,
    rules: &RULES,
    code_toggled: &HUD_TOGGLED,
    code_classes: &[],
    toggle_any_depth: false,
    // Each KOTH team timer also loads the round timer's file, as the round timer does.
    nested: &[("BlueTimer", TIME_PANEL), ("RedTimer", TIME_PANEL)],
    hides_hud: false,
    sample: SampleSet::Empty,
    mode: BoardMode::Casual,
    conditions: &[],
};

// Visibility the scoreboard's code sets each update (`tf_clientscoreboard.cpp`), with the
// preview default under a casual match with the 3D class model on.
const BOARD_TOGGLED: [CodeToggled; 15] = [
    toggled("scores", true, HOLD_SCORES),
    toggled(
        "BlueTeamImage",
        true,
        "Shows unless a team has a party leader or in Mann vs. Machine",
    ),
    toggled(
        "RedTeamImage",
        true,
        "Shows unless a team has a party leader or in Mann vs. Machine",
    ),
    toggled(
        "BlueLeaderAvatar",
        false,
        "Shows when a team has a party leader",
    ),
    toggled(
        "BlueLeaderAvatarBG",
        false,
        "Shows when a team has a party leader",
    ),
    toggled(
        "RedLeaderAvatar",
        false,
        "Shows when a team has a party leader",
    ),
    toggled(
        "RedLeaderAvatarBG",
        false,
        "Shows when a team has a party leader",
    ),
    toggled("BlueTeamLabel", false, "Shows in tournament mode"),
    toggled("RedTeamLabel", false, "Shows in tournament mode"),
    toggled("ClassImage", false, "Shows when the 3D class model is off"),
    toggled(
        "classmodelpanel",
        true,
        "Shows while the 3D class model is on",
    ),
    toggled(
        "LocalPlayerStatsPanel",
        true,
        "Shows unless you are in a duel",
    ),
    toggled(
        "LocalPlayerDuelStatsPanel",
        false,
        "Shows while you are in a duel",
    ),
    toggled("BluePlayerList", true, "Shows outside co-op modes"),
    toggled("RedPlayerList", true, "Shows outside co-op modes"),
];

// Competitive shows the two tournament team names; every other line is the casual one.
const COMPETITIVE_TOGGLED: [CodeToggled; 15] = {
    let mut table = BOARD_TOGGLED;
    table[7] = toggled("BlueTeamLabel", true, "Shows in tournament mode");
    table[8] = toggled("RedTeamLabel", true, "Shows in tournament mode");
    table
};

const MVM_NO_BOARD: &str = "Shows outside Mann vs. Machine";

// In MvM the dialog drops the team images, names and lists for its own table; the selected
// player strip stays (assumed, not checked in game) and the mission name shows for a named mission.
static MVM_TOGGLED: [CodeToggled; 17] = [
    toggled("scores", true, HOLD_SCORES),
    toggled("BlueTeamImage", false, MVM_NO_BOARD),
    toggled("RedTeamImage", false, MVM_NO_BOARD),
    toggled("BlueLeaderAvatar", false, MVM_NO_BOARD),
    toggled("BlueLeaderAvatarBG", false, MVM_NO_BOARD),
    toggled("RedLeaderAvatar", false, MVM_NO_BOARD),
    toggled("RedLeaderAvatarBG", false, MVM_NO_BOARD),
    toggled("BlueTeamLabel", false, MVM_NO_BOARD),
    toggled("RedTeamLabel", false, MVM_NO_BOARD),
    toggled("ClassImage", false, "Shows when the 3D class model is off"),
    toggled(
        "classmodelpanel",
        true,
        "Shows while the 3D class model is on",
    ),
    toggled(
        "LocalPlayerStatsPanel",
        true,
        "Shows unless you are in a duel",
    ),
    toggled(
        "LocalPlayerDuelStatsPanel",
        false,
        "Shows while you are in a duel",
    ),
    toggled("BluePlayerList", false, MVM_NO_BOARD),
    toggled("RedPlayerList", false, MVM_NO_BOARD),
    toggled(
        "DifficultyContainer",
        true,
        "Shows when the mission has a name",
    ),
    toggled(
        "RespecStatusLabel",
        false,
        "Shows when you can upgrade again",
    ),
];

const BOARD_BASE: SceneRule = SceneRule {
    scene: Scene::Scoreboard,
    label: "Scoreboard",
    files: &["resource/ui/scoreboard.res"],
    extras: &[],
    read_only: &[],
    root: Some("scores"),
    rules: std::slice::from_ref(&SCOREBOARD),
    code_toggled: &BOARD_TOGGLED,
    code_classes: &[],
    toggle_any_depth: false,
    nested: &[],
    hides_hud: true,
    sample: SampleSet::Casual,
    mode: BoardMode::Casual,
    conditions: &[],
};

static BOARD: SceneRule = BOARD_BASE;

// Competitive is the casual dialog with tournament names; later tasks give it its own table
// and sample.
static BOARD_COMPETITIVE: SceneRule = SceneRule {
    mode: BoardMode::Competitive,
    code_toggled: &COMPETITIVE_TOGGLED,
    sample: SampleSet::Competitive,
    ..BOARD_BASE
};

// MvM reads the nested MvM file with `if_mvm` on and matches toggles at any depth.
static BOARD_MVM: SceneRule = SceneRule {
    mode: BoardMode::Mvm,
    files: &[
        "resource/ui/scoreboard.res",
        "resource/ui/mvmscoreboard.res",
    ],
    nested: &[("MvMScoreboard", "resource/ui/mvmscoreboard.res")],
    conditions: &["if_mvm"],
    code_toggled: &MVM_TOGGLED,
    sample: SampleSet::Mvm,
    toggle_any_depth: true,
    ..BOARD_BASE
};

const HIDDEN: &str = "The game keeps this hidden";
const GC: &str = "Shows while TF2 is connected to its item server";
const IN_GAME: &str = "Shows while you are in a game";

// What the main menu's code (`CHudMainMenuOverride`) shows at the menu: not in a game or a
// replay, connected to the item server, no holiday, VR and safe mode off, nothing unread.
// The SDK's code never names the two `*SDK` settings buttons, but the game keeps them hidden;
// the file stacks them on the replay and bug-report buttons.
static MENU_TOGGLED: [CodeToggled; 30] = [
    toggled(
        "Background",
        false,
        "Shows a holiday picture at Halloween and Christmas",
    ),
    toggled(
        "TFCharacterImage",
        true,
        "Shows at the main menu outside some holiday pictures",
    ),
    toggled("RankPanel", true, GC),
    toggled("RankModelPanel", true, GC),
    toggled("CycleRankTypeButton", true, GC),
    toggled("RankTooltipPanel", false, "Shows beside a competitive rank"),
    toggled("NoGCMessage", false, HIDDEN),
    toggled("NoGCImage", false, HIDDEN),
    toggled("RankBorder", false, HIDDEN),
    toggled("GeneralStoreButton", false, HIDDEN),
    toggled("WatchStreamButton", false, HIDDEN),
    toggled("QuestLogButton", false, HIDDEN),
    toggled("SettingsButtonSDK", false, HIDDEN),
    toggled("TF2SettingsButtonSDK", false, HIDDEN),
    toggled("EventPromo", false, HIDDEN),
    toggled(
        "Notifications_ShowButtonPanel",
        false,
        "Shows when you have unread notifications",
    ),
    toggled(
        "Notifications_Panel",
        false,
        "Shows when you open a notification",
    ),
    toggled(
        "MOTD_ShowButtonPanel",
        false,
        "Shows when there is unread news",
    ),
    toggled("MOTD_Panel", false, "Shows when you open the news"),
    toggled("SafeMode", false, "Shows in the main menu's safe mode"),
    toggled(
        "StoreHasNewItemsImage",
        false,
        "Shows when the store has new items",
    ),
    toggled(
        "ShowPromoCodesButton",
        false,
        "Shows when you have promo codes to claim",
    ),
    toggled(
        "VRBGPanel",
        false,
        "Shows behind the promo codes or VR button",
    ),
    toggled("VRModeButton", false, "Shows while VR mode is on"),
    toggled("CallVoteButton", false, IN_GAME),
    toggled("MutePlayersButton", false, IN_GAME),
    toggled("RequestCoachButton", false, IN_GAME),
    toggled("ReportPlayerButton", false, IN_GAME),
    toggled(
        "BackToReplaysButton",
        false,
        "Shows while you watch a replay",
    ),
    toggled(
        "TooltipPanel",
        false,
        "Shows while you point at a button with a tip",
    ),
];

// The main menu reads its override file over the game's own menu; the HUD is not drawn there.
// The code makes the class picture itself, so its block names no `ControlName`.
static MENU: SceneRule = SceneRule {
    scene: Scene::MainMenu,
    label: "Main menu",
    files: &["resource/ui/mainmenuoverride.res"],
    // Decoration the preview reads but never edits or reports; see RankLayout.
    extras: &[],
    read_only: &["resource/ui/pvprankpanel.res"],
    root: Some("MainMenuOverride"),
    rules: std::slice::from_ref(&MAIN_MENU),
    code_toggled: &MENU_TOGGLED,
    code_classes: &[("TFCharacterImage", "ImagePanel")],
    toggle_any_depth: false,
    nested: &[],
    hides_hud: true,
    sample: SampleSet::Empty,
    mode: BoardMode::Casual,
    conditions: &[],
};

// What the freeze cam's code shows or hides (`tf_hud_freezepanel.cpp`, by function name):
// the screenshot hint only with a screenshot key bound, the nemesis and item panels only in
// their situations. The last two are the killer health panel's, assumed rather than checked in game.
static FREEZE_TOGGLED: [CodeToggled; 9] = [
    toggled(
        "ScreenshotPanel",
        true,
        "Shows when you can save a screenshot",
    ),
    toggled(
        "itempanel",
        false,
        "Shows when your killer carries a special item",
    ),
    toggled(
        "NemesisSubPanel",
        false,
        "Shows when your killer is dominating you",
    ),
    toggled(
        "PlayerStatusHealthBonusImage",
        false,
        "Shows while your killer is overhealed",
    ),
    toggled(
        "BuildingStatusHealthImageBG",
        false,
        "Shows when your killer is a building",
    ),
    // The HUD parts kept over the freeze cam: the KOTH panel only on a KOTH map, its timers'
    // digits blank until the respawn count starts, and no chat typing in the sample.
    KOTH_TOGGLED,
    CHAT_INPUT_TOGGLED,
    CHAT_FILTERS_TOGGLED,
    KOTH_DIGITS_TOGGLED,
];

// The HUD parts the game keeps while you are dead, in the freeze cam and the spectator screen:
// nobody types in the sample; in the freeze cam the KOTH timers also blank their digits.
const CHAT_INPUT_TOGGLED: CodeToggled = toggled("ChatInputLine", false, TYPING_WHEN);
const CHAT_FILTERS_TOGGLED: CodeToggled = toggled("ChatFiltersButton", false, TYPING_WHEN);
const KOTH_DIGITS_TOGGLED: CodeToggled = toggled(
    "TimePanelValue",
    false,
    "Shows the time left while you are alive",
);
const DEAD_HUD_FILES: [&str; 3] = [
    "resource/ui/basechat.res",
    "resource/ui/hudobjectivekothtimepanel.res",
    TIME_PANEL,
];

// The freeze cam reads its root's box from hudlayout, the panel file and the killer health
// file; of the HUD only the killfeed, chat and KOTH panel stay, read from their own files.
static FREEZE: SceneRule = SceneRule {
    scene: Scene::FreezeCam,
    label: "Death and freeze cam",
    files: &[
        "scripts/hudlayout.res",
        "resource/ui/freezepanel_basic.res",
        "resource/ui/freezepanelkillerhealth.res",
        DEAD_HUD_FILES[0],
        DEAD_HUD_FILES[1],
        DEAD_HUD_FILES[2],
    ],
    extras: &DEAD_HUD_FILES,
    read_only: &[],
    root: Some("FreezePanel"),
    rules: &FREEZE_PANELS,
    code_toggled: &FREEZE_TOGGLED,
    code_classes: &[],
    toggle_any_depth: true,
    nested: &[
        (
            "FreezePanelHealth",
            "resource/ui/freezepanelkillerhealth.res",
        ),
        ("BlueTimer", TIME_PANEL),
        ("RedTimer", TIME_PANEL),
    ],
    hides_hud: true,
    sample: SampleSet::Killed,
    mode: BoardMode::Casual,
    conditions: &[],
};

const ROUND_END: &str = "Shows when a round ends";
const NO_LEADER: &str = "Shows unless a team has a party leader";
const LEADER: &str = "Shows when a team has a party leader";
const SCORED: &str = "Shows when the player scored this round";
const RANKED: &str = "Shows in games with player ranks";
const STREAK: &str = "Shows when a player has a killstreak";

// What the win panel's code shows or hides (`CTFWinPanel::FireGameEvent` and `UpdateTeamInfo`,
// by function name): the scores only at a round's end, the leader pictures only for premade
// parties, the player rows only for a player who scored, the badges only with player ranks.
static WIN_TOGGLED: [CodeToggled; 27] = [
    toggled("TeamScoresPanel", true, ROUND_END),
    toggled("BlueTeamLabel", true, NO_LEADER),
    toggled("RedTeamLabel", true, NO_LEADER),
    toggled("BlueLeaderAvatar", false, LEADER),
    toggled("BlueLeaderAvatarBG", false, LEADER),
    toggled("RedLeaderAvatar", false, LEADER),
    toggled("RedLeaderAvatarBG", false, LEADER),
    toggled("Player1Avatar", true, SCORED),
    toggled("Player1Name", true, SCORED),
    toggled("Player1Class", true, SCORED),
    toggled("Player1Score", true, SCORED),
    toggled("Player2Avatar", true, SCORED),
    toggled("Player2Name", true, SCORED),
    toggled("Player2Class", true, SCORED),
    toggled("Player2Score", true, SCORED),
    toggled("Player3Avatar", true, SCORED),
    toggled("Player3Name", true, SCORED),
    toggled("Player3Class", true, SCORED),
    toggled("Player3Score", true, SCORED),
    toggled("Player1Badge", false, RANKED),
    toggled("Player2Badge", false, RANKED),
    toggled("Player3Badge", false, RANKED),
    toggled("KillStreakPlayer1Avatar", true, STREAK),
    toggled("KillStreakPlayer1Name", true, STREAK),
    toggled("KillStreakPlayer1Class", true, STREAK),
    toggled("KillStreakPlayer1Score", true, STREAK),
    toggled("KillStreakPlayer1Badge", false, RANKED),
];

// The win panel reads its root's box from hudlayout and its controls from one file; the HUD
// is not drawn there. Its code toggles match at any depth because the scores sit one level down.
static WIN: SceneRule = SceneRule {
    scene: Scene::WinPanel,
    label: "Win panel",
    files: &["scripts/hudlayout.res", "resource/ui/winpanel.res"],
    extras: &[],
    read_only: &[],
    root: Some("WinPanel"),
    rules: std::slice::from_ref(&WIN_PANEL),
    code_toggled: &WIN_TOGGLED,
    code_classes: &[],
    toggle_any_depth: true,
    nested: &[],
    hides_hud: true,
    sample: SampleSet::Won,
    mode: BoardMode::Casual,
    conditions: &[],
};

const CHOSEN: &str = "Shows once you have picked a class";
const SELECTED: &str = "Shows while a class is selected";
const TEAMMATE: &str = "Shows for each teammate who has picked a class";
const MVM_CLASS: &str = "Shows in Mann vs. Machine for a class you upgraded";

// What the class menu's code shows or hides (`CTFClassMenu`, by function name): the cancel
// button, model and local picture once a class is chosen, the teammate pictures per teammate,
// and the Highlander and Mann vs. Machine controls only in those modes.
static CLASS_TOGGLED: [CodeToggled; 29] = [
    toggled("CancelButton", true, CHOSEN),
    toggled(
        "ClassMenuSelect",
        false,
        "Shows until you have picked a class",
    ),
    toggled("ResetButton", false, "Shows in Highlander matches"),
    toggled("EditLoadoutButton", true, SELECTED),
    toggled("TFPlayerModel", true, SELECTED),
    toggled("localPlayerImage", true, CHOSEN),
    toggled("localPlayerBG", true, CHOSEN),
    toggled(
        "CountLabel",
        true,
        "Shows when a teammate has picked a class",
    ),
    toggled("countImage0", true, TEAMMATE),
    toggled("countImage1", true, TEAMMATE),
    toggled("countImage2", true, TEAMMATE),
    toggled("countImage3", true, TEAMMATE),
    toggled("countImage4", true, TEAMMATE),
    toggled("countImage5", true, TEAMMATE),
    toggled("countImage6", true, TEAMMATE),
    toggled("countImage7", true, TEAMMATE),
    toggled("countImage8", true, TEAMMATE),
    toggled("countImage9", true, TEAMMATE),
    toggled("countImage10", true, TEAMMATE),
    toggled("MvMUpgradeImageScout", false, MVM_CLASS),
    toggled("MvMUpgradeImageSolider", false, MVM_CLASS),
    toggled("MvMUpgradeImagePyro", false, MVM_CLASS),
    toggled("MvMUpgradeImageDemoman", false, MVM_CLASS),
    toggled("MvMUpgradeImageHeavy", false, MVM_CLASS),
    toggled("MvMUpgradeImageEngineer", false, MVM_CLASS),
    toggled("MvMUpgradeImageMedic", false, MVM_CLASS),
    toggled("MvMUpgradeImageSniper", false, MVM_CLASS),
    toggled("MvMUpgradeImageSpy", false, MVM_CLASS),
    toggled(
        "StartExplanation",
        false,
        "Shows in Mann vs. Machine when you first upgrade a class",
    ),
];

// The class menu is one file whose first block is the menu's own box; the HUD is not drawn.
static CLASS: SceneRule = SceneRule {
    scene: Scene::ClassSelect,
    label: "Class select",
    files: &["resource/ui/classselection.res"],
    extras: &[],
    read_only: &[],
    root: Some("class"),
    rules: std::slice::from_ref(&CLASS_SELECT),
    code_toggled: &CLASS_TOGGLED,
    code_classes: &[],
    toggle_any_depth: false,
    nested: &[],
    hides_hud: true,
    sample: SampleSet::ClassPick,
    mode: BoardMode::Casual,
    conditions: &[],
};

const WATCHING: &str = "Shows while you watch other players";
const WATCHING_FULL: &str = "Shows while you watch other players with Minimal HUD off";
const CAMERA: &str = "Shows when the server lets you change the camera mode";
const PICKING_FULL: &str = "Shows when you may pick who to watch, with Minimal HUD off";
// Minimal HUD drops the bars and the key hints from the spectator screen.
const FULL_HUD: ShowsFor = ShowsFor::Mode("full hud");

// What the spectator code (`CTFSpectatorGUI` and `CSpectatorGUI`, by function name) shows or
// fits: the bars are turned on and sized after the file loads, the camera hint hidden under the
// default mp_forcecamera 1, the buy-back line and item panel kept for their modes.
const TARGET: &str = "Shows while you watch a player in first person";
// The HUD tab's target ID labels: the game sets their colour on every update. Their x is the
// file's here, so only the colour keys are owned. Whether fgcolor_override is also overwritten is
// unverified, so both are treated as owned.
const LABEL_COLOUR_OWNED: &[&str] = &["fgcolor", "fgcolor_override"];

// The target ID labels' x and colour are the game's: the code places them after the health
// panel and overwrites their colour with the scheme's label colour.
const LABEL_OWNED: &[&str] = &["xpos", "fgcolor", "fgcolor_override"];
static SPECTATOR_TOGGLED: [CodeToggled; 26] = [
    toggled("Spectator", true, WATCHING),
    fitted(
        when("topbar", FULL_HUD, WATCHING_FULL),
        &["xpos", "ypos", "wide"],
        HideTechnique::TallZero,
        Fit::TopBar,
    ),
    fitted(
        when("bottombarblank", FULL_HUD, WATCHING_FULL),
        &["xpos", "ypos", "wide", "tall"],
        HideTechnique::NotOffered,
        Fit::BottomBar,
    ),
    toggled(
        "ReinforcementsLabel",
        true,
        "Shows to a dead player waiting to respawn",
    ),
    toggled("SwitchCamModeKeyLabel", false, CAMERA),
    toggled("SwitchCamModeLabel", false, CAMERA),
    when("CycleTargetFwdKeyLabel", FULL_HUD, PICKING_FULL),
    when("CycleTargetFwdLabel", FULL_HUD, PICKING_FULL),
    when("CycleTargetRevKeyLabel", FULL_HUD, PICKING_FULL),
    when("CycleTargetRevLabel", FULL_HUD, PICKING_FULL),
    toggled(
        "BuyBackLabel",
        false,
        "Shows in Mann vs. Machine when you can buy back in",
    ),
    toggled(
        "itempanel",
        false,
        "Shows when the player you watch carries a special item",
    ),
    toggled("CSpectatorTargetID", true, TARGET),
    toggled("SpectatorGUIHealth", true, TARGET),
    toggled(
        "TargetIDBG_Spec_Red",
        true,
        "Shows when the player you watch is on RED",
    ),
    toggled(
        "TargetIDBG",
        false,
        "Shows in the HUD's target ID, never on the spectator screen",
    ),
    toggled(
        "TargetIDBG_Spec_Blue",
        false,
        "Shows when the player you watch is on BLU",
    ),
    toggled(
        "AvatarImage",
        false,
        "Shows when the player you watch has a Steam avatar to draw",
    ),
    toggled(
        "AmmoIcon",
        false,
        "Shows to a Medic healing a player with a clip",
    ),
    toggled(
        "KillStreakIcon",
        false,
        "Shows when the player you watch has a killstreak",
    ),
    toggled(
        "MoveableSubPanel",
        false,
        "Shows when you can pick up or move what you aim at",
    ),
    fitted(
        toggled(
            "TargetNameLabel",
            true,
            "Shows the name of the player you watch",
        ),
        LABEL_OWNED,
        HideTechnique::SizeZero,
        Fit::AfterSibling {
            sibling: "SpectatorGUIHealth",
            gap: 8,
        },
    ),
    fitted(
        toggled(
            "TargetDataLabel",
            true,
            "Shows a line about the player you watch, such as a Medic's charge",
        ),
        LABEL_OWNED,
        HideTechnique::SizeZero,
        Fit::AfterSibling {
            sibling: "SpectatorGUIHealth",
            gap: 8,
        },
    ),
    // The HUD parts kept while dead; the KOTH digits, three levels down, keep the file's state.
    KOTH_TOGGLED,
    CHAT_INPUT_TOGGLED,
    CHAT_FILTERS_TOGGLED,
];

// The spectator screen is one file whose first block is the screen's own box; of the HUD only
// the killfeed, chat and KOTH panel stay over it, as in the freeze cam.
static SPECTATE: SceneRule = SceneRule {
    scene: Scene::Spectator,
    label: "Spectator",
    files: &[
        "resource/ui/spectator.res",
        "scripts/hudlayout.res",
        "resource/ui/targetid.res",
        "resource/ui/spectatorguihealth.res",
        DEAD_HUD_FILES[0],
        DEAD_HUD_FILES[1],
        DEAD_HUD_FILES[2],
    ],
    extras: &[
        "scripts/hudlayout.res",
        "resource/ui/targetid.res",
        "resource/ui/spectatorguihealth.res",
        DEAD_HUD_FILES[0],
        DEAD_HUD_FILES[1],
        DEAD_HUD_FILES[2],
    ],
    read_only: &[],
    root: Some("Spectator"),
    rules: &SPECTATOR_PANELS,
    code_toggled: &SPECTATOR_TOGGLED,
    code_classes: &[],
    toggle_any_depth: false,
    nested: &[
        ("SpectatorGUIHealth", "resource/ui/spectatorguihealth.res"),
        ("BlueTimer", TIME_PANEL),
        ("RedTimer", TIME_PANEL),
    ],
    hides_hud: true,
    sample: SampleSet::Spectating,
    mode: BoardMode::Casual,
    conditions: &[],
};

impl SceneRule {
    /// The rule for `scene`.
    pub fn get(scene: Scene) -> &'static SceneRule {
        match scene {
            Scene::Hud => &HUD,
            Scene::Scoreboard => &BOARD,
            Scene::MainMenu => &MENU,
            Scene::FreezeCam => &FREEZE,
            Scene::WinPanel => &WIN,
            Scene::ClassSelect => &CLASS,
            Scene::Spectator => &SPECTATE,
        }
    }

    /// The rule for `scene` in `mode`; only the Scoreboard has one per mode.
    pub fn for_mode(scene: Scene, mode: BoardMode) -> &'static SceneRule {
        match (scene, mode) {
            (Scene::Scoreboard, BoardMode::Competitive) => &BOARD_COMPETITIVE,
            (Scene::Scoreboard, BoardMode::Mvm) => &BOARD_MVM,
            _ => SceneRule::get(scene),
        }
    }

    /// Every scene's rule once, then the Scoreboard's Competitive and Mvm rules.
    pub fn every() -> &'static [&'static SceneRule] {
        static EVERY: [&SceneRule; 9] = [
            &HUD,
            &BOARD,
            &MENU,
            &FREEZE,
            &WIN,
            &CLASS,
            &SPECTATE,
            &BOARD_COMPETITIVE,
            &BOARD_MVM,
        ];
        &EVERY
    }

    /// The match mode this rule previews; Casual outside the Scoreboard.
    pub fn mode(&self) -> BoardMode {
        self.mode
    }

    /// The conditional blocks (such as `if_mvm`) this rule turns on when it reads files.
    pub fn conditions(&self) -> &'static [&'static str] {
        self.conditions
    }

    /// The scene this rule describes.
    pub fn scene(&self) -> Scene {
        self.scene
    }

    /// The scene's name as players see it.
    pub fn label(&self) -> &'static str {
        self.label
    }

    /// The lowercase game paths the scene reads.
    pub fn files(&self) -> &'static [&'static str] {
        self.files
    }

    /// The files only a secondary root reads; a scene still draws without them.
    pub fn extras(&self) -> &'static [&'static str] {
        self.extras
    }

    /// The files whose absence or errors are the scene's problems: `files` minus `extras`.
    pub fn problem_files(&self) -> Vec<&'static str> {
        (self.files.iter())
            .filter(|f| !self.extras.contains(f))
            .copied()
            .collect()
    }

    /// The lowercase game paths the scene reads for its drawing only: followed through `#base`,
    /// never edited, and never a scene problem.
    pub fn read_only_files(&self) -> &'static [&'static str] {
        self.read_only
    }

    /// The panel every other part of the scene sits in, if the scene has one.
    pub fn root(&self) -> Option<&'static str> {
        self.root
    }

    /// The scene's part rules.
    pub fn rules(&self) -> &'static [PanelRule] {
        self.rules
    }

    /// The panels the game's code shows or hides in this scene.
    pub fn code_toggled(&self) -> &'static [CodeToggled] {
        self.code_toggled
    }

    /// The code-toggled entry for `panel`, compared ASCII case-insensitively.
    pub fn toggled(&self, panel: &str) -> Option<&'static CodeToggled> {
        self.code_toggled
            .iter()
            .find(|t| t.panel.eq_ignore_ascii_case(panel))
    }

    /// The class the game's code gives `panel`, a direct child of the root that the file
    /// names without a `ControlName`; compared ASCII case-insensitively.
    pub fn code_class(&self, panel: &str) -> Option<&'static str> {
        self.code_classes
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(panel))
            .map(|&(_, class)| class)
    }

    /// Whether code toggles match a part at any depth, not only the root and its direct
    /// children.
    pub fn toggles_any_depth(&self) -> bool {
        self.toggle_any_depth
    }

    /// Pairs of a panel name and a lowercase game file: the file's root children become that
    /// panel's children, edited in that file.
    pub fn nested(&self) -> &'static [(&'static str, &'static str)] {
        self.nested
    }

    /// Whether the game hides the HUD's crosshair and every HUD part the scene's rules omit.
    pub fn hides_hud(&self) -> bool {
        self.hides_hud
    }

    /// The sample the preview fills code-driven parts with.
    pub fn sample(&self) -> SampleSet {
        self.sample
    }
}
