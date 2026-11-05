//! The made-up match a scene's preview fills its code-driven parts with.

use super::GameSituation;
use super::scheme::Rgba;

/// A side of the match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Team {
    /// RED.
    Red,
    /// BLU.
    Blue,
}

/// Which sample a scene's preview uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SampleSet {
    /// No sample data; the HUD scene draws only what its files say.
    Empty,
    /// A casual match: two teams of players, no tournament mode, no party leaders.
    Casual,
    /// A tournament match: six players a side, with team names.
    Competitive,
    /// Mann vs. Machine: six RED defenders with a stat row each and no BLU team.
    Mvm,
    /// The freeze cam: one made-up killer and no match.
    Killed,
    /// The win panel: one round won by BLU, its top players and its highest killstreak.
    Won,
    /// The class menu: a RED player on the Soldier button, with the casual sample's teammates.
    ClassPick,
    /// The spectator bar: a RED player who just died, watching a teammate.
    Spectating,
}

/// The made-up player who killed you on the freeze cam.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct SampleKiller {
    name: &'static str,
    team: Team,
    health: u16,
}

impl SampleKiller {
    /// The killer's name.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// The killer's team; the local player is on the other one.
    pub fn team(&self) -> Team {
        self.team
    }

    /// The killer's health when they killed you.
    pub fn health(&self) -> u16 {
        self.health
    }
}

/// The made-up player the Spectator tab watches.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct SampleWatched {
    name: &'static str,
    class: &'static str,
    health: u16,
    max_health: u16,
    team: Team,
    data: &'static str,
}

impl SampleWatched {
    /// The player's name.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// The player's class.
    pub fn class(&self) -> &'static str {
        self.class
    }

    /// The player's health.
    pub fn health(&self) -> u16 {
        self.health
    }

    /// The most health the player's class can have.
    pub fn max_health(&self) -> u16 {
        self.max_health
    }

    /// The player's team; the red or the blue target ID background follows it.
    pub fn team(&self) -> Team {
        self.team
    }

    /// The line `GetTargetIDDataString` gives a Medic: their ÜberCharge.
    pub fn data(&self) -> &'static str {
        self.data
    }
}

static WATCHED: SampleWatched = SampleWatched {
    name: "Dr. Pancake",
    class: "Medic",
    health: 118,
    max_health: 150,
    team: Team::Red,
    data: "ÜberCharge: 42%",
};

static KILLER: SampleKiller = SampleKiller {
    name: "Captain Crumpet",
    team: Team::Blue,
    health: 187,
};

// The freeze cam's labels, keyed in lowercase. The nemesis panel is hidden in the sample, so
// its two labels stay empty; the hint names F5, the default screenshot key.
static KILLED_VARIABLES: [(&str, &str); 5] = [
    ("killername", "Captain Crumpet"),
    ("health", "187"),
    ("text", "[F5] Save this moment!"),
    ("nemesisname", ""),
    ("nemesisadvice", ""),
];

/// How the scoreboard draws a player's ping, as one of four bar images.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PingLevel {
    /// Under 125 ms.
    Low,
    /// 125 to 199 ms.
    Medium,
    /// 200 to 274 ms.
    High,
    /// 275 ms and up.
    VeryHigh,
}

impl PingLevel {
    /// The bar for a ping in milliseconds; `None` below 1 ms, which the game draws as no bar.
    pub fn of(ms: u32) -> Option<PingLevel> {
        // Thresholds from tf_clientscoreboard.cpp:1414-1446.
        match ms {
            0 => None,
            1..125 => Some(PingLevel::Low),
            125..200 => Some(PingLevel::Medium),
            200..275 => Some(PingLevel::High),
            _ => Some(PingLevel::VeryHigh),
        }
    }
}

/// One made-up player row on the sample scoreboard.
#[derive(Debug)]
#[non_exhaustive]
pub struct SamplePlayer {
    name: &'static str,
    team: Team,
    class: &'static str,
    score: i32,
    ping: u32,
    alive: bool,
    local: bool,
    mvm: Option<MvmStats>,
}

/// One defender's Mann vs. Machine stat columns.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct MvmStats {
    tour: u32,
    damage: u32,
    tank: u32,
    healing: u32,
    support: u32,
    credits: u32,
    squad_surplus: bool,
}

impl MvmStats {
    /// Tours of duty completed.
    pub fn tour(&self) -> u32 {
        self.tour
    }

    /// Damage dealt.
    pub fn damage(&self) -> u32 {
        self.damage
    }

    /// Damage done to tanks.
    pub fn tank(&self) -> u32 {
        self.tank
    }

    /// Healing given.
    pub fn healing(&self) -> u32 {
        self.healing
    }

    /// Support points.
    pub fn support(&self) -> u32 {
        self.support
    }

    /// Credits collected.
    pub fn credits(&self) -> u32 {
        self.credits
    }

    /// Whether the squad surplus marker shows.
    pub fn squad_surplus(&self) -> bool {
        self.squad_surplus
    }
}

impl SamplePlayer {
    /// The MvM stat columns; `None` outside the MvM sample.
    pub fn mvm(&self) -> Option<MvmStats> {
        self.mvm
    }

    /// The player's name.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// The player's team.
    pub fn team(&self) -> Team {
        self.team
    }

    /// The player's class in lowercase, such as `"scout"`.
    pub fn class(&self) -> &'static str {
        self.class
    }

    /// The player's score.
    pub fn score(&self) -> i32 {
        self.score
    }

    /// The player's ping in milliseconds.
    pub fn ping(&self) -> u32 {
        self.ping
    }

    /// Whether the player is alive.
    pub fn alive(&self) -> bool {
        self.alive
    }

    /// Whether this is the player looking at the scoreboard.
    pub fn local(&self) -> bool {
        self.local
    }

    /// The ping bar image the game draws for this row, with the `_d` variant when dead.
    pub fn ping_image(&self) -> Option<&'static str> {
        // Image names from tf_clientscoreboard.cpp:101-116; static so callers can hold them.
        let level = PingLevel::of(self.ping)?;
        Some(match (level, self.alive) {
            (PingLevel::Low, true) => "../hud/scoreboard_ping_low",
            (PingLevel::Low, false) => "../hud/scoreboard_ping_low_d",
            (PingLevel::Medium, true) => "../hud/scoreboard_ping_med",
            (PingLevel::Medium, false) => "../hud/scoreboard_ping_med_d",
            (PingLevel::High, true) => "../hud/scoreboard_ping_high",
            (PingLevel::High, false) => "../hud/scoreboard_ping_high_d",
            (PingLevel::VeryHigh, true) => "../hud/scoreboard_ping_very_high",
            (PingLevel::VeryHigh, false) => "../hud/scoreboard_ping_very_high_d",
        })
    }

    /// The colour the game draws this row's text in.
    pub fn row_colour(&self) -> Rgba {
        self.team.row_colour(self.alive, self.local)
    }

    /// The class icon the scoreboard draws for this row when `viewer` looks, the `_d` variant
    /// when dead; `None` for the other team's rows, whose classes the game keeps hidden.
    pub fn class_image(&self, viewer: Team) -> Option<&'static str> {
        if self.team != viewer {
            return None;
        }
        CLASS_ICONS
            .iter()
            .find(|(class, _, _)| *class == self.class)
            .map(|&(_, alive, dead)| if self.alive { alive } else { dead })
    }

    /// The weapon this player's sample kills name: the class's stock primary.
    pub fn kill_weapon(&self) -> &'static str {
        match self.class {
            "scout" => "scattergun",
            "soldier" => "tf_projectile_rocket",
            "pyro" => "flamethrower",
            "demoman" => "tf_projectile_pipe",
            "heavy" => "minigun",
            "engineer" => "obj_sentrygun",
            "medic" => "syringegun_medic",
            "sniper" => "sniperrifle",
            _ => "knife",
        }
    }
}

// Each class's scoreboard icon, alive and dead; the Demoman's file says "demo".
static CLASS_ICONS: [(&str, &str, &str); 9] = [
    (
        "scout",
        "../hud/leaderboard_class_scout",
        "../hud/leaderboard_class_scout_d",
    ),
    (
        "soldier",
        "../hud/leaderboard_class_soldier",
        "../hud/leaderboard_class_soldier_d",
    ),
    (
        "pyro",
        "../hud/leaderboard_class_pyro",
        "../hud/leaderboard_class_pyro_d",
    ),
    (
        "demoman",
        "../hud/leaderboard_class_demo",
        "../hud/leaderboard_class_demo_d",
    ),
    (
        "heavy",
        "../hud/leaderboard_class_heavy",
        "../hud/leaderboard_class_heavy_d",
    ),
    (
        "engineer",
        "../hud/leaderboard_class_engineer",
        "../hud/leaderboard_class_engineer_d",
    ),
    (
        "medic",
        "../hud/leaderboard_class_medic",
        "../hud/leaderboard_class_medic_d",
    ),
    (
        "sniper",
        "../hud/leaderboard_class_sniper",
        "../hud/leaderboard_class_sniper_d",
    ),
    (
        "spy",
        "../hud/leaderboard_class_spy",
        "../hud/leaderboard_class_spy_d",
    ),
];

impl Team {
    /// A row's text colour: the team colour while alive, a dimmed one when dead, brighter for
    /// the local player (shareddefs.h:565-566, tf_clientscoreboard.cpp:1542-1566).
    pub fn row_colour(self, alive: bool, local: bool) -> Rgba {
        match (self, alive, local) {
            (Team::Red, true, _) => Rgba::new(255, 64, 64, 255),
            (Team::Blue, true, _) => Rgba::new(153, 204, 255, 255),
            (Team::Red, false, false) => Rgba::new(135, 83, 83, 255),
            (Team::Red, false, true) => Rgba::new(182, 75, 75, 255),
            (Team::Blue, false, false) => Rgba::new(81, 97, 129, 255),
            (Team::Blue, false, true) => Rgba::new(123, 153, 187, 255),
        }
    }
}

const fn player(
    name: &'static str,
    team: Team,
    class: &'static str,
    score: i32,
    ping: u32,
    alive: bool,
) -> SamplePlayer {
    SamplePlayer {
        name,
        team,
        class,
        score,
        ping,
        alive,
        local: false,
        mvm: None,
    }
}

const fn defender(
    name: &'static str,
    class: &'static str,
    score: i32,
    alive: bool,
    stats: [u32; 6],
    squad_surplus: bool,
) -> SamplePlayer {
    let [tour, damage, tank, healing, support, credits] = stats;
    SamplePlayer {
        mvm: Some(MvmStats {
            tour,
            damage,
            tank,
            healing,
            support,
            credits,
            squad_surplus,
        }),
        ..player(name, Team::Red, class, score, 40, alive)
    }
}

static COMPETITIVE_RED: [SamplePlayer; 6] = [
    SamplePlayer {
        local: true,
        ..player("You", Team::Red, "soldier", 31, 35, true)
    },
    player("Rocket Jumper", Team::Red, "soldier", 27, 48, true),
    player("Quick Feet", Team::Red, "scout", 22, 41, true),
    player("Sticky Situation", Team::Red, "demoman", 19, 62, true),
    player("Second Scout", Team::Red, "scout", 12, 58, false),
    player("Uber Ready", Team::Red, "medic", 8, 54, true),
];

static COMPETITIVE_BLUE: [SamplePlayer; 6] = [
    player("Blue Rocket", Team::Blue, "soldier", 29, 45, true),
    player("Market Garden", Team::Blue, "soldier", 24, 58, true),
    player("Bonk", Team::Blue, "scout", 21, 90, true),
    player("Demoknight", Team::Blue, "demoman", 16, 77, false),
    player("Scatter Shot", Team::Blue, "scout", 13, 66, true),
    player("Kritz", Team::Blue, "medic", 6, 52, true),
];

// The defenders, highest score first; stats are tour, damage, tank, healing, support, credits.
static MVM_RED: [SamplePlayer; 6] = [
    SamplePlayer {
        local: true,
        ..defender(
            "You",
            "soldier",
            4210,
            true,
            [12, 3890, 1200, 0, 640, 2150],
            false,
        )
    },
    defender(
        "Rocket Jumper",
        "soldier",
        3650,
        true,
        [9, 3320, 980, 0, 410, 1900],
        true,
    ),
    defender(
        "Fire Starter",
        "pyro",
        2980,
        false,
        [7, 2410, 300, 0, 1210, 1700],
        false,
    ),
    defender(
        "Big Sandvich",
        "heavy",
        2410,
        true,
        [15, 2290, 2650, 0, 220, 1500],
        false,
    ),
    defender(
        "Sentry Nest",
        "engineer",
        1760,
        true,
        [4, 1330, 760, 0, 1980, 1100],
        false,
    ),
    defender(
        "Uber Ready",
        "medic",
        1240,
        true,
        [10, 180, 0, 6420, 3100, 900],
        false,
    ),
];

// Each team is listed highest score first, as the game sorts its rows.
static CASUAL_RED: [SamplePlayer; 12] = [
    SamplePlayer {
        local: true,
        ..player("You", Team::Red, "soldier", 24, 35, true)
    },
    player("Rocket Jumper", Team::Red, "soldier", 21, 48, true),
    player("Sticky Situation", Team::Red, "demoman", 18, 62, true),
    player("Fire Starter", Team::Red, "pyro", 15, 80, true),
    player("Big Sandvich", Team::Red, "heavy", 13, 95, true),
    player("Uber Ready", Team::Red, "medic", 11, 54, true),
    player("Sentry Nest", Team::Red, "engineer", 9, 130, true),
    player("Long Range", Team::Red, "sniper", 7, 150, false),
    player("Not A Spy", Team::Red, "spy", 6, 210, true),
    player("Pocket Medic", Team::Red, "medic", 4, 40, true),
    player("Second Scout", Team::Red, "scout", 2, 290, false),
    player("Just Joined", Team::Red, "soldier", 0, 70, true),
];

static CASUAL_BLUE: [SamplePlayer; 12] = [
    player("Blue Rocket", Team::Blue, "soldier", 22, 45, true),
    player("Market Garden", Team::Blue, "soldier", 19, 58, true),
    player("Demoknight", Team::Blue, "demoman", 17, 77, true),
    player("Backburner", Team::Blue, "pyro", 14, 120, false),
    player("Minigun Mike", Team::Blue, "heavy", 12, 66, true),
    player("Kritz", Team::Blue, "medic", 10, 52, true),
    player("Wrangler", Team::Blue, "engineer", 8, 140, true),
    player("Headshot", Team::Blue, "sniper", 7, 180, true),
    player("Dead Ringer", Team::Blue, "spy", 5, 230, false),
    player("Bonk", Team::Blue, "scout", 4, 90, true),
    player("Gunboats", Team::Blue, "soldier", 3, 300, true),
    player("Late Arrival", Team::Blue, "engineer", 0, 110, true),
];

// Dialog variables the scoreboard's labels name with `%var%`, keyed in lowercase.
static CASUAL_VARIABLES: [(&str, &str); 32] = [
    ("blueteamscore", "2"),
    ("redteamscore", "1"),
    ("blueteamplayercount", "12 players"),
    ("redteamplayercount", "12 players"),
    ("blueteamname", "BLU"),
    ("redteamname", "RED"),
    ("server", "TF2Tools Preview"),
    ("mapname", "2Fort"),
    ("servertimeleft", "25:00"),
    ("servertime", "25:00"),
    ("playername", "You"),
    ("playerscore", "24"),
    ("kills", "12"),
    ("deaths", "4"),
    ("score", "0"),
    ("assists", "0"),
    ("destruction", "0"),
    ("captures", "0"),
    ("defenses", "0"),
    ("dominations", "0"),
    ("revenge", "0"),
    ("healing", "0"),
    ("invulns", "0"),
    ("teleports", "0"),
    ("headshots", "0"),
    ("backstabs", "0"),
    ("bonus", "0"),
    ("support", "0"),
    ("damage", "0"),
    ("spectators", ""),
    ("waitingtoplay", ""),
    ("gametype", ""),
];

// The lines the win panel's code fills through dialog variables for a full round BLU won by
// capture on a community server; a full round sets the winning line, not the advancing one.
static WON_VARIABLES: [(&str, &str); 9] = [
    ("winningteamlabel", "BLU TEAM WINS!"),
    ("advancingteamlabel", ""),
    ("winreasonlabel", "BLU captured all control points"),
    ("detailslabel", "Winning capture: Blue Rocket, Demoknight"),
    ("topplayerslabel", "BLU Team MVPs:"),
    ("blueteamname", "BLU"),
    ("redteamname", "RED"),
    ("blueteamscore", "2"),
    ("redteamscore", "1"),
];

/// Text and colour the game's code sets straight on a label, over what the file says.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct CodeLabel {
    text: &'static str,
    colour: Rgba,
}

impl CodeLabel {
    /// The text the code sets.
    pub fn text(&self) -> &'static str {
        self.text
    }

    /// The colour the code sets, the player's team colour.
    pub fn colour(&self) -> Rgba {
        self.colour
    }
}

// The top three are the blue sample's best, the killstreak leader is the red sample's
// Rocket Jumper, so one row shows the other team's colour. Names are keyed by part name.
static WON_LABELS: [(&str, &str, Team); 12] = [
    ("Player1Name", "Blue Rocket", Team::Blue),
    ("Player1Class", "Soldier", Team::Blue),
    ("Player1Score", "22", Team::Blue),
    ("Player2Name", "Market Garden", Team::Blue),
    ("Player2Class", "Soldier", Team::Blue),
    ("Player2Score", "19", Team::Blue),
    ("Player3Name", "Demoknight", Team::Blue),
    ("Player3Class", "Demoman", Team::Blue),
    ("Player3Score", "17", Team::Blue),
    ("KillStreakPlayer1Name", "Rocket Jumper", Team::Red),
    ("KillStreakPlayer1Class", "Soldier", Team::Red),
    ("KillStreakPlayer1Score", "9", Team::Red),
];

// What the competitive sample changes from the casual variables: names, scores and counts.
static COMPETITIVE_VARIABLES: [(&str, &str); 8] = [
    ("blueteamscore", "3"),
    ("redteamscore", "2"),
    ("blueteamplayercount", "6 players"),
    ("redteamplayercount", "6 players"),
    ("blueteamname", "Pancake Club"),
    ("redteamname", "Waffle Squad"),
    ("mapname", "Process"),
    ("playerscore", "31"),
];

// What the MvM sample adds: the mission name, its difficulty and no respec offer.
static MVM_VARIABLES: [(&str, &str); 3] = [
    ("popfile", "Operation Pancake Panic"),
    ("difficultyvalue", "Intermediate"),
    ("respecstatus", ""),
];

const MVM_CREDIT_PANELS: [&str; 4] = [
    "PreviousWaveCreditInfoPanel",
    "TotalGameCreditInfoPanel",
    "PreviousWaveCreditSpendPanel",
    "TotalGameCreditSpendPanel",
];

// The teammates per class of the casual sample's RED team, as the class menu's count labels.
static CLASS_VARIABLES: [(&str, &str); 9] = [
    ("numscout", "1"),
    ("numsoldier", "3"),
    ("numpyro", "1"),
    ("numdemoman", "1"),
    ("numheavy", "1"),
    ("numengineer", "1"),
    ("nummedic", "2"),
    ("numsniper", "1"),
    ("numspy", "1"),
];

// What the spectator view fills in: the tip and the default key names the game puts in its hints.
static SPECTATOR_VARIABLES: [(&str, &str); 8] = [
    ("tip", "Tip: Stick with the Medic to find the next fight."),
    ("changeclass", "N"),
    ("jump", "SPACE"),
    ("attack", "MOUSE1"),
    ("attack2", "MOUSE2"),
    ("targetname", "Dr. Pancake"),
    ("targetdata", "ÜberCharge: 42%"),
    ("health", "118"),
];

// The lines the spectator code writes over a label's `labelText`; a `#` name is a game token.
static SPECTATOR_TEXT: [(&str, &str); 6] = [
    ("ReinforcementsLabel", "Respawn in: 7 seconds"),
    ("MapLabel", "Map: 2Fort"),
    ("ClassOrTeamLabel", "#TF_Spectator_ChangeClass"),
    ("SwitchCamModeKeyLabel", "#TF_Spectator_SwitchCamModeKey"),
    ("CycleTargetFwdKeyLabel", "#TF_Spectator_CycleTargetFwdKey"),
    ("CycleTargetRevKeyLabel", "#TF_Spectator_CycleTargetRevKey"),
];

// The ten class buttons of the class menu, by the names the game's code gives them.
const CLASS_BUTTONS: [&str; 10] = [
    "scout",
    "soldier",
    "pyro",
    "demoman",
    "heavyweapons",
    "medic",
    "sniper",
    "engineer",
    "spy",
    "random",
];

// The eleven teammate pictures in class order: the casual RED team less the local Soldier.
const TEAMMATE_PICTURES: [&str; 11] = [
    "class_sel_sm_scout_red",
    "class_sel_sm_soldier_red",
    "class_sel_sm_soldier_red",
    "class_sel_sm_pyro_red",
    "class_sel_sm_demo_red",
    "class_sel_sm_heavy_red",
    "class_sel_sm_engineer_red",
    "class_sel_sm_medic_red",
    "class_sel_sm_medic_red",
    "class_sel_sm_sniper_red",
    "class_sel_sm_spy_red",
];

impl SampleSet {
    /// A team's rows, highest score first; empty for [`SampleSet::Empty`].
    pub fn players(self, team: Team) -> &'static [SamplePlayer] {
        match (self, team) {
            (
                SampleSet::Empty
                | SampleSet::Killed
                | SampleSet::Won
                | SampleSet::ClassPick
                | SampleSet::Spectating,
                _,
            ) => &[],
            (SampleSet::Casual, Team::Red) => &CASUAL_RED,
            (SampleSet::Casual, Team::Blue) => &CASUAL_BLUE,
            (SampleSet::Competitive, Team::Red) => &COMPETITIVE_RED,
            (SampleSet::Competitive, Team::Blue) => &COMPETITIVE_BLUE,
            (SampleSet::Mvm, Team::Red) => &MVM_RED,
            (SampleSet::Mvm, Team::Blue) => &[],
        }
    }

    /// A team's round score, or `None` when the sample has no match.
    pub fn team_score(self, team: Team) -> Option<i32> {
        match (self, team) {
            (
                SampleSet::Empty
                | SampleSet::Killed
                | SampleSet::Won
                | SampleSet::ClassPick
                | SampleSet::Spectating,
                _,
            ) => None,
            (SampleSet::Casual, Team::Red) => Some(1),
            (SampleSet::Casual, Team::Blue) => Some(2),
            (SampleSet::Competitive, Team::Red) => Some(2),
            (SampleSet::Competitive, Team::Blue) => Some(3),
            (SampleSet::Mvm, _) => None,
        }
    }

    /// The made-up player who killed you; `None` outside the freeze cam sample.
    pub fn killer(self) -> Option<SampleKiller> {
        (self == SampleSet::Killed).then_some(KILLER)
    }

    /// The made-up player you watch; `None` outside the spectator sample.
    pub fn watched(self) -> Option<SampleWatched> {
        (self == SampleSet::Spectating).then_some(WATCHED)
    }

    /// The team that won the round the sample shows; `None` outside the win panel sample.
    pub fn winner(self) -> Option<Team> {
        (self == SampleSet::Won).then_some(Team::Blue)
    }

    /// The text and colour the game's code sets on the part named `part`, matched ASCII
    /// case-insensitively; only the win panel sample has any.
    pub fn code_label(self, part: &str) -> Option<CodeLabel> {
        if self != SampleSet::Won {
            return None;
        }
        let (_, text, team) = WON_LABELS
            .iter()
            .find(|(name, _, _)| name.eq_ignore_ascii_case(part))?;
        Some(CodeLabel {
            text,
            colour: team.row_colour(true, false),
        })
    }

    /// The scheme border the game's code sets on the part named `part`, whatever the file's
    /// `border` key says; only the win panel sample has any.
    pub fn code_border(self, part: &str) -> Option<&'static str> {
        if self != SampleSet::Won {
            return None;
        }
        const BLUE: &str = "TFFatLineBorderBlueBG";
        match part.to_ascii_lowercase().as_str() {
            "winpanelbgborder" | "bluescorebg" => Some(BLUE),
            "redscorebg" => Some("TFFatLineBorderRedBG"),
            _ => None,
        }
    }

    /// Whether `name` is one of the class menu's ten buttons, compared ASCII
    /// case-insensitively; only the class menu sample has any.
    pub fn is_class_button(self, name: &str) -> bool {
        self == SampleSet::ClassPick && CLASS_BUTTONS.iter().any(|b| b.eq_ignore_ascii_case(name))
    }

    /// The picture the game's code sets on `part` (child of `parent`), compared ASCII
    /// case-insensitively: the selected button's `SubImage`, the teammate row and the local
    /// player's picture. Only the class menu sample has any.
    pub fn code_image(self, part: &str, parent: Option<&str>) -> Option<&'static str> {
        if self != SampleSet::ClassPick {
            return None;
        }
        let part = part.to_ascii_lowercase();
        if part == "subimage" {
            return parent
                .is_some_and(|p| p.eq_ignore_ascii_case("soldier"))
                .then_some("class_sel_sm_soldier_red");
        }
        if part == "localplayerimage" {
            return Some("class_sel_sm_soldier_red");
        }
        let index: usize = part.strip_prefix("countimage")?.parse().ok()?;
        TEAMMATE_PICTURES.get(index).copied()
    }

    /// The flat colour the game's code fills `part` with, standing in for the 3D room behind
    /// the class menu; only the class menu sample has any.
    pub fn code_fill(self, part: &str) -> Option<Rgba> {
        match self {
            SampleSet::ClassPick if part.eq_ignore_ascii_case("MenuBG") => {
                Some(Rgba::new(26, 24, 22, 255))
            }
            SampleSet::Spectating if self.code_fill_wins(part) => Some(Rgba::new(52, 48, 45, 255)),
            _ => None,
        }
    }

    /// Whether the game paints `part` after its file loads, so the code fill beats the file's
    /// colour keys; false where `code_fill` is only a fallback.
    pub fn code_fill_wins(self, part: &str) -> bool {
        self == SampleSet::Spectating
            && ["topbar", "bottombarblank"]
                .iter()
                .any(|n| n.eq_ignore_ascii_case(part))
    }

    /// The text the game's code writes over `part`'s `labelText`; a `#` name is a game token the
    /// canvas resolves. Only the spectator sample and the HUD's chat input line have any.
    pub fn code_text(self, part: &str) -> Option<&'static str> {
        // CBaseHudChat::StartMessageMode's English prompt for a chat to everyone.
        if self == SampleSet::Empty && part.eq_ignore_ascii_case("ChatInputLine") {
            return Some("Say :");
        }
        if self != SampleSet::Spectating {
            return None;
        }
        SPECTATOR_TEXT
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(part))
            .map(|&(_, text)| text)
    }

    /// The note for a part the game fills with content the preview cannot draw; only the
    /// class menu and MvM samples have any.
    pub fn game_filled(self, part: &str) -> Option<&'static str> {
        let is = |name: &str| part.eq_ignore_ascii_case(name);
        match self {
            SampleSet::ClassPick if is("ClassTipsPanel") => Some("Class tips draw in game"),
            SampleSet::Mvm if is("WaveStatusPanel") => Some("The wave bar draws in game"),
            SampleSet::Mvm if MVM_CREDIT_PANELS.iter().any(|name| is(name)) => {
                Some("The credit tables draw in game")
            }
            _ => None,
        }
    }

    /// The team and row index of the local player, whose row the scoreboard selects.
    pub fn selected(self) -> Option<(Team, usize)> {
        [Team::Red, Team::Blue].into_iter().find_map(|team| {
            let row = self.players(team).iter().position(SamplePlayer::local)?;
            Some((team, row))
        })
    }

    /// `text` with this sample's variables, then the game's live values, filled in; any other
    /// `%token%` is left empty, as the preview draws it.
    pub fn fill(self, text: &str) -> String {
        self.fill_in(text, &GameSituation::default())
    }

    /// `text` with each `%name%` filled, the health, ammo, reserve and charge numbers taken
    /// from situation `s`.
    pub fn fill_in(self, text: &str, s: &GameSituation) -> String {
        let percent = u32::from(s.ammo_percent());
        let numbers = [
            ("health", s.health().to_string()),
            ("ammo", (40 * percent / 100).to_string()),
            ("ammoinreserve", (150 * percent / 100).to_string()),
            ("charge", format!("{}%", s.charge())),
            // A meter's count (heads, kills) is a sample value; the situation has no count.
            ("progresscount", "3".to_owned()),
        ];
        let mut out = String::new();
        let mut rest = text;
        while let Some(start) = rest.find('%') {
            let Some(len) = rest[start + 1..].find('%') else {
                break;
            };
            out.push_str(&rest[..start]);
            let token = &rest[start + 1..start + 1 + len];
            let number = (numbers.iter())
                .find(|(name, _)| name.eq_ignore_ascii_case(token))
                .map(|(_, n)| n.as_str());
            out.push_str(self.variable(token).or(number).unwrap_or(
                match token.to_ascii_lowercase().as_str() {
                    "timer" => "5:00",
                    _ => "",
                },
            ));
            rest = &rest[start + len + 2..];
        }
        out.push_str(rest);
        out
    }

    /// A dialog variable's text, matching the name without regard to ASCII case.
    pub fn variable(self, name: &str) -> Option<&'static str> {
        match self {
            SampleSet::Empty => None,
            SampleSet::Casual => lookup(&CASUAL_VARIABLES, name),
            SampleSet::Competitive => {
                lookup(&COMPETITIVE_VARIABLES, name).or_else(|| lookup(&CASUAL_VARIABLES, name))
            }
            SampleSet::Mvm => {
                lookup(&MVM_VARIABLES, name).or_else(|| lookup(&CASUAL_VARIABLES, name))
            }
            SampleSet::Killed => lookup(&KILLED_VARIABLES, name),
            SampleSet::Won => lookup(&WON_VARIABLES, name),
            SampleSet::ClassPick => lookup(&CLASS_VARIABLES, name),
            SampleSet::Spectating => lookup(&SPECTATOR_VARIABLES, name),
        }
    }
}

fn lookup(table: &[(&'static str, &'static str)], name: &str) -> Option<&'static str> {
    (table.iter())
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, text)| *text)
}
