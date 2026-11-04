mod support;

use std::cell::{Cell, RefCell};
use std::io::Write as _;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::TestAppContext;
use gpui_kit::component::button::ButtonVariant;
use gpui_kit::{AnyWindowHandle, Entity};
use gpui_kit::{Context, IntoElement, ParentElement as _, Render, Styled as _, Window, div};
use tf2tools_app::ids;
use tf2tools_app::services::Services;
use tf2tools_app::store_download::{
    DownloadEvent, DownloadTarget, StoreDownloads, download_button, download_variant,
};
use tf2tools_app::store_download::{
    Progress, format_bytes, kind_noun, kind_title, progress_label, progress_percent,
    stayed_description,
};
use tf2tools_app::tokio_bridge;
use tf2tools_core::{DownloadPhase, GbId, StoreItemId, StoreKind};
use tf2tools_core::{HudEntry, Provenance, StoreFile};

#[test]
fn sizes_read_in_binary_units() {
    assert_eq!(format_bytes(512), "512 B");
    assert_eq!(format_bytes(1536), "1.5 KB");
    assert_eq!(format_bytes(3_565_158), "3.4 MB");
    assert_eq!(format_bytes(2 * 1024 * 1024 * 1024), "2.0 GB");
}

#[test]
fn the_label_and_bar_follow_the_phase() {
    assert_eq!(progress_label(None), "Downloading...");
    assert_eq!(progress_percent(None), 0.0);
    let p = Progress {
        phase: DownloadPhase::Downloading,
        downloaded: 1_048_576,
        total: Some(2_097_152),
    };
    assert_eq!(progress_label(Some(p)), "Downloading... 1.0 MB / 2.0 MB");
    assert_eq!(progress_percent(Some(p)), 50.0);
    let unsized_ = Progress { total: None, ..p };
    assert_eq!(progress_label(Some(unsized_)), "Downloading... 1.0 MB");
    assert_eq!(progress_percent(Some(unsized_)), 0.0);
    assert_eq!(
        progress_percent(Some(Progress {
            total: Some(0),
            ..p
        })),
        0.0
    );
    let importing = Progress {
        phase: DownloadPhase::Importing,
        ..p
    };
    assert_eq!(progress_label(Some(importing)), "Extracting...");
    assert_eq!(progress_percent(Some(importing)), 100.0);
}

#[test]
fn nouns_and_what_stayed() {
    assert_eq!(kind_noun(StoreKind::CuratedHuds), "HUD");
    assert_eq!(kind_noun(StoreKind::GameBananaHuds), "HUD");
    assert_eq!(kind_noun(StoreKind::Sounds), "sound");
    assert_eq!(kind_noun(StoreKind::Crosshairs), "crosshair");
    assert_eq!(kind_noun(StoreKind::Addons), "addon");
    assert_eq!(kind_title(StoreKind::Sounds), "Sound");
    assert_eq!(kind_title(StoreKind::GameBananaHuds), "HUD");
    assert_eq!(stayed_description("Try again.", &[]), "Try again.");
    assert_eq!(
        stayed_description("Try again.", &["a".into(), "b".into()]),
        "Try again. Still in your library: a, b."
    );
}

// An item the player already has rests in the secondary style, any other in the primary one.
#[test]
fn a_secondary_download_button_takes_the_secondary_variant() {
    assert_eq!(download_variant(true), ButtonVariant::Secondary);
    assert_eq!(download_variant(false), ButtonVariant::Primary);
}

#[gpui_kit::test]
fn the_fake_store_answers_core_listings(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_gb_files(
        7,
        &[("a.zip", vec![1, 2, 3]), ("a.rar", vec![4])],
        Duration::ZERO,
    );
    let repo = server.mount_curated("rayshud", vec![5], Duration::ZERO);
    support::init(cx);
    support::install_services(
        cx,
        support::services(&env, support::no_steam()).with_endpoints(server.endpoints()),
    );
    support::seed_curated(&env, "rayshud", "rayshud", "raysfire", Some(&repo), "{}");
    cx.update(tokio_bridge::init);
    let store = cx.update(|cx| cx.global::<Services>().store.clone());
    let rt = cx.update(|cx| tokio_bridge::handle(cx));
    let gb = rt
        .block_on(store.item_files(
            StoreKind::GameBananaHuds,
            &StoreItemId::GameBanana(GbId::new(7)),
        ))
        .unwrap();
    let listed: Vec<(String, Option<u64>)> = gb
        .iter()
        .map(|f| (f.file_name().to_string(), f.size()))
        .collect();
    assert_eq!(
        listed,
        [
            ("a.zip".to_string(), Some(3)),
            ("a.rar".to_string(), Some(1))
        ]
    );
    let curated = rt
        .block_on(store.item_files(
            StoreKind::CuratedHuds,
            &StoreItemId::Curated("rayshud".into()),
        ))
        .unwrap();
    assert_eq!(curated.len(), 1);
    assert_eq!(
        curated[0].url(),
        format!("{repo}/archive/{}.zip", support::CURATED_HASH)
    );
}

const GB: StoreKind = StoreKind::GameBananaHuds;

fn gb(id: u64) -> StoreItemId {
    StoreItemId::GameBanana(GbId::new(id))
}

fn target(kind: StoreKind, item: StoreItemId, name: &str) -> DownloadTarget {
    DownloadTarget {
        kind,
        item,
        name: name.into(),
    }
}

fn files(cx: &mut TestAppContext, kind: StoreKind, id: &StoreItemId) -> Vec<StoreFile> {
    let store = cx.update(|cx| cx.global::<Services>().store.clone());
    let rt = cx.update(|cx| tokio_bridge::handle(cx));
    rt.block_on(store.item_files(kind, id)).unwrap()
}

fn downloads(cx: &mut TestAppContext) -> Entity<StoreDownloads> {
    cx.update(|cx| StoreDownloads::global(cx))
        .expect("downloads installed")
}

fn events(cx: &mut TestAppContext, d: &Entity<StoreDownloads>) -> Rc<RefCell<Vec<DownloadEvent>>> {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let log = seen.clone();
    cx.update(|cx| {
        cx.subscribe(d, move |_, e: &DownloadEvent, _| {
            log.borrow_mut().push(e.clone())
        })
        .detach()
    });
    seen
}

fn start(
    cx: &mut TestAppContext,
    d: &Entity<StoreDownloads>,
    win: AnyWindowHandle,
    t: DownloadTarget,
    f: StoreFile,
) -> bool {
    cx.update(|cx| d.update(cx, |d, cx| d.start(t, f, win, cx)))
}

fn wait_idle(cx: &mut TestAppContext, d: &Entity<StoreDownloads>) {
    support::wait_real(cx, "the download to end", |cx| {
        cx.update(|cx| !d.read(cx).is_busy())
    });
}

fn library(cx: &mut TestAppContext) -> Vec<HudEntry> {
    cx.update(|cx| cx.global::<Services>().library.list_huds().unwrap())
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

/// A zip holding `pack/{name}` for each of `names`, each the same short WAV.
fn sound_pack(names: &[&str]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    for name in names {
        zip.start_file(format!("pack/{name}"), options).unwrap();
        zip.write_all(&wav(4_410)).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

/// Fails every library_sounds insert after the first and every delete, so a two-sound pack
/// imports one sound and cannot roll it back, as core's rollback test does.
const STUCK_ROLLBACK: &str = "CREATE TRIGGER fail_second_sound BEFORE INSERT ON library_sounds
     WHEN (SELECT COUNT(*) FROM library_sounds) >= 1
     BEGIN SELECT RAISE(ABORT, 'disk full'); END;
     CREATE TRIGGER keep_sounds BEFORE DELETE ON library_sounds
     BEGIN SELECT RAISE(ABORT, 'locked'); END;";

#[gpui_kit::test]
fn a_gamebanana_hud_downloads_into_the_library_and_toasts(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_gb_files(
        7,
        &[("flawhud.zip", support::hud_zip_bytes("flawhud"))],
        Duration::ZERO,
    );
    let (win, _) = support::store_window(
        cx,
        &env,
        &server,
        |env| support::seed_gb_huds(env, &[(7, "FlawHUD")]),
        |_, _| support::Blank,
    );
    let d = downloads(cx);
    let seen = events(cx, &d);
    let file = files(cx, GB, &gb(7)).remove(0);
    assert!(start(cx, &d, win, target(GB, gb(7), "FlawHUD"), file));
    assert!(cx.update(|cx| d.read(cx).is_downloading(GB, &gb(7))));
    wait_idle(cx, &d);
    support::wait_real(cx, "the toast", |cx| support::present(cx, win, ids::TOAST));
    assert_eq!(
        support::label_of(cx, win, ids::TOAST).as_deref(),
        Some("HUD downloaded. FlawHUD added to your library")
    );
    let huds = library(cx);
    assert_eq!(huds.len(), 1);
    assert!(
        matches!(huds[0].provenance(), Provenance::GameBanana { gb_id, .. } if *gb_id == GbId::new(7))
    );
    assert_eq!(
        *seen.borrow(),
        vec![DownloadEvent::Finished {
            target: target(GB, gb(7), "FlawHUD")
        }]
    );
    assert_eq!(
        cx.update(|cx| (d.read(cx).progress(), d.read(cx).active().cloned())),
        (None, None)
    );
}

#[gpui_kit::test]
fn a_failed_download_toasts_and_leaves_the_library_alone(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_gb_files(9, &[("broken.zip", b"not a zip".to_vec())], Duration::ZERO);
    let (win, _) = support::store_window(
        cx,
        &env,
        &server,
        |env| support::seed_gb_huds(env, &[(9, "Broken")]),
        |_, _| support::Blank,
    );
    let d = downloads(cx);
    let seen = events(cx, &d);
    let file = files(cx, GB, &gb(9)).remove(0);
    assert!(start(cx, &d, win, target(GB, gb(9), "Broken"), file));
    wait_idle(cx, &d);
    support::wait_real(cx, "the toast", |cx| support::present(cx, win, ids::TOAST));
    let label = support::label_of(cx, win, ids::TOAST).unwrap();
    assert!(label.starts_with("Failed to download HUD. "), "{label}");
    support::assert_plain(&label);
    assert!(
        matches!(&seen.borrow()[..], [DownloadEvent::Failed { message, .. }] if !message.contains("broken.zip")),
        "the event carries the plain message"
    );
    assert!(library(cx).is_empty());
    assert!(
        matches!(&seen.borrow()[..], [DownloadEvent::Failed { target: t, .. }] if t.item == gb(9))
    );
}

#[gpui_kit::test]
fn a_curated_hud_downloads_from_its_repo_and_redownloads_in_place(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let repo = server.mount_curated("rayshud", support::hud_zip_bytes("rayshud"), Duration::ZERO);
    let (win, _) = support::store_window(
        cx,
        &env,
        &server,
        move |env| support::seed_curated(env, "rayshud", "rayshud", "raysfire", Some(&repo), "{}"),
        |_, _| support::Blank,
    );
    let d = downloads(cx);
    let id = StoreItemId::Curated("rayshud".into());
    for _ in 0..2 {
        let file = files(cx, StoreKind::CuratedHuds, &id).remove(0);
        assert!(start(
            cx,
            &d,
            win,
            target(StoreKind::CuratedHuds, id.clone(), "rayshud"),
            file
        ));
        wait_idle(cx, &d);
    }
    let huds = library(cx);
    assert_eq!(huds.len(), 1, "a re-download replaces the HUD");
    assert_eq!(
        huds[0].provenance(),
        &Provenance::Curated {
            hud_id: "rayshud".into()
        }
    );
}

struct Buttons {
    clicked: Rc<Cell<u32>>,
}

impl Render for Buttons {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let clicked = self.clicked.clone();
        div().size_full().children([7u64, 8].map(|id| {
            let clicked = clicked.clone();
            download_button(
                GB,
                &gb(id),
                "Download".into(),
                false,
                false,
                move |_, _| clicked.set(clicked.get() + 1),
                cx,
            )
        }))
    }
}

#[gpui_kit::test]
fn one_download_at_a_time_with_progress_on_its_card(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_gb_files(
        7,
        &[("a.zip", support::hud_zip_bytes("ahud"))],
        Duration::from_millis(800),
    );
    server.mount_gb_files(
        8,
        &[("b.zip", support::hud_zip_bytes("bhud"))],
        Duration::ZERO,
    );
    let clicked = Rc::new(Cell::new(0));
    let c = clicked.clone();
    let (win, host) = support::store_window(
        cx,
        &env,
        &server,
        |env| support::seed_gb_huds(env, &[(7, "A"), (8, "B")]),
        move |_, _| Buttons { clicked: c },
    );
    assert_eq!(
        support::label_of(cx, win, ids::store_download(GB, &gb(8))).as_deref(),
        Some("Download")
    );
    support::click_id(cx, win, ids::store_download(GB, &gb(8)));
    assert_eq!(clicked.get(), 1);
    let d = downloads(cx);
    let (a, b) = (
        files(cx, GB, &gb(7)).remove(0),
        files(cx, GB, &gb(8)).remove(0),
    );
    assert!(start(cx, &d, win, target(GB, gb(7), "A"), a));
    assert!(
        !start(cx, &d, win, target(GB, gb(8), "B"), b.clone()),
        "the second waits"
    );
    assert_eq!(
        cx.update(|cx| d.read(cx).active().cloned()),
        Some(target(GB, gb(7), "A"))
    );
    cx.update(|cx| host.update(cx, |_, cx| cx.notify()));
    let label = support::label_of(cx, win, ids::store_download(GB, &gb(7))).unwrap();
    assert!(label.starts_with("Downloading..."), "{label}");
    assert!(support::present(cx, win, ids::STORE_PROGRESS));
    support::click_id(cx, win, ids::store_download(GB, &gb(8)));
    assert_eq!(clicked.get(), 1, "disabled while another runs");
    wait_idle(cx, &d);
    cx.update(|cx| host.update(cx, |_, cx| cx.notify()));
    support::click_id(cx, win, ids::store_download(GB, &gb(8)));
    assert_eq!(clicked.get(), 2);
    assert!(start(cx, &d, win, target(GB, gb(8), "B"), b));
    wait_idle(cx, &d);
    assert_eq!(library(cx).len(), 2);
}

// The busy guard refuses a second start and leaves the first running; every progress report
// lands while the entity still reads busy, so a card can draw it.
#[gpui_kit::test]
fn a_second_start_is_refused_and_progress_lands_mid_download(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_gb_files(
        7,
        &[("a.zip", support::hud_zip_bytes("ahud"))],
        Duration::from_millis(300),
    );
    server.mount_gb_files(
        8,
        &[("b.zip", support::hud_zip_bytes("bhud"))],
        Duration::ZERO,
    );
    let (win, _) = support::store_window(
        cx,
        &env,
        &server,
        |env| support::seed_gb_huds(env, &[(7, "A"), (8, "B")]),
        |_, _| support::Blank,
    );
    let d = downloads(cx);
    let seen = Rc::new(RefCell::new(Vec::<(bool, Option<Progress>)>::new()));
    let log = seen.clone();
    let _watch = cx.update(|cx| {
        cx.observe(&d, move |d, cx| {
            let d = d.read(cx);
            log.borrow_mut().push((d.is_busy(), d.progress()));
        })
    });
    let (a, b) = (
        files(cx, GB, &gb(7)).remove(0),
        files(cx, GB, &gb(8)).remove(0),
    );
    assert!(start(cx, &d, win, target(GB, gb(7), "A"), a));
    assert!(
        !start(cx, &d, win, target(GB, gb(8), "B"), b),
        "refused while busy"
    );
    assert_eq!(
        cx.update(|cx| d.read(cx).active().cloned()),
        Some(target(GB, gb(7), "A"))
    );
    assert!(!cx.update(|cx| d.read(cx).is_downloading(GB, &gb(8))));
    wait_idle(cx, &d);
    assert_eq!(library(cx).len(), 1, "only the first downloaded");
    let seen = seen.borrow().clone();
    assert!(
        seen.iter().any(|(busy, p)| *busy
            && matches!(
                p,
                Some(Progress {
                    phase: DownloadPhase::Downloading,
                    ..
                })
            )),
        "no download progress while busy: {seen:?}"
    );
    assert!(
        seen.iter().any(|(busy, p)| *busy
            && matches!(
                p,
                Some(Progress {
                    phase: DownloadPhase::Importing,
                    ..
                })
            )),
        "no import progress while busy: {seen:?}"
    );
    assert_eq!(seen.last(), Some(&(false, None)), "idle again: {seen:?}");
}

// Core's RollbackFailed lists the entries a failed undo left behind; the toast and the Failed
// event both name them after the player message.
#[gpui_kit::test]
fn a_failed_rollback_names_what_stayed(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_sound_files(
        5,
        &[("pack.zip", sound_pack(&["a.wav", "b.wav"]))],
        Duration::ZERO,
    );
    let (win, _) = support::store_window(
        cx,
        &env,
        &server,
        |env| {
            support::seed_sounds(env, &[(5, "Pack", "hitsound", None)]);
            rusqlite::Connection::open(env.data.db_path())
                .unwrap()
                .execute_batch(STUCK_ROLLBACK)
                .unwrap();
        },
        |_, _| support::Blank,
    );
    let d = downloads(cx);
    let seen = events(cx, &d);
    let file = files(cx, StoreKind::Sounds, &gb(5)).remove(0);
    assert!(start(
        cx,
        &d,
        win,
        target(StoreKind::Sounds, gb(5), "Pack"),
        file
    ));
    wait_idle(cx, &d);
    support::wait_real(cx, "the toast", |cx| support::present(cx, win, ids::TOAST));
    let toast = support::label_of(cx, win, ids::TOAST).unwrap();
    assert!(toast.starts_with("Failed to download sound. "), "{toast}");
    assert!(
        toast.ends_with(" Still in your library: Pack - a."),
        "{toast}"
    );
    // The triggers' raw words stay in the developer log; the whole toast is player text.
    support::assert_plain(&toast);
    assert!(
        !toast.contains("disk full") && !toast.contains("locked"),
        "{toast}"
    );
    let message = match &seen.borrow()[..] {
        [DownloadEvent::Failed { message, .. }] => message.clone(),
        other => panic!("expected one Failed event, got {other:?}"),
    };
    assert!(
        message.ends_with(" Still in your library: Pack - a."),
        "{message}"
    );
    let sounds = cx
        .update(|cx| cx.global::<Services>().library.list_sounds())
        .unwrap();
    assert_eq!(sounds.len(), 1, "the stuck sound stayed");
}

#[test]
fn a_size_that_rounds_to_the_next_unit_reads_in_it() {
    assert_eq!(format_bytes(1_023), "1023 B");
    assert_eq!(format_bytes(1_048_524), "1023.9 KB");
    assert_eq!(format_bytes(1_048_575), "1.0 MB");
    assert_eq!(format_bytes(1_073_741_823), "1.0 GB");
}
