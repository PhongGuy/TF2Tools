//! The class select canvas: the code's pictures, the backdrop room, the model area and the tips box.

use crate::support;

use gpui_kit::{Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_app::services::Tf2State;
use tf2tools_core::{PartKey, Scene, Tf2Install};

const CLASSES: [&str; 9] = [
    "scout", "soldier", "pyro", "demo", "heavy", "engineer", "medic", "sniper", "spy",
];

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

/// A solid 8x8 texture under `materials/<dir>/`, as a material and a texture file.
fn texture(dir: &str, name: &str, rgba: [u8; 4]) -> [(String, Vec<u8>); 2] {
    let image = tf2tools_core::RgbaImage::from_pixels(8, 8, rgba.repeat(64)).unwrap();
    let vmt = format!(r#""UnlitGeneric" {{ "$baseTexture" "{dir}/{name}" }}"#);
    [
        (format!("materials/{dir}/{name}.vmt"), vmt.into_bytes()),
        (
            format!("materials/{dir}/{name}.vtf"),
            tf2tools_core::rgba_to_vtf(&image).unwrap(),
        ),
    ]
}

/// The stock game with every class picture: each team colour differs by class, the idle ones are grey.
fn textures() -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    for (i, class) in CLASSES.iter().enumerate() {
        let shade = 40 + 20 * i as u8;
        out.extend(texture(
            "vgui",
            &format!("class_sel_sm_{class}_red"),
            [shade, 30, 30, 255],
        ));
        out.extend(texture(
            "vgui",
            &format!("class_sel_sm_{class}_inactive"),
            [90, 90, 90, 255],
        ));
    }
    out.extend(texture("hud", "class_soldierred", [200, 180, 150, 255]));
    out
}

fn shell_with_textures(
    cx: &mut TestAppContext,
    env: &support::Env,
    overrides: &[(&str, Option<&str>)],
) -> support::ShellWindow {
    let files = textures();
    let extra: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(p, b)| (p.as_str(), b.as_slice()))
        .collect();
    support::seed_stock_game_with_bytes(&env.game, overrides, &extra);
    support::shell_window(cx, env, found(env))
}

fn pixels(cx: &mut TestAppContext, page: &Entity<HudMakerPage>, key: &str) -> Option<Vec<u8>> {
    cx.update(|cx| page.read(cx).image(&PartKey::new(key)))
        .and_then(|image| image.as_bytes(0).map(<[u8]>::to_vec))
}

#[gpui_kit::test]
fn the_selected_button_and_the_teammate_row_draw_the_game_s_pictures(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with_textures(cx, &env, &[]);
    let page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::ClassSelect);
    let get = |cx: &mut TestAppContext, key: &str| {
        pixels(cx, &page, key).unwrap_or_else(|| panic!("{key}"))
    };
    let soldier = get(cx, "class/soldier/SubImage");
    // The Soldier button, the local player's picture and both teammate Soldiers share one picture.
    for key in [
        "class/localPlayerImage",
        "class/countImage1",
        "class/countImage2",
    ] {
        assert_eq!(get(cx, key), soldier, "{key}");
    }
    // The scout teammate is another picture; the idle buttons are the grey ones, all alike.
    assert_ne!(get(cx, "class/countImage0"), soldier);
    assert_ne!(get(cx, "class/countImage3"), get(cx, "class/countImage0"));
    let idle = get(cx, "class/scout/SubImage");
    assert_ne!(idle, soldier);
    assert_eq!(get(cx, "class/pyro/SubImage"), idle);
    // Eleven teammates: the last count picture draws, as its file `visible 0` is overridden.
    let last = PartKey::new("class/countImage10");
    assert!(support::present(
        cx,
        s.win,
        ids::hud_maker_image_cell(&last, 0)
    ));
}

#[gpui_kit::test]
fn the_counts_the_game_fills_show_on_the_labels(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with_textures(cx, &env, &[]);
    let _page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::ClassSelect);
    for (key, text) in [
        ("numSoldier", "3"),
        ("numScout", "1"),
        ("numMedic", "2"),
        ("numSpy", "1"),
    ] {
        let got = support::label_of(
            cx,
            s.win,
            ids::hud_maker_part(&PartKey::new(format!("class/{key}"))),
        );
        assert_eq!(got.as_deref(), Some(text), "{key}");
    }
}

#[gpui_kit::test]
fn the_cancel_button_hides_its_hotkey_marker(cx: &mut TestAppContext) {
    let env = support::env();
    let english = br#""lang" { "Language" "English" "Tokens" {
      "TF_ClassMenu_Cancel" "CANCEL (&0)" } }"#;
    support::seed_stock_game_with_bytes(
        &env.game,
        &[],
        &[("resource/tf_english.txt", english.as_slice())],
    );
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::ClassSelect);
    let got = support::label_of(
        cx,
        s.win,
        ids::hud_maker_part(&PartKey::new("class/CancelButton")),
    );
    assert_eq!(got.as_deref(), Some("CANCEL (0)"));
}

#[gpui_kit::test]
fn the_backdrop_room_is_a_wall_under_a_top_bar_unless_the_hud_hides_it(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with_textures(cx, &env, &[]);
    let _page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::ClassSelect);
    let bg = PartKey::new("class/MenuBG");
    assert!(support::present(cx, s.win, ids::hud_maker_fill(&bg)));
    // The bar spans the room's top 111 of its 480 rows, as the game's room model draws it.
    let room = support::bounds_of(cx, s.win, ids::hud_maker_part(&bg)).unwrap();
    let bar = support::bounds_of(cx, s.win, ids::hud_maker_fill_band(&bg)).unwrap();
    let tall = f32::from(room.size.height) * 111. / 480.;
    assert_eq!(bar.top(), room.top());
    assert_eq!(bar.size.width, room.size.width);
    assert!(
        (f32::from(bar.size.height) - tall).abs() <= 1.,
        "{bar:?} in {room:?}"
    );
    let hidden = support::hud_stock::class_select().replace(
        "MenuBG { ControlName CModelPanel xpos 0 ypos 0 zpos 0 wide f0 tall 480 visible 1",
        "MenuBG { ControlName CModelPanel xpos 0 ypos 0 zpos 0 wide f0 tall 480 visible 0",
    );
    assert_ne!(hidden, support::hud_stock::class_select());
    let env = support::env();
    let s = shell_with_textures(
        cx,
        &env,
        &[("resource/ui/classselection.res", Some(&hidden))],
    );
    let _page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::ClassSelect);
    assert!(!support::present(cx, s.win, ids::hud_maker_fill(&bg)));
    assert!(!support::present(cx, s.win, ids::hud_maker_fill_band(&bg)));
}

// The model's top and bottom as fractions of its box under the stock camera: fov 25 at 320
// units, the feet 49 below the eye line, an 82-unit class.
fn framed(part: gpui_kit::Bounds<gpui_kit::Pixels>, feet: f32) -> (f32, f32) {
    let span = 2. * 320. * (12.5f32).to_radians().tan();
    let at = |z: f32| f32::from(part.top()) + (0.5 - z / span) * f32::from(part.size.height);
    (at(feet + 82.), at(feet))
}

#[gpui_kit::test]
fn the_class_model_stands_where_the_camera_frames_it_below_the_buttons(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with_textures(cx, &env, &[]);
    let _page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::ClassSelect);
    let key = PartKey::new("class/TFPlayerModel");
    let part = support::bounds_of(cx, s.win, ids::hud_maker_part(&key)).unwrap();
    let portrait = support::bounds_of(cx, s.win, ids::hud_maker_portrait(&key)).unwrap();
    let (top, foot) = framed(part, -49.);
    assert!(
        (f32::from(portrait.top()) - top).abs() <= 1.,
        "{portrait:?} top {top}"
    );
    assert!(
        (f32::from(portrait.bottom()) - foot).abs() <= 1.,
        "{portrait:?} foot {foot}"
    );
    // The figure clears the class buttons, so none of them is drawn over, and the tips box,
    // higher in z, does not squeeze it.
    for button in ["scout", "soldier", "random"] {
        let b = support::bounds_of(
            cx,
            s.win,
            ids::hud_maker_part(&PartKey::new(format!("class/{button}"))),
        )
        .unwrap();
        assert!(portrait.top() >= b.bottom(), "{button} {b:?} {portrait:?}");
    }
}

#[gpui_kit::test]
fn the_class_model_follows_the_camera_the_file_sets(cx: &mut TestAppContext) {
    let moved = support::hud_stock::class_select().replace(
        "TFPlayerModel { ControlName CTFPlayerModelPanel xpos 0 ypos 0 zpos 6 wide 480 tall 480 visible 1",
        "TFPlayerModel { ControlName CTFPlayerModelPanel xpos 0 ypos 0 zpos 6 wide 480 tall 480 visible 1 fov 25\n    model { force_pos 1 origin_x 320 origin_y 0 origin_z -20 }",
    );
    assert_ne!(moved, support::hud_stock::class_select());
    let env = support::env();
    let s = shell_with_textures(
        cx,
        &env,
        &[("resource/ui/classselection.res", Some(&moved))],
    );
    let _page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::ClassSelect);
    let key = PartKey::new("class/TFPlayerModel");
    let part = support::bounds_of(cx, s.win, ids::hud_maker_part(&key)).unwrap();
    let portrait = support::bounds_of(cx, s.win, ids::hud_maker_portrait(&key)).unwrap();
    let (top, foot) = framed(part, -20.);
    assert!(
        (f32::from(portrait.top()) - top).abs() <= 1.,
        "{portrait:?} top {top}"
    );
    assert!(
        (f32::from(portrait.bottom()) - foot).abs() <= 1.,
        "{portrait:?} foot {foot}"
    );
    // origin_y 0 puts the model on the camera's axis: centred across the box.
    let mid =
        |b: gpui_kit::Bounds<gpui_kit::Pixels>| f32::from(b.left()) + f32::from(b.size.width) / 2.;
    assert!((mid(portrait) - mid(part)).abs() <= 1.);
}

#[gpui_kit::test]
fn the_tips_box_is_outlined_as_drawn_in_game(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with_textures(cx, &env, &[]);
    let _page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::ClassSelect);
    let tips = ids::hud_maker_part(&PartKey::new("class/ClassTipsPanel"));
    assert_eq!(
        support::label_of(cx, s.win, tips).as_deref(),
        Some("Class tips draw in game")
    );
}

#[gpui_kit::test]
fn the_team_count_line_shows_whole_as_the_game_sizes_it_to_its_text(cx: &mut TestAppContext) {
    let lang = r#""lang" { "Language" "English" "Tokens" { "TF_TeamCount" "Other players on your team" } }"#;
    let env = support::env();
    let s = shell_with_textures(cx, &env, &[("resource/tf_english.txt", Some(lang))]);
    let _page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::ClassSelect);
    let key = PartKey::new("class/CountLabel");
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_text(&key)).as_deref(),
        Some("Other players on your team")
    );
    // The file's 150-unit box is narrower than the line; the game's code widens the label to
    // its text, so nothing past the box is cut.
    let part = support::bounds_of(cx, s.win, ids::hud_maker_part(&key)).unwrap();
    let shown = support::bounds_of(cx, s.win, ids::hud_maker_text_clip(&key)).unwrap();
    assert!(shown.right() > part.right(), "{shown:?} cut at {part:?}");
}
