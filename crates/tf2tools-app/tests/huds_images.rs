mod support;

use std::io::{Cursor, Write as _};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use gpui_kit::{AppContext as _, Entity, TestAppContext};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use tf2tools_app::ids;
use tf2tools_app::pages::huds::HudsPage;
use tf2tools_app::remote_images::{ImageFetch, RemoteImages};
use tf2tools_app::services::{Services, Tf2State, install_tf2_state};
use tf2tools_app::shell::Shell;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{GbId, HudEntry, Provenance, Tf2Install};

const RAYS: &str =
    "https://raw.githubusercontent.com/mastercomfig/hud-db/main/hud-resources/rayshud/r1.webp";
const BUD: &str = "https://images.gamebanana.com/img/ss/mods/100-90_bud.jpg";

fn found(env: &support::Env) -> Tf2State {
    Tf2State::Found(Tf2Install::from_path(&env.game).unwrap())
}

fn hud_zip(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(format!("{name}.zip"));
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file(format!("{name}/info.vdf"), options).unwrap();
    write!(zip, "\"{name}\"\n{{\n\t\"ui_version\"\t\"3\"\n}}\n").unwrap();
    zip.start_file(format!("{name}/resource/ui/hudlayout.res"), options)
        .unwrap();
    write!(zip, "\"Resource/HudLayout.res\"\n{{\n}}\n").unwrap();
    zip.finish().unwrap();
    path
}

fn seed(services: &Services, env: &support::Env, name: &str, provenance: Provenance) -> HudEntry {
    services
        .library
        .import_hud(&hud_zip(env.tmp.path(), name), provenance)
        .unwrap()
}

fn store_rows(env: &support::Env) {
    rusqlite::Connection::open(env.data.db_path())
        .unwrap()
        .execute_batch(&format!(
            "INSERT INTO store_huds (hud_id, name, author, repo, hash, data_json, fetched_at)
                 VALUES ('rayshud', 'rayshud', 'raysfire', NULL, NULL, '{{\"resources\":[\"r1\",\"r2\"]}}', 'x');
             INSERT INTO store_huds_gb (gb_id, name, author, thumbnail_url, fetched_at)
                 VALUES (7, 'budhud', 'Bud', '{BUD}', 'x');"
        ))
        .unwrap();
}

/// Serves one PNG for every URL and logs what was asked.
fn fetch_log() -> (ImageFetch, Arc<Mutex<Vec<String>>>) {
    let mut out = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(RgbaImage::from_pixel(960, 540, Rgba([1, 2, 3, 255])))
        .write_to(&mut out, ImageFormat::Png)
        .unwrap();
    let png = Arc::new(out.into_inner());
    let asked = Arc::new(Mutex::new(Vec::new()));
    let log = asked.clone();
    let fetch: ImageFetch = Arc::new(move |url: String| {
        log.lock().unwrap().push(url);
        let png = png.clone();
        Box::pin(async move { Ok(png.to_vec()) })
    });
    (fetch, asked)
}

/// Leaves for Settings and comes back, which builds a fresh page that has loaded.
fn revisit(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<HudsPage> {
    support::click_id(cx, s.win, ids::nav(Page::Settings));
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    let page = cx
        .update(|cx| s.shell.read(cx).huds_page())
        .expect("a HUDs page");
    support::wait_real(cx, "HUDs to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

#[gpui_kit::test]
fn store_hud_cards_show_their_screenshots(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    cx.executor().allow_parking();
    let (fetch, asked) = fetch_log();
    let images = cx.update(|cx| RemoteImages::install(cx, fetch));
    store_rows(&env);
    seed(
        &s.services,
        &env,
        "rayshud",
        Provenance::Curated {
            hud_id: "rayshud".into(),
        },
    );
    seed(
        &s.services,
        &env,
        "budhud",
        Provenance::GameBanana {
            gb_id: GbId::new(7),
            author: "Bud".into(),
        },
    );
    let _page = revisit(cx, &s);
    support::wait_real(cx, "two screenshots", |cx| {
        cx.update(|cx| images.read(cx).stats().decodes == 2)
    });
    let mut asked_for = asked.lock().unwrap().clone();
    asked_for.sort();
    let mut expected = vec![RAYS.to_owned(), BUD.to_owned()];
    expected.sort();
    assert_eq!(asked_for, expected);
    assert!(support::present(cx, s.win, ids::hud_image("rayshud")));
    assert!(support::present(cx, s.win, ids::hud_image("budhud")));
    let card = support::bounds_of(cx, s.win, ids::hud_card("rayshud")).unwrap();
    let expected = (card.size.width * (9. / 16.)).floor() + gpui_kit::px(136.);
    assert!(
        (card.size.height - expected).abs() <= gpui_kit::px(1.),
        "{card:?}"
    );
}

#[gpui_kit::test]
fn a_store_hud_without_a_store_row_and_a_local_hud_fetch_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    cx.executor().allow_parking();
    let (fetch, asked) = fetch_log();
    cx.update(|cx| RemoteImages::install(cx, fetch));
    seed(
        &s.services,
        &env,
        "flawhud",
        Provenance::Curated {
            hud_id: "flawhud".into(),
        },
    );
    seed(&s.services, &env, "mine", Provenance::Local);
    let _page = revisit(cx, &s);
    support::settle(cx, s.win);
    assert!(asked.lock().unwrap().is_empty());
    assert!(!support::present(cx, s.win, ids::hud_image("flawhud")));
    assert!(support::present(cx, s.win, ids::hud_card("flawhud")));
}

// Startup hands the shell its HUD list while TF2 is still being detected, so the page skips
// the load and must resolve its screenshot URLs on its own.
#[gpui_kit::test]
fn a_preloaded_library_shows_screenshots_before_tf2_is_found(cx: &mut TestAppContext) {
    let env = support::env();
    support::init(cx);
    let services = support::services(&env, support::no_steam());
    support::install_services(cx, services.clone());
    cx.executor().allow_parking();
    let (fetch, asked) = fetch_log();
    let images = cx.update(|cx| RemoteImages::install(cx, fetch));
    store_rows(&env);
    let huds = vec![
        seed(
            &services,
            &env,
            "rayshud",
            Provenance::Curated {
                hud_id: "rayshud".into(),
            },
        ),
        seed(
            &services,
            &env,
            "budhud",
            Provenance::GameBanana {
                gb_id: GbId::new(7),
                author: "Bud".into(),
            },
        ),
    ];
    cx.update(|cx| install_tf2_state(cx, Tf2State::Detecting));
    let (win, shell) = cx
        .update(|cx| {
            gpui_kit::open_window(support::window_options(), cx, |window, cx| {
                cx.new(|cx| Shell::with_huds(false, Some(huds), window, cx))
            })
        })
        .unwrap();
    support::wait_real(cx, "two screenshots", |cx| {
        support::settle(cx, win);
        cx.update(|cx| images.read(cx).stats().decodes == 2)
    });
    let page = cx
        .update(|cx| shell.read(cx).huds_page())
        .expect("a HUDs page");
    assert!(cx.update(|cx| page.read(cx).is_loaded()));
    let mut asked_for = asked.lock().unwrap().clone();
    asked_for.sort();
    let mut expected = vec![RAYS.to_owned(), BUD.to_owned()];
    expected.sort();
    assert_eq!(asked_for, expected);
    assert!(support::present(cx, win, ids::hud_image("rayshud")));
    assert!(support::present(cx, win, ids::hud_image("budhud")));
}

#[gpui_kit::test]
fn an_old_page_released_late_keeps_the_new_pages_wants(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    cx.executor().allow_parking();
    let (fetch, _asked) = fetch_log();
    let images = cx.update(|cx| RemoteImages::install(cx, fetch));
    store_rows(&env);
    seed(
        &s.services,
        &env,
        "rayshud",
        Provenance::Curated {
            hud_id: "rayshud".into(),
        },
    );
    let old = revisit(cx, &s);
    support::wait_real(cx, "the first page's want", |cx| {
        cx.update(|cx| images.read(cx).viewers() == 1)
    });
    // The shell has moved on, but a handle keeps the old page alive past the new one's want.
    let new = revisit(cx, &s);
    assert_ne!(old.entity_id(), new.entity_id());
    support::wait_real(cx, "both pages' wants", |cx| {
        cx.update(|cx| images.read(cx).viewers() == 2)
    });
    drop(old);
    support::settle(cx, s.win);
    assert_eq!(cx.update(|cx| images.read(cx).viewers()), 1);
}
