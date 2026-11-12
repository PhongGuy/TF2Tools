//! The Graphics page: mastercomfig's status card and preset picker.

use crate::support;

use gpui_kit::assets::IconName;
use gpui_kit::{AppContext as _, Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::graphics::GraphicsPage;
use tf2tools_app::pages::graphics::model::StatusCard;
use tf2tools_app::pages::profiles::probe::Tf2Probe;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::nav_icon;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{Preset, Tf2Install};

fn found(env: &support::Env) -> Tf2State {
    Tf2State::Found(Tf2Install::from_path(&env.game).unwrap())
}

/// Opens Graphics from the sidebar and waits for its first load. A managed install without
/// a saved catalog fetches one on the tokio bridge, so the executor must be allowed to park.
fn open_graphics(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<GraphicsPage> {
    cx.executor().allow_parking();
    support::click_nav(cx, s.win, Page::Graphics);
    let page = cx
        .update(|cx| s.shell.read(cx).graphics_page())
        .expect("a Graphics page");
    support::wait_real(cx, "Graphics to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

#[test]
fn graphics_follows_addons_in_the_tools_group_with_the_gauge_icon() {
    let at = Page::ALL.iter().position(|p| *p == Page::Graphics).unwrap();
    assert_eq!(Page::ALL.len(), 16);
    assert_eq!(Page::ALL[at - 1], Page::Addons);
    assert_eq!(Page::ALL[at + 1], Page::HudStore);
    let page = Page::Graphics;
    assert_eq!(
        (page.slug(), page.label(), page.title()),
        ("graphics", "Graphics", "Graphics")
    );
    assert_eq!(nav_icon(page), IconName::Gauge);
    assert!(!page.is_placeholder());
}

#[gpui_kit::test]
fn graphics_is_a_real_page_rebuilt_on_each_visit(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_graphics(cx, &s);
    assert!(support::present(cx, s.win, ids::page(Page::Graphics)));
    assert!(!support::present(cx, s.win, ids::PLACEHOLDER_MESSAGE));
    assert_eq!(
        support::label_of(cx, s.win, ids::GRAPHICS_SUBTITLE).as_deref(),
        Some("Graphics and game settings from mastercomfig")
    );
    assert!(!support::present(cx, s.win, ids::GRAPHICS_NO_TF2));
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    assert!(cx.update(|cx| s.shell.read(cx).graphics_page()).is_none());
}

#[gpui_kit::test]
fn without_tf2_the_page_asks_for_the_tf2_folder(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let _page = open_graphics(cx, &s);
    assert!(support::present(cx, s.win, ids::page(Page::Graphics)));
    assert!(support::present(cx, s.win, ids::GRAPHICS_NO_TF2));
}

const HOOK: &str = "tf/custom/comfig-custom/cfg/app/setup_hook.cfg";
const BASE: &str = "tf/custom/mastercomfig-base.vpk";
const BASE_ROUTE: &str = "/gh/mastercomfig/mastercomfig-base.vpk";

fn shell(
    cx: &mut TestAppContext,
    env: &support::Env,
    server: &support::StoreServer,
) -> support::ShellWindow {
    cx.executor().allow_parking();
    let endpoints = server.endpoints();
    support::shell_window_tweaked(cx, env, found(env), |services| {
        *services = services.clone().with_endpoints(endpoints);
    })
}

fn card(cx: &mut TestAppContext, page: &Entity<GraphicsPage>) -> Option<StatusCard> {
    cx.update(|cx| page.read(cx).card())
}

fn installed(cx: &mut TestAppContext, page: &Entity<GraphicsPage>, version: &str) -> bool {
    matches!(card(cx, page), Some(StatusCard::Installed { version: v, .. }) if v == version)
}

fn hook(env: &support::Env) -> String {
    std::fs::read_to_string(env.game.join(HOOK)).unwrap()
}

fn selected(cx: &mut TestAppContext, s: &support::ShellWindow, preset: Preset) -> bool {
    support::selected_of(cx, s.win, ids::graphics_preset(preset)) == Some(true)
}

fn label(cx: &mut TestAppContext, s: &support::ShellWindow, id: &'static str) -> Option<String> {
    support::label_of(cx, s.win, id)
}

fn tf2_running(cx: &mut TestAppContext) {
    cx.update(|cx| Tf2Probe::install(cx, || true));
}

fn toasted(cx: &mut TestAppContext, s: &support::ShellWindow, text: &str) {
    support::wait_real(cx, text, |cx| {
        support::toast_labels(cx, s.win)
            .iter()
            .any(|t| t.contains(text))
    });
}

fn idle(cx: &mut TestAppContext, page: &Entity<GraphicsPage>) {
    support::wait_real(cx, "the page to settle", |cx| {
        cx.update(|cx| !page.read(cx).is_busy())
    });
}

#[gpui_kit::test]
fn install_without_a_recorded_release_syncs_first_even_on_a_fresh_cache(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_mastercomfig_release("9.100.1");
    server.mount_bytes(BASE_ROUTE, support::mastercomfig_base("9.100.1"));
    let s = shell(cx, &env, &server);
    // Synced an hour ago, before the release table existed: fresh, yet no release row.
    let an_hour_ago = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
    rusqlite::Connection::open(env.data.db_path())
        .unwrap()
        .execute(
            "INSERT OR REPLACE INTO store_cache_meta (key, value) VALUES ('addons_last_synced', ?1)",
            [an_hour_ago],
        )
        .unwrap();
    let page = open_graphics(cx, &s);
    assert!(cx.update(|cx| page.read(cx).release().is_none()));
    support::click_id(cx, s.win, ids::GRAPHICS_INSTALL);
    support::wait_real(cx, "the install", |cx| installed(cx, &page, "9.100.1"));
    assert_eq!(
        std::fs::read(env.game.join(BASE)).unwrap(),
        support::mastercomfig_base("9.100.1")
    );
    assert_eq!(
        cx.update(|cx| page.read(cx).release().map(|r| r.tag().to_owned())),
        Some("9.100.1".to_owned())
    );
}

#[gpui_kit::test]
fn install_refuses_while_tf2_runs(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes(BASE_ROUTE, support::mastercomfig_base("9.100.1"));
    let s = shell(cx, &env, &server);
    support::seed_mastercomfig_release(&env, &server.uri(), "9.100.1");
    tf2_running(cx);
    let page = open_graphics(cx, &s);
    support::click_id(cx, s.win, ids::GRAPHICS_INSTALL);
    toasted(cx, &s, "Close TF2 first");
    idle(cx, &page);
    assert!(!env.game.join(BASE).exists());
    assert_eq!(card(cx, &page), Some(StatusCard::NotInstalled));
}

#[gpui_kit::test]
fn update_refuses_while_tf2_runs(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes(BASE_ROUTE, support::mastercomfig_base("9.101.0"));
    let s = shell(cx, &env, &server);
    support::seed_managed_mastercomfig(&env, "9.100.1", "low");
    support::seed_mastercomfig_release(&env, &server.uri(), "9.101.0");
    tf2_running(cx);
    let page = open_graphics(cx, &s);
    support::click_id(cx, s.win, ids::GRAPHICS_UPDATE);
    toasted(cx, &s, "Close TF2 first");
    idle(cx, &page);
    assert!(installed(cx, &page, "9.100.1"));
    assert_eq!(
        std::fs::read(env.game.join(BASE)).unwrap(),
        support::mastercomfig_base("9.100.1")
    );
}

#[gpui_kit::test]
fn install_fetches_the_recorded_release_and_shows_the_presets(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes(BASE_ROUTE, support::mastercomfig_base("9.100.1"));
    let s = shell(cx, &env, &server);
    support::seed_mastercomfig_release(&env, &server.uri(), "9.100.1");
    let page = open_graphics(cx, &s);
    assert_eq!(
        label(cx, &s, ids::GRAPHICS_HEADLINE).as_deref(),
        Some("mastercomfig isn't installed")
    );
    assert!(support::present(cx, s.win, ids::GRAPHICS_CREDIT));
    assert!(!support::present(
        cx,
        s.win,
        ids::graphics_preset(Preset::Medium)
    ));
    support::click_id(cx, s.win, ids::GRAPHICS_INSTALL);
    support::wait_real(cx, "the install", |cx| installed(cx, &page, "9.100.1"));
    assert_eq!(
        std::fs::read(env.game.join(BASE)).unwrap(),
        support::mastercomfig_base("9.100.1")
    );
    assert_eq!(
        label(cx, &s, ids::GRAPHICS_DETAIL).as_deref(),
        Some("Up to date")
    );
    for preset in Preset::PICKER {
        assert!(
            support::present(cx, s.win, ids::graphics_preset(preset)),
            "{preset:?}"
        );
    }
    assert!(selected(cx, &s, Preset::Medium));
    assert!(!selected(cx, &s, Preset::High));
    assert_eq!(
        label(cx, &s, ids::GRAPHICS_FOOTER).as_deref(),
        Some("Changes apply the next time TF2 starts.")
    );
}

#[gpui_kit::test]
fn a_hand_install_is_taken_over_with_its_own_preset(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    std::fs::write(env.game.join(BASE), support::mastercomfig_base("9.99.0")).unwrap();
    std::fs::create_dir_all(env.game.join(HOOK).parent().unwrap()).unwrap();
    std::fs::write(env.game.join(HOOK), "preset=low\n").unwrap();
    let page = open_graphics(cx, &s);
    assert_eq!(
        label(cx, &s, ids::GRAPHICS_DETAIL).as_deref(),
        Some("Managed outside TF2Tools")
    );
    assert!(!support::present(
        cx,
        s.win,
        ids::graphics_preset(Preset::Low)
    ));
    support::click_id(cx, s.win, ids::GRAPHICS_TAKE_OVER);
    support::wait_real(cx, "the take over", |cx| installed(cx, &page, "9.99.0"));
    assert!(selected(cx, &s, Preset::Low));
    assert_eq!(hook(&env), "preset=low\n");
}

#[gpui_kit::test]
fn opening_restores_a_cleared_preset_file_and_a_pick_writes_at_once(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "ultra");
    let page = open_graphics(cx, &s);
    assert_eq!(
        hook(&env),
        "preset=ultra\n",
        "the page writes back what another tool cleared"
    );
    support::click_id(cx, s.win, ids::graphics_preset(Preset::High));
    support::wait_real(cx, "the pick", |cx| {
        cx.update(|cx| page.read(cx).preset()) == Some(Preset::High)
    });
    assert_eq!(hook(&env), "preset=high\n");
    assert!(selected(cx, &s, Preset::High));
}

#[gpui_kit::test]
fn a_newer_recorded_release_is_offered_and_installed(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes(BASE_ROUTE, support::mastercomfig_base("9.101.0"));
    let s = shell(cx, &env, &server);
    support::seed_managed_mastercomfig(&env, "9.100.1", "low");
    support::seed_mastercomfig_release(&env, &server.uri(), "9.101.0");
    let page = open_graphics(cx, &s);
    assert_eq!(
        label(cx, &s, ids::GRAPHICS_DETAIL).as_deref(),
        Some("Update to 9.101.0")
    );
    support::click_id(cx, s.win, ids::GRAPHICS_UPDATE);
    support::wait_real(cx, "the update", |cx| installed(cx, &page, "9.101.0"));
    assert!(selected(cx, &s, Preset::Low));
}

#[gpui_kit::test]
fn remove_asks_first_then_removes_mastercomfig(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "high");
    let page = open_graphics(cx, &s);
    support::click_id(cx, s.win, ids::GRAPHICS_REMOVE);
    assert!(support::present(cx, s.win, ids::CONFIRM_DIALOG));
    assert!(
        env.game.join(BASE).exists(),
        "nothing goes before the answer"
    );
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    support::wait_real(cx, "the removal", |cx| {
        card(cx, &page) == Some(StatusCard::NotInstalled)
    });
    assert!(!env.game.join(BASE).exists());
}

#[gpui_kit::test]
fn the_buttons_stay_locked_until_the_page_has_reloaded(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "ultra");
    let page = open_graphics(cx, &s);
    support::click_without_settling(cx, s.win, ids::graphics_preset(Preset::High));
    assert!(
        cx.update(|cx| page.read(cx).is_busy()),
        "the pick locks the buttons"
    );
    // Step the executor one task at a time, so the moment the lock lifts is seen.
    while cx.executor().tick() {
        let (busy, preset) = cx.update(|cx| (page.read(cx).is_busy(), page.read(cx).preset()));
        if !busy {
            assert_eq!(
                preset,
                Some(Preset::High),
                "unlocked before the reload landed"
            );
        }
    }
    support::settle(cx, s.win);
    let disabled = cx
        .update_window(s.win, |_, w, _| {
            support::button_disabled(w, ids::GRAPHICS_REMOVE)
        })
        .unwrap();
    assert!(!disabled, "Remove unlocks once the page has reloaded");
}

#[gpui_kit::test]
fn a_preset_card_is_picked_with_the_keyboard(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "ultra");
    let page = open_graphics(cx, &s);
    support::tab_to(cx, s.win, ids::graphics_preset(Preset::High), 64);
    support::press(cx, s.win, "enter");
    support::wait_real(cx, "the Enter pick", |cx| {
        cx.update(|cx| page.read(cx).preset()) == Some(Preset::High)
    });
    support::tab_to(cx, s.win, ids::graphics_preset(Preset::Low), 64);
    support::press(cx, s.win, "space");
    support::wait_real(cx, "the Space pick", |cx| {
        cx.update(|cx| page.read(cx).preset()) == Some(Preset::Low)
    });
    assert_eq!(hook(&env), "preset=low\n");
}

#[gpui_kit::test]
fn remove_refuses_while_tf2_runs(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "high");
    tf2_running(cx);
    let page = open_graphics(cx, &s);
    support::click_id(cx, s.win, ids::GRAPHICS_REMOVE);
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    toasted(cx, &s, "Close TF2 first");
    idle(cx, &page);
    assert!(env.game.join(BASE).exists());
    assert!(installed(cx, &page, "9.100.1"));
}

#[gpui_kit::test]
fn check_for_updates_records_a_newer_release(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_mastercomfig_release("9.101.0");
    let s = shell(cx, &env, &server);
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    let page = open_graphics(cx, &s);
    assert!(!support::present(cx, s.win, ids::GRAPHICS_UPDATE));
    support::click_id(cx, s.win, ids::GRAPHICS_CHECK);
    toasted(cx, &s, "Checked for updates");
    idle(cx, &page);
    assert_eq!(
        label(cx, &s, ids::GRAPHICS_DETAIL).as_deref(),
        Some("Update to 9.101.0")
    );
    assert!(support::present(cx, s.win, ids::GRAPHICS_UPDATE));
}

#[gpui_kit::test]
fn a_rate_limited_check_says_github_is_busy(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_status(support::MASTERCOMFIG_RELEASE_ROUTE, 403);
    let s = shell(cx, &env, &server);
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    let page = open_graphics(cx, &s);
    support::click_id(cx, s.win, ids::GRAPHICS_CHECK);
    toasted(
        cx,
        &s,
        "GitHub is busy right now. Wait a few minutes, then try again.",
    );
    idle(cx, &page);
    assert!(installed(cx, &page, "9.100.1"));
}

#[gpui_kit::test]
fn only_the_running_job_s_button_spins(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_bytes(BASE_ROUTE, support::mastercomfig_base("9.101.0"));
    let s = shell(cx, &env, &server);
    support::seed_managed_mastercomfig(&env, "9.100.1", "ultra");
    support::seed_mastercomfig_release(&env, &server.uri(), "9.101.0");
    let page = open_graphics(cx, &s);
    support::click_without_settling(cx, s.win, ids::graphics_preset(Preset::High));
    let (busy, installing, checking) = cx.update(|cx| {
        let p = page.read(cx);
        (p.is_busy(), p.is_installing(), p.is_checking())
    });
    assert!(busy && !installing && !checking, "a pick spins nothing");
    idle(cx, &page);
    support::click_without_settling(cx, s.win, ids::GRAPHICS_UPDATE);
    assert!(cx.update(|cx| page.read(cx).is_installing()));
    assert!(!cx.update(|cx| page.read(cx).is_checking()));
    support::wait_real(cx, "the update", |cx| installed(cx, &page, "9.101.0"));
    idle(cx, &page);
    assert!(!cx.update(|cx| page.read(cx).is_installing()));
    support::click_without_settling(cx, s.win, ids::GRAPHICS_CHECK);
    assert!(cx.update(|cx| page.read(cx).is_checking()));
    assert!(!cx.update(|cx| page.read(cx).is_installing()));
    idle(cx, &page);
    assert!(!cx.update(|cx| page.read(cx).is_checking()));
}
