use std::io::Cursor;
use std::ops::Range;
use std::time::{Duration, Instant};

use image::{DynamicImage, ImageFormat, Rgb, RgbImage, Rgba, RgbaImage};
use tf2tools_app::remote_images::decode::decode_image;
use tf2tools_app::remote_images::queue::{
    ImageQueue, ImageState, MAX_DECODES, MAX_FETCHES, RETRY_AFTER,
};
use tf2tools_app::remote_images::{FetchFailure, ImageKey, ImageTarget};

fn k(n: usize) -> ImageKey {
    ImageKey::card(format!("https://images.gamebanana.com/{n}.jpg"))
}

fn keys(range: Range<usize>) -> Vec<ImageKey> {
    range.map(k).collect()
}

/// Wants, fetches and decodes `key` as `image` of `bytes`; returns the evicted.
fn ready(q: &mut ImageQueue<u32>, key: ImageKey, image: u32, bytes: u64) -> Vec<u32> {
    q.want("v", vec![key.clone()], Instant::now());
    assert_eq!(q.next_fetch(), Some(key.clone()));
    q.fetched(&key, Ok(vec![1]), Instant::now());
    assert_eq!(q.next_decode().map(|(k, _)| k), Some(key.clone()));
    q.decoded(&key, Some((image, bytes)))
}

fn encode(image: DynamicImage, format: ImageFormat) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    image.write_to(&mut out, format).unwrap();
    out.into_inner()
}

fn size_of(bytes: &[u8], target: ImageTarget) -> (i32, i32) {
    let image = decode_image(bytes, target).expect("decodes");
    let size = image.size(0);
    (i32::from(size.width), i32::from(size.height))
}

#[test]
fn targets_and_limits_follow_the_spec() {
    assert_eq!(ImageTarget::Card.bounds(), (480, 270));
    assert_eq!(ImageTarget::Carousel.bounds(), (1280, 720));
    assert_eq!((MAX_FETCHES, MAX_DECODES), (6, 4));
}

#[test]
fn the_newest_request_is_served_first() {
    let now = Instant::now();
    let mut q = ImageQueue::<u32>::new(1_000);
    q.want("a", keys(0..2), now);
    q.want("b", vec![k(2)], now);
    assert_eq!(q.next_fetch(), Some(k(2)));
    assert_eq!(q.next_fetch(), Some(k(0)));
    assert_eq!(q.next_fetch(), Some(k(1)));
    assert_eq!(q.next_fetch(), None);
}

#[test]
fn at_most_six_fetches_run_at_once() {
    let now = Instant::now();
    let mut q = ImageQueue::<u32>::new(1_000);
    q.want("a", keys(0..10), now);
    for i in 0..6 {
        assert_eq!(q.next_fetch(), Some(k(i)));
    }
    assert_eq!(q.next_fetch(), None);
    assert_eq!(q.fetching(), 6);
    q.fetched(&k(0), Ok(vec![1]), now);
    assert_eq!(q.next_fetch(), Some(k(6)));
}

#[test]
fn a_request_that_left_the_view_before_its_fetch_is_dropped() {
    let now = Instant::now();
    let mut q = ImageQueue::<u32>::new(1_000);
    q.want("g", keys(0..8), now);
    for _ in 0..6 {
        q.next_fetch().unwrap();
    }
    q.want("g", vec![k(9)], now);
    assert_eq!(q.queued(), 1);
    q.fetched(&k(0), Ok(vec![1]), now);
    assert_eq!(q.next_fetch(), Some(k(9)));
    assert_eq!(q.next_fetch(), None);
}

#[test]
fn one_image_wanted_twice_is_fetched_once() {
    let now = Instant::now();
    let mut q = ImageQueue::<u32>::new(1_000);
    q.want("a", vec![k(0)], now);
    q.want("b", vec![k(0)], now);
    assert_eq!(q.next_fetch(), Some(k(0)));
    assert_eq!(q.next_fetch(), None);
    q.want("c", vec![k(0)], now);
    assert_eq!(q.next_fetch(), None);
    // A card and a carousel of one URL are two images.
    let carousel = ImageKey::carousel(k(0).url);
    q.want("d", vec![carousel.clone()], now);
    assert_eq!(q.next_fetch(), Some(carousel));
}

#[test]
fn at_most_four_decodes_run_at_once() {
    let now = Instant::now();
    let mut q = ImageQueue::<u32>::new(1_000);
    q.want("a", keys(0..6), now);
    for i in 0..6 {
        let key = q.next_fetch().unwrap();
        q.fetched(&key, Ok(vec![i as u8]), now);
    }
    for i in 0..4 {
        assert_eq!(q.next_decode(), Some((k(i), vec![i as u8])));
    }
    assert_eq!(q.next_decode(), None);
    assert!(q.decoded(&k(0), Some((7, 10))).is_empty());
    assert_eq!(q.next_decode(), Some((k(4), vec![4])));
    assert_eq!(q.get(&k(0)), ImageState::Ready(7));
    assert_eq!(q.get(&k(5)), ImageState::Loading);
}

#[test]
fn the_least_recently_used_image_is_evicted_past_the_budget() {
    let mut q = ImageQueue::<u32>::new(250);
    assert!(ready(&mut q, k(1), 1, 100).is_empty());
    assert!(ready(&mut q, k(2), 2, 100).is_empty());
    assert_eq!(q.get(&k(1)), ImageState::Ready(1));
    assert_eq!(ready(&mut q, k(3), 3, 100), vec![2]);
    assert_eq!(q.get(&k(2)), ImageState::Loading);
    assert_eq!(q.bytes(), 200);
    q.want("v", vec![k(2)], Instant::now());
    assert_eq!(q.next_fetch(), Some(k(2)));
}

#[test]
fn a_permanent_failure_is_never_retried_this_session() {
    let now = Instant::now();
    let mut q = ImageQueue::<u32>::new(1_000);
    q.want("a", keys(0..2), now);
    let a = q.next_fetch().unwrap();
    q.fetched(&a, Err(FetchFailure::Permanent), now);
    let b = q.next_fetch().unwrap();
    q.fetched(&b, Ok(b"x".to_vec()), now);
    let (key, _) = q.next_decode().unwrap();
    assert!(q.decoded(&key, None).is_empty());
    assert_eq!(q.get(&k(0)), ImageState::Failed);
    assert_eq!(q.get(&k(1)), ImageState::Failed);
    q.want("a", keys(0..2), now + Duration::from_secs(3600));
    assert_eq!(q.next_fetch(), None);
}

#[test]
fn a_transient_failure_is_retried_after_the_pause() {
    let now = Instant::now();
    let mut q = ImageQueue::<u32>::new(1_000);
    q.want("a", vec![k(0)], now);
    let a = q.next_fetch().unwrap();
    q.fetched(&a, Err(FetchFailure::Transient), now);
    assert_eq!(q.get(&k(0)), ImageState::Failed);
    q.want(
        "a",
        vec![k(0)],
        now + RETRY_AFTER - Duration::from_millis(1),
    );
    assert_eq!(q.next_fetch(), None);
    q.want("a", vec![k(0)], now + RETRY_AFTER);
    assert_eq!(q.next_fetch(), Some(k(0)));
}

#[test]
fn a_large_image_is_downsized_into_its_box_as_bgra() {
    let png = encode(
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(960, 540, Rgba([10, 20, 30, 255]))),
        ImageFormat::Png,
    );
    assert_eq!(size_of(&png, ImageTarget::Card), (480, 270));
    let image = decode_image(&png, ImageTarget::Card).unwrap();
    assert_eq!(&image.as_bytes(0).unwrap()[..4], &[30, 20, 10, 255]);
}

#[test]
fn a_small_image_keeps_its_size() {
    let png = encode(
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(100, 56, Rgba([1, 2, 3, 255]))),
        ImageFormat::Png,
    );
    assert_eq!(size_of(&png, ImageTarget::Card), (100, 56));
}

#[test]
fn a_tall_jpeg_fits_the_carousel_height() {
    let jpeg = encode(
        DynamicImage::ImageRgb8(RgbImage::from_pixel(300, 1000, Rgb([9, 9, 9]))),
        ImageFormat::Jpeg,
    );
    assert_eq!(size_of(&jpeg, ImageTarget::Carousel), (216, 720));
}

#[test]
fn webp_decodes_and_bytes_that_are_no_image_do_not() {
    let webp = encode(
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(64, 36, Rgba([5, 6, 7, 255]))),
        ImageFormat::WebP,
    );
    assert_eq!(size_of(&webp, ImageTarget::Card), (64, 36));
    assert!(decode_image(b"nope", ImageTarget::Card).is_none());
}

/// Fetches and decodes `key`, already wanted and next in the queue, as `image` of `bytes`.
fn fetch_decode(q: &mut ImageQueue<u32>, key: &ImageKey, image: u32, bytes: u64) -> Vec<u32> {
    assert_eq!(q.next_fetch(), Some(key.clone()));
    q.fetched(key, Ok(vec![1]), Instant::now());
    assert_eq!(q.next_decode().map(|(k, _)| k), Some(key.clone()));
    q.decoded(key, Some((image, bytes)))
}

#[test]
fn an_image_no_viewer_wants_is_evicted_before_a_wanted_one() {
    let now = Instant::now();
    let mut q = ImageQueue::<u32>::new(250);
    // A static carousel wants image 1 and never asks again.
    q.want("carousel", vec![k(1)], now);
    assert!(fetch_decode(&mut q, &k(1), 1, 100).is_empty());
    q.want("grid", vec![k(2)], now);
    assert!(fetch_decode(&mut q, &k(2), 2, 100).is_empty());
    // Image 2 was drawn last, but the grid has moved on from it.
    assert_eq!(q.get(&k(2)), ImageState::Ready(2));
    q.want("grid", vec![k(3)], now);
    assert_eq!(fetch_decode(&mut q, &k(3), 3, 100), vec![2]);
    assert_eq!(q.get(&k(1)), ImageState::Ready(1));
    assert_eq!(q.queued(), 0);
}

#[test]
fn wanted_images_over_the_budget_evict_once_without_a_refetch_loop() {
    let now = Instant::now();
    let mut q = ImageQueue::<u32>::new(250);
    q.want("carousel", vec![k(1)], now);
    assert!(fetch_decode(&mut q, &k(1), 1, 100).is_empty());
    q.want("grid", vec![k(2)], now);
    assert!(fetch_decode(&mut q, &k(2), 2, 100).is_empty());
    // Every image is wanted and the three overrun the budget: the least recently used goes,
    // and nothing is queued to fetch it back and evict the next.
    q.want("grid", vec![k(2), k(3)], now);
    assert_eq!(fetch_decode(&mut q, &k(3), 3, 100), vec![1]);
    assert_eq!(q.get(&k(1)), ImageState::Loading);
    assert_eq!(q.queued(), 0);
    assert_eq!(q.next_fetch(), None);
    assert_eq!(q.bytes(), 200);
}

#[test]
fn an_empty_want_forgets_the_viewer() {
    let now = Instant::now();
    let mut q = ImageQueue::<u32>::new(1_000);
    q.want("a", keys(0..2), now);
    q.want("b", vec![k(2)], now);
    assert_eq!(q.viewers(), 2);
    q.want("a", Vec::new(), now);
    assert_eq!(q.viewers(), 1);
    q.want("never-wanted", Vec::new(), now);
    assert_eq!(q.viewers(), 1);
    assert_eq!(q.next_fetch(), Some(k(2)));
    assert_eq!(q.next_fetch(), None);
}
