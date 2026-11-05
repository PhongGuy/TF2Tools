//! The HUD Store page: the curated hud-db tab and the GameBanana tab.

pub mod bodies;
pub mod curated;
pub mod curated_tab;
pub mod gamebanana_tab;

use gpui_kit::component::button::{Button, ButtonGroup};
use gpui_kit::component::{Selectable as _, v_flex};
use gpui_kit::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, Styled as _, TestSupportExt as _, Window, div, px,
};

use crate::ids;
use crate::shell::CONTENT_PADDING;
use crate::shell::route::Page;
use curated_tab::CuratedTab;
use gamebanana_tab::GameBananaTab;

/// Which of the HUD Store's two sources shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HudStoreTab {
    Curated,
    GameBanana,
}

/// The HUD Store page. Each tab is built on first show and kept while the page lives, so
/// switching back finds its filters as they were; the shell keeps the page across visits.
pub struct HudStorePage {
    tab: HudStoreTab,
    curated: Option<Entity<CuratedTab>>,
    gamebanana: Option<Entity<GameBananaTab>>,
}

impl HudStorePage {
    /// Needs the `Services` global. Opens on the Curated tab, which starts loading at once.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut page = Self {
            tab: HudStoreTab::GameBanana,
            curated: None,
            gamebanana: None,
        };
        page.select(HudStoreTab::Curated, window, cx);
        page
    }

    pub fn tab(&self) -> HudStoreTab {
        self.tab
    }

    /// Shows `tab`, building it the first time.
    pub fn select(&mut self, tab: HudStoreTab, window: &mut Window, cx: &mut Context<Self>) {
        self.tab = tab;
        if tab == HudStoreTab::Curated && self.curated.is_none() {
            self.curated = Some(cx.new(|cx| CuratedTab::new(window, cx)));
        }
        if tab == HudStoreTab::GameBanana && self.gamebanana.is_none() {
            self.gamebanana = Some(cx.new(|cx| GameBananaTab::new(window, cx)));
        }
        // A tab the page hid when it went away wakes when the player picks it.
        self.set_tab_hidden(tab, false, cx);
        cx.notify();
    }

    /// Hides both tabs while the page is away; showing it wakes only the selected tab.
    pub fn set_hidden(&mut self, hidden: bool, cx: &mut Context<Self>) {
        if hidden {
            self.set_tab_hidden(HudStoreTab::Curated, true, cx);
            self.set_tab_hidden(HudStoreTab::GameBanana, true, cx);
        } else {
            self.set_tab_hidden(self.tab, false, cx);
        }
    }

    fn set_tab_hidden(&self, tab: HudStoreTab, hidden: bool, cx: &mut Context<Self>) {
        match tab {
            HudStoreTab::Curated => {
                if let Some(curated) = &self.curated {
                    curated.update(cx, |tab, cx| tab.set_hidden(hidden, cx));
                }
            }
            HudStoreTab::GameBanana => {
                if let Some(gamebanana) = &self.gamebanana {
                    gamebanana.update(cx, |tab, cx| tab.set_hidden(hidden, cx));
                }
            }
        }
    }

    /// The Curated tab, once built.
    pub fn curated(&self) -> Option<Entity<CuratedTab>> {
        self.curated.clone()
    }

    /// The GameBanana tab, once built.
    pub fn gamebanana(&self) -> Option<Entity<GameBananaTab>> {
        self.gamebanana.clone()
    }

    fn tab_button(&self, tab: HudStoreTab, cx: &mut Context<Self>) -> Button {
        let (id, label) = match tab {
            HudStoreTab::Curated => (ids::HUD_STORE_TAB_CURATED, "Curated"),
            HudStoreTab::GameBanana => (ids::HUD_STORE_TAB_GAMEBANANA, "GameBanana"),
        };
        // v5's TabsList is h-9, so the strip and every row under it sit where v5's do.
        Button::new(id)
            .label(label)
            .h(px(36.))
            .selected(self.tab == tab)
            .on_click(cx.listener(move |page, _, window, cx| page.select(tab, window, cx)))
    }
}

impl Render for HudStorePage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Only the selected tab's body renders, so ids the tabs share never collide.
        let body = match self.tab {
            HudStoreTab::Curated => self.curated.clone().map(|tab| tab.into_any_element()),
            HudStoreTab::GameBanana => self.gamebanana.clone().map(|tab| tab.into_any_element()),
        };
        v_flex()
            .id(ids::page(Page::HudStore))
            .test_support()
            .size_full()
            .p(CONTENT_PADDING)
            .gap_6()
            .child(
                ButtonGroup::new(ids::HUD_STORE_TABS)
                    .outline()
                    .child(self.tab_button(HudStoreTab::Curated, cx))
                    .child(self.tab_button(HudStoreTab::GameBanana, cx)),
            )
            .child(div().flex_1().min_h_0().children(body))
    }
}
