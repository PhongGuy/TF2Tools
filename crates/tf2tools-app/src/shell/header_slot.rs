//! The header slot: a page's own view drawn in the header row in place of its plain title.

use gpui_kit::{AnyView, App, AppContext as _, Context, Entity, Global};

use super::route::Page;

/// What a page draws in the header row in place of its plain title, while it is the page shown.
pub struct HeaderSlot {
    owner: Option<(Page, AnyView)>,
}

struct HeaderSlotGlobal(Entity<HeaderSlot>);
impl Global for HeaderSlotGlobal {}

impl HeaderSlot {
    /// The app's slot, made on first use.
    pub fn global(cx: &mut App) -> Entity<HeaderSlot> {
        if let Some(global) = cx.try_global::<HeaderSlotGlobal>() {
            return global.0.clone();
        }
        let slot = cx.new(|_| Self { owner: None });
        cx.set_global(HeaderSlotGlobal(slot.clone()));
        slot
    }

    /// Puts `view` in the header while `page` shows; replaces any earlier view.
    pub fn set(&mut self, page: Page, view: AnyView, cx: &mut Context<Self>) {
        if self
            .owner
            .as_ref()
            .is_some_and(|(p, v)| *p == page && v.entity_id() == view.entity_id())
        {
            return;
        }
        self.owner = Some((page, view));
        cx.notify();
    }

    /// Takes `page`'s view out; another page's view stays.
    pub fn clear(&mut self, page: Page, cx: &mut Context<Self>) {
        if self.owner.as_ref().is_some_and(|(p, _)| *p == page) {
            self.owner = None;
            cx.notify();
        }
    }

    /// The view `page` put in the slot, if it is the slot's owner.
    pub fn view_for(&self, page: Page) -> Option<AnyView> {
        self.owner
            .as_ref()
            .filter(|(p, _)| *p == page)
            .map(|(_, v)| v.clone())
    }
}
