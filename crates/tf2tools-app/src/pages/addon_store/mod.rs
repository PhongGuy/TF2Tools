//! The Addon Store page: mastercomfig addons with Download, Update and Installed states.

pub mod model;

use std::collections::HashMap;
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _, Size, h_flex, v_flex,
};
use gpui_kit::{
    AnyElement, App, Context, EventEmitter, FontWeight, HighlightStyle, InteractiveElement as _,
    InteractiveText, IntoElement, ParentElement as _, Pixels, Render, ScrollHandle,
    StatefulInteractiveElement as _, Styled as _, StyledText, Subscription, Task,
    TestSupportExt as _, UnderlineStyle, WeakEntity, Window, div, px, transparent_black,
};
use tf2tools_core::{AddonEntry, AddonName, StoreItem, StoreItemId, StoreKind};

use crate::addon_updates::{AddonEvent, AddonUpdates};
use crate::ids;
use crate::pages::common::{
    SkeletonTop, badge_of, card_frame, empty_state_card, no_tf2_warning, skeleton_cards, steady,
};
use crate::services::{OpError, Tf2State, run_blocking, tf2_state};
use crate::shell::CONTENT_PADDING;
use crate::shell::route::{NavigateTo, Page};
use crate::store_download::{StoreDownloads, download_button, format_bytes};
use crate::store_messages::read_failed;
use crate::sync::sync_hub;
use crate::widgets::card_grid::{SHELL_CHROME_WIDTH, columns_for, stretch_rows};
use crate::widgets::scroll::scroll_beside;
use crate::widgets::store_error::store_error_line;
use model::{AddonState, addon_state, downloads_label, ordered, store_release};

const KIND: StoreKind = StoreKind::Addons;

/// v5's placeholder cards while the store loads or a sync fills it.
const SKELETON_CARDS: usize = 6;

/// The info box's sentence and its two underlined link words, in order.
const INFO_TEXT: &str = "These addons work best with a mastercomfig preset installed. Download \
                         addons to your library, then install them from the Addons page.";

/// An underlined word in the Addon Store's info box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InfoLink {
    /// "mastercomfig": opens mastercomfig's site.
    Mastercomfig,
    /// "Addons": shows the Addons page.
    Addons,
}

impl InfoLink {
    /// The web page the link opens, or `None` for an in-app page.
    pub fn url(self) -> Option<&'static str> {
        match self {
            Self::Mastercomfig => Some("https://mastercomfig.com"),
            Self::Addons => None,
        }
    }

    // The word the link underlines in `INFO_TEXT`.
    fn word(self) -> &'static str {
        match self {
            Self::Mastercomfig => "mastercomfig",
            Self::Addons => "Addons",
        }
    }
}

/// The library's addons by name, as the last load read them.
type Library = Rc<HashMap<AddonName, AddonEntry>>;

/// The Addon Store. The whole addon table and the library's addons load together off the UI
/// thread, and reload when TF2's state changes or an addon sync ends.
pub struct AddonStorePage {
    items: Rc<[StoreItem]>,
    library: Library,
    loaded: bool,
    error: Option<String>,
    reloads: usize,
    /// Whether an addon sync ran at the last hub notification, to catch the run's end.
    hub_running: bool,
    /// An addon run ended and its reload has not landed; the page still reads as syncing.
    sync_reload_pending: bool,
    load: Option<Task<()>>,
    /// The card grid's scroll.
    scroll: ScrollHandle,
    /// Set while the page is away; it then skips reloads and rereads on return.
    hidden: bool,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<NavigateTo> for AddonStorePage {}

impl AddonStorePage {
    /// Needs the `Services` and `Tf2Global` globals; loads at once.
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        // A new TF2 path changes which addons read Installed.
        let mut subscriptions = vec![cx.observe(&tf2_state(cx), |page, _, cx| page.reload(cx))];
        // Reload once per addon run, on its end (finished, failed or cancelled); ticks and
        // other kinds' runs only redraw the Refresh button.
        let hub = sync_hub(cx);
        let hub_running = hub
            .as_ref()
            .is_some_and(|hub| hub.read(cx).is_running(KIND));
        if let Some(hub) = hub {
            subscriptions.push(cx.observe(&hub, |page, hub, cx| {
                let running = hub.read(cx).is_running(KIND);
                let ended = page.hub_running && !running;
                page.hub_running = running;
                if ended {
                    page.sync_reload_pending = true;
                    page.reload(cx);
                }
                cx.notify();
            }));
        }
        // `AddonUpdates::global` installs `StoreDownloads`, so the progress observer below
        // always exists. Reinstalls redraw their card as "Installing..."; a settled download
        // or reinstall changes the library, so the cards reread it.
        let updates = AddonUpdates::global(cx);
        if let Some(downloads) = StoreDownloads::global(cx) {
            subscriptions.push(cx.observe(&downloads, |_, _, cx| cx.notify()));
        }
        subscriptions.push(cx.observe(&updates, |_, _, cx| cx.notify()));
        subscriptions.push(cx.subscribe(&updates, |page, _, event, cx| match event {
            AddonEvent::Settled { .. } => page.reload(cx),
        }));
        let mut page = Self {
            items: Rc::from(Vec::new()),
            library: Rc::default(),
            loaded: false,
            error: None,
            reloads: 0,
            hub_running,
            sync_reload_pending: false,
            load: None,
            scroll: ScrollHandle::new(),
            hidden: false,
            _subscriptions: subscriptions,
        };
        page.reload(cx);
        page
    }

    /// True once a load has ended, failed or not.
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// True while an addon sync runs and until the reload after it lands, as v5 holds `syncing`.
    pub fn is_syncing(&self) -> bool {
        self.hub_running || self.sync_reload_pending
    }

    /// Completed loads, counting the first.
    pub fn reloads(&self) -> usize {
        self.reloads
    }

    /// The card state of addon `name`, from the last load.
    pub fn state_of(&self, name: &AddonName) -> AddonState {
        addon_state(self.library.get(name))
    }

    /// Download or Update `name` through `AddonUpdates`, reinstalling when its state is Update.
    /// Does nothing while any download or addon job runs, as v5's `isAnyBusy` gates it.
    pub fn download(&mut self, name: &AddonName, window: &mut Window, cx: &mut Context<Self>) {
        if any_busy(cx) {
            return;
        }
        let display = self
            .items
            .iter()
            .find(|item| matches!(item.id(), StoreItemId::Addon(n) if n == name))
            .map_or_else(|| name.as_str().to_owned(), |item| item.name().to_owned());
        let reinstall = self.state_of(name) == AddonState::Update;
        let win = window.window_handle();
        AddonUpdates::global(cx).update(cx, |u, cx| {
            u.start(name.clone(), display, reinstall, win, cx);
        });
    }

    /// Hides or shows the page. Shown again, it rereads, since the Addons page may have changed
    /// the library meanwhile; the scroll stays where it was.
    pub fn set_hidden(&mut self, hidden: bool, cx: &mut Context<Self>) {
        if hidden != std::mem::replace(&mut self.hidden, hidden) && !hidden {
            self.reload(cx);
        }
    }

    /// Rereads the store's addons and the library's off the UI thread; a newer reload drops an
    /// older one in flight. Without a TF2 install nothing reads Installed. A hidden page
    /// rereads on return instead.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        if self.hidden {
            return;
        }
        let tf2 = tf2_state(cx).read(cx).install().cloned();
        let read = run_blocking(cx, move |s| {
            let items = s
                .store
                .store_items(KIND)
                .map_err(|e| OpError::core("Failed to load the store", e))?;
            let library = s
                .library
                .list_addons(tf2.as_ref())
                .map_err(|e| OpError::core("Failed to load the store", e))?;
            let library = library.into_iter().map(|e| (e.name().clone(), e)).collect();
            Ok((ordered(items), library))
        });
        self.load = Some(cx.spawn(async move |page, cx| {
            let read = read.await;
            let _ = page.update(cx, |page, cx| {
                match read {
                    Ok((items, library)) => {
                        page.items = items.into();
                        page.library = Rc::new(library);
                        page.error = None;
                    }
                    Err(err) => page.error = Some(read_failed(&err)),
                }
                page.loaded = true;
                page.sync_reload_pending = false;
                page.reloads += 1;
                cx.notify();
            });
        }));
    }

    /// Follows an info box link: a web link opens in the browser, Addons asks the shell to show
    /// the Addons page.
    pub fn follow_info_link(&mut self, link: InfoLink, cx: &mut Context<Self>) {
        match link.url() {
            Some(url) => cx.open_url(url),
            None => cx.emit(NavigateTo(Page::Addons)),
        }
    }

    // Forces an addon sync; the hub observer reloads when it ends.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        if let Some(hub) = sync_hub(cx) {
            hub.update(cx, |hub, cx| hub.start(KIND, true, cx));
        }
    }

    // v5's warning while TF2 is not found; a detection still in flight shows nothing yet.
    fn no_tf2(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let missing = matches!(tf2_state(cx).read(cx), Tf2State::Missing { .. });
        missing.then(|| {
            no_tf2_warning(
                ids::ADDON_STORE_NO_TF2,
                "Set the TF2 folder in Settings to install addons.",
                cx,
            )
        })
    }

    // Release badge and subtitle on the left, Refresh on the right; the top bar names the page.
    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let syncing = self.is_syncing();
        let tag = store_release(&self.items).map(|tag| {
            badge_of(
                ids::ADDON_STORE_TAG,
                Tag::secondary(),
                transparent_black(),
                tag.to_owned(),
            )
        });
        h_flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap_4()
            .child(
                v_flex()
                    .gap_1()
                    .children(tag.map(|tag| h_flex().child(tag)))
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("Mastercomfig performance and quality-of-life addons"),
                    ),
            )
            .child(
                Button::new(ids::STORE_REFRESH)
                    .outline()
                    .icon(IconName::RefreshCw)
                    .label(if syncing { "Syncing..." } else { "Refresh" })
                    .loading(syncing)
                    .loading_icon(IconName::RefreshCw)
                    .disabled(syncing)
                    .on_click(cx.listener(|page, _, _, cx| page.refresh(cx))),
            )
    }

    fn info(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        // The link words are underlined ranges of one wrapping text; a click on either follows it.
        // A word missing from the text just goes without its link.
        let (links, ranges): (Vec<_>, Vec<_>) = [InfoLink::Mastercomfig, InfoLink::Addons]
            .into_iter()
            .filter_map(|link| {
                let start = INFO_TEXT.find(link.word())?;
                Some((link, start..start + link.word().len()))
            })
            .unzip();
        let underline = HighlightStyle {
            underline: Some(UnderlineStyle {
                thickness: px(1.),
                ..UnderlineStyle::default()
            }),
            ..HighlightStyle::default()
        };
        let text = StyledText::new(INFO_TEXT)
            .with_highlights(ranges.iter().map(|range| (range.clone(), underline)));
        let page = cx.entity().downgrade();
        let text = InteractiveText::new(ids::ADDON_STORE_INFO_TEXT, text).on_click(
            ranges,
            move |ix, _, cx| {
                let _ = page.update(cx, |page, cx| page.follow_info_link(links[ix], cx));
            },
        );
        h_flex()
            .id(ids::ADDON_STORE_INFO)
            .test_support()
            .items_start()
            .gap_3()
            .p_4()
            .rounded(theme.radius_lg)
            .border_1()
            .border_color(theme.border)
            .bg(theme.muted.opacity(0.3))
            .text_color(theme.muted_foreground)
            .child(
                Icon::new(IconName::Info)
                    .with_size(Size::Size(px(20.)))
                    .mt_0p5()
                    .flex_none(),
            )
            // min_w_0 lets the sentence wrap inside the box instead of sizing the row to one line.
            .child(div().flex_1().min_w_0().text_sm().child(text))
    }

    // The same inline error as the HUD Store's Curated tab.
    fn error_line(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let error = self.error.clone()?;
        let this = cx.entity().downgrade();
        Some(store_error_line(
            error,
            move |_, cx| {
                this.update(cx, |page, cx| page.reload(cx)).ok();
            },
            cx,
        ))
    }

    // v5's states: skeletons before the first load and while a sync fills an empty store, the
    // WifiOff card when a good load found no addons and nothing syncs, else the cards. Both
    // grids take v5's viewport breakpoints; the few dozen cards need no virtualization.
    fn body(&self, viewport_width: Pixels, cx: &mut Context<Self>) -> Option<AnyElement> {
        let columns = columns_for(viewport_width - SHELL_CHROME_WIDTH);
        let skeletons =
            |id, label| skeleton_cards(id, label, SKELETON_CARDS, columns, SkeletonTop::None, cx);
        if !self.loaded {
            return Some(skeletons(ids::STORE_LOADING, "Loading..."));
        }
        if self.items.is_empty() {
            if self.is_syncing() {
                return Some(skeletons(ids::STORE_SYNCING, "Syncing addons..."));
            }
            if self.error.is_some() {
                return None;
            }
            return Some(empty_state_card(
                ids::STORE_EMPTY,
                IconName::WifiOff,
                "Could not load addons",
                None,
                "Check your internet connection and try again.",
                cx,
            ));
        }
        let page = cx.entity().downgrade();
        let updates = AddonUpdates::global(cx);
        let blocked = any_busy(cx);
        let cards = self
            .items
            .iter()
            .map(|item| {
                let (state, installing) = match item.id() {
                    StoreItemId::Addon(name) => (
                        addon_state(self.library.get(name)),
                        updates.read(cx).is_installing(name),
                    ),
                    StoreItemId::GameBanana(_) | StoreItemId::Curated(_) | _ => {
                        (AddonState::Download, false)
                    }
                };
                addon_card(item, state, installing, blocked, page.clone(), cx)
            })
            .collect();
        Some(
            scroll_beside(
                ids::grid_scrollbar(ids::STORE_GRID),
                &self.scroll,
                div()
                    .id(ids::STORE_GRID)
                    .test_support()
                    .size_full()
                    .track_scroll(&self.scroll)
                    .overflow_y_scroll()
                    .child(stretch_rows(cards, columns)),
            )
            .size_full()
            .into_any_element(),
        )
    }
}

impl Render for AddonStorePage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport_width = window.viewport_size().width;
        v_flex()
            .id(ids::page(Page::AddonStore))
            .test_support()
            .size_full()
            .p(CONTENT_PADDING)
            .gap_6()
            .children(self.no_tf2(cx))
            .child(self.header(cx))
            .child(self.info(cx))
            .children(self.error_line(cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .children(self.body(viewport_width, cx)),
            )
    }
}

// Whether any store download or addon job (lookup, download, reinstall) runs.
fn any_busy(cx: &mut App) -> bool {
    StoreDownloads::global(cx).is_some_and(|d| d.read(cx).is_busy())
        || AddonUpdates::global(cx).read(cx).is_busy()
}

// v5's store AddonCard: name with a check when installed, the release tag, the description,
// size and downloads, then a Download or Update button or an In Library or Installed badge.
fn addon_card(
    item: &StoreItem,
    state: AddonState,
    installing: bool,
    blocked: bool,
    page: WeakEntity<AddonStorePage>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let id = item.id().clone();
    let footer = if installing {
        Button::new(ids::store_download(KIND, &id))
            .primary()
            .label("Installing...")
            .loading(true)
            .disabled(true)
            .into_any_element()
    } else if let (true, StoreItemId::Addon(name)) = (state.is_action(), &id) {
        let name = name.clone();
        let click = move |window: &mut Window, cx: &mut App| {
            let _ = page.update(cx, |page, cx| page.download(&name, window, cx));
        };
        download_button(KIND, &id, state.label().into(), false, blocked, click, cx)
    } else {
        let icon = match state {
            AddonState::Installed => IconName::CircleCheck,
            AddonState::Download | AddonState::InLibrary | AddonState::Update => IconName::Library,
        };
        div()
            .id(ids::store_state(KIND, &id))
            .aria_label(state.label())
            .test_support()
            .w_full()
            .child(
                steady(
                    Tag::secondary()
                        .w_full()
                        .py_1p5()
                        .justify_center()
                        .gap_1p5()
                        .child(Icon::new(icon).xsmall())
                        .child(state.label()),
                )
                .w_full(),
            )
            .into_any_element()
    };
    card_frame(cx)
        .id(ids::store_card(KIND, &id))
        .accessibility_id(ids::store_card(KIND, &id).to_string())
        .aria_label(item.name().to_owned())
        .test_support()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .overflow_hidden()
        // The library card's v5 spacing: py-6 gap-6, a pb-2 header, a pb-2 growing content.
        .px_6()
        .py_6()
        .gap_6()
        .child(
            h_flex()
                .items_start()
                .justify_between()
                .gap_2()
                .pb_2()
                .child(
                    h_flex()
                        .min_w_0()
                        .flex_1()
                        .items_center()
                        .gap_1p5()
                        .child(
                            div()
                                .truncate()
                                .line_height(px(16.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(item.name().to_owned()),
                        )
                        .children((state == AddonState::Installed).then(|| {
                            div()
                                .id(ids::store_check(KIND, &id))
                                .test_support()
                                .flex_none()
                                .child(
                                    Icon::new(IconName::CircleCheck)
                                        .with_size(Size::Medium)
                                        .text_color(theme.primary),
                                )
                        })),
                )
                .children(item.release_tag().map(|tag| {
                    div().flex_none().child(badge_of(
                        ids::store_release_tag(KIND, &id),
                        Tag::secondary().outline().text_color(theme.foreground),
                        theme.border,
                        tag.to_owned(),
                    ))
                })),
        )
        .child(
            v_flex()
                .flex_1()
                .pb_2()
                .text_color(theme.muted_foreground)
                .child(
                    div()
                        .id(ids::store_description(KIND, &id))
                        .test_support()
                        .text_sm()
                        .child(item.description().to_owned()),
                )
                .child(
                    h_flex()
                        .mt_2()
                        .gap_3()
                        .text_xs()
                        .line_height(px(16.))
                        .child(format_bytes(item.file_size()))
                        .child(downloads_label(item.download_count())),
                ),
        )
        .child(footer)
        .into_any_element()
}
