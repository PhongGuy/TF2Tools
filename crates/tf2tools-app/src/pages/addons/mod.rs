//! The local Addons page: the mastercomfig addons in the library, installed or not.

pub mod model;

use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _, Size, h_flex, v_flex,
};
use gpui_kit::{
    AnyElement, AnyWindowHandle, App, Context, Entity, EventEmitter, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, TestSupportExt as _, Window, div, px,
};
use tf2tools_core::{AddonEntry, AddonName, LogLevel, NewLogEntry, StoreItemId, StoreKind};

use crate::activity;
use crate::addon_updates::AddonUpdates;
use crate::ids;
use crate::library_events::{LibraryChanged, LibraryEvents};
use crate::local_messages::{describe_library, describe_tf2_install};
use crate::pages::common::{badge_of, card_frame, no_tf2_warning};
use crate::services::{OpError, Services, Tf2State, require_tf2, run_blocking, tf2_state};
use crate::shell::CONTENT_PADDING;
use crate::shell::confirm::{Confirm, confirm};
use crate::shell::route::{NavigateTo, Page};
use crate::shell::toast::{ToastKind, toast, toast_error};
use crate::store_download::StoreDownloads;
use crate::widgets::card_grid::{SHELL_CHROME_WIDTH, columns_for, stretch_rows};
use crate::widgets::labelled;
use model::{AddonAction, actions, count_line, needs_tf2};

/// The Addons page. Built fresh on each visit; it reloads when TF2's state or the library changes.
pub struct AddonsPage {
    win: AnyWindowHandle,
    addons: Rc<[AddonEntry]>,
    loaded: bool,
    /// Whether a load with a TF2 install has adopted the addons found in tf/custom.
    adopted: bool,
    /// Bumped by each reload, so a slower, older load never overwrites a newer one.
    load_generation: u64,
    updates: Entity<AddonUpdates>,
    /// The addon this page works on now and what it does to it.
    busy: Option<(AddonName, AddonAction)>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<NavigateTo> for AddonsPage {}

impl AddonsPage {
    /// Needs the `Services` and `Tf2Global` globals. The first load adopts addons found in
    /// tf/custom (with a TF2 install), then lists the library, off the UI thread.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let tf2 = cx.observe(&tf2_state(cx), |page, _, cx| page.reload(cx));
        let events = LibraryEvents::global(cx);
        let library = cx.subscribe(&events, |page, _, _: &LibraryChanged, cx| page.reload(cx));
        // Reinstalls and downloads (progress and their end) redraw the cards they work on.
        let updates = AddonUpdates::global(cx);
        let updating = cx.observe(&updates, |_, _, cx| cx.notify());
        let downloads = StoreDownloads::install(cx);
        let downloading = cx.observe(&downloads, |_, _, cx| cx.notify());
        let mut page = Self {
            win: window.window_handle(),
            addons: Rc::from(Vec::new()),
            loaded: false,
            adopted: false,
            load_generation: 0,
            updates,
            busy: None,
            _subscriptions: vec![tf2, library, updating, downloading],
        };
        page.reload(cx);
        page
    }

    /// True once a load has ended, failed or not.
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// The library's addons from the last load, in `list_addons` order.
    pub fn addons(&self) -> &[AddonEntry] {
        &self.addons
    }

    /// v5's `isAnyBusy`: an operation of this page, a pending or running reinstall, or any
    /// store download.
    pub fn is_any_busy(&self, cx: &App) -> bool {
        self.busy.is_some()
            || self.updates.read(cx).is_busy()
            || StoreDownloads::global(cx).is_some_and(|d| d.read(cx).is_busy())
    }

    // Marks `busy`, runs `op` off the UI thread, toasts its (title, description) or the error,
    // then clears `busy`. A success reloads through LibraryChanged, a failure reloads directly.
    fn run(
        &mut self,
        busy: (AddonName, AddonAction),
        op: impl FnOnce(&Services) -> Result<(SharedString, SharedString), OpError> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        self.busy = Some(busy);
        cx.notify();
        let running = run_blocking(cx, op);
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let done = running.await;
            let ok = done.is_ok();
            cx.update(|cx| {
                let _ = win.update(cx, |_, window, cx| match done {
                    Ok((title, description)) => {
                        toast(window, cx, ToastKind::Success, title, description)
                    }
                    Err(err) => toast_error(window, cx, &err),
                });
                // Outside the page, so a success is announced even when the page is gone; a
                // surviving page reloads through its own LibraryChanged subscription.
                if ok {
                    LibraryEvents::changed(cx);
                }
            });
            let _ = page.update(cx, |page, cx| {
                page.busy = None;
                cx.notify();
                if !ok {
                    page.reload(cx)
                }
            });
        })
        .detach();
    }

    fn install(
        &mut self,
        name: AddonName,
        display: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tf2 = match require_tf2(cx) {
            Ok(tf2) => tf2,
            Err(err) => return toast_error(window, cx, &err),
        };
        self.run(
            (name.clone(), AddonAction::Install),
            move |s| {
                s.library
                    .install_addon(&name, &tf2)
                    .map_err(|e| describe_tf2_install(&e).into_op("Failed to install addon"))?;
                let message = format!("Installed addon: {}", name.as_str());
                activity::write(
                    s,
                    &NewLogEntry::new(LogLevel::Info, "addon_store", "install", message),
                );
                Ok((
                    "Addon installed".into(),
                    format!("{display} is now active").into(),
                ))
            },
            cx,
        );
    }

    fn uninstall(
        &mut self,
        name: AddonName,
        display: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tf2 = match require_tf2(cx) {
            Ok(tf2) => tf2,
            Err(err) => return toast_error(window, cx, &err),
        };
        self.run(
            (name.clone(), AddonAction::Uninstall),
            move |s| {
                s.library
                    .uninstall_addon(&name, &tf2)
                    .map_err(|e| describe_tf2_install(&e).into_op("Failed to uninstall addon"))?;
                let message = format!("Uninstalled addon: {}", name.as_str());
                activity::write(
                    s,
                    &NewLogEntry::new(LogLevel::Info, "addon_store", "uninstall", message),
                );
                let removed = format!("{display} has been removed");
                Ok(("Addon uninstalled".into(), removed.into()))
            },
            cx,
        );
    }

    // Asks first; only a confirmed answer deletes, and an answer that lands after the page was
    // dropped deletes nothing. Core also removes the addon from tf/custom when TF2 is known.
    fn delete(
        &mut self,
        name: AddonName,
        display: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let answer = confirm(Confirm::delete_addon(), window, cx);
        cx.spawn(async move |page, cx| {
            if !answer.recv().await.unwrap_or(false) {
                return;
            }
            let _ = page.update(cx, |page, cx| {
                // Read TF2 only once answered, so a path change during the dialog counts.
                let tf2 = tf2_state(cx).read(cx).install().cloned();
                page.run(
                    (name.clone(), AddonAction::Delete),
                    move |s| {
                        s.library
                            .delete_addon(&name, tf2.as_ref())
                            .map_err(|e| describe_library(&e).into_op("Failed to delete addon"))?;
                        let message = format!("Deleted addon from library: {}", name.as_str());
                        let entry = NewLogEntry::new(LogLevel::Info, "addon", "delete", message);
                        activity::write(s, &entry);
                        let removed = format!("{display} removed from library");
                        Ok(("Addon removed".into(), removed.into()))
                    },
                    cx,
                );
            });
        })
        .detach();
    }

    // Downloads the store's newest release into the library through `AddonUpdates`, which
    // reads the release file itself and, for an installed addon, reinstalls it afterwards.
    fn update(
        &mut self,
        name: AddonName,
        display: String,
        installed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let win = window.window_handle();
        self.updates
            .update(cx, |u, cx| u.start(name, display, installed, win, cx));
    }

    // Lists the library in the background, adopting tf/custom's addons first on the first load
    // that knows TF2. A failed adoption only warns; a failed list toasts through the window.
    fn reload(&mut self, cx: &mut Context<Self>) {
        self.load_generation += 1;
        let generation = self.load_generation;
        let tf2 = tf2_state(cx).read(cx).install().cloned();
        let adopt = !self.adopted && tf2.is_some();
        let loading = run_blocking(cx, move |s| {
            if let (true, Some(tf2)) = (adopt, &tf2)
                && let Err(err) = s.library.adopt_installed_addons(tf2)
            {
                tracing::warn!(error = %err, "adopting installed addons failed");
            }
            s.library
                .list_addons(tf2.as_ref())
                .map_err(|e| describe_library(&e).into_op("Failed to load addons"))
        });
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let loaded = loading.await;
            let current = page
                .update(cx, |page, cx| {
                    if page.load_generation != generation {
                        return false;
                    }
                    if let Ok(addons) = &loaded {
                        page.addons = addons.as_slice().into();
                    }
                    page.adopted |= adopt;
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

    // The count line on the left; v5's outline "Browse Addon Store" on the right.
    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let installed = self.addons.iter().filter(|a| a.is_installed()).count();
        h_flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap_4()
            .child(labelled(
                div().text_sm().text_color(cx.theme().muted_foreground),
                ids::ADDONS_COUNT,
                count_line(self.addons.len(), installed),
            ))
            .child(
                Button::new(ids::ADDONS_BROWSE)
                    .outline()
                    .icon(IconName::Store)
                    .label("Browse Addon Store")
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(NavigateTo(Page::AddonStore)))),
            )
    }

    // "Installed Addons" (or its empty card), then "Your Library" when anything is left over.
    fn sections(&self, columns: usize, cx: &mut Context<Self>) -> AnyElement {
        let state = CardState {
            rest: self.is_any_busy(cx),
            has_tf2: tf2_state(cx).read(cx).install().is_some(),
            installing: self
                .busy
                .as_ref()
                .filter(|(_, action)| *action == AddonAction::Install)
                .map(|(name, _)| name.clone()),
            updates: self.updates.clone(),
            downloads: StoreDownloads::global(cx),
        };
        let (installed, library): (Vec<&AddonEntry>, Vec<&AddonEntry>) =
            self.addons.iter().partition(|a| a.is_installed());
        let installed_body = if installed.is_empty() {
            empty_card(
                ids::ADDONS_INSTALLED_EMPTY,
                "No addons installed",
                "Install addons from your library below or browse the Addon Store.",
                cx,
            )
            .py_8()
            .into_any_element()
        } else {
            rows(&installed, columns, &state, cx)
        };
        v_flex()
            .gap_6()
            .child(section(
                ids::ADDONS_INSTALLED,
                "Installed Addons",
                installed_body,
            ))
            .children((!library.is_empty()).then(|| {
                section(
                    ids::ADDONS_LIBRARY,
                    "Your Library",
                    rows(&library, columns, &state, cx),
                )
            }))
            .into_any_element()
    }

    // v5's empty library: the Puzzle state with a primary "Browse Addon Store".
    fn empty_library(&self, cx: &mut Context<Self>) -> AnyElement {
        empty_card(
            ids::ADDONS_EMPTY,
            "No addons in your library",
            "Browse the Addon Store to find performance and quality-of-life addons for TF2.",
            cx,
        )
        .py_12()
        .child(
            div().flex().justify_center().pb_8().child(
                Button::new(ids::ADDONS_EMPTY_BROWSE)
                    .primary()
                    .icon(IconName::Store)
                    .label("Browse Addon Store")
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(NavigateTo(Page::AddonStore)))),
            ),
        )
        .into_any_element()
    }
}

impl Render for AddonsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let missing = matches!(tf2_state(cx).read(cx), Tf2State::Missing { .. });
        let warning = missing.then(|| {
            no_tf2_warning(
                ids::ADDONS_NO_TF2,
                "Configure your TF2 installation path in Settings to manage addons.",
                cx,
            )
        });
        let body = self.loaded.then(|| {
            if self.addons.is_empty() {
                self.empty_library(cx)
            } else {
                let columns = columns_for(window.viewport_size().width - SHELL_CHROME_WIDTH);
                self.sections(columns, cx)
            }
        });
        v_flex()
            .id(ids::page(Page::Addons))
            .test_support()
            .size_full()
            .overflow_y_scroll()
            .p(CONTENT_PADDING)
            .gap_6()
            .children(warning)
            .child(self.header(cx))
            .children(body)
    }
}

// A titled section, its heading labelled for the tests and the pilot.
fn section(id: &'static str, title: &'static str, body: AnyElement) -> impl IntoElement {
    v_flex()
        .gap_3()
        .child(labelled(
            div().text_lg().font_weight(FontWeight::SEMIBOLD),
            id,
            title,
        ))
        .child(body)
}

// v5's EmptyState inside a Card: a Puzzle icon, the title and a muted description.
fn empty_card(
    id: &'static str,
    title: &'static str,
    description: &'static str,
    cx: &App,
) -> impl IntoElement + ParentElement + Styled {
    let muted = cx.theme().muted_foreground;
    card_frame(cx).id(id).test_support().child(
        v_flex()
            .items_center()
            .px_6()
            .py_4()
            .child(
                Icon::new(IconName::Puzzle)
                    .with_size(Size::Size(px(48.)))
                    .text_color(muted)
                    .mb_4(),
            )
            .child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(
                div()
                    .mt_1()
                    .max_w(px(384.))
                    .text_sm()
                    .text_center()
                    .text_color(muted)
                    .child(description),
            ),
    )
}

// The addons as v5's responsive grid of stretch rows.
fn rows(
    addons: &[&AddonEntry],
    columns: usize,
    state: &CardState,
    cx: &mut Context<AddonsPage>,
) -> AnyElement {
    let cards = addons
        .iter()
        .map(|addon| addon_card(addon, state, cx))
        .collect();
    stretch_rows(cards, columns)
}

/// What every card's buttons depend on beyond the addon itself.
struct CardState {
    /// v5's `isAnyBusy`: every addon button rests.
    rest: bool,
    has_tf2: bool,
    /// The addon this page installs now; its footer reads "Installing...".
    installing: Option<AddonName>,
    updates: Entity<AddonUpdates>,
    downloads: Option<Entity<StoreDownloads>>,
}

impl CardState {
    // The busy footer's label for `name`: its reinstall or this page's install reads
    // "Installing...", its download (or a reinstall waiting on one) "Downloading...".
    fn busy_label(&self, name: &AddonName, cx: &App) -> Option<&'static str> {
        let updates = self.updates.read(cx);
        if self.installing.as_ref() == Some(name) || updates.is_installing(name) {
            return Some("Installing...");
        }
        let item = StoreItemId::Addon(name.clone());
        let downloading = self
            .downloads
            .as_ref()
            .is_some_and(|d| d.read(cx).is_downloading(StoreKind::Addons, &item));
        (downloading || updates.is_pending(name)).then_some("Downloading...")
    }
}

// v5's library AddonCard: name with a check when installed and current, the installed tag,
// the description, then the footer buttons `model::actions` picks for its state.
fn addon_card(addon: &AddonEntry, state: &CardState, cx: &mut Context<AddonsPage>) -> AnyElement {
    let (danger, primary, muted, theme_fg, border) = {
        let theme = cx.theme();
        (
            theme.danger,
            theme.primary,
            theme.muted_foreground,
            theme.foreground,
            theme.border,
        )
    };
    let name = addon.name();
    let display = addon.display_name().to_owned();
    let (installed, update) = (addon.is_installed(), addon.update_available());
    let footer = actions(installed, update);
    let beside_update = footer.contains(&AddonAction::Update);
    let disabled =
        |action: AddonAction| state.rest || (needs_tf2(action, installed) && !state.has_tf2);
    // Each handler gets its own copy of the addon's names, as the listener outlives this render.
    let on = |action: AddonAction| {
        let (name, display) = (name.clone(), display.clone());
        cx.listener(move |page: &mut AddonsPage, _, window, cx| {
            let (name, display) = (name.clone(), display.clone());
            match action {
                AddonAction::Install => page.install(name, display, window, cx),
                AddonAction::Uninstall => page.uninstall(name, display, window, cx),
                AddonAction::Delete => page.delete(name, display, window, cx),
                AddonAction::Update => page.update(name, display, installed, window, cx),
            }
        })
    };
    let busy_footer = state.busy_label(name, cx).map(|label| {
        Button::new(ids::addon_busy(name))
            .outline()
            .flex_1()
            .loading(true)
            .disabled(true)
            .label(label)
            .into_any_element()
    });
    let buttons: Vec<AnyElement> = match busy_footer {
        Some(button) => vec![button],
        None => footer
            .iter()
            .map(|&action| match action {
                AddonAction::Update => Button::new(ids::addon_update(name))
                    .primary()
                    .flex_1()
                    .icon(IconName::RefreshCw)
                    .label("Update")
                    .disabled(disabled(action))
                    .on_click(on(action))
                    .into_any_element(),
                AddonAction::Install => {
                    let button = Button::new(ids::addon_install(name))
                        .flex_1()
                        .icon(IconName::Download)
                        .label("Install")
                        .disabled(disabled(action))
                        .on_click(on(action));
                    // Install is the card's primary action unless Update takes that place.
                    if beside_update {
                        button.outline().into_any_element()
                    } else {
                        button.primary().into_any_element()
                    }
                }
                AddonAction::Uninstall => {
                    let button = Button::new(ids::addon_uninstall(name))
                        .outline()
                        .icon(IconName::Trash)
                        .disabled(disabled(action))
                        .on_click(on(action));
                    if beside_update {
                        button.into_any_element()
                    } else {
                        button.flex_1().label("Uninstall").into_any_element()
                    }
                }
                AddonAction::Delete => Button::new(ids::addon_delete(name))
                    .ghost()
                    .icon(Icon::new(IconName::Trash).text_color(danger))
                    .tooltip("Remove from library")
                    .disabled(disabled(action))
                    .on_click(on(action))
                    .into_any_element(),
            })
            .collect(),
    };
    let check = (installed && !update).then(|| {
        div()
            .id(ids::addon_check(name))
            .test_support()
            .flex_none()
            .child(
                Icon::new(IconName::CircleCheck)
                    .with_size(Size::Medium)
                    .text_color(primary),
            )
    });
    card_frame(cx)
        .id(ids::addon_card(name))
        .accessibility_id(ids::addon_card(name).to_string())
        .aria_label(addon.display_name().to_owned())
        .test_support()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .overflow_hidden()
        // v5's Card: py-6 gap-6, a pb-2 header (title, tag right), a pb-2 growing content.
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
                                .child(addon.display_name().to_owned()),
                        )
                        .children(check),
                )
                .child(div().flex_none().child(badge_of(
                    ids::addon_tag(name),
                    Tag::secondary().outline().text_color(theme_fg),
                    border,
                    addon.installed_tag().to_owned(),
                ))),
        )
        .child(
            div().flex_1().pb_2().child(
                div()
                    .id(ids::addon_description(name))
                    .test_support()
                    .text_sm()
                    .text_color(muted)
                    .child(addon.description().to_owned()),
            ),
        )
        .child(h_flex().w_full().gap_2().children(buttons))
        .into_any_element()
}
