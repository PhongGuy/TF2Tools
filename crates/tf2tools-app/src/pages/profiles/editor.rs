//! The full-page profile editor: a header with Back, the name, Activate and Delete, and a queue
//! that saves each change in order.

use std::collections::VecDeque;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, h_flex, v_flex};
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, Subscription, TestSupportExt as _, WeakEntity, Window,
    div, transparent_black,
};
use tf2tools_core::{MissingItem, Profile, ProfileError, ProfileId, ProfileUpdate};

use super::ProfilesPage;
use super::editor_model::Edit;
use super::model;
use crate::ids;
use crate::library_events::{LibraryChanged, LibraryEvents};
use crate::local_messages::describe_profile;
use crate::pages::common::{SM_ICON_PX, badge_of};
use crate::services::{OpError, run_blocking, tf2_state};
use crate::shell::toast::toast_error;
use crate::widgets::{Named as _, labelled};

enum Job {
    Edit(Edit),
    Refresh,
}

enum Done {
    Saved(Edit, Result<ProfileUpdate, ProfileError>),
    Refreshed(Result<(Profile, Vec<MissingItem>), ProfileError>),
}

/// What Back, Activate or Delete does once the queue has saved a typed name.
#[derive(Clone, Copy)]
enum AfterIdle {
    Back,
    Activate,
    Delete,
}

/// One profile, edited in place of the Profiles grid.
pub struct ProfileEditor {
    page: WeakEntity<ProfilesPage>,
    win: AnyWindowHandle,
    saved: Profile,
    missing: Vec<MissingItem>,
    not_applied: bool,
    queue: VecDeque<Job>,
    running: bool,
    name: Entity<InputState>,
    /// The other profile's name, when core refused a rename as taken.
    name_taken: Option<String>,
    /// The rename queued or running, so Enter then Blur sends it once.
    renaming: Option<String>,
    after_idle: Option<AfterIdle>,
    _subscriptions: Vec<Subscription>,
}

impl ProfileEditor {
    /// Subscribes to the page (redraw) and to `LibraryEvents` (a Refresh); queues a Refresh.
    pub fn new(
        profile: Profile,
        page: WeakEntity<ProfilesPage>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let initial = profile.name().to_owned();
        let name = cx.new(|cx| InputState::new(window, cx).default_value(initial));
        // Active, busy and the activating id live on the page, so its changes redraw the header;
        // once the profile is active again, its edits count as applied.
        let mut subscriptions: Vec<Subscription> = page
            .upgrade()
            .map(|page| {
                cx.observe(&page, |editor, page, cx| {
                    if page.read(cx).active() == Some(editor.id()) {
                        editor.not_applied = false;
                    }
                    cx.notify();
                })
            })
            .into_iter()
            .collect();
        // gpui-kit owns InputEvent; Enter and leaving the field save the name.
        #[allow(clippy::wildcard_enum_match_arm)]
        subscriptions.push(cx.subscribe_in(
            &name,
            window,
            |editor, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } | InputEvent::Blur => editor.commit_name(window, cx),
                InputEvent::Change => {
                    editor.name_taken = None;
                    cx.notify();
                }
                _ => {}
            },
        ));
        let events = LibraryEvents::global(cx);
        subscriptions.push(cx.subscribe(&events, |editor, _, _: &LibraryChanged, cx| {
            editor.enqueue_job(Job::Refresh, cx)
        }));
        let mut editor = Self {
            page,
            win: window.window_handle(),
            saved: profile,
            missing: Vec::new(),
            not_applied: false,
            queue: VecDeque::new(),
            running: false,
            name,
            name_taken: None,
            renaming: None,
            after_idle: None,
            _subscriptions: subscriptions,
        };
        editor.enqueue_job(Job::Refresh, cx);
        editor
    }

    pub fn id(&self) -> &ProfileId {
        self.saved.id()
    }

    /// The profile as last saved or reloaded.
    pub fn saved(&self) -> &Profile {
        &self.saved
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

    /// An edit ended this profile's active state and it has not been active since.
    pub fn not_applied(&self) -> bool {
        self.not_applied
    }

    pub fn missing(&self) -> &[MissingItem] {
        &self.missing
    }

    /// No job queued or running.
    pub fn is_idle(&self) -> bool {
        self.queue.is_empty() && !self.running
    }

    /// Queues a change; jobs run one at a time, in order.
    pub fn enqueue(&mut self, edit: Edit, cx: &mut Context<Self>) {
        self.enqueue_job(Job::Edit(edit), cx);
    }

    fn enqueue_job(&mut self, job: Job, cx: &mut Context<Self>) {
        self.queue.push_back(job);
        self.pump(cx);
    }

    // Starts the next job unless one runs. An edit's draft is built here, when it starts, from
    // `saved`, which the job before it has already replaced, so a queued click never drops an
    // earlier one. Refresh reads the missing marks first because that read may rewrite a folder.
    fn pump(&mut self, cx: &mut Context<Self>) {
        if self.running {
            return;
        }
        let Some(job) = self.queue.pop_front() else {
            cx.notify();
            return;
        };
        self.running = true;
        let id = self.saved.id().clone();
        let task = match job {
            Job::Edit(edit) => {
                let draft = edit.draft(&self.saved);
                run_blocking(cx, move |s| {
                    Ok(Done::Saved(edit, s.library.update_profile(&id, &draft)))
                })
            }
            Job::Refresh => run_blocking(cx, move |s| {
                let read = || -> Result<_, ProfileError> {
                    let missing = s.library.missing_items(&id)?;
                    Ok((s.library.profile(&id)?, missing))
                };
                Ok(Done::Refreshed(read()))
            }),
        };
        let win = self.win;
        cx.spawn(async move |editor, cx| {
            let done = task.await;
            let _ = cx.update(|cx| {
                win.update(cx, |_, window, cx| {
                    let _ = editor.update(cx, |editor, cx| {
                        editor.running = false;
                        editor.finish(done, window, cx);
                        editor.pump(cx);
                        if editor.is_idle() {
                            editor.run_after_idle(window, cx);
                        }
                    });
                })
            });
        })
        .detach();
    }

    fn finish(&mut self, done: Result<Done, OpError>, window: &mut Window, cx: &mut Context<Self>) {
        let failed = |e: &ProfileError| describe_profile(e).into_op("Failed to save profile");
        match done {
            Ok(Done::Saved(edit, Ok(update))) => {
                self.saved = update.profile().clone();
                if let Edit::Rename(_) = edit {
                    self.renamed(window, cx);
                }
                if update.deactivated() {
                    self.not_applied = true;
                }
                let _ = self.page.update(cx, |page, cx| page.reload(cx));
            }
            Ok(Done::Saved(_, Err(e @ ProfileError::NotFound { .. }))) => {
                self.gone(Some(failed(&e)), window, cx);
            }
            Ok(Done::Refreshed(Err(ProfileError::NotFound { .. }))) => self.gone(None, window, cx),
            // The page's list was stale: say it as the typed check would, and stay put.
            Ok(Done::Saved(Edit::Rename(_), Err(ProfileError::NameTaken { name, .. }))) => {
                self.renaming = None;
                self.name_taken = Some(name);
                self.after_idle = None;
                self.queue.push_back(Job::Refresh);
            }
            Ok(Done::Saved(edit, Err(e))) => {
                if let Edit::Rename(_) = edit {
                    self.renaming = None;
                }
                toast_error(window, cx, &failed(&e));
                self.queue.push_back(Job::Refresh);
            }
            Ok(Done::Refreshed(Ok((profile, missing)))) => {
                self.saved = profile;
                self.missing = missing;
            }
            Ok(Done::Refreshed(Err(e))) => {
                toast_error(
                    window,
                    cx,
                    &describe_profile(&e).into_op("Failed to load profile"),
                );
            }
            Err(err) => toast_error(window, cx, &err),
        }
        cx.notify();
    }

    // The profile no longer exists: drop the rest of the queue and, while the page still shows
    // this editor, toast `err` and return to the grid. After the editor's own Delete the page has
    // already toasted and is closing it, so nothing more is said.
    fn gone(&mut self, err: Option<OpError>, window: &mut Window, cx: &mut Context<Self>) {
        self.queue.clear();
        self.after_idle = None;
        let me = cx.entity();
        let Some(page) = self.page.upgrade() else {
            return;
        };
        if page.read(cx).editor().as_ref() != Some(&me) {
            return;
        }
        if let Some(err) = err {
            toast_error(window, cx, &err);
        }
        page.update(cx, |page, cx| page.close_editor(cx));
    }

    // A saved rename: clear the taken line and show the trimmed name, unless the player has
    // typed something else since.
    fn renamed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.renaming = None;
        self.name_taken = None;
        let name = self.saved.name().to_owned();
        if self.name.read(cx).value().trim() == name {
            self.name
                .update(cx, |input, cx| input.set_value(name, window, cx));
        }
    }

    // The typed name, trimmed: blank puts the saved name back, a name another loaded profile
    // holds shows the taken line, anything new is queued as a rename.
    fn commit_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.name.read(cx).value().trim().to_owned();
        if text.is_empty() {
            let name = self.saved.name().to_owned();
            self.name
                .update(cx, |input, cx| input.set_value(name, window, cx));
            return;
        }
        if text == self.saved.name() && self.renaming.is_none()
            || self.renaming.as_deref() == Some(text.as_str())
        {
            return;
        }
        if self.taken_name(cx).is_some() {
            cx.notify();
            return;
        }
        self.renaming = Some(text.clone());
        self.enqueue(Edit::Rename(text), cx);
    }

    // The other profile holding the typed name: core's answer, or the page's loaded list.
    fn taken_name(&self, cx: &Context<Self>) -> Option<String> {
        self.name_taken.clone().or_else(|| {
            let page = self.page.upgrade()?;
            page.read(cx)
                .taken_name(&self.name.read(cx).value(), Some(self.id()))
        })
    }

    // A kit Button never blurs the name field, so Back, Activate and Delete save a typed name
    // first and act once the queue is idle.
    fn leave(&mut self, then: AfterIdle, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_name(window, cx);
        self.after_idle = Some(then);
        if self.is_idle() {
            self.run_after_idle(window, cx);
        }
    }

    fn run_after_idle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(then) = self.after_idle.take() else {
            return;
        };
        let (id, name) = (self.id().clone(), self.saved.name().to_owned());
        let _ = self.page.update(cx, |page, cx| match then {
            AfterIdle::Back => page.close_editor(cx),
            AfterIdle::Activate => page.activate(id, window, cx),
            AfterIdle::Delete => page.delete(id, &name, window, cx),
        });
    }

    // Back, the name field, then Activate (or the Active badge) and Delete.
    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let has_tf2 = tf2_state(cx).read(cx).install().is_some();
        let page = self.page.upgrade();
        let page = page.as_ref().map(|p| p.read(cx));
        let id = self.id();
        let active = page.is_some_and(|p| p.active() == Some(id));
        let busy = page.is_some_and(|p| p.is_busy());
        let activating = page.is_some_and(|p| p.activating() == Some(id));
        let not_applied = self.not_applied && !active;
        let taken = self.taken_name(cx);
        let status = if active {
            badge_of(
                ids::PROFILE_EDITOR_ACTIVE_BADGE,
                Tag::primary(),
                transparent_black(),
                "Active",
            )
            .into_any_element()
        } else {
            let button = Button::new(ids::PROFILE_EDITOR_ACTIVATE);
            if not_applied {
                button.primary()
            } else {
                button.outline()
            }
            .px(SM_ICON_PX)
            .icon(IconName::Play)
            .label(if activating {
                "Activating..."
            } else {
                "Activate"
            })
            .loading(activating)
            .disabled(!has_tf2 || busy)
            .on_click(
                cx.listener(|editor, _, window, cx| editor.leave(AfterIdle::Activate, window, cx)),
            )
            .into_any_element()
        };
        h_flex()
            .gap_3()
            .child(
                Button::new(ids::PROFILE_EDITOR_BACK)
                    .ghost()
                    .icon(IconName::ArrowLeft)
                    .named("Back to profiles")
                    .on_click(cx.listener(|editor, _, window, cx| {
                        editor.leave(AfterIdle::Back, window, cx)
                    })),
            )
            .child(
                v_flex()
                    .flex_1()
                    .gap_1()
                    .child(
                        div()
                            .id(ids::PROFILE_EDITOR_NAME)
                            .test_support()
                            .child(Input::new(&self.name)),
                    )
                    .children(taken.map(|name| {
                        labelled(
                            div().text_xs().text_color(cx.theme().danger),
                            ids::PROFILE_EDITOR_NAME_TAKEN,
                            model::taken_message(&name),
                        )
                    }))
                    .children(not_applied.then(|| {
                        labelled(
                            div().text_xs().text_color(cx.theme().muted_foreground),
                            ids::PROFILE_EDITOR_NOT_APPLIED,
                            "Changes not applied yet",
                        )
                    })),
            )
            .child(status)
            .child(
                Button::new(ids::PROFILE_EDITOR_DELETE)
                    .outline()
                    .icon(IconName::Trash)
                    .named("Delete profile")
                    .disabled(busy)
                    .on_click(cx.listener(|editor, _, window, cx| {
                        editor.leave(AfterIdle::Delete, window, cx)
                    })),
            )
    }
}

impl Render for ProfileEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .id(ids::PROFILE_EDITOR)
            .test_support()
            .gap_6()
            .child(self.header(cx))
    }
}
