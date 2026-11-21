//! The profile draft editor: a header crumb with Cancel and Save, the Name field, a rail and
//! one Set / None / Leave card per slot, and the Delete foot. Every change stays in the draft
//! until Save writes it to the library.

use gpui_kit::base::Radio;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::select::{SelectItem, SelectState};
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AnyView, App, AppContext as _, Context, Div, Entity, EventEmitter, FocusHandle,
    FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Render, Role,
    ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _, Subscription,
    TestSupportExt as _, WeakEntity, Window, canvas, deferred, div, point, px, transparent_black,
};
use std::cell::Cell;
use std::rc::Rc;
use tf2tools_core::{
    Choices, CrosshairRef, LogLevel, NewLogEntry, Profile, ProfileError, ProfileId, SoundId,
    SoundKind,
};

use super::ProfilesPage;
use super::draft_slots::{Library, on_sound_confirm};
use super::editor_model::{self, Draft, Mode, SaveBlock, SlotKind};
use crate::activity;
use crate::audio::player::{Player, sound_player};
use crate::i18n::tr;
use crate::ids::{self, IdScope};
use crate::keymap::{PROFILE_EDITOR_CONTEXT, ProfileEditorCancel};
use crate::local_messages::describe_profile;
use crate::pages::common::{badge_of, card_frame};
use crate::pages::graphics::{ChoicesEdit, GraphicsChoicesEditor, pill};
use crate::previews::CrosshairPreviews;
use crate::remote_images::RemoteImages;
use crate::services::{OpError, run_blocking};
use crate::shell::CONTENT_PADDING;
use crate::shell::route::Page;
use crate::shell::toast::toast_error;
use crate::widgets::card_grid::SHELL_CHROME_WIDTH;
use crate::widgets::crosshair_picker::{CrosshairPicker, ImportCrosshair};
use crate::widgets::labelled;
use crate::widgets::scroll::scroll_area;

/// The body's widest column, centred in a wider window.
const BODY_MAX_WIDTH: f32 = 1180.;

/// The slot rail's width beside the cards.
pub const RAIL_WIDTH: Pixels = px(184.);

/// Content narrower than this puts the rail in a wrapping row above the cards.
pub const RAIL_ROW_BELOW: Pixels = px(900.);

/// The space between the rail and the cards beside it.
const RAIL_GUTTER: Pixels = px(24.);

/// A slot card's padding plus its border, each side.
const CARD_INSET: Pixels = px(17.);

/// What the editor asks of the page that hosts it.
#[derive(Debug, Clone)]
pub enum EditorEvent {
    /// The player asked to leave: Cancel, Esc or the crumb.
    Close,
    /// Save landed; `deactivated` when the profile stopped being the active one. Boxed, since
    /// a whole profile dwarfs the other variants.
    Saved {
        profile: Box<Profile>,
        deactivated: bool,
    },
    /// The profile, named so, is gone from the library, so the editor closes at once.
    Deleted(String),
}

impl EventEmitter<EditorEvent> for ProfileDraftEditor {}

/// A library sound, or TF2 default when `id` is None.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundOption {
    pub id: Option<SoundId>,
    pub label: SharedString,
}

/// What the Crosshairs board shows for one weapon of the draft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoardRef {
    Shown(CrosshairRef),
    /// A library crosshair, by folder, that the library no longer has.
    Waiting(String),
}

impl SelectItem for SoundOption {
    type Value = Self;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    fn value(&self) -> &Self {
        self
    }
}

/// One profile's draft, edited in place of the Profiles list.
pub struct ProfileDraftEditor {
    saved: Option<Profile>,
    pub(super) draft: Draft,
    pub(super) baseline: Draft,
    name: Entity<InputState>,
    /// The name field was edited or Save was tried, so a blank name may say so.
    name_touched: bool,
    /// The name core refused as taken at Save, until the field changes.
    name_taken: Option<String>,
    pub(super) busy: bool,
    /// The hosting page, whose loaded list answers the live name check.
    pub(super) page: Option<WeakEntity<ProfilesPage>>,
    focus: FocusHandle,
    scroll: ScrollHandle,
    /// The scroll column's child index of the first slot card, for the rail's jumps.
    cards_from: usize,
    crumb: Entity<EditorCrumb>,
    hit_select: Entity<SelectState<Vec<SoundOption>>>,
    kill_select: Entity<SelectState<Vec<SoundOption>>>,
    /// The HUDs and sounds the Set bodies pick from; None until the first load lands.
    pub(super) library: Option<Library>,
    /// Bumped per library load, so a slower older load never replaces a newer one.
    pub(super) library_generation: u64,
    /// The app's floating player, which plays the previews.
    pub(super) player: Entity<Player>,
    /// The Crosshairs body's gallery strip, whose selection the board assigns.
    pub(super) picker: Entity<CrosshairPicker>,
    pub(super) previews: Entity<CrosshairPreviews>,
    /// The slot cards' inner width last frame, which turns and shortens the board.
    pub(super) card_width: Pixels,
    /// This editor's own `RemoteImages` viewer, handed back on release.
    pub(super) image_viewer: String,
    /// The Graphics body's presets, changes and rows over the draft's choices.
    pub(super) graphics: Entity<GraphicsChoicesEditor>,
    /// The Graphics page's saved choices for Copy; None until the read lands.
    pub(super) page_choices: Option<Result<Option<Choices>, OpError>>,
    /// The content area was narrower than the wide layout last frame.
    pub(super) narrow: bool,
    _subscriptions: Vec<Subscription>,
}

impl ProfileDraftEditor {
    /// An editor on a saved profile (`saved` Some) or on a new, unsaved draft.
    pub fn new(
        saved: Option<Profile>,
        draft: Draft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // A stored Graphics None with Add-ons Set opens as Add-ons None against the stored
        // baseline, so the draft reads dirty and Save writes the fix.
        let baseline = draft.clone();
        let mut draft = draft;
        draft.drop_forbidden_addons();
        let name = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(tr!("profiles.editor.name_placeholder"))
                .default_value(draft.name.clone())
        });
        // `set_value` emits no input event, so the name is read back on every redraw request.
        let mut subscriptions = vec![cx.observe(&name, |editor, _, cx| editor.sync_name(cx))];
        let editor = cx.entity();
        let crumb = cx.new(|cx| EditorCrumb {
            editor: editor.downgrade(),
            fit: Rc::default(),
            _observe: cx.observe(&editor, |_, _, cx| cx.notify()),
        });
        // Focus starts inside the editor, so Esc reaches its key context.
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let hit_select = cx.new(|cx| SelectState::new(Vec::new(), None, window, cx));
        let kill_select = cx.new(|cx| SelectState::new(Vec::new(), None, window, cx));
        subscriptions.push(on_sound_confirm(&hit_select, SoundKind::Hitsound, cx));
        subscriptions.push(on_sound_confirm(&kill_select, SoundKind::Killsound, cx));
        // The play buttons read the player's phase, and HUD pictures redraw as they land; the
        // editor's picture wants go when it does.
        let player = sound_player(window, cx);
        subscriptions.push(cx.observe(&player, |_, _, cx| cx.notify()));
        // The strip's selection and the board's pictures both redraw the Crosshairs body.
        let picker = cx.new(|cx| CrosshairPicker::strip(IdScope::PROFILE_CROSSHAIRS, window, cx));
        subscriptions.push(cx.observe(&picker, |_, _, cx| cx.notify()));
        subscriptions.push(cx.subscribe_in(
            &picker,
            window,
            |editor, _, _: &ImportCrosshair, window, cx| editor.import_crosshair(window, cx),
        ));
        let previews = CrosshairPreviews::global(cx);
        subscriptions.push(cx.observe(&previews, |_, _, cx| cx.notify()));
        // The Graphics widget has no Render of its own, so the editor redraws for it.
        let graphics = cx.new(|cx| {
            GraphicsChoicesEditor::new(
                IdScope::PROFILE_GRAPHICS,
                draft.graphics.clone(),
                window,
                cx,
            )
        });
        subscriptions.push(cx.observe(&graphics, |_, _, cx| cx.notify()));
        subscriptions.push(cx.subscribe_in(
            &graphics,
            window,
            |editor, _, edit: &ChoicesEdit, window, cx| editor.graphics_edit(edit, window, cx),
        ));
        let image_viewer = format!("profile-draft-editor:{}", cx.entity_id());
        if let Some(images) = RemoteImages::global(cx) {
            subscriptions.push(cx.observe(&images, |_, _, cx| cx.notify()));
            let released = image_viewer.clone();
            subscriptions.push(cx.on_release(move |_, cx| {
                images.update(cx, |images, cx| images.want(&released, Vec::new(), cx));
            }));
        }
        cx.defer_in(window, |editor, window, cx| {
            editor.load_library(window, cx);
            editor.load_graphics(window, cx);
        });
        Self {
            graphics,
            page_choices: None,
            narrow: false,
            library: None,
            library_generation: 0,
            player,
            picker,
            previews,
            card_width: px(0.),
            image_viewer,
            hit_select,
            kill_select,
            saved,
            baseline,
            draft,
            name,
            name_touched: false,
            name_taken: None,
            busy: false,
            page: None,
            focus,
            scroll: ScrollHandle::new(),
            cards_from: 1,
            crumb,
            _subscriptions: subscriptions,
        }
    }

    // The page that hosts the editor; set once, right after it is built.
    pub(super) fn set_page(&mut self, page: WeakEntity<ProfilesPage>) {
        self.page = Some(page);
    }

    /// The Sounds body's select for `kind`.
    pub fn sound_select(&self, kind: SoundKind) -> &Entity<SelectState<Vec<SoundOption>>> {
        if kind == SoundKind::Hitsound {
            &self.hit_select
        } else {
            &self.kill_select
        }
    }

    /// The Graphics body's choices editor, under `IdScope::PROFILE_GRAPHICS`.
    pub fn graphics(&self) -> &Entity<GraphicsChoicesEditor> {
        &self.graphics
    }

    /// The Crosshairs body's gallery strip.
    pub fn picker(&self) -> &Entity<CrosshairPicker> {
        &self.picker
    }

    pub fn draft(&self) -> &Draft {
        &self.draft
    }

    /// The saved profile's id; None for a new draft.
    pub fn profile_id(&self) -> Option<&ProfileId> {
        self.saved.as_ref().map(Profile::id)
    }

    /// The draft, with the name as typed, differs from what the editor opened with.
    pub fn is_dirty(&self) -> bool {
        self.draft.differs_from(&self.baseline)
    }

    /// Why Save is off: the first of a blank name, a taken name, HUD Set with no pick, or no
    /// change to a saved profile.
    pub fn save_block(&self, cx: &App) -> Option<SaveBlock> {
        let taken = self.name_taken.clone().or_else(|| {
            let page = self.page.as_ref()?.upgrade()?;
            page.read(cx)
                .taken_name(&self.draft.name, self.profile_id())
        });
        editor_model::save_block(&self.draft, &self.baseline, self.saved.is_some(), taken)
    }

    pub fn name_input(&self) -> &Entity<InputState> {
        &self.name
    }

    /// Focuses the name field and selects its text.
    pub fn focus_name(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.name.update(cx, |input, cx| {
            input.focus(window, cx);
            input.select_all(window, cx);
        });
    }

    /// Takes focus back to the editor unless it already holds it, so Esc answers again.
    pub fn reclaim_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus.contains_focused(window, cx) {
            self.focus.focus(window, cx);
        }
    }

    /// Switches `slot` to `mode` in the draft; false when the draft refuses it or a Save is
    /// running.
    pub fn set_mode(&mut self, slot: SlotKind, mode: Mode, cx: &mut Context<Self>) -> bool {
        if self.busy {
            return false;
        }
        let done = self.draft.set_mode(slot, mode);
        cx.notify();
        done
    }

    /// Applies `change` to the draft and redraws; refused while a Save is running, so the
    /// draft on screen is the one being written.
    pub fn edit(&mut self, change: impl FnOnce(&mut Draft), cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        change(&mut self.draft);
        cx.notify();
    }

    /// The header crumb: "Profiles / <name>", Cancel and Save.
    pub fn crumb(&self) -> AnyView {
        self.crumb.clone().into()
    }

    // Takes the typed name into the draft; any change counts as touching the field and drops
    // core's earlier NameTaken.
    fn sync_name(&mut self, cx: &mut Context<Self>) {
        let value = self.name.read(cx).value().to_string();
        if value != self.draft.name {
            self.draft.name = value;
            self.name_touched = true;
            self.name_taken = None;
            cx.notify();
        }
    }

    // The line under the name: why Save is off, except for an unchanged profile, and a blank
    // name only once the player has touched the field or tried Save.
    fn name_error(&self, cx: &App) -> Option<SharedString> {
        let block = self.save_block(cx)?;
        if block == SaveBlock::BlankName && !self.name_touched {
            return None;
        }
        block.message()
    }

    /// Writes the draft off the UI thread: `update_profile` for a saved profile, else
    /// `create_profile`, plus an activity row. A blocked Save only shows why.
    pub fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if self.save_block(cx).is_some() {
            self.name_touched = true;
            cx.notify();
            return;
        }
        self.set_busy(true, cx);
        let (draft, id) = (self.draft.to_profile_draft(), self.profile_id().cloned());
        let saving = run_blocking(cx, move |s| {
            let saved = match &id {
                Some(id) => (s.library.update_profile(id, &draft))
                    .map(|u| (u.profile().clone(), u.deactivated())),
                None => s.library.create_profile(&draft).map(|p| (p, false)),
            };
            if let Ok((profile, _)) = &saved {
                let message = tr!("profiles.activity.saved", name = profile.name());
                let entry = NewLogEntry::new(LogLevel::Info, "profile", "save", message);
                activity::write(s, &entry);
            }
            Ok(saved)
        });
        let win = window.window_handle();
        cx.spawn(async move |editor, cx| {
            let done = saving.await;
            let _ = cx.update(|cx| {
                win.update(cx, |_, window, cx| {
                    let _ = editor.update(cx, |e, cx| e.saved(done, window, cx));
                })
            });
        })
        .detach();
    }

    // Save's answer: the page closes on success; a taken name shows inline; a profile deleted
    // meanwhile toasts and closes; an item deleted meanwhile toasts and reloads the library, so
    // its tile goes; anything else toasts and stays.
    fn saved(
        &mut self,
        done: Result<Result<(Profile, bool), ProfileError>, OpError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_busy(false, cx);
        let failed =
            |e: &ProfileError| describe_profile(e).into_op(tr!("messages.action.save_profile"));
        match done {
            Ok(Ok((profile, deactivated))) => cx.emit(EditorEvent::Saved {
                profile: Box::new(profile),
                deactivated,
            }),
            Ok(Err(ProfileError::NameTaken { name })) => self.name_taken = Some(name),
            Ok(Err(e @ ProfileError::NotFound { .. })) => {
                toast_error(window, cx, &failed(&e));
                let name = self.saved.as_ref().map(|p| p.name().to_owned());
                cx.emit(EditorEvent::Deleted(name.unwrap_or_default()));
            }
            Ok(Err(e @ ProfileError::ItemNotFound { .. })) => {
                toast_error(window, cx, &failed(&e));
                self.load_library(window, cx);
            }
            Ok(Err(e)) => toast_error(window, cx, &failed(&e)),
            Err(err) => toast_error(window, cx, &err),
        }
        cx.notify();
    }

    // Holds every slot control still while a Save runs; the Graphics widget draws its own.
    fn set_busy(&mut self, busy: bool, cx: &mut Context<Self>) {
        self.busy = busy;
        (self.graphics).update(cx, |g, cx| g.set_disabled(busy, cx));
        cx.notify();
    }

    // The saved profile's Delete asks the page, which confirms and closes the editor on yes.
    fn delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(saved), Some(page)) = (&self.saved, self.page.as_ref()) else {
            return;
        };
        let (id, name) = (saved.id().clone(), saved.name().to_owned());
        let _ = page.update(cx, |page, cx| page.delete(id, &name, window, cx));
    }

    // A new draft's note under the name: where it was filled in from, and what it copied.
    fn new_draft_note(&self, cx: &App) -> Option<impl IntoElement> {
        if self.saved.is_some() {
            return None;
        }
        let (title, body) = (
            tr!("profiles.editor.note_title"),
            editor_model::new_draft_note(&self.baseline),
        );
        let theme = cx.theme();
        Some(
            v_flex()
                .id(ids::PROFILE_EDITOR_NOTE)
                .aria_label(SharedString::from(format!("{title}. {body}")))
                .test_support()
                .max_w(px(480.))
                .gap_1()
                .p_3()
                .rounded_md()
                .border_1()
                .border_color(theme.border)
                .bg(theme.muted)
                .text_sm()
                .child(div().font_weight(FontWeight::MEDIUM).child(title))
                .child(div().text_color(theme.muted_foreground).child(body)),
        )
    }

    // The Name label, field and the line saying why Save is off.
    fn name_field(&self, cx: &App) -> impl IntoElement {
        let theme = cx.theme();
        v_flex()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child(tr!("profiles.editor.name")),
            )
            .child(
                div()
                    .id(ids::PROFILE_EDITOR_NAME)
                    .test_support()
                    .max_w(px(480.))
                    .child(Input::new(&self.name).readonly(self.busy)),
            )
            .children(self.name_error(cx).map(|message| {
                labelled(
                    div().text_sm().text_color(theme.danger),
                    ids::PROFILE_EDITOR_NAME_ERROR,
                    message,
                )
            }))
    }

    // Scrolls the body so `slot`'s card is at the top; when narrow, just under the stuck rail
    // row (child 1), so its title shows.
    fn jump(&mut self, slot: SlotKind, cx: &mut Context<Self>) {
        let item = self.cards_from + slot as usize;
        let measured = (self.scroll.bounds_for_item(item)).zip(self.scroll.bounds_for_item(1));
        match measured {
            Some((card, rail)) if self.narrow => {
                let top = card.top() - self.scroll.bounds().top() - rail.size.height;
                let y = top.max(px(0.)).min(self.scroll.max_offset().y);
                let x = self.scroll.offset().x;
                self.scroll.set_offset(point(x, -y));
            }
            _ => self.scroll.scroll_to_top_of_item(item),
        }
        cx.notify();
    }

    // How far down its own spot in the scroll column (child `item`) the rail sits, so it stays
    // `inset` below the body's top while the cards scroll. The wide rail stops at the last
    // card's top; the narrow row, drawn unclipped, never stops, so it never leaves the body.
    fn rail_pin(&self, item: usize, inset: Pixels, stop_at_last: bool) -> Pixels {
        let last_card = self.cards_from + SlotKind::ALL.len() - 1;
        let first = self.scroll.bounds_for_item(item);
        let last = self.scroll.bounds_for_item(last_card);
        let (Some(first), Some(last)) = (first, last) else {
            return px(0.);
        };
        let top = first.top() + self.scroll.offset().y;
        let pin = (self.scroll.bounds().top() + inset - top).max(px(0.));
        if stop_at_last {
            pin.min(last.top() - first.top())
        } else {
            pin
        }
    }

    /// The slot rail: a link per slot with its mode's badge, in a column, or a wrapping row
    /// when `narrow`. A link scrolls to its card.
    fn rail(&self, narrow: bool, cx: &mut Context<Self>) -> AnyElement {
        let entries = SlotKind::ALL.map(|slot| {
            let button = Button::new(ids::profile_rail(slot.key()))
                .small()
                .accessibility_label(slot.label())
                .on_click(cx.listener(move |editor, _, _, cx| editor.jump(slot, cx)))
                .child(
                    h_flex()
                        .when(!narrow, |row| row.w_full())
                        .gap_2()
                        .child(div().flex_1().child(slot.label()))
                        .child(mode_badge(slot, self.draft.mode(slot), cx)),
                );
            if narrow {
                button.outline()
            } else {
                button.ghost().w_full()
            }
        });
        // The narrow row is drawn over the cards scrolling under it, so it carries the page's
        // background and keeps their clicks, but lets the wheel through to the body.
        let rail = if narrow {
            h_flex()
                .flex_wrap()
                .gap_2()
                .py_1()
                .bg(cx.theme().background)
        } else {
            v_flex().flex_none().w(RAIL_WIDTH).gap_1()
        };
        rail.id(ids::PROFILE_RAIL)
            .test_support()
            .when(narrow, |rail| rail.block_mouse_except_scroll())
            .children(entries)
            .into_any_element()
    }

    // The three-segment Set / None / Leave control: a radio group drawn as the Graphics pill,
    // so the chosen mode reports selected. A mode the draft would refuse (Add-ons Set while
    // Graphics is None) is drawn disabled rather than ignoring the click.
    fn mode_control(&self, slot: SlotKind, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.draft.mode(slot);
        let foreground = cx.theme().foreground;
        let segments = Mode::ALL.into_iter().enumerate().map(|(ix, mode)| {
            let checked = current == mode;
            let disabled = !self.draft.can_set(slot, mode) || self.busy;
            let editor = cx.entity().downgrade();
            let label = mode_label(mode);
            let segment = Radio::new(ids::profile_slot_mode(slot.key(), mode.key()))
                .accessibility_label(label.clone())
                .set_position(ix + 1, Mode::ALL.len())
                .checked(checked)
                .disabled(disabled)
                .on_change(move |_, _, _, cx| {
                    let _ = editor.update(cx, |e, cx| e.set_mode(slot, mode, cx));
                })
                .flex()
                .items_center()
                .h(px(26.))
                .px(px(10.))
                .text_size(px(13.))
                .when(disabled, |segment| segment.opacity(0.5))
                .when(!disabled && !checked, |segment| {
                    segment
                        .cursor_pointer()
                        .hover(move |style| style.text_color(foreground))
                })
                .child(label);
            pill::segment_look(segment, checked, cx)
        });
        pill::row_track(cx)
            .id(ids::profile_slot_modes(slot.key()))
            .role(Role::RadioGroup)
            .aria_label(tr!("profiles.editor.modes_label", slot = slot.label()))
            .test_support()
            .flex_none()
            .children(segments)
    }

    // A slot's card: a header of title, help and mode control over a divider, then the Set
    // body or the line saying what None or Leave does.
    fn slot_card(&self, slot: SlotKind, cx: &mut Context<Self>) -> AnyElement {
        let key = slot.key();
        let (muted, border) = (cx.theme().muted_foreground, cx.theme().border);
        let header = h_flex()
            .flex_wrap()
            .items_start()
            .justify_between()
            .gap_3()
            .px_4()
            .py(px(14.))
            .border_b_1()
            .border_color(border)
            .child(
                v_flex()
                    .flex_1()
                    .min_w(px(220.))
                    .gap_1()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(slot.label()))
                    .child(labelled(
                        div().text_sm().text_color(muted),
                        ids::profile_slot_help(key),
                        slot_help(slot),
                    )),
            )
            .child(self.mode_control(slot, cx));
        // Under the Add-ons header: why its Set segment is off.
        let need_graphics =
            (slot == SlotKind::Addons && self.draft.addons_need_graphics()).then(|| {
                labelled(
                    div().text_sm().text_color(muted),
                    ids::PROFILE_ADDONS_NEED_GRAPHICS,
                    tr!("profiles.editor.addons_need_graphics"),
                )
            });
        let body = match self.draft.mode(slot) {
            Mode::Set => self.set_body(slot, cx),
            mode @ (Mode::None | Mode::Leave) => labelled(
                div().text_sm().text_color(muted),
                ids::profile_slot_text(key),
                slot_text(slot, mode),
            )
            .into_any_element(),
        };
        card_frame(cx)
            .id(ids::profile_slot(key))
            .test_support()
            .flex()
            .flex_col()
            .child(header)
            .child(v_flex().gap_4().p_4().children(need_graphics).child(body))
            .into_any_element()
    }
}

// The scroll column's centred lane every body child sits in.
fn lane() -> Div {
    div()
        .flex_none()
        .w_full()
        .max_w(px(BODY_MAX_WIDTH))
        .mx_auto()
}

// "Set", "None", "Leave as is": the mode control's labels.
fn mode_label(mode: Mode) -> SharedString {
    match mode {
        Mode::Set => tr!("profiles.mode.set"),
        Mode::None => tr!("profiles.mode.none"),
        Mode::Leave => tr!("profiles.mode.leave"),
    }
}

// The rail's badge for `mode`: Set on the secondary fill, None and Left as is dashed and muted.
fn mode_badge(slot: SlotKind, mode: Mode, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let dashed = || {
        Tag::secondary()
            .outline()
            .border_dashed()
            .text_color(theme.muted_foreground)
    };
    let (tag, border, text) = match mode {
        Mode::Set => (
            Tag::secondary(),
            transparent_black(),
            tr!("profiles.mode.set"),
        ),
        Mode::None => (dashed(), theme.border, tr!("profiles.mode.none")),
        Mode::Leave => (dashed(), theme.border, tr!("profiles.card.leave")),
    };
    badge_of(ids::profile_rail_badge(slot.key()), tag, border, text)
}

// The card's help line under its title.
fn slot_help(slot: SlotKind) -> SharedString {
    match slot {
        SlotKind::Hud => tr!("profiles.slot.hud.help"),
        SlotKind::Sounds => tr!("profiles.slot.sounds.help"),
        SlotKind::Crosshairs => tr!("profiles.slot.crosshairs.help"),
        SlotKind::Graphics => tr!("profiles.slot.graphics.help"),
        SlotKind::Addons => tr!("profiles.slot.addons.help"),
    }
}

// What a None or Leave slot does on Apply, shown in place of its body.
fn slot_text(slot: SlotKind, mode: Mode) -> SharedString {
    match (slot, mode == Mode::None) {
        (SlotKind::Hud, true) => tr!("profiles.slot.hud.none"),
        (SlotKind::Hud, false) => tr!("profiles.slot.hud.leave"),
        (SlotKind::Sounds, true) => tr!("profiles.slot.sounds.none"),
        (SlotKind::Sounds, false) => tr!("profiles.slot.sounds.leave"),
        (SlotKind::Crosshairs, true) => tr!("profiles.slot.crosshairs.none"),
        (SlotKind::Crosshairs, false) => tr!("profiles.slot.crosshairs.leave"),
        (SlotKind::Graphics, true) => tr!("profiles.slot.graphics.none"),
        (SlotKind::Graphics, false) => tr!("profiles.slot.graphics.leave"),
        (SlotKind::Addons, true) => tr!("profiles.slot.addons.none"),
        (SlotKind::Addons, false) => tr!("profiles.slot.addons.leave"),
    }
}

impl Render for ProfileDraftEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = window.viewport_size().width - SHELL_CHROME_WIDTH;
        let narrow = content < RAIL_ROW_BELOW;
        self.narrow = narrow;
        // A card's lane (the content width, which already leaves out the body's padding) less
        // the side rail when it stands beside, then the card's own padding.
        let lane_width = content.min(px(BODY_MAX_WIDTH));
        let beside = if narrow {
            px(0.)
        } else {
            RAIL_WIDTH + RAIL_GUTTER
        };
        self.card_width = lane_width - beside - CARD_INSET * 2.;
        // Each card is a direct child of the scroll column so the rail can scroll to it by
        // index; the narrow rail row comes first, the wide rail hangs off the first card.
        self.cards_from = if narrow { 2 } else { 1 };
        let pin = self.rail_pin(self.cards_from, CONTENT_PADDING, true);
        let mut children: Vec<AnyElement> = vec![
            lane()
                .flex()
                .flex_col()
                .gap_6()
                .child(self.name_field(cx))
                .children(self.new_draft_note(cx))
                .into_any_element(),
        ];
        // The narrow row keeps its place in the column but sticks to the body's top once the
        // cards scroll past, painted after them so they pass underneath.
        if narrow {
            let stuck = div()
                .relative()
                .top(self.rail_pin(1, px(0.), false))
                .child(deferred(self.rail(true, cx)));
            children.push(lane().child(stuck).into_any_element());
        }
        for slot in SlotKind::ALL {
            let card = self.slot_card(slot, cx);
            let side = (!narrow && slot == SlotKind::Hud).then(|| {
                div()
                    .absolute()
                    .left_0()
                    .top(pin)
                    .child(self.rail(false, cx))
            });
            let lane = lane()
                .when(!narrow, |lane| lane.relative().pl(RAIL_WIDTH + RAIL_GUTTER))
                .child(card)
                .children(side);
            children.push(lane.into_any_element());
        }
        // Only a saved profile can be deleted; a new draft is dropped with Cancel.
        let delete = self.saved.is_some().then(|| {
            Button::new(ids::PROFILE_EDITOR_DELETE)
                .ghost()
                .label(tr!("profiles.editor.delete"))
                .text_color(cx.theme().danger)
                .disabled(self.busy)
                .on_click(cx.listener(|editor, _, window, cx| editor.delete(window, cx)))
        });
        children.extend(delete.map(|b| lane().flex().child(b).into_any_element()));
        let body = v_flex()
            .id(ids::PROFILE_EDITOR_BODY)
            .test_support()
            .size_full()
            .track_scroll(&self.scroll)
            .overflow_y_scroll()
            .p(CONTENT_PADDING)
            .gap_6()
            .children(children);
        v_flex()
            .id(ids::PROFILE_EDITOR)
            .test_support()
            .key_context(PROFILE_EDITOR_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(|_, _: &ProfileEditorCancel, _, cx| cx.emit(EditorEvent::Close)))
            .size_full()
            .child(scroll_area(ids::PROFILE_EDITOR_SCROLLBAR, &self.scroll, body).size_full())
    }
}

// The header slot's view: the "Profiles" link, the draft's name, Cancel and Save. It redraws
// with the editor it reads.
struct EditorCrumb {
    editor: WeakEntity<ProfileDraftEditor>,
    fit: Rc<CrumbFit>,
    _observe: Subscription,
}

// The painted widths that decide whether "Profiles /" fits. The link keeps its last width
// while hidden, so it comes back once the row has room for it again.
#[derive(Default)]
struct CrumbFit {
    row: Cell<Pixels>,
    nav: Cell<Pixels>,
    actions: Cell<Pixels>,
}

// The space between the crumb's parts.
const CRUMB_GAP: Pixels = px(8.);

impl CrumbFit {
    // The name shrinks to nothing first; only then does the link give way to Cancel and Save.
    fn shows_nav(&self) -> bool {
        self.nav.get() + self.actions.get() + CRUMB_GAP * 2. <= self.row.get()
    }
}

// Records its parent's painted width in `cell`; a change asks for another frame, since a
// refresh requested while drawing is dropped.
fn width_watch(
    cell: impl Fn(&CrumbFit) -> &Cell<Pixels> + 'static,
    fit: &Rc<CrumbFit>,
) -> impl IntoElement {
    let fit = Rc::clone(fit);
    canvas(
        move |bounds, window, _| {
            let cell = cell(&fit);
            if cell.get() != bounds.size.width {
                cell.set(bounds.size.width);
                window.request_animation_frame();
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_full()
}

impl Render for EditorCrumb {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(editor) = self.editor.upgrade() else {
            return h_flex().into_any_element();
        };
        let e = editor.read(cx);
        let name = match e.draft.name.trim() {
            "" => tr!("profiles.editor.crumb_new"),
            name => SharedString::from(name.to_owned()),
        };
        let (busy, blocked) = (e.busy, e.save_block(cx).is_some());
        let muted = cx.theme().muted_foreground;
        let foreground = cx.theme().foreground;
        let nav = Page::Profiles.label();
        let close = {
            let editor = editor.clone();
            move |_: &_, _: &mut Window, cx: &mut App| {
                editor.update(cx, |_, cx| cx.emit(EditorEvent::Close));
            }
        };
        let fit = &self.fit;
        let nav = fit.shows_nav().then(|| {
            h_flex()
                .relative()
                .flex_none()
                .gap(CRUMB_GAP)
                .child(width_watch(|f| &f.nav, fit))
                .child(
                    div()
                        .id(ids::PROFILE_EDITOR_CRUMB)
                        .aria_label(nav.clone())
                        .test_support()
                        .flex_none()
                        .cursor_pointer()
                        .text_color(muted)
                        .hover(move |link| link.text_color(foreground))
                        .child(nav)
                        .on_click(close.clone()),
                )
                .child(div().flex_none().text_color(muted).child("/"))
        });
        h_flex()
            .id(ids::PROFILE_EDITOR_HEADER)
            .aria_label(tr!("profiles.editor.header_label"))
            .test_support()
            .relative()
            .w_full()
            .min_w_0()
            .items_center()
            .gap(CRUMB_GAP)
            .text_lg()
            .child(width_watch(|f| &f.row, fit))
            .children(nav)
            .child(
                div()
                    .id(ids::PROFILE_EDITOR_CRUMB_NAME)
                    .aria_label(name.clone())
                    .test_support()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(name),
            )
            .child(
                h_flex()
                    .relative()
                    .flex_none()
                    .gap(CRUMB_GAP)
                    .child(width_watch(|f| &f.actions, fit))
                    .child(
                        Button::new(ids::PROFILE_EDITOR_CANCEL)
                            .ghost()
                            .flex_none()
                            .label(tr!("confirm.cancel"))
                            .on_click(close),
                    )
                    .child(
                        Button::new(ids::PROFILE_EDITOR_SAVE)
                            .primary()
                            .flex_none()
                            .label(tr!("profiles.editor.save"))
                            .loading(busy)
                            .disabled(busy || blocked)
                            .on_click(move |_, window, cx| {
                                editor.update(cx, |e, cx| e.save(window, cx));
                            }),
                    ),
            )
            .into_any_element()
    }
}
