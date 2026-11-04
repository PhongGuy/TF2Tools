//! Reopening made HUDs from the HUDs page.

mod support;

use std::io::Write as _;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{
    HudEntry, LibraryKind, MakeTarget, PartKey, Platform, Provenance, Scene, Screen, Tf2Install,
};

const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";

fn stock_shell(cx: &mut TestAppContext, env: &support::Env) -> support::ShellWindow {
    support::seed_stock_game(&env.game);
    support::shell_window(
        cx,
        env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    )
}

/// A made HUD whose one change moves HV 45 px right at 1080p.
fn made(s: &support::ShellWindow) -> HudEntry {
    let changes = support::stock_model()
        .move_part(&PartKey::new(HV), 45, 0, Screen::new(1920, 1080), 1)
        .unwrap();
    let files = support::hud_stock::stock_files();
    let saved = s
        .services
        .library
        .save_made_hud(&MakeTarget::New, "Made", &files, Platform::Linux, &changes)
        .unwrap();
    saved.entry().clone()
}

fn maker_json(env: &support::Env, hud: &HudEntry) -> std::path::PathBuf {
    env.data
        .library_dir(LibraryKind::Huds)
        .join(hud.folder_name())
        .join(".tf2tools/maker.json")
}

/// Leaves for Sounds and comes back, so the HUDs page reloads after `made`, then clicks Edit.
fn edit_from_huds(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    hud: &HudEntry,
) -> Entity<HudMakerPage> {
    support::click_id(cx, s.win, ids::nav(Page::Sounds));
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    support::click_id(cx, s.win, ids::hud_edit(hud.folder_name()));
    assert_eq!(cx.update(|cx| s.shell.read(cx).page()), Page::HudMaker);
    let page = cx.update(|cx| s.shell.read(cx).hud_maker_page()).unwrap();
    support::wait_real(cx, "the HUD to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

#[gpui_kit::test]
fn only_made_huds_offer_edit(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let hud = made(&s);
    let zip = env.tmp.path().join("plain.zip");
    std::fs::File::create(&zip)
        .unwrap()
        .write_all(&support::hud_zip_bytes("plainhud"))
        .unwrap();
    let plain = s
        .services
        .library
        .import_hud(&zip, Provenance::Local)
        .unwrap();
    support::click_id(cx, s.win, ids::nav(Page::Sounds));
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    assert!(support::present(
        cx,
        s.win,
        ids::hud_edit(hud.folder_name())
    ));
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_edit(plain.folder_name())
    ));
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_edit(hud.folder_name())).as_deref(),
        Some("Edit in HUD Maker")
    );
}

#[gpui_kit::test]
fn edit_reopens_the_hud_with_its_changes_as_saved(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let hud = made(&s);
    let page = edit_from_huds(cx, &s, &hud);
    let (x, name, clean, saved) = cx.update(|cx| {
        let p = page.read(cx);
        let x = p
            .editor()
            .unwrap()
            .layout()
            .get(&PartKey::new(HV))
            .unwrap()
            .x();
        (
            x,
            p.name_input().read(cx).value().to_string(),
            p.can_leave(),
            p.saved_hud().cloned(),
        )
    });
    assert_eq!((x, name.as_str(), clean), (216, "Made", true));
    assert_eq!(saved.as_ref(), Some(hud.id()));
}

#[gpui_kit::test]
fn a_damaged_maker_file_says_it_cannot_be_edited(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let hud = made(&s);
    std::fs::write(maker_json(&env, &hud), b"not json").unwrap();
    let _page = edit_from_huds(cx, &s, &hud);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_CANT_OPEN).as_deref(),
        Some("This HUD can't be opened for editing. You can still install it.")
    );
}

#[gpui_kit::test]
fn changes_that_no_longer_fit_are_counted(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let hud = made(&s);
    let json = r#"{ "version": 1, "base": { "kind": "default" }, "sources": {},
      "changes": [ { "group": 1, "set": { "file": "resource/ui/hudplayerhealth.res",
        "block": ["Resource/UI/HudPlayerHealth.res", "NoSuchBlock"],
        "key": "xpos", "cond": null, "old": null, "new": "5" } } ] }"#;
    std::fs::write(maker_json(&env, &hud), json).unwrap();
    let _page = edit_from_huds(cx, &s, &hud);
    assert_eq!(
        support::label_of(cx, s.win, ids::TOAST).as_deref(),
        Some("Some changes were left out. 1 of your changes no longer fit and were left out.")
    );
}

#[gpui_kit::test]
fn a_tf2_update_that_keeps_every_change_says_so_without_a_warning(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let hud = made(&s);
    // The game's health file changes by a comment, so every saved change still applies.
    let health = support::hud_stock::stock()
        .into_iter()
        .find(|(path, _)| path == "resource/ui/hudplayerhealth.res")
        .unwrap()
        .1;
    let updated = format!("{health}\n// updated\n");
    support::seed_stock_game_with(
        &env.game,
        &[("resource/ui/hudplayerhealth.res", Some(updated.as_str()))],
    );
    let page = edit_from_huds(cx, &s, &hud);
    assert_eq!(
        support::label_of(cx, s.win, ids::TOAST).as_deref(),
        Some("TF2 updated its HUD files. Your changes still apply. Check the HUD looks right.")
    );
    let x = cx.update(|cx| {
        let e = page.read(cx).editor().unwrap();
        e.layout().get(&PartKey::new(HV)).unwrap().x()
    });
    assert_eq!(x, 216, "the saved move still applies");
}

#[gpui_kit::test]
fn reopening_drops_the_parts_outline(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let hud = made(&s);
    support::click_id(cx, s.win, ids::nav(Page::HudMaker));
    let page = cx.update(|cx| s.shell.read(cx).hud_maker_page()).unwrap();
    support::wait_real(cx, "the HUD to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    if !support::present(cx, s.win, ids::HUD_MAKER_PARTS) {
        support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_COLLAPSE);
    }
    let row = ids::hud_maker_row(&PartKey::new(HV));
    cx.update_window(s.win, |_, w, cx| w.hover(row, cx))
        .unwrap();
    support::settle(cx, s.win);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_HOVER));
    // The pointer stays put while the made HUD replaces the default one.
    let id = hud.id().clone();
    cx.update(|cx| page.update(cx, |p, cx| p.open(id, cx)));
    support::wait_real(cx, "the made HUD to load", |cx| {
        cx.update(|cx| page.read(cx).saved_hud().is_some())
    });
    support::settle(cx, s.win);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_HOVER));
}

#[gpui_kit::test]
fn a_scoreboard_change_reopens_and_shows_on_the_scoreboard(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let blue = PartKey::new("scores/BluePlayerList");
    let changes = support::stock_model()
        .with_scene(Scene::Scoreboard)
        .set_number(&blue, "linespacing", 24, 1)
        .unwrap();
    let files = support::hud_stock::stock_files();
    let saved = s
        .services
        .library
        .save_made_hud(&MakeTarget::New, "Board", &files, Platform::Linux, &changes)
        .unwrap();
    let page = edit_from_huds(cx, &s, &saved.entry().clone());
    assert_eq!(cx.update(|cx| page.read(cx).scene()), Some(Scene::Hud));
    support::click_scene(cx, s.win, Scene::Scoreboard);
    let spacing = cx.update(|cx| {
        let e = page.read(cx).editor().unwrap();
        e.model()
            .part(&blue)?
            .value("linespacing")
            .map(|l| l.text().to_owned())
    });
    assert_eq!(spacing.as_deref(), Some("24"));
    assert!(cx.update(|cx| !page.read(cx).editor().unwrap().is_dirty()));
}

#[gpui_kit::test]
fn a_hud_that_lands_ends_match_size(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let hud = made(&s);
    support::click_id(cx, s.win, ids::nav(Page::HudMaker));
    let page = cx.update(|cx| s.shell.read(cx).hud_maker_page()).unwrap();
    support::wait_real(cx, "the HUD to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            p.editor_mut().unwrap().select(Some(PartKey::new(HV)));
            cx.notify();
        })
    });
    support::settle(cx, s.win);
    support::click_id(cx, s.win, ids::HUD_MAKER_MATCH_SIZE);
    assert!(cx.update(|cx| page.read(cx).matching_size()));
    let id = hud.id().clone();
    cx.update(|cx| page.update(cx, |p, cx| p.open(id, cx)));
    support::wait_real(cx, "the made HUD to load", |cx| {
        cx.update(|cx| page.read(cx).saved_hud().is_some())
    });
    support::settle(cx, s.win);
    assert!(!cx.update(|cx| page.read(cx).matching_size()));
}

#[gpui_kit::test]
fn a_hud_over_the_picture_caps_says_some_borrowed_pictures_were_left_out(cx: &mut TestAppContext) {
    use tf2tools_core::{AssetFile, Change, HudAssets};
    let env = support::env();
    let s = stock_shell(cx, &env);
    // 64 borrowed pictures is exactly what a HUD may hold.
    let (mut changes, mut assets) = (Vec::new(), HudAssets::new());
    for i in 0..64u32 {
        let path = format!("materials/vgui/replay/thumbnails/i{i}.vtf");
        let kept = format!(".tf2tools/borrowed/{path}");
        changes.push(Change::asset(i + 1, AssetFile::new(&path, &kept)));
        assets.insert(&kept, vec![0u8; 8]);
    }
    let saved = s
        .services
        .library
        .save_made_hud_with_assets(
            &MakeTarget::New,
            "Pictures",
            &support::hud_stock::stock_files(),
            Platform::Linux,
            &changes,
            &assets,
        )
        .unwrap();
    let hud = saved.entry().clone();
    // A hand edit lists a 65th picture with its kept copy.
    let extra = "materials/vgui/replay/thumbnails/extra.vtf";
    let dir = env
        .data
        .library_dir(LibraryKind::Huds)
        .join(hud.folder_name());
    let kept = dir.join(format!(".tf2tools/borrowed/{extra}"));
    std::fs::create_dir_all(kept.parent().unwrap()).unwrap();
    std::fs::write(&kept, b"x").unwrap();
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(maker_json(&env, &hud)).unwrap()).unwrap();
    json["changes"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "group": 99,
            "asset": { "path": extra, "source": format!(".tf2tools/borrowed/{extra}") }
        }));
    std::fs::write(maker_json(&env, &hud), json.to_string()).unwrap();
    let _page = edit_from_huds(cx, &s, &hud);
    let toast = support::label_of(cx, s.win, ids::TOAST).unwrap_or_default();
    assert!(
        toast.contains(
            "Some borrowed pictures were left out because this HUD held more than it can keep."
        ),
        "{toast}"
    );
}
