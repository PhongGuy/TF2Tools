//! The pure scheduler behind remote images: per-viewer want sets, a newest-first fetch
//! queue, bounded fetch and decode slots, failure pauses and a byte-budgeted LRU.

use std::cell::Cell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

use super::{FetchFailure, ImageKey};

/// Fetches in flight at once.
pub const MAX_FETCHES: usize = 6;
/// Decodes in flight at once.
pub const MAX_DECODES: usize = 4;
/// Decoded bytes kept before the least recently drawn image is evicted, unwanted ones first.
pub const IMAGE_BUDGET: u64 = 96 * 1024 * 1024;
/// How long a transient failure stays `Failed` before a later `want` retries it.
pub const RETRY_AFTER: Duration = Duration::from_secs(30);

/// What a view draws for one image; keys the queue has never seen read as `Loading`.
#[derive(Clone, Debug, PartialEq)]
pub enum ImageState<T> {
    Loading,
    Ready(T),
    Failed,
}

struct Ready<T> {
    image: T,
    bytes: u64,
    /// The LRU stamp, bumped on every `get` so render reads count as use.
    used: Cell<u64>,
}

/// Schedules fetches and decodes for the images viewers currently want.
pub struct ImageQueue<T> {
    budget: u64,
    wants: HashMap<String, HashSet<ImageKey>>,
    queue: VecDeque<ImageKey>,
    fetching: HashSet<ImageKey>,
    /// Fetched bytes waiting for a decode slot, oldest first.
    pending: VecDeque<(ImageKey, Vec<u8>)>,
    decoding: HashSet<ImageKey>,
    ready: HashMap<ImageKey, Ready<T>>,
    /// A failed image and when it may be retried; None means never this session.
    failed: HashMap<ImageKey, Option<Instant>>,
    bytes: u64,
    clock: Cell<u64>,
}

impl<T: Clone> ImageQueue<T> {
    /// An empty queue that keeps at most `budget` decoded bytes.
    pub fn new(budget: u64) -> Self {
        Self {
            budget,
            wants: HashMap::new(),
            queue: VecDeque::new(),
            fetching: HashSet::new(),
            pending: VecDeque::new(),
            decoding: HashSet::new(),
            ready: HashMap::new(),
            failed: HashMap::new(),
            bytes: 0,
            clock: Cell::new(0),
        }
    }

    /// Replaces `viewer`'s wanted set. The queue becomes this call's keys that still need a
    /// fetch, in order, then older queued keys some viewer still wants; the rest drop.
    pub fn want(&mut self, viewer: &str, keys: Vec<ImageKey>, now: Instant) {
        // A transient failure whose pause is over is forgotten, so this want retries it.
        self.failed
            .retain(|_, until| until.is_none_or(|until| now < until));
        let mut queue = VecDeque::new();
        let mut seen = HashSet::new();
        for key in &keys {
            if self.needs_fetch(key) && seen.insert(key.clone()) {
                queue.push_back(key.clone());
            }
        }
        // An empty want is a release, so the viewer leaves no entry behind once it is gone.
        if keys.is_empty() {
            self.wants.remove(viewer);
        } else {
            self.wants
                .insert(viewer.to_owned(), keys.into_iter().collect());
        }
        for key in std::mem::take(&mut self.queue) {
            let wanted = self.wants.values().any(|set| set.contains(&key));
            if wanted && seen.insert(key.clone()) {
                queue.push_back(key);
            }
        }
        self.queue = queue;
    }

    // Not yet held, in flight or failed, so a want must schedule it.
    fn needs_fetch(&self, key: &ImageKey) -> bool {
        !self.ready.contains_key(key)
            && !self.fetching.contains(key)
            && !self.decoding.contains(key)
            && !self.failed.contains_key(key)
            && !self.pending.iter().any(|(k, _)| k == key)
    }

    /// The next key to fetch, or None when the queue is empty or every fetch slot is busy.
    pub fn next_fetch(&mut self) -> Option<ImageKey> {
        if self.fetching.len() >= MAX_FETCHES {
            return None;
        }
        let key = self.queue.pop_front()?;
        self.fetching.insert(key.clone());
        Some(key)
    }

    /// Records a fetch's outcome. Bytes wait for a decode even when no viewer still wants
    /// them, since they are already paid for.
    pub fn fetched(&mut self, key: &ImageKey, result: Result<Vec<u8>, FetchFailure>, now: Instant) {
        self.fetching.remove(key);
        match result {
            Ok(bytes) => self.pending.push_back((key.clone(), bytes)),
            Err(FetchFailure::Permanent) => {
                self.failed.insert(key.clone(), None);
            }
            Err(FetchFailure::Transient) => {
                self.failed.insert(key.clone(), Some(now + RETRY_AFTER));
            }
        }
    }

    /// The next fetched image to decode, or None when none waits or every slot is busy.
    pub fn next_decode(&mut self) -> Option<(ImageKey, Vec<u8>)> {
        if self.decoding.len() >= MAX_DECODES {
            return None;
        }
        let (key, bytes) = self.pending.pop_front()?;
        self.decoding.insert(key.clone());
        Some((key, bytes))
    }

    /// Records a decode: `Some((image, bytes))` caches it, None fails it for the session.
    /// Returns the images evicted to stay within the budget, never the one just added.
    /// Images no viewer wants go first; an evicted wanted image returns on its next `want`.
    pub fn decoded(&mut self, key: &ImageKey, image: Option<(T, u64)>) -> Vec<T> {
        self.decoding.remove(key);
        let Some((image, bytes)) = image else {
            self.failed.insert(key.clone(), None);
            return Vec::new();
        };
        let used = Cell::new(self.tick());
        if let Some(old) = self.ready.insert(key.clone(), Ready { image, bytes, used }) {
            self.bytes -= old.bytes;
        }
        self.bytes += bytes;
        let mut evicted = Vec::new();
        // Least recently used first, but an image no viewer wants goes before any wanted one,
        // so a static view's image survives the grid scrolling. A wanted eviction is not
        // re-queued: that only happens when the wanted set alone overruns the budget.
        while self.bytes > self.budget {
            let oldest = self
                .ready
                .iter()
                .filter(|(k, _)| *k != key)
                .min_by_key(|(k, entry)| {
                    let wanted = self.wants.values().any(|set| set.contains(*k));
                    (wanted, entry.used.get())
                })
                .map(|(k, _)| k.clone());
            let Some(oldest) = oldest else { break };
            if let Some(entry) = self.ready.remove(&oldest) {
                self.bytes -= entry.bytes;
                evicted.push(entry.image);
            }
        }
        evicted
    }

    /// The image's state; a ready image counts as just used for the LRU.
    pub fn get(&self, key: &ImageKey) -> ImageState<T> {
        if let Some(entry) = self.ready.get(key) {
            entry.used.set(self.tick());
            ImageState::Ready(entry.image.clone())
        } else if self.failed.contains_key(key) {
            ImageState::Failed
        } else {
            ImageState::Loading
        }
    }

    fn tick(&self) -> u64 {
        let next = self.clock.get() + 1;
        self.clock.set(next);
        next
    }

    /// Viewers holding a want; a viewer that wants nothing is not counted.
    pub fn viewers(&self) -> usize {
        self.wants.len()
    }

    /// Keys waiting for a fetch slot.
    pub fn queued(&self) -> usize {
        self.queue.len()
    }

    /// Fetches in flight.
    pub fn fetching(&self) -> usize {
        self.fetching.len()
    }

    /// Decodes in flight.
    pub fn decoding(&self) -> usize {
        self.decoding.len()
    }

    /// Decoded bytes held.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
}
