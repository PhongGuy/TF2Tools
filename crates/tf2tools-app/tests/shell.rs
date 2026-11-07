use gpui_kit::{AppContext as _, TestAppContext, px};
use tf2tools_app::ids;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_app::shell::{CONTENT_PADDING, HEADER_HEIGHT, SIDEBAR_COLLAPSED_WIDTH, SIDEBAR_WIDTH};

use crate::support;

#[gpui_kit::test]
fn the_shell_shows_every_shell_id_and_opens_on_huds(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    for id in [
        ids::SIDEBAR,
        ids::SIDEBAR_TITLE,
        ids::SIDEBAR_TOGGLE,
        ids::HEADER,
        ids::HEADER_TITLE,
    ] {
        assert!(support::present(cx, s.win, id), "{id} missing");
    }
    for page in Page::ALL {
        assert!(
            support::present(cx, s.win, ids::nav(page)),
            "{page:?} nav missing"
        );
        assert_eq!(
            support::label_of(cx, s.win, ids::nav(page)).as_deref(),
            Some(page.label())
        );
    }
    assert!(support::present(cx, s.win, ids::page(Page::Huds)));
    assert_eq!(
        support::label_of(cx, s.win, ids::HEADER_TITLE).as_deref(),
        Some("HUDs")
    );
}

#[gpui_kit::test]
fn every_nav_item_opens_its_page_and_title(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    for page in Page::ALL {
        support::click_id(cx, s.win, ids::nav(page));
        assert_eq!(cx.update(|cx| s.shell.read(cx).page()), page);
        assert!(
            support::present(cx, s.win, ids::page(page)),
            "{page:?} root missing"
        );
        assert_eq!(
            support::label_of(cx, s.win, ids::HEADER_TITLE).as_deref(),
            Some(page.title())
        );
        if page.is_placeholder() {
            assert_eq!(
                support::label_of(cx, s.win, ids::PLACEHOLDER_MESSAGE).as_deref(),
                Some("Coming in a later version")
            );
        }
        for other in Page::ALL.into_iter().filter(|p| *p != page) {
            assert!(
                !support::present(cx, s.win, ids::page(other)),
                "{other:?} still shown"
            );
        }
    }
}

#[gpui_kit::test]
fn the_layout_matches_v5_sizes(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let sidebar = support::bounds_of(cx, s.win, ids::SIDEBAR).unwrap();
    let header = support::bounds_of(cx, s.win, ids::HEADER).unwrap();
    support::click_id(cx, s.win, ids::nav(Page::Profiles));
    let page = support::bounds_of(cx, s.win, ids::page(Page::Profiles)).unwrap();
    assert_eq!(sidebar.size.width, SIDEBAR_WIDTH);
    assert_eq!(header.size.height, HEADER_HEIGHT);
    assert_eq!(header.origin.x, SIDEBAR_WIDTH);
    // The page fills the content area; its padding lives inside it.
    assert_eq!(page.origin.x, SIDEBAR_WIDTH);
    assert_eq!(page.origin.y, HEADER_HEIGHT);
    assert_eq!(page.size.width, px(1200.) - SIDEBAR_WIDTH);
    assert_eq!(page.size.height, px(900.) - HEADER_HEIGHT);
    let search = support::bounds_of(cx, s.win, ids::PROFILES_SEARCH).unwrap();
    assert!(search.origin.x >= SIDEBAR_WIDTH + CONTENT_PADDING);
}

#[gpui_kit::test]
fn the_toggle_collapses_to_icons_and_persists(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    support::click_id(cx, s.win, ids::SIDEBAR_TOGGLE);
    assert!(cx.update(|cx| s.shell.read(cx).collapsed()));
    assert_eq!(
        support::bounds_of(cx, s.win, ids::SIDEBAR)
            .unwrap()
            .size
            .width,
        SIDEBAR_COLLAPSED_WIDTH
    );
    assert!(!support::present(cx, s.win, ids::SIDEBAR_TITLE));
    // Collapsed items keep their id and accessible name; the text moves to a tooltip.
    assert_eq!(
        support::label_of(cx, s.win, ids::nav(Page::Huds)).as_deref(),
        Some("HUDs")
    );
    assert!(
        support::bounds_of(cx, s.win, ids::nav(Page::Huds))
            .unwrap()
            .size
            .width
            <= px(48.)
    );
    support::wait_real(cx, "the setting", |_| {
        s.services.db.settings().unwrap().sidebar_collapsed()
    });
}

#[gpui_kit::test]
fn ctrl_b_toggles_the_sidebar(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    support::press(cx, s.win, "ctrl-b");
    assert_eq!(
        support::bounds_of(cx, s.win, ids::SIDEBAR)
            .unwrap()
            .size
            .width,
        SIDEBAR_COLLAPSED_WIDTH
    );
    support::press(cx, s.win, "ctrl-b");
    assert_eq!(
        support::bounds_of(cx, s.win, ids::SIDEBAR)
            .unwrap()
            .size
            .width,
        SIDEBAR_WIDTH
    );
    support::wait_real(cx, "the setting", |_| {
        !s.services.db.settings().unwrap().sidebar_collapsed()
    });
}

#[gpui_kit::test]
fn a_saved_collapsed_state_is_restored(cx: &mut TestAppContext) {
    let env = support::env();
    support::services(&env, support::no_steam())
        .db
        .set_sidebar_collapsed(true)
        .unwrap();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    assert_eq!(
        support::bounds_of(cx, s.win, ids::SIDEBAR)
            .unwrap()
            .size
            .width,
        SIDEBAR_COLLAPSED_WIDTH
    );
}

#[gpui_kit::test]
fn the_first_nav_item_sits_where_v5s_does(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let item = support::bounds_of(cx, s.win, ids::nav(Page::Huds)).unwrap();
    // v5: header p-2 and a py-3 title row of 28 px, then the group's p-2.
    assert_eq!(
        (item.origin.x, item.origin.y, item.size.height),
        (px(8.), px(76.), px(32.))
    );
}

// In a window shorter than the 800x600 minimum the groups outgrow the rail: they scroll above
// the footer, as v5's SidebarContent does, so the footer's divider never crosses a nav item.
#[gpui_kit::test]
fn a_short_window_scrolls_the_groups_above_the_footer(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    cx.simulate_window_resize(s.win, gpui_kit::size(px(800.), px(480.)));
    support::settle(cx, s.win);
    let groups = support::bounds_of(cx, s.win, ids::SIDEBAR_GROUPS).unwrap();
    let settings = support::bounds_of(cx, s.win, ids::nav(Page::Settings)).unwrap();
    // The footer's p-2 and gap-2 sit between the groups and Settings; the separator takes no
    // layout height and draws its line at the top of the gap-2.
    assert!(
        groups.bottom() + px(16.) <= settings.top(),
        "{groups:?} vs {settings:?}"
    );
    let maker = support::bounds_of(cx, s.win, ids::nav(Page::HudMaker)).unwrap();
    assert!(
        maker.bottom() > groups.bottom(),
        "the groups fit; the test proves nothing"
    );
    cx.update_window(s.win, |_, w, cx| {
        let wheel = gpui_kit::ScrollWheelEvent {
            position: groups.center(),
            delta: gpui_kit::ScrollDelta::Lines(gpui_kit::point(0., -20.)),
            modifiers: gpui_kit::Modifiers::default(),
            touch_phase: gpui_kit::TouchPhase::Moved,
        };
        let _ = w.dispatch_event(gpui_kit::InputEvent::to_platform_input(wheel), cx);
    })
    .unwrap();
    support::settle(cx, s.win);
    let maker = support::bounds_of(cx, s.win, ids::nav(Page::HudMaker)).unwrap();
    assert!(
        maker.bottom() <= groups.bottom(),
        "{maker:?} under {groups:?}"
    );
    let after = support::bounds_of(cx, s.win, ids::nav(Page::Settings)).unwrap();
    assert_eq!(after, settings, "the footer stays put");
}

// At the 800x600 minimum every nav item shows whole, so one click at its centre opens its page
// without scrolling the groups first.
#[gpui_kit::test]
fn at_800x600_every_nav_item_shows_whole_and_takes_one_click(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    cx.simulate_window_resize(s.win, gpui_kit::size(px(800.), px(600.)));
    support::settle(cx, s.win);
    let sidebar = support::bounds_of(cx, s.win, ids::SIDEBAR).unwrap();
    assert_eq!(sidebar.size.width, SIDEBAR_WIDTH, "the sidebar starts open");
    let groups = support::bounds_of(cx, s.win, ids::SIDEBAR_GROUPS).unwrap();
    let footer = support::bounds_of(cx, s.win, ids::nav(Page::Settings)).unwrap();
    for page in Page::ALL {
        let item = support::bounds_of(cx, s.win, ids::nav(page)).unwrap();
        let shown = if item.top() >= footer.top() {
            item.bottom() <= sidebar.bottom()
        } else {
            item.top() >= groups.top() && item.bottom() <= groups.bottom()
        };
        assert!(shown, "{page:?} {item:?} cut off by {groups:?}");
    }
    for page in [Page::HudMaker, Page::CrosshairMaker, Page::Logs] {
        support::click_id(cx, s.win, ids::nav(page));
        assert_eq!(cx.update(|cx| s.shell.read(cx).page()), page);
    }
}

// Session-mode pilot runs match the accessibility id the app derives from each element id with
// `to_string()`. gpui builds no accessibility tree headless, so this pins the string a pilot
// targets; the sessions themselves are checked by hand (CONTRIBUTING.md, Pilot QA).
#[test]
fn accessibility_ids_are_the_documented_element_id_strings() {
    assert_eq!(ids::nav(Page::Crosshairs).to_string(), "nav-crosshairs");
    assert_eq!(ids::nav(Page::HudStore).to_string(), "nav-hud-store");
    assert_eq!(ids::hud_card("Some Hud").to_string(), "hud-card:Some Hud");
}
