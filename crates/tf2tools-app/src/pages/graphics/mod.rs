//! The Graphics page: mastercomfig's status and the graphics preset picker.

pub mod model;

use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::{
    AnyWindowHandle, Context, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    ScrollHandle, StatefulInteractiveElement as _, Styled as _, Subscription, TestSupportExt as _,
    Window, div,
};
use tf2tools_core::{MastercomfigRelease, MastercomfigStatus};

use crate::ids;
use crate::local_messages::describe_store_status;
use crate::pages::common::no_tf2_warning;
use crate::services::{Tf2State, run_blocking, tf2_state};
use crate::shell::CONTENT_PADDING;
use crate::shell::route::Page;
use crate::shell::toast::toast_error;
use crate::widgets::labelled;
use crate::widgets::scroll::scroll_area;

/// The Graphics page. Built fresh on each visit; it reloads when TF2's state changes.
pub struct GraphicsPage {
    win: AnyWindowHandle,
    /// mastercomfig in `tf/custom`, from the last load that knew TF2.
    status: Option<MastercomfigStatus>,
    /// The release the last addon-store sync recorded.
    release: Option<MastercomfigRelease>,
    loaded: bool,
    /// Bumped by each reload, so a slower, older load never overwrites a newer one.
    load_generation: u64,
    scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl GraphicsPage {
    /// Needs the `Services` and `Tf2Global` globals. Loads the status and release off the UI thread.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let tf2 = cx.observe(&tf2_state(cx), |page, _, cx| page.reload(cx));
        let mut page = Self {
            win: window.window_handle(),
            status: None,
            release: None,
            loaded: false,
            load_generation: 0,
            scroll: ScrollHandle::new(),
            _subscriptions: vec![tf2],
        };
        page.reload(cx);
        page
    }

    /// True once a load has ended, failed or not, with or without TF2.
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// mastercomfig's status from the last load; None without TF2 or before a load.
    pub fn status(&self) -> Option<&MastercomfigStatus> {
        self.status.as_ref()
    }

    /// The recorded mastercomfig release from the last load, if a sync recorded one.
    pub fn release(&self) -> Option<&MastercomfigRelease> {
        self.release.as_ref()
    }

    // Reads the install's status (only with TF2) and the recorded release in the background;
    // a failed read toasts through the window and leaves the last good values.
    fn reload(&mut self, cx: &mut Context<Self>) {
        self.load_generation += 1;
        let generation = self.load_generation;
        let tf2 = tf2_state(cx).read(cx).install().cloned();
        let loading = run_blocking(cx, move |s| {
            let status = tf2
                .map(|tf2| s.library.mastercomfig_status(&tf2))
                .transpose()
                .map_err(|e| model::describe_status(&e).into_op("Failed to load mastercomfig"))?;
            let release = s
                .store
                .mastercomfig_release()
                .map_err(|e| describe_store_status(&e).into_op("Failed to load mastercomfig"))?;
            Ok((status, release))
        });
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let loaded = loading.await;
            let current = page
                .update(cx, |page, cx| {
                    if page.load_generation != generation {
                        return false;
                    }
                    if let Ok((status, release)) = &loaded {
                        page.status = status.clone();
                        page.release = release.clone();
                    }
                    page.loaded = true;
                    cx.notify();
                    true
                })
                .unwrap_or(false);
            if let (true, Err(err)) = (current, loaded) {
                let _ =
                    cx.update(|cx| win.update(cx, |_, window, cx| toast_error(window, cx, &err)));
            }
        })
        .detach();
    }
}

impl Render for GraphicsPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let missing = matches!(tf2_state(cx).read(cx), Tf2State::Missing { .. });
        let warning = missing.then(|| {
            no_tf2_warning(
                ids::GRAPHICS_NO_TF2,
                "Set the TF2 folder in Settings to use mastercomfig.",
                cx,
            )
        });
        let content = v_flex()
            .id(ids::page(Page::Graphics))
            .test_support()
            .size_full()
            .track_scroll(&self.scroll)
            .overflow_y_scroll()
            .p(CONTENT_PADDING)
            .gap_6()
            .children(warning)
            .child(labelled(
                div().text_sm().text_color(cx.theme().muted_foreground),
                ids::GRAPHICS_SUBTITLE,
                "Graphics and game settings from mastercomfig",
            ));
        scroll_area(ids::page_scrollbar(Page::Graphics), &self.scroll, content).size_full()
    }
}
