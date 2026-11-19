//! The page reads its pictures off the UI thread and asks the image cache for them.

use crate::support;

use gpui_kit::TestAppContext;
use support::graphics::{
    found, lod_only_assets, no_network, open, picture_page, picture_url, wait_for_list,
    wait_for_pictures, wants,
};
use tf2tools_app::assets::AssetRoot;
use tf2tools_app::remote_images::{ImageKey, RemoteImage, RemoteImages};
use tf2tools_app::shell::route::Page;

#[gpui_kit::test]
fn the_page_wants_each_row_s_current_picture(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = picture_page(cx, &env, "medium", r#"{"lod":"high"}"#);
    let key = |cx: &mut TestAppContext, module: &str, file: &str| {
        ImageKey::card(&picture_url(cx, &s, &page, module, file))
    };
    let (high, decals, medium) = (
        key(cx, "lod", "high.webp"),
        key(cx, "decals", "low.webp"),
        key(cx, "lod", "medium.webp"),
    );
    assert!(wants(cx, &page, &high));
    assert!(wants(cx, &page, &decals));
    assert!(!wants(cx, &page, &medium));
}

#[gpui_kit::test]
fn under_custom_an_untouched_row_wants_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let (s, page) = picture_page(cx, &env, "custom", "{}");
    for file in ["low.webp", "medium.webp", "high.webp", "ultra.webp"] {
        let key = ImageKey::card(&picture_url(cx, &s, &page, "lod", file));
        assert!(!wants(cx, &page, &key), "{file}");
    }
}

#[gpui_kit::test]
fn without_the_pictures_folder_nothing_is_wanted(cx: &mut TestAppContext) {
    let env = support::env();
    let root =
        AssetRoot::resolve_from(Some(support::graphics::bare_assets(&env)), None, None).unwrap();
    let s = support::shell_window_tweaked(cx, &env, found(&env), |services| services.assets = root);
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_full_mastercomfig_catalog(&env, "9.100.1");
    let page = open(cx, &s);
    wait_for_list(cx, &s, &page);
    wait_for_pictures(cx, &s, &page);
    assert!(cx.update(|cx| !page.read(cx).pictures().unwrap().has_pictures("lod")));
    assert!(cx.update(|cx| page.read(cx).wanted_images().is_empty()));
}

#[gpui_kit::test]
fn pictures_decode_off_the_ui_thread_and_leave_with_the_page(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    cx.executor().allow_parking();
    let images = cx.update(|cx| RemoteImages::install(cx, no_network()));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_full_mastercomfig_catalog(&env, "9.100.1");
    let page = open(cx, &s);
    wait_for_list(cx, &s, &page);
    wait_for_pictures(cx, &s, &page);
    let lod = ImageKey::card(&picture_url(cx, &s, &page, "lod", "medium.webp"));
    support::wait_real(cx, "the lod picture", |cx| {
        support::settle(cx, s.win);
        cx.update(|cx| matches!(images.read(cx).image(&lod), RemoteImage::Ready(_)))
    });
    assert_eq!(
        cx.update(|cx| images.read(cx).stats().foreground_decodes),
        0
    );
    // The shell drops the page on leaving; the test's handle must go too, or it stays alive.
    drop(page);
    support::click_id(cx, s.win, tf2tools_app::ids::nav(Page::Settings));
    support::wait_real(cx, "the page's wants to go", |cx| {
        cx.update(|cx| images.read(cx).viewers() == 0)
    });
}

#[gpui_kit::test]
fn an_undecodable_picture_fails_and_the_row_still_works(cx: &mut TestAppContext) {
    let env = support::env();
    let assets = lod_only_assets(&env, b"not a picture");
    let s =
        support::shell_window_tweaked(cx, &env, found(&env), |services| services.assets = assets);
    cx.executor().allow_parking();
    let images = cx.update(|cx| RemoteImages::install(cx, no_network()));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_full_mastercomfig_catalog(&env, "9.100.1");
    let page = open(cx, &s);
    wait_for_list(cx, &s, &page);
    wait_for_pictures(cx, &s, &page);
    let key = ImageKey::card(&picture_url(cx, &s, &page, "lod", "medium.webp"));
    support::wait_real(cx, "the failed decode", |cx| {
        support::settle(cx, s.win);
        cx.update(|cx| matches!(images.read(cx).image(&key), RemoteImage::Failed))
    });
    assert!(support::present(
        cx,
        s.win,
        tf2tools_app::ids::graphics_segments("lod")
    ));
}
