mod support;

use std::time::Duration;

use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::services::Services;
use tf2tools_app::store_detail::has_screenshots;
use tf2tools_app::store_download::StoreDownloads;
use tf2tools_core::{GbId, Provenance, StoreItemId, StoreKind};

const S: StoreKind = StoreKind::Sounds;

fn gb(id: u64) -> StoreItemId {
    StoreItemId::GameBanana(GbId::new(id))
}

/// A 16-bit stereo 44.1 kHz WAV of `frames` silent frames.
fn wav(frames: u32) -> Vec<u8> {
    let data_len = frames * 4;
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&44_100u32.to_le_bytes());
    out.extend_from_slice(&(44_100u32 * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.resize(out.len() + data_len as usize, 0);
    out
}

fn store(
    cx: &mut TestAppContext,
    env: &support::Env,
    server: &support::StoreServer,
) -> support::SoundStoreWindow {
    let url = server.mount_preview(5, vec![9, 9]);
    server.mount_sound_files(5, &[("boop.wav", wav(4_410))], Duration::from_millis(300));
    support::sound_store_window(cx, env, server, |env| {
        support::seed_sounds(env, &[(5, "Boop", "hitsound", Some(url.as_str()))]);
    })
}

#[test]
fn only_huds_and_crosshairs_have_screenshots() {
    assert!(has_screenshots(StoreKind::CuratedHuds));
    assert!(has_screenshots(StoreKind::GameBananaHuds));
    assert!(has_screenshots(StoreKind::Crosshairs));
    assert!(!has_screenshots(StoreKind::Sounds));
    assert!(!has_screenshots(StoreKind::Addons));
}

#[gpui_kit::test]
fn the_detail_lists_files_plays_the_preview_and_has_no_carousel(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let w = store(cx, &env, &server);
    assert_eq!(
        support::label_of(cx, w.win, ids::store_download(S, &gb(5))).as_deref(),
        Some("Download")
    );
    support::click_id(cx, w.win, ids::store_download(S, &gb(5)));
    support::wait_real(cx, "the file row", |cx| {
        support::present(cx, w.win, ids::store_file(0))
    });
    assert!(support::present(cx, w.win, ids::STORE_DETAIL));
    assert!(
        !support::present(cx, w.win, ids::CAROUSEL_EMPTY),
        "sounds have no carousel"
    );
    assert_eq!(
        support::label_of(cx, w.win, ids::STORE_PREVIEW).as_deref(),
        Some("Play Preview")
    );
    support::click_id(cx, w.win, ids::STORE_PREVIEW);
    support::wait_real(cx, "the preview", |_| w.audio.plays().len() == 1);
    assert_eq!(
        support::label_of(cx, w.win, ids::STORE_PREVIEW).as_deref(),
        Some("Stop Preview")
    );
    assert_eq!(
        cx.update(|cx| w.page.read(cx).playing(cx)),
        Some(GbId::new(5))
    );
}

#[gpui_kit::test]
fn a_downloaded_sound_lands_in_the_library_and_the_card_flips(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let w = store(cx, &env, &server);
    support::click_id(cx, w.win, ids::store_download(S, &gb(5)));
    support::wait_real(cx, "the file row", |cx| {
        support::present(cx, w.win, ids::store_file(0))
    });
    // The fake server's delay is real time and a settle can outlast it, so click without
    // settling past the click and check the download is running straight away.
    support::settle(cx, w.win);
    cx.update_window(w.win, |_, win, cx| win.click(ids::store_file(0), cx))
        .unwrap();
    let dl = cx.update(|cx| StoreDownloads::global(cx)).unwrap();
    assert!(
        cx.update(|cx| dl.read(cx).is_busy()),
        "the click starts the download"
    );
    support::wait_real(cx, "the download", |cx| {
        cx.update(|cx| !dl.read(cx).is_busy())
    });
    support::wait_real(cx, "the toast", |cx| {
        support::present(cx, w.win, ids::TOAST)
    });
    assert_eq!(
        support::label_of(cx, w.win, ids::TOAST).as_deref(),
        Some("Sound downloaded. Boop added to your library")
    );
    cx.update_window(w.win, |_, window, cx| window.close_dialog(cx))
        .unwrap();
    let card = cx.update(|cx| w.page.read(cx).grid().read(cx).cards()[0].clone());
    assert!(cx.update(|cx| w.page.read(cx).is_in_library(&card)));
    assert_eq!(
        support::label_of(cx, w.win, ids::store_download(S, &gb(5))).as_deref(),
        Some("In Library")
    );
    let sounds = cx
        .update(|cx| cx.global::<Services>().library.list_sounds())
        .unwrap();
    assert_eq!(sounds.len(), 1);
    assert!(
        matches!(sounds[0].provenance(), Provenance::GameBanana { gb_id, .. } if gb_id.get() == 5)
    );
}

#[gpui_kit::test]
fn a_play_click_on_the_card_opens_no_detail(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let w = store(cx, &env, &server);
    support::click_id(cx, w.win, ids::store_play(S, &gb(5)));
    support::wait_real(cx, "the preview", |_| w.audio.plays().len() == 1);
    support::settle(cx, w.win);
    assert!(!support::present(cx, w.win, ids::STORE_DETAIL));
}

// v5's sound detail draws its category and tags as the cards' 21 px badges, which never dim.
#[gpui_kit::test]
fn the_sound_detail_badges_are_the_cards_21_px_badges(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let w = store(cx, &env, &server);
    support::click_id(cx, w.win, ids::store_download(S, &gb(5)));
    support::wait_real(cx, "the detail tags", |cx| {
        support::present(cx, w.win, ids::store_detail_tag("tags", 0))
    });
    assert_eq!(
        support::label_of(cx, w.win, ids::STORE_DETAIL_CATEGORY).as_deref(),
        Some("Hitsound")
    );
    assert_eq!(
        support::label_of(cx, w.win, ids::store_detail_tag("tags", 0)).as_deref(),
        Some("Purity: Fun")
    );
    for id in [
        gpui_kit::ElementId::from(ids::STORE_DETAIL_CATEGORY),
        ids::store_detail_tag("tags", 0),
    ] {
        let badge = support::bounds_of(cx, w.win, id.clone()).expect("a badge");
        assert_eq!(badge.size.height, gpui_kit::px(21.), "{id:?}");
    }
}
