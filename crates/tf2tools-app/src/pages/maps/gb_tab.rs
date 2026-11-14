//! The Maps page's GameBanana source: the synced maps catalogue in the add-on card grid,
//! read in keyset pages from the local database as the player scrolls, and re-read while
//! the maps sync commits pages.

use std::collections::HashSet;
use std::ops::Range;
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::{ActiveTheme as _, Icon, IndexPath, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, Styled as _, Subscription, Task, TestSupportExt as _,
    Window, div, point, px,
};
use tf2tools_core::{
    AddonSection, GbId, GbMapCard, GbMapCursor, GbMapGamemode, GbMapQuery, GbMapSort,
};

use crate::addon_detail::open_addon_detail;
use crate::ids;
use crate::maps_sync::{MapsSync, MapsSyncEvent, maps_sync};
use crate::mature::MatureAddons;
use crate::pages::addon_store::card::{OpenAddon, card_height, gb_addon_card};
use crate::pages::addon_store::gamebanana_tab::CategoryOption;
use crate::pages::common::{SkeletonTop, empty_state_card, skeleton_cards};
use crate::remote_images::{ImageKey, RemoteImages};
use crate::services::{OpError, run_blocking};
use crate::store_grid::feed::{
    Feed, FeedItem, Gate, PAGE, REQUERY_INTERVAL, Throttle, anchored_scroll, first_visible, reload,
};
use crate::store_grid::{SEARCH_DEBOUNCE, same_search};
use crate::store_messages::read_failed;
use crate::widgets::card_grid::{CardGrid, SHELL_CHROME_WIDTH, columns_for_store};
use crate::widgets::label_option::LabelOption;
use crate::widgets::store_error::error_line;
use crate::widgets::sync_banner::maps_sync_banner;

/// The most columns the map grid lays out, as the Addon Store's.
const MAX_COLUMNS: usize = 4;

/// v5 draws eight skeleton cards while a store loads.
const SKELETON_CARDS: usize = 8;

/// Fits "Most downloaded" whole beside the kit's caret and padding, on a 4 px step.
const SORT_WIDTH: f32 = 172.;

/// The sort Select's labels, in menu order.
const SORTS: [(&str, GbMapSort); 2] = [
    ("Most downloaded", GbMapSort::MostDownloaded),
    ("Newest", GbMapSort::Newest),
];

impl FeedItem for GbMapCard {
    type Id = GbId;
    type Cursor = GbMapCursor;

    fn feed_id(&self) -> GbId {
        self.card.id
    }

    fn feed_cursor(&self) -> GbMapCursor {
        self.cursor.clone()
    }
}

/// The synced GameBanana maps grid. Pages are read off the UI thread; the grid asks for the
/// next one near its end.
pub struct GbMapsTab {
    feed: Feed<GbMapCard>,
    grid: CardGrid,
    sort: GbMapSort,
    gamemode: Option<u32>,
    search: String,
    /// The catalogue's first-level gamemodes, A to Z; empty until read.
    gamemodes: Vec<GbMapGamemode>,
    search_input: Entity<InputState>,
    sort_select: Entity<SelectState<Vec<LabelOption>>>,
    gamemode_select: Entity<SelectState<Vec<CategoryOption>>>,
    debounce: Option<Task<()>>,
    /// The mature setting the current query uses; no page is read until it is known.
    mature: bool,
    mature_known: bool,
    /// Mature cards the player unblurred; cleared when the setting flips.
    revealed: HashSet<GbId>,
    /// The first page of the current query landed.
    loaded: bool,
    error: Option<String>,
    /// Limits sync re-reads to one per `REQUERY_INTERVAL`, with one trailing run.
    throttle: Throttle,
    trailing: Option<Task<()>>,
    /// The maps sync's last seen `fetched`, so only a new count asks for a re-read.
    sync_fetched: u32,
    /// A re-read is owed for when the hidden tab shows again.
    stale: bool,
    requeries: usize,
    /// This tab's own `RemoteImages` viewer.
    viewer: String,
    visible: Range<usize>,
    hidden: bool,
    _subscriptions: Vec<Subscription>,
}

impl GbMapsTab {
    /// Needs the `Services` global. Reads the gamemodes now and page one once the mature
    /// setting is known.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let viewer = format!("gb-maps-tab:{}", cx.entity_id());
        let mature = MatureAddons::global(cx);
        let (mature_known, show_mature) = {
            let m = mature.read(cx);
            (m.known(), m.show())
        };
        // Thumbnails redraw the grid as they land, and the wants go back when the tab goes.
        let mut subscriptions = vec![cx.observe(&mature, Self::mature_changed)];
        if let Some(images) = RemoteImages::global(cx) {
            subscriptions.push(cx.observe(&images, |_, _, cx| cx.notify()));
            let released = viewer.clone();
            subscriptions.push(cx.on_release(move |_, cx| {
                images.update(cx, |images, cx| images.want(&released, Vec::new(), cx));
            }));
        }
        let mut grid = CardGrid::store(ids::MAPS_GB_GRID, MAX_COLUMNS);
        let this = cx.entity().downgrade();
        grid.on_visible(move |range, cx| {
            let _ = this.update(cx, |tab, cx| tab.visible_changed(range, cx));
        });
        let this = cx.entity().downgrade();
        grid.on_near_end(move |cx| {
            let _ = this.update(cx, |tab, cx| tab.next_page(cx));
        });
        let search_input = cx.new(|cx| InputState::new(window, cx).placeholder("Search maps..."));
        let labels = LabelOption::all(ids::MAPS_GB_SORT, SORTS.iter().map(|(label, _)| *label));
        let sort_select =
            cx.new(|cx| SelectState::new(labels, Some(IndexPath::default()), window, cx));
        let rows = vec![CategoryOption::new(
            ids::MAPS_GB_GAMEMODE,
            None,
            "All gamemodes",
        )];
        let gamemode_select =
            cx.new(|cx| SelectState::new(rows, Some(IndexPath::default()), window, cx));
        subscriptions.extend([
            cx.subscribe(&search_input, |tab, input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    let text = input.read(cx).value().to_string();
                    tab.search_typed(text, cx);
                }
            }),
            cx.subscribe(
                &sort_select,
                |tab, _, event: &SelectEvent<Vec<LabelOption>>, cx| {
                    if let SelectEvent::Confirm(Some(label)) = event
                        && let Some((_, sort)) = SORTS.iter().find(|(l, _)| l == label)
                        && *sort != tab.sort
                    {
                        tab.sort = *sort;
                        tab.restart(cx);
                    }
                },
            ),
            cx.subscribe(
                &gamemode_select,
                |tab, _, event: &SelectEvent<Vec<CategoryOption>>, cx| {
                    if let SelectEvent::Confirm(Some(id)) = event
                        && *id != tab.gamemode
                    {
                        tab.gamemode = *id;
                        tab.restart(cx);
                    }
                },
            ),
        ]);
        // Each committed page asks for a throttled re-read; the end of a run re-reads at once
        // and refills the gamemodes, which a first launch has none of until the sync stores them.
        if let Some(sync) = maps_sync(cx) {
            subscriptions.push(cx.observe(&sync, |tab, sync, cx| tab.sync_changed(&sync, cx)));
            subscriptions.push(cx.subscribe_in(
                &sync,
                window,
                |tab, _, event: &MapsSyncEvent, window, cx| {
                    if matches!(event, MapsSyncEvent::Finished) {
                        tab.requery(cx);
                        tab.load_gamemodes(window, cx);
                    }
                },
            ));
        }
        let mut tab = Self {
            feed: Feed::new(),
            grid,
            sort: GbMapSort::MostDownloaded,
            gamemode: None,
            search: String::new(),
            gamemodes: Vec::new(),
            search_input,
            sort_select,
            gamemode_select,
            debounce: None,
            mature: show_mature,
            mature_known,
            revealed: HashSet::new(),
            loaded: false,
            error: None,
            throttle: Throttle::new(REQUERY_INTERVAL),
            trailing: None,
            sync_fetched: 0,
            stale: false,
            requeries: 0,
            viewer,
            visible: 0..0,
            hidden: false,
            _subscriptions: subscriptions,
        };
        tab.load_gamemodes(window, cx);
        if mature_known {
            tab.next_page(cx);
        }
        tab
    }

    /// The cards read so far for the current query.
    pub fn cards(&self) -> &[GbMapCard] {
        self.feed.items()
    }

    /// The order the catalogue is listed in.
    pub fn sort(&self) -> GbMapSort {
        self.sort
    }

    /// The catalogue's first-level gamemodes, A to Z; empty before the first sync.
    pub fn gamemodes(&self) -> &[GbMapGamemode] {
        &self.gamemodes
    }

    /// The chosen gamemode; `None` lists every map.
    pub fn gamemode(&self) -> Option<u32> {
        self.gamemode
    }

    /// The search the tab queries, once typing has paused.
    pub fn search(&self) -> &str {
        &self.search
    }

    /// Searches for `text` once typing pauses for `SEARCH_DEBOUNCE`; a newer keystroke replaces
    /// the pending one, and text that is the same query as the current search changes nothing.
    pub fn search_typed(&mut self, text: String, cx: &mut Context<Self>) {
        let timer = cx.background_executor().timer(SEARCH_DEBOUNCE);
        self.debounce = Some(cx.spawn(async move |tab, cx| {
            timer.await;
            let _ = tab.update(cx, |tab, cx| {
                if same_search(&tab.search, &text) {
                    return;
                }
                tab.search = text;
                tab.restart(cx);
            });
        }));
    }

    /// How many reloads from the top have landed, for tests.
    pub fn requeries(&self) -> usize {
        self.requeries
    }

    /// Whether the first page of the current query landed, empty or not.
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// The last failed read in player words, until a page lands.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Whether the player unblurred mature card `id`.
    pub fn is_revealed(&self, id: GbId) -> bool {
        self.revealed.contains(&id)
    }

    /// Unblurs mature card `id` and asks for its sharp thumbnail.
    pub fn reveal(&mut self, id: GbId, cx: &mut Context<Self>) {
        if self.revealed.insert(id) {
            self.want_thumbnails(cx);
            cx.notify();
        }
    }

    /// Whether the tab is hidden, holding no image wants.
    pub fn is_hidden(&self) -> bool {
        self.hidden
    }

    /// Hides or shows the tab. Hidden, it drops its image wants and holds sync re-reads;
    /// shown again, it rearms the grid's hooks and re-reads once if the sync committed meanwhile.
    pub fn set_hidden(&mut self, hidden: bool, cx: &mut Context<Self>) {
        if hidden == std::mem::replace(&mut self.hidden, hidden) {
            return;
        }
        if hidden {
            self.stale |= self.trailing.take().is_some();
            if let Some(images) = RemoteImages::global(cx) {
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

    /// Clears the failure and reads the failed page again.
    pub fn retry(&mut self, cx: &mut Context<Self>) {
        self.error = None;
        self.next_page(cx);
        cx.notify();
    }

    // The first known value reads page one; a later flip relists with or without mature maps.
    fn mature_changed(&mut self, mature: Entity<MatureAddons>, cx: &mut Context<Self>) {
        let (known, show) = {
            let m = mature.read(cx);
            (m.known(), m.show())
        };
        if !known || (self.mature_known && show == self.mature) {
            return;
        }
        if self.mature_known {
            self.revealed.clear();
        }
        self.mature_known = true;
        self.mature = show;
        self.restart(cx);
    }

    // A new query from the first page, the grid rearmed since new cards may land on the range
    // and count its hooks last fired for.
    fn restart(&mut self, cx: &mut Context<Self>) {
        self.feed.reset();
        self.loaded = false;
        self.error = None;
        self.grid.rearm();
        self.grid
            .scroll_handle()
            .0
            .borrow()
            .base_handle
            .set_offset(point(px(0.), px(0.)));
        self.visible = 0..0;
        self.next_page(cx);
        cx.notify();
    }

    fn query(&self) -> GbMapQuery {
        GbMapQuery::new(self.sort, PAGE)
            .with_gamemode(self.gamemode)
            .with_search(self.search.clone())
            .with_mature(self.mature)
    }

    // Reads the page after the last card on a pool thread; a page of an older query is dropped.
    fn next_page(&mut self, cx: &mut Context<Self>) {
        if !self.mature_known {
            return;
        }
        let Some((after, generation)) = self.feed.next_page() else {
            return;
        };
        let query = match after {
            Some(cursor) => self.query().with_after(cursor),
            None => self.query(),
        };
        let reading = run_blocking(cx, move |s| {
            s.store
                .gb_map_cards(&query)
                .map_err(|e| OpError::core("load the maps", e))
        });
        cx.spawn(async move |tab, cx| {
            let page = reading.await;
            let _ = tab.update(cx, |tab, cx| {
                match page {
                    Ok(page) => {
                        if tab.feed.page_loaded(generation, page) {
                            tab.loaded = true;
                            tab.error = None;
                            tab.want_thumbnails(cx);
                            tab.after_landing(cx);
                        }
                    }
                    Err(err) if generation == tab.feed.generation() => {
                        tab.feed.page_failed(generation);
                        tab.loaded = true;
                        tab.error = Some(read_failed(&err));
                        if tab.feed.take_reload_wanted() {
                            tab.start_reload(cx);
                        }
                    }
                    Err(_) => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    // Runs what waited on a landed page or reload: a held-back re-read, then a near-end the
    // in-flight request swallowed.
    fn after_landing(&mut self, cx: &mut Context<Self>) {
        if self.feed.take_reload_wanted() {
            self.start_reload(cx);
        }
        if self.feed.take_more_wanted() {
            self.next_page(cx);
        }
    }

    // A tick of the maps sync: repaints the banner, and a new nonzero `fetched` asks the
    // throttle for a re-read.
    fn sync_changed(&mut self, sync: &Entity<MapsSync>, cx: &mut Context<Self>) {
        cx.notify();
        let fetched = sync.read(cx).progress().map_or(0, |t| t.fetched);
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
                self.trailing = Some(cx.spawn(async move |tab, cx| {
                    timer.await;
                    let _ = tab.update(cx, |tab, cx| tab.requery(cx));
                }));
            }
            Gate::Pending => {}
        }
    }

    // A re-read now, marking the throttle's run; a hidden tab only notes that one is owed.
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
    // visible card is back in it, then swaps it in with that card kept in place on screen.
    fn start_reload(&mut self, cx: &mut Context<Self>) {
        if !self.mature_known {
            return;
        }
        let Some(generation) = self.feed.begin_reload() else {
            return;
        };
        let anchor = self
            .first_visible_index()
            .and_then(|ix| self.feed.items().get(ix))
            .map(FeedItem::feed_id);
        let query = self.query();
        let loaded = self.feed.items().len();
        let reading = run_blocking(cx, move |s| {
            reload(loaded, anchor.as_ref(), |after, limit| {
                let mut page = query.clone();
                page.limit = limit;
                let page = match after {
                    Some(cursor) => page.with_after(cursor),
                    None => page,
                };
                s.store
                    .gb_map_cards(&page)
                    .map_err(|e| OpError::core("load the maps", e))
            })
        });
        cx.spawn(async move |tab, cx| {
            let reloaded = reading.await;
            let _ = tab.update(cx, |tab, cx| {
                tab.requeries += 1;
                match reloaded {
                    Ok((items, end)) => tab.apply_reload(generation, items, end, cx),
                    Err(_) => tab.feed.reload_failed(generation),
                }
                if generation == tab.feed.generation() {
                    tab.after_landing(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn apply_reload(
        &mut self,
        generation: u64,
        items: Vec<GbMapCard>,
        end: bool,
        cx: &mut Context<Self>,
    ) {
        if generation != self.feed.generation() {
            return;
        }
        let handle = self.grid.scroll_handle().0.borrow().base_handle.clone();
        let offset = handle.offset();
        let old: Vec<GbId> = self.feed.items().iter().map(FeedItem::feed_id).collect();
        let new: Vec<GbId> = items.iter().map(FeedItem::feed_id).collect();
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
        self.loaded = true;
        if let Some(scroll_y) = scroll_y {
            handle.set_offset(point(offset.x, px(-scroll_y)));
        }
        self.want_thumbnails(cx);
    }

    // The index of the first card of the first visible row, once the grid has laid out.
    fn first_visible_index(&self) -> Option<usize> {
        let scroll_y = -f32::from(self.grid.scroll_handle().0.borrow().base_handle.offset().y);
        self.grid.row_height().and_then(|row_height| {
            first_visible(scroll_y, f32::from(row_height), self.grid.columns())
        })
    }

    // Fills the gamemode Select from the stored tree; a failure leaves it at "All gamemodes".
    fn load_gamemodes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let reading = run_blocking(cx, |s| {
            s.store
                .gb_map_gamemodes()
                .map_err(|e| OpError::core("load the gamemodes", e))
        });
        cx.spawn_in(window, async move |tab, cx| {
            let gamemodes = match reading.await {
                Ok(gamemodes) => gamemodes,
                Err(err) => {
                    tracing::warn!(%err, "could not read the map gamemodes");
                    return;
                }
            };
            let _ = tab.update_in(cx, |tab, window, cx| {
                let rows = std::iter::once(CategoryOption::new(
                    ids::MAPS_GB_GAMEMODE,
                    None,
                    "All gamemodes",
                ))
                .chain(gamemodes.iter().map(|g| {
                    CategoryOption::new(ids::MAPS_GB_GAMEMODE, Some(g.id), g.name.clone())
                }))
                .collect();
                // A chosen gamemode the new tree no longer has falls back to every map.
                let gone = tab
                    .gamemode
                    .is_some_and(|id| !gamemodes.iter().any(|g| g.id == id));
                tab.gamemode_select.update(cx, |select, cx| {
                    select.set_items(rows, window, cx);
                    if gone {
                        select.set_selected_index(Some(IndexPath::default()), window, cx);
                    }
                });
                tab.gamemodes = gamemodes;
                if gone {
                    tab.gamemode = None;
                    tab.restart(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn visible_changed(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        self.visible = range;
        self.want_thumbnails(cx);
    }

    // A mature card stays blurred until the player reveals it.
    fn blurred(&self, card: &GbMapCard) -> bool {
        card.card.mature && !self.revealed.contains(&card.card.id)
    }

    // The drawn cards' thumbnails, asked for again on every page since a page can fill the
    // drawn range without changing it.
    fn want_thumbnails(&mut self, cx: &mut Context<Self>) {
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
            .filter_map(|card| {
                let url = card.card.thumbnail_url.as_deref()?;
                Some(if self.blurred(card) {
                    ImageKey::card_blurred(url)
                } else {
                    ImageKey::card(url)
                })
            })
            .collect();
        images.update(cx, |images, cx| images.want(&self.viewer, keys, cx));
    }

    // The Addon Store's toolbar without the weapon Select: search, then sort and gamemode.
    fn toolbar(&self, cx: &App) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let select = |id: &'static str, width: f32, state: AnyElement| {
            div().id(id).test_support().w(px(width)).child(state)
        };
        h_flex()
            .flex_wrap()
            .items_center()
            .gap_4()
            .child(select(
                ids::MAPS_GB_SEARCH,
                288.,
                Input::new(&self.search_input)
                    .h_9()
                    .prefix(Icon::new(IconName::Search).text_color(muted))
                    .into_any_element(),
            ))
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(muted)
                            .child("Sort:"),
                    )
                    .child(select(
                        ids::MAPS_GB_SORT,
                        SORT_WIDTH,
                        Select::new(&self.sort_select).h_9().into_any_element(),
                    )),
            )
            .child(select(
                ids::MAPS_GB_GAMEMODE,
                192.,
                Select::new(&self.gamemode_select).h_9().into_any_element(),
            ))
    }

    // Cards fill the grid; before the first card, a pending first page shows skeletons. With
    // none, a running sync leaves the banner alone, a failed one its message with Retry, and
    // an ended query says "No maps found".
    fn body(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.feed.items().is_empty() {
            return Some(self.cards_grid(cx));
        }
        if !self.loaded {
            let columns = columns_for_store(
                window.viewport_size().width - SHELL_CHROME_WIDTH,
                MAX_COLUMNS,
            );
            return Some(skeleton_cards(
                ids::MAPS_GB_LOADING,
                "Loading...",
                SKELETON_CARDS,
                columns,
                SkeletonTop::Image,
                cx,
            ));
        }
        if self.error.is_some() || !self.feed.is_end() {
            return None;
        }
        if let Some(sync) = maps_sync(cx) {
            let sync = sync.read(cx);
            if sync.is_running() {
                return None;
            }
            // The sync's failure explains an empty catalogue, not a filter that matches nothing.
            let unfiltered = self.search.trim().is_empty() && self.gamemode.is_none();
            if let Some(message) = sync.error().filter(|_| unfiltered) {
                let message = message.to_owned();
                let sync = maps_sync(cx).map(|s| s.downgrade());
                let on_retry = move |_: &mut Window, cx: &mut App| {
                    if let Some(sync) = sync.as_ref().and_then(|s| s.upgrade()) {
                        sync.update(cx, |sync, cx| sync.start(false, cx));
                    }
                };
                return Some(
                    error_line(
                        ids::MAPS_GB_ERROR,
                        ids::MAPS_GB_RETRY,
                        message,
                        on_retry,
                        cx,
                    )
                    .into_any_element(),
                );
            }
        }
        let hint = if self.search.trim().is_empty() {
            "Try another gamemode."
        } else {
            "Try another search."
        };
        Some(empty_state_card(
            ids::MAPS_GB_EMPTY,
            IconName::Store,
            "No maps found",
            Some(ids::STORE_EMPTY_HINT),
            hint,
            cx,
        ))
    }

    // Each card reads its row from the entity while the list lays out, so a frame clones only
    // the drawn cards.
    fn cards_grid(&self, cx: &mut Context<Self>) -> AnyElement {
        let this = cx.entity().downgrade();
        self.grid
            .render_sized(self.feed.items().len(), card_height, move |ix, _, cx| {
                let card = this.upgrade().and_then(|tab| {
                    let tab = tab.read(cx);
                    let card = tab.feed.items().get(ix)?;
                    Some((card.card.clone(), tab.blurred(card)))
                });
                let Some((card, blurred)) = card else {
                    return div().into_any_element();
                };
                let image = card.thumbnail_url.as_deref().and_then(|url| {
                    let key = if blurred {
                        ImageKey::card_blurred(url)
                    } else {
                        ImageKey::card(url)
                    };
                    RemoteImages::global(cx).map(|images| images.read(cx).image(&key))
                });
                let id = card.id;
                let opened = card.clone();
                let open: OpenAddon = Rc::new(move |window, cx: &mut App| {
                    open_addon_detail(AddonSection::MAPS, &opened, window, cx);
                });
                let tab = this.clone();
                let reveal = blurred.then(|| -> OpenAddon {
                    Rc::new(move |_, cx: &mut App| {
                        let _ = tab.update(cx, |tab, cx| tab.reveal(id, cx));
                    })
                });
                gb_addon_card(
                    AddonSection::MAPS.key(),
                    &card,
                    image,
                    None,
                    open,
                    reveal,
                    cx,
                )
            })
    }
}

impl Render for GbMapsTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // A failed read's message sits above the grid, or alone before the first card.
        let error = self.error.clone().map(|message| {
            let this = cx.entity().downgrade();
            error_line(
                ids::MAPS_GB_ERROR,
                ids::MAPS_GB_RETRY,
                message,
                move |_, cx| {
                    let _ = this.update(cx, |tab, cx| tab.retry(cx));
                },
                cx,
            )
        });
        let banner = maps_sync_banner(!self.feed.items().is_empty(), cx);
        v_flex()
            .size_full()
            .gap_6()
            .child(self.toolbar(cx))
            .children(banner)
            .children(error)
            .children(self.body(window, cx))
    }
}
