//! The Demo Viewer bar's camera chip: the mode, the previous and next player, the followed
//! player's name and a dead badge while that player waits to respawn.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex};
use gpui_kit::{
    AnyElement, Context, Entity, IntoElement, ParentElement as _, SharedString, Styled as _, div,
    px,
};
use tf2tools_core::{CameraMode, DemoTracks, PlayerId};

use super::DemoViewerPage;
use crate::i18n::tr;
use crate::ids;
use crate::viewer::viewport::{CameraState, DemoViewport};
use crate::widgets::{Named as _, labelled};

/// The widest the target's name draws; a narrow bar shrinks it further, to an ellipsis.
const TARGET_CAP: f32 = 128.;

fn mode_icon(mode: CameraMode) -> IconName {
    match mode {
        CameraMode::FirstPerson => IconName::Eye,
        CameraMode::Chase => IconName::Video,
        CameraMode::FreeFly | _ => IconName::Move,
    }
}

fn mode_words(mode: CameraMode) -> SharedString {
    match mode {
        CameraMode::FirstPerson => tr!("demo_viewer.camera.first_person"),
        CameraMode::Chase => tr!("demo_viewer.camera.chase"),
        CameraMode::FreeFly | _ => tr!("demo_viewer.camera.free"),
    }
}

/// The target's name from the tracks, its entity when it has none, or "No player".
fn target_words(target: Option<PlayerId>, tracks: Option<&DemoTracks>) -> SharedString {
    let Some(id) = target else {
        return tr!("demo_viewer.camera.none");
    };
    let name = tracks.and_then(|t| t.players().iter().find(|p| p.id() == id)?.name());
    match name {
        Some(name) => SharedString::from(name.to_owned()),
        None => tr!("demo_viewer.camera.unnamed", entity = id.entity()),
    }
}

impl DemoViewerPage {
    /// The camera chip for `viewport`; its buttons wait, disabled, for the tracks.
    pub(super) fn camera_chip(
        &self,
        viewport: &Entity<DemoViewport>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let v = viewport.read(cx);
        let waiting = v.playback().is_none();
        let state: CameraState = v.camera_state();
        let target = target_words(state.target, v.tracks().map(|t| &**t));
        let mode = Button::new(ids::DEMO_VIEWER_CAMERA_MODE)
            .ghost()
            .small()
            .flex_none()
            .icon(mode_icon(state.mode))
            .named(tr!(
                "demo_viewer.camera.mode",
                mode = mode_words(state.mode)
            ))
            .disabled(waiting)
            .on_click(cx.listener(|page, _, _, cx| {
                page.on_viewport(cx, |v, cx| v.cycle_camera(cx));
            }));
        let prev = Button::new(ids::DEMO_VIEWER_CAMERA_TARGET_PREV)
            .ghost()
            .small()
            .flex_none()
            .icon(IconName::ChevronLeft)
            .named(tr!("demo_viewer.camera.prev"))
            .disabled(waiting)
            .on_click(cx.listener(|page, _, _, cx| {
                page.on_viewport(cx, |v, cx| v.prev_target(cx));
            }));
        let next = Button::new(ids::DEMO_VIEWER_CAMERA_TARGET_NEXT)
            .ghost()
            .small()
            .flex_none()
            .icon(IconName::ChevronRight)
            .named(tr!("demo_viewer.camera.next"))
            .disabled(waiting)
            .on_click(cx.listener(|page, _, _, cx| {
                page.on_viewport(cx, |v, cx| v.next_target(cx));
            }));
        // A screen reader hears the death in the name, as the badge beside it is only seen.
        let heard = match state.dead {
            true => tr!("demo_viewer.camera.target_dead", name = target.clone()),
            false => target.clone(),
        };
        let target_button = Button::new(ids::DEMO_VIEWER_CAMERA_TARGET)
            .ghost()
            .small()
            .label(target)
            .min_w_0()
            .max_w(px(TARGET_CAP))
            .tooltip(tr!("demo_viewer.camera.choose"))
            .accessibility_label(heard)
            .disabled(waiting);
        let dead = state.dead.then(|| {
            let theme = cx.theme();
            labelled(
                div()
                    .flex_none()
                    .px_1p5()
                    .rounded_sm()
                    .whitespace_nowrap()
                    .text_xs()
                    .bg(theme.danger)
                    .text_color(theme.danger_foreground),
                ids::DEMO_VIEWER_CAMERA_DEAD,
                tr!("demo_viewer.camera.dead"),
            )
        });
        h_flex()
            .min_w_0()
            .gap_0p5()
            .items_center()
            .child(mode)
            .child(prev)
            .child(target_button)
            .child(next)
            .children(dead)
            .into_any_element()
    }
}
