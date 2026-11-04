//! A copy's card offers Update when its base has a newer version, and reports the result.

mod support;

use std::path::Path;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::local_messages::copy_in_store_text;
use tf2tools_app::services::{Services, Tf2State};
use tf2tools_app::shell::route::Page;
use tf2tools_core::{CopyUpdate, HudEntry, Provenance, Tf2Install};

const RAYS: &str = "rayshud-a22ede014e4f3b1260aa8fd657ff0757d9d006b8";

fn stock_shell(cx: &mut TestAppContext, env: &support::Env) -> support::ShellWindow {
    support::seed_stock_game(&env.game);
    support::shell_window(
        cx,
        env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    )
}

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
fn a_copy_with_a_newer_base_offers_update_and_reports_it(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let base = seed_base(
        &s.services,
        env.tmp.path(),
        "somehud",
        "TanLight",
        Provenance::Local,
    );
    let copy = s.services.library.copy_hud(base.id()).unwrap();
    seed_base(
        &s.services,
        env.tmp.path(),
        "somehud",
        "TanDark",
        Provenance::Local,
    );
    revisit_huds(cx, &s);
    support::wait_real(cx, "the update check", |cx| {
        support::present(cx, s.win, ids::hud_update(copy.folder_name()))
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_update(copy.folder_name())).as_deref(),
        Some("Update")
    );
    support::click_id(cx, s.win, ids::hud_update(copy.folder_name()));
    support::wait_real(cx, "the update", |cx| {
        support::toast_labels(cx, s.win)
            .iter()
            .any(|t| t.contains("now uses the newest version of its HUD"))
    });
    assert_eq!(
        s.services.library.copy_update(copy.id()).unwrap(),
        CopyUpdate::UpToDate
    );
    support::wait_real(cx, "the Update button to go", |cx| {
        !support::present(cx, s.win, ids::hud_update(copy.folder_name()))
    });
}

#[gpui_kit::test]
fn updating_an_installed_copy_reinstalls_it(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let base = seed_base(
        &s.services,
        env.tmp.path(),
        "somehud",
        "TanLight",
        Provenance::Local,
    );
    let copy = s.services.library.copy_hud(base.id()).unwrap();
    let tf2 = Tf2Install::from_path(&env.game).unwrap();
    let folder = s
        .services
        .library
        .install_hud(copy.id(), &tf2)
        .unwrap()
        .folder()
        .to_owned();
    seed_base(
        &s.services,
        env.tmp.path(),
        "somehud",
        "TanDark",
        Provenance::Local,
    );
    revisit_huds(cx, &s);
    support::wait_real(cx, "the update check", |cx| {
        support::present(cx, s.win, ids::hud_update(copy.folder_name()))
    });
    support::click_id(cx, s.win, ids::hud_update(copy.folder_name()));
    let installed = env
        .game
        .join("tf/custom")
        .join(&folder)
        .join("resource/ui/hudplayerhealth.res");
    support::wait_real(cx, "the reinstall", |_| {
        std::fs::read_to_string(&installed).is_ok_and(|t| t.contains("fgcolor TanDark"))
    });
}

#[gpui_kit::test]
fn a_newer_version_only_in_the_store_says_where_it_is(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let curated = Provenance::Curated {
        hud_id: "rayshud".into(),
    };
    let base = seed_base(&s.services, env.tmp.path(), RAYS, "TanDark", curated);
    let copy = s.services.library.copy_hud(base.id()).unwrap();
    rusqlite::Connection::open(env.data.db_path())
        .unwrap()
        .execute(
            "INSERT OR REPLACE INTO store_huds (hud_id, name, author, repo, hash, data_json, fetched_at)
             VALUES ('rayshud', 'rayshud', 'raysfire', 'raysfire/rayshud',
                     'b33fef125f5f4c2371ba9fe768ff1868e0e7e119', '{}', '2026-10-28T00:00:00Z')",
            [],
        )
        .unwrap();
    revisit_huds(cx, &s);
    support::wait_real(cx, "the update check", |cx| {
        support::present(cx, s.win, ids::hud_update_note(copy.folder_name()))
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_update_note(copy.folder_name())).as_deref(),
        Some(copy_in_store_text(base.name()).as_str())
    );
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_update(copy.folder_name())
    ));
}

#[gpui_kit::test]
fn edit_waits_while_an_update_runs(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let base = seed_base(
        &s.services,
        env.tmp.path(),
        "somehud",
        "TanLight",
        Provenance::Local,
    );
    let copy = s.services.library.copy_hud(base.id()).unwrap();
    seed_base(
        &s.services,
        env.tmp.path(),
        "somehud",
        "TanDark",
        Provenance::Local,
    );
    revisit_huds(cx, &s);
    support::wait_real(cx, "the update check", |cx| {
        support::present(cx, s.win, ids::hud_update(copy.folder_name()))
    });
    // The update's job stays queued until the executor next runs, so the clicks and frames
    // below all land while it is still running.
    let (update, edit) = (
        ids::hud_update(copy.folder_name()),
        ids::hud_edit(copy.folder_name()),
    );
    let edit_disabled = cx
        .update_window(s.win, |_, w, cx| {
            w.click(update, cx);
            w.render_frame(cx);
            let disabled = support::button_disabled(w, edit.clone());
            w.click(edit.clone(), cx);
            disabled
        })
        .unwrap();
    assert!(edit_disabled);
    assert_eq!(cx.update(|cx| s.shell.read(cx).page()), Page::Huds);
    support::wait_real(cx, "the update", |cx| {
        support::toast_labels(cx, s.win)
            .iter()
            .any(|t| t.contains("now uses the newest version of its HUD"))
    });
    assert_eq!(cx.update(|cx| s.shell.read(cx).page()), Page::Huds);
    assert!(!support::disabled_of(
        cx,
        s.win,
        ids::hud_edit(copy.folder_name())
    ));
}
