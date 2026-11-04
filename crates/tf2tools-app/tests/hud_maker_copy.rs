//! The HUD Maker edits a copy over its own files and saves it with kept originals.

mod support;

use std::path::Path;

use gpui_kit::{AppContext as _, Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_app::services::{Services, Tf2State};
use tf2tools_app::shell::route::Page;
use tf2tools_core::{HudEntry, LibraryKind, PartKey, Provenance, Tf2Install};

const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";
const HEALTH: &str = "resource/ui/hudplayerhealth.res";

fn stock_shell(cx: &mut TestAppContext, env: &support::Env) -> support::ShellWindow {
    support::seed_stock_game(&env.game);
    support::shell_window(
        cx,
        env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    )
}

fn seed_copy(services: &Services, dir: &Path) -> (HudEntry, HudEntry) {
    let health = support::hud_stock::HEALTH.replace("TanDark", "TanLight");
    let extra = [
        ("resource/ui/hudplayerhealth.res", health.as_str()),
        ("README.txt", "hi"),
    ];
    let base =
        support::hud_copies::import_stock_hud(services, dir, "somehud", &extra, Provenance::Local);
    let copy = services.library.copy_hud(base.id()).unwrap();
    (base, copy)
}

fn edit_from_huds(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    hud: &HudEntry,
) -> Entity<HudMakerPage> {
    support::click_id(cx, s.win, ids::nav(Page::Sounds));
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    support::click_id(cx, s.win, ids::hud_edit(hud.folder_name()));
    let page = cx.update(|cx| s.shell.read(cx).hud_maker_page()).unwrap();
    support::wait_real(cx, "the HUD to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

fn recolour(cx: &mut TestAppContext, page: &Entity<HudMakerPage>, colour: &str) {
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            let e = p.editor_mut().unwrap();
            e.select(Some(PartKey::new(HV)));
            e.recolour_selected(colour).unwrap();
            cx.notify();
        })
    });
}

fn set_name(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    text: &str,
) {
    let input = cx.update(|cx| page.read(cx).name_input().clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.set_value(text, window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
}

fn save(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    button: &'static str,
) {
    if button == ids::HUD_MAKER_SAVE_AS_NEW {
        support::click_id(cx, s.win, ids::HUD_MAKER_MORE);
    }
    support::click_id(cx, s.win, button);
    support::wait_real(cx, "the save", |cx| {
        cx.update(|cx| !page.read(cx).is_saving())
    });
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn a_copy_opens_with_its_own_look(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let (_, copy) = seed_copy(&s.services, env.tmp.path());
    let page = edit_from_huds(cx, &s, &copy);
    let colour = cx.update(|cx| {
        let editor = page.read(cx).editor().unwrap();
        editor
            .model()
            .part(&PartKey::new(HV))
            .unwrap()
            .value("fgcolor")
            .unwrap()
            .text()
            .to_owned()
    });
    assert_eq!(colour, "TanLight");
}

#[gpui_kit::test]
fn a_copy_saves_twice_and_keeps_the_base_bytes_as_its_original(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let (_, copy) = seed_copy(&s.services, env.tmp.path());
    let page = edit_from_huds(cx, &s, &copy);
    for colour in ["255 0 0 255", "0 0 255 255"] {
        recolour(cx, &page, colour);
        save(cx, &s, &page, ids::HUD_MAKER_SAVE);
    }
    let dir = env
        .data
        .library_dir(LibraryKind::Huds)
        .join(copy.folder_name());
    let health = std::fs::read_to_string(dir.join(HEALTH)).unwrap();
    assert!(health.contains("0 0 255 255") && !health.contains("255 0 0 255"));
    let original = std::fs::read_to_string(dir.join(".tf2tools/original").join(HEALTH)).unwrap();
    assert_eq!(
        original,
        support::hud_stock::HEALTH.replace("TanDark", "TanLight")
    );
}

#[gpui_kit::test]
fn a_hotkey_marker_hidden_on_the_canvas_stays_in_the_saved_file(cx: &mut TestAppContext) {
    const MARKED: &str = "&Salt && Pepper";
    let env = support::env();
    let s = stock_shell(cx, &env);
    let health = (support::hud_stock::HEALTH).replace("%Health%", &format!("\"{MARKED}\""));
    assert!(health.contains(MARKED));
    let extra = [(HEALTH, health.as_str())];
    let base = support::hud_copies::import_stock_hud(
        &s.services,
        env.tmp.path(),
        "somehud",
        &extra,
        Provenance::Local,
    );
    let copy = s.services.library.copy_hud(base.id()).unwrap();
    let page = edit_from_huds(cx, &s, &copy);
    let part = ids::hud_maker_part(&PartKey::new(HV));
    assert_eq!(
        support::label_of(cx, s.win, part).as_deref(),
        Some("Salt & Pepper")
    );
    recolour(cx, &page, "255 0 0 255");
    save(cx, &s, &page, ids::HUD_MAKER_SAVE);
    let saved = env
        .data
        .library_dir(LibraryKind::Huds)
        .join(copy.folder_name())
        .join(HEALTH);
    assert!(std::fs::read_to_string(saved).unwrap().contains(MARKED));
}

#[gpui_kit::test]
fn save_as_new_from_a_copy_keeps_every_base_file(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let (base, copy) = seed_copy(&s.services, env.tmp.path());
    let page = edit_from_huds(cx, &s, &copy);
    recolour(cx, &page, "255 0 0 255");
    set_name(cx, &s, &page, "Mine 2");
    save(cx, &s, &page, ids::HUD_MAKER_SAVE_AS_NEW);
    let mine = s
        .services
        .library
        .list_huds()
        .unwrap()
        .into_iter()
        .find(|h| h.name() == "Mine 2")
        .expect("the new HUD");
    let dir = env
        .data
        .library_dir(LibraryKind::Huds)
        .join(mine.folder_name());
    assert_eq!(
        std::fs::read_to_string(dir.join("README.txt")).unwrap(),
        "hi"
    );
    let record = s
        .services
        .library
        .copy_base(mine.id())
        .unwrap()
        .expect("still a copy");
    assert_eq!(record.hud_id(), base.id().as_str());
    // The page now edits the new HUD over its own files, so a second save lands there cleanly.
    assert_eq!(
        cx.update(|cx| page.read(cx).saved_hud().cloned()),
        Some(mine.id().clone())
    );
    recolour(cx, &page, "0 0 255 255");
    save(cx, &s, &page, ids::HUD_MAKER_SAVE);
    let health = std::fs::read_to_string(dir.join(HEALTH)).unwrap();
    assert!(health.contains("0 0 255 255") && !health.contains("255 0 0 255"));
    let original = std::fs::read_to_string(dir.join(".tf2tools/original").join(HEALTH)).unwrap();
    assert_eq!(
        original,
        support::hud_stock::HEALTH.replace("TanDark", "TanLight")
    );
}

#[gpui_kit::test]
fn an_undone_move_saved_again_puts_the_part_back(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let (_, copy) = seed_copy(&s.services, env.tmp.path());
    let page = edit_from_huds(cx, &s, &copy);
    recolour(cx, &page, "255 0 0 255");
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            p.editor_mut().unwrap().nudge_selected(10, 10).unwrap();
            cx.notify();
        })
    });
    save(cx, &s, &page, ids::HUD_MAKER_SAVE);
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            assert!(p.editor_mut().unwrap().undo());
            cx.notify();
        })
    });
    save(cx, &s, &page, ids::HUD_MAKER_SAVE);
    let dir = env
        .data
        .library_dir(LibraryKind::Huds)
        .join(copy.folder_name());
    let health = std::fs::read_to_string(dir.join(HEALTH)).unwrap();
    let original = std::fs::read_to_string(dir.join(".tf2tools/original").join(HEALTH)).unwrap();
    // The part's own block, from its name to the next closing brace.
    let block = |text: &str| {
        let at = text.find("PlayerStatusHealthValue").unwrap();
        let end = at + text[at..].find('}').unwrap();
        let lines = text[at..end].lines().map(str::trim);
        lines
            .filter(|l| l.starts_with("xpos") || l.starts_with("ypos"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    assert_eq!(block(&health), block(&original), "{health}");
    assert!(health.contains("255 0 0 255"));
}
