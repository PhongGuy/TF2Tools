//! The Maps page's GameBanana source: the synced maps catalogue in the add-on card grid,
//! read in keyset pages from the local database as the player scrolls.

use std::collections::HashSet;
use std::ops::Range;
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::select::{Select, SelectState};
use gpui_kit::component::{ActiveTheme as _, Icon, IndexPath, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, Styled as _, Subscription, TestSupportExt as _,
    Window, div, point, px,
};
use tf2tools_core::{
    AddonSection, GbId, GbMapCard, GbMapCursor, GbMapGamemode, GbMapQuery, GbMapSort,
};

use crate::addon_detail::open_addon_detail;
use crate::ids;
use crate::mature::MatureAddons;
use crate::pages::addon_store::card::{OpenAddon, card_height, gb_addon_card};
use crate::pages::addon_store::gamebanana_tab::CategoryOption;
use crate::pages::common::{SkeletonTop, empty_state_card, skeleton_cards};
use crate::remote_images::{ImageKey, RemoteImages};
use crate::services::{OpError, run_blocking};
use crate::store_grid::feed::{Feed, FeedItem, PAGE};
use crate::store_messages::read_failed;
use crate::widgets::card_grid::{CardGrid, SHELL_CHROME_WIDTH, columns_for_store};
use crate::widgets::label_option::LabelOption;
use crate::widgets::store_error::error_line;

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
    /// The mature setting the current query uses; no page is read until it is known.
    mature: bool,
    mature_known: bool,
    /// Mature cards the player unblurred; cleared when the setting flips.
    revealed: HashSet<GbId>,
    /// The first page of the current query landed.
    loaded: bool,
    error: Option<String>,
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
            mature: show_mature,
            mature_known,
            revealed: HashSet::new(),
            loaded: false,
            error: None,
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

    /// Hides or shows the tab. Hidden, it drops its image wants; shown again, it rearms the
    /// grid's hooks so the drawn range and the near end are reported afresh.
    pub fn set_hidden(&mut self, hidden: bool, cx: &mut Context<Self>) {
        if hidden == std::mem::replace(&mut self.hidden, hidden) {
            return;
        }
        if hidden {
            if let Some(images) = RemoteImages::global(cx) {
                images.update(cx, |images, cx| images.want(&self.viewer, Vec::new(), cx));
            }
            return;
        }
        self.grid.rearm();
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
                            if tab.feed.take_more_wanted() {
                                tab.next_page(cx);
                            }
                        }
                    }
                    Err(err) if generation == tab.feed.generation() => {
                        tab.feed.page_failed(generation);
                        tab.loaded = true;
                        tab.error = Some(read_failed(&err));
                    }
                    Err(_) => {}
                }
                cx.notify();
            });
        })
        .detach();
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
                tab.gamemode_select
                    .update(cx, |select, cx| select.set_items(rows, window, cx));
                tab.gamemodes = gamemodes;
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

    // Cards fill the grid; before the first card, a pending first page shows skeletons and an
    // ended query "No maps found".
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
        self.feed.is_end().then(|| {
            empty_state_card(
                ids::MAPS_GB_EMPTY,
                IconName::Store,
                "No maps found",
                Some(ids::STORE_EMPTY_HINT),
                "Try another gamemode.",
                cx,
            )
        })
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
        v_flex()
            .size_full()
            .gap_6()
            .child(self.toolbar(cx))
            .children(error)
            .children(self.body(window, cx))
    }
}
