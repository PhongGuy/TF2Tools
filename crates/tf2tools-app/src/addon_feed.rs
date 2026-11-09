//! The card list behind a GameBanana add-on tab, free of GPUI so it tests plainly.
//!
//! A step streams chunks of cards and ends with the cursor of the next step. The feed drops
//! repeated cards, ignores chunks from an earlier query, and decides whether a step that added
//! nothing runs once more by itself or waits behind "Load more".

use std::collections::HashSet;
use std::hash::Hash;

/// A card the feed can tell apart from its repeats.
pub trait Keyed {
    /// The identity two copies of one card share.
    type Key: Eq + Hash + Clone;
    /// This card's identity.
    fn key(&self) -> Self::Key;
}

/// What the tab does after a step ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepAfter {
    /// The step added cards; wait for the grid to ask for more.
    Idle,
    /// The step added nothing, so run the next step at once.
    Continue,
    /// Steps keep coming back empty; show "Load more".
    LoadMore,
    /// GameBanana has no more pages for this query.
    End,
    /// The step belonged to an earlier query and changed nothing.
    Stale,
}

/// The cards of one query, the cursor of its next step and the dry-step rule.
pub struct AddonFeed<T: Keyed, C: Clone> {
    items: Vec<T>,
    seen: HashSet<T::Key>,
    cursor: C,
    generation: u64,
    loading: bool,
    end: bool,
    dry: bool,
    from_button: bool,
    load_more: bool,
    added: usize,
}

impl<T: Keyed, C: Clone> AddonFeed<T, C> {
    /// An empty feed whose first step starts at `start`.
    pub fn new(start: C) -> Self {
        Self {
            items: Vec::new(),
            seen: HashSet::new(),
            cursor: start,
            generation: 0,
            loading: false,
            end: false,
            dry: false,
            from_button: false,
            load_more: false,
            added: 0,
        }
    }

    /// Clears the feed for a new query starting at `start` and returns its generation; chunks
    /// and step ends tagged with any earlier generation are ignored from now on.
    pub fn reset(&mut self, start: C) -> u64 {
        let generation = self.generation.wrapping_add(1);
        *self = Self::new(start);
        self.generation = generation;
        generation
    }

    /// The cards so far, in arrival order.
    pub fn items(&self) -> &[T] {
        &self.items
    }

    /// The current query's generation.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Whether a step is running.
    pub fn is_loading(&self) -> bool {
        self.loading
    }

    /// Whether the query has no more pages.
    pub fn is_end(&self) -> bool {
        self.end
    }

    /// Whether the feed waits behind "Load more".
    pub fn wants_load_more(&self) -> bool {
        self.load_more
    }

    /// Whether the query ended without a single card.
    pub fn is_no_results(&self) -> bool {
        self.end && self.items.is_empty()
    }

    /// Starts the next step and returns its cursor and generation; `None` while a step runs,
    /// after the end, or while "Load more" is offered.
    pub fn begin_step(&mut self) -> Option<(C, u64)> {
        if self.loading || self.end || self.load_more {
            return None;
        }
        self.loading = true;
        self.added = 0;
        self.from_button = false;
        Some((self.cursor.clone(), self.generation))
    }

    /// Starts the step the player asked for with "Load more"; `None` unless it is offered.
    pub fn load_more(&mut self) -> Option<(C, u64)> {
        if !self.load_more {
            return None;
        }
        self.load_more = false;
        let step = self.begin_step()?;
        self.from_button = true;
        Some(step)
    }

    /// Appends the cards not seen before and returns how many were new; a chunk from an
    /// earlier generation appends nothing.
    pub fn chunk(&mut self, generation: u64, cards: Vec<T>) -> usize {
        if generation != self.generation {
            return 0;
        }
        let before = self.items.len();
        for card in cards {
            if self.seen.insert(card.key()) {
                self.items.push(card);
            }
        }
        let added = self.items.len() - before;
        self.added += added;
        added
    }

    /// Ends the running step with the next step's cursor, or `None` when GameBanana has no
    /// more pages, and says what the tab does next.
    pub fn step_done(&mut self, generation: u64, next: Option<C>) -> StepAfter {
        if generation != self.generation {
            return StepAfter::Stale;
        }
        self.loading = false;
        let Some(next) = next else {
            self.end = true;
            return StepAfter::End;
        };
        self.cursor = next;
        // A step that found cards clears the dry rule; an empty one continues once by itself,
        // and an empty one after that, or after a click, waits for the player.
        if self.added > 0 {
            self.dry = false;
            self.from_button = false;
            StepAfter::Idle
        } else if self.from_button || self.dry {
            self.load_more = true;
            StepAfter::LoadMore
        } else {
            self.dry = true;
            StepAfter::Continue
        }
    }

    /// Ends the running step without moving the cursor, so a retry repeats it; the cards stay.
    pub fn step_failed(&mut self, generation: u64) {
        if generation == self.generation {
            self.loading = false;
        }
    }
}
