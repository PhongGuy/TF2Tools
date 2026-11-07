//! An unedited store HUD's card marks a newer store version and updates to it in place.

use crate::support;

use std::time::Duration;

use gpui_kit::TestAppContext;
use tf2tools_app::ids;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{HudEntry, HudStatus, Provenance, Tf2Install};

const A: &str = "a22ede014e4f3b1260aa8fd657ff0757d9d006b8";
const B: &str = "b33fef125f5f4c2371ba9fe768ff1868e0e7e119";

fn curated() -> Provenance {
    Provenance::Curated {
        hud_id: "rayshud".into(),
    }
}

// The store lists rayshud at commit `hash`, its archive served from `repo`.
fn store_at(env: &support::Env, repo: &str, hash: &str) {
    rusqlite::Connection::open(env.data.db_path())
        .unwrap()
        .execute(
            "INSERT OR REPLACE INTO store_huds (hud_id, name, author, repo, hash, data_json, fetched_at)
             VALUES ('rayshud', 'rayshud', 'raysfire', ?1, ?2, '{}', '2026-10-28T00:00:00Z')",
            (repo, hash),
        )
        .unwrap();
}

fn shell(cx: &mut TestAppContext, env: &support::Env) -> support::ShellWindow {
    support::seed_stock_game(&env.game);
    support::shell_window(
        cx,
        env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    )
}

fn download(s: &support::ShellWindow, env: &support::Env, folder: &str) -> HudEntry {
    support::hud_copies::import_stock_hud(&s.services, env.tmp.path(), folder, &[], curated())
}

fn revisit_huds(cx: &mut TestAppContext, s: &support::ShellWindow) {
    support::click_id(cx, s.win, ids::nav(Page::Sounds));
    support::click_id(cx, s.win, ids::nav(Page::Huds));
}

#[gpui_kit::test]
fn only_a_download_behind_the_store_is_marked(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let behind = download(&s, &env, &format!("rayshud-{A}"));
    let plain = download(&s, &env, "rayshud");
    store_at(&env, "https://github.com/raysfire/rayshud", B);
    let current = download(&s, &env, &format!("rayshud-{B}"));
    revisit_huds(cx, &s);
    support::wait_real(cx, "the update check", |cx| {
        support::present(cx, s.win, ids::hud_update_mark(behind.folder_name()))
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_update_mark(behind.folder_name())).as_deref(),
        Some("Update available")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_update(behind.folder_name())).as_deref(),
        Some("Update")
    );
    for hud in [&plain, &current] {
        let folder = hud.folder_name();
        assert!(!support::present(cx, s.win, ids::hud_update_mark(folder)));
        assert!(!support::present(cx, s.win, ids::hud_update(folder)));
    }
}

#[gpui_kit::test]
fn updating_an_installed_download_installs_the_new_version(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let server = support::StoreServer::start();
    support::store_ready(cx, &server);
    let new_root = format!("rayshud-{B}");
    let repo = server.mount_curated_at(
        "rayshud",
        B,
        support::hud_zip_bytes(&new_root),
        Duration::ZERO,
    );
    store_at(&env, &repo, B);
    let old = download(&s, &env, &format!("rayshud-{A}"));
    let tf2 = Tf2Install::from_path(&env.game).unwrap();
    s.services.library.install_hud(old.id(), &tf2).unwrap();
    revisit_huds(cx, &s);
    support::wait_real(cx, "the update check", |cx| {
        support::present(cx, s.win, ids::hud_update(old.folder_name()))
    });
    support::click_id(cx, s.win, ids::hud_update(old.folder_name()));
    support::wait_real(cx, "the update", |_| {
        let huds = s.services.library.list_huds().unwrap();
        huds.len() == 1 && huds[0].folder_name() == new_root
    });
    let new = s.services.library.list_huds().unwrap().remove(0);
    assert_eq!(new.provenance(), &curated());
    support::wait_real(cx, "the new version installed", |_| {
        matches!(
            s.services.library.hud_status(&tf2),
            Ok(HudStatus::Managed { id, .. }) if &id == new.id()
        )
    });
    support::wait_real(cx, "the toast", |cx| {
        support::toast_labels(cx, s.win)
            .iter()
            .any(|t| t.contains("rayshud is now up to date"))
    });
}

#[gpui_kit::test]
fn updating_a_download_that_is_not_installed_leaves_tf2_alone(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let server = support::StoreServer::start();
    support::store_ready(cx, &server);
    let new_root = format!("rayshud-{B}");
    let repo = server.mount_curated_at(
        "rayshud",
        B,
        support::hud_zip_bytes(&new_root),
        Duration::ZERO,
    );
    let old = download(&s, &env, &format!("rayshud-{A}"));
    store_at(&env, &repo, B);
    revisit_huds(cx, &s);
    support::wait_real(cx, "the update check", |cx| {
        support::present(cx, s.win, ids::hud_update(old.folder_name()))
    });
    support::click_id(cx, s.win, ids::hud_update(old.folder_name()));
    support::wait_real(cx, "the update", |_| {
        let huds = s.services.library.list_huds().unwrap();
        huds.len() == 1 && huds[0].folder_name() == new_root
    });
    let tf2 = Tf2Install::from_path(&env.game).unwrap();
    assert_eq!(
        s.services.library.hud_status(&tf2).unwrap(),
        HudStatus::NotInstalled
    );
}
