//! The full-page profile editor: a header with Back, the name, Activate and Delete, and a queue
//! that saves each change in order.

use std::collections::VecDeque;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
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
    Saved(Result<ProfileUpdate, ProfileError>),
    Refreshed(Result<(Profile, Vec<MissingItem>), ProfileError>),
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
                    Ok(Done::Saved(s.library.update_profile(&id, &draft)))
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
                    });
                })
            });
        })
        .detach();
    }

    fn finish(&mut self, done: Result<Done, OpError>, window: &mut Window, cx: &mut Context<Self>) {
        let failed = |e: &ProfileError| describe_profile(e).into_op("Failed to save profile");
        match done {
            Ok(Done::Saved(Ok(update))) => {
                self.saved = update.profile().clone();
                if update.deactivated() {
                    self.not_applied = true;
                }
                let _ = self.page.update(cx, |page, cx| page.reload(cx));
            }
            Ok(Done::Saved(Err(e @ ProfileError::NotFound { .. }))) => {
                self.gone(Some(failed(&e)), window, cx);
            }
            Ok(Done::Refreshed(Err(ProfileError::NotFound { .. }))) => self.gone(None, window, cx),
            Ok(Done::Saved(Err(e))) => {
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

    fn back(&mut self, cx: &mut Context<Self>) {
        let _ = self.page.update(cx, |page, cx| page.close_editor(cx));
    }

    fn activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.id().clone();
        let _ = self
            .page
            .update(cx, |page, cx| page.activate(id, window, cx));
    }

    fn delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (id, name) = (self.id().clone(), self.saved.name().to_owned());
        let _ = self
            .page
            .update(cx, |page, cx| page.delete(id, &name, window, cx));
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
            .on_click(cx.listener(|editor, _, window, cx| editor.activate(window, cx)))
            .into_any_element()
        };
        h_flex()
            .gap_3()
            .child(
                Button::new(ids::PROFILE_EDITOR_BACK)
                    .ghost()
                    .icon(IconName::ArrowLeft)
                    .named("Back to profiles")
                    .on_click(cx.listener(|editor, _, _, cx| editor.back(cx))),
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
                    .on_click(cx.listener(|editor, _, window, cx| editor.delete(window, cx))),
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
