//! The Demos.tf page: a list of demos that opens the chosen one in the 3D viewer.

mod crumb;
mod list;
pub mod text;

use std::collections::HashSet;

use gpui_kit::component::v_flex;
use gpui_kit::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ListAlignment,
    ListState, ParentElement as _, Render, SharedString, Styled as _, Subscription,
    TestSupportExt as _, Window, div, px,
};
use tf2tools_core::{DemoCursor, DemoListQuery, DemoPage, DemoSummary, DemosError, DemosRange};

use crate::i18n::tr;
use crate::ids;
use crate::pages::demo_viewer::DemoViewerPage;
use crate::picker::PickKind;
use crate::services::Services;
use crate::shell::CONTENT_PADDING;
use crate::shell::header_slot::HeaderSlot;
use crate::shell::route::Page;
use crate::store_grid::feed::{Feed, FeedItem};
use crate::tokio_bridge;

use crumb::DemosTfCrumb;
use text::list_error;

/// One demo of the list with the cursor of the page it came on, so the feed's next page
/// starts where demos.tf said, not after the last demo the format filter kept.
pub(crate) struct DemoRow {
    pub(crate) demo: DemoSummary,
    pub(crate) next: Option<DemoCursor>,
}

impl FeedItem for DemoRow {
    type Id = u64;
    type Cursor = Option<DemoCursor>;

    fn feed_id(&self) -> u64 {
        self.demo.id()
    }

    fn feed_cursor(&self) -> Option<DemoCursor> {
        self.next
    }
}

/// What the page shows: the list, or one demo in the viewer.
enum Mode {
    List,
    Viewer(Entity<DemoViewerPage>),
}

/// The Demos.tf page: starts on the list and swaps to the viewer once a demo is opened.
pub struct DemosTfPage {
    mode: Mode,
    hidden: bool,
    feed: Feed<DemoRow>,
    // Item 0 is the toolbar, then one item per row, then the footer.
    list: ListState,
    query: DemoListQuery,
    // The cursor a failed page asked with, so Retry asks for that page again.
    resume: Option<DemoCursor>,
    error: Option<SharedString>,
    _release: Subscription,
}

impl DemosTfPage {
    /// A page on its list.
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        // The crumb belongs to this page's viewer, so it goes when the page does.
        let release = cx.on_release(|_, cx| {
            HeaderSlot::global(cx).update(cx, |slot, cx| slot.clear(Page::DemosTf, cx));
        });
        let mut page = Self {
            mode: Mode::List,
            hidden: false,
            feed: Feed::new(),
            list: ListState::new(2, ListAlignment::Top, px(200.)),
            query: DemoListQuery::new(DemosRange::default(), 0),
            resume: None,
            error: None,
            _release: release,
        };
        page.restart(cx);
        page
    }

    /// The listed demos' ids, in list order.
    pub fn demo_ids(&self) -> Vec<u64> {
        self.feed.items().iter().map(|row| row.demo.id()).collect()
    }

    /// True once demos.tf has no more demos for the query.
    pub fn is_end(&self) -> bool {
        self.feed.is_end()
    }

    /// True while a page of demos is in flight.
    pub fn is_loading(&self) -> bool {
        self.feed.is_loading()
    }

    /// The list's scroll state.
    pub fn list_state(&self) -> &ListState {
        &self.list
    }

    /// Clears a failed page's error and asks for that page again.
    pub fn retry(&mut self, cx: &mut Context<Self>) {
        self.error = None;
        self.next_page(cx);
        cx.notify();
    }

    // Starts the list over at the top with the range ending now, so a page still in flight
    // lands under an old generation and is dropped.
    fn restart(&mut self, cx: &mut Context<Self>) {
        let q = &self.query;
        self.query = DemoListQuery::new(q.range(), chrono::Utc::now().timestamp())
            .with_format(q.format())
            .with_map(q.map().unwrap_or_default())
            .with_players(q.players());
        self.feed.reset();
        self.resume = None;
        self.error = None;
        self.list.reset(2);
        self.next_page(cx);
    }

    // True while the list may page on: no failed page waits for Retry, the end is not
    // reached and no page is in flight.
    fn wants_page(&self) -> bool {
        self.error.is_none()
            && self.resume.is_none()
            && !self.feed.is_end()
            && !self.feed.is_loading()
    }

    // Asks demos.tf for the next page on tokio. A failed page's cursor wins over the feed's,
    // and a last page with no next cursor lands empty at once, which ends the list.
    fn next_page(&mut self, cx: &mut Context<Self>) {
        if self.error.is_some() {
            return;
        }
        let Some((after, generation)) = self.feed.next_page() else {
            return;
        };
        let cursor = match (self.resume.take(), after) {
            (Some(cursor), _) => cursor,
            (None, None) => DemoCursor::First,
            (None, Some(Some(cursor))) => cursor,
            (None, Some(None)) => {
                self.land(generation, Vec::new(), cx);
                return;
            }
        };
        if !tokio_bridge::is_running(cx) {
            tokio_bridge::init(cx);
        }
        let client = cx.global::<Services>().demos.clone();
        let query = self.query.clone();
        let fetching = tokio_bridge::spawn(cx, async move { client.list(&query, cursor).await });
        cx.spawn(async move |page, cx| {
            let fetched = fetching.await;
            let _ = page.update(cx, |page, cx| match fetched {
                Ok(Ok(fetched)) => page.landed(generation, &fetched, cx),
                Ok(Err(error)) => {
                    tracing::debug!(%error, "a page of demos.tf demos failed");
                    page.failed(generation, cursor, Some(&error), cx);
                }
                Err(error) => {
                    tracing::debug!(%error, "the demos.tf list task stopped");
                    page.failed(generation, cursor, None, cx);
                }
            });
        })
        .detach();
    }

    // Keeps the page's demos the list lacks; a newer demo shifts the pages, so a later page
    // can repeat ones already shown.
    fn landed(&mut self, generation: u64, fetched: &DemoPage, cx: &mut Context<Self>) {
        let seen: HashSet<u64> = self.demo_ids().into_iter().collect();
        let rows = fetched
            .demos()
            .iter()
            .filter(|demo| !seen.contains(&demo.id()))
            .map(|demo| DemoRow {
                demo: demo.clone(),
                next: fetched.next(),
            })
            .collect();
        self.land(generation, rows, cx);
    }

    // Appends rows of this generation before the footer, keeping the scroll where it was.
    fn land(&mut self, generation: u64, rows: Vec<DemoRow>, cx: &mut Context<Self>) {
        let old = self.feed.items().len();
        if self.feed.page_loaded(generation, rows) {
            let added = self.feed.items().len() - old;
            self.list.splice(old + 1..old + 1, added);
            cx.notify();
        }
    }

    // Stops paging on a failure of this generation until Retry asks for the same page.
    fn failed(
        &mut self,
        generation: u64,
        cursor: DemoCursor,
        error: Option<&DemosError>,
        cx: &mut Context<Self>,
    ) {
        if generation != self.feed.generation() {
            return;
        }
        self.feed.page_failed(generation);
        self.resume = Some(cursor);
        self.error = Some(error.map_or_else(|| tr!("logs_tf.demo.down"), list_error));
        cx.notify();
    }

    /// The viewer, while a demo shows.
    pub fn viewer(&self) -> Option<&Entity<DemoViewerPage>> {
        match &self.mode {
            Mode::Viewer(viewer) => Some(viewer),
            Mode::List => None,
        }
    }

    /// Hiding drops the viewer, freeing its renderer and pointer lock, so a return starts on
    /// the list.
    pub fn set_hidden(&mut self, hidden: bool, cx: &mut Context<Self>) {
        self.hidden = hidden;
        if hidden && self.viewer().is_some() {
            self.back(cx);
        }
    }

    /// Closes the viewer back to the list, taking its header crumb along.
    pub fn back(&mut self, cx: &mut Context<Self>) {
        HeaderSlot::global(cx).update(cx, |slot, cx| slot.clear(Page::DemosTf, cx));
        self.mode = Mode::List;
        cx.notify();
    }

    // Asks for a demo from the computer; a cancelled pick stays on the list, and a pick that
    // lands after the page was hidden is dropped, so no viewer or crumb outlives the visit.
    fn open_demo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picking = cx.global::<Services>().picker.pick(PickKind::OpenDemo, cx);
        cx.spawn_in(window, async move |page, cx| {
            let Some(path) = picking.await else { return };
            let _ = page.update_in(cx, |page, window, cx| {
                if page.hidden {
                    return;
                }
                let viewer = cx.new(|cx| DemoViewerPage::open_path(path, window, cx));
                let this = cx.entity().downgrade();
                let crumb = cx.new(|cx| DemosTfCrumb::new(this, &viewer, cx));
                HeaderSlot::global(cx)
                    .update(cx, |slot, cx| slot.set(Page::DemosTf, crumb.into(), cx));
                page.mode = Mode::Viewer(viewer);
                cx.notify();
            });
        })
        .detach();
    }

    fn list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .id(ids::DEMOS_TF_LIST)
            .test_support()
            .size_full()
            .p(CONTENT_PADDING)
            .child(list::page_list(self, cx).flex_1().min_h_0())
    }
}

impl Render for DemosTfPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match &self.mode {
            Mode::List => self.list(cx).into_any_element(),
            Mode::Viewer(viewer) => viewer.clone().into_any_element(),
        };
        div()
            .id(ids::page(Page::DemosTf))
            .test_support()
            .size_full()
            .child(body)
    }
}
