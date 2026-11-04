use std::time::{Duration, Instant};

use tf2tools_app::store_grid::feed::{
    Feed, FeedItem, Gate, PAGE, REQUERY_INTERVAL, Throttle, anchored_scroll, first_visible, reload,
};

#[derive(Clone, Debug, PartialEq)]
struct Item(u32);

impl FeedItem for Item {
    type Id = u32;
    type Cursor = u32;
    fn feed_id(&self) -> u32 {
        self.0
    }
    fn feed_cursor(&self) -> u32 {
        self.0
    }
}

/// Items with ids `range`, highest first, as a keyset page lists them.
fn items(range: std::ops::Range<u32>) -> Vec<Item> {
    range.rev().map(Item).collect()
}

/// A keyset source over `ids` (highest first): the ids below `after`, at most `limit`.
fn pager(ids: Vec<u32>) -> impl FnMut(Option<u32>, u32) -> Result<Vec<Item>, String> {
    move |after, limit| {
        Ok(ids
            .iter()
            .copied()
            .filter(|id| after.is_none_or(|a| *id < a))
            .take(limit as usize)
            .map(Item)
            .collect())
    }
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[test]
fn pages_append_until_an_empty_page_and_a_short_page_is_not_the_end() {
    assert_eq!(PAGE, 60);
    let mut feed = Feed::<Item>::new();
    let (after, generation) = feed.next_page().unwrap();
    assert_eq!(after, None);
    assert!(feed.next_page().is_none(), "one page at a time");
    assert!(feed.page_loaded(generation, items(100..160)));
    let (after, generation) = feed.next_page().unwrap();
    assert_eq!(after, Some(100));
    assert!(feed.page_loaded(generation, items(95..100)));
    assert!(!feed.is_end(), "a short page is not the end");
    let (after, generation) = feed.next_page().unwrap();
    assert_eq!(after, Some(95));
    assert!(feed.page_loaded(generation, Vec::new()));
    assert!(feed.is_end());
    assert!(feed.next_page().is_none());
    assert_eq!(feed.items().len(), 65);
}

#[test]
fn a_page_from_an_older_query_is_ignored() {
    let mut feed = Feed::<Item>::new();
    let (_, old) = feed.next_page().unwrap();
    assert!(feed.next_page().is_none(), "a page is in flight");
    let new = feed.reset();
    assert!(
        !feed.take_more_wanted(),
        "a reset drops a near-end refused under the old query"
    );
    assert!(!feed.page_loaded(old, items(0..60)));
    assert!(feed.items().is_empty());
    let (after, generation) = feed.next_page().unwrap();
    assert_eq!((after, generation), (None, new));
    assert!(!feed.reloaded(old, items(0..10), false));
}

#[test]
fn pages_and_reloads_never_overlap() {
    let mut feed = Feed::<Item>::new();
    let (_, generation) = feed.next_page().unwrap();
    assert!(feed.begin_reload().is_none(), "a page is in flight");
    assert!(feed.page_loaded(generation, items(100..160)));
    let reload = feed.begin_reload().unwrap();
    assert!(feed.next_page().is_none(), "a reload is in flight");
    assert!(feed.reloaded(reload, items(100..168), false));
    // The refused near-end request is remembered once, so the grid can ask again.
    assert!(feed.take_more_wanted());
    assert!(!feed.take_more_wanted());
    assert_eq!(feed.items().len(), 68);
    assert_eq!(feed.next_page().map(|(after, _)| after), Some(Some(100)));
    feed.page_failed(feed.generation());
    let reload = feed.begin_reload().unwrap();
    feed.reset();
    assert!(
        !feed.reloaded(reload, items(0..10), false),
        "a reset outdates the reload"
    );
    let reload = feed.begin_reload().unwrap();
    feed.reload_failed(reload);
    assert!(feed.next_page().is_some());
}

#[test]
fn a_failed_page_can_be_asked_for_again() {
    let mut feed = Feed::<Item>::new();
    let (_, generation) = feed.next_page().unwrap();
    assert!(feed.page_loaded(generation, items(0..60)));
    let (after, generation) = feed.next_page().unwrap();
    feed.page_failed(generation);
    assert_eq!(feed.next_page().map(|(a, _)| a), Some(after));
}

#[test]
fn a_reload_reaches_the_anchor_even_when_new_items_push_it_down() {
    // 80 new items (300..380) arrived above the 60 that were loaded (299..240).
    let (got, end) =
        reload::<Item, String>(60, Some(&250), pager((0..380).rev().collect())).unwrap();
    assert_eq!(got.len(), 180);
    assert!(got.iter().any(|i| i.0 == 250));
    assert!(!end);
}

#[test]
fn a_reload_never_loads_fewer_than_were_loaded() {
    let (got, end) = reload::<Item, String>(130, None, pager((0..500).rev().collect())).unwrap();
    assert_eq!(got.len(), 130);
    assert!(!end);
}

#[test]
fn a_reload_with_a_vanished_anchor_stops_at_the_end() {
    let (got, end) =
        reload::<Item, String>(60, Some(&9999), pager((0..100).rev().collect())).unwrap();
    assert_eq!(got.len(), 100);
    assert!(end);
}

#[test]
fn the_anchor_keeps_its_place_in_the_viewport() {
    // Four columns, 200 px rows, 450 px down: row 2 is first visible with 50 px hidden; its first card is 8.
    let old: Vec<u32> = (0..40).collect();
    let mut new: Vec<u32> = (100..106).collect();
    new.extend(0..40);
    // Six new cards above move card 8 to index 14, row 3.
    assert_eq!(anchored_scroll(450., 200., 4, &old, &new), 650.);
    assert_eq!(anchored_scroll(0., 200., 4, &old, &new), 200.);
    let gone: Vec<u32> = (9..40).collect();
    assert_eq!(anchored_scroll(450., 200., 4, &old, &gone), 450.);
    assert_eq!(anchored_scroll(450., 200., 4, &[] as &[u32], &new), 450.);
}

#[test]
fn requeries_run_at_most_once_a_second_with_one_trailing_run() {
    assert_eq!(REQUERY_INTERVAL, Duration::from_secs(1));
    let t0 = Instant::now();
    let mut t = Throttle::new(REQUERY_INTERVAL);
    assert_eq!(t.request(t0), Gate::Now);
    t.ran(t0);
    assert_eq!(t.request(t0 + ms(300)), Gate::After(ms(700)));
    assert_eq!(t.request(t0 + ms(500)), Gate::Pending);
    t.ran(t0 + ms(1000));
    assert_eq!(t.request(t0 + ms(1500)), Gate::After(ms(500)));
    t.ran(t0 + ms(2000));
    assert_eq!(t.request(t0 + ms(3000)), Gate::Now);
}

#[test]
fn the_row_maths_survive_absurd_and_nan_offsets() {
    // f32::MAX rows saturate to usize::MAX, and three columns of that overflow.
    assert_eq!(first_visible(f32::MAX, 1., 3), None);
    assert_eq!(first_visible(f32::INFINITY, 200., 4), None);
    assert_eq!(first_visible(f32::NAN, 200., 4), None);
    assert_eq!(first_visible(450., f32::NAN, 4), None);
    assert_eq!(first_visible(450., f32::INFINITY, 4), None);
    assert_eq!(first_visible(450., 200., 4), Some(8));
    let ids: Vec<u32> = (0..40).collect();
    assert_eq!(anchored_scroll(f32::NAN, 200., 4, &ids, &ids), 0.);
    assert_eq!(anchored_scroll(f32::INFINITY, 200., 4, &ids, &ids), 0.);
    assert_eq!(anchored_scroll(f32::MAX, 1., 3, &ids, &ids), f32::MAX);
}

#[test]
fn a_viewport_past_the_reloaded_prefix_follows_the_nearest_card_above_it() {
    // Four columns, 200 px rows, 1250 px down: card 24 is first visible with 50 px hidden.
    let old: Vec<u32> = (0..40).collect();
    // The reload stopped at its start anchor, card 19, with six new cards above it.
    let mut new: Vec<u32> = (100..106).collect();
    new.extend(0..20);
    // Card 19 moved six places, so card 24 belongs at index 30, row 7.
    assert_eq!(anchored_scroll(1250., 200., 4, &old, &new), 1450.);
    // Scrolled back above the start anchor, the first visible card is in the reload itself.
    assert_eq!(anchored_scroll(450., 200., 4, &old, &new), 650.);
}

#[test]
fn a_duplicated_card_id_anchors_on_its_first_place() {
    // Card 8, first visible at 450 px with 50 px hidden, appears at index 0 and again at 12.
    let old: Vec<u32> = (0..40).collect();
    let mut new: Vec<u32> = vec![8, 100, 101, 102];
    new.extend(0..40);
    assert_eq!(anchored_scroll(450., 200., 4, &old, &new), 50.);
}

#[test]
fn a_reload_refused_during_a_page_is_still_wanted_after_the_page_fails() {
    let mut feed = Feed::<Item>::new();
    let (_, generation) = feed.next_page().unwrap();
    assert!(feed.begin_reload().is_none(), "a page is in flight");
    feed.page_failed(generation);
    assert!(feed.take_reload_wanted());
    assert!(!feed.take_reload_wanted(), "handed out once");
    assert!(feed.begin_reload().is_some());
}

#[test]
fn a_reload_refused_during_a_page_is_wanted_after_it_lands() {
    let mut feed = Feed::<Item>::new();
    let (_, generation) = feed.next_page().unwrap();
    assert!(!feed.take_reload_wanted());
    assert!(feed.begin_reload().is_none());
    assert!(feed.page_loaded(generation, items(0..60)));
    assert!(feed.take_reload_wanted());
}

#[test]
fn a_reset_forgets_a_wanted_reload() {
    let mut feed = Feed::<Item>::new();
    feed.next_page().unwrap();
    assert!(feed.begin_reload().is_none());
    feed.reset();
    assert!(!feed.take_reload_wanted());
}
