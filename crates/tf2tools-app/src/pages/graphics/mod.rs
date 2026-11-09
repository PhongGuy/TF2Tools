//! The Graphics page: mastercomfig's status, the graphics preset picker and the settings list.

pub mod jobs;
pub mod model;
mod settings;

use std::collections::BTreeMap;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::select::SelectState;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AnyWindowHandle, AppContext as _, Context, Entity, FontWeight,
    InteractiveElement as _, IntoElement, MouseButton, ParentElement as _, Render, Role,
    ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _, Subscription,
    TestSupportExt as _, Window, div, px,
};
use tf2tools_core::{Choices, MastercomfigRelease, MastercomfigStatus, ModuleCatalog, Preset};

use crate::ids;
use crate::local_messages::describe_store_status;
use crate::pages::common::{card_frame, no_tf2_warning};
use crate::pages::profiles::probe::Tf2Probe;
use crate::services::{OpError, Services, Tf2State, require_tf2, run_blocking, tf2_state};
use crate::shell::CONTENT_PADDING;
use crate::shell::confirm::{Confirm, confirm};
use crate::shell::route::Page;
use crate::shell::toast::{ToastKind, toast, toast_error};
use crate::widgets::labelled;
use crate::widgets::scroll::scroll_area;
use model::{
    SettingsNote, StatusCard, ValueOption, describe_apply, describe_remove, describe_take_over,
    detail, headline, preset_description, preset_label, shown_choice, status_card, update_to,
    value_options,
};

const CREDIT: &str = "mastercomfig is made by the mastercomfig team and shared under the MIT licence. Preset descriptions come from comfig-app.";
const FOOTER: &str = "Changes apply the next time TF2 starts.";

/// A finished action's success toast: (title, description).
type Done = (SharedString, SharedString);

/// A settings row's value picker.
type ValueSelect = Entity<SelectState<Vec<ValueOption>>>;

/// What one load read off the UI thread.
struct Loaded {
    status: Option<MastercomfigStatus>,
    release: Option<MastercomfigRelease>,
    choices: Option<Choices>,
    /// The cached catalog of a managed version the page did not hold yet.
    catalog: Option<ModuleCatalog>,
}

/// The download job whose button spins; other actions only lock the buttons.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Job {
    Install,
    Check,
}

/// The Graphics page. Built fresh on each visit; it reloads when TF2's state changes.
pub struct GraphicsPage {
    win: AnyWindowHandle,
    /// mastercomfig in `tf/custom`, from the last load that knew TF2.
    status: Option<MastercomfigStatus>,
    /// The release the last addon-store sync recorded.
    release: Option<MastercomfigRelease>,
    loaded: bool,
    /// An install, update, take-over, removal or preset pick is running; every action rests.
    busy: bool,
    /// An action has ended and its reload is out; that reload's landing lifts `busy`.
    settling: bool,
    /// The job whose button spins, held as long as `busy`.
    running: Option<Job>,
    /// Bumped by each reload, so a slower, older load never overwrites a newer one.
    load_generation: u64,
    /// The saved choices of a managed install, from the last load.
    choices: Option<Choices>,
    /// The settings list's catalog, always for the managed install's version.
    catalog: Option<ModuleCatalog>,
    /// One Select per catalog module, built when the catalog arrives.
    selects: BTreeMap<String, ValueSelect>,
    /// Why there is no catalog yet; None before a fetch is asked.
    note: Option<SettingsNote>,
    /// The version whose catalog this visit already asked GitHub for.
    asked: Option<String>,
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
            busy: false,
            settling: false,
            running: None,
            load_generation: 0,
            choices: None,
            catalog: None,
            selects: BTreeMap::new(),
            note: None,
            asked: None,
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

    /// True while an action, or the reload after it, is running; every action button rests.
    pub fn is_busy(&self) -> bool {
        self.busy
    }

    /// True while an install or update runs, until its reload lands.
    pub fn is_installing(&self) -> bool {
        self.running == Some(Job::Install)
    }

    /// True while a check for updates runs, until its reload lands.
    pub fn is_checking(&self) -> bool {
        self.running == Some(Job::Check)
    }

    /// The status card from the last load; None without TF2 or before a load.
    pub fn card(&self) -> Option<StatusCard> {
        let latest = self.release.as_ref().map(MastercomfigRelease::tag);
        self.status
            .as_ref()
            .map(|status| status_card(status, latest))
    }

    /// The chosen preset of a TF2Tools-managed install; None for any other state.
    pub fn preset(&self) -> Option<Preset> {
        match self.status {
            Some(MastercomfigStatus::Managed { preset, .. }) => Some(preset),
            _ => None,
        }
    }

    /// The catalog of the managed install's version, once loaded.
    pub fn catalog(&self) -> Option<&ModuleCatalog> {
        self.catalog.as_ref()
    }

    /// The saved choices of a managed install.
    pub fn choices(&self) -> Option<&Choices> {
        self.choices.as_ref()
    }

    /// What shows in place of the list while there is no catalog; None with a catalog or unmanaged.
    pub fn settings_note(&self) -> Option<SettingsNote> {
        if self.catalog.is_some() || self.managed_version().is_none() {
            return None;
        }
        Some(self.note.unwrap_or(SettingsNote::Loading))
    }

    /// The Select state of `module`'s row.
    pub fn module_select(&self, module: &str) -> Option<ValueSelect> {
        self.selects.get(module).cloned()
    }

    fn managed_version(&self) -> Option<&str> {
        match &self.status {
            Some(MastercomfigStatus::Managed { version, .. }) => Some(version),
            _ => None,
        }
    }

    // Downloads the release into tf/custom through `jobs::install_base`; Update is the same
    // download over an older base, and core keeps the saved preset either way.
    fn install(&mut self, update: bool, window: &mut Window, cx: &mut Context<Self>) {
        let tf2 = match require_tf2(cx) {
            Ok(tf2) => tf2,
            Err(err) => return toast_error(window, cx, &err),
        };
        let action = if update {
            "Failed to update mastercomfig"
        } else {
            "Failed to install mastercomfig"
        };
        let job = jobs::install_base(tf2, action, cx);
        self.running = Some(Job::Install);
        self.finish(
            async move {
                let version = job.await?;
                Ok(if update {
                    (
                        "mastercomfig updated".into(),
                        format!("Now on {version}.").into(),
                    )
                } else {
                    let ready = format!("mastercomfig {version} is ready. Pick a preset below.");
                    ("mastercomfig installed".into(), ready.into())
                })
            },
            cx,
        );
    }

    // Forces the addon-store sync; the reloaded card then says "Up to date" or "Update to X",
    // so the toast names no version.
    fn check(&mut self, cx: &mut Context<Self>) {
        let job = jobs::check_for_updates(cx);
        self.running = Some(Job::Check);
        self.finish(
            async move {
                job.await?;
                Ok(("Checked for updates".into(), SharedString::default()))
            },
            cx,
        );
    }

    fn take_over(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tf2 = match require_tf2(cx) {
            Ok(tf2) => tf2,
            Err(err) => return toast_error(window, cx, &err),
        };
        self.run(
            move |s| {
                s.library.take_over_mastercomfig(&tf2).map_err(|e| {
                    describe_take_over(&e).into_op("Failed to take over mastercomfig")
                })?;
                let text = "TF2Tools now looks after mastercomfig.";
                Ok(("mastercomfig taken over".into(), text.into()))
            },
            cx,
        );
    }

    // Asks first; only a confirmed answer removes, and the TF2 folder is read once answered.
    fn remove(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let answer = confirm(Confirm::remove_mastercomfig(), window, cx);
        let win = self.win;
        cx.spawn(async move |page, cx| {
            if !answer.recv().await.unwrap_or(false) {
                return;
            }
            let _ = page.update(cx, |page, cx| {
                let tf2 = match require_tf2(cx) {
                    Ok(tf2) => tf2,
                    Err(err) => {
                        let _ = win.update(cx, |_, window, cx| toast_error(window, cx, &err));
                        return;
                    }
                };
                let probe = Tf2Probe::get(cx);
                page.run(
                    move |s| {
                        jobs::refuse_while_running(&*probe)?;
                        s.library.remove_mastercomfig(&tf2).map_err(|e| {
                            describe_remove(&e).into_op("Failed to remove mastercomfig")
                        })?;
                        let text = "mastercomfig is no longer in TF2.";
                        Ok(("mastercomfig removed".into(), text.into()))
                    },
                    cx,
                );
            });
        })
        .detach();
    }

    // Writes the picked preset at once; picking the current one again does nothing.
    fn pick(&mut self, preset: Preset, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.preset() == Some(preset) {
            return;
        }
        let tf2 = match require_tf2(cx) {
            Ok(tf2) => tf2,
            Err(err) => return toast_error(window, cx, &err),
        };
        self.run(
            move |s| {
                s.library
                    .apply_preset(&tf2, preset)
                    .map_err(|e| describe_apply(&e).into_op("Failed to apply the preset"))?;
                let text = format!("{} applies the next time TF2 starts.", preset_label(preset));
                Ok(("Preset applied".into(), text.into()))
            },
            cx,
        );
    }

    // Runs a blocking library change off the UI thread through `finish`.
    fn run(
        &mut self,
        op: impl FnOnce(&Services) -> Result<Done, OpError> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let running = run_blocking(cx, op);
        self.finish(running, cx);
    }

    // Marks the page busy until `job` ends, toasts its outcome, then reloads the status.
    fn finish(
        &mut self,
        job: impl std::future::Future<Output = Result<Done, OpError>> + 'static,
        cx: &mut Context<Self>,
    ) {
        self.busy = true;
        cx.notify();
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let done = job.await;
            let _ = cx.update(|cx| {
                win.update(cx, |_, window, cx| match done {
                    Ok((title, text)) => toast(window, cx, ToastKind::Success, title, text),
                    Err(err) => toast_error(window, cx, &err),
                })
            });
            // `busy` holds until the reload lands, so no button acts on the stale state.
            let _ = page.update(cx, |page, cx| {
                page.settling = true;
                page.reload(cx);
            });
        })
        .detach();
    }

    // Reads the install's status (only with TF2), the recorded release and, for a managed
    // install, its saved choices and any cached catalog the page lacks, in the background. A
    // failed read toasts through the window and leaves the last good values.
    fn reload(&mut self, cx: &mut Context<Self>) {
        self.load_generation += 1;
        let generation = self.load_generation;
        let tf2 = tf2_state(cx).read(cx).install().cloned();
        let held = self.catalog.as_ref().map(|c| c.tag().to_owned());
        let loading = run_blocking(cx, move |s| {
            let failed = "Failed to load mastercomfig";
            // Puts back a preset file another tool cleared; a failure only costs the restore.
            if let Some(tf2) = &tf2
                && let Err(e) = s.library.restore_mastercomfig_files(tf2)
            {
                tracing::warn!("mastercomfig restore: {e}");
            }
            let status = tf2
                .map(|tf2| s.library.mastercomfig_status(&tf2))
                .transpose()
                .map_err(|e| model::describe_status(&e).into_op(failed))?;
            let release = s
                .store
                .mastercomfig_release()
                .map_err(|e| describe_store_status(&e).into_op(failed))?;
            let (choices, catalog) = match &status {
                Some(MastercomfigStatus::Managed { version, .. }) => {
                    let choices = s
                        .library
                        .mastercomfig_choices()
                        .map_err(|e| model::describe_status(&e).into_op(failed))?;
                    let catalog = (held.as_deref() != Some(version.as_str()))
                        .then(|| s.store.cached_mastercomfig_catalog(version))
                        .flatten();
                    (choices, catalog)
                }
                _ => (None, None),
            };
            Ok(Loaded {
                status,
                release,
                choices,
                catalog,
            })
        });
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let loaded = loading.await;
            // Through the window, so the catalog and its Selects land in the same update.
            let current = cx
                .update(|cx| {
                    win.update(cx, |_, window, cx| {
                        page.update(cx, |page, cx| {
                            if page.load_generation != generation {
                                return false;
                            }
                            if let Ok(loaded) = &loaded {
                                page.status = loaded.status.clone();
                                page.release = loaded.release.clone();
                                page.choices = loaded.choices.clone();
                                page.sync_list(loaded.catalog.clone(), window, cx);
                            }
                            page.loaded = true;
                            if page.settling {
                                page.settling = false;
                                page.busy = false;
                                page.running = None;
                            }
                            cx.notify();
                            true
                        })
                    })
                })
                .ok()
                .and_then(Result::ok)
                .unwrap_or(false);
            if let (true, Err(err)) = (current, loaded) {
                let _ =
                    cx.update(|cx| win.update(cx, |_, window, cx| toast_error(window, cx, &err)));
            }
        })
        .detach();
    }

    // Keeps the list on the managed version's catalog: a version change swaps the catalog
    // (`fresh`, the cached one or None) and rebuilds the Selects; otherwise the Selects only
    // move to the shown values, so focus stays put. A missing catalog is fetched once.
    fn sync_list(
        &mut self,
        fresh: Option<ModuleCatalog>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let version = self.managed_version().map(str::to_owned);
        if self.catalog.as_ref().map(ModuleCatalog::tag) != version.as_deref() {
            self.catalog = fresh;
            self.build_selects(window, cx);
        } else {
            self.show_choices(window, cx);
        }
        if let Some(tag) = version
            && self.catalog.is_none()
            && self.asked.as_deref() != Some(tag.as_str())
        {
            self.fetch(tag, cx);
        }
    }

    // Asks the store for `tag`'s catalog; the answer lands only while `tag` is still managed.
    fn fetch(&mut self, tag: String, cx: &mut Context<Self>) {
        self.asked = Some(tag.clone());
        self.note = Some(SettingsNote::Loading);
        let job = jobs::fetch_catalog(tag.clone(), cx);
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let fetched = job.await;
            let _ = cx.update(|cx| {
                win.update(cx, |_, window, cx| {
                    page.update(cx, |page, cx| {
                        if page.managed_version() != Some(tag.as_str()) || page.catalog.is_some() {
                            return;
                        }
                        match fetched {
                            Ok(catalog) => {
                                page.catalog = Some(catalog);
                                page.build_selects(window, cx);
                            }
                            Err(note) => page.note = Some(note),
                        }
                        cx.notify();
                    })
                })
            });
        })
        .detach();
    }

    // One Select per module of the held catalog, each on its shown value.
    fn build_selects(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.selects.clear();
        let Some(catalog) = &self.catalog else {
            return;
        };
        let choices = self.shown_against();
        for module in catalog.categories().iter().flat_map(|c| c.modules()) {
            let shown = shown_choice(catalog, &choices, module.name());
            let options = value_options(module);
            let select = cx.new(|cx| {
                let mut select = SelectState::new(options, None, window, cx);
                select.set_selected_value(&shown, window, cx);
                select
            });
            self.selects.insert(module.name().to_owned(), select);
        }
    }

    // Moves each existing Select to its row's shown value.
    fn show_choices(&self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(catalog) = &self.catalog else {
            return;
        };
        let choices = self.shown_against();
        for (name, select) in &self.selects {
            let shown = shown_choice(catalog, &choices, name);
            select.update(cx, |select, cx| {
                select.set_selected_value(&shown, window, cx)
            });
        }
    }

    // The choices the rows show: the saved ones, else none on top of the current preset.
    fn shown_against(&self) -> Choices {
        self.choices
            .clone()
            .unwrap_or_else(|| Choices::new(self.preset().unwrap_or(Preset::Medium)))
    }
}

impl Render for GraphicsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
            ))
            .children(self.card().map(|card| self.status_card(&card, cx)))
            .children(
                self.preset()
                    .map(|current| self.presets(current, window, cx)),
            )
            .children(self.preset().map(|_| settings::section(self, cx)))
            .children(self.preset().map(|_| {
                labelled(
                    div().text_sm().text_color(cx.theme().muted_foreground),
                    ids::GRAPHICS_FOOTER,
                    FOOTER,
                )
            }));
        scroll_area(ids::page_scrollbar(Page::Graphics), &self.scroll, content).size_full()
    }
}

impl GraphicsPage {
    // The headline, its detail line, the buttons this state offers and the credit line.
    fn status_card(&self, card: &StatusCard, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let busy = self.busy;
        let installing = self.running == Some(Job::Install);
        let checking = self.running == Some(Job::Check);
        let mut buttons: Vec<AnyElement> = Vec::new();
        match card {
            StatusCard::NotInstalled => buttons.push(
                Button::new(ids::GRAPHICS_INSTALL)
                    .primary()
                    .icon(IconName::Download)
                    .label("Install")
                    .loading(installing)
                    .disabled(busy)
                    .on_click(cx.listener(|page, _, window, cx| page.install(false, window, cx)))
                    .into_any_element(),
            ),
            StatusCard::Installed { .. } => {
                if update_to(card).is_some() {
                    buttons.push(
                        Button::new(ids::GRAPHICS_UPDATE)
                            .primary()
                            .icon(IconName::RefreshCw)
                            .label("Update")
                            .loading(installing)
                            .disabled(busy)
                            .on_click(
                                cx.listener(|page, _, window, cx| page.install(true, window, cx)),
                            )
                            .into_any_element(),
                    );
                }
                buttons.push(
                    Button::new(ids::GRAPHICS_CHECK)
                        .outline()
                        .icon(IconName::RotateCw)
                        .label("Check for updates")
                        .loading(checking)
                        .disabled(busy)
                        .on_click(cx.listener(|page, _, _, cx| page.check(cx)))
                        .into_any_element(),
                );
                buttons.push(
                    Button::new(ids::GRAPHICS_REMOVE)
                        .outline()
                        .icon(IconName::Trash)
                        .label("Remove")
                        .disabled(busy)
                        .on_click(cx.listener(|page, _, window, cx| page.remove(window, cx)))
                        .into_any_element(),
                );
            }
            StatusCard::Outside { .. } => buttons.push(
                Button::new(ids::GRAPHICS_TAKE_OVER)
                    .primary()
                    .label("Take over")
                    .disabled(busy)
                    .on_click(cx.listener(|page, _, window, cx| page.take_over(window, cx)))
                    .into_any_element(),
            ),
        }
        let detail = detail(card).map(|text| {
            labelled(
                div().text_sm().text_color(muted),
                ids::GRAPHICS_DETAIL,
                text,
            )
        });
        card_frame(cx)
            .id(ids::GRAPHICS_STATUS)
            .test_support()
            .p_6()
            .child(
                v_flex()
                    .gap_4()
                    .child(
                        v_flex()
                            .gap_1()
                            .child(labelled(
                                div().text_lg().font_weight(FontWeight::SEMIBOLD),
                                ids::GRAPHICS_HEADLINE,
                                headline(card),
                            ))
                            .children(detail),
                    )
                    .child(h_flex().flex_wrap().gap_2().children(buttons))
                    .child(labelled(
                        div().text_xs().text_color(muted),
                        ids::GRAPHICS_CREDIT,
                        CREDIT,
                    )),
            )
            .into_any_element()
    }

    // The five preset cards in comfig-app's order, the current one in the primary border. Each
    // is a tab stop where Enter or Space picks it; a click does not take focus, as on the pills.
    fn presets(&self, current: Preset, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let (primary, border, ring, muted, radius) = (
            theme.primary,
            theme.border,
            theme.ring,
            theme.muted_foreground,
            theme.radius_lg,
        );
        let cards: Vec<AnyElement> = Preset::PICKER
            .into_iter()
            .map(|preset| {
                let chosen = preset == current;
                let id = ids::graphics_preset(preset);
                let focus = window
                    .use_keyed_state(id.clone(), cx, |_, cx| cx.focus_handle().tab_stop(true))
                    .read(cx)
                    .clone();
                let edge = if focus.is_focused(window) {
                    ring
                } else if chosen {
                    primary
                } else {
                    border
                };
                card_frame(cx)
                    .id(id)
                    .role(Role::Button)
                    .aria_label(preset_label(preset))
                    .aria_selected(chosen)
                    .test_support()
                    .track_focus(&focus)
                    .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                    .flex_1()
                    .min_w(px(160.))
                    .p_4()
                    .rounded(radius)
                    .border_color(edge)
                    .cursor_pointer()
                    .when(self.busy, |card| card.opacity(0.6))
                    .on_click(cx.listener(move |page, _, window, cx| page.pick(preset, window, cx)))
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(preset_label(preset)),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(muted)
                                    .child(preset_description(preset)),
                            ),
                    )
                    .into_any_element()
            })
            .collect();
        v_flex()
            .gap_3()
            .child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Preset"),
            )
            .child(h_flex().flex_wrap().gap_4().children(cards))
            .into_any_element()
    }
}
