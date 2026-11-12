//! A GameBanana HUD's card marks a newer file on GameBanana and updates to it.

use crate::support;

use gpui_kit::TestAppContext;
use tf2tools_app::ids;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{GbId, HudEntry, Provenance, Tf2Install};

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
