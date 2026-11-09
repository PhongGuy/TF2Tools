//! Every button says what it does: an icon-only button carries an accessible name, so a
//! screen reader and the pilot can read it, and a tooltip a player sees on hover.
use crate::support;

use std::time::Duration;

use gpui_kit::{AnyWindowHandle, AppContext as _, Role, TestAppContext};
use tf2tools_app::audio::player::NowPlaying;
use tf2tools_app::ids;
use tf2tools_app::pages::hud_store::bodies::gamebanana_body;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_app::store_detail::{DetailItem, open_detail};
use tf2tools_core::{
    DraftSlot, GbId, Preset, ProfileDraft, Provenance, StoreItemId, StoreKind, Tf2Install,
};

// A short silent stereo WAV the library imports as a sound.
fn wav() -> Vec<u8> {
    let (rate, frames) = (44_100u32, 441u32);
    let data_len = frames * 4;
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.resize(out.len() + data_len as usize, 0);
    out
}

// The path of every button drawn in `win` that has no name to announce, labelled with `where_`.
fn unnamed(cx: &mut TestAppContext, win: AnyWindowHandle, where_: &str) -> Vec<String> {
    support::settle(cx, win);
    cx.update_window(win, |_, w, _| {
        gpui_kit::base::test_support::snapshots(w)
            .iter()
            .filter(|e| e.role() == Some(Role::Button))
            .filter(|e| e.label().is_none_or(|l| l.trim().is_empty()))
            .map(|e| format!("{where_}: {:?}", e.path().last()))
            .collect()
    })
    .unwrap()
}

// Opens `page` from the sidebar and waits for whatever first load it runs.
fn visit(cx: &mut TestAppContext, s: &support::ShellWindow, page: Page) {
    support::click_id(cx, s.win, ids::nav(page));
    support::wait_real(cx, page.slug(), |cx| {
        cx.update(|cx| {
            let shell = s.shell.read(cx);
            match page {
                Page::Huds => shell.huds_page().is_some_and(|p| p.read(cx).is_loaded()),
                Page::Sounds => shell.sounds_page().is_some_and(|p| p.read(cx).is_loaded()),
                Page::Crosshairs => shell
                    .crosshairs_page()
                    .is_some_and(|p| p.read(cx).is_loaded()),
                Page::Profiles => shell
                    .profiles_page()
                    .is_some_and(|p| p.read(cx).is_loaded()),
                Page::Addons => shell.addons_page().is_some_and(|p| p.read(cx).is_loaded()),
                Page::AddonStore => shell
                    .addon_store_page()
                    .is_some_and(|p| p.read(cx).is_loaded()),
                Page::Settings => shell
                    .settings_page()
                    .is_some_and(|p| p.read(cx).is_loaded()),
                Page::Logs => shell.logs_page().is_some_and(|p| p.read(cx).is_loaded()),
                Page::Graphics => shell
                    .graphics_page()
                    .is_some_and(|p| p.read(cx).is_loaded()),
                Page::Maps => shell.maps_page().is_some_and(|p| p.read(cx).is_loaded()),
                Page::CrosshairMaker => shell
                    .crosshair_maker_page()
                    .is_some_and(|p| p.read(cx).is_loaded()),
                Page::HudMaker => shell
                    .hud_maker_page()
                    .is_some_and(|p| p.read(cx).is_loaded()),
                _ => true,
            }
        })
    });
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn every_button_on_every_page_has_a_name(cx: &mut TestAppContext) {
    let env = support::env();
    let found = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    let s = support::shell_window(cx, &env, found);
    // One of everything, so each page draws its cards and their buttons.
    let archive = env.tmp.path().join("budhud.zip");
    std::fs::write(&archive, support::hud_zip_bytes("budhud")).unwrap();
    let hud = s
        .services
        .library
        .import_hud(&archive, Provenance::Local)
        .unwrap();
    let sound_path = env.tmp.path().join("Ding.wav");
    std::fs::write(&sound_path, wav()).unwrap();
    s.services
        .library
        .import_sound(&sound_path, "Ding", Provenance::Local)
        .unwrap();
    support::seed_crosshair(&s.services, &env, "dot");
    let vpk = support::vpk_bytes(&[("cfg/addons/x.cfg", b"x".as_slice())]);
    let foot = "mastercomfig-addon-no-footsteps.vpk";
    let pyro = "mastercomfig-addon-no-pyroland.vpk";
    support::seed_library_addon(&s.services, &env, foot, "9.1.0", &vpk, true);
    support::seed_library_addon(&s.services, &env, pyro, "9.0.0", &vpk, false);
    s.services
        .library
        .create_profile(&ProfileDraft::new("Comp").with_hud(DraftSlot::Item(hud.id().clone())))
        .unwrap();
    support::seed_gb_huds(&env, &[(7, "FlawHUD")]);
    support::seed_sounds(&env, &[(1, "Ding", "hitsound", None)]);
    support::seed_crosshairs(&env, [3], 0, false);
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_mastercomfig_catalog(&env, "9.100.1");
    support::seed_mastercomfig_choices(&env, r#"{"fpscap":"120"}"#);
    support::seed_mastercomfig_release(&env, "http://127.0.0.1:9", "9.101.0");

    let mut missing = Vec::new();
    // HUDs, the start page, loaded before the seeding, so it comes round again at the end.
    for page in Page::ALL.into_iter().chain([Page::Huds]) {
        visit(cx, &s, page);
        missing.extend(unnamed(cx, s.win, page.slug()));
    }
    // Graphics in each card state, so every one of its buttons is drawn: installed (Update,
    // Remove, the preset cards), a hand install (Take over), then none (Install).
    visit(cx, &s, Page::Graphics);
    for id in [ids::GRAPHICS_UPDATE, ids::GRAPHICS_REMOVE] {
        assert!(support::present(cx, s.win, id), "{id}");
    }
    assert!(support::present(cx, s.win, ids::GRAPHICS_RESET_ALL));
    assert!(support::present(
        cx,
        s.win,
        ids::graphics_module_reset("fpscap")
    ));
    assert!(support::present(
        cx,
        s.win,
        ids::graphics_preset(Preset::Medium)
    ));
    missing.extend(unnamed(cx, s.win, "graphics-installed"));
    visit(cx, &s, Page::Huds);
    rusqlite::Connection::open(env.data.db_path())
        .unwrap()
        .execute("DELETE FROM mastercomfig_state", [])
        .unwrap();
    visit(cx, &s, Page::Graphics);
    assert!(support::present(cx, s.win, ids::GRAPHICS_TAKE_OVER));
    missing.extend(unnamed(cx, s.win, "graphics-outside"));
    visit(cx, &s, Page::Huds);
    std::fs::remove_file(env.game.join("tf/custom/mastercomfig-base.vpk")).unwrap();
    visit(cx, &s, Page::Graphics);
    assert!(support::present(cx, s.win, ids::GRAPHICS_INSTALL));
    missing.extend(unnamed(cx, s.win, "graphics-not-installed"));
    assert!(missing.is_empty(), "buttons without a name: {missing:#?}");
}

#[gpui_kit::test]
fn the_playing_sound_player_names_its_buttons(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    visit(cx, &s, Page::Sounds);
    let player = cx
        .update(|cx| s.shell.read(cx).sounds_page())
        .map(|p| cx.update(|cx| p.read(cx).player().clone()))
        .expect("the Sounds page");
    support::wait_real(cx, "the player", |cx| {
        cx.update(|cx| player.read(cx).is_ready())
    });
    let now = NowPlaying {
        key: "ding".into(),
        name: "Ding".into(),
        subtitle: "by vex".into(),
    };
    cx.update(|cx| player.update(cx, |p, cx| p.toggle(now, |_| Ok(vec![1]), cx).detach()));
    support::wait_real(cx, "the play", |_| !s.audio.plays().is_empty());
    // The floating player shows on every page while it plays.
    visit(cx, &s, Page::Huds);
    assert!(support::present(cx, s.win, ids::SOUND_PLAYER_STOP));
    let missing = unnamed(cx, s.win, "player");
    assert!(missing.is_empty(), "buttons without a name: {missing:#?}");
}

#[gpui_kit::test]
fn the_store_detail_names_its_buttons(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_gb_files(
        7,
        &[("flawhud.zip", support::hud_zip_bytes("flawhud"))],
        Duration::ZERO,
    );
    let (win, _) = support::store_window(
        cx,
        &env,
        &server,
        |env| support::seed_gb_huds(env, &[(7, "FlawHUD")]),
        |_, _| support::Blank,
    );
    let item = DetailItem {
        kind: StoreKind::GameBananaHuds,
        id: StoreItemId::GameBanana(GbId::new(7)),
        name: "FlawHUD".into(),
        in_library: false,
    };
    cx.update_window(win, |_, window, cx| {
        open_detail(item, gamebanana_body(), window, cx);
    })
    .unwrap();
    support::wait_real(cx, "the files", |cx| {
        support::present(cx, win, ids::store_file(0))
    });
    assert!(
        support::present(cx, win, ids::CAROUSEL_NEXT),
        "the gallery's arrows"
    );
    let missing = unnamed(cx, win, "store detail");
    assert!(missing.is_empty(), "buttons without a name: {missing:#?}");
}
