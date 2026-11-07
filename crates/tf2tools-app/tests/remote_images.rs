use crate::support;

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::{Entity, RenderImage, TestAppContext};
use image::{DynamicImage, Frame, ImageFormat, Rgba, RgbaImage};
use tf2tools_app::remote_images::decode::decode_image;
use tf2tools_app::remote_images::{
    DecodeFn, FetchFailure, ImageFetch, ImageKey, ImageTarget, RemoteImage, RemoteImages,
};
use tf2tools_app::tokio_bridge;

const CARD_BYTES: u64 = 480 * 270 * 4;

fn png(w: u32, h: u32) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(RgbaImage::from_pixel(w, h, Rgba([1, 2, 3, 255])))
        .write_to(&mut out, ImageFormat::Png)
        .unwrap();
    out.into_inner()
}

/// A fetcher that serves `served` (anything else is a permanent failure) and logs each URL asked.
fn fake(
    served: Vec<(&str, Result<Vec<u8>, FetchFailure>)>,
) -> (ImageFetch, Arc<Mutex<Vec<String>>>) {
    let asked = Arc::new(Mutex::new(Vec::new()));
    let map: Arc<HashMap<String, Result<Vec<u8>, FetchFailure>>> =
        Arc::new(served.into_iter().map(|(u, r)| (u.to_owned(), r)).collect());
    let log = asked.clone();
    let fetch: ImageFetch = Arc::new(move |url: String| {
        log.lock().unwrap().push(url.clone());
        let result = map
            .get(&url)
            .cloned()
            .unwrap_or(Err(FetchFailure::Permanent));
        Box::pin(async move { result })
    });
    (fetch, asked)
}

fn install(cx: &mut TestAppContext, fetch: ImageFetch, budget: u64) -> Entity<RemoteImages> {
    install_decoding(cx, fetch, budget, decode_image)
}

fn install_decoding(
    cx: &mut TestAppContext,
    fetch: ImageFetch,
    budget: u64,
    decode: DecodeFn,
) -> Entity<RemoteImages> {
    support::init(cx);
    // Fetches cross the tokio runtime, so the test executor must be allowed to park.
    cx.executor().allow_parking();
    cx.update(|cx| RemoteImages::install_with(cx, fetch, budget, decode))
}

/// A decoder with a bug: it panics on the bytes "boom".
fn boom(bytes: &[u8], target: ImageTarget) -> Option<Arc<RenderImage>> {
    assert!(bytes != b"boom", "decoder bug");
    decode_image(bytes, target)
}

/// A decoder whose images hold no frames, so no frame bytes.
fn frameless(_: &[u8], _: ImageTarget) -> Option<Arc<RenderImage>> {
    Some(Arc::new(RenderImage::new(Vec::<Frame>::new())))
}

fn want(cx: &mut TestAppContext, images: &Entity<RemoteImages>, viewer: &str, keys: Vec<ImageKey>) {
    cx.update(|cx| images.update(cx, |i, cx| i.want(viewer, keys, cx)));
}

fn state(cx: &mut TestAppContext, images: &Entity<RemoteImages>, key: &ImageKey) -> RemoteImage {
    cx.update(|cx| images.read(cx).image(key))
}

fn settled(cx: &mut TestAppContext, images: &Entity<RemoteImages>, key: &ImageKey) -> RemoteImage {
    support::wait_real(cx, "the image", |cx| {
        !matches!(state(cx, images, key), RemoteImage::Loading)
    });
    state(cx, images, key)
}

#[gpui_kit::test]
fn a_wanted_image_is_fetched_decoded_and_ready(cx: &mut TestAppContext) {
    let (fetch, asked) = fake(vec![("u/a", Ok(png(960, 540)))]);
    let images = install(cx, fetch, CARD_BYTES * 10);
    let key = ImageKey::card("u/a");
    assert!(matches!(state(cx, &images, &key), RemoteImage::Loading));
    want(cx, &images, "grid", vec![key.clone()]);
    let RemoteImage::Ready(image) = settled(cx, &images, &key) else {
        panic!("not ready");
    };
    let size = image.size(0);
    assert_eq!((i32::from(size.width), i32::from(size.height)), (480, 270));
    assert_eq!(*asked.lock().unwrap(), ["u/a"]);
    assert_eq!(cx.update(|cx| images.read(cx).stats().bytes), CARD_BYTES);
}

#[gpui_kit::test]
fn an_undecodable_image_fails_and_is_not_fetched_again(cx: &mut TestAppContext) {
    let (fetch, asked) = fake(vec![("u/bad", Ok(b"nope".to_vec()))]);
    let images = install(cx, fetch, CARD_BYTES);
    let key = ImageKey::card("u/bad");
    want(cx, &images, "grid", vec![key.clone()]);
    assert!(matches!(settled(cx, &images, &key), RemoteImage::Failed));
    want(cx, &images, "grid", vec![key.clone()]);
    cx.run_until_parked();
    assert_eq!(asked.lock().unwrap().len(), 1);
}

#[gpui_kit::test]
fn a_transient_failure_is_retried_after_thirty_seconds(cx: &mut TestAppContext) {
    let (fetch, asked) = fake(vec![("u/busy", Err(FetchFailure::Transient))]);
    let images = install(cx, fetch, CARD_BYTES);
    let key = ImageKey::card("u/busy");
    want(cx, &images, "grid", vec![key.clone()]);
    assert!(matches!(settled(cx, &images, &key), RemoteImage::Failed));
    want(cx, &images, "grid", vec![key.clone()]);
    cx.run_until_parked();
    assert_eq!(asked.lock().unwrap().len(), 1);
    cx.executor().advance_clock(Duration::from_secs(30));
    want(cx, &images, "grid", vec![key.clone()]);
    support::wait_real(cx, "the retry", |_| asked.lock().unwrap().len() == 2);
}

#[gpui_kit::test]
fn a_panicking_decode_fails_only_its_image(cx: &mut TestAppContext) {
    let (fetch, _) = fake(vec![
        ("u/boom", Ok(b"boom".to_vec())),
        ("u/ok", Ok(png(16, 9))),
    ]);
    let images = install_decoding(cx, fetch, CARD_BYTES, boom);
    let (bad, good) = (ImageKey::card("u/boom"), ImageKey::card("u/ok"));
    want(cx, &images, "grid", vec![bad.clone(), good.clone()]);
    assert!(matches!(settled(cx, &images, &bad), RemoteImage::Failed));
    assert!(matches!(settled(cx, &images, &good), RemoteImage::Ready(_)));
}

#[gpui_kit::test]
fn decodes_never_run_on_the_foreground(cx: &mut TestAppContext) {
    let urls = ["u/1", "u/2", "u/3", "u/4", "u/5"];
    let (fetch, _) = fake(urls.iter().map(|u| (*u, Ok(png(32, 18)))).collect());
    let images = install(cx, fetch, CARD_BYTES * 10);
    want(
        cx,
        &images,
        "grid",
        urls.iter().map(|u| ImageKey::card(*u)).collect(),
    );
    support::wait_real(cx, "five decodes", |cx| {
        cx.update(|cx| images.read(cx).stats().decodes == 5)
    });
    assert_eq!(
        cx.update(|cx| images.read(cx).stats().foreground_decodes),
        0
    );
}

#[gpui_kit::test]
fn the_least_recently_used_image_is_evicted_past_the_budget(cx: &mut TestAppContext) {
    let (fetch, _) = fake(vec![
        ("u/a", Ok(png(960, 540))),
        ("u/b", Ok(png(960, 540))),
        ("u/c", Ok(png(960, 540))),
    ]);
    let images = install(cx, fetch, CARD_BYTES * 2);
    let (a, b, c) = (
        ImageKey::card("u/a"),
        ImageKey::card("u/b"),
        ImageKey::card("u/c"),
    );
    want(cx, &images, "grid", vec![a.clone(), b.clone()]);
    settled(cx, &images, &a);
    settled(cx, &images, &b);
    assert!(matches!(state(cx, &images, &a), RemoteImage::Ready(_)));
    want(cx, &images, "grid", vec![c.clone()]);
    assert!(matches!(settled(cx, &images, &c), RemoteImage::Ready(_)));
    let stats = cx.update(|cx| images.read(cx).stats());
    assert_eq!(
        (stats.evicted, stats.released, stats.bytes),
        (1, 1, CARD_BYTES * 2)
    );
    assert!(matches!(state(cx, &images, &b), RemoteImage::Loading));
    assert!(matches!(state(cx, &images, &a), RemoteImage::Ready(_)));
}

#[gpui_kit::test]
fn an_image_without_frame_bytes_still_counts_against_the_budget(cx: &mut TestAppContext) {
    let (fetch, _) = fake(vec![("u/a", Ok(b"a".to_vec())), ("u/b", Ok(b"b".to_vec()))]);
    let images = install_decoding(cx, fetch, 1, frameless);
    let (a, b) = (ImageKey::card("u/a"), ImageKey::card("u/b"));
    want(cx, &images, "grid", vec![a.clone()]);
    assert!(matches!(settled(cx, &images, &a), RemoteImage::Ready(_)));
    assert_eq!(cx.update(|cx| images.read(cx).stats().bytes), 1);
    want(cx, &images, "grid", vec![b.clone()]);
    assert!(matches!(settled(cx, &images, &b), RemoteImage::Ready(_)));
    let stats = cx.update(|cx| images.read(cx).stats());
    assert_eq!(
        (stats.evicted, stats.bytes),
        (1, 1),
        "the unwanted image goes"
    );
}

#[gpui_kit::test]
fn the_runtime_starts_only_for_an_image_to_fetch(cx: &mut TestAppContext) {
    let (fetch, asked) = fake(vec![("u/ok", Ok(png(16, 9)))]);
    let images = install(cx, fetch, CARD_BYTES);
    let running = |cx: &mut TestAppContext| cx.update(|cx| tokio_bridge::is_running(cx));
    want(cx, &images, "grid", vec![]);
    cx.run_until_parked();
    assert!(!running(cx), "an empty want starts nothing");

    let (ok, bad) = (ImageKey::card("u/ok"), ImageKey::card("u/bad"));
    want(cx, &images, "grid", vec![ok.clone(), bad.clone()]);
    assert!(running(cx), "a fetch starts the runtime");
    assert!(matches!(settled(cx, &images, &ok), RemoteImage::Ready(_)));
    assert!(matches!(settled(cx, &images, &bad), RemoteImage::Failed));

    // With the runtime gone, a want of only cached and failed keys must not restart it.
    cx.update(tokio_bridge::shutdown);
    want(cx, &images, "grid", vec![ok, bad]);
    cx.run_until_parked();
    assert!(!running(cx), "nothing to fetch starts nothing");
    assert_eq!(asked.lock().unwrap().len(), 2);
}

/// Sets its flag when dropped, so a test sees a fetch future go.
struct DropFlag(Arc<AtomicBool>);

impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

// The fetch never ends on its own, so its future is dropped only if the cache's drop aborts it.
#[gpui_kit::test]
fn dropping_the_cache_cancels_its_fetches(cx: &mut TestAppContext) {
    let started = Arc::new(AtomicBool::new(false));
    let dropped = Arc::new(AtomicBool::new(false));
    let (on_start, on_drop) = (started.clone(), dropped.clone());
    let fetch: ImageFetch = Arc::new(move |_url: String| {
        let (on_start, guard) = (on_start.clone(), DropFlag(on_drop.clone()));
        Box::pin(async move {
            let _guard = guard;
            on_start.store(true, Ordering::SeqCst);
            std::future::pending::<()>().await;
            Ok(Vec::new())
        })
    });
    let images = install(cx, fetch, CARD_BYTES);
    want(cx, &images, "grid", vec![ImageKey::card("u/slow")]);
    support::wait_real(cx, "the fetch to start", |_| started.load(Ordering::SeqCst));
    cx.run_until_parked();
    assert!(
        !dropped.load(Ordering::SeqCst),
        "the fetch is still running"
    );
    // The global holds the cache too: a second install replaces it, so this releases the first.
    let (other, _) = fake(vec![]);
    cx.update(|cx| {
        RemoteImages::install(cx, other);
        drop(images);
    });
    support::wait_real(cx, "the fetch future to drop", |_| {
        dropped.load(Ordering::SeqCst)
    });
}
