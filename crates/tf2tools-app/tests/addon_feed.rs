use tf2tools_app::addon_feed::{AddonFeed, Keyed, StepAfter};

#[derive(Clone, Debug, PartialEq)]
struct Card(u64);

impl Keyed for Card {
    type Key = u64;
    fn key(&self) -> u64 {
        self.0
    }
}

fn cards(ids: &[u64]) -> Vec<Card> {
    ids.iter().copied().map(Card).collect()
}
fn ids(feed: &AddonFeed<Card, u32>) -> Vec<u64> {
    feed.items().iter().map(|c| c.0).collect()
}

#[test]
fn chunks_append_as_they_arrive_and_the_step_moves_the_cursor() {
    let mut feed = AddonFeed::new(1u32);
    let (from, g) = feed.begin_step().unwrap();
    assert_eq!(from, 1);
    assert!(feed.is_loading());
    assert_eq!(feed.begin_step(), None, "one step at a time");
    assert_eq!(feed.chunk(g, cards(&[1, 2])), 2);
    assert_eq!(ids(&feed), [1, 2]);
    assert_eq!(feed.chunk(g, cards(&[3])), 1);
    assert_eq!(feed.step_done(g, Some(4)), StepAfter::Idle);
    assert!(!feed.is_loading());
    assert_eq!(feed.begin_step(), Some((4, g)));
}

#[test]
fn a_repeated_card_is_dropped_and_does_not_count_as_added() {
    let mut feed = AddonFeed::new(1u32);
    let (_, g) = feed.begin_step().unwrap();
    feed.chunk(g, cards(&[1, 2]));
    assert_eq!(feed.step_done(g, Some(2)), StepAfter::Idle);
    feed.begin_step().unwrap();
    assert_eq!(feed.chunk(g, cards(&[2, 1])), 0);
    assert_eq!(ids(&feed), [1, 2]);
    assert_eq!(feed.step_done(g, Some(3)), StepAfter::Continue);
}

#[test]
fn a_dry_step_continues_once_then_offers_load_more() {
    let mut feed = AddonFeed::new(1u32);
    let (_, g) = feed.begin_step().unwrap();
    assert_eq!(feed.step_done(g, Some(5)), StepAfter::Continue);
    assert_eq!(feed.begin_step(), Some((5, g)));
    assert_eq!(feed.step_done(g, Some(9)), StepAfter::LoadMore);
    assert!(feed.wants_load_more() && !feed.is_no_results());
    assert_eq!(
        feed.begin_step(),
        None,
        "scrolling does not pass the button"
    );
    assert_eq!(feed.load_more(), Some((9, g)));
    assert_eq!(
        feed.step_done(g, Some(13)),
        StepAfter::LoadMore,
        "a clicked step never continues by itself"
    );
    feed.load_more().unwrap();
    feed.chunk(g, cards(&[7]));
    assert_eq!(feed.step_done(g, Some(17)), StepAfter::Idle);
    assert_eq!(feed.load_more(), None);
    feed.begin_step().unwrap();
    assert_eq!(
        feed.step_done(g, Some(21)),
        StepAfter::Continue,
        "cards reset the rule"
    );
}

#[test]
fn the_end_says_no_results_only_when_nothing_was_found() {
    let mut feed = AddonFeed::new(1u32);
    let (_, g) = feed.begin_step().unwrap();
    assert_eq!(feed.step_done(g, None), StepAfter::End);
    assert!(feed.is_end() && feed.is_no_results() && !feed.wants_load_more());
    assert_eq!(feed.begin_step(), None);
    let g = feed.reset(1);
    feed.begin_step().unwrap();
    feed.chunk(g, cards(&[1]));
    assert_eq!(feed.step_done(g, None), StepAfter::End);
    assert!(feed.is_end() && !feed.is_no_results());
}

#[test]
fn a_failed_step_keeps_the_cards_and_retries_from_the_same_cursor() {
    let mut feed = AddonFeed::new(1u32);
    let (_, g) = feed.begin_step().unwrap();
    feed.chunk(g, cards(&[1, 2]));
    feed.step_failed(g);
    assert!(!feed.is_loading());
    assert_eq!(ids(&feed), [1, 2]);
    assert_eq!(feed.begin_step(), Some((1, g)));
    assert_eq!(feed.chunk(g, cards(&[1, 2, 3])), 1);
    assert_eq!(feed.step_done(g, Some(5)), StepAfter::Idle);
}

#[test]
fn a_reset_drops_late_chunks_of_the_old_query() {
    let mut feed = AddonFeed::new(1u32);
    let (_, old) = feed.begin_step().unwrap();
    let new = feed.reset(1);
    assert_ne!(old, new);
    assert_eq!(feed.chunk(old, cards(&[1])), 0);
    assert!(feed.items().is_empty());
    assert_eq!(feed.begin_step(), Some((1, new)));
    assert_eq!(feed.step_done(old, Some(2)), StepAfter::Stale);
    feed.step_failed(old);
    assert!(feed.is_loading(), "the old query cannot end the new step");
}
