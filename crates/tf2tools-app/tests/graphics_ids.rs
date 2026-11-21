//! Every Graphics id ids.rs declares is drawn in some state of the page.

use crate::support;

use std::collections::BTreeSet;

use gpui_kit::{AppContext as _, ElementId, Entity, TestAppContext};
use support::graphics::{
    idle, no_network, open, reveal, saved, scroll_to_top, search, server_shell, toggle_pictures,
    value, wait_for_list, wait_for_pictures,
};
use tf2tools_app::ids;
use tf2tools_app::pages::graphics::GraphicsPage;
use tf2tools_app::pages::profiles::probe::Tf2Probe;
use tf2tools_app::remote_images::RemoteImages;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;

// The kind of every Graphics id ids.rs spells: a constant's whole name, or a keyed id's kind.
fn declared_kinds() -> BTreeSet<String> {
    include_str!("../src/ids.rs")
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .filter_map(|line| line.split('"').nth(1))
        .filter(|lit| lit.starts_with("graphics-"))
        .map(str::to_owned)
        .collect()
}

// Adds the kind of every Graphics id the last frame registered, settling first.
fn collect(cx: &mut TestAppContext, s: &support::ShellWindow, seen: &mut BTreeSet<String>) {
    support::settle(cx, s.win);
    let drawn: Vec<String> = cx
        .update_window(s.win, |_, w, _| {
            gpui_kit::base::test_support::snapshots(w)
                .iter()
                .filter_map(|e| match e.path().last() {
                    Some(ElementId::Name(n)) => n.split(':').next().map(str::to_owned),
                    _ => None,
                })
                .filter(|kind| kind.starts_with("graphics-"))
                .collect()
        })
        .unwrap();
    seen.extend(drawn);
}

// Leaves Graphics and opens it again, so the page is rebuilt over what is now on disk.
fn revisit(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<GraphicsPage> {
    support::click_nav(cx, s.win, Page::Huds);
    support::settle(cx, s.win);
    open(cx, s)
}

// Collects once `id` is drawn, for parts that appear only after their picture decodes.
fn collect_when(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    id: ElementId,
    seen: &mut BTreeSet<String>,
) {
    support::wait_real(cx, &format!("{id:?}"), |cx| {
        support::present(cx, s.win, id.clone())
    });
    collect(cx, s, seen);
}

// Walks one shell through each state that draws ids of its own: every install state, the
// list before and after a change, open pictures with and without a difference, an open
// Select, an empty search and a missing TF2.
#[gpui_kit::test]
fn every_graphics_id_is_drawn_in_some_page_state(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    cx.update(|cx| Tf2Probe::install(cx, || true));
    let s = server_shell(cx, &env, &server);
    cx.update(|cx| RemoteImages::install(cx, no_network()));
    let mut seen = BTreeSet::new();

    let _page = open(cx, &s);
    collect(cx, &s, &mut seen);

    std::fs::write(
        env.game.join("tf/custom/mastercomfig-base.vpk"),
        support::mastercomfig_base("9.99.0"),
    )
    .unwrap();
    let hook = env
        .game
        .join("tf/custom/comfig-custom/cfg/app/setup_hook.cfg");
    std::fs::create_dir_all(hook.parent().unwrap()).unwrap();
    std::fs::write(&hook, "preset=low\n").unwrap();
    let _page = revisit(cx, &s);
    collect(cx, &s, &mut seen);

    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_mastercomfig_release(&env, &server.uri(), "9.101.0");
    let page = revisit(cx, &s);
    support::wait_real(cx, "the running answer", |cx| {
        cx.update(|cx| page.read(cx).tf2_running())
    });
    collect(cx, &s, &mut seen);

    support::seed_full_mastercomfig_catalog(&env, "9.100.1");
    support::seed_mastercomfig_choices(&env, "{}");
    let page = revisit(cx, &s);
    wait_for_list(cx, &s, &page);
    wait_for_pictures(cx, &s, &page);
    collect(cx, &s, &mut seen);

    reveal(cx, &s, ids::graphics_segment("lod", "high"));
    support::click_id(cx, s.win, ids::graphics_segment("lod", "high"));
    support::wait_real(cx, "lod high", |cx| {
        saved(cx, &page, "lod") == Some(value("high"))
    });
    idle(cx, &page);
    collect(cx, &s, &mut seen);
    scroll_to_top(cx, &s);
    collect(cx, &s, &mut seen);

    toggle_pictures(cx, &s, "lod");
    collect_when(cx, &s, ids::graphics_thumb_image("lod"), &mut seen);
    collect_when(cx, &s, ids::graphics_strip_image("lod", "low"), &mut seen);
    collect_when(
        cx,
        &s,
        ids::graphics_compare_image("lod", "yours"),
        &mut seen,
    );
    toggle_pictures(cx, &s, "texture_filter");
    collect_when(
        cx,
        &s,
        ids::graphics_compare_pill("texture_filter", "same"),
        &mut seen,
    );

    scroll_to_top(cx, &s);
    reveal(cx, &s, ids::graphics_module_select("decals"));
    support::click_id(cx, s.win, ids::graphics_module_select("decals"));
    collect(cx, &s, &mut seen);
    support::press(cx, s.win, "escape");

    search(cx, &s, &page, "zzz");
    collect(cx, &s, &mut seen);

    cx.update(|cx| {
        s.tf2.update(cx, |state, cx| {
            *state = Tf2State::Missing { saved: None };
            cx.notify();
        })
    });
    collect(cx, &s, &mut seen);

    let missing: Vec<String> = declared_kinds().difference(&seen).cloned().collect();
    assert!(missing.is_empty(), "never drawn: {missing:?}");
}
