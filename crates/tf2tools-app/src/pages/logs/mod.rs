//! v5's Activity Log page: category and level filters over the log, newest first, in a
//! variable-height list.

pub mod model;

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, IndexPath, Sizable as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext as _, Context, Entity, FontWeight,
    InteractiveElement as _, IntoElement, ListAlignment, ListState, ParentElement as _, Render,
    ScrollHandle, StatefulInteractiveElement as _, Styled as _, Subscription, TestSupportExt as _,
    WeakEntity, Window, div, list, px, rgb, transparent_black,
};
use tf2tools_core::{LogEntry, LogLevel, LogQuery};

use super::common::{YELLOW_500, badge_of};
use crate::ids;
use crate::local_messages::describe_log;
use crate::services::run_blocking;
use crate::shell::CONTENT_PADDING;
use crate::shell::confirm::{Confirm, confirm};
use crate::shell::route::Page;
use crate::shell::toast::{ToastKind, toast, toast_error};
use crate::widgets::labelled;
use crate::widgets::scroll::{scroll_area, scroll_beside};
use crate::widgets::store_error::error_line;
use model::{CATEGORIES, LEVELS, PAGE_SIZE, level_name, relative_time};

/// Tailwind green-500 and red-500, v5's info and error level icons.
const GREEN_500: u32 = 0x22c55e;
const RED_500: u32 = 0xef4444;

type Labels = Entity<SelectState<Vec<&'static str>>>;

/// The Activity Log page. Built fresh on each visit, it queries the first page on creation.
pub struct LogsPage {
    entries: Vec<LogEntry>,
    category: Option<&'static str>,
    level: Option<LogLevel>,
    loaded: bool,
    /// Some load has landed, so a reload keeps showing the last result's view.
    landed: bool,
    error: Option<String>,
    /// The last page came back full, so the list ends in a "Load more" item.
    has_more: bool,
    loading_more: bool,
    /// Rows whose details are open, by row id so the set survives Load more, each with its
    /// details box's scroll position.
    expanded: HashMap<i64, ScrollHandle>,
    win: AnyWindowHandle,
    /// Bumped by every load; a load that lands under an older generation is dropped.
    load_generation: u64,
    list: ListState,
    category_select: Labels,
    level_select: Labels,
    _subscriptions: [Subscription; 2],
}

impl LogsPage {
    /// Builds the selects and the list, then loads the first page for All / All.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let categories: Vec<&'static str> = CATEGORIES.iter().map(|(l, _)| *l).collect();
        let levels: Vec<&'static str> = LEVELS.iter().map(|(l, _)| *l).collect();
        let category_select =
            cx.new(|cx| SelectState::new(categories, Some(IndexPath::default()), window, cx));
        let level_select =
            cx.new(|cx| SelectState::new(levels, Some(IndexPath::default()), window, cx));
        // Each select maps its confirmed label back to its filter and re-queries.
        let subscriptions = [
            cx.subscribe(
                &category_select,
                |page, _, event: &SelectEvent<Vec<&'static str>>, cx| {
                    if let SelectEvent::Confirm(Some(label)) = event
                        && let Some((_, category)) = CATEGORIES.iter().find(|(l, _)| l == label)
                    {
                        page.set_category(*category, cx);
                    }
                },
            ),
            cx.subscribe(
                &level_select,
                |page, _, event: &SelectEvent<Vec<&'static str>>, cx| {
                    if let SelectEvent::Confirm(Some(label)) = event
                        && let Some((_, level)) = LEVELS.iter().find(|(l, _)| l == label)
                    {
                        page.set_level(*level, cx);
                    }
                },
            ),
        ];
        let mut page = Self {
            entries: Vec::new(),
            category: None,
            level: None,
            loaded: false,
            landed: false,
            error: None,
            has_more: false,
            loading_more: false,
            expanded: HashMap::new(),
            win: window.window_handle(),
            load_generation: 0,
            list: ListState::new(0, ListAlignment::Top, px(200.)),
            category_select,
            level_select,
            _subscriptions: subscriptions,
        };
        page.reload(cx);
        page
    }

    /// True once a load (first page) has landed, with rows or with an error.
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// The rows shown, newest first.
    pub fn entries(&self) -> &[LogEntry] {
        &self.entries
    }

    pub fn category(&self) -> Option<&'static str> {
        self.category
    }

    pub fn level(&self) -> Option<LogLevel> {
        self.level
    }

    /// Sets a filter and re-queries from the top; a load still running is dropped.
    pub fn set_category(&mut self, category: Option<&'static str>, cx: &mut Context<Self>) {
        self.category = category;
        self.expanded.clear();
        self.reload(cx);
    }

    pub fn set_level(&mut self, level: Option<LogLevel>, cx: &mut Context<Self>) {
        self.level = level;
        self.expanded.clear();
        self.reload(cx);
    }

    pub fn category_select(&self) -> &Labels {
        &self.category_select
    }

    pub fn level_select(&self) -> &Labels {
        &self.level_select
    }

    pub fn list_state(&self) -> &ListState {
        &self.list
    }

    /// Whether more rows may exist past the last one shown (the last page came back full).
    pub fn has_more(&self) -> bool {
        self.has_more
    }

    /// Whether row `id`'s details are expanded.
    pub fn is_expanded(&self, id: i64) -> bool {
        self.expanded.contains_key(&id)
    }

    /// Queries the next page with `before = last shown id` and appends it; a failure toasts
    /// "Failed to load more logs" and keeps what is shown.
    pub fn load_more(&mut self, cx: &mut Context<Self>) {
        let Some(last) = self.entries.last().map(LogEntry::id) else {
            return;
        };
        // While a filter reload runs, the rows shown belong to the old filter.
        if !self.loaded || self.loading_more || !self.has_more {
            return;
        }
        // Shares the filters' generation, so a filter change while this loads drops it.
        self.load_generation += 1;
        let generation = self.load_generation;
        self.loading_more = true;
        cx.notify();
        let query = self.query().with_before(last);
        let loading = run_blocking(cx, move |s| {
            s.activity
                .query(&query)
                .map_err(|e| describe_log(&e).into_op("Failed to load more logs"))
        });
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let loaded = loading.await;
            let _ = page.update(cx, |page, cx| {
                if page.load_generation != generation {
                    return;
                }
                page.loading_more = false;
                match loaded {
                    Ok(rows) => {
                        // The new rows (and a new More item if this page was full) replace
                        // the old More item, keeping the measured rows and the scroll.
                        let old = page.entries.len();
                        page.has_more = rows.len() == PAGE_SIZE as usize;
                        let added = rows.len() + usize::from(page.has_more);
                        page.entries.extend(rows);
                        page.list.splice(old..old + 1, added);
                    }
                    Err(err) => {
                        let _ = win.update(cx, |_, window, cx| toast_error(window, cx, &err));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    // Flips one row's details and re-measures it so the rows below move with it.
    fn toggle_details(&mut self, id: i64, ix: usize, cx: &mut Context<Self>) {
        if self.expanded.remove(&id).is_none() {
            self.expanded.insert(id, ScrollHandle::new());
        }
        self.list.remeasure_items(ix..ix + 1);
        cx.notify();
    }

    // Asks first, then clears the whole log off the UI thread, toasts and re-queries.
    fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let answer = confirm(Confirm::clear_logs(), window, cx);
        let win = self.win;
        cx.spawn(async move |page, cx| {
            if !answer.recv().await.unwrap_or(false) {
                return;
            }
            let Ok(clearing) = page.update(cx, |_, cx| {
                run_blocking(cx, |s| {
                    s.activity
                        .clear()
                        .map_err(|e| describe_log(&e).into_op("Failed to clear logs"))
                })
            }) else {
                return;
            };
            let done = clearing.await;
            let _ = win.update(cx, |_, window, cx| match done {
                Ok(()) => toast(window, cx, ToastKind::Success, "Logs cleared", ""),
                Err(err) => toast_error(window, cx, &err),
            });
            let _ = page.update(cx, |page, cx| {
                page.expanded.clear();
                page.reload(cx);
            });
        })
        .detach();
    }

    fn query(&self) -> LogQuery {
        let mut query = LogQuery::new(PAGE_SIZE);
        if let Some(category) = self.category {
            query = query.with_category(category);
        }
        if let Some(level) = self.level {
            query = query.with_level(level);
        }
        query
    }

    // Queries the first page for the current filters off the UI thread. Only the newest
    // load may land; a failure clears the rows and shows the inline error.
    fn reload(&mut self, cx: &mut Context<Self>) {
        self.load_generation += 1;
        let generation = self.load_generation;
        self.loaded = false;
        self.loading_more = false;
        let query = self.query();
        let loading = run_blocking(cx, move |s| {
            s.activity
                .query(&query)
                .map_err(|e| describe_log(&e).into_op("Failed to load logs"))
        });
        cx.spawn(async move |page, cx| {
            let loaded = loading.await;
            let _ = page.update(cx, |page, cx| {
                if page.load_generation != generation {
                    return;
                }
                match loaded {
                    Ok(rows) => {
                        page.has_more = rows.len() == PAGE_SIZE as usize;
                        page.entries = rows;
                        page.error = None;
                    }
                    Err(err) => {
                        page.has_more = false;
                        page.entries.clear();
                        page.error = Some(err.description().to_owned());
                    }
                }
                page.list
                    .reset(page.entries.len() + usize::from(page.has_more));
                page.loaded = true;
                page.landed = true;
                cx.notify();
            });
        })
        .detach();
    }

    fn toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .gap_3()
            .child(
                div()
                    .id(ids::LOGS_CATEGORY)
                    .test_support()
                    .w(px(160.))
                    .child(Select::new(&self.category_select)),
            )
            .child(
                div()
                    .id(ids::LOGS_LEVEL)
                    .test_support()
                    .w(px(128.))
                    .child(Select::new(&self.level_select)),
            )
            .child(div().flex_1())
            .child(
                Button::new(ids::LOGS_CLEAR)
                    .outline()
                    .icon(IconName::Trash)
                    .label("Clear All")
                    .disabled(self.entries.is_empty())
                    .on_click(cx.listener(|page, _, window, cx| page.clear(window, cx))),
            )
    }

    // The same inline error as the store grid's.
    fn error_line(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let error = self.error.clone()?;
        let page = cx.entity().downgrade();
        let on_retry = move |_: &mut Window, cx: &mut App| {
            let _ = page.update(cx, |page, cx| page.reload(cx));
        };
        Some(error_line(
            ids::LOGS_ERROR,
            ids::LOGS_RETRY,
            error,
            on_retry,
            cx,
        ))
    }

    fn empty_state(cx: &App) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        v_flex()
            .items_center()
            .gap_2()
            .py_12()
            .text_color(muted)
            .child(Icon::new(IconName::Info).size_8())
            .child(labelled(
                div().text_sm().font_weight(FontWeight::MEDIUM),
                ids::LOGS_EMPTY,
                "No log entries found",
            ))
            .child(
                div()
                    .text_xs()
                    .child("Activity will appear here as you use TF2Tools"),
            )
    }

    // Each card reads its row from the page while the list lays out, so a frame builds only
    // the visible cards.
    fn rows(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.entity().downgrade();
        let now = Utc::now();
        let rows = div().id(ids::LOGS_LIST).test_support().size_full().child(
            list(self.list.clone(), move |ix, _, cx| {
                let Some(page) = this.upgrade() else {
                    return div().into_any_element();
                };
                let page = page.read(cx);
                match page.entries.get(ix).cloned() {
                    Some(entry) => {
                        let expanded = page.expanded.get(&entry.id()).cloned();
                        card(&entry, ix, expanded, now, this.clone(), cx)
                    }
                    None if page.has_more && ix == page.entries.len() => {
                        more_row(page.loading_more, this.clone())
                    }
                    None => div().into_any_element(),
                }
            })
            .size_full(),
        );
        scroll_beside(ids::LOGS_LIST_SCROLLBAR, &self.list, rows)
            .flex_1()
            .min_h_0()
    }
}

// The list's last item while more rows may exist: v5's centred outline button.
fn more_row(loading: bool, page: WeakEntity<LogsPage>) -> AnyElement {
    h_flex()
        .justify_center()
        .pb_2()
        .child(
            Button::new(ids::LOGS_MORE)
                .outline()
                .loading(loading)
                .disabled(loading)
                .label(if loading { "Loading..." } else { "Load more" })
                .on_click(move |_, _, cx| {
                    let _ = page.update(cx, |page, cx| page.load_more(cx));
                }),
        )
        .into_any_element()
}

// A row's details: a ghost toggle, and when open the text in a short scrolling mono block.
fn details(
    id: i64,
    ix: usize,
    text: &str,
    expanded: Option<ScrollHandle>,
    page: WeakEntity<LogsPage>,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let text = text.to_owned();
    // v5's `<pre class="mt-2 max-h-48 rounded p-2 text-xs">`: 8 px under the toggle, 16 px
    // lines, a 4 px corner and a 192 px cap past which the block scrolls.
    v_flex()
        .gap_2()
        .items_start()
        .child(
            Button::new(ids::log_details_toggle(id))
                .ghost()
                .xsmall()
                .text_xs()
                .icon(if expanded.is_some() {
                    IconName::ChevronDown
                } else {
                    IconName::ChevronRight
                })
                .label("Details")
                .on_click(move |_, _, cx| {
                    let _ = page.update(cx, |page, cx| page.toggle_details(id, ix, cx));
                }),
        )
        .when_some(expanded, |el, scroll| {
            let block = div()
                .id(ids::log_details(id))
                .aria_label(text.clone())
                .test_support()
                .w_full()
                .max_h(px(192.))
                .track_scroll(&scroll)
                .overflow_y_scroll()
                .p_2()
                .rounded(px(4.))
                .bg(theme.muted)
                .font_family(theme.mono_font_family.clone())
                .text_xs()
                .line_height(px(16.))
                .child(text);
            el.child(scroll_area(ids::log_details_scrollbar(id), &scroll, block).w_full())
        })
}

// One row as v5 draws it: the level icon, then the time, category badge and action over
// the message and any details. The list has no gap, so each card carries its own bottom
// spacing.
fn card(
    entry: &LogEntry,
    ix: usize,
    expanded: Option<ScrollHandle>,
    now: DateTime<Utc>,
    page: WeakEntity<LogsPage>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let id = entry.id();
    let (icon, colour) = match entry.level() {
        LogLevel::Warning => (IconName::TriangleAlert, YELLOW_500),
        LogLevel::Error => (IconName::CircleX, RED_500),
        _ => (IconName::Info, GREEN_500),
    };
    let category = entry.category().to_owned();
    let badge = match category.as_str() {
        "system" => badge_of(
            ids::log_category(id),
            Tag::secondary().outline().text_color(theme.foreground),
            theme.border,
            category,
        ),
        "store" => badge_of(
            ids::log_category(id),
            Tag::secondary(),
            transparent_black(),
            category,
        ),
        _ => badge_of(
            ids::log_category(id),
            Tag::primary(),
            transparent_black(),
            category,
        ),
    };
    div()
        .id(ids::log_row(id))
        .test_support()
        .pb_2()
        .child(
            h_flex()
                .items_start()
                .gap_2()
                .p_3()
                .rounded(theme.radius_lg)
                .border_1()
                .border_color(theme.border)
                .child(
                    div()
                        .id(ids::log_level(id))
                        .aria_label(level_name(entry.level()))
                        .test_support()
                        .mt_0p5()
                        .flex_none()
                        .child(Icon::new(icon).size_4().text_color(rgb(colour))),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_1()
                        .child(
                            h_flex()
                                .gap_2()
                                .child(labelled(
                                    div().text_xs().text_color(theme.muted_foreground),
                                    ids::log_time(id),
                                    relative_time(entry.timestamp(), now),
                                ))
                                .child(badge)
                                .child(labelled(
                                    div().text_sm().font_weight(FontWeight::MEDIUM),
                                    ids::log_action(id),
                                    entry.action().to_owned(),
                                )),
                        )
                        .child(labelled(
                            div().text_sm().text_color(theme.muted_foreground),
                            ids::log_message(id),
                            entry.message().to_owned(),
                        ))
                        .when_some(entry.details(), |el, text| {
                            el.child(details(id, ix, text, expanded, page, cx))
                        }),
                ),
        )
        .into_any_element()
}

impl Render for LogsPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = if let Some(error) = self.error_line(cx) {
            error.into_any_element()
        } else if self.landed && self.entries.is_empty() {
            Self::empty_state(cx).into_any_element()
        } else {
            self.rows(cx).into_any_element()
        };
        v_flex()
            .id(ids::page(Page::Logs))
            .test_support()
            .size_full()
            .p(CONTENT_PADDING)
            .gap_6()
            .child(self.toolbar(cx))
            .child(body)
    }
}
