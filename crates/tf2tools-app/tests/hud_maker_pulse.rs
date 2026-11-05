//! The health pulse image drawn as the game draws it while dying, headless.

mod support;

use std::sync::Arc;

use gpui_kit::{Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{GameSituation, PartKey, RgbaImage, Tf2Install, rgba_to_vtf};

const BONUS: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthBonusImage";
const BONUS_VMT: &str = r#""UnlitGeneric" { "$basetexture" "hud/health_over_bg" }"#;

/// The stock HUD with a white, scaled pulse image and a red death warning colour.
fn opened(cx: &mut TestAppContext) -> (support::Env, support::ShellWindow, Entity<HudMakerPage>) {
    use support::hud_stock::{HEALTH, block, with_blocks};
    let env = support::env();
    let keys = [
        ("xpos", "60"),
        ("ypos", "20"),
        ("wide", "80"),
        ("tall", "80"),
        ("visible", "0"),
        ("scaleImage", "1"),
        ("image", "../hud/health_over_bg"),
    ];
    let health = with_blocks(
        HEALTH,
        &block("PlayerStatusHealthBonusImage", "ImagePanel", &keys),
    )
    .replacen(
        "    visible 1\n  }",
        "    visible 1\n    HealthDeathWarningColor \"200 30 30 255\"\n  }",
        1,
    );
    let white = RgbaImage::from_pixels(8, 8, vec![255; 8 * 8 * 4]).unwrap();
    let texture = rgba_to_vtf(&white).unwrap();
    support::seed_stock_game_with_bytes(
        &env.game,
        &[("resource/ui/hudplayerhealth.res", Some(&health))],
        &[
            ("materials/hud/health_over_bg.vmt", BONUS_VMT.as_bytes()),
            ("materials/hud/health_over_bg.vtf", &texture),
        ],
    );
    let state = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    let s = support::shell_window(cx, &env, state);
    support::click_nav(cx, s.win, Page::HudMaker);
    let page = cx
        .update(|cx| s.shell.read(cx).hud_maker_page())
        .expect("a HUD Maker page");
    support::wait_real(cx, "the HUD to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    (env, s, page)
}

fn at_health(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    health: u16,
) {
    support::maker_page::on_page(cx, s, page, |p, window, cx| {
        p.set_situation(GameSituation::default().with_health(health), window, cx);
    });
}

// The drawn texture's first texel, as RGBA.
fn first_texel(image: Option<Arc<gpui_kit::RenderImage>>) -> [u8; 4] {
    let image = image.expect("a drawn pulse image");
    let bgra = &image.as_bytes(0).unwrap()[..4];
    [bgra[2], bgra[1], bgra[0], bgra[3]]
}

#[gpui_kit::test]
fn the_dying_pulse_draws_in_the_warning_colour_at_the_game_s_size(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    let key = PartKey::new(BONUS);
    // At 97 of a Soldier's 200 health the pulse is just under the line, so it keeps its size.
    at_health(cx, &s, &page, 97);
    let edge = support::bounds_of(cx, s.win, ids::hud_maker_image_cell(&key, 0))
        .expect("the pulse image drawn at 97 health");
    at_health(cx, &s, &page, 30);
    let grown = support::bounds_of(cx, s.win, ids::hud_maker_image_cell(&key, 0))
        .expect("the pulse image drawn at 30 health");
    let drawn = cx.update(|cx| page.read(cx).drawn_image(&key));
    assert_eq!(first_texel(drawn), [200, 30, 30, 255]);
    // 80 units are 180 px on a 1080 tall screen; at 30 health the game grows 17 px a side.
    let ratio = f32::from(grown.size.width) / f32::from(edge.size.width);
    assert!((ratio - 214. / 180.).abs() < 0.01, "ratio {ratio}");
}

#[gpui_kit::test]
fn the_overheal_pulse_draws_its_texture_untinted(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    at_health(cx, &s, &page, 260);
    let drawn = cx.update(|cx| page.read(cx).drawn_image(&PartKey::new(BONUS)));
    assert_eq!(first_texel(drawn), [255, 255, 255, 255]);
}
