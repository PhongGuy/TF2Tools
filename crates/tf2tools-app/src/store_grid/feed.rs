//! The pure model behind a store grid: keyset pages, an anchored reload from the top, and the
//! throttle that limits re-queries while a sync streams rows in.

use std::collections::HashMap;
use std::hash::Hash;
use std::time::{Duration, Instant};

use tf2tools_core::{StoreCard, StoreCursor, StoreItemId};

/// Cards per keyset page.
pub const PAGE: u32 = 60;

/// The shortest gap between two re-queries of a grid.
pub const REQUERY_INTERVAL: Duration = Duration::from_secs(1);

/// An item a feed can page over: a stable id to anchor on and a cursor to read past it.
pub trait FeedItem {
    type Id: PartialEq + Clone;
    type Cursor: Clone;
    fn feed_id(&self) -> Self::Id;
    fn feed_cursor(&self) -> Self::Cursor;
}

impl FeedItem for StoreCard {
    type Id = StoreItemId;
    type Cursor = StoreCursor;

    fn feed_id(&self) -> StoreItemId {
        self.id.clone()
    }

    fn feed_cursor(&self) -> StoreCursor {
        self.cursor.clone()
    }
}

/// The loaded prefix of one query. Every page and reload carries the generation it was asked
/// under, so a result that lands after a reset is dropped instead of mixing two queries.
pub struct Feed<T: FeedItem> {
    items: Vec<T>,
    end: bool,
    loading: bool,
    reloading: bool,
    more_wanted: bool,
    reload_wanted: bool,
    generation: u64,
}

impl<T: FeedItem> Default for Feed<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: FeedItem> Feed<T> {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            end: false,
            loading: false,
            reloading: false,
            more_wanted: false,
            reload_wanted: false,
            generation: 0,
        }
    }

    pub fn items(&self) -> &[T] {
        &self.items
    }

    pub fn is_end(&self) -> bool {
        self.end
    }

    pub fn is_loading(&self) -> bool {
        self.loading
    }

    /// True while a re-read from the top is in flight.
    pub fn is_reloading(&self) -> bool {
        self.reloading
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The cursor after the last item and the generation, unless a page or reload is in flight
    /// or at the end; marks loading.
    pub fn next_page(&mut self) -> Option<(Option<T::Cursor>, u64)> {
        if self.end {
            return None;
        }
        // A page and a reload never overlap, or the page would append to a prefix the reload
        // is about to replace. Remember the refusal so the grid can ask again afterwards.
        if self.loading || self.reloading {
            self.more_wanted = true;
            return None;
        }
        self.loading = true;
        Some((self.items.last().map(T::feed_cursor), self.generation))
    }

    /// Appends a page of this generation; an empty page is the end. False (ignored) for an
    /// older generation.
    pub fn page_loaded(&mut self, generation: u64, page: Vec<T>) -> bool {
        if generation != self.generation {
            return false;
        }
        self.loading = false;
        if page.is_empty() {
            self.end = true;
        }
        self.items.extend(page);
        true
    }

    /// Clears loading for this generation, so the same page can be asked for again.
    pub fn page_failed(&mut self, generation: u64) {
        if generation == self.generation {
            self.loading = false;
        }
    }

    /// Starts a new query: clears items, the end, loading, reloading, a refused near-end and a
    /// refused reload; returns the new generation.
    pub fn reset(&mut self) -> u64 {
        self.items.clear();
        self.end = false;
        self.loading = false;
        self.reloading = false;
        self.more_wanted = false;
        self.reload_wanted = false;
        self.generation += 1;
        self.generation
    }

    /// The generation to reload under, unless a page or reload is in flight; a refusal is
    /// remembered for `take_reload_wanted`. Marks reloading.
    pub fn begin_reload(&mut self) -> Option<u64> {
        if self.loading || self.reloading {
            self.reload_wanted = true;
            return None;
        }
        self.reloading = true;
        Some(self.generation)
    }

    /// Replaces the items with a reload of this generation and clears reloading; false for an
    /// older one.
    pub fn reloaded(&mut self, generation: u64, items: Vec<T>, end: bool) -> bool {
        if generation != self.generation {
            return false;
        }
        self.items = items;
        self.end = end;
        self.reloading = false;
        true
    }

    /// Clears reloading for this generation; the items stay as they were.
    pub fn reload_failed(&mut self, generation: u64) {
        if generation == self.generation {
            self.reloading = false;
        }
    }

    /// True once after `next_page` was refused because a page or reload was in flight (not at
    /// the end).
    pub fn take_more_wanted(&mut self) -> bool {
        std::mem::take(&mut self.more_wanted)
    }

    /// True once after `begin_reload` was refused; `reset` forgets it.
    pub fn take_reload_wanted(&mut self) -> bool {
        std::mem::take(&mut self.reload_wanted)
    }
}

/// Re-reads from the top: one call of max(loaded, PAGE), then PAGE more while `anchor` is
/// unseen and the last page was not empty. `end` is true when a page came back empty.
pub fn reload<T: FeedItem, E>(
    loaded: usize,
    anchor: Option<&T::Id>,
    mut page: impl FnMut(Option<T::Cursor>, u32) -> Result<Vec<T>, E>,
) -> Result<(Vec<T>, bool), E> {
    let first = u32::try_from(loaded).unwrap_or(u32::MAX).max(PAGE);
    let mut items = page(None, first)?;
    let mut end = items.is_empty();
    let seen = |items: &[T]| anchor.is_none_or(|a| items.iter().any(|i| i.feed_id() == *a));
    // Rows a sync inserts above the anchor push it further down; keep paging until it is back
    // in the loaded set, so the viewport can stay on it.
    while !end && !seen(&items) {
        let after = items.last().map(T::feed_cursor);
        let more = page(after, PAGE)?;
        end = more.is_empty();
        items.extend(more);
    }
    Ok((items, end))
}

/// The index of the first card of the row at `scroll_y`; None without rows or columns, for a
/// non-finite offset or row height, or when the index overflows.
pub fn first_visible(scroll_y: f32, row_height: f32, columns: usize) -> Option<usize> {
    if !(scroll_y.is_finite() && row_height.is_finite()) {
        return None;
    }
    if columns == 0 || row_height <= 0. || scroll_y < 0. {
        return None;
    }
    // The float-to-int cast saturates, so only the multiply can overflow.
    ((scroll_y / row_height).floor() as usize).checked_mul(columns)
}

/// The scroll distance that keeps the first visible card in place, or, when the reload lost
/// it, follows the nearest earlier card that survived; unchanged when none did, 0 for a
/// non-finite `scroll_y`.
pub fn anchored_scroll<Id: Eq + Hash>(
    scroll_y: f32,
    row_height: f32,
    columns: usize,
    old: &[Id],
    new: &[Id],
) -> f32 {
    if !scroll_y.is_finite() {
        return 0.;
    }
    let Some(first) = first_visible(scroll_y, row_height, columns) else {
        return scroll_y;
    };
    if first >= old.len() {
        return scroll_y;
    }
    // A reload that stopped at its start anchor can end above the viewport; keep the first
    // visible card's distance from the nearest card above it that the reload still holds.
    // A duplicated id keeps its first place, as a linear search would.
    let mut positions: HashMap<&Id, usize> = HashMap::with_capacity(new.len());
    for (i, id) in new.iter().enumerate() {
        positions.entry(id).or_insert(i);
    }
    let Some((found_old, found_new)) = old[..=first]
        .iter()
        .enumerate()
        .rev()
        .find_map(|(i, id)| positions.get(id).map(|&n| (i, n)))
    else {
        return scroll_y;
    };
    let index = found_new + (first - found_old);
    let row = (first / columns) as f32;
    let new_row = (index / columns) as f32;
    new_row * row_height + (scroll_y - row * row_height)
}

/// What a throttled request should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    /// Run now.
    Now,
    /// Schedule one trailing run after this delay.
    After(Duration),
    /// A trailing run is already scheduled; it will cover this request.
    Pending,
}

/// Limits a job to one run per interval, folding requests in between into one trailing run.
pub struct Throttle {
    interval: Duration,
    last: Option<Instant>,
    pending: bool,
}

impl Throttle {
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last: None,
            pending: false,
        }
    }

    /// Decides a request at `now`; the caller reports each actual run through `ran`.
    pub fn request(&mut self, now: Instant) -> Gate {
        if self.pending {
            return Gate::Pending;
        }
        let since = self.last.map(|last| now.saturating_duration_since(last));
        match since {
            Some(since) if since < self.interval => {
                self.pending = true;
                Gate::After(self.interval - since)
            }
            _ => Gate::Now,
        }
    }

    /// Records a run at `now`, which also clears a scheduled trailing run.
    pub fn ran(&mut self, now: Instant) {
        self.last = Some(now);
        self.pending = false;
    }
}
