//! The full-page profile editor: a header with Back, the name, Activate and Delete.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, Styled as _, Subscription, TestSupportExt as _, WeakEntity, Window, div,
    transparent_black,
};
use tf2tools_core::{Profile, ProfileId};

use super::ProfilesPage;
use crate::ids;
use crate::pages::common::{SM_ICON_PX, badge_of};
use crate::services::tf2_state;
use crate::widgets::Named as _;

/// One profile, edited in place of the Profiles grid.
pub struct ProfileEditor {
    page: WeakEntity<ProfilesPage>,
    saved: Profile,
    name: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

impl ProfileEditor {
    /// Subscribes to the page (redraw).
    pub fn new(
        profile: Profile,
        page: WeakEntity<ProfilesPage>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let initial = profile.name().to_owned();
        let name = cx.new(|cx| InputState::new(window, cx).default_value(initial));
        // Active, busy and the activating id live on the page, so its changes redraw the header.
        let subscriptions = page
            .upgrade()
            .map(|page| cx.observe(&page, |_, _, cx| cx.notify()))
            .into_iter()
            .collect();
        Self {
            page,
            saved: profile,
            name,
            _subscriptions: subscriptions,
        }
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

    /// No job queued or running.
    pub fn is_idle(&self) -> bool {
        true
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
        let status = if active {
            badge_of(
                ids::PROFILE_EDITOR_ACTIVE_BADGE,
                Tag::primary(),
                transparent_black(),
                "Active",
            )
            .into_any_element()
        } else {
            Button::new(ids::PROFILE_EDITOR_ACTIVATE)
                .outline()
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
                div()
                    .id(ids::PROFILE_EDITOR_NAME)
                    .test_support()
                    .flex_1()
                    .child(Input::new(&self.name)),
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
