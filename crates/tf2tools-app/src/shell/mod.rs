//! The window shell: sidebar, header and the routed page, plus the toasts and confirm dialog
//! every page shares.

pub mod confirm;
mod error_view;
pub mod external_huds;
pub mod font_check;
mod header;
pub mod leave;
pub mod root;
pub mod route;
mod sidebar;
pub mod toast;

pub use header::ring_icon;
pub use sidebar::icon as nav_icon;

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, Styled as _, Subscription, Window, div, px,
};

use tf2tools_core::{CrosshairId, CrosshairRef, HudEntry, HudId};

use crate::audio::player::show_idle;
use crate::keymap::ToggleSidebar;
use crate::local_messages::describe_settings;
use crate::pages::addon_store::AddonStorePage;
use crate::pages::addons::AddonsPage;
use crate::pages::crosshair_maker::CrosshairMakerPage;
use crate::pages::crosshair_store::CrosshairStorePage;
use crate::pages::crosshairs::{CopyToMaker, CrosshairsPage, EditPainted};
use crate::pages::graphics::GraphicsPage;
use crate::pages::hud_maker::HudMakerPage;
use crate::pages::hud_store::HudStorePage;
use crate::pages::huds::{EditInHudMaker, HudsPage};
use crate::pages::logs::LogsPage;
use crate::pages::logs_tf::LogsTfPage;
use crate::pages::maps::MapsPage;
use crate::pages::profiles::ProfilesPage;
use crate::pages::profiles::probe::Tf2Probe;
use crate::pages::settings::SettingsPage;
use crate::pages::sound_store::SoundStorePage;
use crate::pages::sounds::SoundsPage;
use crate::services::{recheck_tf2, run_blocking, tf2_state};
use crate::sync::{SyncEvent, store_name, sync_hub};
use crate::updater::{UpdateEvent, updater, view as update_view};
use route::{NavigateTo, Page};
use toast::ToastKind;

pub const SIDEBAR_WIDTH: Pixels = px(256.);
pub const SIDEBAR_COLLAPSED_WIDTH: Pixels = px(48.);
pub const HEADER_HEIGHT: Pixels = px(56.);
/// Each page root applies this padding on its own scrolling element, so the padding scrolls with the page.
pub const CONTENT_PADDING: Pixels = px(48.);

/// The main window's root view: which page shows and whether the sidebar is collapsed.
pub struct Shell {
    page: Page,
    collapsed: bool,
    focus: FocusHandle,
    huds: Option<Entity<HudsPage>>,
    /// The HUDs search the player left, which the rebuilt page starts from.
    huds_search: String,
    /// The HUD list startup read beside the services, kept for the first HUDs page only.
    preloaded_huds: Option<Vec<HudEntry>>,
    /// The hub observation and failure-toast subscription, while a hub is installed.
    _hub_subscriptions: Option<[Subscription; 2]>,
    /// The updater observation and failure-toast subscription, while an updater is installed.
    _update_subscriptions: Option<[Subscription; 2]>,
    /// The crash notice observation, while a notice is installed.
    _crash_subscription: Option<Subscription>,
    /// The TF2Tools 4 banner observation, while the notice is installed.
    _old_v4_subscription: Option<Subscription>,
    sounds: Option<Entity<SoundsPage>>,
    crosshairs: Option<Entity<CrosshairsPage>>,
    crosshair_maker: Option<Entity<CrosshairMakerPage>>,
    /// The painted crosshair the next maker opens, set by `edit_painted`.
    pending_edit: Option<CrosshairId>,
    /// The made HUD the next HUD Maker opens, set by `edit_hud`.
    pending_hud: Option<HudId>,
    /// The crosshair the next maker copies in as a new drawing, set by `copy_to_maker`.
    pending_copy: Option<CrosshairRef>,
    hud_maker: Option<Entity<HudMakerPage>>,
    settings: Option<Entity<SettingsPage>>,
    hud_store: Option<Entity<HudStorePage>>,
    sound_store: Option<Entity<SoundStorePage>>,
    addon_store: Option<Entity<AddonStorePage>>,
    crosshair_store: Option<Entity<CrosshairStorePage>>,
    logs: Option<Entity<LogsPage>>,
    maps: Option<Entity<MapsPage>>,
    logs_tf: Option<Entity<LogsTfPage>>,
    addons: Option<Entity<AddonsPage>>,
    graphics: Option<Entity<GraphicsPage>>,
    profiles: Option<Entity<ProfilesPage>>,
    /// Whether TF2 ran at the last check, for the header's Play TF2 button.
    tf2_running: bool,
}

impl Shell {
    /// Needs the `Services` and `Tf2Global` globals. Starts on `Page::Huds` and takes focus.
    pub fn new(collapsed: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::with_huds(collapsed, None, window, cx)
    }

    /// `new`, plus the HUD list startup read beside the services; the first HUDs page starts from it.
    pub fn with_huds(
        collapsed: bool,
        huds: Option<Vec<HudEntry>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        // The header badge reads the TF2 state, so the shell redraws whenever it changes.
        cx.observe(&tf2_state(cx), |_, _, cx| {
            Self::recheck_running(cx);
            cx.notify();
        })
        .detach();
        // A player back from moving the game folder, or from closing TF2, sees the header catch up.
        cx.observe_window_activation(window, |_, window, cx| {
            if window.is_window_active() {
                recheck_tf2(cx);
                Self::recheck_running(cx);
            }
        })
        .detach();
        Self::recheck_running(cx);
        // The header rings read the sync hub, and a failed sync toasts; no hub, no syncs.
        let hub_subscriptions = sync_hub(cx).map(|hub| {
            [
                cx.observe(&hub, |_, _, cx| cx.notify()),
                cx.subscribe_in(&hub, window, |_, _, event: &SyncEvent, window, cx| {
                    if let SyncEvent::Failed { kind, failure } = event {
                        toast::toast(
                            window,
                            cx,
                            ToastKind::Error,
                            format!("Failed to sync {}", store_name(*kind)),
                            failure.message.clone(),
                        );
                    }
                }),
            ]
        });
        // The banner reads the updater, and a download or install the player asked for toasts when it fails.
        let update_subscriptions = updater(cx).map(|u| {
            [
                cx.observe(&u, |_, _, cx| cx.notify()),
                cx.subscribe_in(&u, window, |_, _, event: &UpdateEvent, window, cx| {
                    let (UpdateEvent::DownloadFailed(failure)
                    | UpdateEvent::InstallFailed(failure)) = event;
                    toast::toast(
                        window,
                        cx,
                        ToastKind::Error,
                        "Update failed",
                        failure.message.clone(),
                    );
                }),
            ]
        });
        // The crash banner reads the notice, so dismissing it redraws the shell.
        let crash_subscription =
            crate::crash::notice::crash_notice(cx).map(|n| cx.observe(&n, |_, _, cx| cx.notify()));
        let old_v4_subscription =
            crate::old_v4::old_v4_notice(cx).map(|n| cx.observe(&n, |_, _, cx| cx.notify()));
        crate::timing::register_shell(cx.entity().downgrade(), cx);
        window.on_window_should_close(cx, leave::should_close);
        Self {
            page: Page::Huds,
            collapsed,
            focus,
            huds: None,
            huds_search: String::new(),
            preloaded_huds: huds,
            _hub_subscriptions: hub_subscriptions,
            _update_subscriptions: update_subscriptions,
            _crash_subscription: crash_subscription,
            _old_v4_subscription: old_v4_subscription,
            sounds: None,
            crosshairs: None,
            crosshair_maker: None,
            pending_edit: None,
            pending_hud: None,
            pending_copy: None,
            hud_maker: None,
            settings: None,
            hud_store: None,
            sound_store: None,
            addon_store: None,
            crosshair_store: None,
            logs: None,
            maps: None,
            logs_tf: None,
            addons: None,
            graphics: None,
            profiles: None,
            tf2_running: false,
        }
    }

    // Asks off the UI thread whether TF2 runs and redraws when the answer changed. Only a found
    // TF2 is asked about, as the Play button shows only then.
    fn recheck_running(cx: &mut Context<Self>) {
        if tf2_state(cx).read(cx).install().is_none() {
            return;
        }
        let probe = Tf2Probe::get(cx);
        cx.spawn(async move |shell, cx| {
            let running = cx.background_spawn(async move { probe() }).await;
            shell.update(cx, |shell, cx| {
                if shell.tf2_running != running {
                    shell.tf2_running = running;
                    cx.notify();
                }
            })
        })
        .detach();
    }

    pub fn page(&self) -> Page {
        self.page
    }

    // Leaving a library page or a maker drops it, so the next visit builds it fresh from the
    // saved state; only the HUDs search text is carried over. The stores, Maps, logs.tf, the Activity Log and Settings are kept, hidden, so a return finds
    // their tab, search, filters, sort, scroll and unsaved edits as left. Re-selecting the
    // current page keeps it as it is. A playing sound plays on; the idle player shows only on
    // the sound pages, and toasts rise above it while it shows.
    pub fn navigate(&mut self, page: Page, cx: &mut Context<Self>) {
        if page == self.page {
            return;
        }
        self.set_kept_hidden(self.page, true, cx);
        self.page = page;
        recheck_tf2(cx);
        Self::recheck_running(cx);
        leave::forget_maker(cx);
        if let Some(huds) = self.huds.take() {
            self.huds_search = huds.read(cx).search_input().read(cx).value().to_string();
        }
        self.sounds = None;
        self.crosshairs = None;
        self.crosshair_maker = None;
        self.hud_maker = None;
        self.addons = None;
        self.graphics = None;
        self.profiles = None;
        self.set_kept_hidden(page, false, cx);
        show_idle(page.shows_player(), cx);
        cx.notify();
    }

    // Tells a kept page, if built, that it leaves or comes back on screen.
    fn set_kept_hidden(&self, page: Page, hidden: bool, cx: &mut Context<Self>) {
        match page {
            Page::HudStore => self.hud_store.as_ref().map(|p| {
                p.update(cx, |p, cx| p.set_hidden(hidden, cx));
            }),
            Page::SoundStore => self.sound_store.as_ref().map(|p| {
                p.update(cx, |p, cx| p.set_hidden(hidden, cx));
            }),
            Page::CrosshairStore => self.crosshair_store.as_ref().map(|p| {
                p.update(cx, |p, cx| p.set_hidden(hidden, cx));
            }),
            Page::AddonStore => self.addon_store.as_ref().map(|p| {
                p.update(cx, |p, cx| p.set_hidden(hidden, cx));
            }),
            Page::Logs => self.logs.as_ref().map(|p| {
                p.update(cx, |p, cx| p.set_hidden(hidden, cx));
            }),
            Page::Maps => self.maps.as_ref().map(|p| {
                p.update(cx, |p, cx| p.set_hidden(hidden, cx));
            }),
            Page::LogsTf => self.logs_tf.as_ref().map(|p| {
                p.update(cx, |p, cx| p.set_hidden(hidden, cx));
            }),
            Page::Settings => self.settings.as_ref().map(|p| {
                p.update(cx, |p, cx| p.set_hidden(hidden, cx));
            }),
            Page::Huds
            | Page::Sounds
            | Page::Crosshairs
            | Page::CrosshairMaker
            | Page::HudMaker
            | Page::Profiles
            | Page::Addons
            | Page::Graphics => None,
        };
    }

    // A kept page, but only while it shows, as every other page's accessor reads.
    fn shown<T>(&self, page: Page, entity: Option<&Entity<T>>) -> Option<Entity<T>> {
        entity.cloned().filter(|_| self.page == page)
    }

    // The sidebar's way to another page: at once when no drawing would be lost, else after the
    // player chooses Discard. Re-selecting the current page never asks.
    fn request_navigate(&mut self, page: Page, window: &mut Window, cx: &mut Context<Self>) {
        if page == self.page {
            return;
        }
        if leave::can_leave(cx) {
            self.navigate(page, cx);
            return;
        }
        let shell = cx.entity().downgrade();
        leave::leave_then(window, cx, move |_, cx| {
            let _ = shell.update(cx, |shell, cx| shell.navigate(page, cx));
        });
    }

    /// Shows a fresh Crosshair Maker that opens painted crosshair `id` when it is built.
    pub fn edit_painted(&mut self, id: CrosshairId, cx: &mut Context<Self>) {
        self.navigate(Page::CrosshairMaker, cx);
        self.pending_edit = Some(id);
        cx.notify();
    }

    /// Shows a fresh HUD Maker that opens made HUD `id` when it is built.
    pub fn edit_hud(&mut self, id: HudId, cx: &mut Context<Self>) {
        self.navigate(Page::HudMaker, cx);
        self.pending_hud = Some(id);
        cx.notify();
    }

    /// Shows a fresh Crosshair Maker that copies `crosshair` in when it is built.
    pub fn copy_to_maker(&mut self, crosshair: CrosshairRef, cx: &mut Context<Self>) {
        self.navigate(Page::CrosshairMaker, cx);
        self.pending_copy = Some(crosshair);
        cx.notify();
    }

    /// The HUDs page while it shows.
    pub fn huds_page(&self) -> Option<Entity<HudsPage>> {
        self.huds.clone()
    }

    /// The Sounds page while it shows.
    pub fn sounds_page(&self) -> Option<Entity<SoundsPage>> {
        self.sounds.clone()
    }

    /// The Crosshairs page while it shows.
    pub fn crosshairs_page(&self) -> Option<Entity<CrosshairsPage>> {
        self.crosshairs.clone()
    }

    /// The Crosshair Maker page while it shows.
    pub fn crosshair_maker_page(&self) -> Option<Entity<CrosshairMakerPage>> {
        self.crosshair_maker.clone()
    }

    /// The HUD Maker page while it shows.
    pub fn hud_maker_page(&self) -> Option<Entity<HudMakerPage>> {
        self.hud_maker.clone()
    }

    /// The Settings page while it shows.
    pub fn settings_page(&self) -> Option<Entity<SettingsPage>> {
        self.shown(Page::Settings, self.settings.as_ref())
    }

    /// The HUD Store page while it shows.
    pub fn hud_store_page(&self) -> Option<Entity<HudStorePage>> {
        self.shown(Page::HudStore, self.hud_store.as_ref())
    }

    /// The Sound Store page while it shows.
    pub fn sound_store_page(&self) -> Option<Entity<SoundStorePage>> {
        self.shown(Page::SoundStore, self.sound_store.as_ref())
    }

    /// The Addon Store page while it shows.
    pub fn addon_store_page(&self) -> Option<Entity<AddonStorePage>> {
        self.shown(Page::AddonStore, self.addon_store.as_ref())
    }

    /// The Crosshair Store page while it shows.
    pub fn crosshair_store_page(&self) -> Option<Entity<CrosshairStorePage>> {
        self.shown(Page::CrosshairStore, self.crosshair_store.as_ref())
    }

    /// The Logs page while it shows.
    pub fn logs_page(&self) -> Option<Entity<LogsPage>> {
        self.shown(Page::Logs, self.logs.as_ref())
    }

    /// The Maps page while it shows.
    pub fn maps_page(&self) -> Option<Entity<MapsPage>> {
        self.shown(Page::Maps, self.maps.as_ref())
    }

    /// The logs.tf page while it shows.
    pub fn logs_tf_page(&self) -> Option<Entity<LogsTfPage>> {
        self.shown(Page::LogsTf, self.logs_tf.as_ref())
    }

    /// The Addons page while it shows.
    pub fn addons_page(&self) -> Option<Entity<AddonsPage>> {
        self.addons.clone()
    }

    /// The Graphics page while it shows.
    pub fn graphics_page(&self) -> Option<Entity<GraphicsPage>> {
        self.graphics.clone()
    }

    /// The Profiles page while it shows.
    pub fn profiles_page(&self) -> Option<Entity<ProfilesPage>> {
        self.profiles.clone()
    }

    pub fn collapsed(&self) -> bool {
        self.collapsed
    }

    // Flips the sidebar at once, then saves the choice off the UI thread; a failed save
    // toasts and leaves the sidebar as the user set it.
    fn toggle_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.collapsed = !self.collapsed;
        cx.notify();
        let collapsed = self.collapsed;
        let save = run_blocking(cx, move |s| {
            s.db.set_sidebar_collapsed(collapsed)
                .map_err(|e| describe_settings(&e).into_op("Save sidebar state"))
        });
        cx.spawn_in(window, async move |_, cx| {
            if let Err(err) = save.await {
                let _ = cx.update(|window, cx| toast::toast_error(window, cx, &err));
            }
        })
        .detach();
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (bg, fg) = (theme.background, theme.foreground);
        // The content area never scrolls; each page owns its scrolling, so grids get a bounded height.
        // Every page has its arm here; the page entity is built on first draw, since its
        // inputs need the window.
        let body = match self.page {
            // Edit in HUD Maker and the TF2 notice ask by event; subscriptions die with the page.
            Page::Huds => {
                let (preloaded, search) = (&mut self.preloaded_huds, &self.huds_search);
                self.huds
                    .get_or_insert_with(|| {
                        let page = cx.new(|cx| {
                            let mut page = match preloaded.take() {
                                Some(huds) => HudsPage::with_library(huds, window, cx),
                                None => HudsPage::new(window, cx),
                            };
                            page.set_search(search, window, cx);
                            page
                        });
                        cx.subscribe(&page, |shell, _, edit: &EditInHudMaker, cx| {
                            shell.edit_hud(edit.0.clone(), cx)
                        })
                        .detach();
                        cx.subscribe(&page, |shell, _, nav: &NavigateTo, cx| {
                            shell.navigate(nav.0, cx)
                        })
                        .detach();
                        page
                    })
                    .clone()
                    .into_any_element()
            }
            // Its TF2 notice asks for Settings by event, as the other pages here do.
            Page::Sounds => self
                .sounds
                .get_or_insert_with(|| {
                    let page = cx.new(|cx| SoundsPage::new(window, cx));
                    cx.subscribe(&page, |shell, _, nav: &NavigateTo, cx| {
                        shell.navigate(nav.0, cx)
                    })
                    .detach();
                    page
                })
                .clone()
                .into_any_element(),
            // Edit, Copy and the TF2 notice ask by event; the subscriptions die with the page.
            Page::Crosshairs => self
                .crosshairs
                .get_or_insert_with(|| {
                    let page = cx.new(|cx| CrosshairsPage::new(window, cx));
                    cx.subscribe(&page, |shell, _, edit: &EditPainted, cx| {
                        shell.edit_painted(edit.0.clone(), cx)
                    })
                    .detach();
                    cx.subscribe(&page, |shell, _, copy: &CopyToMaker, cx| {
                        shell.copy_to_maker(copy.0.clone(), cx)
                    })
                    .detach();
                    cx.subscribe(&page, |shell, _, nav: &NavigateTo, cx| {
                        shell.navigate(nav.0, cx)
                    })
                    .detach();
                    page
                })
                .clone()
                .into_any_element(),
            Page::CrosshairMaker => {
                let pending = self.pending_edit.take();
                let copy = self.pending_copy.take();
                self.crosshair_maker
                    .get_or_insert_with(|| {
                        let page = cx.new(|cx| {
                            let mut page = CrosshairMakerPage::new(window, cx);
                            if let Some(id) = pending {
                                page.open(id, cx);
                            }
                            if let Some(crosshair) = copy {
                                page.copy_from(crosshair, cx);
                            }
                            page
                        });
                        leave::register_crosshair_maker(page.downgrade(), cx);
                        page
                    })
                    .clone()
                    .into_any_element()
            }
            // Its Settings button asks for the Settings page by event, as Addons does below.
            Page::HudMaker => {
                let pending = self.pending_hud.take();
                self.hud_maker
                    .get_or_insert_with(|| {
                        let page = cx.new(|cx| {
                            let mut page = HudMakerPage::new(window, cx);
                            if let Some(id) = pending {
                                page.open(id, cx);
                            }
                            page
                        });
                        cx.subscribe(&page, |shell, _, nav: &NavigateTo, cx| {
                            shell.navigate(nav.0, cx)
                        })
                        .detach();
                        leave::register_hud_maker(page.downgrade(), cx);
                        page
                    })
                    .clone()
                    .into_any_element()
            }
            Page::Settings => self
                .settings
                .get_or_insert_with(|| cx.new(|cx| SettingsPage::new(window, cx)))
                .clone()
                .into_any_element(),
            Page::HudStore => self
                .hud_store
                .get_or_insert_with(|| cx.new(|cx| HudStorePage::new(window, cx)))
                .clone()
                .into_any_element(),
            Page::SoundStore => self
                .sound_store
                .get_or_insert_with(|| cx.new(|cx| SoundStorePage::new(window, cx)))
                .clone()
                .into_any_element(),
            // The info box's Addons link asks for the Addons page by event, as Addons does below.
            Page::AddonStore => self
                .addon_store
                .get_or_insert_with(|| {
                    let page = cx.new(|cx| AddonStorePage::new(window, cx));
                    cx.subscribe(&page, |shell, _, nav: &NavigateTo, cx| {
                        shell.navigate(nav.0, cx)
                    })
                    .detach();
                    page
                })
                .clone()
                .into_any_element(),
            Page::CrosshairStore => self
                .crosshair_store
                .get_or_insert_with(|| cx.new(|cx| CrosshairStorePage::new(window, cx)))
                .clone()
                .into_any_element(),
            Page::Logs => self
                .logs
                .get_or_insert_with(|| cx.new(|cx| LogsPage::new(window, cx)))
                .clone()
                .into_any_element(),
            Page::Maps => self
                .maps
                .get_or_insert_with(|| cx.new(|cx| MapsPage::new(window, cx)))
                .clone()
                .into_any_element(),
            Page::LogsTf => self
                .logs_tf
                .get_or_insert_with(|| cx.new(|cx| LogsTfPage::new(window, cx)))
                .clone()
                .into_any_element(),
            // The page asks to show another page by event; the subscription dies with the page.
            Page::Addons => self
                .addons
                .get_or_insert_with(|| {
                    let page = cx.new(|cx| AddonsPage::new(window, cx));
                    cx.subscribe(&page, |shell, _, nav: &NavigateTo, cx| {
                        shell.navigate(nav.0, cx)
                    })
                    .detach();
                    page
                })
                .clone()
                .into_any_element(),
            Page::Graphics => self
                .graphics
                .get_or_insert_with(|| cx.new(|cx| GraphicsPage::new(window, cx)))
                .clone()
                .into_any_element(),
            // The editor's empty pickers ask for a store by event.
            Page::Profiles => self
                .profiles
                .get_or_insert_with(|| {
                    let page = cx.new(|cx| ProfilesPage::new(window, cx));
                    cx.subscribe(&page, |shell, _, nav: &NavigateTo, cx| {
                        shell.navigate(nav.0, cx)
                    })
                    .detach();
                    page
                })
                .clone()
                .into_any_element(),
        };
        h_flex()
            .key_context("Shell")
            .track_focus(&self.focus)
            .on_action(
                cx.listener(|shell, _: &ToggleSidebar, window, cx| {
                    shell.toggle_sidebar(window, cx)
                }),
            )
            .size_full()
            .bg(bg)
            .text_color(fg)
            .child(sidebar::render(
                self.page,
                self.collapsed,
                window.viewport_size().height,
                cx,
            ))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(header::render(
                        self.page,
                        self.tf2_running,
                        window.viewport_size().width,
                        cx,
                    ))
                    .children(update_view::banner(cx))
                    .children(crate::crash::notice::banner(cx))
                    .children(crate::old_v4::banner(cx))
                    .child(div().flex_1().min_h_0().child(body)),
            )
    }
}
