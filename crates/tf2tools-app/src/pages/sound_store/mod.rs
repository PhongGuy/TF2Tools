//! The Sound Store page: GameBanana sounds with v5's category pills and sync banner.

pub mod body;
pub mod card;
pub mod model;

use std::rc::Rc;

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    App, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _,
    Subscription, TestSupportExt as _, WeakEntity, Window, div, px,
};
use tf2tools_core::{GbId, StoreCard, StoreItemId, StoreKind};

use crate::audio::player::{Player, sound_player};
use crate::ids;
use crate::pages::common::filter_badge;
use crate::shell::CONTENT_PADDING;
use crate::shell::route::Page;
use crate::store_download::SessionDownloads;
use crate::store_grid::{CardRenderer, CardSize, StoreGrid, StoreGridConfig};
use crate::store_preview::{parse_preview_key, preview_key};
use crate::sync::sync_hub;
use crate::widgets::sync_banner::sync_banner;
use card::{CardState, sound_card};
use model::Category;

const KIND: StoreKind = StoreKind::Sounds;
/// v5's sound card height.
const CARD_HEIGHT: f32 = 382.;

/// The Sound Store. Search, sort, count and paging belong to the grid, which this page lays out
/// in v5's rows around the sync banner and category pills; its previews play through the app's
/// one player.
pub struct SoundStorePage {
    grid: Entity<StoreGrid>,
    player: Entity<Player>,
    category: Category,
    downloaded: SessionDownloads,
    /// The key of the preview the cards last drew as playing.
    playing_key: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl SoundStorePage {
    /// Needs the `Services` global; the grid is `StoreGridConfig::new(Sounds, "sounds")` with 382 px cards.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // The player comes first so each card can read what plays and start its preview.
        let player = sound_player(window, cx);
        let weak = player.downgrade();
        let downloaded = SessionDownloads::new(KIND);
        let render: CardRenderer = {
            let downloaded = downloaded.clone();
            Rc::new(move |card, _, window, cx| {
                let state = CardState {
                    playing: card_playing(&weak, card, cx),
                    in_library: downloaded.in_library(card),
                };
                sound_card(card, state, &weak, window, cx)
            })
        };
        let config = StoreGridConfig {
            card: CardSize::Fixed(px(CARD_HEIGHT)),
            ..StoreGridConfig::new(KIND, "sounds")
                .external_toolbar()
                .with_filters()
        };
        let grid = cx.new(|cx| StoreGrid::new(config, render, window, cx));
        // The banner follows the hub's sounds progress; without a hub there is no banner.
        let mut subscriptions: Vec<Subscription> = sync_hub(cx)
            .map(|hub| cx.observe(&hub, |_, _, cx| cx.notify()))
            .into_iter()
            .collect();
        // A play, stop or failure flips a card's icon and the open detail's preview button;
        // the detail does not observe the player, so the whole window redraws. Any other
        // player change (the volume) leaves both alone.
        subscriptions.push(cx.observe_in(
            &player,
            window,
            |page: &mut Self, player, window, cx| {
                let key = player.read(cx).now_playing().map(|p| p.key.clone());
                if key == page.playing_key {
                    return;
                }
                page.playing_key = key;
                page.grid.update(cx, |_, cx| cx.notify());
                window.refresh();
            },
        ));
        // The page draws the grid's search row, count and the banner's rows suffix, so it
        // redraws when the grid changes.
        subscriptions.push(cx.observe(&grid, |_, _, cx| cx.notify()));
        subscriptions.extend(downloaded.track(&grid, cx));
        Self {
            grid,
            player,
            category: Category::All,
            downloaded,
            playing_key: None,
            _subscriptions: subscriptions,
        }
    }

    /// The store card grid this page draws and queries.
    pub fn grid(&self) -> &Entity<StoreGrid> {
        &self.grid
    }

    /// The app's floating player; a preview keeps playing after the page hides.
    pub fn player(&self) -> &Entity<Player> {
        &self.player
    }

    /// Hides or shows the page's grid (see `StoreGrid::set_hidden`).
    pub fn set_hidden(&mut self, hidden: bool, cx: &mut Context<Self>) {
        self.grid.update(cx, |grid, cx| grid.set_hidden(hidden, cx));
    }

    /// The GameBanana id whose preview plays, if any.
    pub fn playing(&self, cx: &App) -> Option<GbId> {
        parse_preview_key(&self.player.read(cx).now_playing()?.key)
    }

    /// `card.in_library`, or downloaded from the store this session.
    pub fn is_in_library(&self, card: &StoreCard) -> bool {
        self.downloaded.in_library(card)
    }

    pub fn category(&self) -> Category {
        self.category
    }

    /// Selects a pill and filters the grid by its store category.
    pub fn set_category(&mut self, category: Category, cx: &mut Context<Self>) {
        self.category = category;
        let filter = category.filter().map(str::to_owned);
        self.grid
            .update(cx, |grid, cx| grid.set_category(filter, cx));
        cx.notify();
    }

    // "Category:" and v5's FilterBadge per category; the selected one is filled.
    fn pills(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let label = div()
            .text_sm()
            .font_weight(FontWeight::MEDIUM)
            .text_color(cx.theme().muted_foreground)
            .child("Category:");
        let mut pill = |category: Category| {
            filter_badge(
                ids::store_category(category.slug()),
                category == self.category,
                category.label(),
                window,
                cx,
            )
            .cursor_pointer()
            .on_click(cx.listener(move |page, _, _, cx| page.set_category(category, cx)))
        };
        let pills = h_flex()
            .gap_1()
            .items_center()
            .children(Category::ALL.map(&mut pill));
        h_flex().gap_2().items_center().child(label).child(pills)
    }

    // v5's filter row: the pills left and the grid's Sort pushed to the right edge.
    fn filter_row(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let sort = self.grid.read(cx).sort_control(cx);
        h_flex()
            .flex_wrap()
            .gap_4()
            .items_center()
            .child(self.pills(window, cx))
            .child(div().ml_auto().child(sort))
    }
}

// Whether `card`'s preview is what the player holds; a dropped player plays nothing.
fn card_playing(player: &WeakEntity<Player>, card: &StoreCard, cx: &App) -> bool {
    let (Some(player), StoreItemId::GameBanana(id)) = (player.upgrade(), &card.id) else {
        return false;
    };
    player
        .read(cx)
        .now_playing()
        .is_some_and(|now| now.key == preview_key(*id))
}

impl Render for SoundStorePage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .id(ids::page(Page::SoundStore))
            .test_support()
            .relative()
            .size_full()
            .p(CONTENT_PADDING)
            .gap_6()
            .children(sync_banner(KIND, self.grid.read(cx).total() > Some(0), cx))
            .child(self.grid.read(cx).search_row(cx))
            .child(self.filter_row(window, cx))
            .child(div().flex_1().min_h_0().child(self.grid.clone()))
    }
}
