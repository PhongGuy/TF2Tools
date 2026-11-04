//! A synthetic stand-in for the default HUD's Stage 1 and 2 files, shared by the HUD Maker tests.

use tf2tools_core::HudFiles;

// The killfeed block as a literal, so HUDLAYOUT holds it and tests can swap it by its text.
macro_rules! killfeed {
    () => {
        r#"  HudDeathNotice
  {
    fieldName HudDeathNotice
    visible 1
    enabled 1
    xpos r640 [$WIN32]
    ypos 18 [$WIN32]
    xpos r672 [$X360]
    ypos 35 [$X360]
    wide 628
    tall 468
    MaxDeathNotices 4
    IconScale 0.35
    LineHeight 16
    LineSpacing 4
    CornerRadius 3
    RightJustify 1
    TextFont Default
    TeamBlue HUDBlueTeamSolid
    TeamRed HUDRedTeamSolid
    IconColor HudWhite
    LocalPlayerColor HUDBlack
    BaseBackgroundColor "46 43 42 220" [$WIN32]
    LocalBackgroundColor "245 229 196 200" [$WIN32]
  }
"#
    };
}

/// The stock HudDeathNotice block inside HUDLAYOUT.
pub const KILLFEED: &str = killfeed!();

pub const HUDLAYOUT: &str = concat!(
    r#""Resource/HudLayout.res"
{
  HudPlayerStatus { xpos 0 ypos 0 wide f0 tall 480 }
  HudWeaponAmmo { visible 1 xpos r95 [$WIN32] xpos r131 [$X360] ypos r55 wide 94 tall 45 }
  HudMedicCharge { xpos c-38 ypos c130 wide 100 tall 40 }
  CHealthAccountPanel { xpos c-60 ypos c-8 wide 120 tall 70 }
  CDamageAccountPanel { xpos 0 ypos 0 wide f0 tall 480 }
  CMainTargetID { visible 0 xpos c-126 ypos 355 wide 252 tall 35 if_vr { ypos 290 } }
  HudItemEffectMeter { xpos r162 ypos r52 wide 100 tall 50 }
  HudMatchStatus { xpos 0 ypos 0 wide f0 tall f0 }
  HudKothTimeStatus { xpos c-100 ypos 0 wide 200 tall 160 blue_active_xpos 16 red_active_xpos 106 }
  FreezePanel { visible 1 xpos 0 ypos 0 wide f0 tall 480 }
  WinPanel { visible 1 xpos c-150 ypos 215 wide 300 tall 300 }
"#,
    killfeed!(),
    "}\n"
);

/// A synthetic `clientscheme.res`: the stock team colours and a few stock-shaped fonts, the
/// `Default` font with per-height variants as the game writes it.
pub const SCHEME: &str = r#""Scheme"
{
  Colors
  {
    TanDark "117 107 94 255"
    TanLight "235 226 202 255"
    HUDBlueTeamSolid "104 124 155 255"
    HUDRedTeamSolid "180 92 77 255"
    HudWhite "255 255 255 255"
    HUDBlack "0 0 0 255"
  }
  Fonts
  {
    Default
    {
      "1" { name Verdana tall 12 weight 900 yres "480 599" }
      "2" { name Verdana tall 13 weight 900 yres "600 767" }
      "3" { name Verdana tall 14 weight 900 yres "768 1023" antialias 1 }
      "4" { name Verdana tall 20 weight 900 yres "1024 1199" antialias 1 }
      "5" { name Verdana tall 9 weight 900 antialias 1 }
    }
    HudFontSmall { "1" { name "TF2 Secondary" tall 11 weight 400 antialias 1 } }
    HudFontMedium { "1" { name "TF2 Build" tall 16 weight 500 additive 0 antialias 1 } }
    HudFontGiant { "1" { name "TF2 Build" tall 33 weight 500 additive 0 antialias 1 } }
    HudClassHealth { "1" { name "TF2 Build" tall 16 weight 500 additive 0 antialias 1 } }
  }
}
"#;

/// `fgcolor` is on line 32; tests rely on that.
pub const HEALTH: &str = r#""Resource/UI/HudPlayerHealth.res"
{
  HudPlayerHealth
  {
    ControlName EditablePanel
    xpos 0 [$WIN32]
    xpos 32 [$X360]
    ypos r120
    xpos_minmode -5
    zpos 2
    wide 250
    tall 120
    visible 1
  }
  PlayerStatusHealthImage
  {
    ControlName ImagePanel
    xpos 73
    ypos 33
    zpos 4
    wide 51
    tall 51
  }
  PlayerStatusHealthValue
  {
    ControlName CExLabel
    xpos 76
    ypos 52
    zpos 5
    wide 44
    tall 18
    fgcolor TanDark
    font HudClassHealth
    labelText %Health%
  }
  HealthBonusSettings
  {
    note "not a control"
  }
}
"#;

/// A synthetic `scoreboard.res`, no Valve bytes: the root, code-toggled children, both player
/// lists, `if_mvm` overrides and a nested control. Tests edit its root lines by exact text.
pub const SCOREBOARD: &str = r#""Resource/UI/Scoreboard.res"
{
  scores
  {
    ControlName CTFClientScoreBoardDialog
    xpos cs-0.5
    ypos 31
    wide 640
    tall 448
    visible 1
    medal_column_width 18
    avatar_width 28
    spacer 5
    name_width 118
    nemesis_width 15
    class_width 15
    score_width 25
    ping_width 15
    killstreak_width 18
    killstreak_image_width 15
  }
  BlueTeamImage { ControlName ImagePanel xpos 7 ypos 9 wide 60 tall 60 visible 0 }
  BlueLeaderAvatar { ControlName CAvatarImagePanel xpos 7 ypos 9 wide 60 tall 60 }
  BlueTeamLabel { ControlName CExLabel xpos 75 ypos 20 wide 120 tall 20 fgcolor TanLight labelText %blueteamname% }
  BlueTeamScore { ControlName CExLabel xpos 200 ypos 20 wide 80 tall 40 fgcolor TanLight labelText %blueteamscore% }
  BluePlayerList { ControlName SectionedListPanel xpos 5 ypos 67 wide 310 tall 280 linespacing 18 linegap 5 fgcolor blue }
  RedPlayerList { ControlName SectionedListPanel xpos 325 ypos 67 wide 310 tall 280 linespacing 18 linegap 5 fgcolor red if_mvm { visible 0 } }
  ServerLabel { ControlName CExLabel xpos 8 ypos 350 wide 200 tall 12 labelText %server% if_mvm { ypos 0 } }
  ClassImage { ControlName ImagePanel xpos 10 ypos 350 wide 60 tall 60 }
  classmodelpanel { ControlName CTFPlayerModelPanel xpos 10 ypos 340 wide 80 tall 100 }
  LocalPlayerStatsPanel
  {
    ControlName EditablePanel
    xpos 300 ypos 350 wide 340 tall 100
    Kills { ControlName CExLabel xpos 0 ypos 0 wide 40 tall 10 visible 0 labelText %kills% }
  }
  LocalPlayerDuelStatsPanel { ControlName EditablePanel xpos 300 ypos 350 wide 340 tall 100 }
  MvMScoreboard { ControlName EditablePanel xpos 0 ypos 0 wide 640 tall 448 visible 0 if_mvm { visible 1 } }
}
"#;

/// The ammo in the clip, in the stock font.
pub const AMMO: &str = r#""Resource/UI/HudAmmoWeapons.res" { AmmoInClip { ControlName CExLabel xpos 10 ypos 5 wide 55 tall 40 fgcolor TanLight font HudFontGiant labelText %Ammo% } }"#;

/// A stock-shaped `basechat.res`: the chat panel and its input line, no Valve bytes.
pub const BASECHAT: &str = r#""Resource/UI/BaseChat.res"
{
  HudChat
  {
    ControlName EditablePanel
    fieldName HudChat
    visible 1
    enabled 1
    xpos 10
    ypos 275
    wide 320
    tall 120
    PaintBackgroundType 2
  }
  ChatInputLine { ControlName EditablePanel xpos 10 ypos 395 wide 320 tall 2 }
}
"#;

/// A stock-shaped `mainmenuoverride.res` holding the friends list, no Valve bytes.
pub const MAINMENU: &str = r#""Resource/UI/MainMenuOverride.res"
{
  MainMenuOverride { ControlName EditablePanel xpos 0 ypos 0 wide f0 tall f0 }
  FriendsContainer
  {
    ControlName EditablePanel
    xpos r280
    ypos 50
    zpos 2
    wide 270
    tall 310
    visible 1
  }
}
"#;

/// A stock-shaped freezepanel_basic.res, no Valve bytes: both platform blocks, the item panel
/// and the screenshot hint. Linux reads the [$WIN32] block.
pub const FREEZE_PANEL: &str = r##""Resource/UI/FreezePanel_Basic.res"
{
  FreezePanel { ControlName CTFFreezePanel fieldName FreezePanel screenshot_move_panel_to_corner 1 }
  FreezePanelBase [$X360] { ControlName EditablePanel xpos 0 ypos 0 wide 640 tall 480 visible 1
    FreezePanelBG { ControlName CTFImagePanel xpos c-38 ypos 245 wide 266 tall 48 image ../hud/color_panel_blu } }
  itempanel { ControlName CItemModelPanel xpos r200 ypos 0 zpos 10 wide 190 tall 100 visible 0 }
  FreezePanelBase [$WIN32] { ControlName EditablePanel xpos r270 ypos 93 wide 267 tall 200 visible 1
    FreezePanelBG { ControlName CTFImagePanel xpos 8 ypos 160 zpos 0 wide 256 tall 33 visible 1
      image ../hud/color_panel_blu src_corner_height 23 src_corner_width 23 draw_corner_width 5 draw_corner_height 5 }
    FreezeLabel { ControlName Label font DefaultSmall xpos 40 ypos 162 zpos 1 wide 252 tall 12 visible 1
      labelText "#FreezePanel_Killer" textAlignment North-West }
    FreezePanelHealth { ControlName CTFFreezePanelHealth xpos 8 ypos 160 zpos 1 wide 31 tall 31 visible 1 }
    FreezeLabelKiller { ControlName Label font HudFontSmall xpos 61 ypos 173 zpos 5 wide 252 tall 18 visible 1
      labelText %killername% textAlignment Left }
    AvatarImage { ControlName CAvatarImagePanel xpos 49 ypos 174 zpos 0 wide 14 tall 14 visible 1 image "" scaleImage 1 }
    NemesisSubPanel { ControlName EditablePanel xpos 0 ypos 130 zpos 4 wide 306 tall 38 visible 1
      NemesisPanelBG { ControlName ScalableImagePanel xpos 95 ypos 4 zpos 5 wide 161 tall 30 image ../hud/color_panel_brown }
      NemesisIcon { ControlName ImagePanel xpos 95 ypos -1 zpos 6 wide 36 tall 36 image ../hud/leaderboard_nemesis_freezecam }
      NemesisLabel { ControlName Label font HudFontMediumSmall xpos 134 ypos 10 zpos 6 wide 112 tall 18 labelText %nemesisname% } } }
  ScreenshotPanel { ControlName EditablePanel xpos c-83 ypos 50 zpos 2 wide 166 tall 40 visible 0
    ScreenshotPanelBG { ControlName ImagePanel xpos 0 ypos 8 wide 166 tall 38 image ../hud/freezecam_black_bg }
    ScreenshotLabel { ControlName Label font SpectatorKeyHints xpos 40 ypos 25 zpos 2 wide 133 tall 12 labelText %text% } }
}
"##;

/// A stock-shaped freezepanelkillerhealth.res: the controls of FreezePanelHealth.
pub const FREEZE_HEALTH: &str = r##""Resource/UI/FreezePanelKillerHealth.res"
{
  PlayerStatusHealthImage { ControlName ImagePanel xpos 7 ypos 7 zpos 4 wide 18 tall 18 visible 1 scaleImage 1 }
  PlayerStatusHealthImageBG { ControlName ImagePanel xpos 5 ypos 5 zpos 3 wide 22 tall 22 visible 1 image ../hud/health_bg }
  BuildingStatusHealthImageBG { ControlName ImagePanel xpos 1 ypos 2 zpos 3 wide 28 tall 28 visible 1 image ../hud/health_equip_bg }
  PlayerStatusHealthBonusImage { ControlName ImagePanel xpos 5 ypos 5 zpos 3 wide 22 tall 22 visible 0 image ../hud/health_over_bg }
  PlayerStatusHealthValue { ControlName CExLabel xpos 6 ypos 11 zpos 5 wide 20 tall 10 visible 1
    labelText %Health% textAlignment center font SpectatorVerySmall fgcolor TanDark }
}
"##;

/// A synthetic `mvmscoreboard.res`, no Valve bytes: the stock panel names and boxes.
pub const MVM_SCOREBOARD: &str = r#""Resource/UI/MvMScoreboard.res"
{
  WaveStatusPanel { ControlName CWaveStatusPanel xpos 0 ypos 8 wide 600 tall 67 visible 1 }
  PopFileLabel { ControlName CExLabel xpos 290 ypos 375 wide 290 tall 20 labelText %popfile% }
  DifficultyContainer { ControlName EditablePanel xpos 425 ypos 30 wide 150 tall 20 visible 1
    DifficultyValue { ControlName CExLabel xpos 0 ypos 9 wide 150 tall 10 labelText %difficultyvalue% } }
  PlayerListBackground { ControlName ScalableImagePanel xpos 25 ypos 75 wide 550 tall 150 visible 1 }
  MvMPlayerList { ControlName SectionedListPanel xpos 35 ypos 79 wide 530 tall 150 linespacing 22 visible 1 }
  CreditStatsContainer { ControlName EditablePanel xpos 100 ypos 230 wide 400 tall 205 visible 1
    RespecStatusLabel { ControlName CExLabel xpos 115 ypos 8 wide 275 tall 20 labelText %respecstatus% }
    PreviousWaveCreditInfoPanel { ControlName CCreditDisplayPanel xpos 8 ypos 30 wide 200 tall 60 visible 1 }
    TotalGameCreditInfoPanel { ControlName CCreditDisplayPanel xpos 208 ypos 30 wide 200 tall 60 visible 1 }
    PreviousWaveCreditSpendPanel { ControlName CCreditSpendPanel xpos 8 ypos 75 wide 200 tall 60 visible 1 }
    TotalGameCreditSpendPanel { ControlName CCreditSpendPanel xpos 208 ypos 75 wide 200 tall 60 visible 1 } }
}
"#;

/// A stock-shaped Eyelander meter: its own root box, a label, the head count and the bar.
pub const METER_DEMOMAN: &str = r##""Resource/UI/HudItemEffectMeter_Demoman.res"
{
  HudItemEffectMeter { xpos r162 ypos r92 wide 100 tall 50 MeterFG White MeterBG Gray }
  ItemEffectMeterLabel { ControlName CExLabel xpos 25 ypos 27 wide 41 tall 15 labelText "#TF_Demoman_Heads" textAlignment center }
  ItemEffectMeterCount { ControlName CExLabel xpos 73 ypos 7 wide 30 tall 18 labelText %progresscount% textAlignment east }
  ItemEffectMeter { ControlName ContinuousProgressBar xpos 25 ypos 23 wide 40 tall 4 }
}
"##;

/// A stock-shaped hudobjectivekothtimepanel.res: the panel's own block, the two team timers with
/// their own time labels and the active-timer background. Boxes follow the game's file.
pub const KOTH_PANEL: &str = r#""Resource/UI/HudObjectiveKothTimePanel.res"
{
  HudKothTimeStatus { if_match { zpos 5 } }
  BlueTimer { ControlName CTFHudTimeStatus xpos 0 xpos_minmode 20 ypos 0 zpos 2 wide 100 tall 150 visible 1
    if_match { proportionaltoparent 1 xpos cs-0.5 ypos 0 }
    TimePanelValue { ControlName CExLabel xpos 23 ypos 11 zpos 3 wide 45 tall 31 labelText 0:00 textAlignment center
      if_match { proportionaltoparent 1 xpos 15 ypos 12 wide 35 tall 10 } } }
  RedTimer { ControlName CTFHudTimeStatus xpos 90 xpos_minmode 70 ypos 0 zpos 2 wide 100 tall 150 visible 1
    if_match { proportionaltoparent 1 xpos cs-0.5 ypos 0 }
    TimePanelValue { ControlName CExLabel xpos 23 ypos 11 zpos 3 wide 45 tall 31 labelText 0:00 textAlignment center
      if_match { proportionaltoparent 1 xpos rs1-15 ypos 12 wide 35 tall 10 } } }
  ActiveTimerBG { ControlName ImagePanel xpos 0 ypos 9 zpos 1 wide 78 tall 33 visible 0 image ../hud/objectives_timepanel_active_bg scaleImage 1 }
}
"#;

/// Every stock file as (lowercase game path, text).
pub fn stock() -> Vec<(String, String)> {
    let class = class_select();
    [
        ("scripts/hudlayout.res", HUDLAYOUT),
        ("resource/ui/hudplayerhealth.res", HEALTH),
        ("resource/ui/hudplayerclass.res", r#""Resource/UI/HudPlayerClass.res" { HudPlayerClass { ControlName EditablePanel xpos 0 ypos 0 wide f0 tall 480 }
  classmodelholder { ControlName EditablePanel xpos 0 ypos r200 wide 100 tall 200
    classmodelpanel { ControlName CTFPlayerModelPanel xpos 10 ypos r170 wide 80 tall 150 proportionaltoparent 1 model { modelname models/player/heavy.mdl } } } }"#),
        ("resource/ui/hudammoweapons.res", AMMO),
        ("resource/ui/hudmediccharge.res", r#""Resource/UI/HudMedicCharge.res" { ChargeLabel { ControlName CExLabel xpos 0 ypos 0 wide 100 tall 20 } }"#),
        ("resource/ui/hudhealthaccount.res", r#""Resource/UI/HudHealthAccount.res" { CHealthAccountPanel { delta_item_x 25 delta_item_start_y 40 } }"#),
        ("resource/ui/huddamageaccount.res", r#""Resource/UI/HudDamageAccount.res" { CDamageAccountPanel { delta_item_font HudFontMedium } }"#),
        ("resource/ui/targetid.res", r#""Resource/UI/TargetID.res" { TargetNameLabel { ControlName CExLabel xpos 35 ypos 6 wide 200 tall 20 } }"#),
        ("resource/ui/huditemeffectmeter.res", r#""Resource/UI/HudItemEffectMeter.res" { HudItemEffectMeter { xpos r174 ypos r62 } ItemEffectMeterLabel { ControlName CExLabel xpos 5 ypos 5 wide 90 tall 15 } }"#),
        ("resource/ui/huditemeffectmeter_demoman.res", METER_DEMOMAN),
        ("resource/ui/hudmatchstatus.res", r#""Resource/UI/HudMatchStatus.res" { ObjectiveStatusTimePanel { ControlName EditablePanel visible 0
  xpos c-55 ypos 0 wide 110 tall 30 if_match { xpos cs-0.5 wide 130 } } RoundSignModel { ControlName CModelPanel } }"#),
        ("resource/ui/hudobjectivetimepanel.res", r#""Resource/UI/HudObjectiveTimePanel.res" { TimePanelValue { ControlName CExLabel xpos 25 ypos 2 wide 60 tall 20 labelText 0:00 if_match { xpos 35 } } }"#),
        ("resource/ui/scoreboard.res", SCOREBOARD),
        ("resource/ui/mvmscoreboard.res", MVM_SCOREBOARD),
        ("resource/ui/basechat.res", BASECHAT),
        ("resource/ui/hudobjectivekothtimepanel.res", KOTH_PANEL),
        ("resource/ui/mainmenuoverride.res", MAINMENU),
        ("resource/ui/freezepanel_basic.res", FREEZE_PANEL),
        ("resource/ui/freezepanelkillerhealth.res", FREEZE_HEALTH),
        ("resource/ui/winpanel.res", WIN_PANEL),
        ("resource/ui/classselection.res", &class),
        ("resource/ui/spectator.res", SPECTATOR),
        ("resource/clientscheme.res", SCHEME),
    ]
    .into_iter()
    .map(|(p, t)| (p.to_owned(), t.to_owned()))
    .collect()
}

/// HUDLAYOUT with its killfeed block replaced by `block` (empty removes it).
pub fn hudlayout_with_killfeed(block: &str) -> String {
    HUDLAYOUT.replacen(KILLFEED, block, 1)
}

/// The stock files with each `(path, Some(text))` replaced or added and each `(path, None)` removed.
pub fn stock_files_with(overrides: &[(&str, Option<&str>)]) -> HudFiles {
    let mut files = stock();
    for (path, text) in overrides {
        files.retain(|(p, _)| p != path);
        if let Some(text) = text {
            files.push(((*path).to_owned(), (*text).to_owned()));
        }
    }
    HudFiles::from_memory(vec![
        files
            .into_iter()
            .map(|(p, t)| (p, t.into_bytes()))
            .collect(),
    ])
}

pub fn stock_files() -> HudFiles {
    stock_files_with(&[])
}

/// The stock files with the first `from` in SCOREBOARD replaced by `to`; panics when absent.
pub fn scoreboard_with(from: &str, to: &str) -> HudFiles {
    assert!(SCOREBOARD.contains(from), "{from:?} is not in SCOREBOARD");
    let text = SCOREBOARD.replacen(from, to, 1);
    stock_files_with(&[("resource/ui/scoreboard.res", Some(&text))])
}

/// A `ControlName` block of `class` with `keys` in order, at the root block's indent.
pub fn block(name: &str, class: &str, keys: &[(&str, &str)]) -> String {
    let mut out = format!("  {name}\n  {{\n    ControlName {class}\n");
    for (k, v) in keys {
        out += &format!("    {k} {v}\n");
    }
    out + "  }\n"
}

/// A CExLabel at (`xpos`, `ypos`), 20x10 units, then `extra` keys.
pub fn label(name: &str, xpos: &str, ypos: &str, extra: &[(&str, &str)]) -> String {
    let mut keys = vec![
        ("xpos", xpos),
        ("ypos", ypos),
        ("wide", "20"),
        ("tall", "10"),
    ];
    keys.extend_from_slice(extra);
    block(name, "CExLabel", &keys)
}

/// `base` with `blocks` inserted before its last closing brace.
pub fn with_blocks(base: &str, blocks: &str) -> String {
    let at = base.rfind('}').expect("a root block");
    format!("{}{blocks}{}", &base[..at], &base[at..])
}

/// A stock-shaped pvprankpanel.res holding only the controls the preview draws, no Valve bytes.
pub const RANK_PANEL: &str = r##""Resource/UI/PvPRankPanel.res"
{
  BGPanel { ControlName EditablePanel xpos rs1 ypos 20 wide 260 tall f40 visible 1
    NameLabel { ControlName Label xpos 65 ypos 5 wide f0 tall 20 visible 1 font HudFontSmallishBold fgcolor_override TanLight textAlignment north-west }
    StatsContainer { ControlName EditablePanel xpos rs1-5 ypos 0 wide f70 tall f0
      XPBar { ControlName EditablePanel xpos cs-0.5 ypos rs1-3 wide p1 tall 30
        ProgressBarsContainer { ControlName EditablePanel xpos 0 ypos rs1-10 wide p1 tall 7
          ProgressBar { ControlName ProgressBar xpos 0 ypos cs-0.5 wide f0 tall f-2 fgcolor_override "20 20 20 180" }
          ContinuousProgressBar { ControlName ContinuousProgressBar xpos cs-0.5 ypos cs-0.5 wide f2 tall f2 fgcolor_override CreditsGreen } } } } }
}
"##;

/// The stock freeze panel under a variant name, plus a stub `freezepanel_basic.res` that only
/// includes it.
pub fn freeze_stub() -> HudFiles {
    stock_files_with(&[
        (
            "resource/ui/freezepanel_basic_variant.res",
            Some(FREEZE_PANEL),
        ),
        (
            "resource/ui/freezepanel_basic.res",
            Some("#base \"freezepanel_basic_variant.res\"\n"),
        ),
    ])
}

/// A stock-shaped winpanel.res: the scores panel, the border, the four text lines, three
/// player rows and the killstreak row. Names and boxes follow the game's file; the text is ours.
pub const WIN_PANEL: &str = r##""Resource/UI/WinPanel.res"
{
  TeamScoresPanel { ControlName EditablePanel xpos -41 ypos 13 wide 480 tall 60 visible 1
    BlueScoreBG { ControlName EditablePanel xpos 50 ypos 15 wide 135 tall 40 visible 1 border TFFatLineBorderBlueBGMoreOpaque }
    RedScoreBG { ControlName EditablePanel xpos 197 ypos 15 wide 135 tall 40 visible 1 border TFFatLineBorderRedBGMoreOpaque }
    BlueTeamLabel { ControlName CExLabel font HudFontMedium labelText %blueteamname% textAlignment west xpos 56 ypos 25 zpos 10 wide 100 tall 20 visible 1 fgcolor TanLight }
    BlueTeamScore { ControlName CExLabel font HudFontGiant labelText %blueteamscore% textAlignment east xpos 80 ypos 0 zpos 3 wide 100 tall 50 visible 1 }
    BlueLeaderAvatar { ControlName CAvatarImagePanel xpos 61 ypos 14 zpos 5 wide 40 tall 40 visible 1 image "" scaleImage 1 }
    BlueLeaderAvatarBG { ControlName EditablePanel xpos 59 ypos 12 zpos 4 wide 44 tall 44 visible 1 bgcolor_override "117 107 94 255" }
    RedTeamLabel { ControlName CExLabel font HudFontMedium labelText %redteamname% textAlignment east xpos 224 ypos 25 zpos 10 wide 100 tall 20 visible 1 fgcolor TanLight }
    RedTeamScore { ControlName CExLabel font HudFontGiant labelText %redteamscore% textAlignment west xpos 202 ypos 0 zpos 3 wide 100 tall 50 visible 1 }
    RedLeaderAvatar { ControlName CAvatarImagePanel xpos 280 ypos 14 zpos 5 wide 40 tall 40 visible 1 image "" scaleImage 1 }
    RedLeaderAvatarBG { ControlName EditablePanel xpos 278 ypos 12 zpos 4 wide 44 tall 44 visible 1 bgcolor_override "117 107 94 255" } }
  WinPanelBGBorder { ControlName EditablePanel xpos cs-0.5 ypos 70 zpos 0 wide p0.94 tall 185 visible 1 border TFFatLineBorderBlueBG proportionaltoparent 1 }
  WinningTeamLabel { ControlName CExLabel font HudFontMedium xpos 15 ypos 72 zpos 1 wide 268 tall 24 visible 1 labelText %WinningTeamLabel% textAlignment Center }
  AdvancingTeamLabel { ControlName CExLabel font HudFontMedium xpos 15 ypos 72 zpos 1 wide 268 tall 24 visible 1 labelText %AdvancingTeamLabel% textAlignment Center }
  WinReasonLabel { ControlName CExLabel font HudFontSmall xpos 15 ypos 98 zpos 1 wide 268 tall 20 visible 1 labelText %WinReasonLabel% textAlignment Center }
  DetailsLabel { ControlName CExLabel font HudFontSmall xpos 15 ypos 112 zpos 1 wide 268 tall 20 visible 1 labelText %DetailsLabel% textAlignment Center }
  TopPlayersLabel { ControlName CExLabel font HudFontSmall xpos 18 ypos 134 zpos 3 wide 200 tall 20 visible 1 labelText %TopPlayersLabel% textAlignment west }
  Player1Badge { ControlName CTFBadgePanel xpos 11 ypos 152 zpos 3 wide 25 tall 25 visible 0 }
  Player1Avatar { ControlName CAvatarImagePanel xpos 39 ypos 157 zpos 3 wide 14 tall 14 visible 1 image "" scaleImage 1 }
  Player1Name { ControlName CExLabel font HudFontSmall xpos 56 ypos 154 zpos 3 wide 125 tall 20 visible 1 labelText "" textAlignment west fgcolor TanLight }
  Player1Class { ControlName CExLabel font HudFontSmall xpos 183 ypos 154 zpos 3 wide 200 tall 20 visible 1 labelText "" textAlignment west }
  Player1Score { ControlName CExLabel font HudFontSmall xpos 243 ypos 154 zpos 3 wide 30 tall 20 visible 1 labelText "" textAlignment east }
  Player2Badge { ControlName CTFBadgePanel xpos 11 ypos 174 zpos 3 wide 25 tall 25 visible 0 }
  Player2Avatar { ControlName CAvatarImagePanel xpos 39 ypos 179 zpos 3 wide 14 tall 14 visible 1 image "" scaleImage 1 }
  Player2Name { ControlName CExLabel font HudFontSmall xpos 56 ypos 176 zpos 3 wide 125 tall 20 visible 1 labelText "" textAlignment west }
  Player2Class { ControlName CExLabel font HudFontSmall xpos 183 ypos 176 zpos 3 wide 200 tall 20 visible 1 labelText "" textAlignment west }
  Player2Score { ControlName CExLabel font HudFontSmall xpos 243 ypos 176 zpos 3 wide 30 tall 20 visible 1 labelText "" textAlignment east }
  Player3Badge { ControlName CTFBadgePanel xpos 11 ypos 196 zpos 3 wide 25 tall 25 visible 0 }
  Player3Avatar { ControlName CAvatarImagePanel xpos 39 ypos 201 zpos 3 wide 14 tall 14 visible 1 image "" scaleImage 1 }
  Player3Name { ControlName CExLabel font HudFontSmall xpos 56 ypos 198 zpos 3 wide 125 tall 20 visible 1 labelText "" textAlignment west }
  Player3Class { ControlName CExLabel font HudFontSmall xpos 183 ypos 198 zpos 3 wide 200 tall 20 visible 1 labelText "" textAlignment west }
  Player3Score { ControlName CExLabel font HudFontSmall xpos 243 ypos 198 zpos 3 wide 30 tall 20 visible 1 labelText "" textAlignment east }
  KillStreakLeaderLabel { ControlName CExLabel font HudFontSmall xpos 18 ypos 218 zpos 3 wide 200 tall 20 visible 1 labelText "#Winpanel_KillStreakLeader" textAlignment west }
  KillStreakPlayer1Badge { ControlName CTFBadgePanel xpos 11 ypos 236 zpos 3 wide 25 tall 25 visible 0 }
  KillStreakPlayer1Avatar { ControlName CAvatarImagePanel xpos 39 ypos 241 zpos 3 wide 14 tall 14 visible 1 image "" scaleImage 1 }
  KillStreakPlayer1Name { ControlName CExLabel font HudFontSmall xpos 56 ypos 238 zpos 3 wide 125 tall 20 visible 1 labelText "" textAlignment west }
  KillStreakPlayer1Class { ControlName CExLabel font HudFontSmall xpos 183 ypos 238 zpos 3 wide 200 tall 20 visible 1 labelText "" textAlignment west }
  KillStreakPlayer1Score { ControlName CExLabel font HudFontSmall xpos 243 ypos 238 zpos 3 wide 30 tall 20 visible 1 labelText "" textAlignment east }
}
"##;

/// The stock files with winpanel.res reduced to a `#base` stub of a variant file.
pub fn win_stub() -> HudFiles {
    stock_files_with(&[
        ("resource/ui/winpanel_variant.res", Some(WIN_PANEL)),
        (
            "resource/ui/winpanel.res",
            Some("#base \"winpanel_variant.res\"\n"),
        ),
    ])
}

// Block name, xpos, select id, picture name and count variable of each class button.
const CLASS_BUTTONS: [(&str, &str, &str, &str, &str); 10] = [
    ("scout", "c-300", "1", "scout", "numScout"),
    ("soldier", "c-250", "3", "soldier", "numSoldier"),
    ("pyro", "c-200", "7", "pyro", "numPyro"),
    ("demoman", "c-110", "4", "demo", "numDemoman"),
    ("heavyweapons", "c-60", "6", "heavy", "numHeavy"),
    ("engineer", "c-10", "9", "engineer", "numEngineer"),
    ("medic", "c78", "5", "medic", "numMedic"),
    ("sniper", "c128", "2", "sniper", "numSniper"),
    ("spy", "c178", "8", "spy", "numSpy"),
    ("random", "c250", "12", "random", ""),
];

/// A stock-shaped classselection.res: the root, ten buttons with their pictures and count
/// labels, the eleven teammate pictures, the local player's, and the menu's other controls.
/// Names and boxes follow the game's file; the text is ours.
pub fn class_select() -> String {
    let mut t = String::from("\"Resource/UI/ClassSelection.res\"\n{\n");
    t += "  class { ControlName Frame fieldName class xpos 0 ypos 0 wide f0 tall 480 visible 1 }\n";
    for (name, x, id, image, count) in CLASS_BUTTONS {
        t += &format!(
            "  {name} {{ ControlName CExImageButton xpos {x} ypos -5 zpos 6 wide 50 tall 100 visible 1 labelText {id} Command \"select {id}\" image_default class_sel_sm_{image}_inactive image_armed class_sel_sm_{image}_inactive\n    SubImage {{ ControlName ImagePanel xpos 0 ypos 0 zpos 7 wide 50 tall 100 visible 1 image class_sel_sm_{image}_inactive scaleImage 1 }} }}\n"
        );
        if !count.is_empty() {
            t += &format!(
                "  {count} {{ ControlName CExLabel xpos {x} ypos 10 wide 50 tall 12 visible 1 labelText %{count}% }}\n"
            );
        }
    }
    for i in 0..11 {
        let x = 20 + 25 * i;
        t += &format!(
            "  countImage{i} {{ ControlName CTFImagePanel xpos c{x} ypos 345 wide 30 tall 60 visible 0 }}\n"
        );
    }
    for name in [
        "Scout", "Solider", "Pyro", "Demoman", "Heavy", "Engineer", "Medic", "Sniper", "Spy",
    ] {
        t += &format!(
            "  MvMUpgradeImage{name} {{ ControlName ImagePanel xpos c-270 ypos 30 wide 10 tall 10 visible 0 image mvm/class_upgraded }}\n"
        );
    }
    t += r#"  localPlayerImage { ControlName CTFImagePanel xpos c-30 ypos 330 wide 45 tall 90 visible 0 }
  localPlayerBG { ControlName CTFImagePanel xpos c-35 ypos 345 wide 55 tall 80 visible 0 image ../hud/color_panel_clear }
  CountLabel { ControlName CExLabel xpos c35 ypos 402 wide 150 tall 18 visible 1 labelText #TF_TeamCount }
  CancelButton { ControlName CExButton xpos c-295 ypos r38 wide 100 tall 25 visible 1 labelText #TF_ClassMenu_Cancel }
  EditLoadoutButton { ControlName CExButton xpos r180 ypos r38 wide 150 tall 25 visible 1 labelText #EditLoadout }
  ResetButton { ControlName CExButton xpos r470 ypos r38 wide 110 tall 25 visible 0 labelText #TF_ClassMenu_Reset }
  ClassMenuSelect { ControlName CExLabel xpos 30 ypos r40 wide 450 tall 30 visible 1 labelText #TF_SelectAClass }
  MenuBG { ControlName CModelPanel xpos 0 ypos 0 zpos 0 wide f0 tall 480 visible 1 }
  TFPlayerModel { ControlName CTFPlayerModelPanel xpos 0 ypos 0 zpos 6 wide 480 tall 480 visible 1 }
  ClassTipsPanel { ControlName CTFClassTipsPanel xpos c28 ypos c-84 zpos 7 wide 235 tall 165 visible 1 }
  StartExplanation { ControlName CExplanationPopup xpos 0 ypos 0 wide 250 tall 165 visible 0
    TitleLabel { ControlName CExLabel xpos 20 ypos 10 wide 260 tall 30 visible 1 labelText #TF_MvM_UpgradeExplanation_Title } }
}
"#;
    t
}

/// The stock files with classselection.res reduced to a `#base` stub of a variant file.
pub fn class_stub() -> HudFiles {
    stock_files_with(&[
        (
            "resource/ui/classselection_variant.res",
            Some(&class_select()),
        ),
        (
            "resource/ui/classselection.res",
            Some("#base \"classselection_variant.res\"\n"),
        ),
    ])
}

/// A stock-shaped spectator.res: the root, the two bars, the labels the code writes, the tip
/// and the item panel. Names and boxes follow the game's file; the text is ours.
pub const SPECTATOR: &str = r##""Resource/UI/Spectator.res"
{
  Spectator { ControlName Frame fieldName Spectator tall 480 visible 1 enabled 1 }
  topbar { ControlName Panel fieldName TopBar xpos 0 ypos 0 tall 84 wide f0 visible 1 }
  BottomBar { ControlName Frame fieldName BottomBar xpos 0 ypos r70 tall 70 visible 1 enabled 0 }
  bottombarblank { ControlName Panel fieldName bottombarblank xpos 0 ypos r70 tall 70 wide f0 visible 1 }
  ReinforcementsLabel { ControlName CExLabel xpos c-300 ypos 50 wide 600 tall 18 visible 1 labelText "from the file" textAlignment center }
  BuyBackLabel { ControlName CExLabel xpos 0 ypos 0 wide 400 tall f0 visible 0 labelText "" textAlignment center }
  MapLabel { ControlName CExLabel xpos r260 ypos 16 wide 240 tall 20 visible 1 labelText "from the file" textAlignment east }
  ClassOrTeamLabel { ControlName CExLabel xpos c-85 ypos 68 wide 170 tall 15 zpos 2 visible 1 labelText "from the file" textAlignment center }
  SwitchCamModeKeyLabel { ControlName CExLabel xpos 5 ypos 10 wide 60 tall 20 visible 1 labelText "from the file" textAlignment east }
  SwitchCamModeLabel { ControlName CExLabel xpos 80 ypos 10 wide 125 tall 20 visible 1 labelText "#TF_Spectator_SwitchCamMode" textAlignment west }
  CycleTargetFwdKeyLabel { ControlName CExLabel xpos 5 ypos 20 wide 60 tall 20 visible 1 labelText "from the file" textAlignment east }
  CycleTargetFwdLabel { ControlName CExLabel xpos 80 ypos 20 wide 125 tall 20 visible 1 labelText "#TF_Spectator_CycleTargetFwd" textAlignment west }
  CycleTargetRevKeyLabel { ControlName CExLabel xpos 5 ypos 30 wide 60 tall 20 visible 1 labelText "from the file" textAlignment east }
  CycleTargetRevLabel { ControlName CExLabel xpos 80 ypos 30 wide 125 tall 20 visible 1 labelText "#TF_Spectator_CycleTargetRev" textAlignment west }
  TipLabel { ControlName CExLabel xpos 15 ypos r67 wide 145 tall 64 visible 1 labelText %tip% textAlignment center wrap 1 }
  itempanel { ControlName CItemModelPanel xpos 0 ypos 0 zpos 10 wide 190 tall 100 visible 0
    ItemLabel { ControlName Label xpos 10 ypos 3 zpos 1 wide 270 tall 9 visible 1 labelText "#FreezePanel_Item" } }
  spectator_extras { ControlName EditablePanel xpos 0 ypos 0 wide f0 tall 480 visible 1 }
}
"##;

/// The stock files with spectator.res reduced to a `#base` stub of a variant file.
pub fn spectator_stub() -> HudFiles {
    stock_files_with(&[
        ("resource/ui/spectator_variant.res", Some(SPECTATOR)),
        (
            "resource/ui/spectator.res",
            Some("#base \"spectator_variant.res\"\n"),
        ),
    ])
}

/// A stock-shaped targetid.res: the HUD's three backgrounds, the two labels, the health panel
/// the spectator code shows, and the icons and panels it hides.
pub const TARGET_ID: &str = r##""Resource/UI/TargetID.res"
{
  TargetIDBG { ControlName CTFImagePanel xpos 0 ypos 0 zpos -1 wide 252 tall 50 visible 1 image "../hud/color_panel_brown" }
  TargetIDBG_Spec_Blue { ControlName ScalableImagePanel xpos 0 ypos 0 zpos -1 wide 252 tall 30 visible 0 image "../hud/color_panel_blu" }
  TargetIDBG_Spec_Red { ControlName ScalableImagePanel xpos 0 ypos 0 zpos -1 wide 252 tall 30 visible 0 image "../hud/color_panel_red" }
  TargetNameLabel { ControlName Label font HudFontSmall xpos 8 ypos 5 zpos 1 wide 280 tall 24 visible 1 labelText %targetname% textAlignment North-West }
  TargetDataLabel { ControlName Label font HudFontSmall xpos 8 ypos 17 zpos 1 wide 280 tall 15 visible 1 labelText %targetdata% textAlignment Left }
  SpectatorGUIHealth { ControlName EditablePanel xpos 3 ypos 2 wide 32 tall 32 visible 0 enabled 0 }
  AmmoIcon { ControlName ImagePanel xpos 40 ypos 20 zpos 12 wide 8 tall 8 visible 0 image "../hud/leaderboard_class_heavy" }
  KillStreakIcon { ControlName ImagePanel xpos 10 ypos 20 zpos 12 wide 8 tall 8 visible 0 image "../hud/leaderboard_streak" }
  MoveableSubPanel { ControlName EditablePanel xpos 0 ypos 0 zpos -5 wide 32 tall 36 visible 1 }
  AvatarImage { ControlName CAvatarImagePanel xpos 6 ypos 6 zpos 99 wide 11 tall 11 visible 1 }
}
"##;

/// A stock-shaped spectatorguihealth.res: the health image and the label the code fills.
pub const SPEC_HEALTH: &str = r##""Resource/UI/SpectatorGUIHealth.res"
{
  PlayerStatusHealthImage { ControlName ImagePanel xpos 6 ypos 6 zpos 4 wide 20 tall 20 visible 1 scaleImage 1 }
  PlayerStatusHealthValue { ControlName CExLabel xpos 6 ypos 11 zpos 5 wide 20 tall 10 visible 1 labelText %Health% textAlignment center font HudFontSmall fgcolor TanDark }
}
"##;

/// The four files the target ID tests replace: hudlayout with a `CSpectatorTargetID` block, the
/// two files above and a scheme whose `Label.TextColor` is `TanLight`.
pub fn target_id_texts() -> Vec<(&'static str, String)> {
    vec![
        ("scripts/hudlayout.res", HUDLAYOUT.replacen(
            "  CMainTargetID {",
            "  CSpectatorTargetID { visible 0 xpos c-126 ypos 250 wide 252 tall 35 }\n  CMainTargetID {", 1)),
        ("resource/ui/targetid.res", TARGET_ID.to_owned()),
        ("resource/ui/spectatorguihealth.res", SPEC_HEALTH.to_owned()),
        ("resource/clientscheme.res", SCHEME.replacen(
            "  Fonts\n", "  BaseSettings { Label.TextColor TanLight }\n  Fonts\n", 1)),
    ]
}

/// The stock files with `target_id_texts` applied.
pub fn target_id_files() -> HudFiles {
    let texts = target_id_texts();
    let over: Vec<(&str, Option<&str>)> =
        texts.iter().map(|(p, t)| (*p, Some(t.as_str()))).collect();
    stock_files_with(&over)
}
