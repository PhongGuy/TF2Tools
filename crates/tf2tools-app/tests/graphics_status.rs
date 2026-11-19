//! The status row: one line per state, its actions, the credit and the mastercomfig docs.

use crate::support;

use gpui_kit::TestAppContext;
use support::graphics::{found, open};
use tf2tools_app::ids;

fn label(cx: &mut TestAppContext, s: &support::ShellWindow, id: &'static str) -> Option<String> {
    support::label_of(cx, s.win, id)
}

#[gpui_kit::test]
fn not_installed_offers_the_install_the_credit_and_the_docs(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open(cx, &s);
    assert_eq!(
        label(cx, &s, ids::GRAPHICS_HEADLINE).as_deref(),
        Some("mastercomfig isn't installed")
    );
    assert_eq!(
        label(cx, &s, ids::GRAPHICS_INSTALL).as_deref(),
        Some("Install mastercomfig")
    );
    assert_eq!(
        label(cx, &s, ids::GRAPHICS_CREDIT).as_deref(),
        Some("mastercomfig is made by the mastercomfig team and shared under the MIT licence.")
    );
    assert_eq!(
        label(cx, &s, ids::GRAPHICS_DOCS).as_deref(),
        Some("mastercomfig docs")
    );
    support::click_id(cx, s.win, ids::GRAPHICS_DOCS);
    assert_eq!(
        cx.opened_url().as_deref(),
        Some("https://docs.mastercomfig.com/")
    );
}

#[gpui_kit::test]
fn a_managed_install_says_every_change_saves_at_once(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_mastercomfig_release(&env, "http://127.0.0.1:9", "9.100.1");
    let _page = open(cx, &s);
    assert_eq!(
        label(cx, &s, ids::GRAPHICS_DETAIL).as_deref(),
        Some("Every change saves at once.")
    );
    assert_eq!(
        label(cx, &s, ids::GRAPHICS_CHECK).as_deref(),
        Some("Check for updates")
    );
    assert_eq!(
        label(cx, &s, ids::GRAPHICS_REMOVE).as_deref(),
        Some("Remove")
    );
    assert!(support::present(cx, s.win, ids::GRAPHICS_DOCS));
    assert!(!support::present(cx, s.win, ids::GRAPHICS_CREDIT));
    assert!(!support::present(cx, s.win, ids::GRAPHICS_UPDATE));
}

#[gpui_kit::test]
fn a_hand_install_offers_the_take_over_and_the_docs(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    std::fs::write(
        env.game.join("tf/custom/mastercomfig-base.vpk"),
        support::mastercomfig_base("9.99.0"),
    )
    .unwrap();
    let hook = env
        .game
        .join("tf/custom/comfig-custom/cfg/app/setup_hook.cfg");
    std::fs::create_dir_all(hook.parent().unwrap()).unwrap();
    std::fs::write(hook, "preset=low\n").unwrap();
    let _page = open(cx, &s);
    assert_eq!(
        label(cx, &s, ids::GRAPHICS_TAKE_OVER).as_deref(),
        Some("Take over")
    );
    assert!(support::present(cx, s.win, ids::GRAPHICS_DOCS));
}
