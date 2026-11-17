//! The Maps page: serveme.tf's community maps in a searchable, sortable list, each one
//! downloadable into `tf/maps`, and removable when TF2Tools put it there. A source strip
//! switches to GameBanana's maps, browsed from the synced catalogue.

pub mod gb_tab;
pub mod model;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonGroup, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, IndexPath, Selectable as _, Sizable as _,
};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FontWeight, Global, InteractiveElement as _,
    IntoElement, ListAlignment, ListState, ParentElement as _, Pixels, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, TestSupportExt as _, WeakEntity,
    Window, div, list, px,
};
use tf2tools_core::{MapState, StoreClient};

use crate::i18n::{self, tr};
use crate::ids;
use crate::map_messages::{
    describe_map_download, describe_map_remove, describe_map_sync, map_stopped,
};
use crate::pages::common::no_tf2_warning;
use crate::services::{OpError, Services, Tf2State, require_tf2, run_blocking, tf2_state};
use crate::shell::CONTENT_PADDING;
use crate::shell::confirm::{Confirm, confirm};
use crate::shell::route::Page;
use crate::shell::toast::{ToastKind, toast, toast_error};
use crate::store_download::format_bytes;
use crate::store_messages::{busy_text, busy_title};
use crate::tokio_bridge;
use crate::widgets::label_option::LabelOption;
use crate::widgets::scroll::scroll_beside;
use crate::widgets::store_error::error_line;
use crate::widgets::{Named as _, labelled};
use gb_tab::GbMapsTab;
use model::{MapRow, MapSort, SORTS, count_label, last_played_label, sort_label, visible};

const ROW_HEIGHT: f32 = 44.;
const PLAYED_WIDTH: f32 = 96.;
const LAST_WIDTH: f32 = 112.;
const SIZE_WIDTH: f32 = 88.;
const ACTION_WIDTH: f32 = 160.;
// Below this window width the play columns give their room to the map name.
const WIDE_WINDOW: Pixels = px(1100.);

/// The client the page fetches serveme.tf's list with. The app installs it at startup;
/// without it (as in tests that do not ask for one) the page shows the cached list only.
pub struct MapListSource(pub StoreClient);

impl Global for MapListSource {}

impl MapListSource {
    pub fn install(cx: &mut App, client: StoreClient) {
        cx.set_global(Self(client));
    }
}

/// A running download: the map and its bytes so far of the announced size.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Running {
    name: String,
    done: u64,
    total: Option<u64>,
}

type Labels = Entity<SelectState<Vec<LabelOption>>>;

/// Where the Maps page lists maps from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapsSource {
    Serveme,
    GameBanana,
}

impl MapsSource {
    pub const ALL: [MapsSource; 2] = [MapsSource::Serveme, MapsSource::GameBanana];

    /// The key in the source button's id.
    pub fn key(self) -> &'static str {
        match self {
            MapsSource::Serveme => "serveme",
            MapsSource::GameBanana => "gamebanana",
        }
    }

    /// The source button's label.
    pub fn title(self) -> &'static str {
        match self {
            MapsSource::Serveme => "serveme.tf",
            MapsSource::GameBanana => "GameBanana",
        }
    }
}

/// The Maps page. It reads the cached list on creation and asks for a fresh one once; it
/// is kept while hidden, so search, sort and scroll survive a trip to another page.
pub struct MapsPage {
    rows: Vec<MapRow>,
    /// Indexes into `rows` that pass the search, in sort order.
    shown: Vec<usize>,
    sort: MapSort,
    /// The search text `shown` was filtered by; render refilters when the box differs.
    query: String,
    loaded: bool,
    syncing: bool,
    /// The page has asked for a fresh list, so a return does not fetch again.
    refreshed: bool,
    // Whether the window is too narrow for the play columns; set each render.
    compact: bool,
    error: Option<String>,
    downloading: Option<Running>,
    /// Bumped by every load; a load that lands under an older generation is dropped.
    load_generation: u64,
    list: ListState,
    search: Entity<InputState>,
    sort_select: Labels,
    source: MapsSource,
    /// GameBanana's map tab, built the first time that source shows.
    gamebanana: Option<Entity<GbMapsTab>>,
    hidden: bool,
    /// The language the search box and sort Select were titled in.
    language: &'static str,
    _subscriptions: Vec<Subscription>,
}

// The sort Select's rows, titled in the current language.
fn sort_rows() -> Vec<LabelOption> {
    LabelOption::keyed(
        ids::MAPS_SORT,
        SORTS
            .iter()
            .map(|(value, sort)| (*value, sort_label(*sort))),
    )
}

impl MapsPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder(tr!("maps.search")));
        let sort_select =
            cx.new(|cx| SelectState::new(sort_rows(), Some(IndexPath::default()), window, cx));
        let subscriptions = vec![
            // A TF2 folder found or moved changes which maps the game already has.
            cx.observe(&tf2_state(cx), |page, _, cx| page.reload(cx)),
            cx.subscribe(&search, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
            cx.subscribe(
                &sort_select,
                |page, _, event: &SelectEvent<Vec<LabelOption>>, cx| {
                    if let SelectEvent::Confirm(Some(value)) = event
                        && let Some((_, sort)) = SORTS.iter().find(|(v, _)| v == value)
                    {
                        page.set_sort(*sort, cx);
                    }
                },
            ),
        ];
        let mut page = Self {
            rows: Vec::new(),
            shown: Vec::new(),
            sort: MapSort::Name,
            query: String::new(),
            loaded: false,
            syncing: false,
            refreshed: false,
            compact: false,
            error: None,
            downloading: None,
            load_generation: 0,
            list: ListState::new(0, ListAlignment::Top, px(400.)),
            search,
            sort_select,
            source: MapsSource::Serveme,
            gamebanana: None,
            hidden: false,
            language: i18n::current(),
            _subscriptions: subscriptions,
        };
        page.reload(cx);
        page
    }

    /// True once a read of the cached list has landed.
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// True while serveme.tf's list is being fetched.
    pub fn is_syncing(&self) -> bool {
        self.syncing
    }

    /// True while a map downloads.
    pub fn is_downloading(&self) -> bool {
        self.downloading.is_some()
    }

    /// The names shown, in order.
    pub fn shown_names(&self) -> Vec<String> {
        self.shown
            .iter()
            .map(|&i| self.rows[i].name.clone())
            .collect()
    }

    /// The state of the listed map `name`, if it is listed.
    pub fn state_of(&self, name: &str) -> Option<MapState> {
        self.rows.iter().find(|r| r.name == name).map(|r| r.state)
    }

    /// The player message of the last failed fetch, if any.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn search_input(&self) -> &Entity<InputState> {
        &self.search
    }

    pub fn sort(&self) -> MapSort {
        self.sort
    }

    pub fn list_state(&self) -> &ListState {
        &self.list
    }

    pub fn set_sort(&mut self, sort: MapSort, cx: &mut Context<Self>) {
        if sort != self.sort {
            self.sort = sort;
            self.refilter(cx);
        }
    }

    /// Re-reads the states on return, since the player may have changed `tf/maps` meanwhile.
    /// The GameBanana tab hides with the page.
    pub fn set_hidden(&mut self, hidden: bool, cx: &mut Context<Self>) {
        self.hidden = hidden;
        if !hidden {
            self.reload(cx);
        }
        self.sync_tab_hidden(cx);
    }

    // The page is kept while other pages show, so a language switched meanwhile retitles the
    // search box and the sort Select here; the Select keeps its chosen sort.
    fn follow_language(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.language == i18n::current() {
            return;
        }
        self.language = i18n::current();
        self.search.update(cx, |input, cx| {
            input.set_placeholder(tr!("maps.search"), window, cx)
        });
        self.sort_select
            .update(cx, |select, cx| select.set_items(sort_rows(), window, cx));
    }

    /// The source showing; serveme.tf when the page is made.
    pub fn source(&self) -> MapsSource {
        self.source
    }

    /// Shows `source`, building the GameBanana tab the first time, which reads its first page.
    pub fn select_source(
        &mut self,
        source: MapsSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.source = source;
        if source == MapsSource::GameBanana && self.gamebanana.is_none() {
            self.gamebanana = Some(cx.new(|cx| GbMapsTab::new(window, cx)));
        }
        self.sync_tab_hidden(cx);
        cx.notify();
    }

    /// GameBanana's map tab, once built.
    pub fn gamebanana(&self) -> Option<Entity<GbMapsTab>> {
        self.gamebanana.clone()
    }

    // Only a drawn tab holds image wants, so it shows only on a shown page under
    // the GameBanana source.
    fn sync_tab_hidden(&self, cx: &mut Context<Self>) {
        let hidden = self.hidden || self.source != MapsSource::GameBanana;
        if let Some(tab) = &self.gamebanana {
            tab.update(cx, |tab, cx| tab.set_hidden(hidden, cx));
        }
    }

    // As on the Addon Store, a joined group of h-9 buttons.
    fn source_strip(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let buttons = MapsSource::ALL.map(|source| {
            Button::new(ids::maps_source(source.key()))
                .label(source.title())
                .h(px(36.))
                .selected(self.source == source)
                .on_click(
                    cx.listener(move |page, _, window, cx| page.select_source(source, window, cx)),
                )
        });
        h_flex().id(ids::MAPS_SOURCE_TABS).test_support().child(
            ButtonGroup::new(ids::MAPS_SOURCE_GROUP)
                .outline()
                .children(buttons),
        )
    }

    // Recomputes the shown rows from the search and sort, back at the top of the list.
    fn refilter(&mut self, cx: &mut Context<Self>) {
        self.query = self.search.read(cx).value().to_string();
        self.shown = visible(&self.rows, &self.query, self.sort);
        self.list.reset(self.shown.len());
        cx.notify();
    }

    // Reads the cached list with each map's state off the UI thread; the first landing asks
    // for a fresh list, which reloads again when it brings anything new.
    fn reload(&mut self, cx: &mut Context<Self>) {
        self.load_generation += 1;
        let generation = self.load_generation;
        let install = tf2_state(cx).read(cx).install().cloned();
        let loading = run_blocking(cx, move |s| {
            s.store
                .maps(install.as_ref())
                .map(|entries| entries.iter().map(MapRow::from).collect::<Vec<_>>())
                .map_err(|e| {
                    tracing::warn!(error = %e, "reading the map list failed");
                    OpError::core(tr!("messages.action.load_maps"), tr!("messages.maps.read"))
                })
        });
        cx.spawn(async move |page, cx| {
            let loaded = loading.await;
            let _ = page.update(cx, |page, cx| {
                if page.load_generation != generation {
                    return;
                }
                match loaded {
                    Ok(rows) => page.rows = rows,
                    Err(err) => page.error = Some(err.description().to_string()),
                }
                page.loaded = true;
                page.refilter(cx);
                if !page.refreshed {
                    page.refresh(false, cx);
                }
            });
        })
        .detach();
    }

    /// Fetches serveme.tf's list when the cache is stale, or always when `force`; a no-op
    /// while one runs or when the app installed no `MapListSource`.
    pub fn refresh(&mut self, force: bool, cx: &mut Context<Self>) {
        let Some(client) = cx.try_global::<MapListSource>().map(|s| s.0.clone()) else {
            return;
        };
        if self.syncing {
            return;
        }
        self.refreshed = true;
        self.syncing = true;
        cx.notify();
        if !tokio_bridge::is_running(cx) {
            tokio_bridge::init(cx);
        }
        let job = tokio_bridge::spawn(cx, async move { client.sync_maps(force).await });
        cx.spawn(async move |page, cx| {
            let joined = job.await;
            let _ = page.update(cx, |page, cx| {
                page.syncing = false;
                match joined {
                    Ok(Ok(outcome)) => {
                        page.error = None;
                        if !outcome.is_fresh() {
                            page.reload(cx);
                        }
                    }
                    Ok(Err(e)) => {
                        let failure = describe_map_sync(&e);
                        failure.log("map list fetch");
                        page.error = Some(failure.message);
                    }
                    Err(e) => page.error = Some(map_stopped(e.to_string()).message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Downloads `name` into the game's maps folder, one at a time, then toasts and reloads.
    pub fn download(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.downloading.is_some() {
            toast(window, cx, ToastKind::Info, busy_title(), busy_text());
            return;
        }
        let install = match require_tf2(cx) {
            Ok(install) => install,
            Err(err) => return toast_error(window, cx, &err),
        };
        let store = cx.global::<Services>().store.clone();
        self.downloading = Some(Running {
            name: name.to_owned(),
            done: 0,
            total: None,
        });
        cx.notify();
        if !tokio_bridge::is_running(cx) {
            tokio_bridge::init(cx);
        }
        // Only the newest byte count is kept, so a slow frame skips counts rather than queueing them.
        let (tx, mut rx) = tokio::sync::watch::channel((0u64, None::<u64>));
        let map = name.to_owned();
        let job = tokio_bridge::spawn(cx, async move {
            let report = move |done, total| {
                tx.send_replace((done, total));
            };
            store.download_map(&install, &map, &report).await
        });
        let name = name.to_owned();
        cx.spawn_in(window, async move |page, cx| {
            while rx.changed().await.is_ok() {
                let (done, total) = *rx.borrow_and_update();
                let _ = page.update(cx, |page, cx| {
                    if let Some(running) = &mut page.downloading {
                        running.done = done;
                        running.total = total;
                        cx.notify();
                    }
                });
            }
            let joined = job.await;
            let _ = page.update_in(cx, |page, window, cx| {
                page.downloading = None;
                match joined {
                    Ok(Ok(())) => toast(
                        window,
                        cx,
                        ToastKind::Success,
                        tr!("maps.toast.downloaded_title"),
                        tr!("maps.toast.downloaded", name = name),
                    ),
                    Ok(Err(e)) => {
                        let failure = describe_map_download(&e);
                        failure.log("map download");
                        toast(
                            window,
                            cx,
                            ToastKind::Error,
                            tr!("maps.toast.download_failed"),
                            failure.message,
                        );
                    }
                    Err(e) => toast(
                        window,
                        cx,
                        ToastKind::Error,
                        tr!("maps.toast.download_failed"),
                        map_stopped(e.to_string()).message,
                    ),
                }
                page.reload(cx);
            });
        })
        .detach();
    }

    /// Asks, then deletes the downloaded map `name` from the game's maps folder.
    pub fn remove(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let install = match require_tf2(cx) {
            Ok(install) => install,
            Err(err) => return toast_error(window, cx, &err),
        };
        let answer = confirm(Confirm::remove_map(name), window, cx);
        let name = name.to_owned();
        cx.spawn_in(window, async move |page, cx| {
            if answer.recv().await != Ok(true) {
                return;
            }
            let map = name.clone();
            let removing = page.update(cx, |_, cx| {
                run_blocking(cx, move |s| {
                    s.store.remove_map(&install, &map).map_err(|e| {
                        OpError::core(
                            tr!("messages.action.remove_failed"),
                            describe_map_remove(&e).message,
                        )
                    })
                })
            });
            let Ok(removing) = removing else {
                return;
            };
            let removed = removing.await;
            let _ = page.update_in(cx, |page, window, cx| {
                match removed {
                    Ok(()) => toast(
                        window,
                        cx,
                        ToastKind::Success,
                        tr!("maps.toast.removed_title"),
                        tr!("maps.toast.removed", name = name),
                    ),
                    Err(err) => toast_error(window, cx, &err),
                }
                page.reload(cx);
            });
        })
        .detach();
    }

    fn toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let syncing = self.syncing;
        h_flex()
            .flex_wrap()
            .items_center()
            .gap_3()
            .child(
                div()
                    .id(ids::MAPS_SEARCH)
                    .test_support()
                    .flex_1()
                    .min_w(px(160.))
                    .max_w(px(256.))
                    .child(
                        Input::new(&self.search)
                            .h_9()
                            .prefix(Icon::new(IconName::Search).text_color(muted)),
                    ),
            )
            .child(labelled(
                div().text_sm().text_color(muted),
                ids::MAPS_COUNT,
                count_label(self.shown.len(), self.rows.len()),
            ))
            .child(div().flex_1())
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(muted)
                            .child(tr!("maps.sort.title")),
                    )
                    .child(
                        div()
                            .id(ids::MAPS_SORT)
                            .test_support()
                            .w(px(160.))
                            .child(Select::new(&self.sort_select).h_9()),
                    ),
            )
            .child(
                Button::new(ids::MAPS_REFRESH)
                    .outline()
                    .icon(IconName::RefreshCw)
                    .label(if syncing {
                        tr!("maps.button.refreshing")
                    } else {
                        tr!("maps.button.refresh")
                    })
                    .loading(syncing)
                    .loading_icon(IconName::RefreshCw)
                    .disabled(syncing || !cx.has_global::<MapListSource>())
                    .on_click(cx.listener(|page, _, _, cx| page.refresh(true, cx))),
            )
    }

    fn error_line(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let error = self.error.clone()?;
        let page = cx.entity().downgrade();
        let on_retry = move |_: &mut Window, cx: &mut App| {
            let _ = page.update(cx, |page, cx| page.refresh(true, cx));
        };
        Some(error_line(
            ids::MAPS_ERROR,
            ids::MAPS_RETRY,
            error,
            on_retry,
            cx,
        ))
    }

    fn column_titles(compact: bool, cx: &App) -> impl IntoElement {
        let cell = |width: f32, text: SharedString| {
            div().w(px(width)).flex_none().text_right().child(text)
        };
        h_flex()
            .px_3()
            .pb_2()
            .gap_3()
            .border_b_1()
            .border_color(cx.theme().border)
            .text_xs()
            .font_weight(FontWeight::MEDIUM)
            .text_color(cx.theme().muted_foreground)
            .child(div().flex_1().min_w_0().child(tr!("maps.table.map")))
            .when(!compact, |row| {
                row.child(cell(PLAYED_WIDTH, tr!("maps.table.times_played")))
                    .child(cell(LAST_WIDTH, tr!("maps.table.last_played")))
            })
            .child(cell(SIZE_WIDTH, tr!("maps.table.size")))
            .child(div().w(px(ACTION_WIDTH)).flex_none())
    }

    fn empty_state(&self, cx: &App) -> impl IntoElement {
        let text = if self.rows.is_empty() && (self.syncing || !self.loaded) {
            tr!("maps.empty.loading")
        } else if self.rows.is_empty() {
            tr!("maps.empty.none")
        } else {
            tr!("maps.empty.no_match")
        };
        h_flex().justify_center().py_12().child(labelled(
            div().text_sm().text_color(cx.theme().muted_foreground),
            ids::MAPS_EMPTY,
            text,
        ))
    }

    // Each row reads its map from the page while the list lays out, so a frame builds only
    // the rows on screen.
    fn rows(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.entity().downgrade();
        let has_tf2 = !matches!(tf2_state(cx).read(cx), Tf2State::Missing { .. });
        let rows = div().id(ids::MAPS_LIST).test_support().size_full().child(
            list(self.list.clone(), move |ix, _, cx| {
                let Some(page) = this.upgrade() else {
                    return div().into_any_element();
                };
                let page = page.read(cx);
                match page.shown.get(ix).and_then(|&i| page.rows.get(i)) {
                    Some(map) => map_row(map, page, has_tf2, this.clone(), cx),
                    None => div().into_any_element(),
                }
            })
            .size_full(),
        );
        scroll_beside(ids::MAPS_LIST_SCROLLBAR, &self.list, rows)
            .flex_1()
            .min_h_0()
    }
}

// One map: its name, play count, last day played and size, then what the player can do.
fn map_row(
    map: &MapRow,
    shown_in: &MapsPage,
    has_tf2: bool,
    page: WeakEntity<MapsPage>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let name = map.name.clone();
    let running = shown_in.downloading.as_ref();
    let cell = |width: f32, text: String| {
        div()
            .w(px(width))
            .flex_none()
            .text_right()
            .text_color(muted)
            .child(text)
    };
    let state_label = |text: SharedString| {
        labelled(
            div().text_sm().text_color(muted),
            ids::map_state(&map.name),
            text,
        )
    };
    let action: AnyElement = match map.state {
        MapState::Downloaded => {
            let remove_name = name.clone();
            let page = page.clone();
            h_flex()
                .gap_2()
                .items_center()
                .child(
                    Icon::new(IconName::Check)
                        .size_4()
                        .text_color(theme.success),
                )
                .child(state_label(tr!("maps.badge.downloaded")))
                .child(
                    Button::new(ids::map_remove(&map.name))
                        .ghost()
                        .small()
                        .icon(IconName::Trash)
                        .named(tr!("maps.button.remove"))
                        .on_click(move |_, window, cx| {
                            let _ =
                                page.update(cx, |page, cx| page.remove(&remove_name, window, cx));
                        }),
                )
                .into_any_element()
        }
        MapState::InGame => state_label(tr!("maps.badge.in_game")).into_any_element(),
        MapState::Available | _ => {
            let this_one = running.filter(|r| r.name == map.name);
            let label = match this_one {
                Some(Running {
                    done,
                    total: Some(total),
                    ..
                }) if *total > 0 => format!("{}%", (done * 100 / total).min(100)),
                Some(_) => tr!("common.size.downloading").to_string(),
                None => tr!("maps.button.download").to_string(),
            };
            let download_name = name.clone();
            Button::new(ids::map_download(&map.name))
                .outline()
                .small()
                .icon(IconName::Download)
                .label(label)
                .loading(this_one.is_some())
                .disabled(!has_tf2 || running.is_some())
                .on_click(move |_, window, cx| {
                    let _ = page.update(cx, |page, cx| page.download(&download_name, window, cx));
                })
                .into_any_element()
        }
    };
    h_flex()
        .id(ids::map_row(&map.name))
        .aria_label(map.name.clone())
        .test_support()
        // A list item shrinks to its content unless told otherwise, which skews the columns.
        .w_full()
        .h(px(ROW_HEIGHT))
        .px_3()
        .gap_3()
        .items_center()
        .border_b_1()
        .border_color(theme.border)
        .text_sm()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .font_weight(FontWeight::MEDIUM)
                .child(map.name.clone()),
        )
        .when(!shown_in.compact, |row| {
            row.child(cell(PLAYED_WIDTH, map.times_played.to_string()))
                .child(cell(
                    LAST_WIDTH,
                    last_played_label(map.last_played.as_deref()),
                ))
        })
        .child(cell(SIZE_WIDTH, format_bytes(map.size)))
        .child(
            h_flex()
                .w(px(ACTION_WIDTH))
                .flex_none()
                .justify_end()
                .child(action),
        )
        .into_any_element()
}

impl Render for MapsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.follow_language(window, cx);
        let page = v_flex()
            .id(ids::page(Page::Maps))
            .test_support()
            .size_full()
            .p(CONTENT_PADDING)
            .gap_4()
            .child(self.source_strip(cx));
        // Only one source renders, so the tab's ids never meet the list's.
        if self.source == MapsSource::GameBanana {
            return page
                .child(
                    div()
                        .id(ids::MAPS_GB_INFO)
                        .test_support()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr!("maps.gb.info")),
                )
                .child(div().flex_1().min_h_0().children(self.gamebanana.clone()));
        }
        self.compact = window.viewport_size().width < WIDE_WINDOW;
        // The box can change without an event (a value set by code), so it is compared here.
        if self.search.read(cx).value().as_ref() != self.query {
            self.refilter(cx);
        }
        let missing = matches!(tf2_state(cx).read(cx), Tf2State::Missing { .. });
        let body = if self.shown.is_empty() {
            self.empty_state(cx).into_any_element()
        } else {
            self.rows(cx).into_any_element()
        };
        page.child(self.toolbar(cx))
            .when(missing, |page| {
                page.child(no_tf2_warning(ids::MAPS_NO_TF2, tr!("maps.no_tf2"), cx))
            })
            .children(self.error_line(cx))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr!("maps.serveme_info")),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .child(Self::column_titles(self.compact, cx))
                    .child(body),
            )
    }
}
