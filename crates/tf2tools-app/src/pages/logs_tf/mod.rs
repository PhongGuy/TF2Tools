//! The logs.tf page: tracked players as chips, their 90-day class averages and their logs.

pub mod text;

use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    App, Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Styled as _,
    TestSupportExt as _, Window, div,
};
use tf2tools_core::{SteamId, TrackedPlayer};

use crate::ids;
use crate::services::{OpError, run_blocking};
use crate::shell::CONTENT_PADDING;
use crate::shell::route::Page;
use crate::widgets::labelled;
use text::{EMPTY_TEXT, LOGS_TF_URL};

/// The logs.tf page. It reads the tracked players from the cache on creation and is kept
/// while hidden, so a visit elsewhere leaves it as it was.
pub struct LogsTfPage {
    players: Vec<TrackedPlayer>,
    active: Option<SteamId>,
    loaded: bool,
}

impl LogsTfPage {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut page = Self {
            players: Vec::new(),
            active: None,
            loaded: false,
        };
        page.load(cx);
        page
    }

    /// Kept pages hear when they leave and return; logs.tf keeps everything as it was.
    pub fn set_hidden(&mut self, _hidden: bool, _cx: &mut Context<Self>) {}

    /// True once the first read of the cache has landed.
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// True while a player's logs sync.
    pub fn is_syncing(&self) -> bool {
        false
    }

    /// The tracked players, in the order they were added.
    pub fn players(&self) -> &[TrackedPlayer] {
        &self.players
    }

    /// The player whose chip is selected, if any is tracked.
    pub fn active(&self) -> Option<SteamId> {
        self.active
    }

    // Reads the players and the active chip off the UI thread. A failed read leaves the page
    // empty rather than stuck loading; the warning keeps the cause for the log.
    fn load(&mut self, cx: &mut Context<Self>) {
        let loading = run_blocking(cx, |s| {
            let cache = s.logs.cache();
            let read = |e| OpError::core("Failed to load logs.tf players", e);
            Ok((
                cache.players().map_err(read)?,
                cache.active_player().map_err(read)?,
            ))
        });
        cx.spawn(async move |page, cx| {
            let loaded = loading.await;
            let _ = page.update(cx, |page, cx| {
                match loaded {
                    Ok((players, active)) => {
                        page.players = players;
                        page.active = active;
                    }
                    Err(error) => tracing::warn!(%error, "reading the logs.tf players failed"),
                }
                page.loaded = true;
                cx.notify();
            });
        })
        .detach();
    }

    // The no-player explanation and its way to track someone.
    fn empty_state(&self, cx: &App) -> impl IntoElement {
        v_flex()
            .items_center()
            .gap_4()
            .py_12()
            .child(labelled(
                div().text_sm().text_color(cx.theme().muted_foreground),
                ids::LOGSTF_EMPTY,
                EMPTY_TEXT,
            ))
            .child(
                Button::new(ids::LOGSTF_EMPTY_ADD)
                    .primary()
                    .icon(IconName::Plus)
                    .label("Add player"),
            )
    }
}

impl Render for LogsTfPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let empty = self.loaded && self.players.is_empty();
        v_flex()
            .id(ids::page(Page::LogsTf))
            .test_support()
            .size_full()
            .p(CONTENT_PADDING)
            .gap_4()
            .child(
                h_flex().justify_end().child(
                    Button::new(ids::LOGSTF_CREDIT)
                        .ghost()
                        .icon(IconName::ExternalLink)
                        .label("Stats from logs.tf")
                        .on_click(|_, _, cx| cx.open_url(LOGS_TF_URL)),
                ),
            )
            .children(empty.then(|| self.empty_state(cx)))
    }
}
