//! The Demo Viewer bar's camera chip: the mode, the previous and next player, the followed
//! player's name and a dead badge while that player waits to respawn. The name opens the
//! players list, RED then BLU, where a pick follows that player.

use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    Anchor, AnyElement, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    Pixels, SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _,
    canvas, div, px,
};
use tf2tools_core::{CameraMode, DemoTracks, FollowPose, PlayerId, Team};

use super::DemoViewerPage;
use crate::i18n::tr;
use crate::ids;
use crate::viewer::viewport::{CameraState, DemoViewport};
use crate::widgets::{Named as _, labelled};

/// The widest the target's name draws before it ends in an ellipsis.
const TARGET_CAP: f32 = 160.;
/// The tallest the players list grows before it scrolls.
const PLAYERS_CAP: f32 = 320.;
/// The gap between the players list's surface and the bar's top.
const POPOVER_GAP: Pixels = px(4.);
/// The popover surface's own padding around the list, the kit's `p_3`.
const SURFACE_PAD: Pixels = px(12.);
/// The room the list's surface keeps from the page's top.
const PAGE_MARGIN: Pixels = px(8.);

/// One players-list row: who, their team at the shown tick and whether they are dead there.
struct PlayerRow {
    id: PlayerId,
    red: bool,
    dead: bool,
}

/// The players present at `tick`, RED first then BLU, each in entity order.
fn player_rows(tracks: &DemoTracks, tick: f64) -> Vec<PlayerRow> {
    let mut rows: Vec<PlayerRow> = tracks
        .players()
        .iter()
        .filter_map(|p| {
            let (pose, dead) = match tracks.eye_at(p.id(), tick) {
                FollowPose::Alive(pose) => (pose, false),
                FollowPose::Dead(pose) => (pose, true),
                FollowPose::Absent | _ => return None,
            };
            let red = pose.team() == Team::Red;
            Some(PlayerRow {
                id: p.id(),
                red,
                dead,
            })
        })
        .collect();
    rows.sort_by_key(|r| (!r.red, r.id.entity()));
    rows
}

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
    /// The bar's camera row for `viewport`; its buttons wait, disabled, for the tracks.
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
            .on_click(cx.listener(|page, _, window, cx| {
                page.on_viewport(window, cx, |v, cx| v.cycle_camera(cx));
            }));
        let prev = Button::new(ids::DEMO_VIEWER_CAMERA_TARGET_PREV)
            .ghost()
            .small()
            .flex_none()
            .icon(IconName::ChevronLeft)
            .named(tr!("demo_viewer.camera.prev"))
            .disabled(waiting)
            .on_click(cx.listener(|page, _, window, cx| {
                page.on_viewport(window, cx, |v, cx| v.prev_target(cx));
            }));
        let next = Button::new(ids::DEMO_VIEWER_CAMERA_TARGET_NEXT)
            .ghost()
            .small()
            .flex_none()
            .icon(IconName::ChevronRight)
            .named(tr!("demo_viewer.camera.next"))
            .disabled(waiting)
            .on_click(cx.listener(|page, _, window, cx| {
                page.on_viewport(window, cx, |v, cx| v.next_target(cx));
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
            .max_w(px(TARGET_CAP))
            .tooltip(tr!("demo_viewer.camera.choose"))
            .accessibility_label(heard)
            .disabled(waiting);
        let target_button = self.players_list(viewport, target_button, cx);
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
        let chip_top = Rc::clone(&self.chip_top);
        let row_watch = canvas(
            move |row, window, _| {
                if chip_top.get() != row.top() {
                    chip_top.set(row.top());
                    window.request_animation_frame();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        h_flex()
            .relative()
            .w_full()
            .gap_0p5()
            .items_center()
            .child(row_watch)
            .child(mode)
            .child(prev)
            .child(target_button)
            .children(dead)
            .child(next)
            .into_any_element()
    }

    /// The target button as the trigger of the players list, opening upward from the bar.
    fn players_list(
        &self,
        viewport: &Entity<DemoViewport>,
        trigger: Button,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let v = viewport.read(cx);
        let rows = match (v.tracks(), v.playback()) {
            (Some(tracks), Some(p)) if self.players_open => player_rows(tracks, p.tick),
            _ => Vec::new(),
        };
        let tracks = v.tracks().cloned();
        let muted = cx.theme().muted_foreground;
        let header = |text: &'static str| div().pt_1().text_xs().text_color(muted).child(text);
        // The list opens from the bar's top rather than the chip's, and takes only the room
        // between the page's top and the bar, so a full server scrolls instead of covering it.
        let lift = (self.chip_top.get() - self.bar_top.get()).max(px(0.));
        let bar_at = self.bar.clamp(self.area.get(), self.bar_size.get()).y;
        let room = bar_at - POPOVER_GAP - SURFACE_PAD * 2. - PAGE_MARGIN;
        let mut list = v_flex()
            .id(ids::DEMO_VIEWER_PLAYERS)
            .test_support()
            .min_w(px(160.))
            .max_h(room.clamp(px(0.), px(PLAYERS_CAP)))
            .overflow_y_scroll()
            .gap_0p5();
        // "RED" and "BLU" are the game's own names, so they stay untranslated.
        for (red, title) in [(true, "RED"), (false, "BLU")] {
            let group: Vec<&PlayerRow> = rows.iter().filter(|r| r.red == red).collect();
            if group.is_empty() {
                continue;
            }
            list = list.child(header(title));
            for row in group {
                let name = target_words(Some(row.id), tracks.as_deref());
                let heard = match row.dead {
                    true => tr!("demo_viewer.camera.target_dead", name = name.clone()),
                    false => name.clone(),
                };
                let id = row.id;
                let pick = Button::new(ids::demo_viewer_player(id.entity()))
                    .ghost()
                    .small()
                    .label(name)
                    .accessibility_label(heard)
                    .on_click(cx.listener(move |page, _, window, cx| {
                        page.players_open = false;
                        page.on_viewport(window, cx, |v, cx| v.follow(id, cx));
                        cx.notify();
                    }));
                let badge = row.dead.then(|| {
                    div()
                        .flex_none()
                        .text_xs()
                        .text_color(muted)
                        .child(tr!("demo_viewer.camera.dead"))
                });
                list = list.child(h_flex().gap_1().items_center().child(pick).children(badge));
            }
        }
        let page = cx.entity().downgrade();
        Popover::new(ids::DEMO_VIEWER_PLAYERS_POPOVER)
            .anchor(Anchor::BottomLeft)
            .offset(lift + POPOVER_GAP)
            .open(self.players_open)
            .on_open_change(move |&open, _, cx| {
                let _ = page.update(cx, |page, cx| {
                    page.players_open = open;
                    cx.notify();
                });
            })
            .trigger(trigger)
            .child(list)
            .into_any_element()
    }
}
