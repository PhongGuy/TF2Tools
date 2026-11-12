//! A GameBanana HUD's card marks a newer file on GameBanana and updates to it.

use crate::support;

use gpui_kit::TestAppContext;
use tf2tools_app::ids;
use tf2tools_app::pages::profiles::probe::Tf2Probe;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{GbId, HudEntry, HudStatus, Provenance, Tf2Install};

fn gb(id: u64) -> Provenance {
    Provenance::GameBanana {
        gb_id: GbId::new(id),
        author: "vex".into(),
    }
}

fn shell(cx: &mut TestAppContext, env: &support::Env) -> support::ShellWindow {
    support::seed_stock_game(&env.game);
    support::shell_window(
        cx,
        env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    )
}

// A GameBanana HUD of item `id` downloaded from a file dated `file_date`.
fn import(
    s: &support::ShellWindow,
    env: &support::Env,
    root: &str,
    id: u64,
    file_date: i64,
) -> HudEntry {
    let hud = support::hud_copies::import_stock_hud(&s.services, env.tmp.path(), root, &[], gb(id));
    support::sql(env)
        .execute(
            "UPDATE library_huds SET gb_file_date = ?1 WHERE id = ?2",
            (file_date, hud.id().as_str()),
        )
        .unwrap();
    hud
}

// Item `id` was checked just now and its newest file is dated `latest`.
fn checked(env: &support::Env, id: u64, latest: i64, gone: bool) {
    support::sql(env)
        .execute(
            "INSERT OR REPLACE INTO gb_hud_checks (gb_id, latest_file_date, checked_at, gone) VALUES (?1, ?2, ?3, ?4)",
            (id as i64, latest, chrono::Utc::now().timestamp(), gone),
        )
        .unwrap();
}

fn revisit_huds(cx: &mut TestAppContext, s: &support::ShellWindow) {
    support::click_id(cx, s.win, ids::nav(Page::Sounds));
    support::click_id(cx, s.win, ids::nav(Page::Huds));
}

#[gpui_kit::test]
fn a_newer_file_is_marked_and_current_or_gone_items_are_not(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let behind = import(&s, &env, "darhud-202517", 41, 100);
    checked(&env, 41, 300, false);
    let current = import(&s, &env, "budhud", 42, 300);
    checked(&env, 42, 300, false);
    let gone = import(&s, &env, "toonhud", 43, 100);
    checked(&env, 43, 300, true);
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
    for hud in [&current, &gone] {
        assert!(!support::present(
            cx,
            s.win,
            ids::hud_update_mark(hud.folder_name())
        ));
        assert!(!support::present(
            cx,
            s.win,
            ids::hud_update(hud.folder_name())
        ));
    }
}

#[gpui_kit::test]
fn the_check_runs_once_a_session_when_the_huds_page_opens(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    let server = support::StoreServer::start();
    support::store_ready(cx, &server);
    let hud = import(&s, &env, "darhud-202517", 44, 100);
    server.mount_gb_profile(
        44,
        serde_json::json!({"_idRow": 44, "_sName": "darhud", "_aFiles": [
            {"_idRow": 1, "_sFile": "darhud-202618.zip", "_tsDateAdded": 300,
             "_sDownloadUrl": format!("{}/dl/44/0", server.uri())}]}),
    );
    let profiles =
        |server: &support::StoreServer| server.requests_where(|p| p.ends_with("/ProfilePage"));
    revisit_huds(cx, &s);
    support::wait_real(cx, "the check's mark", |cx| {
        support::present(cx, s.win, ids::hud_update_mark(hud.folder_name()))
    });
    assert_eq!(profiles(&server), 1);
    support::sql(&env)
        .execute("UPDATE gb_hud_checks SET checked_at = 0", [])
        .unwrap();
    revisit_huds(cx, &s);
    support::wait_real(cx, "the page again", |cx| {
        support::present(cx, s.win, ids::hud_card(hud.folder_name()))
    });
    // Nothing signals "no check started", so poll for half a second and fail on the first extra request.
    let until = std::time::Instant::now() + std::time::Duration::from_millis(500);
    while std::time::Instant::now() < until {
        support::settle(cx, s.win);
        assert_eq!(profiles(&server), 1, "one check a session");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn ready(cx: &mut TestAppContext) -> (support::Env, support::ShellWindow, support::StoreServer) {
    let env = support::env();
    let s = shell(cx, &env);
    let server = support::StoreServer::start();
    support::store_ready(cx, &server);
    (env, s, server)
}

fn click_update(cx: &mut TestAppContext, s: &support::ShellWindow, hud: &HudEntry) {
    revisit_huds(cx, s);
    support::wait_real(cx, "the Update button", |cx| {
        support::present(cx, s.win, ids::hud_update(hud.folder_name()))
    });
    support::click_id(cx, s.win, ids::hud_update(hud.folder_name()));
}

fn installed_is(s: &support::ShellWindow, env: &support::Env, hud: &HudEntry) -> bool {
    let tf2 = Tf2Install::from_path(&env.game).unwrap();
    matches!(s.services.library.hud_status(&tf2), Ok(HudStatus::Managed { id, .. }) if &id == hud.id())
}

#[gpui_kit::test]
fn a_one_file_item_replaces_the_installed_hud_from_a_new_folder(cx: &mut TestAppContext) {
    let (env, s, server) = ready(cx);
    let old = import(&s, &env, "darhud-202517", 45, 100);
    checked(&env, 45, 300, false);
    server.mount_gb_files_dated(
        45,
        &[(
            "darhud-202618.zip",
            support::hud_zip_bytes("darhud-202618"),
            300,
        )],
    );
    s.services
        .library
        .install_hud(old.id(), &Tf2Install::from_path(&env.game).unwrap())
        .unwrap();
    click_update(cx, &s, &old);
    support::wait_real(cx, "the update", |_| {
        let huds = s.services.library.list_huds().unwrap();
        huds.len() == 1 && huds[0].folder_name() == "darhud-202618"
    });
    let new = s.services.library.list_huds().unwrap().remove(0);
    assert!(
        matches!(new.provenance(), Provenance::GameBanana { gb_id, .. } if *gb_id == GbId::new(45))
    );
    support::wait_real(cx, "the new version installed", |_| {
        installed_is(&s, &env, &new)
    });
    assert!(
        !env.data
            .library_dir(tf2tools_core::LibraryKind::Huds)
            .join("darhud-202517")
            .exists()
    );
    support::wait_real(cx, "the toast", |cx| {
        support::toast_labels(cx, s.win)
            .iter()
            .any(|t| t.contains("is now up to date"))
    });
}

#[gpui_kit::test]
fn a_download_into_the_same_folder_swaps_the_row_and_stays_installed(cx: &mut TestAppContext) {
    let (env, s, server) = ready(cx);
    let old = import(&s, &env, "darhud", 46, 100);
    checked(&env, 46, 300, false);
    server.mount_gb_files_dated(46, &[("darhud.zip", support::hud_zip_bytes("darhud"), 300)]);
    s.services
        .library
        .install_hud(old.id(), &Tf2Install::from_path(&env.game).unwrap())
        .unwrap();
    click_update(cx, &s, &old);
    support::wait_real(cx, "the toast", |cx| {
        support::toast_labels(cx, s.win)
            .iter()
            .any(|t| t.contains("is now up to date"))
    });
    let huds = s.services.library.list_huds().unwrap();
    assert_eq!(huds.len(), 1);
    assert_eq!(huds[0].folder_name(), "darhud");
    assert_ne!(huds[0].id(), old.id());
    assert!(installed_is(&s, &env, &huds[0]));
    assert!(
        !support::toast_labels(cx, s.win)
            .iter()
            .any(|t| t.starts_with("Failed"))
    );
}

#[gpui_kit::test]
fn update_refuses_while_tf2_runs_before_any_request(cx: &mut TestAppContext) {
    let (env, s, server) = ready(cx);
    cx.update(|cx| Tf2Probe::install(cx, || true));
    let old = import(&s, &env, "darhud-202517", 47, 100);
    checked(&env, 47, 300, false);
    server.mount_gb_files_dated(
        47,
        &[(
            "darhud-202618.zip",
            support::hud_zip_bytes("darhud-202618"),
            300,
        )],
    );
    click_update(cx, &s, &old);
    support::wait_real(cx, "the refusal", |cx| {
        support::toast_labels(cx, s.win)
            .iter()
            .any(|t| t == "Close TF2 first. TF2 is running. Close the game, then try again.")
    });
    assert_eq!(
        server.requests_where(|p| p.starts_with("/dl/") || p == "/apiv11/Mod/47"),
        0
    );
    let ids: Vec<_> = s
        .services
        .library
        .list_huds()
        .unwrap()
        .iter()
        .map(|h| h.id().clone())
        .collect();
    assert_eq!(ids, [old.id().clone()]);
}

#[gpui_kit::test]
fn a_failed_file_listing_toasts_and_keeps_the_hud(cx: &mut TestAppContext) {
    let (env, s, server) = ready(cx);
    let old = import(&s, &env, "darhud-202517", 51, 100);
    checked(&env, 51, 300, false);
    server.mount_gb_missing(51, std::time::Duration::ZERO);
    click_update(cx, &s, &old);
    support::wait_real(cx, "the failure toast", |cx| {
        support::toast_labels(cx, s.win)
            .iter()
            .any(|t| t.starts_with("Failed to update HUD"))
    });
    let huds = s.services.library.list_huds().unwrap();
    assert_eq!(huds.len(), 1);
    assert_eq!(huds[0].id(), old.id());
    assert!(
        !support::disabled_of(cx, s.win, ids::hud_update(old.folder_name())),
        "busy is cleared"
    );
}
