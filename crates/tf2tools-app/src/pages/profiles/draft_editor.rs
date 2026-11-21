//! The profile draft editor: a header crumb with Cancel and Save, the Name field and the
//! Delete foot. Every change stays in the draft until Save writes it to the library.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, h_flex, v_flex};
use gpui_kit::{
    AnyView, App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, TestSupportExt as _, WeakEntity,
    Window, div, px,
};
use tf2tools_core::{LogLevel, NewLogEntry, Profile, ProfileError, ProfileId};

use super::ProfilesPage;
use super::editor_model::{self, Draft, Mode, SaveBlock, SlotKind};
use crate::activity;
use crate::i18n::tr;
use crate::ids;
use crate::keymap::{PROFILE_EDITOR_CONTEXT, ProfileEditorCancel};
use crate::local_messages::describe_profile;
use crate::services::{OpError, run_blocking};
use crate::shell::CONTENT_PADDING;
use crate::shell::route::Page;
use crate::shell::toast::toast_error;
use crate::widgets::labelled;
use crate::widgets::scroll::scroll_area;

/// The body's widest column, centred in a wider window.
const BODY_MAX_WIDTH: f32 = 1180.;

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

/// One profile's draft, edited in place of the Profiles list.
pub struct ProfileDraftEditor {
    saved: Option<Profile>,
    draft: Draft,
    baseline: Draft,
    name: Entity<InputState>,
    /// The name field was edited or Save was tried, so a blank name may say so.
    name_touched: bool,
    /// The name core refused as taken at Save, until the field changes.
    name_taken: Option<String>,
    busy: bool,
    /// The hosting page, whose loaded list answers the live name check.
    page: Option<WeakEntity<ProfilesPage>>,
    focus: FocusHandle,
    scroll: ScrollHandle,
    crumb: Entity<EditorCrumb>,
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
        let name = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(tr!("profiles.editor.name_placeholder"))
                .default_value(draft.name.clone())
        });
        // `set_value` emits no input event, so the name is read back on every redraw request.
        let subscriptions = vec![cx.observe(&name, |editor, _, cx| editor.sync_name(cx))];
        let editor = cx.entity();
        let crumb = cx.new(|cx| EditorCrumb {
            editor: editor.downgrade(),
            _observe: cx.observe(&editor, |_, _, cx| cx.notify()),
        });
        // Focus starts inside the editor, so Esc reaches its key context.
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        Self {
            saved,
            baseline: draft.clone(),
            draft,
            name,
            name_touched: false,
            name_taken: None,
            busy: false,
            page: None,
            focus,
            scroll: ScrollHandle::new(),
            crumb,
            _subscriptions: subscriptions,
        }
    }

    // The page that hosts the editor; set once, right after it is built.
    pub(super) fn set_page(&mut self, page: WeakEntity<ProfilesPage>) {
        self.page = Some(page);
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

    /// Switches `slot` to `mode` in the draft; false when the draft refuses it.
    pub fn set_mode(&mut self, slot: SlotKind, mode: Mode, cx: &mut Context<Self>) -> bool {
        let done = self.draft.set_mode(slot, mode);
        cx.notify();
        done
    }

    /// Applies `change` to the draft and redraws.
    pub fn edit(&mut self, change: impl FnOnce(&mut Draft), cx: &mut Context<Self>) {
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
        self.busy = true;
        cx.notify();
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
    // meanwhile toasts and closes; anything else toasts and stays.
    fn saved(
        &mut self,
        done: Result<Result<(Profile, bool), ProfileError>, OpError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.busy = false;
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
            Ok(Err(e)) => toast_error(window, cx, &failed(&e)),
            Err(err) => toast_error(window, cx, &err),
        }
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
                    .child(Input::new(&self.name)),
            )
            .children(self.name_error(cx).map(|message| {
                labelled(
                    div().text_sm().text_color(theme.danger),
                    ids::PROFILE_EDITOR_NAME_ERROR,
                    message,
                )
            }))
    }
}

impl Render for ProfileDraftEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Only a saved profile can be deleted; a new draft is dropped with Cancel.
        let delete = self.saved.is_some().then(|| {
            Button::new(ids::PROFILE_EDITOR_DELETE)
                .ghost()
                .label(tr!("profiles.editor.delete"))
                .text_color(cx.theme().danger)
                .disabled(self.busy)
                .on_click(cx.listener(|editor, _, window, cx| editor.delete(window, cx)))
        });
        let body = div()
            .id(ids::PROFILE_EDITOR_BODY)
            .test_support()
            .size_full()
            .track_scroll(&self.scroll)
            .overflow_y_scroll()
            .p(CONTENT_PADDING)
            .child(
                v_flex()
                    .w_full()
                    .max_w(px(BODY_MAX_WIDTH))
                    .mx_auto()
                    .gap_6()
                    .child(self.name_field(cx))
                    .children(delete.map(|b| h_flex().child(b))),
            );
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
    _observe: Subscription,
}

impl Render for EditorCrumb {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(editor) = self.editor.upgrade() else {
            return h_flex();
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
        h_flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_2()
            .text_lg()
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
            )
    }
}
