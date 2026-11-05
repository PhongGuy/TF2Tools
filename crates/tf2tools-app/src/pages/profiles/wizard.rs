//! v5's Create New / Edit profile wizard: a four-step dialog over Name, HUD, Sounds and
//! Crosshairs that saves a `ProfileDraft` through core and reloads the Profiles page.

use std::collections::BTreeMap;
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::select::{Select, SelectItem, SelectState};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _, Size, WindowExt as _, h_flex, v_flex,
};
use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext as _, Context, Entity, FocusHandle,
    Focusable as _, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, TestSupportExt as _,
    WeakEntity, Window, div, px,
};
use tf2tools_core::{
    Assignment, CrosshairEntry, DraftSlot, HudEntry, HudId, LibraryError, Profile, ProfileDraft,
    ProfileId, SoundEntry, SoundId,
};

use super::ProfilesPage;
use super::model as profiles_model;
use super::wizard_model::{self as model, CrosshairMode, CustomCrosshair, SlotOption, WizardStep};
use crate::activity;
use crate::audio::player::{NowPlaying, Player};
use crate::ids;
use crate::local_messages::{describe_library, describe_profile};
use crate::services::{Services, run_blocking};
use crate::shell::toast::{ToastKind, toast, toast_error};
use crate::widgets::Named as _;
use crate::widgets::crosshair_picker::CrosshairPicker;
use crate::widgets::dialog_frame::{DETAIL_WIDTH, DialogFrame};
use crate::widgets::labelled;
use crate::widgets::weapon_assigner::{
    AssignRequested, AssignedCell, AssignerState, WeaponAssigner,
};

/// Whether the wizard makes a new profile or rewrites a saved one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WizardMode {
    Create,
    Edit(ProfileId),
}

type Slots<T> = Entity<SelectState<Vec<SlotOption<T>>>>;

/// The wizard's state; it lives as long as its dialog, which owns the only strong handle.
pub struct ProfileWizard {
    kind: WizardMode,
    page: WeakEntity<ProfilesPage>,
    win: AnyWindowHandle,
    loaded: bool,
    step: WizardStep,
    mode: CrosshairMode,
    saving: bool,
    name: Entity<InputState>,
    hud: Slots<HudId>,
    hitsound: Slots<SoundId>,
    killsound: Slots<SoundId>,
    huds: usize,
    sounds: Rc<[SoundEntry]>,
    live: Rc<[Assignment]>,
    /// Library crosshairs, for labelling kept crosshairs.
    crosshairs: Rc<[CrosshairEntry]>,
    /// Customize mode's crosshairs by weapon script.
    custom: BTreeMap<String, CustomCrosshair>,
    picker: Entity<CrosshairPicker>,
    assigner: Entity<WeaponAssigner>,
    player: Entity<Player>,
    /// The dialog's own focus scope, captured as it opens.
    dialog_focus: Option<FocusHandle>,
    _subscriptions: Vec<Subscription>,
}

// Each select row carries its pilot id, since the kit's rows have none.
impl<T: Clone + PartialEq + 'static> SelectItem for SlotOption<T> {
    type Value = DraftSlot<T>;

    fn title(&self) -> SharedString {
        self.title.clone()
    }

    fn render(&self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .id(ids::wizard_option(self.field, &self.key))
            .test_support()
            .child(self.title.clone())
    }

    fn value(&self) -> &DraftSlot<T> {
        &self.value
    }
}

// What the load job reads: the libraries, the live crosshair assignments and, when editing,
// the profile.
struct Loaded {
    huds: Vec<HudEntry>,
    sounds: Vec<SoundEntry>,
    crosshairs: Vec<CrosshairEntry>,
    live: Vec<Assignment>,
    profile: Option<Profile>,
}

impl ProfileWizard {
    /// Builds the wizard, opens it in a dialog (`ids::WIZARD_DIALOG`, 672 px wide with 24 px
    /// padding and the kit's X, v5's sm:max-w-2xl Dialog.Content) and loads the library, the
    /// live assignments and, when editing, the profile on `run_blocking`. A save reloads `page`.
    pub fn open(
        mode: WizardMode,
        page: WeakEntity<ProfilesPage>,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<ProfileWizard> {
        let wizard = cx.new(|cx| Self::new(mode, page, window, cx));
        let child = wizard.clone();
        // Deferred, as `confirm` is, so the clicked button cannot pull focus back out.
        window.defer(cx, move |window, cx| {
            let content = child.clone();
            let frame = DialogFrame::new(DETAIL_WIDTH);
            window.open_dialog(cx, move |dialog, window, cx| {
                let wizard = content.clone();
                frame
                    .apply(dialog, &*window, &*cx)
                    // The kit closes on true: Enter never saves (the name field's Enter is
                    // Next), and Escape or the X cancels, silencing a preview, unless a save is
                    // in flight.
                    .on_ok(|_, _, _| false)
                    .on_cancel(move |_, _, cx| {
                        wizard.update(cx, |wizard, cx| {
                            let closing = !wizard.saving;
                            if closing {
                                wizard.stop_preview(cx);
                            }
                            closing
                        })
                    })
                    .child(frame.measure(content.clone()))
            });
            // Opening focuses the new dialog, which tells this one apart from any stacked later;
            // the name field then takes the keyboard from it, so the player can type at once.
            let focus = window.focused(cx);
            child.update(cx, |wizard, cx| {
                wizard.dialog_focus = focus;
                wizard.name.update(cx, |st, cx| st.focus(window, cx));
            });
        });
        wizard
    }

    fn new(
        kind: WizardMode,
        page: WeakEntity<ProfilesPage>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("e.g., Competitive Setup"));
        let hud = cx.new(|cx| SelectState::new(Vec::new(), None, window, cx));
        let hitsound = cx.new(|cx| SelectState::new(Vec::new(), None, window, cx));
        let killsound = cx.new(|cx| SelectState::new(Vec::new(), None, window, cx));
        let player = cx.new(|cx| Player::new(window, cx));
        let picker = cx.new(|cx| CrosshairPicker::new(true, window, cx));
        let assigner = cx.new(|cx| WeaponAssigner::new(true, cx));
        // Next, Save and the play buttons read the name and the selects at render, so any change
        // to them just redraws; Enter in the name field acts as Next.
        let mut subscriptions = vec![
            cx.subscribe_in(
                &name,
                window,
                |wizard, _, event: &InputEvent, window, cx| match event {
                    InputEvent::PressEnter { .. } => wizard.next(window, cx),
                    _ => cx.notify(),
                },
            ),
            cx.observe(&player, |_, _, cx| cx.notify()),
        ];
        subscriptions.push(cx.observe(&hud, |_, _, cx| cx.notify()));
        subscriptions.push(cx.observe(&hitsound, |_, _, cx| cx.notify()));
        subscriptions.push(cx.observe(&killsound, |_, _, cx| cx.notify()));
        // The assigner's rows and bulk buttons wait for a picked crosshair and stay inert while
        // a save runs; its clicks edit the draft's custom map, never the live assignment table.
        subscriptions.push(cx.observe(&picker, |wizard, _, cx| wizard.sync_assigner(cx)));
        subscriptions.push(
            cx.subscribe(&assigner, |wizard, _, event: &AssignRequested, cx| {
                wizard.assign(&event.scripts, cx)
            }),
        );
        let wizard = Self {
            kind,
            page,
            win: window.window_handle(),
            loaded: false,
            step: WizardStep::Name,
            mode: CrosshairMode::None,
            saving: false,
            name,
            hud,
            hitsound,
            killsound,
            huds: 0,
            sounds: Rc::from([]),
            live: Rc::from([]),
            crosshairs: Rc::from([]),
            custom: BTreeMap::new(),
            picker,
            assigner,
            player,
            dialog_focus: None,
            _subscriptions: subscriptions,
        };
        wizard.load(cx);
        wizard
    }

    /// True once the library load has ended, failed or not.
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    pub fn step(&self) -> WizardStep {
        self.step
    }

    /// The crosshair mode chosen on the last step.
    pub fn mode(&self) -> CrosshairMode {
        self.mode
    }

    pub fn is_saving(&self) -> bool {
        self.saving
    }

    pub fn name_input(&self) -> &Entity<InputState> {
        &self.name
    }

    pub fn hud_select(&self) -> &Entity<SelectState<Vec<SlotOption<HudId>>>> {
        &self.hud
    }

    pub fn hitsound_select(&self) -> &Entity<SelectState<Vec<SlotOption<SoundId>>>> {
        &self.hitsound
    }

    pub fn killsound_select(&self) -> &Entity<SelectState<Vec<SlotOption<SoundId>>>> {
        &self.killsound
    }

    /// Customize mode's crosshair picker.
    pub fn picker(&self) -> &Entity<CrosshairPicker> {
        &self.picker
    }

    /// Customize mode's per-class weapon list.
    pub fn assigner(&self) -> &Entity<WeaponAssigner> {
        &self.assigner
    }

    fn editing(&self) -> bool {
        matches!(self.kind, WizardMode::Edit(_))
    }

    // Whether the wizard's dialog is still the top one: the kit focuses each dialog it opens and
    // gives the focus back on close, so a dialog stacked above this one holds the focus.
    fn owns_top_dialog(&self, window: &Window, cx: &App) -> bool {
        self.dialog_focus
            .as_ref()
            .is_some_and(|focus| focus.contains_focused(window, cx))
    }

    fn can_proceed(&self, cx: &App) -> bool {
        model::can_proceed(&self.name.read(cx).value()) && self.taken_name(cx).is_none()
    }

    // The other profile already holding the typed name, read from the page's loaded list.
    fn taken_name(&self, cx: &App) -> Option<String> {
        let except = match &self.kind {
            WizardMode::Edit(id) => Some(id),
            WizardMode::Create => None,
        };
        let page = self.page.upgrade()?;
        page.read(cx)
            .taken_name(&self.name.read(cx).value(), except)
    }

    // Reads the libraries (and the edited profile) off the UI thread, then fills the selects
    // through the window, which `set_items` and `set_selected_value` need. A failed read
    // toasts; Create keeps the dialog with empty selects, Edit closes it rather than save
    // over the profile from a blank form.
    fn load(&self, cx: &mut Context<Self>) {
        let kind = self.kind.clone();
        let loading = run_blocking(cx, move |s| {
            let profile = match &kind {
                WizardMode::Create => None,
                WizardMode::Edit(id) => Some(
                    s.library
                        .profile(id)
                        .map_err(|e| describe_profile(&e).into_op("Failed to load profile"))?,
                ),
            };
            let failed = |e: LibraryError| describe_library(&e).into_op("Failed to load library");
            Ok(Loaded {
                huds: s.library.list_huds().map_err(failed)?,
                sounds: s.library.list_sounds().map_err(failed)?,
                crosshairs: s.library.list_crosshairs().map_err(failed)?,
                live: s.library.assignments().map_err(failed)?,
                profile,
            })
        });
        let editing = self.editing();
        let win = self.win;
        cx.spawn(async move |wizard, cx| {
            let loaded = loading.await;
            let _ = cx.update(|cx| {
                win.update(cx, |_, window, cx| {
                    let _ = wizard.update(cx, |wizard, cx| match loaded {
                        Ok(loaded) => wizard.fill(loaded, window, cx),
                        Err(err) => {
                            toast_error(window, cx, &err);
                            if editing && wizard.owns_top_dialog(window, cx) {
                                window.close_dialog(cx);
                            }
                        }
                    });
                })
            });
            let _ = wizard.update(cx, |wizard, cx| {
                wizard.loaded = true;
                cx.notify();
            });
        })
        .detach();
    }

    // Fills the selects, preselecting an edited profile's items, and seeds the name, the
    // crosshair mode and the custom map from it.
    fn fill(&mut self, loaded: Loaded, window: &mut Window, cx: &mut Context<Self>) {
        let profile = loaded.profile.as_ref();
        let (options, initial) = model::hud_options(&loaded.huds, profile.and_then(Profile::hud));
        set_slots(&self.hud, options, initial, window, cx);
        let sounds = [
            (
                &self.hitsound,
                "hitsound",
                profile.and_then(Profile::hitsound),
            ),
            (
                &self.killsound,
                "killsound",
                profile.and_then(Profile::killsound),
            ),
        ];
        for (select, field, saved) in sounds {
            let (options, initial) = model::sound_options(&loaded.sounds, saved, field);
            set_slots(select, options, initial, window, cx);
        }
        if let Some(profile) = profile {
            self.name.update(cx, |st, cx| {
                st.set_value(profile.name().to_owned(), window, cx)
            });
            self.custom = model::custom_from_profile(profile);
        }
        self.mode = model::initial_mode(profile);
        self.huds = loaded.huds.len();
        self.sounds = loaded.sounds.into();
        self.live = loaded.live.into();
        self.crosshairs = loaded.crosshairs.into();
        let library = self.crosshairs.clone();
        self.picker
            .update(cx, |picker, cx| picker.set_library(library, cx));
        self.relabel(cx);
    }

    // Sets each script to the picked crosshair; without a pick, or while saving, nothing changes.
    fn assign(&mut self, scripts: &[&'static str], cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        let Some(picked) = self.picker.read(cx).selected().cloned() else {
            return;
        };
        for script in scripts {
            self.custom
                .insert((*script).to_owned(), CustomCrosshair::Set(picked.clone()));
        }
        self.relabel(cx);
        cx.notify();
    }

    // Pushes the custom map to the assigner's cells: a picked crosshair by its picker name,
    // a kept one by `keep_label`.
    fn relabel(&self, cx: &mut Context<Self>) {
        let picker = self.picker.read(cx);
        let builtins = &cx.global::<Services>().builtins;
        let cells = self
            .custom
            .iter()
            .map(|(script, crosshair)| match crosshair {
                CustomCrosshair::Set(picked) => AssignedCell {
                    script: script.clone(),
                    label: picker
                        .name_of(picked)
                        .unwrap_or_else(|| "Unknown".to_owned()),
                    crosshair: Some(picked.clone()),
                },
                CustomCrosshair::Keep(saved) => AssignedCell {
                    script: script.clone(),
                    label: model::keep_label(saved, builtins, &self.crosshairs),
                    crosshair: None,
                },
            })
            .collect();
        self.assigner
            .update(cx, |assigner, cx| assigner.set_assigned(cells, cx));
    }

    // Mirrors the picker's selection and the save state into the assigner.
    fn sync_assigner(&self, cx: &mut Context<Self>) {
        let state = AssignerState {
            has_selection: self.picker.read(cx).selected().is_some(),
            busy: self.saving,
            running: None,
        };
        self.assigner
            .update(cx, |assigner, cx| assigner.set_state(state, cx));
    }

    // While a save runs the picker and the assigner are inert; both come back when it ends.
    fn sync_inputs(&self, cx: &mut Context<Self>) {
        let saving = self.saving;
        self.picker
            .update(cx, |picker, cx| picker.set_disabled(saving, cx));
        self.sync_assigner(cx);
    }

    fn stop_preview(&self, cx: &mut Context<Self>) {
        self.player.update(cx, |player, cx| player.stop(cx));
    }

    // Moves to `step`; leaving the Sounds step stops a preview. The name field stops rendering
    // off its step, so the dialog takes the keyboard back from it and Escape still cancels.
    fn go_to(&mut self, step: WizardStep, window: &mut Window, cx: &mut Context<Self>) {
        if self.step == WizardStep::Sounds && step != WizardStep::Sounds {
            self.stop_preview(cx);
        }
        if step != WizardStep::Name
            && self.name.read(cx).focus_handle(cx).is_focused(window)
            && let Some(focus) = &self.dialog_focus
        {
            window.focus(focus, cx);
        }
        self.step = step;
        cx.notify();
    }

    fn next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(next) = self.step.next()
            && self.can_proceed(cx)
        {
            self.go_to(next, window, cx);
        }
    }

    // v5's stepper: a later circle jumps there with a valid name; an earlier one, or any while
    // saving, does nothing.
    fn jump(&mut self, step: WizardStep, window: &mut Window, cx: &mut Context<Self>) {
        if !self.saving && step > self.step && self.can_proceed(cx) {
            self.go_to(step, window, cx);
        }
    }

    // Copy needs a live assignment; no mode changes while a save runs.
    fn set_mode(&mut self, mode: CrosshairMode, cx: &mut Context<Self>) {
        if self.saving || (mode == CrosshairMode::Copy && self.live.is_empty()) {
            return;
        }
        self.mode = mode;
        cx.notify();
    }

    // Plays the sound a select holds, or stops it when it is the one playing.
    fn preview(&mut self, id: SoundId, cx: &mut Context<Self>) {
        let Some(sound) = self.sounds.iter().find(|s| s.id() == &id) else {
            return;
        };
        let now = NowPlaying {
            key: sound.folder_name().to_owned().into(),
            name: sound.name().to_owned().into(),
            subtitle: sound.original_filename().to_owned().into(),
        };
        self.player
            .update(cx, |player, cx| {
                player.toggle(
                    now,
                    move |s| {
                        s.library
                            .sound_bytes(&id)
                            .map_err(|e| describe_library(&e).into_op("Failed to play sound"))
                    },
                    cx,
                )
            })
            .detach();
    }

    fn draft(&self, cx: &App) -> ProfileDraft {
        let slot = |select: &Slots<SoundId>| {
            select
                .read(cx)
                .selected_value()
                .cloned()
                .unwrap_or(DraftSlot::Empty)
        };
        let hud = self
            .hud
            .read(cx)
            .selected_value()
            .cloned()
            .unwrap_or(DraftSlot::Empty);
        let crosshairs = model::draft_crosshairs(self.mode, &self.live, &self.custom);
        ProfileDraft::new(self.name.read(cx).value().trim())
            .with_hud(hud)
            .with_hitsound(slot(&self.hitsound))
            .with_killsound(slot(&self.killsound))
            .with_crosshairs(crosshairs)
    }

    // Saves off the UI thread and writes the activity row. Success toasts, closes the dialog and
    // reloads the page; a failure toasts and keeps the dialog open for another try.
    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving || !self.can_proceed(cx) {
            return;
        }
        // Save was clicked, so this dialog is on top; pull the focus back from a field that has
        // stopped rendering, so only a dialog opened during the save can take it away.
        if let Some(focus) = &self.dialog_focus {
            window.focus(focus, cx);
        }
        let draft = self.draft(cx);
        let (kind, editing) = (self.kind.clone(), self.editing());
        self.saving = true;
        self.sync_inputs(cx);
        cx.notify();
        let saving = run_blocking(cx, move |s| {
            let (saved, deactivated) = match &kind {
                WizardMode::Create => s.library.create_profile(&draft).map(|p| (p, false)),
                WizardMode::Edit(id) => s
                    .library
                    .update_profile(id, &draft)
                    .map(|done| (done.profile().clone(), done.deactivated())),
            }
            .map_err(|e| describe_profile(&e).into_op(model::failed_title(editing)))?;
            activity::write(s, &model::log_entry(editing, &saved));
            Ok((saved, deactivated))
        });
        let (win, page) = (self.win, self.page.clone());
        cx.spawn(async move |wizard, cx| {
            let saved = saving.await;
            let ok = saved.is_ok();
            let _ = cx.update(|cx| {
                win.update(cx, |_, window, cx| match saved {
                    Ok((profile, deactivated)) => {
                        let (title, description) = if deactivated {
                            model::deactivated_toast(profile.name())
                        } else {
                            model::saved_toast(editing, profile.name())
                        };
                        toast(window, cx, ToastKind::Success, title, description);
                        let on_top = wizard
                            .read_with(cx, |wizard, cx| wizard.owns_top_dialog(window, cx))
                            .unwrap_or(false);
                        if on_top {
                            window.close_dialog(cx);
                        }
                    }
                    Err(err) => toast_error(window, cx, &err),
                })
            });
            if ok {
                let _ = page.update(cx, |page, cx| page.reload(cx));
            }
            let _ = wizard.update(cx, |wizard, cx| {
                wizard.saving = false;
                wizard.sync_inputs(cx);
                cx.notify();
            });
        })
        .detach();
    }
}

fn set_slots<T: Clone + PartialEq + 'static>(
    select: &Slots<T>,
    options: Vec<SlotOption<T>>,
    initial: DraftSlot<T>,
    window: &mut Window,
    cx: &mut App,
) {
    select.update(cx, |st, cx| {
        st.set_items(options, window, cx);
        st.set_selected_value(&initial, window, cx);
    });
}

// A form field: its label over the control.
fn field(label: &'static str, control: impl IntoElement) -> impl IntoElement {
    v_flex()
        .gap_2()
        .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(label))
        .child(control)
}

impl ProfileWizard {
    // v5's stepper: a compact, centred row of size-7 circles joined by short lines. Steps
    // before the current one show a check, and the circles from the current one on its number.
    fn stepper(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (primary, on_primary) = (theme.primary, theme.primary_foreground);
        let (muted_bg, muted) = (theme.muted, theme.muted_foreground);
        let mut row = h_flex().justify_center().items_center().gap_1().py_2();
        for (ix, step) in WizardStep::ALL.into_iter().enumerate() {
            if ix > 0 {
                let line = if step <= self.step { primary } else { muted_bg };
                row = row.child(div().mx_1().w(px(24.)).h(px(1.)).bg(line));
            }
            let reached = step <= self.step;
            let mark = if step < self.step {
                Icon::new(IconName::Check)
                    .with_size(Size::Size(px(14.)))
                    .into_any_element()
            } else {
                step.number().to_string().into_any_element()
            };
            let circle = div()
                .id(ids::wizard_step(step.number()))
                .aria_label(step.label())
                .test_support()
                .size(px(28.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .cursor_pointer()
                .child(mark)
                .on_click(cx.listener(move |wizard, _, window, cx| wizard.jump(step, window, cx)));
            let circle = if reached {
                circle.bg(primary).text_color(on_primary)
            } else {
                circle.bg(muted_bg).text_color(muted)
            };
            let label = div().text_xs().child(step.label());
            let label = if step == self.step {
                label.font_weight(FontWeight::MEDIUM)
            } else {
                label.text_color(muted)
            };
            row = row.child(h_flex().gap_1().child(circle).child(label));
        }
        row
    }

    fn name_step(&self, cx: &App) -> AnyElement {
        let taken = self.taken_name(cx).map(|name| {
            labelled(
                div().text_sm().text_color(cx.theme().danger),
                ids::WIZARD_NAME_TAKEN,
                profiles_model::taken_message(&name),
            )
        });
        v_flex()
            .gap_2()
            .child(field(
                "Profile Name",
                div()
                    .id(ids::WIZARD_NAME)
                    .test_support()
                    .child(Input::new(&self.name).disabled(self.saving)),
            ))
            .children(taken)
            .into_any_element()
    }

    fn hud_step(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        v_flex()
            .gap_2()
            .child(field(
                "HUD",
                div()
                    .id(ids::WIZARD_HUD)
                    .test_support()
                    .child(Select::new(&self.hud).disabled(self.saving)),
            ))
            .children((self.huds == 0).then(|| {
                labelled(
                    div().text_sm().text_color(muted),
                    ids::WIZARD_HUD_EMPTY,
                    "No HUDs in your library. Import or download one first.",
                )
            }))
            .into_any_element()
    }

    // A sound select with its play button, shown while the select holds a library sound.
    fn sound_row(
        &self,
        label: &'static str,
        id: &'static str,
        play_id: &'static str,
        select: &Slots<SoundId>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let playing = self.player.read(cx).now_playing().map(|n| n.key.clone());
        let play = match select.read(cx).selected_value() {
            Some(DraftSlot::Item(sound)) => {
                let sound = sound.clone();
                let folder = self
                    .sounds
                    .iter()
                    .find(|s| s.id() == &sound)
                    .map(|s| SharedString::from(s.folder_name().to_owned()));
                let (icon, words) = if folder.is_some() && folder == playing {
                    (IconName::Square, "Stop")
                } else {
                    (IconName::Play, "Play")
                };
                Some(
                    Button::new(play_id)
                        .outline()
                        .small()
                        .icon(icon)
                        .named(words)
                        .on_click(
                            cx.listener(move |wizard, _, _, cx| wizard.preview(sound.clone(), cx)),
                        ),
                )
            }
            _ => None,
        };
        field(
            label,
            h_flex()
                .gap_2()
                .child(
                    div()
                        .id(id)
                        .test_support()
                        .flex_1()
                        .child(Select::new(select).disabled(self.saving)),
                )
                .children(play),
        )
    }

    fn sounds_step(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let (hitsound, killsound) = (self.hitsound.clone(), self.killsound.clone());
        v_flex()
            .gap_4()
            .child(self.sound_row(
                "Hitsound",
                ids::WIZARD_HITSOUND,
                ids::WIZARD_HITSOUND_PLAY,
                &hitsound,
                cx,
            ))
            .child(self.sound_row(
                "Killsound",
                ids::WIZARD_KILLSOUND,
                ids::WIZARD_KILLSOUND_PLAY,
                &killsound,
                cx,
            ))
            .children(self.sounds.is_empty().then(|| {
                labelled(
                    div().text_sm().text_color(muted),
                    ids::WIZARD_SOUNDS_EMPTY,
                    "No sounds in your library. Import or download some first.",
                )
            }))
            .into_any_element()
    }

    // v5's three mode buttons; Copy needs a live assignment. Customize stacks the compact
    // picker over the weapon list, then the count of assigned weapons.
    fn crosshairs_step(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let live = self.live.len();
        let mode_button = |id: &'static str, label: SharedString, mode: CrosshairMode| {
            let button = Button::new(id).small().label(label).disabled(self.saving);
            if self.mode == mode {
                button.primary()
            } else {
                button.outline()
            }
        };
        let customize = (self.mode == CrosshairMode::Customize).then(|| {
            v_flex()
                .gap_4()
                .child(self.picker.clone())
                .child(self.assigner.clone())
                .children((!self.custom.is_empty()).then(|| {
                    labelled(
                        div().text_sm().text_color(muted),
                        ids::WIZARD_ASSIGNED_COUNT,
                        model::assigned_count(self.custom.len()),
                    )
                }))
        });
        let note = (self.mode == CrosshairMode::Copy).then(|| {
            labelled(
                div().text_sm().text_color(muted),
                ids::WIZARD_COPY_NOTE,
                model::copy_note(live),
            )
        });
        v_flex()
            .gap_4()
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        mode_button(
                            ids::WIZARD_MODE_NONE,
                            "No crosshairs".into(),
                            CrosshairMode::None,
                        )
                        .on_click(
                            cx.listener(|wizard, _, _, cx| {
                                wizard.set_mode(CrosshairMode::None, cx)
                            }),
                        ),
                    )
                    .child(
                        mode_button(
                            ids::WIZARD_MODE_COPY,
                            model::copy_label(live).into(),
                            CrosshairMode::Copy,
                        )
                        .disabled(live == 0 || self.saving)
                        .on_click(
                            cx.listener(|wizard, _, _, cx| {
                                wizard.set_mode(CrosshairMode::Copy, cx)
                            }),
                        ),
                    )
                    .child(
                        mode_button(
                            ids::WIZARD_MODE_CUSTOM,
                            "Customize".into(),
                            CrosshairMode::Customize,
                        )
                        .on_click(cx.listener(|wizard, _, _, cx| {
                            wizard.set_mode(CrosshairMode::Customize, cx)
                        })),
                    ),
            )
            .children(note)
            .children(customize)
            .into_any_element()
    }

    // v5's footer under a top border: Back alone on the left; Cancel and Next, or Save on the
    // last step, on the right.
    fn footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let ready = self.can_proceed(cx);
        let last = self.step.next().is_none();
        let back = self.step.back().map(|back| {
            Button::new(ids::WIZARD_BACK)
                .outline()
                .label("Back")
                .disabled(self.saving)
                .on_click(cx.listener(move |wizard, _, window, cx| wizard.go_to(back, window, cx)))
        });
        let forward = if last {
            Button::new(ids::WIZARD_SAVE)
                .primary()
                .label(model::save_label(self.editing(), self.saving))
                .loading(self.saving)
                .disabled(!ready || self.saving)
                .on_click(cx.listener(|wizard, _, window, cx| wizard.save(window, cx)))
        } else {
            Button::new(ids::WIZARD_NEXT)
                .primary()
                .label("Next")
                .disabled(!ready)
                .on_click(cx.listener(|wizard, _, window, cx| wizard.next(window, cx)))
        };
        let cancel = Button::new(ids::WIZARD_CANCEL)
            .outline()
            .label("Cancel")
            .disabled(self.saving)
            .on_click(cx.listener(|wizard, _, window, cx| {
                wizard.stop_preview(cx);
                window.close_dialog(cx);
            }));
        h_flex()
            .justify_between()
            .border_t_1()
            .border_color(cx.theme().border)
            .pt_4()
            .child(div().children(back))
            .child(h_flex().gap_2().child(cancel).child(forward))
    }
}

impl Render for ProfileWizard {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let editing = self.editing();
        let body = match self.step {
            WizardStep::Name => self.name_step(cx),
            WizardStep::Hud => self.hud_step(cx),
            WizardStep::Sounds => self.sounds_step(cx),
            WizardStep::Crosshairs => self.crosshairs_step(cx),
        };
        v_flex()
            .id(ids::WIZARD_DIALOG)
            .test_support()
            .gap_4()
            .child(
                v_flex()
                    .gap_2()
                    .child(labelled(
                        div().text_lg().font_weight(FontWeight::SEMIBOLD),
                        ids::WIZARD_TITLE,
                        model::title(editing),
                    ))
                    .child(labelled(
                        div().text_sm().text_color(muted),
                        ids::WIZARD_DESCRIPTION,
                        self.step.description(),
                    )),
            )
            .child(self.stepper(cx))
            .child(div().min_h(px(200.)).py_2().child(body))
            .child(self.footer(cx))
    }
}
