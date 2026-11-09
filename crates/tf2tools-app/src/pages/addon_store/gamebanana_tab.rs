//! One GameBanana section of the Addon Store: a grid that streams cards from
//! `AddonCatalog::step`, one step at a time, as the player scrolls.

use std::ops::Range;
use std::rc::Rc;

use gpui_kit::component::button::Button;
use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, App, Context, IntoElement, ParentElement as _, Pixels, Render, Styled as _,
    Subscription, Task, Window, div, point, px,
};
use tf2tools_core::{
    AddonCard, AddonCursor, AddonQuery, AddonSection, AddonSort, AddonStep, BrowseError, GbId,
};

use super::card::{CARD_TEXT_HEIGHT, OpenAddon, gb_addon_card};
use crate::addon_feed::{AddonFeed, Keyed, StepAfter};
use crate::ids;
use crate::pages::common::{SkeletonTop, skeleton_cards};
use crate::remote_images::{ImageKey, RemoteImages};
use crate::services::Services;
use crate::store_messages::{StoreFailure, describe_browse, listing_stopped};
use crate::tokio_bridge;
use crate::widgets::card_grid::{CardGrid, SHELL_CHROME_WIDTH, columns_for_store};

/// The most columns an add-on grid lays out.
const MAX_COLUMNS: usize = 4;

/// v5 draws eight skeleton cards while a store loads.
const SKELETON_CARDS: usize = 8;

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
    mature: bool,
    error: Option<String>,
    step_task: Option<Task<()>>,
    /// This tab's own `RemoteImages` viewer, so tabs alive together keep separate wants.
    viewer: String,
    visible: Range<usize>,
    hidden: bool,
    _subscriptions: Vec<Subscription>,
}

impl AddonTab {
    /// Needs the `Services` global. Starts the first step at once.
    pub fn new(section: AddonSection, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let viewer = format!("addon-tab:{}", cx.entity_id());
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
        let mut tab = Self {
            section,
            feed: AddonFeed::new(AddonCursor::START),
            grid,
            sort: AddonSort::MostDownloaded,
            mature: false,
            error: None,
            step_task: None,
            viewer,
            visible: 0..0,
            hidden: false,
            _subscriptions: subscriptions,
        };
        tab.restart(cx);
        tab
    }

    /// The cards streamed so far for the current query.
    pub fn cards(&self) -> &[AddonCard] {
        self.feed.items()
    }

    /// Whether a step runs.
    pub fn is_loading(&self) -> bool {
        self.feed.is_loading()
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

    /// Opens item `id`'s detail.
    pub fn open(&mut self, _id: GbId, _cx: &mut Context<Self>) {}

    // The section in the tab's order, with mature items as the setting says.
    fn query(&self) -> AddonQuery {
        AddonQuery::new(self.section, self.sort).with_mature(self.mature)
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
        self.next_step(cx);
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
            .filter_map(|card| card.thumbnail_url.as_deref().map(ImageKey::card))
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

    // Cards fill the grid with "Load more" under it when offered; before the first card, a
    // running step shows skeletons and a dry one the button alone.
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
        if self.feed.is_loading() {
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
                let card = this
                    .upgrade()
                    .and_then(|tab| tab.read(cx).feed.items().get(ix).cloned());
                let Some(card) = card else {
                    return div().into_any_element();
                };
                let image = card.thumbnail_url.as_deref().and_then(|url| {
                    RemoteImages::global(cx)
                        .map(|images| images.read(cx).image(&ImageKey::card(url)))
                });
                let (tab, id) = (this.clone(), card.id);
                let open: OpenAddon = Rc::new(move |_, cx: &mut App| {
                    let _ = tab.update(cx, |tab, cx| tab.open(id, cx));
                });
                gb_addon_card(section, &card, image, open, cx)
            })
    }
}

impl Render for AddonTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex().size_full().children(self.body(window, cx))
    }
}
