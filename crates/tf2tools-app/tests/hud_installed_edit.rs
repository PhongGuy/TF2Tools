//! Edit on the installed HUD: a made HUD or copy opens in the HUD Maker, any other HUD is
//! copied first so the original stays as it is.

use crate::support;

use std::path::Path;

use gpui_kit::TestAppContext;
use tf2tools_app::ids;
use tf2tools_app::services::{Services, Tf2State};
use tf2tools_app::shell::route::Page;
use tf2tools_core::{GbId, HudEntry, HudId, MakeTarget, Platform, Provenance, Tf2Install};

fn tf2(env: &support::Env) -> Tf2Install {
    Tf2Install::from_path(&env.game).unwrap()
}

fn stock_shell(cx: &mut TestAppContext, env: &support::Env) -> support::ShellWindow {
    support::seed_stock_game(&env.game);
    support::shell_window(cx, env, Tf2State::Found(tf2(env)))
}

fn seed(services: &Services, dir: &Path, root: &str, provenance: Provenance) -> HudEntry {
    support::hud_copies::import_stock_hud(services, dir, root, &[], provenance)
}

fn store() -> Provenance {
    Provenance::GameBanana {
        gb_id: GbId::new(7),
        author: "Ray".into(),
    }
}

fn revisit_huds(cx: &mut TestAppContext, s: &support::ShellWindow) {
    support::click_id(cx, s.win, ids::nav(Page::Sounds));
    support::click_id(cx, s.win, ids::nav(Page::Huds));
}

// Waits for the HUD Maker to show with a HUD loaded and returns the HUD it opened.
fn opened(cx: &mut TestAppContext, s: &support::ShellWindow) -> Option<HudId> {
    support::wait_real(cx, "the HUD Maker to open", |cx| {
        cx.update(|cx| {
            let shell = s.shell.read(cx);
            shell.page() == Page::HudMaker
                && shell
                    .hud_maker_page()
                    .is_some_and(|p| p.read(cx).is_loaded())
        })
    });
    let page = cx.update(|cx| s.shell.read(cx).hud_maker_page()).unwrap();
    cx.update(|cx| page.read(cx).saved_hud().cloned())
}

#[gpui_kit::test]
fn edit_on_an_installed_made_hud_opens_it(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let files = support::hud_stock::stock_files();
    let made = s
        .services
        .library
        .save_made_hud(&MakeTarget::New, "Made", &files, Platform::Linux, &[])
        .unwrap();
    let id = made.entry().id().clone();
    s.services.library.install_hud(&id, &tf2(&env)).unwrap();
    revisit_huds(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUDS_INSTALLED_EDIT).as_deref(),
        Some("Edit")
    );
    support::click_id(cx, s.win, ids::HUDS_INSTALLED_EDIT);
    assert_eq!(opened(cx, &s), Some(id));
    assert_eq!(s.services.library.list_huds().unwrap().len(), 1);
}

#[gpui_kit::test]
fn edit_on_an_installed_store_hud_opens_a_copy(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let base = seed(&s.services, env.tmp.path(), "rayshud", store());
    s.services
        .library
        .install_hud(base.id(), &tf2(&env))
        .unwrap();
    revisit_huds(cx, &s);
    support::click_id(cx, s.win, ids::HUDS_INSTALLED_EDIT);
    let open = opened(cx, &s);
    let huds = s.services.library.list_huds().unwrap();
    assert_eq!(huds.len(), 2);
    let copy = (huds.iter())
        .find(|h| h.id() != base.id())
        .expect("the copy's row");
    assert_eq!(open.as_ref(), Some(copy.id()));
    assert!(!s.services.library.is_made_hud(base.id()).unwrap());
    let name = base.name();
    let toast = format!("Editing a copy of {name}. The original stays as it is.");
    assert!(
        support::toast_labels(cx, s.win)
            .iter()
            .any(|t| t.contains(&toast)),
        "{:?}",
        support::toast_labels(cx, s.win)
    );
}

#[gpui_kit::test]
fn a_failed_copy_stays_on_huds(cx: &mut TestAppContext) {
    let env = support::env();
    support::seed_stock_game(&env.game);
    let state = Tf2State::Found(tf2(&env));
    let s = support::shell_window_tweaked(cx, &env, state, |services| {
        services.library.set_free_space_probe(|_| Ok(0))
    });
    let base = seed(&s.services, env.tmp.path(), "rayshud", store());
    s.services
        .library
        .install_hud(base.id(), &tf2(&env))
        .unwrap();
    revisit_huds(cx, &s);
    support::click_id(cx, s.win, ids::HUDS_INSTALLED_EDIT);
    support::wait_real(cx, "the refusal", |cx| {
        support::toast_labels(cx, s.win)
            .iter()
            .any(|t| t.contains("There isn't enough free disk space to copy this HUD."))
    });
    assert_eq!(s.services.library.list_huds().unwrap().len(), 1);
    assert_eq!(cx.update(|cx| s.shell.read(cx).page()), Page::Huds);
}

#[gpui_kit::test]
fn every_library_card_offers_an_edit(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let local = seed(&s.services, env.tmp.path(), "budhud", Provenance::Local);
    let shop = seed(&s.services, env.tmp.path(), "rayshud", store());
    let copy = s.services.library.copy_hud(local.id()).unwrap();
    revisit_huds(cx, &s);
    for hud in [&local, &shop] {
        assert!(support::present(
            cx,
            s.win,
            ids::hud_copy(hud.folder_name())
        ));
        assert!(!support::present(
            cx,
            s.win,
            ids::hud_edit(hud.folder_name())
        ));
    }
    assert!(support::present(
        cx,
        s.win,
        ids::hud_edit(copy.folder_name())
    ));
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_copy(copy.folder_name())
    ));
    support::click_id(cx, s.win, ids::hud_edit(copy.folder_name()));
    assert_eq!(opened(cx, &s), Some(copy.id().clone()));
}
