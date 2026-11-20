//! The Profiles page: saved whole setups (HUD, sounds, crosshairs, graphics, add-ons), applied
//! in one go.

pub mod editor;
pub mod editor_model;
pub mod model;
pub mod probe;

use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _, Size, WindowExt as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext as _, AsyncApp, Context, Entity, EventEmitter,
    FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Render,
    ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _, Subscription,
    TestSupportExt as _, WeakEntity, Window, div, px,
};

use tf2tools_core::{
    AddonName, ApplyProfileError, ApplyProfileReport, ApplyStep, Choices, CrosshairRef,
    ExternalHud, LogLevel, MastercomfigStatus, MissingItem, MissingKind, NewLogEntry, Profile,
    ProfileError, ProfileId, StoreKind, Tf2Install,
};

use self::editor::ProfileEditor;
use self::editor_model::{
    Mode, Slot, SlotKind, Summary, SummaryRow, card_lines, left_line, next_profile_name, summary,
};
use self::model::ApplyOutcome;
use self::probe::Tf2Probe;
use super::addons;
use super::common::{ORANGE_500, SM_ICON_PX, card_frame, no_tf2_warning};
use super::graphics::jobs::{self, ModulesFailure};
use super::hud_store::bodies::{curated_body, gamebanana_body};
use crate::activity;
use crate::font_check::active_hud;
use crate::i18n::tr;
use crate::ids;
use crate::library_events::{LibraryChanged, LibraryEvents};
use crate::local_messages::{describe_apply_profile, describe_profile};
use crate::pages::huds::{card_images, top_rounded_picture};
use crate::remote_images::{ImageKey, RemoteImages};
use crate::services::{OpError, Services, Tf2State, run_blocking, tf2_state};
use crate::shell::CONTENT_PADDING;
use crate::shell::confirm::{Confirm, confirm};
use crate::shell::external_huds::{ExternalPlan, HudAction, ask_external};
use crate::shell::font_check::ask_about_fonts;
use crate::shell::route::{NavigateTo, Page};
use crate::shell::toast::{ToastKind, toast, toast_error};
use crate::store_detail::{DetailItem, open_detail};
use crate::widgets::card_grid::CardGrid;
use crate::widgets::dialog_frame::{DIALOG_WIDTH, DialogFrame};
use crate::widgets::scroll::{scroll_area, scroll_beside};

/// A card below its 16:9 picture: padding, the name, five slot lines, the buttons and the border
/// (12 + 20 + 10 + 5 x 20 + 4 x 6 + 10 + 36 + 14 + 2).
pub const CARD_BODY_HEIGHT: Pixels = px(228.);
// The mock's `minmax(250px, 1fr)` columns.
const MIN_CARD_WIDTH: Pixels = px(250.);
// The lead line's flex basis; narrower than this beside New profile, the button wraps under it.
const LEAD_BASIS: Pixels = px(280.);
const SLOT_ICON: Pixels = px(14.);
// The mock's card radius; the picture rounds its top corners to match, since gpui never
// clips a child to its parent's rounding.
const CARD_RADIUS: Pixels = px(12.);
// The Apply summary dialog and its rows' label column.
const SUMMARY_WIDTH: Pixels = px(520.);
const SUMMARY_LABEL_WIDTH: Pixels = px(96.);
// How much of the next card row the grid's floor shows, so a one-column window reads as a list.
const NEXT_ROW_PEEK: Pixels = px(48.);

// A card's height at cell width `w`: the picture plus the fixed body.
fn card_height(w: Pixels) -> Pixels {
    w * (9. / 16.) + CARD_BODY_HEIGHT
}

/// The editor's empty pickers ask for a store page by event.
impl EventEmitter<NavigateTo> for ProfilesPage {}

// What the page shows: its grid of profiles, or one profile open in the editor (with its id,
// readable while the editor itself is being updated).
enum View {
    Grid,
    Editor(Entity<ProfileEditor>, ProfileId),
}

/// The Profiles page. Built fresh on each visit; it reloads on TF2 state and library changes.
pub struct ProfilesPage {
    win: AnyWindowHandle,
    profiles: Rc<[Profile]>,
    active: Option<ProfileId>,
    loaded: bool,
    /// Bumped by each reload, so a slower, older load never overwrites a newer one.
    load_generation: u64,
    /// Card picture URLs by HUD library folder, from the last load.
    hud_images: Rc<HashMap<String, String>>,
    /// Add-on display names by asset name, from the last load.
    addon_names: Rc<HashMap<String, String>>,
    /// The cards the grid last drew, whose pictures the page asks for.
    visible: Range<usize>,
    image_viewer: String,
    busy: bool,
    activating: Option<ProfileId>,
    view: View,
    grid: CardGrid,
    /// The page's own scroll, which moves only in a window too short for the page.
    scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl ProfilesPage {
    /// Needs the `Services` and `Tf2Global` globals; starts the first load off the UI thread.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let tf2 = cx.observe(&tf2_state(cx), |page, _, cx| page.reload(cx));
        let events = LibraryEvents::global(cx);
        let library = cx.subscribe(&events, |page, _, _: &LibraryChanged, cx| page.reload(cx));
        let mut subscriptions = vec![tf2, library];
        // HUD pictures redraw the cards as they land; the page's wants go when it does.
        let image_viewer = format!("profiles-page:{}", cx.entity_id());
        if let Some(images) = RemoteImages::global(cx) {
            subscriptions.push(cx.observe(&images, |_, _, cx| cx.notify()));
            let released = image_viewer.clone();
            subscriptions.push(cx.on_release(move |_, cx| {
                images.update(cx, |images, cx| images.want(&released, Vec::new(), cx));
            }));
        }
        let mut grid = CardGrid::new(ids::PROFILES_GRID).with_min_card_width(MIN_CARD_WIDTH);
        let this = cx.entity().downgrade();
        grid.on_visible(move |range, cx| {
            let _ = this.update(cx, |page, cx| {
                page.visible = range;
                page.want_images(cx);
            });
        });
        let mut page = Self {
            win: window.window_handle(),
            profiles: Rc::from([]),
            active: None,
            loaded: false,
            load_generation: 0,
            hud_images: Rc::default(),
            addon_names: Rc::default(),
            visible: 0..0,
            image_viewer,
            busy: false,
            activating: None,
            view: View::Grid,
            grid,
            scroll: ScrollHandle::new(),
            _subscriptions: subscriptions,
        };
        page.reload(cx);
        page
    }

    /// True once a load has ended, failed or not.
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// Every profile from the last load, in `profiles()` order.
    pub fn profiles(&self) -> &[Profile] {
        &self.profiles
    }

    /// The active profile from the last load.
    pub fn active(&self) -> Option<&ProfileId> {
        self.active.as_ref()
    }

    /// The page's own scroll handle, which moves the lead row and the grid together.
    pub fn page_scroll(&self) -> ScrollHandle {
        self.scroll.clone()
    }

    /// The profile grid's scroll handle; its offset is what the grid drew.
    pub fn grid_scroll(&self) -> ScrollHandle {
        self.grid.scroll_handle().0.borrow().base_handle.clone()
    }

    /// The name of the loaded profile, other than `except`, that already has `name` (any case).
    pub fn taken_name(&self, name: &str, except: Option<&ProfileId>) -> Option<String> {
        model::taken_by(&self.profiles, name, except).map(|p| p.name().to_owned())
    }

    /// True while a create, Delete or an activate runs.
    pub fn is_busy(&self) -> bool {
        self.busy || self.activating.is_some()
    }

    /// The profile whose activate flow is running, from the click until it ends.
    pub fn activating(&self) -> Option<&ProfileId> {
        self.activating.as_ref()
    }

    /// Shows the editor for `id` in place of the grid; does nothing while `is_busy()` or for an
    /// id the last load lacks.
    pub fn open_editor(&mut self, id: ProfileId, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        let Some(profile) = self.profiles.iter().find(|p| p.id() == &id).cloned() else {
            return;
        };
        self.show_editor(profile, window, cx);
    }

    /// Opens the editor for `id` on its Crosshairs slot with `crosshair` picked, saving nothing;
    /// a profile that can't be read toasts its error and leaves the grid.
    // Reads the one profile at once rather than waiting for the page's first load.
    pub fn open_with_crosshair(
        &mut self,
        id: ProfileId,
        crosshair: CrosshairRef,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let profile = match cx.global::<Services>().library.profile(&id) {
            Ok(profile) => profile,
            Err(e) => {
                let err = describe_profile(&e).into_op(tr!("messages.action.open_profile"));
                return toast_error(window, cx, &err);
            }
        };
        self.show_editor(profile, window, cx);
        if let Some(editor) = self.editor() {
            editor.update(cx, |editor, cx| {
                editor.select(Slot::Crosshairs, cx);
                let picker = editor.picker().clone();
                picker.update(cx, |picker, cx| picker.select(Some(crosshair), cx));
            });
        }
    }

    // Opens the editor on a profile already in hand, such as one just created.
    fn show_editor(&mut self, profile: Profile, window: &mut Window, cx: &mut Context<Self>) {
        let (page, active, id) = (
            cx.entity().downgrade(),
            self.active.clone(),
            profile.id().clone(),
        );
        let editor = cx.new(|cx| ProfileEditor::new(profile, page, active, window, cx));
        self.view = View::Editor(editor, id);
        cx.notify();
    }

    /// The open editor, while the page shows one.
    pub fn editor(&self) -> Option<Entity<ProfileEditor>> {
        match &self.view {
            View::Editor(editor, _) => Some(editor.clone()),
            View::Grid => None,
        }
    }

    /// Back to the grid; reloads.
    pub fn close_editor(&mut self, cx: &mut Context<Self>) {
        self.view = View::Grid;
        self.reload(cx);
    }

    /// Starts the activate flow for `id` unless a save, delete or activate runs; does nothing
    /// without TF2.
    pub fn activate(&mut self, id: ProfileId, _window: &mut Window, cx: &mut Context<Self>) {
        self.start_activate(id, true, cx);
    }

    /// `activate` with its Apply summary already answered yes; the timing harness drives
    /// applies this way, since no one is there to confirm.
    pub fn activate_confirmed(&mut self, id: ProfileId, cx: &mut Context<Self>) {
        self.start_activate(id, false, cx);
    }

    // One future per click chains the Apply summary, the TF2-running confirm and the apply;
    // `activating` is cleared once the chain ends, at whichever step it stopped.
    fn start_activate(&mut self, id: ProfileId, ask: bool, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        let Some(tf2) = tf2_state(cx).read(cx).install().cloned() else {
            return;
        };
        let Some(name) = self.profiles.iter().find(|p| p.id() == &id) else {
            return;
        };
        let name = name.name().to_owned();
        self.activating = Some(id.clone());
        cx.notify();
        let reading = ask.then(|| {
            let (id, tf2) = (id.clone(), tf2.clone());
            run_blocking(cx, move |s| summary_job(s, &id, &tf2))
        });
        let probe = Tf2Probe::get(cx);
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let flow = async {
                // The summary asks before the TF2 probe, so a cancelled summary never probes.
                if let Some(reading) = reading {
                    let answer = match reading.await {
                        Ok((name, summary)) => cx.update(|cx| {
                            win.update(cx, |_, window, cx| {
                                apply_summary_dialog(&name, &summary, window, cx)
                            })
                        }),
                        Err(err) => {
                            let _ = cx.update(|cx| {
                                win.update(cx, |_, window, cx| toast_error(window, cx, &err))
                            });
                            return;
                        }
                    };
                    let Ok(answer) = answer else {
                        return;
                    };
                    if !answer.recv().await.unwrap_or(false) {
                        return;
                    }
                }
                if cx.background_spawn(async move { probe() }).await {
                    let Ok(answer) = cx.update(|cx| {
                        win.update(cx, |_, window, cx| {
                            confirm(Confirm::tf2_running(), window, cx)
                        })
                    }) else {
                        return;
                    };
                    if !answer.recv().await.unwrap_or(false) {
                        return;
                    }
                }
                // A missing item asks first; "Install Anyway" re-runs the apply skipping it,
                // without the earlier confirms.
                let mut skip_missing = false;
                let mut plan = ExternalPlan::default();
                loop {
                    let (id, applying_tf2, applying_plan) = (id.clone(), tf2.clone(), plan.clone());
                    let applying = cx.update(|cx| {
                        run_blocking(cx, move |s| {
                            apply_job(s, &id, &applying_tf2, skip_missing, &applying_plan)
                        })
                    });
                    let done = applying.await;
                    if !skip_missing && let Ok(Activation::Missing { name: shown, items }) = &done {
                        let Ok(answer) = cx.update(|cx| {
                            win.update(cx, |_, window, cx| {
                                missing_items_dialog(shown, items, window, cx)
                            })
                        }) else {
                            return;
                        };
                        if answer.recv().await != Ok(MissingAnswer::InstallAnyway) {
                            return;
                        }
                        skip_missing = true;
                        continue;
                    }
                    // HUDs the library cannot put back ask next, and the re-run does as told.
                    if let Ok(Activation::External(huds)) = &done {
                        let (action, huds) = (HudAction::ActivateProfile, huds.clone());
                        let Some(next) = ask_external(action, huds, win, cx).await else {
                            return;
                        };
                        plan = next;
                        continue;
                    }
                    // The modules phase runs inside the flow, so `activating` still holds it alone.
                    let tf2 = tf2.clone();
                    match done {
                        Ok(Activation::Applied {
                            report,
                            hud_folder,
                            note,
                        }) => {
                            let outcome = finish_apply(&report, note, &tf2, cx).await;
                            let _ = cx.update(|cx| {
                                win.update(cx, |_, window, cx| {
                                    announce(&outcome, hud_folder, tf2, window, cx)
                                })
                            });
                        }
                        refused => {
                            let _ = cx.update(|cx| {
                                win.update(cx, |_, window, cx| {
                                    announce_refusal(refused, &name, window, cx)
                                })
                            });
                        }
                    }
                    return;
                }
            };
            flow.await;
            let _ = page.update(cx, |page, cx| {
                page.activating = None;
                cx.notify();
            });
        })
        .detach();
    }

    /// Loads `profiles()` and `active_profile()` in one `run_blocking` job.
    // A failed load toasts through the stored window handle, so a page dropped mid-load is harmless.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.load_generation += 1;
        let generation = self.load_generation;
        let open_at_start = match &self.view {
            View::Editor(_, id) => Some(id.clone()),
            View::Grid => None,
        };
        let loading = run_blocking(cx, |s| {
            let failed = |e: ProfileError| {
                describe_profile(&e).into_op(tr!("messages.action.load_profiles"))
            };
            let profiles = s.library.profiles().map_err(failed)?;
            let active = s.library.active_profile().map_err(failed)?;
            // Pictures and add-on names only dress the cards, so a failed read leaves them out.
            let images = s
                .library
                .list_huds()
                .map(|huds| card_images(s, &huds))
                .unwrap_or_default();
            let names = s.library.list_addons(None).map_or_else(
                |_| HashMap::new(),
                |addons| {
                    addons
                        .iter()
                        .map(|a| (a.name().as_str().to_owned(), a.display_name().to_owned()))
                        .collect()
                },
            );
            Ok((profiles, active, images, names))
        });
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let loaded = loading.await;
            let current = page
                .update(cx, |page, cx| {
                    if page.load_generation != generation {
                        return false;
                    }
                    if let Ok((profiles, active, images, names)) = &loaded {
                        page.profiles = profiles.as_slice().into();
                        page.active = active.clone();
                        page.hud_images = Rc::new(images.clone());
                        page.addon_names = Rc::new(names.clone());
                        page.want_images(cx);
                        // A profile deleted here or elsewhere takes its open editor with it.
                        if let View::Editor(_, open) = &page.view
                            && model::load_closes_editor(
                                open_at_start.as_ref(),
                                open,
                                profiles.iter().map(Profile::id),
                            )
                        {
                            page.view = View::Grid;
                        }
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

    // Runs `op` off the UI thread with `busy` set, toasts its texts or its error through the
    // stored window handle, then reloads; Delete touches no other page.
    fn run(
        &mut self,
        op: impl FnOnce(&Services) -> Result<(SharedString, SharedString), OpError> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        self.busy = true;
        cx.notify();
        let running = run_blocking(cx, op);
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let done = running.await;
            let _ = cx.update(|cx| {
                win.update(cx, |_, window, cx| match done {
                    Ok((title, description)) => {
                        toast(window, cx, ToastKind::Success, title, description)
                    }
                    Err(err) => toast_error(window, cx, &err),
                })
            });
            let _ = page.update(cx, |page, cx| {
                page.busy = false;
                page.reload(cx);
            });
        })
        .detach();
    }

    /// Saves what is installed as "Profile N", writes the create row, toasts, and opens the
    /// editor on it with the name focused and selected. A NameTaken from a race retries once
    /// with the next free name. Does nothing while busy or without TF2.
    pub fn new_profile(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        let Some(tf2) = tf2_state(cx).read(cx).install().cloned() else {
            return;
        };
        let name = next_profile_name(&self.profiles);
        self.busy = true;
        cx.notify();
        let creating = run_blocking(cx, move |s| {
            let create = |name: &str| s.library.save_current_profile(name, &tf2);
            let failed = |e: ProfileError| {
                describe_profile(&e).into_op(tr!("messages.action.create_profile"))
            };
            let profile = match create(&name) {
                Err(ProfileError::NameTaken { .. }) => {
                    let all = s.library.profiles().map_err(failed)?;
                    create(&next_profile_name(&all))
                }
                made => made,
            }
            .map_err(failed)?;
            let message = tr!("profiles.activity.saved_current", name = profile.name());
            let entry = NewLogEntry::new(LogLevel::Info, "profile", "save_current", message);
            activity::write(s, &entry);
            Ok(profile)
        });
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let done = creating.await;
            let _ = cx.update(|cx| {
                win.update(cx, |_, window, cx| {
                    let profile = match done {
                        Ok(profile) => profile,
                        Err(err) => return toast_error(window, cx, &err),
                    };
                    toast(
                        window,
                        cx,
                        ToastKind::Success,
                        tr!("profiles.toast.created_from_setup"),
                        SharedString::default(),
                    );
                    // The reload follows the editor in this same update, so no load that
                    // started before the create can land after it as the newest.
                    let _ = page.update(cx, |page, cx| {
                        page.busy = false;
                        page.show_editor(profile, window, cx);
                        page.reload(cx);
                        // Deferred so the click that started the create cannot take focus back.
                        if let Some(editor) = page.editor() {
                            window.defer(cx, move |window, cx| {
                                editor.update(cx, |e, cx| e.focus_name(window, cx));
                            });
                        }
                    });
                })
            });
            // A failed create, or a window gone meanwhile, still frees the page.
            let _ = page.update(cx, |page, cx| {
                if page.busy {
                    page.busy = false;
                    page.reload(cx);
                }
            });
        })
        .detach();
    }

    /// Opens v5's delete confirm for `id`, then removes it on yes; does nothing while `is_busy()`.
    pub fn delete(
        &mut self,
        id: ProfileId,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_busy() {
            return;
        }
        let answer = confirm(Confirm::delete_profile(name), window, cx);
        let message = tr!("profiles.activity.deleted", name = name).to_string();
        cx.spawn(async move |page, cx| {
            if !answer.recv().await.unwrap_or(false) {
                return;
            }
            let _ = page.update(cx, |page, cx| {
                page.run(
                    move |s| {
                        s.library.delete_profile(&id).map_err(|e| {
                            describe_profile(&e).into_op(tr!("messages.action.delete_failed"))
                        })?;
                        // The id is internal; the player's row names the profile instead.
                        let entry = NewLogEntry::new(LogLevel::Info, "profile", "delete", message);
                        activity::write(s, &entry);
                        Ok((tr!("profiles.toast.deleted"), SharedString::default()))
                    },
                    cx,
                );
            });
        })
        .detach();
    }

    // Asks for the HUD pictures of the cards in the grid's last drawn range.
    fn want_images(&mut self, cx: &mut Context<Self>) {
        let Some(images) = RemoteImages::global(cx) else {
            return;
        };
        let end = self.visible.end.min(self.profiles.len());
        let keys: Vec<ImageKey> = self.profiles[self.visible.start.min(end)..end]
            .iter()
            .filter_map(|p| self.hud_images.get(model::hud_item(p)?.folder_name()))
            .map(|url| ImageKey::card(url))
            .collect();
        images.update(cx, |images, cx| images.want(&self.image_viewer, keys, cx));
    }
}

// The activate job's result: `OpError` cannot carry the missing items, so `Missing` is a value.
enum Activation {
    Applied {
        // Boxed since the report carries a whole profile and dwarfs the other variants.
        report: Box<ApplyProfileReport>,
        hud_folder: Option<String>,
        /// What the toast adds about the HUDs kept or removed on the way.
        note: Option<String>,
    },
    Missing {
        name: String,
        items: Vec<MissingItem>,
    },
    External(Vec<ExternalHud>),
}

// Reads the profile, what is installed now and the add-on display names, and words the
// summary; add-on names only dress the rows, so a failed list falls back to asset names.
fn summary_job(
    s: &Services,
    id: &ProfileId,
    tf2: &Tf2Install,
) -> Result<(String, Summary), OpError> {
    let failed =
        |e: ProfileError| describe_profile(&e).into_op(tr!("messages.action.activate_profile"));
    let profile = s.library.profile(id).map_err(failed)?;
    let current = s.library.current_setup(tf2).map_err(failed)?;
    let names: HashMap<AddonName, String> = s
        .library
        .list_addons(None)
        .map(|addons| {
            addons
                .iter()
                .map(|a| (a.name().clone(), a.display_name().to_owned()))
                .collect()
        })
        .unwrap_or_default();
    let addon_name = |name: &AddonName| {
        names
            .get(name)
            .cloned()
            .unwrap_or_else(|| name.as_str().to_owned())
    };
    let summary = summary(&profile, &current, &addon_name);
    Ok((profile.name().to_owned(), summary))
}

// Opens the Apply summary over the page: what changes, or that nothing does, and the slots left
// alone. Only "Apply <name>" answers true; Cancel, Escape, Enter and closing answer false.
fn apply_summary_dialog(
    name: &str,
    summary: &Summary,
    window: &mut Window,
    cx: &mut App,
) -> async_channel::Receiver<bool> {
    let (tx, rx) = async_channel::bounded(1);
    let title: SharedString = tr!("profiles.apply.title", name = name);
    let ok_label: SharedString = tr!("profiles.apply.ok", name = name);
    let left: Option<SharedString> = left_line(&summary.left).map(Into::into);
    let rows: Rc<[SummaryRow]> = summary.rows.as_slice().into();
    window.defer(cx, move |window, cx| {
        let frame = DialogFrame::new(SUMMARY_WIDTH);
        window.open_dialog(cx, move |dialog, window, cx| {
            // The builder runs every frame, so each handler clones what it captures.
            let answer = |said: bool| {
                let tx = tx.clone();
                move |_: &_, _: &mut Window, _: &mut App| {
                    let _ = tx.try_send(said);
                    true
                }
            };
            let button = |id: &'static str, said: bool| {
                let tx = tx.clone();
                Button::new(id).on_click(move |_, window, cx| {
                    let _ = tx.try_send(said);
                    window.close_dialog(cx);
                })
            };
            let theme = cx.theme();
            let (muted, border) = (theme.muted_foreground, theme.border);
            let changes = if rows.is_empty() {
                let nothing: SharedString = tr!("profiles.apply.nothing");
                div()
                    .id(ids::PROFILE_APPLY_NOTHING)
                    .aria_label(nothing.clone())
                    .test_support()
                    .text_sm()
                    .child(nothing)
                    .into_any_element()
            } else {
                let last = rows.len() - 1;
                v_flex()
                    .id(ids::PROFILE_APPLY_ROWS)
                    .test_support()
                    .border_1()
                    .border_color(border)
                    .rounded_md()
                    .children(
                        rows.iter()
                            .enumerate()
                            .map(|(ix, row)| summary_row(row, ix < last, muted, border)),
                    )
                    .into_any_element()
            };
            let left = left.clone().map(|line| {
                div()
                    .id(ids::PROFILE_APPLY_LEFT)
                    .aria_label(line.clone())
                    .test_support()
                    .text_sm()
                    .text_color(muted)
                    .child(line)
            });
            frame
                .apply(dialog, &*window, &*cx)
                .on_ok(answer(false))
                .on_cancel(answer(false))
                .child(
                    frame.measure(
                        v_flex()
                            .id(ids::PROFILE_APPLY_DIALOG)
                            .test_support()
                            .gap_4()
                            .child(
                                v_flex()
                                    .gap_2()
                                    .child(
                                        div()
                                            .id(ids::PROFILE_APPLY_TITLE)
                                            .aria_label(title.clone())
                                            .test_support()
                                            .text_lg()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(title.clone()),
                                    )
                                    .child(
                                        div()
                                            .id(ids::PROFILE_APPLY_INTRO)
                                            .aria_label(tr!("profiles.apply.intro"))
                                            .test_support()
                                            .text_sm()
                                            .text_color(muted)
                                            .child(tr!("profiles.apply.intro")),
                                    ),
                            )
                            .child(changes)
                            .children(left)
                            .child(
                                h_flex()
                                    .justify_end()
                                    .gap_2()
                                    .child(
                                        button(ids::PROFILE_APPLY_CANCEL, false)
                                            .ghost()
                                            .label(tr!("confirm.cancel")),
                                    )
                                    .child(
                                        button(ids::PROFILE_APPLY_OK, true)
                                            .primary()
                                            .label(ok_label.clone()),
                                    ),
                            ),
                    ),
                )
        });
    });
    rx
}

// One summary row: the slot label in a fixed column, then "from → to"; a rule under all but the
// last row.
fn summary_row(
    row: &SummaryRow,
    rule: bool,
    muted: gpui_kit::Hsla,
    border: gpui_kit::Hsla,
) -> AnyElement {
    let change = format!("{} → {}", row.from, row.to);
    h_flex()
        .id(ids::profile_apply_row(row.key))
        .aria_label(format!("{}: {change}", row.label))
        .test_support()
        .gap_3()
        .px_3()
        .py_2()
        .text_sm()
        .when(rule, |r| r.border_b_1().border_color(border))
        .child(
            div()
                .w(SUMMARY_LABEL_WIDTH)
                .flex_none()
                .text_color(muted)
                .child(row.label.clone()),
        )
        .child(div().flex_1().min_w_0().child(change))
        .into_any_element()
}

fn apply_job(
    s: &Services,
    id: &ProfileId,
    tf2: &Tf2Install,
    skip_missing: bool,
    plan: &ExternalPlan,
) -> Result<Activation, OpError> {
    fn failed() -> SharedString {
        tr!("messages.action.activate_profile")
    }
    let profile = s
        .library
        .profile(id)
        .map_err(|e| describe_profile(&e).into_op(failed()))?;
    let discard = plan.prepare(s, tf2, failed())?;
    let applied = s
        .library
        .apply_profile_discarding(id, tf2, &s.builtins, skip_missing, discard);
    let report = match applied {
        Ok(report) => report,
        Err(ApplyProfileError::Missing(items)) => {
            return Ok(Activation::Missing {
                name: profile.name().to_owned(),
                items,
            });
        }
        Err(ApplyProfileError::ExternalHuds(huds)) => return Ok(Activation::External(huds)),
        Err(e) => return Err(describe_apply_profile(&e).into_op(failed())),
    };
    let hud_step_ran = report.applied().contains(&ApplyStep::Hud);
    if hud_step_ran {
        plan.log_discarded(s);
    }
    // An apply that put a HUD in place names its folder, so the font check can claim it at once.
    let hud_folder = (model::hud_item(&profile).is_some() && hud_step_ran)
        .then(|| active_hud(s, tf2))
        .flatten();
    Ok(Activation::Applied {
        report: Box::new(report),
        hud_folder,
        note: plan.note().filter(|_| hud_step_ran),
    })
}

// Finishes an applied report: Graphics' module choices, the active mark and the one activity
// row, all off the UI thread. Without a modules phase, core already marked it by the same rule.
async fn finish_apply(
    report: &ApplyProfileReport,
    note: Option<String>,
    tf2: &Tf2Install,
    cx: &mut AsyncApp,
) -> ApplyOutcome {
    let mut failed = report.failed().to_vec();
    let modules = report.pending_modules().cloned();
    if let Some(choices) = modules.clone()
        && let Some(reason) = modules_phase(choices, tf2.clone(), cx).await
    {
        failed.push((ApplyStep::Graphics, reason));
    }
    let clean =
        failed.is_empty() && report.skipped().is_empty() && report.skipped_steps().is_empty();
    let active = if modules.is_some() && clean {
        // An edit saved meanwhile moves `updated_at`, and then core leaves it inactive.
        let profile = report.profile().clone();
        let marking = cx.update(|cx| {
            run_blocking(cx, move |s| {
                Ok(matches!(s.library.mark_profile_active(&profile), Ok(true)))
            })
        });
        marking.await.unwrap_or(false)
    } else {
        clean && modules.is_none()
    };
    let movement = match report.movement().cloned() {
        Some(keys) => {
            let probe = cx.update(|cx| Tf2Probe::get(cx));
            let running = cx.background_spawn(async move { probe() }).await;
            Some(addons::movement_line(&keys, running))
        }
        None => (report.binds_restored() || report.movement_off())
            .then(|| addons::model::movement_off().to_string()),
    };
    let kinds: Vec<MissingKind> = report.skipped().iter().map(|m| m.kind().clone()).collect();
    let outcome = ApplyOutcome {
        name: report.profile().name().to_owned(),
        active,
        failed,
        skipped_steps: report.skipped_steps().to_vec(),
        kept: model::kept_parts(&kinds),
        blocked: report.hud_blocked_name().map(str::to_owned),
        movement,
        note,
    };
    let entry = activity_entry(report, &outcome);
    let writing = cx.update(|cx| {
        run_blocking(cx, move |s| {
            activity::write(s, &entry);
            Ok(())
        })
    });
    let _ = writing.await;
    outcome
}

// Applies `choices` as the Graphics page saves them, against the cached settings catalogue of
// the installed release; the reason it could not, if any.
async fn modules_phase(choices: Choices, tf2: Tf2Install, cx: &mut AsyncApp) -> Option<String> {
    let reading_tf2 = tf2.clone();
    let reading = cx.update(|cx| {
        run_blocking(cx, move |s| {
            Ok(match s.library.mastercomfig_status(&reading_tf2) {
                Ok(MastercomfigStatus::Managed { version, .. }) => {
                    s.store.cached_mastercomfig_catalog(&version)
                }
                _ => None,
            })
        })
    });
    let Some(catalog) = reading.await.ok().flatten() else {
        return Some(model::modules_no_catalog().to_string());
    };
    let applying =
        cx.update(|cx| jobs::apply_modules(tf2, Arc::new(catalog), choices, false, false, cx));
    match applying.await {
        Ok(_) => None,
        Err(ModulesFailure::Foreign) => Some(model::modules_foreign().to_string()),
        Err(ModulesFailure::Op(e)) => Some(e.description().to_string()),
    }
}

// The activate row: activated when the game now matches, applied when nothing failed, failed
// otherwise; its details name the final failed steps.
fn activity_entry(report: &ApplyProfileReport, outcome: &ApplyOutcome) -> NewLogEntry {
    let skipped: Vec<&str> = report.skipped().iter().map(MissingItem::name).collect();
    let details = model::report_details(report.applied(), &skipped, &outcome.failed);
    let name = outcome.name.as_str();
    let (level, message) = if outcome.active {
        (
            LogLevel::Info,
            tr!("profiles.activity.activated", name = name),
        )
    } else if outcome.failed.is_empty() {
        (
            LogLevel::Info,
            tr!("profiles.activity.applied", name = name),
        )
    } else {
        (
            LogLevel::Error,
            tr!("profiles.activity.failed", name = name),
        )
    };
    NewLogEntry::new(level, "profile", "activate", message).with_details(details)
}

// Toasts the outcome and tells every library page to reload, since tf/custom changed; an apply
// that put a HUD in place then checks that HUD's fonts.
fn announce(
    outcome: &ApplyOutcome,
    hud_folder: Option<String>,
    tf2: Tf2Install,
    window: &mut Window,
    cx: &mut App,
) {
    let (kind, title, description) = model::outcome_toast(outcome);
    toast(window, cx, kind, title, description);
    LibraryEvents::changed(cx);
    if let Some(folder) = hud_folder {
        ask_about_fonts(window.window_handle(), tf2, folder, cx);
    }
}

// A refusal or error that reached the end of the flow changed nothing; the missing items and
// unnamed HUDs are answered in the flow, so those only arrive here when that went wrong.
fn announce_refusal(
    done: Result<Activation, OpError>,
    name: &str,
    window: &mut Window,
    cx: &mut App,
) {
    match done {
        Err(err) => toast_error(window, cx, &err),
        Ok(_) => toast(
            window,
            cx,
            ToastKind::Error,
            tr!("profiles.toast.apply_failed", name = name),
            tr!("profiles.toast.missing"),
        ),
    }
}

// The Missing Items dialog's answer; Re-download opens the store detail and answers nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MissingAnswer {
    Cancel,
    InstallAnyway,
}

// Opens v5's Missing Items dialog over the page. Cancel, Escape and Enter answer `Cancel`; the
// receiver errs if the dialog goes away unanswered, which the flow also reads as a cancel.
fn missing_items_dialog(
    profile_name: &str,
    items: &[MissingItem],
    window: &mut Window,
    cx: &mut App,
) -> async_channel::Receiver<MissingAnswer> {
    let (tx, rx) = async_channel::bounded(1);
    let description: SharedString = tr!("profiles.missing.body", name = profile_name);
    let items: Rc<[MissingItem]> = items.into();
    // The list's scroll position outlives the builder, which runs every frame.
    let list_scroll = ScrollHandle::new();
    window.defer(cx, move |window, cx| {
        let frame = DialogFrame::new(DIALOG_WIDTH);
        window.open_dialog(cx, move |dialog, window, cx| {
            // The builder runs every frame, so each handler clones what it captures.
            let answer = |reply: MissingAnswer| {
                let tx = tx.clone();
                move |_: &_, _: &mut Window, _: &mut App| {
                    let _ = tx.try_send(reply);
                    true
                }
            };
            let button = |id: &'static str, reply: MissingAnswer| {
                let tx = tx.clone();
                Button::new(id).on_click(move |_, window, cx| {
                    let _ = tx.try_send(reply);
                    window.close_dialog(cx);
                })
            };
            let muted = cx.theme().muted_foreground;
            let rows = items
                .iter()
                .enumerate()
                .map(|(ix, item)| missing_row(ix, item, cx))
                .collect::<Vec<_>>();
            frame
                .apply(dialog, &*window, &*cx)
                .close_button(false)
                .on_ok(answer(MissingAnswer::Cancel))
                .on_cancel(answer(MissingAnswer::Cancel))
                .child(
                    frame.measure(
                        v_flex()
                            .id(ids::PROFILES_MISSING_DIALOG)
                            .test_support()
                            .gap_4()
                            // v5's dialog header: title and description 8 px apart.
                            .child(
                                v_flex()
                                    .gap_2()
                                    .child(
                                        div()
                                            .id(ids::PROFILES_MISSING_TITLE)
                                            .aria_label(tr!("profiles.missing.title"))
                                            .test_support()
                                            .text_lg()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(tr!("profiles.missing.title")),
                                    )
                                    .child(
                                        div()
                                            .id(ids::PROFILES_MISSING_DESCRIPTION)
                                            .aria_label(description.clone())
                                            .test_support()
                                            .text_sm()
                                            .text_color(muted)
                                            .child(description.clone()),
                                    ),
                            )
                            // The bar sits in the box padding, so the rows keep their width.
                            .child(scroll_beside(
                                ids::PROFILES_MISSING_LIST_SCROLLBAR,
                                &list_scroll,
                                v_flex()
                                    .id(ids::PROFILES_MISSING_LIST)
                                    .track_scroll(&list_scroll)
                                    .test_support()
                                    .gap_3()
                                    .max_h(px(256.))
                                    .overflow_y_scroll()
                                    .children(rows),
                            ))
                            .child(
                                h_flex()
                                    .justify_end()
                                    .gap_2()
                                    .child(
                                        button(ids::PROFILES_MISSING_CANCEL, MissingAnswer::Cancel)
                                            .outline()
                                            .label(tr!("confirm.cancel")),
                                    )
                                    .child(
                                        button(
                                            ids::PROFILES_MISSING_INSTALL,
                                            MissingAnswer::InstallAnyway,
                                        )
                                        .primary()
                                        .label(tr!("profiles.missing.install")),
                                    ),
                            ),
                    ),
                )
        });
    });
    rx
}

// One bordered Missing Items row: the warning, the name over its kind, and Re-download for a
// store item (the store detail stacks over this dialog) or v5's local-import note.
fn missing_row(ix: usize, item: &MissingItem, cx: &App) -> AnyElement {
    let orange: gpui_kit::Hsla = gpui_kit::rgb(ORANGE_500).into();
    let muted = cx.theme().muted_foreground;
    let kind = model::kind_label(item.kind());
    let action = match model::redownload_target(item.kind(), item.provenance()) {
        Some((kind, id)) => {
            let body = if kind == StoreKind::CuratedHuds {
                curated_body
            } else {
                gamebanana_body
            };
            let detail = DetailItem {
                kind,
                id,
                name: item.name().to_owned(),
                in_library: false,
            };
            Button::new(ids::missing_redownload(ix))
                .outline()
                .icon(IconName::Download)
                .label(tr!("profiles.missing.redownload"))
                .on_click(move |_, window, cx| {
                    open_detail(detail.clone(), body(), window, cx);
                })
                .into_any_element()
        }
        None => {
            let local = tr!("profiles.missing.local");
            div()
                .id(ids::missing_local(ix))
                .aria_label(local.clone())
                .test_support()
                .text_xs()
                .text_color(muted)
                .child(local)
                .into_any_element()
        }
    };
    h_flex()
        .id(ids::missing_item(ix))
        .aria_label(format!("{} ({kind})", item.name()))
        .test_support()
        .justify_between()
        .gap_2()
        .p_3()
        .rounded(cx.theme().radius_lg)
        .border_1()
        .border_color(cx.theme().border)
        .child(
            h_flex()
                .gap_2()
                .child(
                    Icon::new(IconName::TriangleAlert)
                        .with_size(Size::Size(px(16.)))
                        .text_color(orange),
                )
                .child(
                    v_flex()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .child(item.name().to_owned()),
                        )
                        .child(div().text_xs().text_color(muted).child(kind)),
                ),
        )
        .child(action)
        .into_any_element()
}

// v5's EmptyState inside a Card: an icon, the title and a muted description, plus any buttons.
fn empty_state(
    id: &'static str,
    icon: IconName,
    title: impl Into<SharedString>,
    description: impl Into<SharedString>,
    actions: Option<AnyElement>,
    cx: &App,
) -> AnyElement {
    let (title, description) = (title.into(), description.into());
    let muted = cx.theme().muted_foreground;
    card_frame(cx)
        .id(id)
        .aria_label(title.clone())
        .test_support()
        .child(
            v_flex()
                .items_center()
                .px_6()
                .py_8()
                .child(
                    Icon::new(icon)
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
                )
                .children(actions.map(|a| div().mt_4().child(a))),
        )
        .into_any_element()
}

// What every card's buttons wait on: a TF2 folder for Apply, and no other profile action.
#[derive(Clone, Copy)]
struct Gates {
    has_tf2: bool,
    busy: bool,
}

// What a card needs from the page besides its profile: the shared lookups and gates.
struct CardContext {
    hud_images: Rc<HashMap<String, String>>,
    addon_names: Rc<HashMap<String, String>>,
    gates: Gates,
    activating: Option<ProfileId>,
    page: WeakEntity<ProfilesPage>,
}

// The card's 16:9 picture: the HUD's own for a Set HUD, "TF2's own HUD" for None, and the
// bare fill for Leave or a HUD without a picture. The active card wears its pill on top.
fn card_picture(profile: &Profile, active: bool, ctx: &CardContext, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let id = profile.id();
    let picture = match profile.hud().as_set() {
        Some(hud) => {
            let key = ctx
                .hud_images
                .get(hud.folder_name())
                .map(|url| ImageKey::card(url));
            top_rounded_picture(
                ids::profile_card_picture(id),
                ids::profile_card_image(id),
                key.as_ref(),
                CARD_RADIUS,
                cx,
            )
            .into_any_element()
        }
        None => {
            let area = div()
                .id(ids::profile_card_picture(id))
                .test_support()
                .flex()
                .items_center()
                .justify_center()
                .w_full()
                .aspect_ratio(16. / 9.)
                .rounded_t(CARD_RADIUS)
                .bg(theme.muted);
            match Mode::of(profile.hud()) {
                Mode::None => {
                    let text = tr!("profiles.card.hud_none");
                    area.aria_label(text.clone())
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(text)
                        .into_any_element()
                }
                Mode::Set | Mode::Leave => area.into_any_element(),
            }
        }
    };
    // The pill is drawn light on dark, the theme's foreground under its background.
    let pill = active.then(|| {
        let text = tr!("profiles.badge.active");
        h_flex()
            .id(ids::profile_card_active(id))
            .aria_label(text.clone())
            .test_support()
            .absolute()
            .top_2()
            .left_2()
            .px_2()
            .py_0p5()
            .rounded_full()
            .bg(theme.foreground)
            .text_color(theme.background)
            .gap(px(6.))
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .child(
                Icon::new(IconName::Check)
                    .with_size(Size::Size(px(12.)))
                    .text_color(theme.background),
            )
            .child(text)
    });
    div()
        .relative()
        .flex_none()
        .w_full()
        .border_b_1()
        .border_color(theme.border)
        .child(picture)
        .children(pill)
        .into_any_element()
}

// One slot's line: its icon and the card text, muted and italic when the slot is left alone.
fn card_line(id: &ProfileId, slot: SlotKind, text: String, left: bool, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let icon = match slot {
        SlotKind::Hud => IconName::Paintbrush,
        SlotKind::Sounds => IconName::Volume2,
        SlotKind::Crosshairs => IconName::Crosshair,
        SlotKind::Graphics => IconName::Gauge,
        SlotKind::Addons => IconName::Puzzle,
    };
    h_flex()
        .id(ids::profile_card_line(id, slot.key()))
        .aria_label(text.clone())
        .test_support()
        .h(px(20.))
        .gap_2()
        .min_w_0()
        .text_size(px(13.))
        .when(left, |line| {
            line.italic().text_color(theme.muted_foreground)
        })
        .child(
            Icon::new(icon)
                .with_size(Size::Size(SLOT_ICON))
                .text_color(theme.muted_foreground),
        )
        .child(div().flex_1().min_w_0().truncate().child(text))
        .into_any_element()
}

// The mock's profile card: the picture, the name, a line per slot, then Apply (inactive cards
// only) and Edit. A click on the body opens the editor; the buttons stop their clicks first.
fn profile_card(profile: &Profile, active: bool, ctx: &CardContext, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let id = profile.id();
    let Gates { has_tf2, busy } = ctx.gates;
    let on_edit = {
        let (page, id) = (ctx.page.clone(), id.clone());
        move |_: &_, window: &mut Window, cx: &mut App| {
            cx.stop_propagation();
            let _ = page.update(cx, |page, cx| page.open_editor(id.clone(), window, cx));
        }
    };
    let apply = (!active).then(|| {
        let (page, id) = (ctx.page.clone(), id.clone());
        Button::new(ids::profile_card_apply(&id))
            .primary()
            .label(tr!("profiles.button.apply"))
            .loading(ctx.activating.as_ref() == Some(&id))
            .disabled(!has_tf2 || busy)
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                let _ = page.update(cx, |page, cx| page.activate(id.clone(), window, cx));
            })
    });
    let names = Rc::clone(&ctx.addon_names);
    let addon_name = move |name: &AddonName| {
        names
            .get(name.as_str())
            .cloned()
            .unwrap_or_else(|| name.as_str().to_owned())
    };
    let lines = card_lines(profile, &addon_name)
        .into_iter()
        .map(|line| card_line(id, line.slot, line.text, line.left, cx));
    let hover = crate::theme::card_hover(theme);
    card_frame(cx)
        .id(ids::profile_card(id))
        .accessibility_id(ids::profile_card(id).to_string())
        .aria_label(profile.name().to_owned())
        .test_support()
        .on_click(on_edit.clone())
        .cursor_pointer()
        .hover(move |card| card.bg(hover))
        .size_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .rounded(CARD_RADIUS)
        .border_color(if active {
            theme.foreground
        } else {
            theme.border
        })
        .child(card_picture(profile, active, ctx, cx))
        .child(
            v_flex()
                .flex_1()
                .min_h_0()
                .px_4()
                .pt_3()
                .pb(px(14.))
                .child(
                    div()
                        .id(ids::profile_card_name(id))
                        .aria_label(profile.name().to_owned())
                        .test_support()
                        .h(px(20.))
                        .truncate()
                        .text_size(px(15.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(profile.name().to_owned()),
                )
                .child(v_flex().mt(px(10.)).gap(px(6.)).children(lines))
                .child(
                    h_flex().mt_auto().gap_2().children(apply).child(
                        Button::new(ids::profile_card_edit(id))
                            .outline()
                            .label(tr!("profiles.button.edit"))
                            .disabled(busy)
                            .on_click(on_edit),
                    ),
                ),
        )
        .into_any_element()
}

impl ProfilesPage {
    // The lead line beside New profile, then the library box: the empty state or the grid.
    fn library(&self, has_tf2: bool, busy: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let body = if !self.loaded {
            None
        } else if self.profiles.is_empty() {
            Some(empty_state(
                ids::PROFILES_EMPTY,
                IconName::PackageOpen,
                tr!("profiles.empty.title"),
                tr!("profiles.empty.body"),
                None,
                cx,
            ))
        } else {
            let (profiles, active) = (Rc::clone(&self.profiles), self.active.clone());
            let ctx = CardContext {
                hud_images: Rc::clone(&self.hud_images),
                addon_names: Rc::clone(&self.addon_names),
                gates: Gates { has_tf2, busy },
                activating: self.activating.clone(),
                page: cx.entity().downgrade(),
            };
            Some(
                self.grid
                    .render_sized(profiles.len(), card_height, move |ix, _, cx| {
                        let profile = &profiles[ix];
                        let is_active = active.as_ref() == Some(profile.id());
                        profile_card(profile, is_active, &ctx, cx)
                    }),
            )
        };
        let lead = tr!("profiles.lead");
        v_flex()
            .flex_1()
            .gap_4()
            .child(
                h_flex()
                    .w_full()
                    .flex_wrap()
                    .items_center()
                    .gap_4()
                    .child(
                        div()
                            .id(ids::PROFILES_LEAD)
                            .aria_label(lead.clone())
                            .test_support()
                            .flex_1()
                            .flex_basis(LEAD_BASIS)
                            .min_w_0()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(lead),
                    )
                    .child(
                        Button::new(ids::PROFILES_NEW)
                            .primary()
                            .flex_none()
                            .h(px(36.))
                            .px(SM_ICON_PX)
                            .icon(IconName::Plus)
                            .label(tr!("profiles.button.new"))
                            .disabled(!has_tf2 || busy)
                            .when(!has_tf2, |b| b.tooltip(tr!("common.tf2_missing.title")))
                            .on_click(
                                cx.listener(|page, _, window, cx| page.new_profile(window, cx)),
                            ),
                    ),
            )
            .child(
                div()
                    .id(ids::PROFILES_LIBRARY)
                    .test_support()
                    .flex_1()
                    // One card row and the top of the next at least; a short window scrolls the page.
                    .min_h(self.grid.row_height_for(card_height) + NEXT_ROW_PEEK)
                    .children(body),
            )
    }
}

impl Render for ProfilesPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = tf2_state(cx).read(cx);
        let has_tf2 = state.install().is_some();
        let missing = matches!(state, Tf2State::Missing { .. });
        let busy = self.is_busy();
        let warning =
            missing.then(|| no_tf2_warning(ids::PROFILES_NO_TF2, tr!("profiles.no_tf2"), cx));
        let content = v_flex()
            .id(ids::page(Page::Profiles))
            .test_support()
            .size_full()
            .track_scroll(&self.scroll)
            .overflow_y_scroll()
            .p(CONTENT_PADDING)
            .gap_6()
            .children(warning);
        // The editor takes the place of the list.
        let content = if let View::Editor(editor, _) = &self.view {
            content.child(editor.clone())
        } else {
            content.child(self.library(has_tf2, busy, cx))
        };
        scroll_area(ids::page_scrollbar(Page::Profiles), &self.scroll, content).size_full()
    }
}
