//! The Profiles page: saved setups of HUD, sounds and crosshairs, activated in one go.

pub mod editor_model;
pub mod model;
pub mod probe;
pub mod wizard;
pub mod wizard_model;

use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _, Size, WindowExt as _, h_flex, v_flex,
};
use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext as _, Context, Entity, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Render, ScrollHandle,
    SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, TestSupportExt as _,
    WeakEntity, Window, div, px, transparent_black,
};

use tf2tools_core::{
    ApplyProfileError, ApplyProfileReport, ApplyStep, ExternalHud, LogLevel, MissingItem,
    MissingKind, NewLogEntry, Profile, ProfileError, ProfileId, StoreKind, Tf2Install,
};

use self::probe::Tf2Probe;
use self::wizard::{ProfileWizard, WizardMode};
use super::common::{ORANGE_500, SM_ICON_PX, badge_of, card_frame, no_tf2_warning};
use super::hud_store::bodies::{curated_body, gamebanana_body};
use crate::activity;
use crate::font_check::active_hud;
use crate::ids;
use crate::library_events::{LibraryChanged, LibraryEvents};
use crate::local_messages::{describe_apply_profile, describe_profile};
use crate::pages::huds::blocked_description;
use crate::services::{OpError, Services, Tf2State, run_blocking, tf2_state};
use crate::shell::CONTENT_PADDING;
use crate::shell::confirm::{Confirm, confirm};
use crate::shell::external_huds::{ExternalPlan, HudAction, ask_external};
use crate::shell::font_check::ask_about_fonts;
use crate::shell::route::Page;
use crate::shell::toast::{ToastKind, toast, toast_error};
use crate::store_detail::{DetailItem, open_detail};
use crate::widgets::Named as _;
use crate::widgets::card_grid::CardGrid;
use crate::widgets::dialog_frame::{DIALOG_WIDTH, DialogFrame};
use crate::widgets::labelled;
use crate::widgets::scroll::{scroll_area, scroll_beside};

/// A card's height: the name row, two wrapped rows of badges and the button footer.
const CARD_HEIGHT: Pixels = px(196.);
// The grid never gets shorter than one card row; a short window scrolls the page.
const LIBRARY_MIN_HEIGHT: Pixels = CARD_HEIGHT;
// Narrower than this, the search box wraps onto its own row instead of shrinking.
const SEARCH_MIN_WIDTH: Pixels = px(160.);
// v5's Save Current dialog description.
const SAVE_DESCRIPTION: &str =
    "Save your currently installed HUD, sounds, and crosshair assignments as a reusable profile.";

/// The Profiles page. Built fresh on each visit; it reloads on TF2 state and library changes.
pub struct ProfilesPage {
    win: AnyWindowHandle,
    profiles: Rc<[Profile]>,
    active: Option<ProfileId>,
    loaded: bool,
    /// Bumped by each reload, so a slower, older load never overwrites a newer one.
    load_generation: u64,
    search: Entity<InputState>,
    save_name: Entity<InputState>,
    busy: bool,
    activating: Option<ProfileId>,
    wizard: Option<WeakEntity<ProfileWizard>>,
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
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search profiles..."));
        // The grid filters from the input's value at render, so each edit just redraws.
        let search_changed = cx.subscribe(&search, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        });
        let save_name =
            cx.new(|cx| InputState::new(window, cx).placeholder("e.g., Competitive Setup"));
        let mut page = Self {
            win: window.window_handle(),
            profiles: Rc::from([]),
            active: None,
            loaded: false,
            load_generation: 0,
            search,
            save_name,
            busy: false,
            activating: None,
            wizard: None,
            grid: CardGrid::new(ids::PROFILES_GRID),
            scroll: ScrollHandle::new(),
            _subscriptions: vec![tf2, library, search_changed],
        };
        page.reload(cx);
        page
    }

    /// True once a load has ended, failed or not.
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// Every profile from the last load, in `profiles()` order (unfiltered).
    pub fn profiles(&self) -> &[Profile] {
        &self.profiles
    }

    /// The active profile from the last load.
    pub fn active(&self) -> Option<&ProfileId> {
        self.active.as_ref()
    }

    pub fn search_input(&self) -> &Entity<InputState> {
        &self.search
    }

    /// The page's own scroll handle, which moves the active card, search and grid together.
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

    /// The Save Current dialog's name field.
    pub fn save_name_input(&self) -> &Entity<InputState> {
        &self.save_name
    }

    /// True while Save Current, Delete or an activate runs.
    pub fn is_busy(&self) -> bool {
        self.busy || self.activating.is_some()
    }

    /// The profile whose activate flow is running, from the click until it ends.
    pub fn activating(&self) -> Option<&ProfileId> {
        self.activating.as_ref()
    }

    /// The wizard last opened, while it is alive.
    pub fn wizard(&self) -> Option<Entity<ProfileWizard>> {
        self.wizard.as_ref()?.upgrade()
    }

    /// Opens the profile wizard for `mode`; does nothing while `is_busy()`.
    pub fn open_wizard(&mut self, mode: WizardMode, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        let wizard = ProfileWizard::open(mode, cx.entity().downgrade(), window, cx);
        self.wizard = Some(wizard.downgrade());
    }

    /// Starts the activate flow for `id` unless a save, delete or activate runs; does nothing
    /// without TF2.
    // One future per click chains v5's empty confirm, the TF2-running confirm and the apply;
    // `activating` is cleared once the chain ends, at whichever step it stopped.
    pub fn activate(&mut self, id: ProfileId, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        let Some(tf2) = tf2_state(cx).read(cx).install().cloned() else {
            return;
        };
        let Some(profile) = self.profiles.iter().find(|p| p.id() == &id) else {
            return;
        };
        let empty_answer =
            model::is_empty(profile).then(|| confirm(Confirm::activate_empty(), window, cx));
        self.activating = Some(id.clone());
        cx.notify();
        let probe = Tf2Probe::get(cx);
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let flow = async {
                if let Some(answer) = empty_answer
                    && !answer.recv().await.unwrap_or(false)
                {
                    return;
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
                    if !skip_missing && let Ok(Activation::Missing { profile, items }) = &done {
                        let Ok(answer) = cx.update(|cx| {
                            win.update(cx, |_, window, cx| {
                                missing_items_dialog(profile.name(), items, window, cx)
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
                    let tf2 = tf2.clone();
                    let _ = cx.update(|cx| {
                        win.update(cx, |_, window, cx| announce(done, tf2, window, cx))
                    });
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
        let loading = run_blocking(cx, |s| {
            let failed = |e: ProfileError| describe_profile(&e).into_op("Failed to load profiles");
            let profiles = s.library.profiles().map_err(failed)?;
            let active = s.library.active_profile().map_err(failed)?;
            Ok((profiles, active))
        });
        let win = self.win;
        cx.spawn(async move |page, cx| {
            let loaded = loading.await;
            let current = page
                .update(cx, |page, cx| {
                    if page.load_generation != generation {
                        return false;
                    }
                    if let Ok((profiles, active)) = &loaded {
                        page.profiles = profiles.as_slice().into();
                        page.active = active.clone();
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
    // stored window handle, then reloads; Save Current and Delete touch no other page.
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

    /// Opens v5's Save Current dialog; does nothing while `is_busy()`.
    // The builder runs every frame, so it reads the name field each time to decide whether
    // "Save Profile" is enabled; Enter saves only a non-blank name.
    pub fn open_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        let input = self.save_name.clone();
        input.update(cx, |st, cx| st.set_value("", window, cx));
        let page = cx.entity().downgrade();
        window.defer(cx, move |window, cx| {
            let frame = DialogFrame::new(DIALOG_WIDTH);
            let field = input.clone();
            window.open_dialog(cx, move |dialog, window, cx| {
                let typed = input.read(cx).value();
                let blank = typed.trim().is_empty();
                let taken = page
                    .upgrade()
                    .and_then(|p| p.read(cx).taken_name(&typed, None));
                let on_ok = {
                    let (page, input) = (page.clone(), input.clone());
                    move |_: &_, _: &mut Window, cx: &mut App| submit_save(&page, &input, cx)
                };
                let ok = {
                    let (page, input) = (page.clone(), input.clone());
                    move |_: &_, window: &mut Window, cx: &mut App| {
                        if submit_save(&page, &input, cx) {
                            window.close_dialog(cx);
                        }
                    }
                };
                frame.apply(dialog, &*window, &*cx).on_ok(on_ok).child(
                    frame.measure(
                        v_flex()
                            .id(ids::PROFILES_SAVE_DIALOG)
                            .test_support()
                            .gap_4()
                            .child(
                                v_flex()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_lg()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child("Save Current as Profile"),
                                    )
                                    .child(labelled(
                                        div().text_sm().text_color(cx.theme().muted_foreground),
                                        ids::PROFILES_SAVE_DESCRIPTION,
                                        SAVE_DESCRIPTION,
                                    )),
                            )
                            .child(
                                v_flex()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::MEDIUM)
                                            .child("Profile Name"),
                                    )
                                    .child(
                                        div()
                                            .id(ids::PROFILES_SAVE_NAME)
                                            .test_support()
                                            .child(Input::new(&input)),
                                    )
                                    .children(taken.as_deref().map(|name| {
                                        labelled(
                                            div().text_sm().text_color(cx.theme().danger),
                                            ids::PROFILES_SAVE_TAKEN,
                                            model::taken_message(name),
                                        )
                                    })),
                            )
                            .child(
                                h_flex()
                                    .justify_end()
                                    .gap_2()
                                    .child(
                                        Button::new(ids::PROFILES_SAVE_CANCEL)
                                            .outline()
                                            .label("Cancel")
                                            .on_click(|_, window, cx| window.close_dialog(cx)),
                                    )
                                    .child(
                                        Button::new(ids::PROFILES_SAVE_OK)
                                            .primary()
                                            .label("Save Profile")
                                            .disabled(blank || taken.is_some())
                                            .on_click(ok),
                                    ),
                            ),
                    ),
                )
            });
            // Opening focuses the dialog; the name field takes the keyboard from it.
            field.update(cx, |st, cx| st.focus(window, cx));
        });
    }

    // Saves what tf/custom holds now under `name`; without a TF2 install there is nothing to read.
    fn save_current(&mut self, name: String, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        let Some(tf2) = tf2_state(cx).read(cx).install().cloned() else {
            return;
        };
        self.run(
            move |s| {
                s.library
                    .save_current_profile(&name, &tf2)
                    .map_err(|e| describe_profile(&e).into_op("Failed to save profile"))?;
                let message = format!("Saved current setup as profile: {name}");
                let entry = NewLogEntry::new(LogLevel::Info, "profile", "save_current", message);
                activity::write(s, &entry);
                let created = format!("\"{name}\" created from current setup");
                Ok(("Profile saved".into(), created.into()))
            },
            cx,
        );
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
        let message = format!("Deleted profile: {name}");
        cx.spawn(async move |page, cx| {
            if !answer.recv().await.unwrap_or(false) {
                return;
            }
            let _ = page.update(cx, |page, cx| {
                page.run(
                    move |s| {
                        s.library
                            .delete_profile(&id)
                            .map_err(|e| describe_profile(&e).into_op("Failed to delete"))?;
                        // The id is internal; the player's row names the profile instead.
                        let entry = NewLogEntry::new(LogLevel::Info, "profile", "delete", message);
                        activity::write(s, &entry);
                        Ok(("Profile deleted".into(), SharedString::default()))
                    },
                    cx,
                );
            });
        })
        .detach();
    }

    // The profiles the grid shows, as indices into `profiles`: those the search matches.
    fn shown(&self, cx: &App) -> Vec<usize> {
        let query = self.search.read(cx).value();
        (0..self.profiles.len())
            .filter(|&ix| model::matches(self.profiles[ix].name(), &query))
            .collect()
    }

    fn active_profile(&self) -> Option<&Profile> {
        let id = self.active.as_ref()?;
        self.profiles.iter().find(|p| p.id() == id)
    }
}

// The activate job's result: `OpError` cannot carry the missing items, so `Missing` is a value.
enum Activation {
    Applied {
        profile: Profile,
        report: ApplyProfileReport,
        hud_folder: Option<String>,
        /// What the toast adds about the HUDs kept or removed on the way.
        note: Option<String>,
    },
    Missing {
        profile: Profile,
        items: Vec<MissingItem>,
    },
    External(Vec<ExternalHud>),
}

// Reads the profile, carries out `plan`, applies it and writes the activate row for an applied
// report; a missing item or an unnamed HUD in the way touches nothing, so it becomes a value
// with no row.
fn apply_job(
    s: &Services,
    id: &ProfileId,
    tf2: &Tf2Install,
    skip_missing: bool,
    plan: &ExternalPlan,
) -> Result<Activation, OpError> {
    const FAILED: &str = "Failed to activate profile";
    let profile = s
        .library
        .profile(id)
        .map_err(|e| describe_profile(&e).into_op(FAILED))?;
    let discard = plan.prepare(s, tf2, FAILED)?;
    let applied = s
        .library
        .apply_profile_discarding(id, tf2, &s.builtins, skip_missing, discard);
    let report = match applied {
        Ok(report) => report,
        Err(ApplyProfileError::Missing(items)) => {
            return Ok(Activation::Missing { profile, items });
        }
        Err(ApplyProfileError::ExternalHuds(huds)) => return Ok(Activation::External(huds)),
        Err(e) => return Err(describe_apply_profile(&e).into_op(FAILED)),
    };
    let skipped: Vec<&str> = report.skipped().iter().map(MissingItem::name).collect();
    let details = model::report_details(report.applied(), &skipped, report.failed());
    let name = profile.name();
    let (level, message) = if !report.failed().is_empty() {
        (
            LogLevel::Error,
            format!("Failed to activate profile: {name}"),
        )
    } else if skipped.is_empty() {
        (LogLevel::Info, format!("Activated profile: {name}"))
    } else {
        // Missing items were left as they were, so the profile is applied, not active.
        (LogLevel::Info, format!("Applied profile: {name}"))
    };
    let entry = NewLogEntry::new(level, "profile", "activate", message).with_details(details);
    activity::write(s, &entry);
    let hud_step_ran = report.applied().contains(&ApplyStep::Hud);
    if hud_step_ran {
        plan.log_discarded(s);
    }
    // An apply that put a HUD in place names its folder, so the font check can claim it at once.
    let hud_folder = (profile.hud().is_some() && hud_step_ran)
        .then(|| active_hud(s, tf2))
        .flatten();
    Ok(Activation::Applied {
        profile,
        report,
        hud_folder,
        note: plan.note().filter(|_| hud_step_ran),
    })
}

// Toasts the activate outcome; an applied report (failed steps or not) changed tf/custom, so
// it also tells every library page to reload.
fn announce(done: Result<Activation, OpError>, tf2: Tf2Install, window: &mut Window, cx: &mut App) {
    // An apply that put a HUD in place checks that HUD's fonts once the toast is up.
    let hud_folder = match &done {
        Ok(Activation::Applied { hud_folder, .. }) => hud_folder.clone(),
        _ => None,
    };
    match done {
        Ok(Activation::Applied {
            profile,
            report,
            note,
            ..
        }) if report.failed().is_empty() => {
            let title = model::success_title(profile.name(), !report.skipped().is_empty());
            // A HUD kept out of its expected folder warns as a plain HUD install does.
            match report.hud_blocked_name() {
                Some(blocked) => toast(
                    window,
                    cx,
                    ToastKind::Warning,
                    title,
                    blocked_description(blocked),
                ),
                None => {
                    let installed = model::installed_parts(&profile, &report);
                    let kinds: Vec<MissingKind> =
                        report.skipped().iter().map(|m| m.kind().clone()).collect();
                    let kept = model::kept_parts(&kinds);
                    let mut description = model::activated_description(&installed, &kept);
                    if let Some(note) = note {
                        description = format!("{description}. {note}");
                    }
                    toast(window, cx, ToastKind::Success, title, description)
                }
            }
            LibraryEvents::changed(cx);
        }
        Ok(Activation::Applied { report, .. }) => {
            let description = model::failed_description(report.failed());
            toast(
                window,
                cx,
                ToastKind::Error,
                "Failed to activate profile",
                description,
            );
            LibraryEvents::changed(cx);
        }
        // Both are answered in the flow; a refusal that reaches here changed nothing.
        Ok(Activation::Missing { .. } | Activation::External(_)) => toast(
            window,
            cx,
            ToastKind::Error,
            "Failed to activate profile",
            "Some items are missing",
        ),
        Err(err) => toast_error(window, cx, &err),
    }
    if let Some(folder) = hud_folder {
        ask_about_fonts(window.window_handle(), tf2, folder, cx);
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
    let description: SharedString =
        format!("Some items in \"{profile_name}\" are no longer in your library.").into();
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
                                            .aria_label("Missing Items")
                                            .test_support()
                                            .text_lg()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child("Missing Items"),
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
                                            .label("Cancel"),
                                    )
                                    .child(
                                        button(
                                            ids::PROFILES_MISSING_INSTALL,
                                            MissingAnswer::InstallAnyway,
                                        )
                                        .primary()
                                        .label("Install Anyway (skip missing)"),
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
                .label("Re-download")
                .on_click(move |_, window, cx| {
                    open_detail(detail.clone(), body(), window, cx);
                })
                .into_any_element()
        }
        None => {
            const LOCAL: &str = "Local import \u{2014} can't re-download";
            div()
                .id(ids::missing_local(ix))
                .aria_label(LOCAL)
                .test_support()
                .text_xs()
                .text_color(muted)
                .child(LOCAL)
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

// The Save Current dialog's OK: false (dialog stays open) for a blank or taken name, else
// starts the save.
fn submit_save(page: &WeakEntity<ProfilesPage>, input: &Entity<InputState>, cx: &mut App) -> bool {
    let name = input.read(cx).value().trim().to_owned();
    let taken = page
        .upgrade()
        .is_some_and(|p| p.read(cx).taken_name(&name, None).is_some());
    if name.is_empty() || taken {
        return false;
    }
    let _ = page.update(cx, |page, cx| page.save_current(name, cx));
    true
}

// A badge led by its size-3 icon, as v5 draws a profile's slots on both kinds of card.
fn icon_badge(
    id: impl Into<gpui_kit::ElementId>,
    tag: Tag,
    border: gpui_kit::Hsla,
    icon: IconName,
    text: impl Into<SharedString>,
) -> impl IntoElement {
    let tag = tag
        .gap_1()
        .child(Icon::new(icon).with_size(Size::Size(px(12.))));
    badge_of(id, tag, border, text)
}

// v5's "Active Profile" card: the name with its "Active" badge, then a badge per filled slot,
// or "Empty profile (vanilla)" when the profile fills none.
fn active_card(profile: &Profile, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let mut badges = h_flex().flex_wrap().gap_2();
    if model::is_empty(profile) {
        badges = badges.child(labelled(
            div().text_sm().text_color(muted),
            ids::PROFILES_ACTIVE_EMPTY,
            "Empty profile (vanilla)",
        ));
    } else {
        let slots = [
            (
                ids::PROFILES_ACTIVE_HUD,
                IconName::Paintbrush,
                profile.hud(),
            ),
            (
                ids::PROFILES_ACTIVE_HITSOUND,
                IconName::Volume2,
                profile.hitsound(),
            ),
            (
                ids::PROFILES_ACTIVE_KILLSOUND,
                IconName::Volume2,
                profile.killsound(),
            ),
        ];
        for (id, icon, item) in slots {
            if let Some(item) = item {
                badges = badges.child(icon_badge(
                    id,
                    Tag::secondary(),
                    transparent_black(),
                    icon,
                    item.name().to_owned(),
                ));
            }
        }
        if !profile.crosshairs().is_empty() {
            badges = badges.child(icon_badge(
                ids::PROFILES_ACTIVE_CROSSHAIRS,
                Tag::secondary(),
                transparent_black(),
                IconName::Crosshair,
                model::crosshair_count(profile.crosshairs().len()),
            ));
        }
    }
    card_frame(cx)
        .id(ids::PROFILES_ACTIVE)
        .test_support()
        .border_color(theme.primary.opacity(0.5))
        .bg(theme.primary.opacity(0.05))
        .p_6()
        .child(
            v_flex()
                .gap_3()
                .child(
                    h_flex()
                        .gap_2()
                        .child(labelled(
                            div().font_weight(FontWeight::SEMIBOLD),
                            ids::PROFILES_ACTIVE_NAME,
                            profile.name().to_owned(),
                        ))
                        .child(badge_of(
                            ids::PROFILES_ACTIVE_BADGE,
                            Tag::primary(),
                            transparent_black(),
                            "Active",
                        )),
                )
                .child(badges),
        )
        .into_any_element()
}

// v5's EmptyState inside a Card: an icon, the title and a muted description, plus any buttons.
fn empty_state(
    id: &'static str,
    icon: IconName,
    title: &'static str,
    description: &'static str,
    actions: Option<AnyElement>,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    card_frame(cx)
        .id(id)
        .aria_label(title)
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

// v5's ProfileCard: the name (and "Active"), its item badges or "Empty profile", then
// Activate, Edit (the wizard in edit mode) and Delete.
fn profile_card(
    profile: &Profile,
    active: bool,
    has_tf2: bool,
    busy: bool,
    activating: Option<&ProfileId>,
    page: &WeakEntity<ProfilesPage>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let id = profile.id();
    let on_activate = {
        let (page, id) = (page.clone(), id.clone());
        move |_: &_, window: &mut Window, cx: &mut App| {
            let _ = page.update(cx, |page, cx| page.activate(id.clone(), window, cx));
        }
    };
    let this_one = activating == Some(id);
    let activate_label = match (this_one, active) {
        (true, _) => "Activating...",
        (false, true) => "Active",
        (false, false) => "Activate",
    };
    let on_edit = {
        let (page, id) = (page.clone(), id.clone());
        move |_: &_, window: &mut Window, cx: &mut App| {
            let _ = page.update(cx, |page, cx| {
                page.open_wizard(WizardMode::Edit(id.clone()), window, cx)
            });
        }
    };
    let on_delete = {
        let (page, id, name) = (page.clone(), id.clone(), profile.name().to_owned());
        move |_: &_, window: &mut Window, cx: &mut App| {
            let _ = page.update(cx, |page, cx| page.delete(id.clone(), &name, window, cx));
        }
    };
    let outline = || Tag::secondary().outline().text_color(theme.foreground);
    let mut badges = h_flex().flex_wrap().gap_1p5();
    if model::is_empty(profile) {
        badges = badges.child(labelled(
            div().text_xs().text_color(theme.muted_foreground),
            ids::profile_empty(id),
            "Empty profile",
        ));
    } else {
        let slots = [
            (ids::profile_hud(id), IconName::Paintbrush, profile.hud()),
            (
                ids::profile_hitsound(id),
                IconName::Volume2,
                profile.hitsound(),
            ),
            (
                ids::profile_killsound(id),
                IconName::Volume2,
                profile.killsound(),
            ),
        ];
        for (badge_id, icon, item) in slots {
            if let Some(item) = item {
                badges = badges.child(icon_badge(
                    badge_id,
                    outline(),
                    theme.border,
                    icon,
                    item.name().to_owned(),
                ));
            }
        }
        if !profile.crosshairs().is_empty() {
            badges = badges.child(icon_badge(
                ids::profile_crosshairs(id),
                outline(),
                theme.border,
                IconName::Crosshair,
                model::crosshair_count(profile.crosshairs().len()),
            ));
        }
    }
    let border = if active {
        theme.primary.opacity(0.5)
    } else {
        theme.border
    };
    card_frame(cx)
        .id(ids::profile_card(id))
        .accessibility_id(ids::profile_card(id).to_string())
        .aria_label(profile.name().to_owned())
        .test_support()
        .size_full()
        .flex()
        .flex_col()
        .border_color(border)
        .p_6()
        .gap_3()
        .child(
            h_flex()
                .gap_2()
                .min_w_0()
                .child(
                    div()
                        .truncate()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(profile.name().to_owned()),
                )
                .children(active.then(|| {
                    badge_of(
                        ids::profile_active_badge(id),
                        Tag::primary(),
                        transparent_black(),
                        "Active",
                    )
                })),
        )
        .child(badges)
        .child(
            h_flex()
                .mt_auto()
                .gap_2()
                .child(
                    Button::new(ids::profile_activate(id))
                        .primary()
                        .px(SM_ICON_PX)
                        .icon(IconName::Play)
                        .label(activate_label)
                        .loading(this_one)
                        .disabled(active || !has_tf2 || busy)
                        .on_click(on_activate),
                )
                .child(
                    Button::new(ids::profile_edit(id))
                        .outline()
                        .icon(IconName::Pencil)
                        .named("Edit profile")
                        .disabled(busy)
                        .on_click(on_edit),
                )
                .child(
                    Button::new(ids::profile_delete(id))
                        .outline()
                        .icon(IconName::Trash)
                        .named("Delete profile")
                        .disabled(busy)
                        .on_click(on_delete),
                ),
        )
        .into_any_element()
}

impl ProfilesPage {
    // The "Your Profiles" row, then the grid of matching profiles or v5's empty states.
    fn library(&self, has_tf2: bool, busy: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let shown = self.shown(cx);
        let body = if !self.loaded {
            None
        } else if self.profiles.is_empty() {
            let actions = h_flex()
                .gap_2()
                .child(
                    Button::new(ids::PROFILES_EMPTY_CREATE)
                        .outline()
                        .px(SM_ICON_PX)
                        .icon(IconName::Plus)
                        .label("Create New")
                        .disabled(busy)
                        .on_click(cx.listener(|page, _, window, cx| {
                            page.open_wizard(WizardMode::Create, window, cx)
                        })),
                )
                .child(
                    Button::new(ids::PROFILES_EMPTY_SAVE)
                        .primary()
                        .px(SM_ICON_PX)
                        .icon(IconName::Save)
                        .label("Save Current Setup")
                        .disabled(!has_tf2 || busy)
                        .on_click(cx.listener(|page, _, window, cx| page.open_save(window, cx))),
                )
                .into_any_element();
            Some(empty_state(
                ids::PROFILES_EMPTY,
                IconName::PackageOpen,
                "No profiles yet",
                "Save your current setup as a profile to quickly switch between configurations.",
                Some(actions),
                cx,
            ))
        } else if shown.is_empty() {
            Some(empty_state(
                ids::PROFILES_NO_MATCH,
                IconName::Search,
                "No profiles found",
                "Try a different search term.",
                None,
                cx,
            ))
        } else {
            let (profiles, active) = (Rc::clone(&self.profiles), self.active.clone());
            let activating = self.activating.clone();
            let page = cx.entity().downgrade();
            Some(
                self.grid
                    .render(shown.len(), CARD_HEIGHT, move |ix, _, cx| {
                        let profile = &profiles[shown[ix]];
                        let is_active = active.as_ref() == Some(profile.id());
                        profile_card(
                            profile,
                            is_active,
                            has_tf2,
                            busy,
                            activating.as_ref(),
                            &page,
                            cx,
                        )
                    }),
            )
        };
        let muted = cx.theme().muted_foreground;
        // A narrow page wraps the search and buttons under the title, then the buttons under
        // the search, so nothing passes the margin and the search keeps a usable width.
        let buttons = h_flex()
            .flex_none()
            .gap_3()
            .child(
                Button::new(ids::PROFILES_CREATE)
                    .outline()
                    .px(SM_ICON_PX)
                    .icon(IconName::Plus)
                    .label("Create New")
                    .disabled(busy)
                    .on_click(cx.listener(|page, _, window, cx| {
                        page.open_wizard(WizardMode::Create, window, cx)
                    })),
            )
            .child(
                Button::new(ids::PROFILES_SAVE_CURRENT)
                    .primary()
                    .px(SM_ICON_PX)
                    .icon(IconName::Save)
                    .label("Save Current")
                    .disabled(!has_tf2 || busy)
                    .on_click(cx.listener(|page, _, window, cx| page.open_save(window, cx))),
            );
        v_flex()
            .flex_1()
            .gap_4()
            .child(
                h_flex()
                    .flex_wrap()
                    .justify_between()
                    .gap_4()
                    .child(
                        div()
                            .flex_none()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Your Profiles"),
                    )
                    .child(
                        h_flex()
                            .flex_wrap()
                            .justify_end()
                            .gap_3()
                            .child(
                                div()
                                    .id(ids::PROFILES_SEARCH)
                                    .test_support()
                                    .flex_grow(1.)
                                    .w(px(256.))
                                    .min_w(SEARCH_MIN_WIDTH)
                                    .child(
                                        Input::new(&self.search)
                                            .h_9()
                                            .prefix(Icon::new(IconName::Search).text_color(muted)),
                                    ),
                            )
                            .child(buttons),
                    ),
            )
            .child(
                div()
                    .id(ids::PROFILES_LIBRARY)
                    .test_support()
                    .flex_1()
                    .min_h(LIBRARY_MIN_HEIGHT)
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
        let active = match self.active_profile() {
            Some(profile) => active_card(profile, cx),
            None => empty_state(
                ids::PROFILES_NO_ACTIVE,
                IconName::Layers,
                "No active profile",
                "Activate a profile to apply a saved collection of customizations.",
                None,
                cx,
            ),
        };
        let warning = missing.then(|| {
            no_tf2_warning(
                ids::PROFILES_NO_TF2,
                "Set the TF2 folder in Settings to activate profiles.",
                cx,
            )
        });
        let content = v_flex()
            .id(ids::page(Page::Profiles))
            .test_support()
            .size_full()
            .track_scroll(&self.scroll)
            .overflow_y_scroll()
            .p(CONTENT_PADDING)
            .gap_6()
            .children(warning)
            .child(
                v_flex()
                    .flex_none()
                    .gap_3()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Active Profile"),
                    )
                    .child(active),
            )
            .child(self.library(has_tf2, busy, cx));
        scroll_area(ids::page_scrollbar(Page::Profiles), &self.scroll, content).size_full()
    }
}
