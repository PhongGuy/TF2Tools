//! One GameBanana section of the Addon Store: a grid that streams cards from
//! `AddonCatalog::step`, one step at a time, as the player scrolls.

use std::collections::HashSet;
use std::ops::Range;
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::select::{Select, SelectEvent, SelectItem, SelectState};
use gpui_kit::component::{ActiveTheme as _, Icon, IndexPath, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, ElementId, Entity, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Render, SharedString,
    Styled as _, Subscription, Task, TestSupportExt as _, Window, div, point, px,
};
use tf2tools_core::{
    AddonCard, AddonCategory, AddonCursor, AddonQuery, AddonSection, AddonSort, AddonStep,
    BrowseError, GbId,
};

use super::card::{CARD_TEXT_HEIGHT, OpenAddon, gb_addon_card};
use crate::addon_detail::open_addon_detail;
use crate::addon_feed::{AddonFeed, Keyed, StepAfter};
use crate::ids;
use crate::mature::MatureAddons;
use crate::pages::common::{SkeletonTop, empty_state_card, skeleton_cards};
use crate::remote_images::{ImageKey, RemoteImages};
use crate::services::Services;
use crate::store_grid::SEARCH_DEBOUNCE;
use crate::store_messages::{StoreFailure, describe_browse, listing_stopped};
use crate::tokio_bridge;
use crate::widgets::card_grid::{CardGrid, SHELL_CHROME_WIDTH, columns_for_store};
use crate::widgets::label_option::LabelOption;
use crate::widgets::store_error::store_error_line;

/// The most columns an add-on grid lays out.
const MAX_COLUMNS: usize = 4;

/// v5 draws eight skeleton cards while a store loads.
const SKELETON_CARDS: usize = 8;

/// The sort Select's labels, in menu order.
const SORTS: [(&str, AddonSort); 2] = [
    ("Most downloaded", AddonSort::MostDownloaded),
    ("Newest", AddonSort::Newest),
];

type Labels = Entity<SelectState<Vec<LabelOption>>>;
type Categories = Entity<SelectState<Vec<CategoryOption>>>;

/// A subcategory Select row; `None` is the "All" row that lists the whole parent.
#[derive(Debug, Clone)]
pub struct CategoryOption {
    value: Option<u32>,
    title: SharedString,
    id: ElementId,
}

impl CategoryOption {
    // "All" first, then `children`, each row keyed under `select` by its id ("all" for None).
    fn rows(select: &str, all: &str, children: &[AddonCategory]) -> Vec<CategoryOption> {
        let row = |value: Option<u32>, title: SharedString| CategoryOption {
            value,
            title,
            id: ids::select_option(select, &value.map_or("all".into(), |i| i.to_string())),
        };
        std::iter::once(row(None, SharedString::new(all)))
            .chain(
                children
                    .iter()
                    .map(|c| row(Some(c.id()), c.name().to_owned().into())),
            )
            .collect()
    }
}

// Kit rows carry no test id, so each row draws its own for headless clicks and the pilot.
impl SelectItem for CategoryOption {
    type Value = Option<u32>;

    fn title(&self) -> SharedString {
        self.title.clone()
    }

    fn render(&self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .id(self.id.clone())
            .test_support()
            .child(self.title.clone())
    }

    fn value(&self) -> &Option<u32> {
        &self.value
    }
}

impl Keyed for AddonCard {
    type Key = GbId;

    fn key(&self) -> GbId {
        self.id
    }
}

/// A GameBanana section's grid. Each step runs on the tokio bridge and streams its cards back
/// through a channel as pages land; the grid asks for the next step near its end.
pub struct AddonTab {
    section: AddonSection,
    feed: AddonFeed<AddonCard, AddonCursor>,
    grid: CardGrid,
    sort: AddonSort,
    category: Option<u32>,
    weapon: Option<u32>,
    search: String,
    /// The root's children, then the chosen class's children; empty until each arrives.
    categories: Vec<AddonCategory>,
    weapons: Vec<AddonCategory>,
    search_input: Entity<InputState>,
    sort_select: Labels,
    category_select: Categories,
    weapon_select: Categories,
    debounce: Option<Task<()>>,
    categories_task: Option<Task<()>>,
    weapons_task: Option<Task<()>>,
    /// The mature setting the current query uses; no step runs until it is known.
    mature: bool,
    mature_known: bool,
    /// Mature cards the player unblurred; cleared when the setting flips.
    revealed: HashSet<GbId>,
    error: Option<String>,
    step_task: Option<Task<()>>,
    /// This tab's own `RemoteImages` viewer, so tabs alive together keep separate wants.
    viewer: String,
    visible: Range<usize>,
    hidden: bool,
    _subscriptions: Vec<Subscription>,
}

impl AddonTab {
    /// Needs the `Services` global. Starts the first step once the mature setting is known.
    pub fn new(section: AddonSection, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let viewer = format!("addon-tab:{}", cx.entity_id());
        let mature = MatureAddons::global(cx);
        let (mature_known, show_mature) = {
            let m = mature.read(cx);
            (m.known(), m.show())
        };
        // Thumbnails redraw the grid as they land, and the wants go back when the tab goes.
        let mut subscriptions = Vec::new();
        if let Some(images) = RemoteImages::global(cx) {
            subscriptions.push(cx.observe(&images, |_, _, cx| cx.notify()));
            let released = viewer.clone();
            subscriptions.push(cx.on_release(move |_, cx| {
                images.update(cx, |images, cx| images.want(&released, Vec::new(), cx));
            }));
        }
        let mut grid = CardGrid::store(ids::ADDON_GRID, MAX_COLUMNS);
        let this = cx.entity().downgrade();
        grid.on_visible(move |range, cx| {
            let _ = this.update(cx, |tab, cx| tab.visible_changed(range, cx));
        });
        let this = cx.entity().downgrade();
        grid.on_near_end(move |cx| {
            let _ = this.update(cx, |tab, cx| tab.next_step(cx));
        });
        let placeholder = format!("Search {}...", section.title().to_lowercase());
        let search_input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let labels = LabelOption::all(ids::ADDON_SORT, SORTS.iter().map(|(label, _)| *label));
        let sort_select =
            cx.new(|cx| SelectState::new(labels, Some(IndexPath::default()), window, cx));
        let rows = CategoryOption::rows(ids::ADDON_CATEGORY, "All", &[]);
        let category_select =
            cx.new(|cx| SelectState::new(rows, Some(IndexPath::default()), window, cx));
        let rows = CategoryOption::rows(ids::ADDON_WEAPON, "All weapons", &[]);
        let weapon_select = cx.new(|cx| SelectState::new(rows, None, window, cx));
        subscriptions.extend([
            cx.observe(&mature, Self::mature_changed),
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
                    {
                        tab.set_sort(*sort, cx);
                    }
                },
            ),
            cx.subscribe_in(
                &category_select,
                window,
                |tab, _, event: &SelectEvent<Vec<CategoryOption>>, window, cx| {
                    if let SelectEvent::Confirm(Some(id)) = event {
                        tab.choose_category(*id, window, cx);
                    }
                },
            ),
            cx.subscribe(
                &weapon_select,
                |tab, _, event: &SelectEvent<Vec<CategoryOption>>, cx| {
                    if let SelectEvent::Confirm(Some(id)) = event {
                        tab.choose_weapon(*id, cx);
                    }
                },
            ),
        ]);
        let mut tab = Self {
            section,
            feed: AddonFeed::new(AddonCursor::START),
            grid,
            sort: AddonSort::MostDownloaded,
            category: None,
            weapon: None,
            search: String::new(),
            categories: Vec::new(),
            weapons: Vec::new(),
            search_input,
            sort_select,
            category_select,
            weapon_select,
            debounce: None,
            categories_task: None,
            weapons_task: None,
            mature: show_mature,
            mature_known,
            revealed: HashSet::new(),
            error: None,
            step_task: None,
            viewer,
            visible: 0..0,
            hidden: false,
            _subscriptions: subscriptions,
        };
        tab.categories_task = Some(tab.load_categories(section.root_category(), window, cx));
        tab.restart(cx);
        tab
    }

    pub fn sort(&self) -> AddonSort {
        self.sort
    }

    /// Lists the section in `sort` order from the first page; the same sort changes nothing.
    pub fn set_sort(&mut self, sort: AddonSort, cx: &mut Context<Self>) {
        if std::mem::replace(&mut self.sort, sort) != sort {
            self.restart(cx);
        }
    }

    /// The root's children, excluded ones removed; empty until they arrive.
    pub fn categories(&self) -> &[AddonCategory] {
        &self.categories
    }

    /// The chosen class's children; empty when none is chosen.
    pub fn weapons(&self) -> &[AddonCategory] {
        &self.weapons
    }

    pub fn category(&self) -> Option<u32> {
        self.category
    }

    pub fn weapon(&self) -> Option<u32> {
        self.weapon
    }

    /// The search the tab queries, once typing has paused.
    pub fn search(&self) -> &str {
        &self.search
    }

    /// Searches for `text` once typing pauses for `SEARCH_DEBOUNCE`; a newer keystroke replaces
    /// the pending one, and text that trims to the current search changes nothing.
    pub fn search_typed(&mut self, text: String, cx: &mut Context<Self>) {
        let timer = cx.background_executor().timer(SEARCH_DEBOUNCE);
        self.debounce = Some(cx.spawn(async move |tab, cx| {
            timer.await;
            let _ = tab.update(cx, |tab, cx| {
                if tab.search.trim() == text.trim() {
                    return;
                }
                tab.search = text;
                tab.restart(cx);
            });
        }));
    }

    /// False in search mode, whose matches the sort and the Selects cannot narrow.
    pub fn selects_enabled(&self) -> bool {
        self.search.trim().is_empty()
    }

    // A new subcategory drops the weapon; a Skins class also fetches its weapons.
    fn choose_category(&mut self, id: Option<u32>, window: &mut Window, cx: &mut Context<Self>) {
        if id == self.category {
            return;
        }
        self.category = id;
        self.weapon = None;
        self.weapons.clear();
        self.weapons_task = id
            .filter(|id| self.section.weapon_parents().contains(id))
            .map(|class| self.load_categories(class, window, cx));
        self.restart(cx);
    }

    fn choose_weapon(&mut self, id: Option<u32>, cx: &mut Context<Self>) {
        if id != self.weapon {
            self.weapon = id;
            self.restart(cx);
        }
    }

    // Fetches `parent`'s children on tokio. They fill the subcategory Select for the root and
    // the weapon Select for a class; a failure leaves the Select at "All" and browsing goes on.
    fn load_categories(
        &self,
        parent: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        if !tokio_bridge::is_running(cx) {
            tokio_bridge::init(cx);
        }
        let catalog = cx.global::<Services>().store.addon_catalog();
        let section = self.section;
        let job = tokio_bridge::spawn(cx, async move { catalog.categories(section, parent).await });
        cx.spawn_in(window, async move |tab, cx| {
            let children = match job.await {
                Ok(Ok(children)) => children,
                Ok(Err(err)) => {
                    tracing::warn!(parent, %err, "could not list add-on subcategories");
                    return;
                }
                Err(err) => {
                    tracing::warn!(parent, %err, "the add-on subcategory task stopped");
                    return;
                }
            };
            let _ = tab.update_in(cx, |tab, window, cx| {
                tab.categories_loaded(parent, children, window, cx)
            });
        })
    }

    fn categories_loaded(
        &mut self,
        parent: u32,
        children: Vec<AddonCategory>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if parent == self.section.root_category() {
            let rows = CategoryOption::rows(ids::ADDON_CATEGORY, "All", &children);
            self.category_select
                .update(cx, |select, cx| select.set_items(rows, window, cx));
            self.categories = children;
        } else if self.category == Some(parent) {
            let rows = CategoryOption::rows(ids::ADDON_WEAPON, "All weapons", &children);
            self.weapon_select.update(cx, |select, cx| {
                select.set_items(rows, window, cx);
                select.set_selected_index(None, window, cx);
            });
            self.weapons = children;
        }
        cx.notify();
    }

    /// The cards streamed so far for the current query.
    pub fn cards(&self) -> &[AddonCard] {
        self.feed.items()
    }

    /// Whether a step runs, or the first waits for the mature setting.
    pub fn is_loading(&self) -> bool {
        self.feed.is_loading() || !self.mature_known
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

    // The first known value starts the listing; a later flip relists with or without mature
    // items, which the page cache answers without new requests.
    fn mature_changed(&mut self, mature: Entity<MatureAddons>, cx: &mut Context<Self>) {
        let (known, show) = {
            let m = mature.read(cx);
            (m.known(), m.show())
        };
        if !known {
            return;
        }
        if !self.mature_known {
            self.mature_known = true;
            self.mature = show;
            self.restart(cx);
        } else if show != self.mature {
            self.mature = show;
            self.revealed.clear();
            self.restart(cx);
        }
    }

    // A mature card stays blurred until the player reveals it.
    fn blurred(&self, card: &AddonCard) -> bool {
        card.mature && !self.revealed.contains(&card.id)
    }

    /// Whether the tab waits behind "Load more".
    pub fn wants_load_more(&self) -> bool {
        self.feed.wants_load_more()
    }

    /// Whether the query ended without a single card.
    pub fn is_no_results(&self) -> bool {
        self.feed.is_no_results()
    }

    /// The last step's failure in player words, until the next query.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Runs the step the player asked for with "Load more"; does nothing unless it is offered.
    pub fn load_more(&mut self, cx: &mut Context<Self>) {
        if let Some((from, generation)) = self.feed.load_more() {
            self.run_step(from, generation, cx);
            cx.notify();
        }
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

    /// Clears the failure and runs the failed step again from where it stopped.
    pub fn retry(&mut self, cx: &mut Context<Self>) {
        self.error = None;
        self.next_step(cx);
        cx.notify();
    }

    // The section in the tab's order, narrowed to the weapon or else the subcategory, with
    // mature items as the setting says.
    fn query(&self) -> AddonQuery {
        AddonQuery::new(self.section, self.sort)
            .with_category(self.weapon.or(self.category))
            .with_search(self.search.clone())
            .with_mature(self.mature)
    }

    // A new query from the first page. The grid is rearmed and scrolled to the top, since the
    // new cards may land on the range and count its hooks last fired for.
    fn restart(&mut self, cx: &mut Context<Self>) {
        self.feed.reset(AddonCursor::START);
        self.step_task = None;
        self.error = None;
        self.grid.rearm();
        self.grid
            .scroll_handle()
            .0
            .borrow()
            .base_handle
            .set_offset(point(px(0.), px(0.)));
        self.visible = 0..0;
        if self.mature_known {
            self.next_step(cx);
        }
        cx.notify();
    }

    // Starts the next step unless one runs, the listing ended or "Load more" waits.
    fn next_step(&mut self, cx: &mut Context<Self>) {
        if let Some((from, generation)) = self.feed.begin_step() {
            self.run_step(from, generation, cx);
            cx.notify();
        }
    }

    // The step reads pages on tokio and hands each page's cards to the channel, so cards
    // appear page by page; the sink's sender drops with the step and ends the receive loop.
    fn run_step(&mut self, from: AddonCursor, generation: u64, cx: &mut Context<Self>) {
        if !tokio_bridge::is_running(cx) {
            tokio_bridge::init(cx);
        }
        let catalog = cx.global::<Services>().store.addon_catalog();
        let query = self.query();
        let (tx, rx) = async_channel::unbounded::<Vec<AddonCard>>();
        let job = tokio_bridge::spawn(cx, async move {
            let sink = move |cards: Vec<AddonCard>| {
                let _ = tx.try_send(cards);
            };
            catalog.step(&query, from, &sink).await
        });
        self.step_task = Some(cx.spawn(async move |tab, cx| {
            while let Ok(cards) = rx.recv().await {
                if tab
                    .update(cx, |tab, cx| tab.chunk(generation, cards, cx))
                    .is_err()
                {
                    return;
                }
            }
            let result = job.await;
            let _ = tab.update(cx, |tab, cx| tab.step_finished(generation, result, cx));
        }));
    }

    fn chunk(&mut self, generation: u64, cards: Vec<AddonCard>, cx: &mut Context<Self>) {
        if self.feed.chunk(generation, cards) > 0 {
            self.want_thumbnails(cx);
            cx.notify();
        }
    }

    // A step that added nothing continues once by itself; one that added cards waits for
    // the grid, rearmed because chunks drawn mid-step already spent its near-end trigger.
    fn step_finished<E>(
        &mut self,
        generation: u64,
        result: Result<Result<AddonStep, BrowseError>, E>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(Ok(step)) => match self.feed.step_done(generation, step.next()) {
                StepAfter::Continue => self.next_step(cx),
                StepAfter::Idle => self.grid.rearm(),
                StepAfter::LoadMore | StepAfter::End | StepAfter::Stale => {}
            },
            Ok(Err(err)) => self.fail(generation, describe_browse(&err)),
            Err(_) => self.fail(
                generation,
                listing_stopped("the add-on listing task stopped unexpectedly".to_owned()),
            ),
        }
        self.want_thumbnails(cx);
        cx.notify();
    }

    // A failure of the current query stops its step where it was, so a retry repeats it.
    fn fail(&mut self, generation: u64, failure: StoreFailure) {
        if generation != self.feed.generation() {
            return;
        }
        self.feed.step_failed(generation);
        failure.log("add-on listing");
        self.error = Some(failure.message);
    }

    fn visible_changed(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        self.visible = range;
        self.want_thumbnails(cx);
    }

    // The drawn cards' thumbnails, asked for again on every chunk since a chunk can fill the
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
                let url = card.thumbnail_url.as_deref()?;
                Some(if self.blurred(card) {
                    ImageKey::card_blurred(url)
                } else {
                    ImageKey::card(url)
                })
            })
            .collect();
        images.update(cx, |images, cx| images.want(&self.viewer, keys, cx));
    }

    fn load_more_button(&self, cx: &mut Context<Self>) -> Option<Button> {
        self.feed.wants_load_more().then(|| {
            Button::new(ids::ADDON_LOAD_MORE)
                .outline()
                .label("Load more")
                .on_click(cx.listener(|tab, _, _, cx| tab.load_more(cx)))
        })
    }

    // Search left, then the sort and the Selects; the weapon Select only once a class has
    // weapons, so it never opens empty. Search mode disables all three.
    fn toolbar(&self, cx: &App) -> impl IntoElement {
        let enabled = self.selects_enabled();
        let muted = cx.theme().muted_foreground;
        let select = |id: &'static str, width: f32, state: AnyElement| {
            div().id(id).test_support().w(px(width)).child(state)
        };
        h_flex()
            .flex_wrap()
            .items_center()
            .gap_4()
            .child(select(
                ids::ADDON_SEARCH,
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
                        ids::ADDON_SORT,
                        160.,
                        Select::new(&self.sort_select)
                            .h_9()
                            .disabled(!enabled)
                            .into_any_element(),
                    )),
            )
            .child(select(
                ids::ADDON_CATEGORY,
                192.,
                Select::new(&self.category_select)
                    .h_9()
                    .disabled(!enabled)
                    .into_any_element(),
            ))
            .when(!self.weapons.is_empty(), |row| {
                row.child(select(
                    ids::ADDON_WEAPON,
                    192.,
                    Select::new(&self.weapon_select)
                        .h_9()
                        .placeholder("All weapons")
                        .disabled(!enabled)
                        .into_any_element(),
                ))
            })
    }

    // Cards fill the grid with "Load more" under it when offered; before the first card, a
    // running step shows skeletons, a dry one the button alone and an ended one "no results".
    fn body(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.feed.items().is_empty() {
            return Some(
                v_flex()
                    .size_full()
                    .gap_4()
                    .child(div().flex_1().min_h_0().child(self.cards_grid(cx)))
                    .children(self.load_more_button(cx))
                    .into_any_element(),
            );
        }
        if self.is_loading() {
            let columns = columns_for_store(
                window.viewport_size().width - SHELL_CHROME_WIDTH,
                MAX_COLUMNS,
            );
            return Some(skeleton_cards(
                ids::ADDON_LOADING,
                "Loading...",
                SKELETON_CARDS,
                columns,
                SkeletonTop::Image,
                cx,
            ));
        }
        if self.feed.is_no_results() {
            return Some(empty_state_card(
                ids::ADDON_EMPTY,
                IconName::Store,
                "No add-ons found",
                None,
                "Try another search or category.",
                cx,
            ));
        }
        self.load_more_button(cx)
            .map(|button| div().child(button).into_any_element())
    }

    // Each card reads its row from the entity while the list lays out, so a frame clones only
    // the drawn cards.
    fn cards_grid(&self, cx: &mut Context<Self>) -> AnyElement {
        let this = cx.entity().downgrade();
        let section = self.section.key();
        let height = |width: Pixels| width * (9. / 16.) + CARD_TEXT_HEIGHT;
        self.grid
            .render_sized(self.feed.items().len(), height, move |ix, _, cx| {
                let card = this.upgrade().and_then(|tab| {
                    let tab = tab.read(cx);
                    let card = tab.feed.items().get(ix)?;
                    Some((card.clone(), tab.blurred(card)))
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
                    open_addon_detail(&opened, window, cx);
                });
                let tab = this.clone();
                let reveal = blurred.then(|| -> OpenAddon {
                    Rc::new(move |_, cx: &mut App| {
                        let _ = tab.update(cx, |tab, cx| tab.reveal(id, cx));
                    })
                });
                gb_addon_card(section, &card, image, open, reveal, cx)
            })
    }
}

impl Render for AddonTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // A failed step's message sits above the grid, or alone before the first card.
        let error = self.error.clone().map(|message| {
            let this = cx.entity().downgrade();
            store_error_line(
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
