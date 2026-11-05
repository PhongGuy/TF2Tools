//! The Crosshair Store: the shared paged store grid of GameBanana crosshair cards.

use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::v_flex;
use gpui_kit::{
    App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, Subscription, TestSupportExt as _, Window, div, px,
};
use tf2tools_core::{StoreCard, StoreItemId, StoreKind};

use crate::gb_card::gamebanana_card;
use crate::ids;
use crate::pages::hud_store::bodies::gamebanana_body;
use crate::shell::CONTENT_PADDING;
use crate::shell::route::Page;
use crate::store_detail::{DetailItem, open_detail};
use crate::store_download::SessionDownloads;
use crate::store_grid::{CardRenderer, StoreGrid, StoreGridConfig};
use crate::sync::sync_hub;
use crate::widgets::sync_banner::sync_banner;

const KIND: StoreKind = StoreKind::Crosshairs;

/// The Crosshair Store page. Search, sort, count and paging belong to the grid.
pub struct CrosshairStorePage {
    grid: Entity<StoreGrid>,
    downloaded: SessionDownloads,
    _subscriptions: Vec<Subscription>,
}

impl CrosshairStorePage {
    /// Needs the `Services` global. The grid starts loading at once.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let downloaded = SessionDownloads::new(KIND);
        let render: CardRenderer = {
            let downloaded = downloaded.clone();
            let page = cx.entity().downgrade();
            Rc::new(move |card, image, _, cx| {
                let in_library = downloaded.in_library(card);
                let (page, id) = (page.clone(), card.id.clone());
                let open = Rc::new(move |window: &mut Window, cx: &mut App| {
                    if let Some(page) = page.upgrade() {
                        page.update(cx, |page, cx| page.open(&id, window, cx));
                    }
                });
                gamebanana_card(KIND, card, image, in_library, IconName::Crosshair, open, cx)
            })
        };
        let config = StoreGridConfig::new(KIND, "crosshairs")
            .image_card(px(268.5))
            .without_search();
        let grid = cx.new(|cx| StoreGrid::new(config, render, window, cx));
        // The banner follows the hub's progress for this store and the grid's rows for its
        // suffix, so the page redraws when either changes.
        let mut subscriptions: Vec<Subscription> = sync_hub(cx)
            .map(|hub| cx.observe(&hub, |_, _, cx| cx.notify()))
            .into_iter()
            .collect();
        subscriptions.push(cx.observe(&grid, |_, _, cx| cx.notify()));
        subscriptions.extend(downloaded.track(&grid, cx));
        Self {
            grid,
            downloaded,
            _subscriptions: subscriptions,
        }
    }

    /// The store card grid this page draws and queries.
    pub fn grid(&self) -> &Entity<StoreGrid> {
        &self.grid
    }

    /// Hides or shows the page's grid; see `StoreGrid::set_hidden`.
    pub fn set_hidden(&mut self, hidden: bool, cx: &mut Context<Self>) {
        self.grid.update(cx, |grid, cx| grid.set_hidden(hidden, cx));
    }

    /// Opens the shared `StoreDetail` for the grid's card `id`, with `in_library` read now; a card
    /// no longer in the grid opens nothing.
    pub fn open(&self, id: &StoreItemId, window: &mut Window, cx: &mut App) {
        let Some(item) = self
            .grid
            .read(cx)
            .cards()
            .iter()
            .find(|card| &card.id == id)
            .map(|card| DetailItem {
                kind: KIND,
                id: card.id.clone(),
                name: card.name.clone(),
                in_library: self.is_in_library(card),
            })
        else {
            return;
        };
        open_detail(item, gamebanana_body(), window, cx);
    }

    /// `card.in_library`, or downloaded from the store this session.
    pub fn is_in_library(&self, card: &StoreCard) -> bool {
        self.downloaded.in_library(card)
    }
}

impl Render for CrosshairStorePage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let has_rows = self.grid.read(cx).total() > Some(0);
        v_flex()
            .id(ids::page(Page::CrosshairStore))
            .test_support()
            .size_full()
            .p(CONTENT_PADDING)
            .gap_6()
            .children(sync_banner(KIND, has_rows, cx))
            .child(div().flex_1().min_h_0().child(self.grid.clone()))
    }
}
