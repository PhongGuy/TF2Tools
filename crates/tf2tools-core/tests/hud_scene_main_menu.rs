//! The main menu scene: its file, its part tree, the wide-screen condition and its problems.

mod common;

use common::hud_stock::{stock_files, stock_files_with};
use tf2tools_core::{
    GameText, HiddenBy, HideTechnique, HudModel, OptionValue, PanelRule, PartId, PartKey, Platform,
    RankPieces, SampleSet, Scene, SceneRule, Screen, rank_medal, rank_pieces,
};

const MENU_FILE: &str = "resource/ui/mainmenuoverride.res";
const HD: Screen = Screen::new(1920, 1080);

/// A stock-shaped main menu, no Valve bytes: a root holding a button template, a background
/// sized by the screen shape, a logo, the code-made class picture, a store button, a rank
/// panel and the friends list with one child.
const MENU: &str = r##""Resource/UI/MainMenuOverride.res"
{
  MainMenuOverride { fieldName MainMenuOverride xpos 0 ypos 0 wide f0 tall 480
    button_kv { xpos 0 ypos 150 wide 250 tall 26 SubButton { ControlName CExImageButton wide 250 tall 26 } } }
  Background { ControlName ScalableImagePanel xpos cs-0.5 ypos 0 zpos -200 wide o1.6 tall f0 visible 0 image ""
    if_wider { wide f0 tall o0.628 } if_taller { wide o1.6 tall f0 } }
  TFLogoImage { ControlName ImagePanel xpos c-290 ypos 32 zpos 1 wide 256 tall 64 image ../logo/new_tf2_logo }
  TFCharacterImage { xpos c-250 ypos -80 zpos -99 wide 600 tall 600 visible 1 scaleImage 1 }
  GeneralStoreButton { ControlName CExImageButton xpos c-165 ypos 437 wide 100 tall 26 labelText "#MMenu_Shop" }
  RankPanel { ControlName CPvPRankPanel xpos c-350 ypos 72 wide 320 tall 100 visible 0 }
  FriendsContainer { ControlName EditablePanel xpos c-290 ypos 260 zpos 5 wide 260 tall 150 visible 1
    SteamFriendsList { ControlName CSteamFriendsListPanel xpos 0 ypos 0 wide 260 tall 150 } }
}
"##;

fn menu() -> HudModel {
    let files = stock_files_with(&[(MENU_FILE, Some(MENU))]);
    HudModel::load_scene(&files, Platform::Linux, Scene::MainMenu).unwrap()
}

fn k(text: &str) -> PartKey {
    PartKey::new(text)
}

fn keys(m: &HudModel) -> Vec<&str> {
    m.parts().iter().map(|p| p.key().as_str()).collect()
}

#[test]
fn the_main_menu_scene_reads_its_override_file_and_hides_the_hud() {
    let rule = SceneRule::get(Scene::MainMenu);
    assert_eq!(rule.label(), "Main menu");
    assert_eq!(rule.files(), [MENU_FILE]);
    assert_eq!(rule.root(), Some("MainMenuOverride"));
    assert!(rule.hides_hud());
    assert_eq!(rule.sample(), SampleSet::Empty);
    let [panel] = rule.rules() else {
        panic!("one rule")
    };
    assert_eq!(panel.id(), PartId::MainMenu);
    assert_eq!(
        (panel.panel(), panel.parent(), panel.children_file()),
        ("MainMenuOverride", None, MENU_FILE)
    );
    assert_eq!(panel.label(), "Main menu");
    assert!(panel.shows_when().starts_with("Shows "));
    assert!(panel.geometry().is_empty());
    assert_eq!(panel.conditions(), ["if_wider"]);
    assert!(!panel.centres_x() && !panel.centres_y());
    assert!(panel.has_children() && !panel.clickable());
    assert_eq!(panel.hide(), HideTechnique::NotOffered);
    assert_eq!(PanelRule::get(PartId::MainMenu).panel(), "MainMenuOverride");
    assert_eq!(Scene::for_file(MENU_FILE), Some(Scene::MainMenu));
    assert_eq!(
        Scene::for_file("RESOURCE/UI/MainMenuOverride.res"),
        Some(Scene::MainMenu)
    );
}

#[test]
fn the_main_menu_scene_resolves_the_override_tree() {
    let m = menu();
    assert_eq!(m.scene(), Scene::MainMenu);
    assert!(m.problems().is_empty());
    let root = m.part(&k("MainMenuOverride")).unwrap();
    assert!(root.is_root() && root.is_readable());
    assert_eq!(root.owner(), Some(PartId::MainMenu));
    let keys = keys(&m);
    for key in [
        "MainMenuOverride/Background",
        "MainMenuOverride/TFLogoImage",
        "MainMenuOverride/GeneralStoreButton",
        "MainMenuOverride/RankPanel",
        "MainMenuOverride/FriendsContainer",
        "MainMenuOverride/FriendsContainer/SteamFriendsList",
    ] {
        assert!(keys.contains(&key), "{key} in {keys:?}");
    }
    // The root's button template is settings, not a control on the screen.
    assert!(
        !keys
            .iter()
            .any(|k| k.contains("button_kv") || k.contains("SubButton"))
    );
    let friends = m.part(&k("MainMenuOverride/FriendsContainer")).unwrap();
    assert_eq!(friends.class(), Some("EditablePanel"));
    assert_eq!(friends.owner(), Some(PartId::MainMenu));
    // The HUD scene of the same files is unchanged.
    let hud = m.with_scene(Scene::Hud);
    assert!(
        keys_of(&hud)
            .iter()
            .all(|k| !k.starts_with("MainMenuOverride"))
    );
}

fn keys_of(m: &HudModel) -> Vec<String> {
    m.parts()
        .iter()
        .map(|p| p.key().as_str().to_owned())
        .collect()
}

#[test]
fn the_main_menu_lays_out_for_a_wide_screen() {
    let l = menu().layout(HD);
    let at = |key: &str| {
        let b = l.get(&k(key)).unwrap_or_else(|| panic!("{key}"));
        (b.x(), b.y(), b.wide(), b.tall())
    };
    assert_eq!(at("MainMenuOverride"), (0, 0, 1920, 1080));
    assert_eq!(at("MainMenuOverride/TFLogoImage"), (308, 72, 576, 144));
    assert_eq!(
        at("MainMenuOverride/FriendsContainer"),
        (308, 585, 585, 337)
    );
    // `if_wider` holds at 16:9, so the background takes the screen's width.
    assert_eq!(at("MainMenuOverride/Background"), (0, 0, 1920, 1205));
    assert!(l.get(&k("MainMenuOverride")).unwrap().hidden().is_none());
    assert!(menu().hidden_by_hud().is_none());
}

#[test]
fn a_missing_menu_file_is_a_problem_for_the_main_menu_only() {
    let files = stock_files_with(&[(MENU_FILE, None)]);
    let m = HudModel::load_scene(&files, Platform::Linux, Scene::MainMenu).unwrap();
    assert_eq!(m.problems(), [MENU_FILE]);
    assert!(!m.part(&k("MainMenuOverride")).unwrap().is_readable());
    for scene in [Scene::Hud, Scene::Scoreboard] {
        assert!(m.with_scene(scene).problems().is_empty(), "{scene:?}");
    }
    // An unparsable one reads the same way.
    let files = stock_files_with(&[(MENU_FILE, Some("}"))]);
    let m = HudModel::load_scene(&files, Platform::Linux, Scene::MainMenu).unwrap();
    assert_eq!(m.problems(), [MENU_FILE]);
}

#[test]
fn hiding_the_friends_list_hides_it_in_the_main_menu() {
    let m = menu();
    assert!(m.options().iter().any(|o| o.id() == "hide_friends"));
    let changes = m
        .set_option("hide_friends", OptionValue::Bool(true), &[], 1)
        .unwrap();
    let edited = m.with_changes(&changes).into_model();
    assert_eq!(edited.scene(), Scene::MainMenu);
    let l = edited.layout(HD);
    let friends = l.get(&k("MainMenuOverride/FriendsContainer")).unwrap();
    assert!(friends.hidden().is_some());
    // The stock fixture's main menu loads too, and the option still fits it.
    let stock = HudModel::load_scene(&stock_files(), Platform::Linux, Scene::MainMenu).unwrap();
    assert!(stock.problems().is_empty());
    assert!(stock.options().iter().any(|o| o.id() == "hide_friends"));
}

#[test]
fn the_code_made_class_picture_is_an_image_part() {
    let m = menu();
    let picture = m.part(&k("MainMenuOverride/TFCharacterImage")).unwrap();
    assert_eq!(picture.class(), Some("ImagePanel"));
    assert!(picture.is_readable());
    let rule = SceneRule::get(Scene::MainMenu);
    assert_eq!(rule.code_class("tfcharacterimage"), Some("ImagePanel"));
    assert_eq!(rule.code_class("FriendsContainer"), None);
    for scene in [Scene::Hud, Scene::Scoreboard] {
        assert_eq!(SceneRule::get(scene).code_class("TFCharacterImage"), None);
    }
    let l = m.layout(HD);
    let b = l.get(&k("MainMenuOverride/TFCharacterImage")).unwrap();
    assert_eq!((b.x(), b.y(), b.wide(), b.tall()), (398, -180, 1350, 1350));
    assert!(b.hidden().is_none());
    // Only a direct child of the root is made by the code; a nested block without a
    // ControlName stays settings.
    let nested = MENU.replace(
        "SteamFriendsList { ControlName CSteamFriendsListPanel",
        "TFCharacterImage { wide 10 } SteamFriendsList { ControlName CSteamFriendsListPanel",
    );
    let files = stock_files_with(&[(MENU_FILE, Some(&nested))]);
    let m = HudModel::load_scene(&files, Platform::Linux, Scene::MainMenu).unwrap();
    assert!(
        m.part(&k("MainMenuOverride/FriendsContainer/TFCharacterImage"))
            .is_none()
    );
}

#[test]
fn the_main_menu_shows_what_the_game_shows_at_the_menu() {
    let rule = SceneRule::get(Scene::MainMenu);
    let shown: Vec<&str> = (rule.code_toggled().iter())
        .filter(|t| t.shown_in_sample())
        .map(|t| t.panel())
        .collect();
    assert_eq!(
        shown,
        [
            "TFCharacterImage",
            "RankPanel",
            "RankModelPanel",
            "CycleRankTypeButton"
        ]
    );
    assert_eq!(rule.code_toggled().len(), 30);
    for t in rule.code_toggled() {
        assert_eq!(t.hide(), HideTechnique::SizeZero, "{}", t.panel());
        assert!(t.shows_for().is_none(), "{}", t.panel());
        let when = t.shows_when();
        assert!(
            when.starts_with("Shows ") || when == "The game keeps this hidden",
            "{}: {when}",
            t.panel()
        );
    }
    let when = |panel| rule.toggled(panel).unwrap().shows_when();
    assert_eq!(when("GeneralStoreButton"), "The game keeps this hidden");
    assert_eq!(when("CallVoteButton"), "Shows while you are in a game");
    assert!(rule.toggled("FriendsContainer").is_none());
    assert!(rule.toggled("TFLogoImage").is_none());

    // In the preview the rank panel shows though its file hides it, the store button and the
    // empty holiday background stay hidden, and the friends list keeps its own `visible`.
    let l = menu().layout(HD);
    let hidden = |key: &str| l.get(&k(key)).unwrap().hidden();
    assert_eq!(hidden("MainMenuOverride/RankPanel"), None);
    assert_eq!(
        hidden("MainMenuOverride/GeneralStoreButton"),
        Some(HiddenBy::Game)
    );
    assert_eq!(hidden("MainMenuOverride/Background"), Some(HiddenBy::Game));
    assert_eq!(hidden("MainMenuOverride/FriendsContainer"), None);
    let m = menu();
    let store = m.part(&k("MainMenuOverride/GeneralStoreButton")).unwrap();
    assert!(store.code_toggled().is_some());
    let nested = m
        .part(&k("MainMenuOverride/FriendsContainer/SteamFriendsList"))
        .unwrap();
    assert!(nested.code_toggled().is_none());
}

/// The stock file stacks two spare settings buttons on the replay and bug-report buttons; the
/// game keeps the spares hidden, so only the replay and bug-report buttons draw there.
#[test]
fn the_spare_settings_buttons_stay_hidden_over_the_replay_and_bug_buttons() {
    const FOOTER: &str = r##""Resource/UI/MainMenuOverride.res"
{
  MainMenuOverride { fieldName MainMenuOverride xpos 0 ypos 0 wide f0 tall 480 }
  ReplayButton { ControlName CExImageButton xpos c-34+273 ypos 437 zpos 5 wide 25 tall 25 visible 1 }
  ReportBugButton { ControlName CExImageButton xpos c-14+273 ypos 437 zpos 4 wide 30 tall 25 visible 1 }
  SettingsButtonSDK { ControlName CExImageButton xpos c-34+273 ypos 437 zpos 4 wide 25 tall 25 visible 1 }
  TF2SettingsButtonSDK { ControlName CExImageButton xpos c-0+273 ypos 437 zpos 5 wide 25 tall 25 visible 1 }
}
"##;
    let rule = SceneRule::get(Scene::MainMenu);
    for spare in ["SettingsButtonSDK", "TF2SettingsButtonSDK"] {
        let t = rule.toggled(spare).unwrap_or_else(|| panic!("{spare}"));
        assert!(!t.shown_in_sample(), "{spare}");
        assert_eq!(t.shows_when(), "The game keeps this hidden");
    }
    assert!(rule.toggled("ReplayButton").is_none());
    assert!(rule.toggled("ReportBugButton").is_none());

    let files = stock_files_with(&[(MENU_FILE, Some(FOOTER))]);
    let m = HudModel::load_scene(&files, Platform::Linux, Scene::MainMenu).unwrap();
    let l = m.layout(HD);
    let hidden = |key: &str| l.get(&k(key)).unwrap().hidden();
    assert_eq!(
        hidden("MainMenuOverride/SettingsButtonSDK"),
        Some(HiddenBy::Game)
    );
    assert_eq!(
        hidden("MainMenuOverride/TF2SettingsButtonSDK"),
        Some(HiddenBy::Game)
    );
    assert_eq!(hidden("MainMenuOverride/ReplayButton"), None);
    assert_eq!(hidden("MainMenuOverride/ReportBugButton"), None);
}

/// A main menu button's text starts past its text inset, clear of the icon its `SubImage`
/// draws at the west end. A proportional inset scales with the screen; a plain one is pixels,
/// taken from the side the text is aligned to.
#[test]
fn a_menu_buttons_text_starts_after_its_inset_clear_of_its_icon() {
    const BUTTONS: &str = r##""Resource/UI/MainMenuOverride.res"
{
  MainMenuOverride { fieldName MainMenuOverride xpos 0 ypos 0 wide f0 tall 480 }
  CharacterSetupButton { ControlName CExImageButton xpos c-288 ypos 400 wide 100 tall 26
    textinsetx 25 use_proportional_insets 1 font HudFontSmallBold textAlignment west labeltext Items
    SubImage { ControlName ImagePanel xpos 6 ypos 6 wide 14 tall 14 image glyph_items } }
  EastButton { ControlName CExImageButton xpos 0 ypos 300 wide 100 tall 26
    textinsetx 10 font HudFontSmallBold textAlignment east labeltext Go }
  TopButton { ControlName CExImageButton xpos 0 ypos 200 wide 100 tall 26
    textinsety 4 use_proportional_insets 1 font HudFontSmallBold textAlignment north-west labeltext Up }
}
"##;
    let files = stock_files_with(&[(MENU_FILE, Some(BUTTONS))]);
    let m = HudModel::load_scene(&files, Platform::Linux, Scene::MainMenu).unwrap();
    let (l, text) = (m.layout(HD), GameText::default());
    let extent = |key: &str| m.text_extent(&l, &k(key), &text).unwrap();
    let items = l.get(&k("MainMenuOverride/CharacterSetupButton")).unwrap();
    let icon = l
        .get(&k("MainMenuOverride/CharacterSetupButton/SubImage"))
        .unwrap();
    // 25 units at 1080 high is 56 pixels.
    let (x, ..) = extent("MainMenuOverride/CharacterSetupButton");
    assert_eq!(x, items.x() + 56);
    assert!(x >= icon.x() + icon.wide(), "text clear of the icon");
    let east = l.get(&k("MainMenuOverride/EastButton")).unwrap();
    let (x, _, w, _) = extent("MainMenuOverride/EastButton");
    assert_eq!(
        x + w,
        east.x() + east.wide() - 10,
        "a plain inset is pixels"
    );
    let top = l.get(&k("MainMenuOverride/TopButton")).unwrap();
    let (x, y, ..) = extent("MainMenuOverride/TopButton");
    assert_eq!((x, y), (top.x(), top.y() + 9));
}

/// The y inset moves text down whatever its alignment, rounding a proportional decimal up;
/// an inset that pushes text out of its box leaves an empty extent inside the box.
#[test]
fn a_y_inset_moves_text_down_and_an_extent_stays_in_its_box() {
    let button = |name: &str, align: &str, keys: &str| {
        format!(
            "  {name} {{ ControlName CExImageButton xpos 0 ypos 100 wide 100 tall 40 \
             textAlignment {align} labeltext Go {keys} }}\n"
        )
    };
    let mut blocks = String::new();
    for align in ["center", "south"] {
        blocks += &button(&format!("{align}0"), align, "");
        blocks += &button(&format!("{align}4"), align, "textinsety 4");
        blocks += &button(
            &format!("{align}p"),
            align,
            "textinsety 2.5 use_proportional_insets 1",
        );
    }
    blocks += &button("Huge", "west", "textinsetx 900 use_proportional_insets 1");
    blocks += &button("HugeY", "south", "textinsety 900");
    blocks += &button("Negative", "west", "textinsetx -30 textinsety -30");
    let menu = format!(
        "\"Resource/UI/MainMenuOverride.res\"\n{{\n  MainMenuOverride {{ fieldName \
         MainMenuOverride xpos 0 ypos 0 wide f0 tall 480 }}\n{blocks}}}\n"
    );
    let files = stock_files_with(&[(MENU_FILE, Some(&menu))]);
    let m = HudModel::load_scene(&files, Platform::Linux, Scene::MainMenu).unwrap();
    let (l, text) = (m.layout(HD), GameText::default());
    let key = |name: &str| k(&format!("MainMenuOverride/{name}"));
    let extent = |name: &str| m.text_extent(&l, &key(name), &text).unwrap();
    for align in ["center", "south"] {
        let (_, y0, ..) = extent(&format!("{align}0"));
        let (_, y4, ..) = extent(&format!("{align}4"));
        let (_, yp, ..) = extent(&format!("{align}p"));
        assert_eq!(y4, y0 + 4, "{align}: a plain inset moves the text down");
        // 2.5 units at 1080 high is 5.625 pixels, rounded up.
        assert_eq!(yp, y0 + 6, "{align}: a proportional decimal rounds up");
    }
    for name in ["Huge", "HugeY", "Negative"] {
        let b = l.get(&key(name)).unwrap();
        let (x, y, w, h) = extent(name);
        assert!(w >= 0 && h >= 0, "{name}: ({x}, {y}, {w}, {h})");
        assert!(
            x >= b.x() && x + w <= b.x() + b.wide(),
            "{name}: x {x} w {w}"
        );
        assert!(
            y >= b.y() && y + h <= b.y() + b.tall(),
            "{name}: y {y} h {h}"
        );
    }
    assert_eq!(
        extent("Huge").2,
        0,
        "text pushed past the box draws nothing"
    );
    let neg = l.get(&key("Negative")).unwrap();
    assert_eq!(
        extent("Negative").0,
        neg.x(),
        "text pulled out starts at the box's edge"
    );
}

/// The game narrows a label's text to its box less the x inset whatever its alignment, then
/// centres or right-aligns that narrowed text; text still wider than its box (only with a
/// negative inset) starts at the west edge, an east label then moved by its inset.
#[test]
fn an_x_inset_narrows_the_text_as_the_game_does() {
    let long = "Gooooooooooooooooooooooooooooooooooooooo";
    let button = |name: &str, align: &str, keys: &str| {
        format!(
            "  {name} {{ ControlName CExImageButton xpos 100 ypos 100 wide 100 tall 40 \
             textAlignment {align} labeltext {long} {keys} }}\n"
        )
    };
    let mut blocks = String::new();
    blocks += &button("Centre0", "center", "");
    blocks += &button("Centre20", "center", "textinsetx 20");
    blocks += &button("East20", "east", "textinsetx 20");
    blocks += &button("EastNeg", "east", "textinsetx -30");
    blocks += &button("CentreNeg", "center", "textinsetx -30");
    let menu = format!(
        "\"Resource/UI/MainMenuOverride.res\"\n{{\n  MainMenuOverride {{ fieldName \
         MainMenuOverride xpos 0 ypos 0 wide f0 tall 480 }}\n{blocks}}}\n"
    );
    let files = stock_files_with(&[(MENU_FILE, Some(&menu))]);
    let m = HudModel::load_scene(&files, Platform::Linux, Scene::MainMenu).unwrap();
    let screen = Screen::new(640, 480);
    let (l, text) = (m.layout(screen), GameText::default());
    let key = |name: &str| k(&format!("MainMenuOverride/{name}"));
    let extent = |name: &str| {
        let (x, _, w, _) = m.text_extent(&l, &key(name), &text).unwrap();
        (x, w)
    };
    let left = l.get(&key("Centre0")).unwrap().x();
    assert_eq!(extent("Centre0"), (left, 100), "too wide fills the box");
    assert_eq!(extent("Centre20"), (left + 10, 80), "narrowed and centred");
    assert_eq!(extent("East20"), (left, 80), "narrowed and moved in");
    // 130 wide in a 100 box: drawn from the west edge, then the east inset moves it 30 right.
    assert_eq!(extent("EastNeg"), (left + 30, 70));
    assert_eq!(extent("CentreNeg"), (left, 100));
}

/// The stock rank panels: the code-made level panel with no medal, and the medal panel with
/// no level bar, as the game's file sets them.
const RANK_MENU: &str = r##""Resource/UI/MainMenuOverride.res"
{
  MainMenuOverride { fieldName MainMenuOverride xpos 0 ypos 0 wide f0 tall 480 }
  RankModelPanel { ControlName CPvPRankPanel xpos cs-0.5-256 ypos cs-0.5-120 zpos -51 wide 1000 tall 1000 visible 0 show_progress 0 }
  RankPanel { ControlName CPvPRankPanel xpos c-350 ypos 72 zpos -52 wide 320 tall 100 visible 0 show_model 0 show_type 1 }
  Plain { ControlName CPvPRankPanel xpos 0 ypos 0 wide 320 tall 100 }
}
"##;

#[test]
fn a_rank_panel_lays_out_the_games_level_bar_name_and_medal() {
    let files = stock_files_with(&[(MENU_FILE, Some(RANK_MENU))]);
    let m = HudModel::load_scene(&files, Platform::Linux, Scene::MainMenu).unwrap();
    let screen = Screen::new(640, 480);
    let pieces = |name: &str, wide, tall| {
        let part = m.part(&k(&format!("MainMenuOverride/{name}"))).unwrap();
        rank_pieces(&m, part, wide, tall, screen)
    };
    // The level panel: the rank name and the bar on the right, beside where the medal sits;
    // the sample fills the bar three fifths of the way.
    let level = pieces("RankPanel", 320, 100);
    assert_eq!(level.medal(), None);
    assert_eq!(level.name(), Some((125, 25, 195, 20)));
    assert_eq!(level.bar(), Some((126, 60, 188, 7)));
    assert_eq!(level.fill(), Some((127, 61, 111, 5)));
    assert_eq!(RankPieces::SAMPLE_NAME, "Level 42");
    // The medal panel: the medal as big as the game's camera shows it, centred, no bar.
    let medal = pieces("RankModelPanel", 1000, 1000);
    assert_eq!(medal.medal(), Some((458, 458, 84, 84)));
    assert_eq!(
        (medal.name(), medal.bar(), medal.fill()),
        (None, None, None)
    );
    // Unset keys show both; on a 1080-line screen the pieces scale with it.
    let plain = m.part(&k("MainMenuOverride/Plain")).unwrap();
    let both = rank_pieces(&m, plain, 720, 225, HD);
    assert_eq!(both.medal(), Some((351, 103, 18, 18)));
    assert_eq!(both.name(), Some((281, 56, 439, 45)));
    assert_eq!(both.bar(), Some((282, 137, 424, 15)));
    assert_eq!(both.fill(), Some((284, 139, 252, 11)));
    // The level area keeps its width past a narrow box's left edge and no height below zero;
    // extreme boxes never overflow.
    let tiny = pieces("RankPanel", 10, 10);
    assert_eq!(tiny.bar(), None);
    let huge = pieces("Plain", i32::MAX, i32::MAX);
    assert!(huge.medal().is_some_and(|(_, _, side, _)| side > 0));
}

#[test]
fn the_medal_stand_in_ends_left_of_the_rank_name_at_stock_geometry() {
    let files = stock_files_with(&[(MENU_FILE, Some(RANK_MENU))]);
    let m = HudModel::load_scene(&files, Platform::Linux, Scene::MainMenu).unwrap();
    let (medal_key, level_key) = (
        k("MainMenuOverride/RankModelPanel"),
        k("MainMenuOverride/RankPanel"),
    );
    for screen in [HD, Screen::new(1280, 960)] {
        let l = m.layout(screen);
        let (mb, lb) = (l.get(&medal_key).unwrap(), l.get(&level_key).unwrap());
        let (mx, my, mw, mh) = rank_medal(&m, &l, &medal_key, screen).unwrap();
        let name = rank_pieces(
            &m,
            m.part(&level_key).unwrap(),
            lb.wide(),
            lb.tall(),
            screen,
        )
        .name()
        .unwrap();
        let (nx, ny) = (lb.x() + name.0, lb.y() + name.1);
        let (mx, my) = (mb.x() + mx, mb.y() + my);
        assert!(mw > 0 && mh > 0);
        // Clear of the name's box, by a gap, and still level with it.
        assert!(
            mx + mw < nx,
            "{screen:?}: medal ends at {} but name starts at {nx}",
            mx + mw
        );
        assert!(my < ny + name.3 && ny < my + mh);
    }
}
