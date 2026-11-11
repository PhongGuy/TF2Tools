//! The logs.tf page: chips, the add dialog, sync, the class grid and the log list.

use crate::support;

use gpui_kit::{Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::logs_tf::LogsTfPage;
use tf2tools_app::pages::logs_tf::text::{log_url, profile_url};
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{STEAM_ID64_BASE, SteamId};

fn sid(account: u64) -> SteamId {
    SteamId::from_id64(STEAM_ID64_BASE + account).unwrap()
}

/// Opens logs.tf through the sidebar and waits for its first load and any sync to finish.
fn open_page(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<LogsTfPage> {
    support::click_id(cx, s.win, ids::nav(Page::LogsTf));
    let page = cx
        .update(|cx| s.shell.read(cx).logs_tf_page())
        .expect("a logs.tf page");
    support::wait_real(cx, "the logs.tf page", |cx| {
        cx.update(|cx| {
            let p = page.read(cx);
            p.is_loaded() && !p.is_syncing()
        })
    });
    support::settle(cx, s.win);
    page
}

#[gpui_kit::test]
fn logs_tf_sits_under_maps_and_starts_empty(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let at = Page::ALL.iter().position(|p| *p == Page::LogsTf).unwrap();
    assert_eq!(Page::ALL[at - 1], Page::Maps);
    assert_eq!(
        (Page::LogsTf.slug(), Page::LogsTf.label()),
        ("logs-tf", "logs.tf")
    );

    let page = open_page(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::HEADER_TITLE).as_deref(),
        Some("logs.tf")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::LOGSTF_CREDIT).as_deref(),
        Some("Stats from logs.tf")
    );
    assert!(support::present(cx, s.win, ids::LOGSTF_EMPTY));
    assert!(support::present(cx, s.win, ids::LOGSTF_EMPTY_ADD));
    assert!(cx.update(|cx| page.read(cx).players().is_empty()));
}

#[test]
fn links_point_at_logs_tf() {
    assert_eq!(log_url(4_137_103), "https://logs.tf/4137103");
    assert_eq!(
        profile_url(sid(231_702)),
        "https://logs.tf/profile/76561197960497430"
    );
}
