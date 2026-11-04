//! Edit a copy: a library HUD's card makes an editable copy and opens it in the HUD Maker.

mod support;

use std::path::Path;

use gpui_kit::TestAppContext;
use tf2tools_app::ids;
use tf2tools_app::services::{Services, Tf2State};
use tf2tools_app::shell::route::Page;
use tf2tools_core::{HudEntry, LibraryKind, MakeTarget, Platform, Provenance, Tf2Install};

fn stock_shell(cx: &mut TestAppContext, env: &support::Env) -> support::ShellWindow {
    support::seed_stock_game(&env.game);
    support::shell_window(
        cx,
        env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    )
}

/// Imports a stock-file HUD rooted at `root` whose health number is `colour`.
fn seed_base(
    services: &Services,
    dir: &Path,
    root: &str,
    colour: &str,
    provenance: Provenance,
) -> HudEntry {
    let health = support::hud_stock::HEALTH.replace("TanDark", colour);
    support::hud_copies::import_stock_hud(
        services,
        dir,
        root,
        &[("resource/ui/hudplayerhealth.res", &health)],
        provenance,
    )
}

fn revisit_huds(cx: &mut TestAppContext, s: &support::ShellWindow) {
    support::click_id(cx, s.win, ids::nav(Page::Sounds));
    support::click_id(cx, s.win, ids::nav(Page::Huds));
}

#[gpui_kit::test]
fn edit_a_copy_makes_a_copy_and_opens_it_in_the_maker(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let base = seed_base(
        &s.services,
        env.tmp.path(),
        "somehud",
        "TanLight",
        Provenance::Local,
    );
    revisit_huds(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_copy(base.folder_name())).as_deref(),
        Some("Edit a copy")
    );
    support::click_id(cx, s.win, ids::hud_copy(base.folder_name()));
    support::wait_real(cx, "the copy to open", |cx| {
        cx.update(|cx| {
            let shell = s.shell.read(cx);
            shell.page() == Page::HudMaker
                && shell
                    .hud_maker_page()
                    .is_some_and(|p| p.read(cx).is_loaded())
        })
    });
    let page = cx.update(|cx| s.shell.read(cx).hud_maker_page()).unwrap();
    let copy_name = format!("{} (copy)", base.name());
    let copy = s
        .services
        .library
        .list_huds()
        .unwrap()
        .into_iter()
        .find(|h| h.name() == copy_name)
        .expect("the copy's row");
    assert_eq!(
        cx.update(|cx| page.read(cx).saved_hud().cloned()),
        Some(copy.id().clone())
    );
    assert!(
        support::toast_labels(cx, s.win)
            .iter()
            .any(|t| t.contains("check the HUD's page for its terms"))
    );
}

#[gpui_kit::test]
fn a_copy_card_names_its_base_and_made_huds_offer_no_copy(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let base = seed_base(
        &s.services,
        env.tmp.path(),
        "somehud",
        "TanDark",
        Provenance::Local,
    );
    let copy = s.services.library.copy_hud(base.id()).unwrap();
    let files = support::hud_stock::stock_files();
    let made = s
        .services
        .library
        .save_made_hud(&MakeTarget::New, "Made", &files, Platform::Linux, &[])
        .unwrap();
    revisit_huds(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_copy_of(copy.folder_name())).as_deref(),
        Some(format!("Copy of {}", base.name()).as_str())
    );
    assert!(support::present(
        cx,
        s.win,
        ids::hud_copy(base.folder_name())
    ));
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_copy(copy.folder_name())
    ));
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_copy(made.entry().folder_name())
    ));
    assert!(support::present(
        cx,
        s.win,
        ids::hud_edit(copy.folder_name())
    ));
}

#[gpui_kit::test]
fn a_full_disk_refuses_the_copy_in_plain_words(cx: &mut TestAppContext) {
    let env = support::env();
    support::seed_stock_game(&env.game);
    let state = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    let s = support::shell_window_tweaked(cx, &env, state, |services| {
        services.library.set_free_space_probe(|_| Ok(0))
    });
    let base = seed_base(
        &s.services,
        env.tmp.path(),
        "somehud",
        "TanDark",
        Provenance::Local,
    );
    revisit_huds(cx, &s);
    support::click_id(cx, s.win, ids::hud_copy(base.folder_name()));
    support::wait_real(cx, "the refusal", |cx| {
        support::toast_labels(cx, s.win)
            .iter()
            .any(|t| t.contains("There isn't enough free disk space to copy this HUD."))
    });
    assert_eq!(s.services.library.list_huds().unwrap().len(), 1);
    assert_eq!(cx.update(|cx| s.shell.read(cx).page()), Page::Huds);
}

#[gpui_kit::test]
fn a_damaged_copy_record_still_shows_the_card(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let base = seed_base(
        &s.services,
        env.tmp.path(),
        "somehud",
        "TanDark",
        Provenance::Local,
    );
    let copy = s.services.library.copy_hud(base.id()).unwrap();
    let maker = env
        .data
        .library_dir(LibraryKind::Huds)
        .join(copy.folder_name())
        .join(".tf2tools/maker.json");
    std::fs::write(maker, b"not json").unwrap();
    revisit_huds(cx, &s);
    assert!(support::present(
        cx,
        s.win,
        ids::hud_card(copy.folder_name())
    ));
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_copy_of(copy.folder_name())
    ));
    assert!(support::present(
        cx,
        s.win,
        ids::hud_card(base.folder_name())
    ));
}
