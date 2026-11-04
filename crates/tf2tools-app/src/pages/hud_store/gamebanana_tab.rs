//! The HUD Store's GameBanana tab: the shared paged store grid with HUD cards that open the
//! detail dialog.

use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::v_flex;
use gpui_kit::{
    App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    Subscription, Window, div, px,
};
use tf2tools_core::{StoreCard, StoreKind};

use super::bodies::gamebanana_body;
use crate::gb_card::gamebanana_card;
use crate::store_detail::{DetailItem, open_detail};
use crate::store_download::SessionDownloads;
use crate::store_grid::{CardRenderer, StoreGrid, StoreGridConfig};
use crate::sync::sync_hub;
use crate::widgets::sync_banner::sync_banner;

const KIND: StoreKind = StoreKind::GameBananaHuds;

/// The GameBanana tab. Search, sort, count and paging belong to the grid; this adds the cards.
pub struct GameBananaTab {
    grid: Entity<StoreGrid>,
    downloaded: SessionDownloads,
    _subscriptions: Vec<Subscription>,
}

impl GameBananaTab {
    /// Needs the `Services` global. The grid starts loading at once.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let downloaded = SessionDownloads::new(KIND);
        let render: CardRenderer = {
            let downloaded = downloaded.clone();
            Rc::new(move |card, image, _, cx| {
                let in_library = downloaded.in_library(card);
                let item = DetailItem {
                    kind: KIND,
                    id: card.id.clone(),
                    name: card.name.clone(),
                    in_library,
                };
                let open = Rc::new(move |window: &mut Window, cx: &mut App| {
                    open_detail(item.clone(), gamebanana_body(), window, cx);
                });
                gamebanana_card(
                    KIND,
                    card,
                    image,
                    in_library,
                    IconName::Paintbrush,
                    open,
                    cx,
                )
            })
        };
        let config = StoreGridConfig::new(KIND, "HUDs").image_card(px(317.));
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

    /// The store card grid this tab draws and queries.
    pub fn grid(&self) -> &Entity<StoreGrid> {
        &self.grid
    }

    /// Whether the library holds `card`, as the store read it or as downloaded this session.
    pub fn is_in_library(&self, card: &StoreCard) -> bool {
        self.downloaded.in_library(card)
    }
}

impl Render for GameBananaTab {
    // v5 puts the banner inside the GameBanana tab, above the grid's header row.
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let has_rows = self.grid.read(cx).total() > Some(0);
        v_flex()
            .size_full()
            .gap_6()
            .children(sync_banner(KIND, has_rows, cx))
            .child(div().flex_1().min_h_0().child(self.grid.clone()))
    }
}
