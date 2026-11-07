//! The Win panel tab: what it draws, its backdrop and its edits in the shared list.

use crate::support;

use gpui_kit::{AppContext as _, Entity, TestAppContext, px};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::{Field, HudMakerPage};
use tf2tools_app::services::Tf2State;
use tf2tools_core::{PartKey, Scene, Tf2Install};

const TEAMS: &str = "WinPanel/TeamScoresPanel";
const LINE: &str = "WinPanel/WinningTeamLabel";
const FREEZE: &str = "FreezePanel/FreezePanelBase";
const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";

fn found(env: &support::Env) -> Tf2State {
    Tf2State::Found(Tf2Install::from_path(&env.game).unwrap())
}

fn open_maker(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<HudMakerPage> {
    support::click_nav(cx, s.win, tf2tools_app::shell::route::Page::HudMaker);
    let page = cx.update(|cx| s.shell.read(cx).hud_maker_page()).unwrap();
    support::wait_real(cx, "the HUD to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

fn present(cx: &mut TestAppContext, s: &support::ShellWindow, key: &str) -> bool {
    support::present(cx, s.win, ids::hud_maker_part(&PartKey::new(key)))
}

#[gpui_kit::test]
fn the_tab_is_last_and_switches_to_the_win_panel(cx: &mut TestAppContext) {
    let env = support::env();
    support::seed_stock_game_with(&env.game, &[]);
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_maker(cx, &s);
    let tab = ids::hud_maker_scene(Scene::WinPanel);
    assert!(format!("{tab:?}").contains("win-panel"));
    assert!(support::present(cx, s.win, tab.clone()));
    support::click_scene(cx, s.win, Scene::WinPanel);
    assert_eq!(cx.update(|cx| page.read(cx).scene()), Some(Scene::WinPanel));
    // Its own parts, none of the HUD's or the freeze cam's, and no crosshair.
    assert!(present(cx, &s, TEAMS) && present(cx, &s, LINE));
    assert!(!present(cx, &s, HV) && !present(cx, &s, FREEZE));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_CROSSHAIR));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_SCENE_NOTE));
    // What the game hides stays off the canvas; the top players it shows are on it.
    assert!(!present(
        cx,
        &s,
        "WinPanel/TeamScoresPanel/BlueLeaderAvatar"
    ));
    assert!(!present(cx, &s, "WinPanel/Player1Badge"));
    assert!(present(cx, &s, "WinPanel/Player3Name"));
    support::click_scene(cx, s.win, Scene::Hud);
    assert!(present(cx, &s, HV) && !present(cx, &s, LINE));
}

#[gpui_kit::test]
fn a_hud_with_a_stub_win_file_shows_it_read_only_with_the_include_note(cx: &mut TestAppContext) {
    let env = support::env();
    support::seed_stock_game_with(
        &env.game,
        &[
            (
                "resource/ui/winpanel_variant.res",
                Some(support::hud_stock::WIN_PANEL),
            ),
            (
                "resource/ui/winpanel.res",
                Some("#base \"winpanel_variant.res\"\n"),
            ),
        ],
    );
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::WinPanel);
    assert!(present(cx, &s, LINE));
    // Pick the line (the Parts list may be folded in a narrow window, as in hud_maker_base.rs):
    // the side panel says why it cannot change, as it does for the freeze cam.
    if !support::present(cx, s.win, ids::HUD_MAKER_PARTS) {
        support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_COLLAPSE);
    }
    support::click_part_row(cx, s.win, &PartKey::new(LINE));
    assert!(support::present(cx, s.win, ids::HUD_MAKER_PANEL_FROM_BASE));
}

#[gpui_kit::test]
fn nothing_picked_the_panel_says_how_to_start(cx: &mut TestAppContext) {
    let env = support::env();
    support::seed_stock_game_with(&env.game, &[]);
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::WinPanel);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_PANEL_EMPTY).as_deref(),
        Some("Click a part of the win panel to change it.")
    );
}

fn utf16le(text: &str) -> Vec<u8> {
    let mut out = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}

/// A synthetic English file; no Valve bytes.
const ENGLISH: &str = r#""lang" { "Language" "English" "Tokens" {
  "Winpanel_KillStreakLeader" "Highest Killstreak:" } }"#;

fn label(cx: &mut TestAppContext, s: &support::ShellWindow, key: &str) -> Option<String> {
    support::label_of(cx, s.win, ids::hud_maker_part(&PartKey::new(key)))
}

/// A solid 8x8 panel texture; the two team panels get different colours so a test can tell them apart.
fn texture(name: &str, rgba: [u8; 4]) -> (String, Vec<u8>) {
    let image = tf2tools_core::RgbaImage::from_pixels(8, 8, rgba.repeat(64)).unwrap();
    let vmt = format!(r#""UnlitGeneric" {{ "$baseTexture" "hud/{name}" }}"#);
    (vmt, tf2tools_core::rgba_to_vtf(&image).unwrap())
}

/// The stock game with the two team borders as scalable images, seeded when `seeded`. The win
/// panel's file asks for a third border, `HudOwnBG`, whose image is never seeded, on the
/// winner's frame: it draws only if the game's BLU border replaces the file's.
fn shell_with_borders(
    cx: &mut TestAppContext,
    env: &support::Env,
    seeded: bool,
) -> support::ShellWindow {
    let scheme = support::hud_stock::SCHEME.replacen(
        "  Fonts\n",
        "  Borders {\n\
         TFFatLineBorderBlueBG { bordertype scalable_image image ../hud/color_panel_blu src_corner_height 2 src_corner_width 2 draw_corner_width 8 draw_corner_height 8 }\n\
         TFFatLineBorderRedBG { bordertype scalable_image image ../hud/color_panel_red src_corner_height 2 src_corner_width 2 draw_corner_width 8 draw_corner_height 8 }\n\
         HudOwnBG { bordertype scalable_image image ../hud/hud_own_panel src_corner_height 2 src_corner_width 2 draw_corner_width 8 draw_corner_height 8 } }\n  Fonts\n",
        1,
    );
    let panel = support::hud_stock::WIN_PANEL.replace(
        "border TFFatLineBorderBlueBG proportionaltoparent",
        "border HudOwnBG proportionaltoparent",
    );
    assert_ne!(panel, support::hud_stock::WIN_PANEL);
    let (blu_vmt, blu_vtf) = texture("color_panel_blu", [0, 0, 255, 255]);
    let (red_vmt, red_vtf) = texture("color_panel_red", [255, 0, 0, 255]);
    let english = utf16le(ENGLISH);
    let mut extra: Vec<(&str, &[u8])> = vec![("resource/tf_english.txt", english.as_slice())];
    if seeded {
        extra.extend([
            ("materials/hud/color_panel_blu.vmt", blu_vmt.as_bytes()),
            ("materials/hud/color_panel_blu.vtf", blu_vtf.as_slice()),
            ("materials/hud/color_panel_red.vmt", red_vmt.as_bytes()),
            ("materials/hud/color_panel_red.vtf", red_vtf.as_slice()),
        ]);
    }
    support::seed_stock_game_with_bytes(
        &env.game,
        &[
            ("resource/clientscheme.res", Some(&scheme)),
            ("resource/ui/winpanel.res", Some(&panel)),
        ],
        &extra,
    );
    support::shell_window(cx, env, found(env))
}

#[gpui_kit::test]
fn the_panel_draws_the_line_the_scores_and_the_top_players_the_code_fills(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with_borders(cx, &env, true);
    let _page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::WinPanel);
    let want = [
        (LINE, "BLU TEAM WINS!"),
        ("WinPanel/AdvancingTeamLabel", ""),
        ("WinPanel/WinReasonLabel", "BLU captured all control points"),
        (
            "WinPanel/DetailsLabel",
            "Winning capture: Blue Rocket, Demoknight",
        ),
        ("WinPanel/TopPlayersLabel", "BLU Team MVPs:"),
        ("WinPanel/TeamScoresPanel/BlueTeamScore", "2"),
        ("WinPanel/TeamScoresPanel/RedTeamScore", "1"),
        ("WinPanel/TeamScoresPanel/BlueTeamLabel", "BLU"),
        // The file's labelText is blank: the code sets these.
        ("WinPanel/Player1Name", "Blue Rocket"),
        ("WinPanel/Player2Score", "19"),
        ("WinPanel/Player3Class", "Demoman"),
        ("WinPanel/KillStreakPlayer1Name", "Rocket Jumper"),
        // A game token still resolves through the game's text.
        ("WinPanel/KillStreakLeaderLabel", "Highest Killstreak:"),
    ];
    for (key, text) in want {
        // A blank line draws nothing, so an absent label reads as "".
        let got = label(cx, &s, key).unwrap_or_default();
        assert_eq!(got, text, "{key}");
    }
}

#[gpui_kit::test]
fn the_game_s_team_border_replaces_the_one_the_file_names(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with_borders(cx, &env, true);
    let page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::WinPanel);
    // The file names HudOwnBG, whose texture does not exist: the frame draws nine cells only
    // because the code's BLU border replaced it.
    let frame = PartKey::new("WinPanel/WinPanelBGBorder");
    let part = support::bounds_of(cx, s.win, ids::hud_maker_part(&frame)).unwrap();
    for i in 0..9 {
        let cell = support::bounds_of(cx, s.win, ids::hud_maker_border_cell(&frame, i)).unwrap();
        assert!(
            cell.left() >= part.left() - px(1.) && cell.right() <= part.right() + px(1.),
            "cell {i} {cell:?} {part:?}"
        );
    }
    // Both score frames draw, each from its own team's texture, which differ in colour.
    for name in ["BlueScoreBG", "RedScoreBG"] {
        let key = PartKey::new(format!("WinPanel/TeamScoresPanel/{name}"));
        assert!(
            support::present(cx, s.win, ids::hud_maker_border_cell(&key, 0)),
            "{name}"
        );
    }
    let bytes = |name: &str| {
        cx.update(|cx| page.read(cx).image(&PartKey::new(name)))
            .map(|image| image.as_bytes(0).map(<[u8]>::to_vec))
    };
    let (blue, red) = (
        bytes("../hud/color_panel_blu"),
        bytes("../hud/color_panel_red"),
    );
    // `bytes()` is Option<Option<Vec>>: the image decoded, and it has a first frame.
    assert!(matches!(blue, Some(Some(_))) && matches!(red, Some(Some(_))));
    assert_ne!(
        blue, red,
        "the two team textures decode to different pixels"
    );
    // The file's own border was never decoded: the code's replaced it before the decode.
    assert!(bytes("../hud/hud_own_panel").is_none());
}

#[gpui_kit::test]
fn a_border_whose_image_is_missing_draws_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with_borders(cx, &env, false);
    let _page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::WinPanel);
    let frame = PartKey::new("WinPanel/WinPanelBGBorder");
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_border_cell(&frame, 0)
    ));
}

#[gpui_kit::test]
fn a_frame_the_file_hides_stays_hidden_whatever_border_the_game_sets(cx: &mut TestAppContext) {
    let env = support::env();
    let hidden = support::hud_stock::WIN_PANEL.replace(
        "tall 185 visible 1 border TFFatLineBorderBlueBG",
        "tall 185 visible 0 border TFFatLineBorderBlueBG",
    );
    assert_ne!(hidden, support::hud_stock::WIN_PANEL);
    support::seed_stock_game_with(&env.game, &[("resource/ui/winpanel.res", Some(&hidden))]);
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::WinPanel);
    let frame = PartKey::new("WinPanel/WinPanelBGBorder");
    assert!(!present(cx, &s, "WinPanel/WinPanelBGBorder"));
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_border_cell(&frame, 0)
    ));
}

fn commit(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    field: Field,
    text: &str,
) {
    use gpui_kit::component::input::InputEvent;
    let input = cx.update(|cx| page.read(cx).field(field).clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| {
            st.set_value(text, window, cx);
            cx.emit(InputEvent::PressEnter {
                secondary: false,
                shift: false,
            });
        })
    })
    .unwrap();
    support::settle(cx, s.win);
}

/// Opens the Parts list if it is folded, then clicks the part's row.
fn click_row(cx: &mut TestAppContext, s: &support::ShellWindow, key: &str) {
    if !support::present(cx, s.win, ids::HUD_MAKER_PARTS) {
        support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_COLLAPSE);
    }
    support::click_part_row(cx, s.win, &PartKey::new(key));
}

fn xpos(cx: &mut TestAppContext, page: &Entity<HudMakerPage>, key: &str) -> Option<String> {
    cx.update(|cx| {
        let e = page.read(cx).editor()?;
        Some(
            e.model()
                .part(&PartKey::new(key))?
                .value("xpos")?
                .text()
                .to_owned(),
        )
    })
}

fn step(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    f: impl FnOnce(&mut HudMakerPage, &mut gpui_kit::Window, &mut gpui_kit::Context<HudMakerPage>),
) -> Option<Scene> {
    let scene = cx
        .update_window(s.win, |_, window, cx| {
            page.update(cx, |p, cx| {
                f(p, window, cx);
                p.scene()
            })
        })
        .unwrap();
    support::settle(cx, s.win);
    scene
}

#[gpui_kit::test]
fn a_hud_that_moves_the_winning_line_moves_it_on_the_canvas(cx: &mut TestAppContext) {
    let env = support::env();
    let moved = support::hud_stock::WIN_PANEL.replace(
        "WinningTeamLabel { ControlName CExLabel font HudFontMedium xpos 15 ypos 72",
        "WinningTeamLabel { ControlName CExLabel font HudFontMedium xpos 40 ypos 20",
    );
    assert_ne!(moved, support::hud_stock::WIN_PANEL);
    support::seed_stock_game_with(&env.game, &[("resource/ui/winpanel.res", Some(&moved))]);
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::WinPanel);
    let canvas = support::bounds_of(cx, s.win, ids::HUD_MAKER_CANVAS).unwrap();
    let line = support::bounds_of(cx, s.win, ids::hud_maker_part(&PartKey::new(LINE))).unwrap();
    // The canvas is 16:9 at scale 2.25: the root sits at the centre less 150 (c-150) and the
    // line 40 in and 20 down from it, so its box starts at x 1920/2 + (40 - 150) * 2.25.
    let fx = f32::from(line.left() - canvas.left()) / f32::from(canvas.size.width);
    let fy = f32::from(line.top() - canvas.top()) / f32::from(canvas.size.height);
    assert!(
        (fx - (960. + (40. - 150.) * 2.25) / 1920.).abs() < 0.005,
        "{fx}"
    );
    assert!((fy - (215. + 20.) * 2.25 / 1080.).abs() < 0.005, "{fy}");
}

#[gpui_kit::test]
fn edits_in_the_win_panel_and_the_freeze_cam_share_one_list_and_undo_switches_tabs(
    cx: &mut TestAppContext,
) {
    let env = support::env();
    support::seed_stock_game_with(&env.game, &[]);
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::WinPanel);
    click_row(cx, &s, LINE);
    // The X field is the screen pixel at 1920x1080: 848 is 100 game units in from the root.
    commit(cx, &s, &page, Field::X, "848");
    assert_eq!(xpos(cx, &page, LINE).as_deref(), Some("100"));
    support::click_scene(cx, s.win, Scene::FreezeCam);
    let name = "FreezePanel/FreezePanelBase/FreezeLabelKiller";
    click_row(cx, &s, name);
    commit(cx, &s, &page, Field::X, "30");
    // Undo walks the one list backwards, switching tabs to where each edit was made.
    assert_eq!(
        step(cx, &s, &page, |p, w, cx| p.undo(w, cx)),
        Some(Scene::FreezeCam)
    );
    assert_eq!(
        step(cx, &s, &page, |p, w, cx| p.undo(w, cx)),
        Some(Scene::WinPanel)
    );
    assert_eq!(xpos(cx, &page, LINE).as_deref(), Some("15"));
    assert_eq!(
        step(cx, &s, &page, |p, w, cx| p.redo(w, cx)),
        Some(Scene::WinPanel)
    );
    assert_eq!(xpos(cx, &page, LINE).as_deref(), Some("100"));
}
