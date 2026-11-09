//! The store grid shared by the HUD, Sound, Crosshair and Addon stores: a search box, a sort
//! select and a count over a virtualized card grid that pages in keyset pages as it scrolls.

pub mod feed;

use std::ops::Range;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_kit::assets::IconName;
use gpui_kit::component::IndexPath;
use gpui_kit::component::button::Button;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::{ActiveTheme as _, Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement as _, Pixels, Render, SharedString, StatefulInteractiveElement as _,
    Styled as _, Subscription, Task, TestSupportExt as _, Window, div, point, px,
};
use tf2tools_core::{StoreCard, StoreKind, StoreQuery, StoreSort};

use crate::ids;
use crate::library_events::{LibraryChanged, LibraryEvents};
use crate::pages::common::{
    SkeletonTop, empty_state_card, empty_state_with_action, skeleton_cards,
};
use crate::remote_images::{ImageKey, RemoteImage, RemoteImages};
use crate::services::{OpError, run_blocking};
use crate::store_download::SessionDownloads;
use crate::store_messages::read_failed;
use crate::sync::{SyncEvent, SyncHub, sync_hub};
use crate::widgets::card_grid::{CardGrid, SHELL_CHROME_WIDTH, columns_for_store};
use crate::widgets::label_option::LabelOption;
use crate::widgets::labelled;
use crate::widgets::store_error::store_error_line;
use feed::{
    Feed, FeedItem as _, Gate, PAGE, REQUERY_INTERVAL, Throttle, anchored_scroll, first_visible,
    reload,
};

/// Draws one card's content. `image` is None without a thumbnail or without image cards.
pub type CardRenderer =
    Rc<dyn Fn(&StoreCard, Option<RemoteImage>, &mut Window, &mut App) -> AnyElement>;

/// How tall a store card is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CardSize {
    Fixed(Pixels),
    /// A 16:9 image area of the cell width, plus `text_height` below it.
    Image {
        text_height: Pixels,
    },
}

/// What a store grid shows: which store, its noun for the count, and its card shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StoreGridConfig {
    pub kind: StoreKind,
    pub noun: &'static str,
    pub max_columns: usize,
    pub card: CardSize,
    pub search: bool,
    pub external_toolbar: bool,
    /// The page filters the grid as well as searching it, so the no-match card says "filters".
    pub filters: bool,
}

impl StoreGridConfig {
    /// Up to 4 columns of 120 px cards.
    pub fn new(kind: StoreKind, noun: &'static str) -> Self {
        Self {
            kind,
            noun,
            max_columns: 4,
            card: CardSize::Fixed(px(120.)),
            search: true,
            external_toolbar: false,
            filters: false,
        }
    }

    /// The grid draws no toolbar; its page places `search_row` and `sort_control` itself.
    pub fn external_toolbar(self) -> Self {
        Self {
            external_toolbar: true,
            ..self
        }
    }

    /// The page has filters of its own, such as Sound Store's category pills.
    pub fn with_filters(self) -> Self {
        Self {
            filters: true,
            ..self
        }
    }

    /// No search box: the toolbar row holds only the count and Sort, at its right.
    pub fn without_search(self) -> Self {
        Self {
            search: false,
            ..self
        }
    }

    pub fn max_columns(self, n: usize) -> Self {
        Self {
            max_columns: n,
            ..self
        }
    }

    /// Cards with a 16:9 thumbnail, fetched through `RemoteImages`, above `text_height` of text.
    pub fn image_card(self, text_height: Pixels) -> Self {
        Self {
            card: CardSize::Image { text_height },
            ..self
        }
    }
}

/// v5 draws eight skeleton cards while a store loads.
const SKELETON_CARDS: usize = 8;

/// How long typing must pause before the search runs.
pub const SEARCH_DEBOUNCE: Duration = Duration::from_millis(400);

const SORTS: [(&str, StoreSort); 3] = [
    ("Most Liked", StoreSort::MostLiked),
    ("Most Viewed", StoreSort::MostViewed),
    ("Newest", StoreSort::Newest),
];

/// One store's searchable, sortable card grid. Every query change starts a new `Feed`
/// generation, so pages and counts of an older query that land late are dropped.
pub struct StoreGrid {
    config: StoreGridConfig,
    render: CardRenderer,
    feed: Feed<StoreCard>,
    grid: CardGrid,
    search_input: Entity<InputState>,
    sort_select: Entity<SelectState<Vec<LabelOption>>>,
    search: String,
    sort: StoreSort,
    category: Option<String>,
    shown: Option<u64>,
    total: Option<u64>,
    counting: bool,
    loaded: bool,
    error: Option<String>,
    /// Which reads of the current query failed, so Retry repeats only those.
    count_failed: bool,
    page_failed: bool,
    visible: Range<usize>,
    /// This grid's own `RemoteImages` viewer, so grids alive together keep separate wants.
    viewer: String,
    requeries: usize,
    /// First pages accepted for a fresh query, empty ones included.
    first_pages: usize,
    throttle: Throttle,
    /// The scheduled trailing re-query; dropping it cancels the run.
    trailing: Option<Task<()>>,
    /// The hub's last `fetched` for this kind while it runs.
    sync_fetched: u32,
    /// Whether the hub runs this kind's sync, as of its last change.
    syncing: bool,
    debounce: Option<Task<()>>,
    /// Wall-clock start of the current query, so timing measures the query and not the debounce.
    query_started: Instant,
    /// Set while the grid's page is away; it then holds its re-queries and image wants.
    hidden: bool,
    /// A sync or a library change landed while hidden, so the rows need a re-read on return.
    stale: bool,
    /// The page's downloads this session, which each re-read retires.
    session: Option<SessionDownloads>,
    _subscriptions: Vec<Subscription>,
}

impl StoreGrid {
    /// Needs the `Services` global. Starts the first page and the counts off the UI thread.
    pub fn new(
        config: StoreGridConfig,
        render: CardRenderer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let placeholder = format!("Search {}...", config.noun);
        let search_input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let labels = LabelOption::all(ids::STORE_SORT, SORTS.iter().map(|(label, _)| *label));
        let sort_select =
            cx.new(|cx| SelectState::new(labels, Some(IndexPath::default()), window, cx));
        let mut subscriptions = vec![
            cx.subscribe(&search_input, |grid, input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    let text = input.read(cx).value().to_string();
                    grid.search_typed(text, cx);
                }
            }),
            cx.subscribe(
                &sort_select,
                |grid, _, event: &SelectEvent<Vec<LabelOption>>, cx| {
                    let SelectEvent::Confirm(Some(label)) = event else {
                        return;
                    };
                    if let Some((_, sort)) = SORTS.iter().find(|(l, _)| l == label) {
                        grid.set_sort(*sort, cx);
                    }
                },
            ),
        ];
        // Image cards redraw as thumbnails arrive, and hand their wants back when the grid goes.
        let viewer = format!("store-grid:{}", cx.entity_id());
        let images = RemoteImages::global(cx);
        if let (CardSize::Image { .. }, Some(images)) = (config.card, images) {
            subscriptions.push(cx.observe(&images, |_, _, cx| cx.notify()));
            let released = viewer.clone();
            subscriptions.push(cx.on_release(move |_, cx| {
                images.update(cx, |images, cx| images.want(&released, Vec::new(), cx));
            }));
        }

        // While a sync of this store streams rows in, its progress re-queries the grid
        // (throttled), and its end re-queries at once.
        if let Some(hub) = sync_hub(cx) {
            subscriptions.push(cx.observe(&hub, |grid, hub, cx| grid.hub_changed(&hub, cx)));
            subscriptions.push(cx.subscribe(&hub, |grid, _, event: &SyncEvent, cx| {
                if matches!(event, SyncEvent::Finished { kind } if *kind == grid.config.kind) {
                    grid.requery(cx);
                }
            }));
        }
        // The cards' In Library marks read the library, so a change made elsewhere while the
        // page is away calls for a re-read on return.
        let events = LibraryEvents::global(cx);
        subscriptions.push(cx.subscribe(&events, |grid, _, _: &LibraryChanged, _| {
            grid.stale |= grid.hidden;
        }));

        let mut grid = CardGrid::store(ids::STORE_GRID, config.max_columns);
        let this = cx.entity().downgrade();
        grid.on_visible(move |range, cx| {
            let _ = this.update(cx, |grid, cx| grid.visible_changed(range, cx));
        });
        let this = cx.entity().downgrade();
        grid.on_near_end(move |cx| {
            let _ = this.update(cx, |grid, cx| grid.next_page(cx));
        });

        let mut store = Self {
            config,
            render,
            feed: Feed::new(),
            grid,
            search_input,
            sort_select,
            search: String::new(),
            sort: StoreSort::MostLiked,
            category: None,
            shown: None,
            total: None,
            counting: false,
            loaded: false,
            error: None,
            count_failed: false,
            page_failed: false,
            visible: 0..0,
            viewer,
            requeries: 0,
            first_pages: 0,
            throttle: Throttle::new(REQUERY_INTERVAL),
            trailing: None,
            sync_fetched: 0,
            syncing: sync_hub(cx).is_some_and(|hub| hub.read(cx).is_running(config.kind)),
            debounce: None,
            query_started: Instant::now(),
            hidden: false,
            stale: false,
            session: None,
            _subscriptions: subscriptions,
        };
        store.restart(cx);
        store
    }

    /// The cards loaded so far for the current query.
    pub fn cards(&self) -> &[StoreCard] {
        self.feed.items()
    }

    /// How many cards match the current query, once counted.
    pub fn shown(&self) -> Option<u64> {
        self.shown
    }

    /// How many cards the store holds with no search, once counted.
    pub fn total(&self) -> Option<u64> {
        self.total
    }

    /// True once a page of the current query has landed.
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// True while a page or the counts are being read.
    pub fn is_loading(&self) -> bool {
        self.feed.is_loading() || self.counting
    }

    /// True once an empty page has ended the current query.
    pub fn is_end(&self) -> bool {
        self.feed.is_end()
    }

    /// The last failed query's message, until Retry or a new query.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn card_grid(&self) -> &CardGrid {
        &self.grid
    }

    /// Re-queries finished during a sync, failed ones included.
    pub fn requeries(&self) -> usize {
        self.requeries
    }

    /// First pages landed for a fresh query (the open, each search, sort or category),
    /// counting an empty first page (a search with no match) like any other.
    pub fn first_pages(&self) -> usize {
        self.first_pages
    }

    /// When the current query (the open, a search, sort or category) started.
    pub fn query_started(&self) -> Instant {
        self.query_started
    }

    /// Runs the search for `text` once typing pauses for `SEARCH_DEBOUNCE`; a newer keystroke
    /// replaces the pending one.
    pub fn search_typed(&mut self, text: String, cx: &mut Context<Self>) {
        let timer = cx.background_executor().timer(SEARCH_DEBOUNCE);
        self.debounce = Some(cx.spawn(async move |grid, cx| {
            timer.await;
            let _ = grid.update(cx, |grid, cx| {
                // Typing that ends on the current query keeps the feed and its scroll.
                if same_search(&grid.search, &text) {
                    return;
                }
                grid.search = text;
                grid.restart(cx);
            });
        }));
    }

    /// The search the grid queries, once typing has paused.
    pub fn search(&self) -> &str {
        &self.search
    }

    pub fn sort(&self) -> StoreSort {
        self.sort
    }

    /// Lets each re-read retire the downloads `session` recorded before it began.
    pub fn track_session(&mut self, session: SessionDownloads) {
        self.session = Some(session);
    }

    /// Hides or shows the grid with its page. Hidden, it drops its image wants and holds sync
    /// re-queries; shown again, it asks for the drawn cards' images and re-reads once if
    /// anything landed meanwhile, keeping search, sort, category and scroll.
    pub fn set_hidden(&mut self, hidden: bool, cx: &mut Context<Self>) {
        if hidden == std::mem::replace(&mut self.hidden, hidden) {
            return;
        }
        if hidden {
            self.stale |= self.trailing.take().is_some();
            if let (CardSize::Image { .. }, Some(images)) =
                (self.config.card, RemoteImages::global(cx))
            {
                images.update(cx, |images, cx| images.want(&self.viewer, Vec::new(), cx));
            }
            return;
        }
        self.grid.rearm();
        if std::mem::take(&mut self.stale) {
            self.requery(cx);
        }
        cx.notify();
    }

    /// Reorders the store from its first card.
    pub fn set_sort(&mut self, sort: StoreSort, cx: &mut Context<Self>) {
        if sort != self.sort {
            self.sort = sort;
            self.restart(cx);
        }
    }

    /// The category filter; None shows every card.
    pub fn category(&self) -> Option<&str> {
        self.category.as_deref()
    }

    /// Filters to `category` and restarts from the top, as a new sort does; the same value is a no-op.
    pub fn set_category(&mut self, category: Option<String>, cx: &mut Context<Self>) {
        if category != self.category {
            self.category = category;
            self.restart(cx);
        }
    }

    /// Clears the error and asks again for whichever of the counts and the page failed.
    pub fn retry(&mut self, cx: &mut Context<Self>) {
        self.error = None;
        if std::mem::take(&mut self.count_failed) {
            self.count(cx);
        }
        if std::mem::take(&mut self.page_failed) {
            self.next_page(cx);
        }
        cx.notify();
    }

    // The current search, sort and category as a first-page query.
    fn query(&self) -> StoreQuery {
        query_for(self.sort, &self.search, self.category.as_deref(), PAGE)
    }

    // A new query: a fresh feed generation from the top, its counts and its first page. The
    // grid is rearmed, since the new first page may land on the range and count its hooks
    // last fired for.
    fn restart(&mut self, cx: &mut Context<Self>) {
        self.query_started = Instant::now();
        self.feed.reset();
        // The new query reads fresh rows, so a scheduled re-query of the old one is moot.
        if self.trailing.take().is_some() {
            self.throttle.ran(cx.background_executor().now());
        }
        self.grid.rearm();
        self.grid
            .scroll_handle()
            .0
            .borrow()
            .base_handle
            .set_offset(point(px(0.), px(0.)));
        self.loaded = false;
        self.error = None;
        (self.count_failed, self.page_failed) = (false, false);
        self.visible = 0..0;
        self.count(cx);
        self.next_page(cx);
        cx.notify();
    }

    fn count(&mut self, cx: &mut Context<Self>) {
        let (kind, generation) = (self.config.kind, self.feed.generation());
        let (shown, total) = (self.query(), StoreQuery::new(self.sort, PAGE));
        self.counting = true;
        let counting = run_blocking(cx, move |s| {
            let count = |q: &StoreQuery| {
                s.store
                    .store_count(kind, q)
                    .map_err(|e| OpError::core("load the store", e))
            };
            Ok((count(&shown)?, count(&total)?))
        });
        cx.spawn(async move |grid, cx| {
            let counted = counting.await;
            let _ = grid.update(cx, |grid, cx| {
                if grid.feed.generation() != generation {
                    return;
                }
                grid.counting = false;
                match counted {
                    Ok((shown, total)) => (grid.shown, grid.total) = (Some(shown), Some(total)),
                    Err(err) => {
                        grid.count_failed = true;
                        grid.error = Some(read_failed(&err));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    // Reads the page after the last card, unless one is in flight or the query has ended.
    fn next_page(&mut self, cx: &mut Context<Self>) {
        let Some((after, generation)) = self.feed.next_page() else {
            return;
        };
        let (kind, first) = (self.config.kind, after.is_none());
        let query = match after {
            Some(cursor) => self.query().with_after(cursor),
            None => self.query(),
        };
        let reading = run_blocking(cx, move |s| {
            s.store
                .store_cards(kind, &query)
                .map_err(|e| OpError::core("load the store", e))
        });
        cx.spawn(async move |grid, cx| {
            let page = reading.await;
            let _ = grid.update(cx, |grid, cx| {
                match page {
                    Ok(page) => {
                        if grid.feed.page_loaded(generation, page) {
                            grid.first_pages += usize::from(first);
                            grid.loaded = true;
                            grid.want_thumbnails(cx);
                            grid.after_landing(cx);
                        }
                    }
                    Err(err) if generation == grid.feed.generation() => {
                        grid.feed.page_failed(generation);
                        grid.page_failed = true;
                        grid.error = Some(read_failed(&err));
                        // A re-query held back by this page still runs; the failed page
                        // itself waits for Retry rather than being asked again here.
                        if grid.feed.take_reload_wanted() {
                            grid.start_reload(cx);
                        }
                    }
                    Err(_) => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    // Runs what waited on a landed page or reload: a held-back re-query, then a near-end the
    // in-flight request swallowed.
    fn after_landing(&mut self, cx: &mut Context<Self>) {
        if self.feed.take_reload_wanted() {
            self.start_reload(cx);
        }
        if self.feed.take_more_wanted() {
            self.next_page(cx);
        }
    }

    // A tick of this store's sync: a new nonzero `fetched` asks the throttle for a re-query.
    fn hub_changed(&mut self, hub: &Entity<SyncHub>, cx: &mut Context<Self>) {
        let kind = self.config.kind;
        let (running, fetched) = {
            let hub = hub.read(cx);
            let fetched = hub.progress(kind).map_or(0, |t| t.fetched);
            (hub.is_running(kind), fetched)
        };
        // A sync starting or ending flips an empty store between skeletons and its error card.
        if running != std::mem::replace(&mut self.syncing, running) {
            cx.notify();
        }
        if fetched == std::mem::replace(&mut self.sync_fetched, fetched) || fetched == 0 {
            return;
        }
        if self.hidden {
            self.stale = true;
            return;
        }
        match self.throttle.request(cx.background_executor().now()) {
            Gate::Now => self.requery(cx),
            Gate::After(delay) => {
                let timer = cx.background_executor().timer(delay);
                self.trailing = Some(cx.spawn(async move |grid, cx| {
                    timer.await;
                    let _ = grid.update(cx, |grid, cx| grid.requery(cx));
                }));
            }
            Gate::Pending => {}
        }
    }

    // A re-query now: it marks the throttle run, which also covers a trailing run it replaces.
    // A hidden grid only notes that one is owed.
    fn requery(&mut self, cx: &mut Context<Self>) {
        if self.hidden {
            self.stale = true;
            return;
        }
        self.trailing = None;
        self.throttle.ran(cx.background_executor().now());
        self.start_reload(cx);
    }

    // Re-reads the loaded prefix from the top off the UI thread, paging on until the first
    // visible card is back in it, then swaps it in with the scroll moved to keep that card in
    // place. A page in flight holds the reload back until it lands or fails.
    fn start_reload(&mut self, cx: &mut Context<Self>) {
        let Some(generation) = self.feed.begin_reload() else {
            return;
        };
        let anchor = self
            .first_visible_index()
            .and_then(|ix| self.feed.items().get(ix))
            .map(|c| c.feed_id());
        let (kind, sort, search) = (self.config.kind, self.sort, self.search.clone());
        let category = self.category.clone();
        let loaded = self.feed.items().len();
        // Downloads recorded before the read began are in the library rows it reads.
        let recorded = self.session.as_ref().map(SessionDownloads::snapshot);
        let reading = run_blocking(cx, move |s| {
            reload(loaded, anchor.as_ref(), |after, limit| {
                let query = query_for(sort, &search, category.as_deref(), limit);
                let query = match after {
                    Some(cursor) => query.with_after(cursor),
                    None => query,
                };
                s.store
                    .store_cards(kind, &query)
                    .map_err(|e| OpError::core("load the store", e))
            })
        });
        cx.spawn(async move |grid, cx| {
            let reloaded = reading.await;
            let _ = grid.update(cx, |grid, cx| {
                grid.requeries += 1;
                match reloaded {
                    Ok((items, end)) => {
                        if generation == grid.feed.generation()
                            && let (Some(session), Some(recorded)) = (&grid.session, &recorded)
                        {
                            session.forget(recorded);
                        }
                        grid.apply_reload(generation, items, end, cx)
                    }
                    Err(_) => grid.feed.reload_failed(generation),
                }
                if generation == grid.feed.generation() {
                    grid.after_landing(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    // Swaps the reload in and moves the scroll so the first visible card stays where it was
    // on screen; the counts are read again, since the sync added rows.
    fn apply_reload(
        &mut self,
        generation: u64,
        items: Vec<StoreCard>,
        end: bool,
        cx: &mut Context<Self>,
    ) {
        if generation != self.feed.generation() {
            return;
        }
        let handle = self.grid.scroll_handle().0.borrow().base_handle.clone();
        let offset = handle.offset();
        let old: Vec<_> = self.feed.items().iter().map(|c| c.id.clone()).collect();
        let new: Vec<_> = items.iter().map(|c| c.id.clone()).collect();
        // Before the first paint there is no row height to anchor by, and nothing scrolled.
        let scroll_y = self.grid.row_height().map(|row_height| {
            anchored_scroll(
                -f32::from(offset.y),
                f32::from(row_height),
                self.grid.columns(),
                &old,
                &new,
            )
        });
        self.feed.reloaded(generation, items, end);
        if let Some(scroll_y) = scroll_y {
            handle.set_offset(point(offset.x, px(-scroll_y)));
        }
        self.want_thumbnails(cx);
        self.count(cx);
    }

    /// The index of the first card of the first visible row, once the grid has laid out.
    pub fn first_visible_index(&self) -> Option<usize> {
        let scroll_y = -f32::from(self.grid.scroll_handle().0.borrow().base_handle.offset().y);
        self.grid.row_height().and_then(|row_height| {
            first_visible(scroll_y, f32::from(row_height), self.grid.columns())
        })
    }

    // The grid drew a new range of cards.
    fn visible_changed(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        self.visible = range;
        self.want_thumbnails(cx);
    }

    // Image cards ask for the visible cards' thumbnails, newest first. Runs on a new range and
    // on every landed page, since a page can replace cards without changing the range.
    fn want_thumbnails(&mut self, cx: &mut Context<Self>) {
        let CardSize::Image { .. } = self.config.card else {
            return;
        };
        if self.hidden {
            return;
        }
        let Some(images) = RemoteImages::global(cx) else {
            return;
        };
        let cards = self.feed.items();
        let end = self.visible.end.min(cards.len());
        let start = self.visible.start.min(end);
        let keys: Vec<ImageKey> = cards[start..end]
            .iter()
            .filter_map(|card| card.thumbnail_url.as_deref().map(ImageKey::card))
            .collect();
        images.update(cx, |images, cx| images.want(&self.viewer, keys, cx));
    }

    fn count_label(&self, cx: &App) -> impl IntoElement {
        let count = match (self.shown, self.total) {
            (Some(shown), Some(total)) => format!("{shown} of {total} {}", self.config.noun),
            _ => String::new(),
        };
        labelled(
            div().text_sm().text_color(cx.theme().muted_foreground),
            ids::STORE_COUNT,
            count,
        )
    }

    fn search_box(&self, cx: &App) -> impl IntoElement {
        div()
            .id(ids::STORE_SEARCH)
            .test_support()
            .w(px(288.))
            .child(
                Input::new(&self.search_input)
                    .h_9()
                    .prefix(Icon::new(IconName::Search).text_color(cx.theme().muted_foreground)),
            )
    }

    /// v5's search row: the 288 x 36 search box (`STORE_SEARCH`) left, the count (`STORE_COUNT`) right.
    pub fn search_row(&self, cx: &App) -> AnyElement {
        h_flex()
            .justify_between()
            .items_center()
            .flex_wrap()
            .gap_4()
            .child(self.search_box(cx))
            .child(self.count_label(cx))
            .into_any_element()
    }

    /// "Sort:" (`STORE_SORT_LABEL`) 8 px before the 144 px select (`STORE_SORT`).
    pub fn sort_control(&self, cx: &App) -> AnyElement {
        // The kit's medium trigger is 32 px; `h_9` refines it to v5's 36.
        h_flex()
            .gap_2()
            .items_center()
            .child(labelled(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(cx.theme().muted_foreground),
                ids::STORE_SORT_LABEL,
                "Sort:",
            ))
            .child(
                div()
                    .id(ids::STORE_SORT)
                    .test_support()
                    .w(px(144.))
                    .child(Select::new(&self.sort_select).h_9()),
            )
            .into_any_element()
    }

    // v5's order: search left with count and Sort right; a grid without search (the Crosshair
    // Store) keeps count and Sort at the right, since the top bar already names the page.
    fn header(&self, cx: &App) -> impl IntoElement {
        let left = if self.config.search {
            self.search_box(cx).into_any_element()
        } else {
            div().into_any_element()
        };
        let right = h_flex()
            .gap_4()
            .items_center()
            .flex_wrap()
            .child(self.count_label(cx))
            .child(self.sort_control(cx));
        h_flex()
            .justify_between()
            .items_center()
            .flex_wrap()
            .gap_4()
            .child(left)
            .child(right)
    }

    fn error_line(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let error = self.error.clone()?;
        let this = cx.entity().downgrade();
        Some(store_error_line(
            error,
            move |_, cx| {
                this.update(cx, |grid, cx| grid.retry(cx)).ok();
            },
            cx,
        ))
    }

    // v5's states for a store with no rows: skeletons until the first page and counts land and
    // while a sync (or the re-read after it) runs, then "Could not load" with Retry. A filled
    // store whose query found nothing says so, even during a sync, until a match lands. Rows,
    // or a failed read (the inline error line speaks for it), show the grid as it is.
    fn body(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        if !self.feed.items().is_empty() || self.error.is_some() {
            return self.cards_grid(cx);
        }
        if self.total.is_some_and(|total| total > 0) {
            // Only a new query clears `loaded`, so the re-reads of a sync keep the card steady.
            return if self.loaded {
                self.no_match_card(cx)
            } else {
                self.cards_grid(cx)
            };
        }
        // The card has no measured width before it draws, so take v5's viewport breakpoints.
        let columns = columns_for_store(
            window.viewport_size().width - SHELL_CHROME_WIDTH,
            self.config.max_columns,
        );
        let top = match self.config.card {
            CardSize::Image { .. } => SkeletonTop::Image,
            CardSize::Fixed(_) => SkeletonTop::Icon,
        };
        let noun = self.config.noun;
        let skeletons =
            |id, label: SharedString| skeleton_cards(id, label, SKELETON_CARDS, columns, top, cx);
        if !self.loaded || self.total.is_none() {
            return skeletons(ids::STORE_LOADING, "Loading...".into());
        }
        if self.syncing || self.is_loading() || self.feed.is_reloading() {
            return skeletons(ids::STORE_SYNCING, format!("Syncing {noun}...").into());
        }
        let kind = self.config.kind;
        let retry = Button::new(ids::STORE_UNLOADED_RETRY)
            .outline()
            .label("Retry")
            .on_click(move |_, _, cx| {
                if let Some(hub) = sync_hub(cx) {
                    hub.update(cx, |hub, cx| hub.start(kind, false, cx));
                }
            });
        empty_state_with_action(
            ids::STORE_UNLOADED,
            IconName::WifiOff,
            format!("Could not load {noun}"),
            None,
            "Check your internet connection and try again.",
            Some(retry.into_any_element()),
            cx,
        )
    }

    // v5's wording: a store with filters speaks of them, a search-only store of the search.
    fn no_match_card(&self, cx: &App) -> AnyElement {
        let (subject, hint) = if self.config.filters {
            ("filters", "Try adjusting your search or filter criteria.")
        } else {
            ("search", "Try adjusting your search criteria.")
        };
        empty_state_card(
            ids::STORE_EMPTY,
            IconName::Store,
            format!("No {} match your {subject}", self.config.noun),
            Some(ids::STORE_EMPTY_HINT),
            hint,
            cx,
        )
    }

    // Each card reads its row from the entity while the list lays out, so a frame clones
    // only the visible cards, never the loaded prefix.
    fn cards_grid(&self, cx: &mut Context<Self>) -> AnyElement {
        let (kind, card_size) = (self.config.kind, self.config.card);
        let (render, this) = (Rc::clone(&self.render), cx.entity().downgrade());
        let height = move |width: Pixels| match card_size {
            CardSize::Fixed(height) => height,
            CardSize::Image { text_height } => width * (9. / 16.) + text_height,
        };
        self.grid
            .render_sized(self.feed.items().len(), height, move |ix, window, cx| {
                let card = this
                    .upgrade()
                    .and_then(|grid| grid.read(cx).feed.items().get(ix).cloned());
                let Some(card) = card else {
                    return div().into_any_element();
                };
                let image = match (card_size, &card.thumbnail_url) {
                    (CardSize::Image { .. }, Some(url)) => RemoteImages::global(cx)
                        .map(|images| images.read(cx).image(&ImageKey::card(url))),
                    _ => None,
                };
                div()
                    .id(ids::store_card(kind, &card.id))
                    .accessibility_id(ids::store_card(kind, &card.id).to_string())
                    .test_support()
                    .size_full()
                    .child(render(&card, image, window, cx))
                    .into_any_element()
            })
    }
}

// Whether two search texts are the same query: equal, or both blank, since a blank search is
// no filter. Non-blank text compares exactly, as core matches its spaces literally.
fn same_search(a: &str, b: &str) -> bool {
    a == b || (a.trim().is_empty() && b.trim().is_empty())
}

// The first page of `limit` cards in `sort` order, filtered by `search` unless it is blank
// and by `category` when set.
fn query_for(sort: StoreSort, search: &str, category: Option<&str>, limit: u32) -> StoreQuery {
    let mut query = StoreQuery::new(sort, limit);
    if !search.trim().is_empty() {
        query = query.with_search(search.to_owned());
    }
    match category {
        Some(category) => query.with_category(category),
        None => query,
    }
}

impl Render for StoreGrid {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // v5's `flex flex-col gap-6`: the first card row sits 24 px under the toolbar.
        v_flex()
            .size_full()
            .gap_6()
            .when(!self.config.external_toolbar, |this| {
                this.child(self.header(cx))
            })
            .children(self.error_line(cx))
            .child(div().flex_1().min_h_0().child(self.body(window, cx)))
    }
}
