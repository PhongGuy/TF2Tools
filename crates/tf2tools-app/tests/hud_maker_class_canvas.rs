//! The class select canvas: the code's pictures, the backdrop room, the model area and the tips box.

mod support;

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
fn the_backdrop_room_is_a_dark_fill_unless_the_hud_hides_it(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with_textures(cx, &env, &[]);
    let _page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::ClassSelect);
    let bg = PartKey::new("class/MenuBG");
    assert!(support::present(cx, s.win, ids::hud_maker_fill(&bg)));
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
}

#[gpui_kit::test]
fn the_class_model_stands_at_its_full_box_not_squeezed_under_the_tips(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell_with_textures(cx, &env, &[]);
    let _page = open_maker(cx, &s);
    support::click_scene(cx, s.win, Scene::ClassSelect);
    let key = PartKey::new("class/TFPlayerModel");
    let part = support::bounds_of(cx, s.win, ids::hud_maker_part(&key)).unwrap();
    let portrait = support::bounds_of(cx, s.win, ids::hud_maker_portrait(&key)).unwrap();
    // The tips box overlaps the model's columns and sits higher in z; the scoreboard would stop
    // the portrait short of it, but the class menu's camera is fixed.
    assert!((f32::from(portrait.size.width) - f32::from(part.size.width)).abs() <= 0.5);
    assert!((f32::from(portrait.bottom()) - f32::from(part.bottom())).abs() <= 0.5);
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
