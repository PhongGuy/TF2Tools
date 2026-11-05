//! The HUD Store's Curated tab: every hud-db HUD read once, filtered in memory with v5's
//! toolbar, on a virtualized card grid.

use std::collections::HashSet;
use std::ops::Range;
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{
    Button, ButtonVariants as _, Toggle, ToggleGroup, ToggleVariants as _,
};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _,
    IntoElement, ObjectFit, ParentElement as _, Pixels, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled as _, StyledImage as _, Subscription, Task,
    TestSupportExt as _, Window, div, img, px,
};
use tf2tools_core::{Provenance, StoreItemId, StoreKind};

use super::bodies::curated_body;
use super::curated::{CuratedFilter, CuratedHud, Facet, Os, count_label, facets};
use crate::gb_card::OpenCard;
use crate::ids;
use crate::pages::common::{
    SkeletonTop, badge_rows, card_frame, empty_state_card, skeleton_cards, tag_badge,
};
use crate::remote_images::{ImageKey, RemoteImage, RemoteImages};
use crate::services::{OpError, run_blocking};
use crate::store_detail::{DetailItem, open_detail};
use crate::store_download::{DownloadEvent, StoreDownloads, download_button};
use crate::store_messages::read_failed;
use crate::sync::{SyncEvent, sync_hub};
use crate::widgets::Named as _;
use crate::widgets::card_grid::{CardGrid, SHELL_CHROME_WIDTH, columns_for_store};
use crate::widgets::labelled;
use crate::widgets::store_error::store_error_line;

const KIND: StoreKind = StoreKind::CuratedHuds;
/// v5's placeholder count while the store loads or a sync fills it.
const SKELETON_CARDS: usize = 8;
/// The 24 px top padding plus v5's gap-6 body of name block, flags and button under a card's
/// 16:9 screenshot: 420 px at v5's 272 px column, as its first screen shows.
const CARD_TEXT_HEIGHT: Pixels = px(267.);
/// v5 shows at most this many flags on a card.
const CARD_FLAGS: usize = 4;
/// The flags wrap onto at most this many rows; a flag past them is hidden whole.
const FLAG_ROWS: usize = 2;
/// The "More filters" chip rows, in v5's order.
const FACET_ROWS: [(Facet, &str); 4] = [
    (Facet::Flag, "Features"),
    (Facet::Tag, "Tags"),
    (Facet::Gamemode, "Gamemode"),
    (Facet::Resolution, "Resolution"),
];

/// The Curated tab. Filters apply in memory, so every change redraws at once.
pub struct CuratedTab {
    huds: Rc<[CuratedHud]>,
    /// Every facet's values across the HUDs, in `FACET_ROWS` order.
    facet_values: Vec<Vec<String>>,
    filter: CuratedFilter,
    /// Indices into `huds` that pass the filter.
    shown: Rc<[usize]>,
    /// The `hud_id`s of curated HUDs in the library.
    downloaded: Rc<HashSet<String>>,
    loaded: bool,
    error: Option<String>,
    /// A curated sync finished and its reload has not landed; the tab still reads as syncing.
    sync_reload_pending: bool,
    more: bool,
    search: Entity<InputState>,
    grid: CardGrid,
    /// The grid's last drawn range, as indices into `shown`.
    visible: Range<usize>,
    viewer: String,
    load: Option<Task<()>>,
    /// Set while the HUD Store is away; the tab then holds its reloads and image wants.
    hidden: bool,
    _subscriptions: Vec<Subscription>,
}

impl CuratedTab {
    /// Needs the `Services` global. Presets the OS filter to the running OS, then loads.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search HUDs..."));
        let mut subscriptions = vec![cx.subscribe(&search, |tab, input, event, cx| {
            if matches!(event, InputEvent::Change) {
                let text = input.read(cx).value().to_string();
                tab.set_search(text, cx);
            }
        })];
        // Cards redraw on download progress, and a finished curated download rereads which
        // HUDs the library holds, so its card flips to "Re-download".
        if let Some(downloads) = StoreDownloads::global(cx) {
            subscriptions.push(cx.observe(&downloads, |_, _, cx| cx.notify()));
            subscriptions.push(cx.subscribe(&downloads, |tab, _, event, cx| {
                if matches!(event, DownloadEvent::Finished { target } if target.kind == KIND) {
                    tab.reload(cx);
                }
            }));
        }
        // An empty store says it is syncing while the hub runs, and a finished curated sync
        // rereads the HUDs, so a first launch fills the tab without leaving the page.
        if let Some(hub) = sync_hub(cx) {
            subscriptions.push(cx.observe(&hub, |_, _, cx| cx.notify()));
            subscriptions.push(cx.subscribe(&hub, |tab, _, event: &SyncEvent, cx| {
                if matches!(event, SyncEvent::Finished { kind } if *kind == KIND) {
                    tab.sync_reload_pending = true;
                    tab.reload(cx);
                }
            }));
        }
        // Thumbnails redraw the tab as they land, and the tab's wants go with it.
        let viewer = format!("hud-store-curated:{}", cx.entity_id());
        if let Some(images) = RemoteImages::global(cx) {
            subscriptions.push(cx.observe(&images, |_, _, cx| cx.notify()));
            let released = viewer.clone();
            subscriptions.push(cx.on_release(move |_, cx| {
                images.update(cx, |images, cx| images.want(&released, Vec::new(), cx));
            }));
        }
        let mut grid = CardGrid::store(ids::STORE_GRID, 4);
        let this = cx.entity().downgrade();
        grid.on_visible(move |range, cx| {
            let _ = this.update(cx, |tab, cx| {
                tab.visible = range;
                tab.want_images(cx);
            });
        });
        let mut tab = Self {
            huds: Rc::from(Vec::new()),
            facet_values: Vec::new(),
            filter: CuratedFilter::new(Os::current()),
            shown: Rc::from(Vec::new()),
            downloaded: Rc::default(),
            loaded: false,
            error: None,
            sync_reload_pending: false,
            more: false,
            search,
            grid,
            visible: 0..0,
            viewer,
            load: None,
            hidden: false,
            _subscriptions: subscriptions,
        };
        tab.reload(cx);
        tab
    }

    /// True once a load has ended, read or failed.
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// True while a curated sync runs and until the reload after it lands, as v5 holds `syncing`.
    pub fn is_syncing(&self, cx: &App) -> bool {
        self.sync_reload_pending || sync_hub(cx).is_some_and(|hub| hub.read(cx).is_running(KIND))
    }

    /// The card grid's scroll handle; its offset is what the grid drew.
    pub fn grid_scroll(&self) -> ScrollHandle {
        self.grid.scroll_handle().0.borrow().base_handle.clone()
    }

    /// The `hud_id`s that pass the filter, in list order.
    pub fn shown_ids(&self) -> Vec<String> {
        self.shown
            .iter()
            .map(|&ix| self.huds[ix].hud_id.clone())
            .collect()
    }

    pub fn filter(&self) -> &CuratedFilter {
        &self.filter
    }

    /// Filters on `text` at once; the list is in memory, so there is no debounce.
    pub fn set_search(&mut self, text: String, cx: &mut Context<Self>) {
        self.filter.search = text;
        self.refilter(cx);
    }

    /// Hides or shows the tab with its page. Hidden, it drops its image wants and skips
    /// reloads; shown again, it rereads the store and the library, keeping filters and scroll.
    pub fn set_hidden(&mut self, hidden: bool, cx: &mut Context<Self>) {
        if hidden == std::mem::replace(&mut self.hidden, hidden) {
            return;
        }
        if hidden {
            if let Some(images) = RemoteImages::global(cx) {
                images.update(cx, |images, cx| images.want(&self.viewer, Vec::new(), cx));
            }
            return;
        }
        self.reload(cx);
    }

    /// Whether the library holds the curated HUD `hud_id`.
    pub fn is_downloaded(&self, hud_id: &str) -> bool {
        self.downloaded.contains(hud_id)
    }

    // Reads the curated HUDs and the library's curated ids off the UI thread; a newer
    // reload drops an older one in flight. A hidden tab rereads on return instead.
    fn reload(&mut self, cx: &mut Context<Self>) {
        if self.hidden {
            return;
        }
        let read = run_blocking(cx, |s| {
            let items = s
                .store
                .store_items(KIND)
                .map_err(|e| OpError::core("Failed to load the store", e))?;
            let library = s
                .library
                .list_huds()
                .map_err(|e| OpError::core("Failed to load the store", e))?;
            let huds: Vec<CuratedHud> = items.iter().filter_map(CuratedHud::from_item).collect();
            let downloaded: HashSet<String> = library
                .iter()
                .filter_map(|hud| match hud.provenance() {
                    Provenance::Curated { hud_id } => Some(hud_id.clone()),
                    _ => None,
                })
                .collect();
            Ok((huds, downloaded))
        });
        self.load = Some(cx.spawn(async move |tab, cx| {
            let read = read.await;
            let _ = tab.update(cx, |tab, cx| {
                match read {
                    Ok((huds, downloaded)) => {
                        tab.facet_values =
                            FACET_ROWS.iter().map(|(f, _)| facets(&huds, *f)).collect();
                        tab.huds = huds.into();
                        tab.downloaded = Rc::new(downloaded);
                        tab.error = None;
                    }
                    Err(err) => tab.error = Some(read_failed(&err)),
                }
                // A failed load ends the skeleton too, so v5's "Could not load store" shows.
                tab.loaded = true;
                tab.sync_reload_pending = false;
                tab.refilter(cx);
            });
        }));
    }

    // Applies the filter. The re-armed grid hook asks for the new cards' thumbnails on the
    // next frame; an empty list draws no grid, so it asks directly to drop the old ones.
    fn refilter(&mut self, cx: &mut Context<Self>) {
        self.shown = self.filter.apply(&self.huds).into();
        self.grid.rearm();
        if self.shown.is_empty() {
            self.want_images(cx);
        }
        cx.notify();
    }

    fn edit_filter(&mut self, edit: impl FnOnce(&mut CuratedFilter), cx: &mut Context<Self>) {
        edit(&mut self.filter);
        self.refilter(cx);
    }

    // v5's Clear: the filter's defaults, and an emptied search box.
    fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.edit_filter(CuratedFilter::clear, cx);
    }

    // Asks for the first screenshot of each drawn card.
    fn want_images(&mut self, cx: &mut Context<Self>) {
        let Some(images) = RemoteImages::global(cx) else {
            return;
        };
        let end = self.visible.end.min(self.shown.len());
        let start = self.visible.start.min(end);
        let keys: Vec<ImageKey> = self.shown[start..end]
            .iter()
            .filter_map(|&ix| self.huds[ix].images.first().cloned().map(ImageKey::card))
            .collect();
        images.update(cx, |images, cx| images.want(&self.viewer, keys, cx));
    }

    // v5's toolbar: count and search, then OS toggles, prereleases, Clear and More filters.
    fn toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let count = if self.loaded {
            count_label(self.shown.len(), self.huds.len())
        } else {
            String::new()
        };
        // v5's joined outline group: the next on/off set comes back whole and replaces `os`.
        let os_group = ToggleGroup::new(ids::STORE_FILTER_OS_GROUP)
            .outline()
            .segmented()
            .children(Os::ALL.map(|os| {
                Toggle::new(ids::store_filter_os(os.key()))
                    .label(os.label())
                    // The kit's toggle reads its name from the tooltip, not the label.
                    .tooltip(format!("HUDs that work on {}", os.label()))
                    .checked(self.filter.os.contains(&os))
                    .px_3()
                    .font_weight(FontWeight::MEDIUM)
            }))
            .on_click(cx.listener(|tab, next: &Vec<bool>, _, cx| {
                let next = next.clone();
                tab.edit_filter(
                    |f| {
                        f.os = Os::ALL
                            .into_iter()
                            .zip(next)
                            .filter(|(_, on)| *on)
                            .map(|(os, _)| os)
                            .collect()
                    },
                    cx,
                );
            }));
        // v5's gap-6 column: the search row, then the filter rows 24 px below.
        v_flex()
            .gap_6()
            .child(
                h_flex()
                    .justify_between()
                    .gap_4()
                    .child(
                        div()
                            .id(ids::STORE_SEARCH)
                            .test_support()
                            .w(px(288.))
                            .child(
                                Input::new(&self.search)
                                    .h_9()
                                    .prefix(Icon::new(IconName::Search).text_color(muted)),
                            ),
                    )
                    .child(labelled(
                        div().text_sm().text_color(muted),
                        ids::STORE_COUNT,
                        count,
                    )),
            )
            // v5's StoreFilters: the filter row, then the expanded facets 12 px below.
            .child(
                v_flex()
                    .gap_3()
                    .child(
                        h_flex()
                            .gap_4()
                            .items_center()
                            .flex_wrap()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(labelled(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(muted),
                                        ids::STORE_FILTER_OS_LABEL,
                                        "OS:",
                                    ))
                                    .child(
                                        div()
                                            .id(ids::STORE_FILTER_OS_GROUP)
                                            .test_support()
                                            .child(os_group),
                                    ),
                            )
                            .child(
                                Checkbox::new(ids::STORE_FILTER_PRERELEASE)
                                    .label("Hide prereleases")
                                    .checked(self.filter.hide_prerelease)
                                    .on_click(cx.listener(|tab, checked: &bool, _, cx| {
                                        let checked = *checked;
                                        tab.edit_filter(|f| f.hide_prerelease = checked, cx);
                                    })),
                            )
                            .child(
                                h_flex()
                                    .ml_auto()
                                    .gap_2()
                                    // v5 offers Clear only under hasActiveFilters.
                                    .children(self.filter.is_active().then(|| {
                                        Button::new(ids::STORE_FILTER_CLEAR)
                                            .ghost()
                                            .icon(IconName::X)
                                            .label("Clear filters")
                                            .on_click(cx.listener(|tab, _, window, cx| {
                                                tab.clear(window, cx)
                                            }))
                                    }))
                                    .child(
                                        Button::new(ids::STORE_FILTER_MORE)
                                            .ghost()
                                            .icon(if self.more {
                                                IconName::ChevronUp
                                            } else {
                                                IconName::ChevronDown
                                            })
                                            .label(if self.more {
                                                "Less filters"
                                            } else {
                                                "More filters"
                                            })
                                            .on_click(cx.listener(|tab, _, _, cx| {
                                                tab.more = !tab.more;
                                                cx.notify();
                                            })),
                                    ),
                            ),
                    )
                    .children(self.more.then(|| self.facet_rows(cx))),
            )
    }

    // One row of toggle chips per facet that has values.
    fn facet_rows(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let rows = FACET_ROWS
            .iter()
            .zip(&self.facet_values)
            .filter(|(_, values)| !values.is_empty())
            .map(|(&(facet, title), values)| {
                let selected = self.filter.selected(facet);
                h_flex()
                    .gap_2()
                    .flex_wrap()
                    .child(div().w(px(96.)).text_sm().text_color(muted).child(title))
                    .children(values.iter().map(|value| {
                        let owned = value.clone();
                        chip(
                            ids::store_filter(facet.key(), value),
                            value.clone(),
                            selected.contains(value),
                        )
                        .on_click(cx.listener(move |tab, _, _, cx| {
                            tab.edit_filter(|f| f.toggle(facet, &owned), cx)
                        }))
                    }))
            })
            .collect::<Vec<_>>();
        v_flex().gap_2().children(rows)
    }

    fn error_line(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let error = self.error.clone()?;
        let this = cx.entity().downgrade();
        Some(store_error_line(
            error,
            move |_, cx| {
                this.update(cx, |tab, cx| tab.reload(cx)).ok();
            },
            cx,
        ))
    }

    // v5's states: skeletons before the first load and while a sync fills an empty store, the
    // WifiOff card for an empty idle store, the Store card when the filters hide every HUD.
    // The skeletons take v5's viewport breakpoints, since the grid has no measured width yet.
    fn body(&self, viewport_width: Pixels, cx: &mut Context<Self>) -> Option<AnyElement> {
        let columns = columns_for_store(viewport_width - SHELL_CHROME_WIDTH, 4);
        let skeletons =
            |id, label| skeleton_cards(id, label, SKELETON_CARDS, columns, SkeletonTop::Image, cx);
        if !self.loaded {
            return Some(skeletons(ids::STORE_LOADING, "Loading..."));
        }
        if self.shown.is_empty() {
            if self.huds.is_empty() {
                if self.is_syncing(cx) {
                    return Some(skeletons(ids::STORE_SYNCING, "Syncing HUDs..."));
                }
                return Some(empty_state_card(
                    ids::STORE_UNLOADED,
                    IconName::WifiOff,
                    "Could not load store",
                    None,
                    "Check your internet connection and try again.",
                    cx,
                ));
            }
            return Some(empty_state_card(
                ids::STORE_EMPTY,
                IconName::Store,
                "No HUDs match your filters",
                Some(ids::STORE_EMPTY_HINT),
                "Try adjusting your search or filter criteria.",
                cx,
            ));
        }
        let (huds, shown, downloaded) = (
            self.huds.clone(),
            self.shown.clone(),
            self.downloaded.clone(),
        );
        Some(self.grid.render_sized(
            shown.len(),
            |width| width * (9. / 16.) + CARD_TEXT_HEIGHT,
            move |ix, _, cx| {
                let hud = &huds[shown[ix]];
                curated_card(hud, downloaded.contains(&hud.hud_id), cx)
            },
        ))
    }
}

// A filter toggle: primary while selected, outline otherwise.
fn chip(id: gpui_kit::ElementId, label: impl Into<SharedString>, selected: bool) -> Button {
    let button = Button::new(id).small().label(label);
    if selected {
        button.primary()
    } else {
        button.outline()
    }
}

// v5's store HudCard: screenshot, name with a check when downloaded, author, up to four
// flags, and the button that opens the detail dialog.
fn curated_card(hud: &CuratedHud, downloaded: bool, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let hover = theme.accent.opacity(0.5);
    let id = StoreItemId::Curated(hud.hud_id.clone());
    let item = DetailItem {
        kind: KIND,
        id: id.clone(),
        name: hud.name.clone(),
        in_library: downloaded,
    };
    let shot = div()
        .id(ids::store_card_image(KIND, &id))
        .test_support()
        .flex_none()
        .w_full()
        .aspect_ratio(16. / 9.)
        .overflow_hidden()
        .bg(theme.muted);
    let image = hud
        .images
        .first()
        .zip(RemoteImages::global(cx))
        .map(|(url, images)| images.read(cx).image(&ImageKey::card(url.clone())));
    let shot = match image {
        Some(RemoteImage::Ready(image)) => {
            shot.child(img(image).size_full().object_fit(ObjectFit::Cover))
        }
        _ => shot,
    };
    let label = if downloaded {
        "Re-download"
    } else {
        "Download"
    };
    // v5 opens the detail from a click anywhere on the card, the button included.
    let open: OpenCard = Rc::new(move |window, cx| {
        open_detail(item.clone(), curated_body(), window, cx);
    });
    let button = {
        let open = open.clone();
        download_button(
            KIND,
            &id,
            label.into(),
            downloaded,
            false,
            move |window, cx| {
                // Kit buttons let clicks bubble, and the card behind opens the dialog too.
                cx.stop_propagation();
                open(window, cx);
            },
            cx,
        )
    };
    // v5's ghost ExternalLink icon, only for a HUD with a repository.
    let repo = hud.repo.clone().map(|repo| {
        Button::new(ids::store_card_repo(KIND, &id))
            .ghost()
            .flex_none()
            .icon(IconName::ExternalLink)
            .named("Visit the HUD's page")
            .size(px(36.))
            .on_click(move |_, _, cx| {
                // The card itself opens the detail, so the click stops here.
                cx.stop_propagation();
                cx.open_url(&repo);
            })
    });
    card_frame(cx)
        .id(ids::store_card(KIND, &id))
        .accessibility_id(ids::store_card(KIND, &id).to_string())
        .aria_label(hud.name.clone())
        .test_support()
        .cursor_pointer()
        // v5's hover:bg-accent/50 over the card's group-box background.
        .hover(move |card| card.bg(hover))
        .on_click(move |_, window, cx| open(window, cx))
        .size_full()
        .flex()
        .flex_col()
        // v5's Card is py-6, so the image sits 24 px below the card's top edge.
        .pt_6()
        .overflow_hidden()
        .child(shot)
        .child(
            v_flex()
                .flex_1()
                .min_h_0()
                // v5's px-6 body under the Card's gap-6 and over its py-6, its sections gap-6 apart.
                .p_6()
                .gap_6()
                // v5's header: the 24 px name and 20 px author column with pb-2 and the grid's
                // 6 px row gap below, then the repo icon at the top right.
                .child(
                    h_flex()
                        .items_start()
                        .justify_between()
                        .gap_2()
                        .pb(px(14.))
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(
                                    h_flex()
                                        .gap_1()
                                        .child(labelled(
                                            div()
                                                .truncate()
                                                .line_height(px(24.))
                                                .font_weight(FontWeight::SEMIBOLD),
                                            ids::store_card_name(KIND, &id),
                                            hud.name.clone(),
                                        ))
                                        .children(downloaded.then(|| {
                                            Icon::new(IconName::CircleCheck)
                                                .small()
                                                .text_color(theme.success)
                                        })),
                                )
                                .child(
                                    div()
                                        .truncate()
                                        .text_sm()
                                        .line_height(px(20.))
                                        .text_color(theme.muted_foreground)
                                        .child(format!("by {}", hud.author)),
                                ),
                        )
                        .children(repo),
                )
                // v5's CardContent: the wrapped flags (left out when there are none) over pb-2.
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .pb_2()
                        .when(!hud.flags.is_empty(), |content| {
                            content.child(
                                badge_rows(ids::store_card_tags(KIND, &id), FLAG_ROWS, px(4.))
                                    .children(hud.flags.iter().take(CARD_FLAGS).enumerate().map(
                                        |(ix, f)| {
                                            tag_badge(
                                                ids::store_card_tag(KIND, &id, ix),
                                                Tag::secondary()
                                                    .outline()
                                                    .text_color(theme.foreground),
                                                theme.border,
                                                f.clone(),
                                            )
                                        },
                                    )),
                            )
                        }),
                )
                // v5's footer: the button alone at the card's bottom, full width.
                .child(div().mt_auto().w_full().child(button)),
        )
        .into_any_element()
}

impl Render for CuratedTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // v5's `flex flex-col gap-6`: the first card row sits 24 px under the toolbar.
        let viewport_width = window.viewport_size().width;
        v_flex()
            .size_full()
            .gap_6()
            .child(self.toolbar(cx))
            .children(self.error_line(cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .children(self.body(viewport_width, cx)),
            )
    }
}
