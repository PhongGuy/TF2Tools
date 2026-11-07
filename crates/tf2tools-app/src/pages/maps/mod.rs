//! The Maps page: serveme.tf's community maps in a searchable, sortable list, each one
//! downloadable into `tf/maps`, and removable when TF2Tools put it there.

pub mod model;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Icon, IndexPath, Sizable as _};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FontWeight, Global, InteractiveElement as _,
    IntoElement, ListAlignment, ListState, ParentElement as _, Pixels, Render,
    StatefulInteractiveElement as _, Styled as _, Subscription, TestSupportExt as _, WeakEntity,
    Window, div, list, px,
};
use tf2tools_core::{MapState, StoreClient};

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
use crate::store_messages::{BUSY_TEXT, BUSY_TITLE};
use crate::tokio_bridge;
use crate::widgets::scroll::scroll_beside;
use crate::widgets::store_error::error_line;
use crate::widgets::{Named as _, labelled};
use model::{MapRow, MapSort, SORTS, count_label, last_played_label, visible};

const MAP_READ: &str = "TF2Tools couldn't read its map list. Restart TF2Tools and try again.";
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

type Labels = Entity<SelectState<Vec<&'static str>>>;

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
    _subscriptions: Vec<Subscription>,
}

impl MapsPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search maps..."));
        let labels: Vec<&'static str> = SORTS.iter().map(|(l, _)| *l).collect();
        let sort_select =
            cx.new(|cx| SelectState::new(labels, Some(IndexPath::default()), window, cx));
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
                |page, _, event: &SelectEvent<Vec<&'static str>>, cx| {
                    if let SelectEvent::Confirm(Some(label)) = event
                        && let Some((_, sort)) = SORTS.iter().find(|(l, _)| l == label)
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
    pub fn set_hidden(&mut self, hidden: bool, cx: &mut Context<Self>) {
        if !hidden {
            self.reload(cx);
        }
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
                    OpError::core("Failed to load maps", MAP_READ)
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
                    Err(err) => page.error = Some(err.description().to_owned()),
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
            toast(window, cx, ToastKind::Info, BUSY_TITLE, BUSY_TEXT);
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
                        "Map downloaded",
                        format!("{name} is ready to play in TF2."),
                    ),
                    Ok(Err(e)) => {
                        let failure = describe_map_download(&e);
                        failure.log("map download");
                        toast(
                            window,
                            cx,
                            ToastKind::Error,
                            "Download failed",
                            failure.message,
                        );
                    }
                    Err(e) => toast(
                        window,
                        cx,
                        ToastKind::Error,
                        "Download failed",
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
        let answer = confirm(
            Confirm {
                title: "Remove map?".into(),
                description: format!(
                    "{name} will be deleted from TF2. You can download it again later."
                )
                .into(),
                confirm_label: "Remove".into(),
                cancel_label: "Cancel".into(),
            },
            window,
            cx,
        );
        let name = name.to_owned();
        cx.spawn_in(window, async move |page, cx| {
            if answer.recv().await != Ok(true) {
                return;
            }
            let map = name.clone();
            let removing = page.update(cx, |_, cx| {
                run_blocking(cx, move |s| {
                    s.store.remove_map(&install, &map).map_err(|e| {
                        OpError::core("Remove failed", describe_map_remove(&e).message)
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
                        "Map removed",
                        format!("{name} was deleted from TF2."),
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
                            .child("Sort:"),
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
                    .label(if syncing { "Refreshing..." } else { "Refresh" })
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
        let cell = |width: f32, text: &'static str| {
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
            .child(div().flex_1().min_w_0().child("Map"))
            .when(!compact, |row| {
                row.child(cell(PLAYED_WIDTH, "Times played"))
                    .child(cell(LAST_WIDTH, "Last played"))
            })
            .child(cell(SIZE_WIDTH, "Size"))
            .child(div().w(px(ACTION_WIDTH)).flex_none())
    }

    fn empty_state(&self, cx: &App) -> impl IntoElement {
        let text = if self.rows.is_empty() && (self.syncing || !self.loaded) {
            "Getting the map list..."
        } else if self.rows.is_empty() {
            "No maps yet. Press Refresh to get the list from serveme.tf."
        } else {
            "No maps match your search"
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
    let state_label = |text: &'static str| {
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
                .child(state_label("Downloaded"))
                .child(
                    Button::new(ids::map_remove(&map.name))
                        .ghost()
                        .small()
                        .icon(IconName::Trash)
                        .named("Remove")
                        .on_click(move |_, window, cx| {
                            let _ =
                                page.update(cx, |page, cx| page.remove(&remove_name, window, cx));
                        }),
                )
                .into_any_element()
        }
        MapState::InGame => state_label("In game").into_any_element(),
        MapState::Available | _ => {
            let this_one = running.filter(|r| r.name == map.name);
            let label = match this_one {
                Some(Running {
                    done,
                    total: Some(total),
                    ..
                }) if *total > 0 => format!("{}%", (done * 100 / total).min(100)),
                Some(_) => "Downloading...".to_owned(),
                None => "Download".to_owned(),
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
        v_flex()
            .id(ids::page(Page::Maps))
            .test_support()
            .size_full()
            .p(CONTENT_PADDING)
            .gap_4()
            .child(self.toolbar(cx))
            .when(missing, |page| {
                page.child(no_tf2_warning(
                    ids::MAPS_NO_TF2,
                    "Set the TF2 folder in Settings to download maps.",
                    cx,
                ))
            })
            .children(self.error_line(cx))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        "Community maps hosted by serveme.tf. Downloads go into TF2's maps folder.",
                    ),
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
