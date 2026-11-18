//! The HUD Maker's floating panels: the side panel with the selection's rows and the parts
//! panel with the part list, each a title bar that drags, docks and collapses it over its body.

use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::color_picker::ColorPicker;
use gpui_kit::component::input::Input;
use gpui_kit::component::popover::Popover;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Selectable as _, Sizable as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Anchor, AnyElement, Context, CursorStyle, DispatchPhase, Div, ElementId, FontWeight, Hsla,
    InteractiveElement as _, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ParentElement as _, Pixels, Point, SharedString, StatefulInteractiveElement as _,
    Styled, TestSupportExt as _, Window, canvas, div, point, px,
};
use tf2tools_core::{
    ALIGNMENTS, CLONE_PREFIX, ClassSchema, EditRefused, FontEdit, FontVariant, HudModel, NewPart,
    NoMirror, PanelRule, Part, PartCaps, PinCorner, PropertyKind, PropertySpec, Scene,
    paints_background,
};

use super::canvas::{colour, scene_note};
use super::editor::{Align, HudEditor, Spread};
use super::parts::row_label;
use super::tip::{Spot, record, tip_beside_row, tipped};
use super::view::{Panel, PanelMode, PanelPlace};
use super::{Field, FontList, HudMakerPage};
use crate::i18n::names::maker_label;
use crate::i18n::{plural, tr};
use crate::ids;
use crate::pages::crosshair_maker::model::hsla_of_rgba;
use crate::widgets::Named as _;
use crate::widgets::labelled;
use crate::widgets::scroll::{BAR_WIDTH, scroll_area};
use crate::widgets::tip::tip;

// The prompt with nothing selected, naming the scene whose tab is open.
fn empty_hint(scene: Option<Scene>) -> SharedString {
    match scene {
        Some(Scene::MainMenu) => tr!("hud_maker.panel.empty.main_menu"),
        Some(Scene::Scoreboard) => tr!("hud_maker.panel.empty.scoreboard"),
        Some(Scene::FreezeCam) => tr!("hud_maker.panel.empty.freeze_cam"),
        Some(Scene::WinPanel) => tr!("hud_maker.panel.empty.win_panel"),
        Some(Scene::ClassSelect) => tr!("hud_maker.panel.empty.class_select"),
        Some(Scene::Spectator) => tr!("hud_maker.panel.empty.spectator"),
        _ => tr!("hud_maker.panel.empty.hud"),
    }
}
fn unreadable() -> SharedString {
    tr!("hud_maker.panel.unreadable")
}
fn from_base() -> SharedString {
    tr!("hud_maker.panel.from_base")
}
fn base_missing() -> SharedString {
    tr!("hud_maker.panel.base_missing")
}
fn code_text() -> SharedString {
    tr!("hud_maker.panel.code_text")
}
pub(super) fn reset_tip() -> SharedString {
    tr!("hud_maker.panel.reset_tip")
}
fn mirror_tip() -> SharedString {
    tr!("hud_maker.panel.mirror_tip")
}
fn mirror_width_tip() -> SharedString {
    tr!("hud_maker.panel.mirror_width_tip")
}
fn mirror_place_tip() -> SharedString {
    tr!("hud_maker.panel.mirror_place_tip")
}
fn match_size_tip() -> SharedString {
    tr!("hud_maker.panel.match_size_tip")
}
fn corners_off_tip() -> SharedString {
    tr!("hud_maker.panel.corners_off_tip")
}
// The player word for `ALIGNMENTS[i]`, in its order.
fn align_word(i: usize) -> SharedString {
    match i {
        0 => tr!("hud_maker.panel.align.top_left"),
        1 => tr!("hud_maker.panel.align.top"),
        2 => tr!("hud_maker.panel.align.top_right"),
        3 => tr!("hud_maker.panel.align.left"),
        4 => tr!("hud_maker.panel.align.centre"),
        5 => tr!("hud_maker.panel.align.right"),
        6 => tr!("hud_maker.panel.align.bottom_left"),
        7 => tr!("hud_maker.panel.align.bottom"),
        _ => tr!("hud_maker.panel.align.bottom_right"),
    }
}
const ALIGN_ICONS: [IconName; 9] = [
    IconName::ArrowUpLeft,
    IconName::ArrowUp,
    IconName::ArrowUpRight,
    IconName::ArrowLeft,
    IconName::Dot,
    IconName::ArrowRight,
    IconName::ArrowDownLeft,
    IconName::ArrowDown,
    IconName::ArrowDownRight,
];
// The value the engine uses when the alignment is unset.
const ALIGN_UNSET: &str = "west";
// The Align buttons for several parts: id name, icon and the line they align on.
const LINE_UPS: [(&str, IconName, Align); 6] = [
    ("left", IconName::ArrowLeft, Align::Left),
    ("centre", IconName::FlipHorizontal2, Align::Centre),
    ("right", IconName::ArrowRight, Align::Right),
    ("top", IconName::ArrowUp, Align::Top),
    ("middle", IconName::FlipVertical2, Align::Middle),
    ("bottom", IconName::ArrowDown, Align::Bottom),
];

// The tooltip of the Align button that lines parts up on `align`.
fn line_up_tip(align: Align) -> SharedString {
    match align {
        Align::Left => tr!("hud_maker.panel.line_up.left"),
        Align::Centre => tr!("hud_maker.panel.line_up.centre"),
        Align::Right => tr!("hud_maker.panel.line_up.right"),
        Align::Top => tr!("hud_maker.panel.line_up.top"),
        Align::Middle => tr!("hud_maker.panel.line_up.middle"),
        Align::Bottom => tr!("hud_maker.panel.line_up.bottom"),
    }
}

// What one line-up button does.
#[derive(Clone, Copy)]
enum LineUp {
    Align(Align),
    Spread(Spread),
}
// The colour-name and font lists scroll past this height.
const NAMES_HEIGHT: f32 = 200.;
fn remove_setting() -> SharedString {
    tr!("hud_maker.panel.remove_setting")
}
fn remove_setting_tip() -> SharedString {
    tr!("hud_maker.panel.remove_setting_tip")
}

/// One row of "All settings": a key the selection resolves, its value, the file and line that
/// write it, and whether core lets the player edit and remove it.
struct SettingRow {
    key: String,
    value: String,
    file: String,
    line: u32,
    editable: bool,
}

/// What the side panel shows for the current selection, read out of the editor before drawing.
enum Body {
    Empty,
    // Two or more parts picked together, with how many.
    Several(usize),
    Unreadable,
    // A part drawn from an included file; `incomplete` when some file behind it was not found.
    FromBase { incomplete: bool },
    Rows(Rows),
}

struct Rows {
    // An editable part some of whose included files were not found, as in a HUD copy.
    base_missing: bool,
    shows: Option<SharedString>,
    // Each offered row up to Stage 5 with whether Reset has anything to drop.
    rows: Vec<(&'static PropertySpec, bool)>,
    caps: PartCaps,
    hidden: bool,
    // The player hid the part by giving it size zero, so its size reads as hidden.
    size_hidden: bool,
    names: Vec<(String, Hsla)>,
    // The game's code writes the part's text, so the canvas shows a sample, not the Text box.
    code_text: bool,
}

/// Sizes a panel body: docked, it fills what its frame has; floating, it keeps its own height
/// and shrinks under the frame's cap.
fn fill<E: Styled>(el: E, docked: bool) -> E {
    match docked {
        true => el.flex_1().min_h_0(),
        false => el.flex_shrink(1.).min_h_0(),
    }
}

// A body already wrapped in its scroll area, sized to the panel. The part list keeps a small
// inset around it.
fn scrolled(area: Div, docked: bool, inset: bool) -> AnyElement {
    fill(v_flex(), docked)
        .when(inset, |el| el.p_1())
        .child(fill(area, docked))
        .into_any_element()
}

// The space between two panels docked on one side.
pub(super) const STACK_GAP: f32 = 8.;
// The space left under a docked column.
const COLUMN_MARGIN: f32 = 16.;
// A docked panel folded to a rail at the area's edge is this wide.
pub(super) const RAIL_WIDTH: Pixels = px(36.);
// The side panel's bottom stays within this share of the area's height, so the lower HUD stays
// clickable, but the panel is never shorter than SIDE_PANEL_MIN; its body scrolls inside.
pub(super) const SIDE_PANEL_SHARE: f32 = 0.6;
pub(super) const SIDE_PANEL_MIN: f32 = 240.;

/// One panel's element ids; both panels are built by the same code.
struct PanelIds {
    frame: &'static str,
    title: &'static str,
    dock_left: &'static str,
    float: &'static str,
    dock_right: &'static str,
    collapse: &'static str,
}

fn ids_of(panel: Panel) -> PanelIds {
    match panel {
        Panel::Options => PanelIds {
            frame: ids::HUD_MAKER_PANEL,
            title: ids::HUD_MAKER_PANEL_TITLE,
            dock_left: ids::HUD_MAKER_PANEL_DOCK_LEFT,
            float: ids::HUD_MAKER_PANEL_FLOAT,
            dock_right: ids::HUD_MAKER_PANEL_DOCK_RIGHT,
            collapse: ids::HUD_MAKER_PANEL_COLLAPSE,
        },
        Panel::Parts => PanelIds {
            frame: ids::HUD_MAKER_PARTS_PANEL,
            title: ids::HUD_MAKER_PARTS_PANEL_TITLE,
            dock_left: ids::HUD_MAKER_PARTS_PANEL_DOCK_LEFT,
            float: ids::HUD_MAKER_PARTS_PANEL_FLOAT,
            dock_right: ids::HUD_MAKER_PARTS_PANEL_DOCK_RIGHT,
            collapse: ids::HUD_MAKER_PARTS_PANEL_COLLAPSE,
        },
    }
}

/// A running title-bar drag: where the pointer went down, the panel's offset then, and the
/// player's own place to return to when the panel never moved.
#[derive(Clone, Copy)]
pub(super) struct PanelDrag {
    pointer: Point<Pixels>,
    from: Point<Pixels>,
    home: PanelPlace,
}

impl HudMakerPage {
    // Floating panels sit where placed; docked ones share one column per side, side panel on
    // top, so two docked on one side stack instead of overlapping. A side holding only rails
    // shows them as slim tabs, and a panel opened from its rail lies over the canvas beside them.
    pub(super) fn panels(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut out = Vec::new();
        // The column runs to just short of the area's bottom, so a full panel shows its border.
        let room = (self.area.get().size.height - px(COLUMN_MARGIN)).max(px(0.));
        let column = |side: PanelMode, inset: Pixels, children: Vec<AnyElement>| {
            let column = v_flex()
                .absolute()
                .top_0()
                .h(room)
                .gap(px(STACK_GAP))
                .children(children);
            match side {
                PanelMode::DockLeft => column.left(inset).items_start(),
                PanelMode::Floating | PanelMode::DockRight => column.right(inset).items_end(),
            }
            .into_any_element()
        };
        for side in [PanelMode::DockLeft, PanelMode::DockRight] {
            let on_side: Vec<Panel> = (Panel::ALL.into_iter())
                .filter(|&p| self.mode_shown(p) == side)
                .collect();
            let (rails, docked): (Vec<Panel>, Vec<Panel>) =
                on_side.into_iter().partition(|&p| self.is_railed(p));
            // A panel opened from its rail stands beside the rails, so each rail stays in reach.
            let inset = match rails.is_empty() {
                true => px(0.),
                false => RAIL_WIDTH + px(STACK_GAP),
            };
            if !rails.is_empty() {
                let rails = rails.into_iter().map(|p| self.rail(p, cx)).collect();
                out.push(column(side, px(0.), rails));
            }
            if !docked.is_empty() {
                let docked = docked.into_iter().map(|p| self.panel(p, cx)).collect();
                out.push(column(side, inset, docked));
            }
        }
        for p in Panel::ALL {
            if self.mode_shown(p) == PanelMode::Floating {
                out.push(self.panel(p, cx));
            }
        }
        out
    }

    // A docked panel folded to a slim tab at its edge; its one button opens the panel, under
    // the collapse button's id since it stands in for the collapsed panel.
    fn rail(&self, panel: Panel, cx: &mut Context<Self>) -> AnyElement {
        let own = ids_of(panel);
        let (icon, tip) = match panel {
            Panel::Options => (IconName::Paintbrush, tr!("hud_maker.panel.show_settings")),
            Panel::Parts => (IconName::Layers, tr!("hud_maker.panel.show_parts")),
        };
        let theme = cx.theme();
        v_flex()
            .id(own.frame)
            .test_support()
            .occlude()
            .flex_none()
            .w(RAIL_WIDTH)
            .py_1()
            .items_center()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .bg(theme.background)
            .shadow_md()
            .child(
                Button::new(own.collapse)
                    .ghost()
                    .small()
                    .icon(icon)
                    .named(tip)
                    .on_click(cx.listener(move |page, _, _, cx| page.toggle_collapsed(panel, cx))),
            )
            .into_any_element()
    }

    // The panel measures itself with a `canvas` watcher, so avoidance can use last frame's size;
    // a moved or resized panel asks for another frame, where the page settles again. The same
    // watcher listens window-wide while a title drag runs, so the drag follows a fast pointer.
    pub(super) fn panel(&mut self, panel: Panel, cx: &mut Context<Self>) -> AnyElement {
        let place = self.panel_shown(panel);
        let state = &self.panels[panel as usize];
        let measured = Rc::clone(&state.bounds);
        let open_size = Rc::clone(&state.open_size);
        let open = !place.collapsed;
        let page = cx.entity().downgrade();
        let dragging = state.drag.is_some();
        let watch = canvas(
            move |inner, window, _| {
                // The watcher fills the inside of the 1 px border; the panel is that much bigger.
                let bounds = inner.dilate(px(1.));
                if measured.get() != bounds {
                    window.request_animation_frame();
                }
                measured.set(bounds);
                if open {
                    open_size.set(bounds.size);
                }
            },
            move |_, _, window, _| {
                if !dragging {
                    return;
                }
                window.set_window_cursor_style(CursorStyle::ClosedHand);
                let moved = page.clone();
                window.on_mouse_event(move |ev: &MouseMoveEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture {
                        let _ = moved.update(cx, |page, cx| page.drag_panel(panel, ev, cx));
                    }
                });
                let released = page.clone();
                window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture {
                        let _ = released.update(cx, |page, cx| page.end_panel_drag(panel, cx));
                    }
                });
            },
        )
        .absolute()
        .size_full();

        // The panel occludes, so a press on it never reaches the canvas and clears the selection.
        let theme = cx.theme();
        let frame = v_flex()
            .id(ids_of(panel).frame)
            .test_support()
            .occlude()
            .w(panel.width())
            .max_h_full()
            .min_h_0()
            .overflow_hidden()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .bg(theme.background)
            .shadow_md()
            .child(watch);
        // A floating panel keeps its height cap; a docked open one shares its column's height
        // with the other open one there, and a collapsed one stays its header.
        let area = self.area.get().size.height;
        let docked = place.mode != PanelMode::Floating;
        let frame = match place.mode {
            PanelMode::Floating => {
                // The cap settle planned with, so the drawn height is the planned one.
                let cap = (self.height_cap(panel))
                    .map_or(self.floating_height(), |cap| cap.at(area, px(place.y)));
                frame
                    .absolute()
                    .left(px(place.x))
                    .top(px(place.y))
                    .max_h(cap)
            }
            PanelMode::DockLeft | PanelMode::DockRight if place.collapsed => frame.flex_none(),
            PanelMode::DockLeft | PanelMode::DockRight => frame.flex_1(),
        };
        let body = (!place.collapsed).then(|| {
            let area = match panel {
                Panel::Options => {
                    let rows = v_flex()
                        .id(ids::HUD_MAKER_PANEL_BODY)
                        .overflow_y_scroll()
                        .track_scroll(&self.panel_scroll)
                        .test_support()
                        .p_3()
                        .pr(BAR_WIDTH)
                        .gap_3()
                        .children(self.panel_rows(cx));
                    let bar = ids::HUD_MAKER_PANEL_BODY_SCROLLBAR;
                    scroll_area(bar, &self.panel_scroll, fill(rows, docked))
                }
                Panel::Parts => {
                    let list = self.part_list(docked, cx);
                    let bar = ids::HUD_MAKER_PARTS_SCROLLBAR;
                    let list = scroll_area(bar, &self.parts_scroll, list);
                    fill(v_flex(), docked)
                        .child(self.parts_filter_box())
                        .child(fill(list, docked))
                }
            };
            scrolled(area, docked, panel == Panel::Parts)
        });
        frame
            .child(self.panel_title(panel, place, cx))
            .children(body)
            .into_any_element()
    }

    // The grip takes the free width, so a press on a button never starts a drag.
    fn panel_title(&self, panel: Panel, place: PanelPlace, cx: &mut Context<Self>) -> AnyElement {
        let floating = place.mode == PanelMode::Floating;
        let own = ids_of(panel);
        let title = match panel {
            Panel::Parts => tr!("hud_maker.panel.title.parts").into_any_element(),
            Panel::Options => match self.selected_name() {
                None if self.editor().is_some_and(|e| e.several()) => {
                    tr!("hud_maker.panel.title.several").into_any_element()
                }
                // The way down sits on its own small line over the part's name, so neither
                // is cut short in a narrow panel.
                Some(name) => v_flex()
                    .min_w_0()
                    .child(
                        h_flex()
                            .min_w_0()
                            .overflow_hidden()
                            .gap_1()
                            .text_xs()
                            .children(self.crumbs(cx)),
                    )
                    .child(labelled(div().truncate(), ids::HUD_MAKER_PANEL_PART, name))
                    .into_any_element(),
                None if self.borrow.open => tr!("hud_maker.borrow.title").into_any_element(),
                None if self.history_open => tr!("hud_maker.history.title").into_any_element(),
                None => tr!("hud_maker.panel.title.part").into_any_element(),
            },
        };
        let grip = div()
            .id(own.title)
            .test_support()
            .flex_1()
            .min_w_0()
            .py_2()
            .text_sm()
            .font_weight(FontWeight::SEMIBOLD)
            .when(floating, |el| el.cursor(CursorStyle::OpenHand))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |page, ev: &MouseDownEvent, _, cx| {
                    page.start_panel_drag(panel, ev, cx)
                }),
            )
            .child(title);
        // The dock buttons show the player's choice.
        let chosen = self.place_of(panel).mode;
        let mode_button = |id: &'static str, icon: IconName, tip: SharedString, mode: PanelMode| {
            Button::new(id)
                .ghost()
                .small()
                .icon(icon)
                .named(tip)
                .selected(chosen == mode)
                .on_click(cx.listener(move |page, _, _, cx| {
                    let place = PanelPlace {
                        mode,
                        ..page.place_of(panel)
                    };
                    page.set_place_of(panel, place, cx)
                }))
        };
        let (collapse_icon, collapse_tip) = if place.collapsed {
            (IconName::ChevronDown, tr!("hud_maker.panel.expand"))
        } else {
            (IconName::ChevronUp, tr!("hud_maker.panel.collapse"))
        };
        // The bar measures its own height, so a panel can fold to it before it was ever collapsed.
        let height = Rc::clone(&self.panels[panel as usize].title_height);
        let watch = canvas(
            move |inner, window, _| {
                if height.get() != inner.size.height {
                    height.set(inner.size.height);
                    window.request_animation_frame();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        h_flex()
            .flex_none()
            .pl_3()
            .pr_1()
            .gap_1()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(watch)
            .child(grip)
            .child(mode_button(
                own.dock_left,
                IconName::PanelLeft,
                tr!("hud_maker.panel.dock_left"),
                PanelMode::DockLeft,
            ))
            .child(mode_button(
                own.float,
                IconName::AppWindow,
                tr!("hud_maker.panel.float"),
                PanelMode::Floating,
            ))
            .child(mode_button(
                own.dock_right,
                IconName::PanelRight,
                tr!("hud_maker.panel.dock_right"),
                PanelMode::DockRight,
            ))
            .child(
                Button::new(own.collapse)
                    .ghost()
                    .small()
                    .icon(collapse_icon)
                    .named(collapse_tip)
                    .on_click(cx.listener(move |page, _, _, cx| page.toggle_collapsed(panel, cx))),
            )
            .into_any_element()
    }

    // The parts enclosing the selection, outermost first, each a step that selects it and a
    // chevron towards the part. A press stops at the step, so it never drags the panel.
    fn crumbs(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(editor) = self.editor() else {
            return Vec::new();
        };
        let model = editor.model();
        let mut steps = Vec::new();
        let mut at = (editor.selected()).and_then(|key| model.part(key)?.parent());
        while let Some(part) = at.and_then(|key| model.part(key)) {
            steps.push((part.key().clone(), row_label(part)));
            at = part.parent();
        }
        let theme = cx.theme();
        let (muted, fg) = (theme.muted_foreground, theme.foreground);
        (steps.into_iter().rev())
            .flat_map(|(key, name)| {
                let step = div()
                    .id(ids::hud_maker_crumb(&key))
                    .aria_label(SharedString::from(name.clone()))
                    .test_support()
                    .flex_none()
                    .whitespace_nowrap()
                    .font_weight(FontWeight::NORMAL)
                    .text_color(muted)
                    .cursor_pointer()
                    .hover(move |el| el.text_color(fg))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |page, _: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            page.select(Some(key.clone()), window, cx);
                            page.focus.focus(window, cx);
                        }),
                    )
                    .child(name);
                let chevron = Icon::new(IconName::ChevronRight)
                    .xsmall()
                    .flex_none()
                    .text_color(muted);
                [step.into_any_element(), chevron.into_any_element()]
            })
            .collect()
    }

    // The selected part named as its part-list row names it.
    fn selected_name(&self) -> Option<String> {
        let editor = self.editor()?;
        let part = editor.model().part(editor.selected()?)?;
        Some(row_label(part))
    }

    /// The active Colour row's colour for the picker, opaque; with no other row active, the
    /// colour slot's, `None` unless the part can be recoloured.
    pub(super) fn selected_colour(&self) -> Option<Hsla> {
        match self.active_colour() {
            Some(key) => self.row_colour(key),
            None => {
                let editor = self.editor()?;
                let key = editor.selected()?;
                let part = editor.model().part(key)?;
                if !editor.model().capabilities(key)?.recolour() {
                    return None;
                }
                Some(Hsla {
                    a: 1.,
                    ..colour(part, editor.model())
                })
            }
        }
    }

    // The selection's colour under `key` through the scheme, opaque: its value, else the row's
    // default, else white.
    pub(super) fn row_colour(&self, key: &str) -> Option<Hsla> {
        let editor = self.editor()?;
        let part = editor.model().part(editor.selected()?)?;
        let row = super::animations::parse_anim_key(key).and_then(|(event, row)| {
            match editor
                .model()
                .animations()
                .event(&event)?
                .commands()
                .get(row)?
            {
                tf2tools_core::AnimCommand::Animate { value, .. } => Some(value.clone()),
                tf2tools_core::AnimCommand::RunEvent { .. }
                | tf2tools_core::AnimCommand::StopEvent { .. }
                | tf2tools_core::AnimCommand::RunEventChild { .. }
                | tf2tools_core::AnimCommand::SetVisible { .. }
                | tf2tools_core::AnimCommand::SetInputEnabled { .. }
                | tf2tools_core::AnimCommand::FireCommand { .. }
                | tf2tools_core::AnimCommand::PlaySound { .. }
                | tf2tools_core::AnimCommand::StopAnimation { .. }
                | tf2tools_core::AnimCommand::StopPanelAnimations { .. }
                | tf2tools_core::AnimCommand::SetFont { .. }
                | tf2tools_core::AnimCommand::SetTexture { .. }
                | tf2tools_core::AnimCommand::SetString { .. }
                | _ => None,
            }
        });
        if key == HIT_FLASH_KEY || row.is_some() {
            let text = row.or_else(|| editor.hit_flash_colour_selected())?;
            let rgba = editor.model().scheme().colour(&text);
            return Some(Hsla {
                a: 1.,
                ..hsla_of_rgba(rgba.to_array())
            });
        }
        let rgba = part
            .value(key)
            .map(|line| line.text().to_owned())
            .or_else(|| {
                ClassSchema::for_part(part)
                    .get(key)?
                    .default()
                    .map(str::to_owned)
            })
            .map(|text| editor.model().scheme().colour(&text))
            .unwrap_or(tf2tools_core::Rgba::WHITE);
        Some(Hsla {
            a: 1.,
            ..hsla_of_rgba(rgba.to_array())
        })
    }

    fn body(&self) -> Body {
        let Some(editor) = self.editor() else {
            return Body::Empty;
        };
        if editor.several() {
            return Body::Several(editor.selection().len());
        }
        let model = editor.model();
        let Some((key, part)) = editor
            .selected()
            .and_then(|key| Some((key, model.part(key)?)))
        else {
            return Body::Empty;
        };
        // A scene note (cannot show, hidden by the HUD) explains every part, so it wins.
        if part.is_from_base() && scene_note(model).is_none() {
            return Body::FromBase {
                incomplete: part.base_incomplete(),
            };
        }
        let caps = model.capabilities(key);
        let Some(caps) = caps.filter(|_| part.is_readable()) else {
            return Body::Unreadable;
        };
        let hidden = editor.hidden_by_player();
        // A box of no size reads as hidden whatever hid it, so a part the game hides and the
        // player sized to zero reads right too.
        let size_hidden = hidden
            && editor
                .layout()
                .get(key)
                .is_some_and(|b| b.wide() <= 0 || b.tall() <= 0);
        // The colour slot shows only where the part can be recoloured; other colours always do.
        // Stage 5 Number rows (the killfeed's corner rounding) have no field yet.
        let rows = ClassSchema::for_part(part)
            .up_to(5)
            .filter(|spec| !(spec.stage() == 5 && spec.kind() == PropertyKind::Number))
            .filter(|spec| offered(spec.kind(), caps))
            .filter(|spec| !is_slot(spec) || caps.recolour())
            .map(|spec| {
                // Hiding by size writes the size; only the Hide row's Reset undoes that.
                let changed = match spec.kind() {
                    PropertyKind::Hide => hidden,
                    PropertyKind::Size if size_hidden => false,
                    PropertyKind::Position
                    | PropertyKind::Size
                    | PropertyKind::Layer
                    | PropertyKind::Colour
                    | PropertyKind::Text
                    | PropertyKind::Alignment
                    | PropertyKind::Toggle
                    | PropertyKind::Font
                    | PropertyKind::Image
                    | PropertyKind::Number
                    | PropertyKind::Choice
                    | _ => editor.changed(spec.keys()),
                };
                (spec, changed)
            })
            .collect();
        let scheme = model.scheme();
        let names = scheme
            .colour_names()
            .into_iter()
            .map(|name| {
                let swatch = hsla_of_rgba(scheme.colour(name).to_array());
                (name.to_owned(), swatch)
            })
            .collect();
        let sample = model.scene_rule().sample();
        let code_text =
            sample.code_text(part.name()).is_some() || sample.code_label(part.name()).is_some();
        Body::Rows(Rows {
            base_missing: part.base_incomplete(),
            shows: (part.code_toggled().map(|toggled| toggled.shows_when()))
                .or_else(|| situational_ancestor(model, part))
                .or_else(|| part.owner().map(|id| PanelRule::get(id).shows_when()))
                .map(maker_label),
            rows,
            caps,
            hidden,
            size_hidden,
            names,
            code_text,
        })
    }

    // The body's contents: a prompt, an unreadable note, or when the part shows and its rows.
    fn panel_rows(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let muted = cx.theme().muted_foreground;
        let note = |id: &'static str, text: SharedString| {
            labelled(div().text_sm().text_color(muted), id, text).into_any_element()
        };
        let rows = match self.body() {
            Body::Empty if self.borrow.open => return self.borrow_rows(cx),
            Body::Empty if self.history_open => return self.history_rows(cx),
            Body::Empty => {
                let mut out = vec![note(ids::HUD_MAKER_PANEL_EMPTY, empty_hint(self.scene()))];
                out.extend(self.hud_rows(cx));
                return out;
            }
            Body::Several(n) => {
                let said = plural("hud_maker.panel.several", n as u64);
                return vec![note(ids::HUD_MAKER_SEVERAL, said), self.line_up_rows(cx)];
            }
            Body::FromBase { incomplete } => {
                let mut out = vec![note(ids::HUD_MAKER_PANEL_FROM_BASE, from_base())];
                if incomplete {
                    out.push(note(ids::HUD_MAKER_PANEL_BASE_MISSING, base_missing()));
                }
                return out;
            }
            Body::Unreadable => {
                // A scene that cannot show, or that the HUD hides, says why for every part.
                let why = (self.editor())
                    .and_then(|e| scene_note(e.model()))
                    .map_or_else(unreadable, SharedString::from);
                return vec![note(ids::HUD_MAKER_PANEL_UNREADABLE, why)];
            }
            Body::Rows(rows) => rows,
        };
        let shows = rows
            .shows
            .map(|text| note(ids::HUD_MAKER_PANEL_SHOWS, text));
        let missing =
            (rows.base_missing).then(|| note(ids::HUD_MAKER_PANEL_BASE_MISSING, base_missing()));
        let mut out: Vec<AnyElement> = missing.into_iter().chain(shows).collect();
        out.extend(self.show_it_button(cx));
        out.extend(self.cut_off_row(cx));
        out.extend(self.part_actions(cx));
        out.extend(self.attach_row(cx));
        let families = match self.open_font_list() {
            Some((_, FontList::Families)) => self.font_families(),
            _ => Vec::new(),
        };
        for &(spec, changed) in &rows.rows {
            let content = match spec.kind() {
                PropertyKind::Position => h_flex()
                    .gap_2()
                    .when(rows.caps.move_x(), |el| {
                        el.child(self.number(ids::HUD_MAKER_X, "X", Field::X, cx))
                    })
                    .when(rows.caps.move_y(), |el| {
                        el.child(self.number(ids::HUD_MAKER_Y, "Y", Field::Y, cx))
                    })
                    .into_any_element(),
                PropertyKind::Size if rows.size_hidden => labelled(
                    div().text_sm().text_color(muted),
                    ids::HUD_MAKER_SIZE_HIDDEN,
                    tr!("hud_maker.panel.size_hidden"),
                )
                .into_any_element(),
                PropertyKind::Size => h_flex()
                    .gap_2()
                    .when(rows.caps.resize_wide(), |el| {
                        el.child(self.number(ids::HUD_MAKER_WIDE, "W", Field::Wide, cx))
                    })
                    .when(rows.caps.resize_tall(), |el| {
                        el.child(self.number(ids::HUD_MAKER_TALL, "H", Field::Tall, cx))
                    })
                    .into_any_element(),
                PropertyKind::Layer => self.layer_buttons(cx),
                PropertyKind::Hide => self.hide_switch(rows.hidden, cx),
                PropertyKind::Number => match Field::of_number(spec.key()) {
                    Some(field) => self.number(ids::hud_maker_number(spec.key()), "", field, cx),
                    None => continue,
                },
                PropertyKind::Text => self.number(ids::HUD_MAKER_TEXT, "", Field::Text, cx),
                PropertyKind::Alignment => self.alignment(spec.key(), cx),
                PropertyKind::Choice => self.choice(spec, cx),
                PropertyKind::Toggle => {
                    // A switch carries its own label, so its row is one line.
                    out.push(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .child(self.toggle(spec, cx))
                            .child(self.reset_button(spec, changed, cx))
                            .into_any_element(),
                    );
                    continue;
                }
                PropertyKind::Colour => {
                    // Named colours just opened scroll their row into view once, like a font list.
                    let active = self.active_colour();
                    let shown = if is_slot(spec) {
                        active.is_none()
                    } else {
                        active == Some(spec.key())
                    };
                    if shown && self.names_open && std::mem::take(&mut self.reveal_names) {
                        self.panel_scroll.scroll_to_item(out.len());
                    }
                    self.colour_row(spec, &rows.names, cx)
                }
                PropertyKind::Font => {
                    // A list just opened scrolls its whole row into view once.
                    let opened = self.open_font_list().is_some_and(|(k, _)| k == spec.key());
                    if opened && std::mem::take(&mut self.reveal_font_list) {
                        self.panel_scroll.scroll_to_item(out.len());
                    }
                    self.font_row(spec, &families, cx)
                }
                PropertyKind::Image | _ => continue,
            };
            // Mirror and Match size sit in their row's title line, so the panel grows no taller.
            let caps = rows.caps;
            let extra = match spec.kind() {
                PropertyKind::Position if caps.move_x() => Some(self.mirror_button(caps, cx)),
                PropertyKind::Size if caps.resize_wide() || caps.resize_tall() => {
                    Some(self.match_size_button(cx))
                }
                PropertyKind::Position
                | PropertyKind::Size
                | PropertyKind::Layer
                | PropertyKind::Hide
                | PropertyKind::Colour
                | PropertyKind::Text
                | PropertyKind::Alignment
                | PropertyKind::Toggle
                | PropertyKind::Font
                | PropertyKind::Image
                | PropertyKind::Number
                | PropertyKind::Choice
                | _ => None,
            };
            out.push(self.row(spec, changed, extra, content, cx));
            if spec.kind() == PropertyKind::Text && rows.code_text {
                out.push(note(ids::HUD_MAKER_PANEL_CODE_TEXT, code_text()));
            }
        }
        out.extend(self.picture_row(cx));
        out.extend(self.flash_row(cx));
        out.extend(self.animations_section(cx));
        out.push(self.all_settings(cx));
        out.extend(self.part_hud_options(cx));
        out
    }

    // The selection's resolved keys, read only while "All settings" is open. A key is editable
    // when core would take a value for it: not an identity or code-owned key, nor unreadable.
    fn setting_rows(&self) -> Vec<SettingRow> {
        let Some(editor) = self.editor().filter(|_| self.settings_open) else {
            return Vec::new();
        };
        let model = editor.model();
        let Some((key, part)) = (editor.selected()).and_then(|k| Some((k, model.part(k)?))) else {
            return Vec::new();
        };
        (part.settings().into_iter())
            .map(|line| SettingRow {
                key: line.key().to_owned(),
                value: line.text().trim().to_owned(),
                file: line.file().to_owned(),
                line: line.line(),
                editable: matches!(
                    model.set_key_text(key, line.key(), line.text(), 0),
                    Ok(_) | Err(EditRefused::BadValue)
                ),
            })
            .collect()
    }

    // The "All settings" button and, when open, one row per resolved key: the key, its value
    // (the one shared input on the row last pressed), where it is written and Remove.
    fn all_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let open = self.settings_open;
        let toggle = Button::new(ids::HUD_MAKER_ALL_SETTINGS)
            .ghost()
            .small()
            .icon(if open {
                IconName::ChevronDown
            } else {
                IconName::ChevronRight
            })
            .label(tr!("hud_maker.panel.all_settings"))
            .on_click(cx.listener(|page, _, _, cx| {
                page.settings_open = !page.settings_open;
                cx.notify();
            }));
        let editing = (self.setting_key.as_ref())
            .filter(|(part, _)| self.editor().and_then(|e| e.selected()) == Some(part))
            .map(|(_, key)| key.clone());
        let theme = cx.theme();
        let (muted, radius, border) = (theme.muted_foreground, theme.radius, theme.border);
        let rows: Vec<AnyElement> = (self.setting_rows().into_iter())
            .map(|row| {
                let key: SharedString = row.key.clone().into();
                let value = if !row.editable {
                    None
                } else if editing.as_deref() == Some(row.key.as_str()) {
                    Some(self.number(ids::HUD_MAKER_SETTING_INPUT, "", Field::Setting, cx))
                } else {
                    let pressed = row.key.clone();
                    Some(
                        div()
                            .id(ids::hud_maker_setting_value(&row.key))
                            .aria_label(SharedString::from(row.value.clone()))
                            .test_support()
                            .h_6()
                            .px_2()
                            .flex()
                            .items_center()
                            .rounded(radius)
                            .border_1()
                            .border_color(border)
                            .cursor_text()
                            .text_sm()
                            .truncate()
                            .child(row.value.clone())
                            .on_click(cx.listener(move |page, _, window, cx| {
                                page.press_setting(&pressed, window, cx)
                            }))
                            .into_any_element(),
                    )
                };
                let remove = row.editable.then(|| {
                    let removed = row.key.clone();
                    // The kit button reports no label of its own, so a wrapper carries the id.
                    div()
                        .id(ids::hud_maker_setting_remove(&row.key))
                        .aria_label(remove_setting())
                        .test_support()
                        .tooltip(tip(ids::HUD_MAKER_TIP, remove_setting_tip()))
                        .child(
                            Button::new(ids::hud_maker_setting_remove_button(&row.key))
                                .ghost()
                                .xsmall()
                                .icon(IconName::Trash)
                                .accessibility_label(remove_setting())
                                .on_click(cx.listener(move |page, _, window, cx| {
                                    page.remove_setting(&removed, window, cx)
                                })),
                        )
                });
                v_flex()
                    .id(ids::hud_maker_setting(&row.key))
                    .aria_label(key.clone())
                    .test_support()
                    .gap_1()
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .child(div().text_xs().truncate().child(key))
                            .children(remove),
                    )
                    .children(value)
                    .child(origin(&row, muted))
                    .into_any_element()
            })
            .collect();
        v_flex()
            .gap_2()
            .child(toggle)
            .children(rows)
            .into_any_element()
    }

    // Moves the one setting input to `key`, committing what was typed for another setting.
    fn press_setting(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.flush(cx);
        let part = self.editor().and_then(|e| e.selected().cloned());
        self.setting_key = part.map(|p| (p, key.to_owned()));
        self.refresh(window, cx);
    }

    // Removes `key` from the selection; text typed for it is dropped, not committed first.
    fn remove_setting(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        if (self.setting_key.as_ref()).is_some_and(|(_, k)| k == key) {
            self.discard(&[Field::Setting]);
            self.setting_key = None;
        }
        self.edit_selected(window, cx, |e| {
            let _ = e.remove_key_selected(key);
        })
    }

    // Bring forward and Send back, each stepping the part past the sibling drawn next to it that
    // way; a part already drawn over or under all its siblings has that button off.
    fn layer_buttons(&self, cx: &mut Context<Self>) -> AnyElement {
        let can = |forward| self.editor().is_some_and(|e| e.can_layer(forward));
        let button = |id: &'static str, label: SharedString, forward: bool| {
            Button::new(id)
                .outline()
                .xsmall()
                .label(label)
                .disabled(!can(forward))
                .on_click(cx.listener(move |page, _, window, cx| {
                    page.edit_selected(window, cx, |e| {
                        if let Err(err) = e.layer_selected(forward) {
                            tracing::debug!("the HUD Maker refused a layer step: {err:?}");
                        }
                    })
                }))
        };
        h_flex()
            .id(ids::HUD_MAKER_LAYER)
            .test_support()
            .gap_2()
            .child(button(
                ids::HUD_MAKER_FORWARD,
                tr!("hud_maker.panel.forward"),
                true,
            ))
            .child(button(
                ids::HUD_MAKER_BACKWARD,
                tr!("hud_maker.panel.backward"),
                false,
            ))
            .into_any_element()
    }

    // Align on one line of the box around several parts, then Distribute to even out the gaps,
    // which needs three parts.
    fn line_up_rows(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let spreads = self.editor().is_some_and(HudEditor::can_distribute);
        let button = |name: &'static str, icon: IconName, tip: SharedString, act: LineUp| {
            let id = ids::hud_maker_line_up(name);
            let off = matches!(act, LineUp::Spread(_)) && !spreads;
            let button = Button::new(id.clone())
                .outline()
                .small()
                .icon(icon)
                .accessibility_label(tip.clone())
                .disabled(off)
                .on_click(cx.listener(move |page, _, window, cx| {
                    page.edit_selected(window, cx, |e| {
                        let done = match act {
                            LineUp::Align(a) => e.align_selected(a),
                            LineUp::Spread(s) => e.distribute_selected(s),
                        };
                        if let Err(err) = done {
                            tracing::debug!("the HUD Maker refused to line parts up: {err:?}");
                        }
                    })
                }));
            tipped(&id, tip, button)
        };
        let heading = |text: SharedString| div().text_xs().text_color(muted).child(text);
        let aligns = LINE_UPS
            .iter()
            .map(|&(name, icon, a)| button(name, icon, line_up_tip(a), LineUp::Align(a)));
        v_flex()
            .gap_2()
            .child(heading(tr!("hud_maker.panel.align_heading")))
            .child(h_flex().gap_1().children(aligns))
            .child(heading(tr!("hud_maker.panel.distribute_heading")))
            .child(
                h_flex()
                    .gap_1()
                    .child(button(
                        "across",
                        IconName::Ellipsis,
                        tr!("hud_maker.panel.line_up.across"),
                        LineUp::Spread(Spread::Across),
                    ))
                    .child(button(
                        "down",
                        IconName::EllipsisVertical,
                        tr!("hud_maker.panel.line_up.down"),
                        LineUp::Spread(Spread::Down),
                    )),
            )
            .into_any_element()
    }

    // Mirror, enabled only where the mirror holds on every screen; otherwise its tooltip says why.
    fn mirror_button(&self, caps: PartCaps, cx: &mut Context<Self>) -> AnyElement {
        let why = (self.editor()).and_then(|e| e.model().why_not_mirrored(e.selected()?));
        let tip = match why {
            _ if caps.mirror() => mirror_tip(),
            Some(NoMirror::Width) => mirror_width_tip(),
            _ => mirror_place_tip(),
        };
        let button = Button::new(ids::HUD_MAKER_MIRROR)
            .ghost()
            .xsmall()
            .label(tr!("hud_maker.panel.mirror"))
            .disabled(!caps.mirror())
            .on_click(cx.listener(|page, _, window, cx| {
                page.edit_selected(window, cx, |e| {
                    if let Err(err) = e.mirror_selected() {
                        tracing::debug!("the HUD Maker refused a mirror: {err:?}");
                    }
                })
            }));
        tipped(&ids::HUD_MAKER_MIRROR.into(), tip, button)
    }

    // Show it, for a part the current situation hides: it sets the situation in which the
    // part, or the panel holding it, shows.
    fn show_it_button(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let editor = self.editor()?;
        let part = editor.selected()?.clone();
        if !editor.offers_show_it(&part) {
            return None;
        }
        let button = Button::new(ids::HUD_MAKER_PANEL_SHOW_IT)
            .outline()
            .xsmall()
            .label(tr!("hud_maker.panel.show_it"))
            .on_click(cx.listener(move |page, _, window, cx| {
                let next = page
                    .editor_mut()
                    .and_then(|e| e.show_part_situation(&part).then(|| e.situation().clone()));
                if let Some(next) = next {
                    page.set_situation(next, window, cx);
                }
            }));
        Some(h_flex().child(button).into_any_element())
    }

    // For a part poking out of its parent, which the game cuts off there: says so, with a
    // button that grows the parent to hold it.
    fn cut_off_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let editor = self.editor()?;
        let parent = editor.model().part(editor.selection_outside_parent()?)?;
        let name = row_label(parent);
        let said = tr!("hud_maker.panel.cut_off", name = name);
        let button = Button::new(ids::HUD_MAKER_GROW_PARENT)
            .outline()
            .xsmall()
            .label(tr!("hud_maker.panel.grow_parent", name = name))
            .on_click(cx.listener(|page, _, window, cx| {
                page.edit_selected(window, cx, |e| {
                    if let Err(err) = e.grow_parent_to_fit() {
                        tracing::debug!("the HUD Maker refused to grow a parent: {err:?}");
                    }
                })
            }));
        let note = labelled(
            div().text_sm().text_color(cx.theme().warning),
            ids::HUD_MAKER_PANEL_CUT_OFF,
            said,
        );
        Some(
            v_flex()
                .gap_1()
                .items_start()
                .child(note)
                .child(button)
                .into_any_element(),
        )
    }

    // Add, a menu of the parts the player can add near the selection, and Delete for a part
    // the player added.
    fn part_actions(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let editor = self.editor()?;
        let part = editor.model().part(editor.selected()?)?;
        let label = is_label_part(part);
        let deletable = editor.can_delete_selected();
        let page = cx.entity().downgrade();
        let kinds = [
            (
                NewPart::Shadow,
                tr!("hud_maker.panel.new_part.shadow"),
                label,
            ),
            (NewPart::Box, tr!("hud_maker.panel.new_part.box"), true),
            (
                NewPart::Anchor,
                tr!("hud_maker.panel.new_part.anchor"),
                true,
            ),
            (
                NewPart::Crosshair,
                tr!("hud_maker.panel.new_part.crosshair"),
                true,
            ),
        ];
        let items = (kinds.into_iter()).filter(|k| k.2).map(|(kind, text, _)| {
            Button::new(ids::hud_maker_add_part(kind))
                .ghost()
                .small()
                .label(text)
                .on_click(cx.listener(move |page, _, window, cx| {
                    page.add_part_open = false;
                    page.add_part(kind, window, cx);
                }))
        });
        let add = Popover::new("hud-maker-add-part-menu")
            .anchor(Anchor::TopLeft)
            .open(self.add_part_open)
            .on_open_change(move |&open, _, cx| {
                let _ = page.update(cx, |page, cx| {
                    page.add_part_open = open;
                    cx.notify();
                });
            })
            .trigger(
                Button::new(ids::HUD_MAKER_ADD_PART)
                    .outline()
                    .xsmall()
                    .icon(IconName::Plus)
                    .label(tr!("hud_maker.panel.add"))
                    .tooltip(tr!("hud_maker.panel.add_tip")),
            )
            .child(v_flex().gap_1().items_start().children(items));
        let delete = deletable.then(|| {
            Button::new(ids::HUD_MAKER_DELETE_PART)
                .outline()
                .xsmall()
                .icon(IconName::Trash)
                .label(tr!("hud_maker.panel.delete"))
                .tooltip(tr!("hud_maker.panel.delete_tip"))
                .on_click(cx.listener(|page, _, window, cx| page.delete_part(window, cx)))
        });
        Some(
            h_flex()
                .gap_1()
                .child(add)
                .children(delete)
                .into_any_element(),
        )
    }

    // "Flash when you hit someone" on a label, with its colour's swatch while it is on.
    fn flash_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let editor = self.editor()?;
        let part = editor.model().part(editor.selected()?)?;
        if !is_label_part(part) {
            return None;
        }
        let on = editor.hit_flash_selected();
        let page = cx.entity().downgrade();
        let switch = Switch::new(ids::HUD_MAKER_HIT_FLASH)
            .small()
            .checked(on)
            .label(tr!("hud_maker.panel.hit_flash"))
            .on_click(move |&on, window, cx| {
                let _ = page.update(cx, |page, cx| page.set_hit_flash(on, window, cx));
            });
        let theme = cx.theme();
        let swatch = on.then(|| {
            let fill = self.row_colour(HIT_FLASH_KEY).unwrap_or(theme.background);
            div()
                .id(ids::HUD_MAKER_HIT_FLASH_COLOUR)
                .aria_label(tr!("hud_maker.panel.flash_colour"))
                .test_support()
                .w_10()
                .h_6()
                .rounded(theme.radius)
                .border_1()
                .border_color(theme.border)
                .cursor_pointer()
                .bg(fill)
                .on_click(cx.listener(|page, _, window, cx| {
                    page.set_active_colour(Some(HIT_FLASH_KEY.to_owned()), window, cx)
                }))
        });
        let picker = (on && self.active_colour() == Some(HIT_FLASH_KEY)).then(|| {
            div()
                .id(ids::HUD_MAKER_COLOUR)
                .aria_label(tr!("hud_maker.panel.colour"))
                .test_support()
                .child(
                    ColorPicker::new(&self.colour)
                        .accessibility_label(tr!("hud_maker.panel.colour")),
                )
        });
        Some(
            v_flex()
                .gap_1()
                .child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child(switch)
                        .children(swatch),
                )
                .children(picker)
                .into_any_element(),
        )
    }

    // "Attach to", a menu of the siblings the selection can attach to by their part-list
    // names; while attached, a grid of corners for each side and Detach.
    fn attach_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let editor = self.editor()?;
        let model = editor.model();
        let part = model.part(editor.selected()?)?;
        let targets = editor.pin_targets_selected();
        let pin = editor.pin_of_selected();
        if targets.is_empty() && pin.is_none() {
            return None;
        }
        // The sibling a pin names, found as the game finds it: the first of that name.
        let pinned_to = pin.as_ref().and_then(|(name, ..)| {
            (model.parts().iter())
                .find(|s| s.parent() == part.parent() && s.name().eq_ignore_ascii_case(name))
        });
        let (own, corner) = pin
            .as_ref()
            .map_or((PinCorner::TopLeft, PinCorner::TopLeft), |p| (p.1, p.2));
        let items: Vec<Button> = (targets.iter())
            .filter_map(|key| {
                let label = row_label(model.part(key)?);
                let key = key.clone();
                Some(
                    Button::new(ids::hud_maker_attach_to(&key))
                        .ghost()
                        .small()
                        .label(label)
                        .on_click(cx.listener(move |page, _, window, cx| {
                            page.attach_open = false;
                            page.attach(key.clone(), own, corner, window, cx);
                        })),
                )
            })
            .collect();
        let label = match pinned_to {
            Some(to) => tr!("hud_maker.panel.attached_to", name = row_label(to)),
            None => tr!("hud_maker.panel.attach_to"),
        };
        let page = cx.entity().downgrade();
        let menu = Popover::new("hud-maker-attach-menu")
            .anchor(Anchor::TopLeft)
            .open(self.attach_open)
            .on_open_change(move |&open, _, cx| {
                let _ = page.update(cx, |page, cx| {
                    page.attach_open = open;
                    cx.notify();
                });
            })
            .trigger(
                Button::new(ids::HUD_MAKER_ATTACH)
                    .outline()
                    .xsmall()
                    .icon(IconName::ChevronsUpDown)
                    .label(label)
                    .tooltip(tr!("hud_maker.panel.attach_tip")),
            )
            .child(scroll_area(
                ids::HUD_MAKER_ATTACH_LIST_SCROLLBAR,
                &self.attach_scroll,
                v_flex()
                    .id(ids::HUD_MAKER_ATTACH_LIST)
                    .test_support()
                    .max_h(px(NAMES_HEIGHT))
                    .track_scroll(&self.attach_scroll)
                    .overflow_y_scroll()
                    .gap_1()
                    .items_start()
                    .pr(BAR_WIDTH)
                    .children(items),
            ));
        let mut out = v_flex().gap_1().child(menu);
        if let Some(to) = pinned_to {
            let to = to.key().clone();
            let muted = cx.theme().muted_foreground;
            let grid = |title: SharedString, own_side: bool, cx: &mut Context<Self>| {
                let at = if own_side { own } else { corner };
                let cell = |c: Option<PinCorner>, cx: &mut Context<Self>| -> AnyElement {
                    let Some(c) = c else {
                        return div().size_5().into_any_element();
                    };
                    let id = match own_side {
                        true => ids::hud_maker_pin_own(c),
                        false => ids::hud_maker_pin_corner(c),
                    };
                    let to = to.clone();
                    let theme = cx.theme();
                    let fill = if c == at {
                        theme.primary
                    } else {
                        theme.background
                    };
                    div()
                        .id(id)
                        .test_support()
                        .size_5()
                        .rounded_sm()
                        .border_1()
                        .border_color(theme.border)
                        .cursor_pointer()
                        .bg(fill)
                        .on_click(cx.listener(move |page, _, window, cx| {
                            let (o, k) = if own_side { (c, corner) } else { (own, c) };
                            page.attach(to.clone(), o, k, window, cx);
                        }))
                        .into_any_element()
                };
                let rows = CORNER_GRID.map(|row| {
                    h_flex()
                        .gap_0p5()
                        .children(row.map(|c| cell(c, cx)))
                        .into_any_element()
                });
                v_flex()
                    .gap_0p5()
                    .child(div().text_xs().text_color(muted).child(title))
                    .children(rows)
            };
            let detach = Button::new(ids::HUD_MAKER_DETACH)
                .ghost()
                .xsmall()
                .label(tr!("hud_maker.panel.detach"))
                .on_click(cx.listener(|page, _, window, cx| {
                    page.edit_selected(window, cx, |e| {
                        if let Err(err) = e.unpin_selected() {
                            tracing::debug!("the HUD Maker refused a detach: {err:?}");
                        }
                    })
                }));
            out = out.child(
                h_flex()
                    .gap_3()
                    .items_end()
                    .child(grid(tr!("hud_maker.panel.this_corner"), true, cx))
                    .child(grid(tr!("hud_maker.panel.to_its_corner"), false, cx))
                    .child(detach),
            );
        }
        Some(out.into_any_element())
    }

    // Match size starts picking: the next canvas press names the part whose size to copy.
    fn match_size_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let picking = self.matching_size;
        let button = Button::new(ids::HUD_MAKER_MATCH_SIZE)
            .ghost()
            .xsmall()
            .label(if picking {
                tr!("hud_maker.panel.pick_part")
            } else {
                tr!("hud_maker.panel.match_size")
            })
            .selected(picking)
            .on_click(cx.listener(|page, _, _, cx| {
                page.matching_size = !page.matching_size;
                cx.notify();
            }));
        tipped(&ids::HUD_MAKER_MATCH_SIZE.into(), match_size_tip(), button)
    }

    // A row: its label, any title-line button and Reset over its controls.
    fn row(
        &self,
        spec: &'static PropertySpec,
        changed: bool,
        extra: Option<AnyElement>,
        content: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let reset = self.reset_button(spec, changed, cx);
        v_flex()
            .gap_1()
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(maker_label(spec.label())),
                    )
                    .child(h_flex().gap_1().items_center().children(extra).child(reset)),
            )
            .child(content)
            .into_any_element()
    }

    // The row's Reset; it wins over text still being typed into the row.
    fn reset_button(
        &self,
        spec: &'static PropertySpec,
        changed: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let number = Field::of_number(spec.key());
        let id = ids::hud_maker_reset(spec.key());
        let button = Button::new(id.clone())
            .ghost()
            .xsmall()
            .icon(IconName::RotateCcw)
            .accessibility_label(reset_tip())
            .disabled(!changed)
            .on_click(cx.listener(move |page, _, window, cx| {
                page.discard(match spec.kind() {
                    PropertyKind::Position => &[Field::X, Field::Y],
                    PropertyKind::Size => &[Field::Wide, Field::Tall],
                    PropertyKind::Text => &[Field::Text],
                    PropertyKind::Number => number.as_slice(),
                    PropertyKind::Font
                        if (page.font_size_row.as_ref()).is_some_and(|(_, k)| k == spec.key()) =>
                    {
                        &[Field::FontSize]
                    }
                    PropertyKind::Layer
                    | PropertyKind::Hide
                    | PropertyKind::Colour
                    | PropertyKind::Alignment
                    | PropertyKind::Toggle
                    | PropertyKind::Font
                    | PropertyKind::Image
                    | PropertyKind::Choice
                    | _ => &[],
                });
                page.edit_selected(window, cx, |e| {
                    if spec.kind() == PropertyKind::Hide {
                        e.unhide_selected();
                    } else {
                        e.reset(spec.keys());
                    }
                })
            }));
        tipped(&id, reset_tip(), button)
    }

    // The selection's text for `key`, trimmed; None when the part leaves it unset.
    fn selected_value(&self, key: &str) -> Option<String> {
        let editor = self.editor()?;
        let part = editor.model().part(editor.selected()?)?;
        Some(part.value(key)?.text().trim().to_owned())
    }

    // Nine buttons laid out as the places they put the text; the current one reads selected.
    fn alignment(&self, key: &'static str, cx: &mut Context<Self>) -> AnyElement {
        let current = self.selected_value(key);
        let current = current.as_deref().unwrap_or(ALIGN_UNSET);
        let button = |i: usize, row: &Spot| {
            let value = ALIGNMENTS[i];
            let spot = Spot::default();
            let on = value.eq_ignore_ascii_case(current);
            // The kit button reports no selected state, so a wrapper carries the id and state.
            let button = Button::new(ids::hud_maker_align_button(value))
                .outline()
                .small()
                .icon(ALIGN_ICONS[i])
                .accessibility_label(align_word(i))
                .selected(on)
                .on_click(cx.listener(move |page, _, window, cx| {
                    page.edit_selected(window, cx, |e| {
                        let _ = e.set_property_selected(key, value);
                    })
                }));
            div()
                .id(ids::hud_maker_align(value))
                .aria_label(align_word(i))
                .aria_selected(on)
                .test_support()
                .tooltip(tip_beside_row(
                    ids::HUD_MAKER_ALIGN_TIP,
                    align_word(i),
                    Rc::clone(&spot),
                    Rc::clone(row),
                ))
                .relative()
                .child(button)
                .child(record(&spot))
        };
        // Each tooltip stands level with its own button, right of its row.
        let lines = (0..3).map(|row| {
            let spot = Spot::default();
            h_flex()
                .relative()
                .gap_1()
                .children((0..3).map(|col| button(row * 3 + col, &spot)))
                .child(record(&spot))
        });
        v_flex().gap_1().children(lines).into_any_element()
    }

    // A row of named values; the one in force reads selected, and a value the row does not
    // offer selects none.
    fn choice(&self, spec: &'static PropertySpec, cx: &mut Context<Self>) -> AnyElement {
        let key = spec.key();
        let current = self.selected_value(key);
        let current = current.as_deref().or(spec.default()).unwrap_or_default();
        // Corners only draw on a painted background, so without one they wait and say why.
        let idle = matches!(key, "PaintBackgroundType" | "RoundedCorners")
            && (self.editor())
                .and_then(|e| e.model().part(e.selected()?))
                .is_some_and(|part| !paints_background(part));
        let buttons = spec.choices().iter().map(|&(english, value)| {
            let label = maker_label(english);
            let on = value == current;
            // The kit button reports no selected state, so a wrapper carries the id and state.
            let id = ids::hud_maker_choice_button(key, value);
            let button = Button::new(id.clone())
                .outline()
                .small()
                .label(label.clone())
                .selected(on)
                .disabled(idle)
                .on_click(cx.listener(move |page, _, window, cx| {
                    page.edit_selected(window, cx, |e| {
                        let _ = e.set_property_selected(key, value);
                    })
                }));
            let button = if idle {
                tipped(&id, corners_off_tip(), button)
            } else {
                button.into_any_element()
            };
            div()
                .id(ids::hud_maker_choice(key, value))
                .aria_label(label)
                .aria_selected(on)
                .test_support()
                .child(button)
        });
        h_flex()
            .flex_wrap()
            .gap_1()
            .children(buttons)
            .into_any_element()
    }

    // An on/off row; unset reads as the engine's default.
    fn toggle(&self, spec: &'static PropertySpec, cx: &mut Context<Self>) -> AnyElement {
        let key = spec.key();
        let value = self.selected_value(key);
        let on = value.as_deref().or(spec.default()) == Some("1");
        let page = cx.entity().downgrade();
        Switch::new(ids::hud_maker_toggle(key))
            .small()
            .checked(on)
            .label(maker_label(spec.label()))
            .on_click(move |&on, window, cx| {
                let _ = page.update(cx, |page, cx| {
                    page.edit_selected(window, cx, |e| {
                        let _ = e.set_property_selected(key, if on { "1" } else { "0" });
                    })
                });
            })
            .into_any_element()
    }

    fn number(
        &self,
        id: impl Into<ElementId>,
        prefix: impl Into<SharedString>,
        field: Field,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (muted, warning) = (cx.theme().muted_foreground, cx.theme().warning);
        let prefix = prefix.into();
        let input = Input::new(self.field(field)).small();
        let input = if prefix.is_empty() {
            input
        } else {
            input.prefix(div().text_xs().text_color(muted).child(prefix))
        };
        let id = id.into();
        // A typed value that was rounded or refused says why under its box.
        let note = self.field_note(field).map(|note| {
            let colour = if note.refused { warning } else { muted };
            let style = div().text_xs().text_color(colour);
            labelled(style, ids::hud_maker_note_for(&id), note.text.clone())
        });
        div()
            .id(id)
            .test_support()
            .flex_1()
            .min_w_0()
            .child(input)
            .when_some(note, |el, note| el.flex().flex_col().gap_0p5().child(note))
            .into_any_element()
    }

    // The switch asks for the next state; the page writes it and redraws.
    fn hide_switch(&self, hidden: bool, cx: &mut Context<Self>) -> AnyElement {
        let page = cx.entity().downgrade();
        Switch::new(ids::HUD_MAKER_HIDE)
            .small()
            .checked(hidden)
            .label(tr!("hud_maker.panel.hide"))
            .on_click(move |&on, window, cx| {
                let _ = page.update(cx, |page, cx| {
                    page.edit_selected(window, cx, |e| {
                        if on {
                            let _ = e.hide_selected();
                        } else {
                            e.unhide_selected();
                        }
                    })
                });
            })
            .into_any_element()
    }

    // A swatch that makes the row active; the active row (the slot when no other is) shows the
    // picker and the HUD's named colours from a list under it.
    fn colour_row(
        &self,
        spec: &'static PropertySpec,
        names: &[(String, Hsla)],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let slot = is_slot(spec);
        let active = self.active_colour();
        if (slot && active.is_some()) || (!slot && active != Some(spec.key())) {
            return self.colour_swatch(spec, cx);
        }
        let theme = cx.theme();
        let (hover, radius, border) = (theme.accent, theme.radius, theme.border);
        let list = self.names_open.then(|| {
            let rows = names.iter().map(|(name, swatch)| {
                let name = name.clone();
                let label = SharedString::from(name.clone());
                div()
                    .id(ids::hud_maker_colour_name(&name))
                    .aria_label(label.clone())
                    .test_support()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded(radius)
                    .cursor_pointer()
                    .hover(|el| el.bg(hover))
                    .text_sm()
                    .child(
                        div()
                            .size_3()
                            .flex_none()
                            .rounded_sm()
                            .border_1()
                            .border_color(border)
                            .bg(*swatch),
                    )
                    .child(label)
                    .on_click(
                        cx.listener(move |page, _, window, cx| page.name_colour(&name, window, cx)),
                    )
            });
            let list = v_flex()
                .id(ids::HUD_MAKER_COLOUR_NAMES)
                .max_h(px(NAMES_HEIGHT))
                .track_scroll(&self.names_scroll)
                .overflow_y_scroll()
                .test_support()
                .pr(BAR_WIDTH)
                .children(rows);
            scroll_area(
                ids::HUD_MAKER_COLOUR_NAMES_SCROLLBAR,
                &self.names_scroll,
                list,
            )
        });
        v_flex()
            .gap_1()
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        div()
                            .id(ids::HUD_MAKER_COLOUR)
                            .aria_label(tr!("hud_maker.panel.colour"))
                            .test_support()
                            .child(
                                ColorPicker::new(&self.colour)
                                    .accessibility_label(tr!("hud_maker.panel.colour")),
                            ),
                    )
                    .child(
                        Button::new(ids::HUD_MAKER_COLOUR_NAME)
                            .outline()
                            .small()
                            .label(tr!("hud_maker.panel.named_colours"))
                            .selected(self.names_open)
                            .on_click(cx.listener(|page, _, _, cx| {
                                page.names_open = !page.names_open;
                                page.reveal_names = page.names_open;
                                page.names_scroll.set_offset(point(px(0.), px(0.)));
                                page.font_open = None;
                                cx.notify();
                            })),
                    ),
            )
            .children(list)
            .into_any_element()
    }

    // A row's colour as a button; pressing it makes the row the one the picker edits.
    fn colour_swatch(&self, spec: &'static PropertySpec, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let fill = self.row_colour(spec.key()).unwrap_or(theme.background);
        let key = (!is_slot(spec)).then(|| spec.key().to_owned());
        div()
            .id(ids::hud_maker_colour_row(spec.key()))
            .aria_label(maker_label(spec.label()))
            .test_support()
            .w_10()
            .h_6()
            .rounded(theme.radius)
            .border_1()
            .border_color(theme.border)
            .cursor_pointer()
            .bg(fill)
            .on_click(cx.listener(move |page, _, window, cx| {
                page.set_active_colour(key.clone(), window, cx)
            }))
            .into_any_element()
    }

    // The font the selection's Font row `key` names: its value, else the row's default.
    fn font_name(&self, key: &str) -> Option<String> {
        let editor = self.editor()?;
        let part = editor.model().part(editor.selected()?)?;
        let value = part.value(key).map(|line| line.text().trim().to_owned());
        value.or_else(|| Some(ClassSchema::for_part(part).get(key)?.default()?.to_owned()))
    }

    /// The variant the selection's Font row `key` uses at the editor's screen height.
    pub(super) fn font_variant(&self, key: &str) -> Option<&FontVariant> {
        let name = self.font_name(key)?;
        let editor = self.editor()?;
        editor.model().scheme().font(&name, editor.screen().tall())
    }

    // The Font row key and list open for the selection, if any.
    fn open_font_list(&self) -> Option<(&str, FontList)> {
        let (part, key, list) = self.font_open.as_ref()?;
        (Some(part) == self.editor()?.selected()).then_some((key.as_str(), *list))
    }

    // The editor's families, read again only when the scene or change list moved on.
    fn font_families(&mut self) -> Vec<String> {
        let Some(editor) = self.editor() else {
            return Vec::new();
        };
        let (scene, changes) = (editor.scene(), editor.changes());
        if let Some((at, was, families)) = &self.families
            && *at == scene
            && was == changes
        {
            return families.clone();
        }
        let families = editor.font_families();
        self.families = Some((scene, changes.to_vec(), families.clone()));
        families
    }

    // Opens `list` under Font row `key`, closing any other list and the named colours;
    // pressing the open one closes it.
    fn toggle_font_list(&mut self, key: &str, list: FontList, cx: &mut Context<Self>) {
        let open = self.open_font_list() == Some((key, list));
        let part = self.editor().and_then(|e| e.selected().cloned());
        self.font_open = part.filter(|_| !open).map(|p| (p, key.to_owned(), list));
        self.reveal_font_list = self.font_open.is_some();
        // Each list opens at its top, whichever list was scrolled before.
        self.font_scroll.set_offset(point(px(0.), px(0.)));
        self.names_open = false;
        cx.notify();
    }

    // Moves the one Size input to Font row `key`, committing what was typed for another row.
    fn press_font_size(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.flush(cx);
        let part = self.editor().and_then(|e| e.selected().cloned());
        self.font_size_row = part.map(|p| (p, key.to_owned()));
        self.refresh(window, cx);
    }

    // The current font, its family with the "Fonts" and "Family" lists, then Size, Outline and Shadow, each
    // control keyed by the row so a part with two Font rows keeps them apart.
    fn font_row(
        &self,
        spec: &'static PropertySpec,
        families: &[String],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = spec.key();
        let theme = cx.theme();
        let (muted, hover, radius, border) = (
            theme.muted_foreground,
            theme.accent,
            theme.radius,
            theme.border,
        );
        let current = self.font_name(key).unwrap_or_default();
        let variant = self.font_variant(key);
        let family = variant.map(|f| f.family().to_owned()).unwrap_or_default();
        let (size, outline, shadow) = variant.map_or((None, false, false), |f| {
            (Some(f.tall()), f.outline(), f.dropshadow())
        });
        // A font the maker made for this part has a made-up name, so the line names it by what
        // the player chose: its family and size.
        let current = match (starts_with_ignoring_case(&current, CLONE_PREFIX), size) {
            (true, Some(size)) => tr!(
                "hud_maker.panel.font.this_part_only",
                family = family,
                size = size
            ),
            _ => current.into(),
        };
        let open = self
            .open_font_list()
            .filter(|(at, _)| *at == key)
            .map(|(_, l)| l);
        let list_button = |id: ElementId, label: SharedString, list: FontList| {
            Button::new(id)
                .outline()
                .small()
                .label(label)
                .selected(open == Some(list))
                .on_click(cx.listener(move |page, _, _, cx| page.toggle_font_list(key, list, cx)))
        };
        let fonts = list_button(
            ids::hud_maker_font_list(key),
            tr!("hud_maker.panel.font.fonts"),
            FontList::Fonts,
        );
        let family_button = list_button(
            ids::hud_maker_font_family_list(key),
            tr!("hud_maker.panel.font.family"),
            FontList::Families,
        );
        // Each choice writes through `pick` as one undo step and closes the list.
        let choice = |id: ElementId, name: String, pick: fn(&'static str, &str) -> Pick| {
            let label = SharedString::from(name.clone());
            div()
                .id(id)
                .aria_label(label.clone())
                .test_support()
                .px_2()
                .py_1()
                .rounded(radius)
                .cursor_pointer()
                .hover(|el| el.bg(hover))
                .text_sm()
                .child(label)
                .on_click(cx.listener(move |page, _, window, cx| {
                    page.font_open = None;
                    let pick = pick(key, &name);
                    page.edit_selected(window, cx, |e| {
                        let _ = match pick {
                            Pick::Font(key, name) => e.set_property_selected(key, &name),
                            Pick::Family(key, family) => {
                                e.edit_font_selected(key, FontEdit::Family(family))
                            }
                        };
                    })
                }))
        };
        let choices: Vec<_> = match open {
            Some(FontList::Fonts) => (self.editor())
                .map(|e| e.model().scheme().font_names())
                .unwrap_or_default()
                .into_iter()
                .filter(|name| !starts_with_ignoring_case(name, CLONE_PREFIX))
                .map(|name| choice(ids::hud_maker_font_name(name), name.to_owned(), Pick::font))
                .collect(),
            Some(FontList::Families) => (families.iter())
                .map(|f| choice(ids::hud_maker_font_family(f), f.clone(), Pick::family))
                .collect(),
            None => Vec::new(),
        };
        let list = open.map(|_| {
            let list = v_flex()
                .id(ids::hud_maker_font_choices(key))
                .max_h(px(NAMES_HEIGHT))
                .track_scroll(&self.font_scroll)
                .overflow_y_scroll()
                .test_support()
                .pr(BAR_WIDTH)
                .children(choices);
            let bar = ids::hud_maker_font_choices_scrollbar(key);
            scroll_area(bar, &self.font_scroll, list)
        });
        let size_here = self.font_size_row.as_ref().is_some_and(|(part, row)| {
            row == key && self.editor().and_then(|e| e.selected()) == Some(part)
        });
        let size = if size_here {
            self.number(
                ids::hud_maker_font_size(key),
                tr!("hud_maker.panel.font.size"),
                Field::FontSize,
                cx,
            )
        } else {
            let text = size.map_or_else(String::new, |s| s.to_string());
            div()
                .id(ids::hud_maker_font_size(key))
                .aria_label(tr!("hud_maker.panel.font.size_value", text = text))
                .test_support()
                .flex_1()
                .min_w_0()
                .h_6()
                .px_2()
                .flex()
                .items_center()
                .gap_2()
                .rounded(radius)
                .border_1()
                .border_color(border)
                .cursor_text()
                .text_sm()
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(tr!("hud_maker.panel.font.size")),
                )
                .child(text)
                .on_click(
                    cx.listener(move |page, _, window, cx| page.press_font_size(key, window, cx)),
                )
                .into_any_element()
        };
        let switch = |id: ElementId,
                      inner: &'static str,
                      label: SharedString,
                      on: bool,
                      edit: fn(bool) -> FontEdit| {
            let page = cx.entity().downgrade();
            div()
                .id(id)
                .aria_label(label.clone())
                .aria_selected(on)
                .test_support()
                .child(
                    Switch::new(inner)
                        .small()
                        .checked(on)
                        .label(label)
                        .on_click(move |&on, window, cx| {
                            let _ = page.update(cx, |page, cx| {
                                page.edit_selected(window, cx, |e| {
                                    let _ = e.edit_font_selected(key, edit(on));
                                })
                            });
                        }),
                )
        };
        // The font's name has a line of its own, since an edited font's line is long; its family sits
        // beside the list buttons below it.
        v_flex()
            .gap_1()
            .child(labelled(
                div().text_sm().min_w_0().truncate(),
                ids::hud_maker_font(key),
                current,
            ))
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .text_color(muted)
                            .child(family),
                    )
                    .child(fonts)
                    .child(family_button),
            )
            .children(list)
            // Size has a line of its own, so three digits fit after its label.
            .child(h_flex().child(size))
            .child(
                h_flex()
                    .gap_4()
                    .items_center()
                    .child(switch(
                        ids::hud_maker_font_outline(key),
                        "hud-maker-font-outline-switch",
                        tr!("hud_maker.panel.font.outline"),
                        outline,
                        FontEdit::Outline,
                    ))
                    .child(switch(
                        ids::hud_maker_font_shadow(key),
                        "hud-maker-font-shadow-switch",
                        tr!("hud_maker.panel.font.shadow"),
                        shadow,
                        FontEdit::Shadow,
                    )),
            )
            .child(
                Button::new(ids::hud_maker_font_add(key))
                    .outline()
                    .small()
                    .label(tr!("hud_maker.panel.font.add_file"))
                    .on_click(cx.listener(move |page, _, _, cx| page.add_font_file(key, cx))),
            )
            .into_any_element()
    }

    // Only a floating panel drags; it starts from where it is drawn, moved off the selection or not.
    fn start_panel_drag(&mut self, panel: Panel, ev: &MouseDownEvent, cx: &mut Context<Self>) {
        let shown = self.panel_shown(panel);
        if shown.mode != PanelMode::Floating {
            return;
        }
        if panel == Panel::Parts {
            self.parts_spot = None;
        }
        let state = &mut self.panels[panel as usize];
        let home = state.place;
        state.place = PanelPlace {
            collapsed: home.collapsed,
            ..shown
        };
        state.off = None;
        state.drag = Some(PanelDrag {
            pointer: ev.position,
            from: point(px(shown.x), px(shown.y)),
            home,
        });
        cx.notify();
    }

    // A move with no button held ends the drag, in case its release never reached the window.
    fn drag_panel(&mut self, panel: Panel, ev: &MouseMoveEvent, cx: &mut Context<Self>) {
        let state = &mut self.panels[panel as usize];
        let Some(drag) = state.drag else {
            return;
        };
        if ev.pressed_button.is_none() {
            self.end_panel_drag(panel, cx);
            return;
        }
        let at = drag.from + (ev.position - drag.pointer);
        state.place.x = f32::from(at.x);
        state.place.y = f32::from(at.y);
        cx.notify();
    }

    // The place is saved once, on release, and only when the panel moved; a press without a
    // move puts back the player's place and the temporary move off the selection.
    fn end_panel_drag(&mut self, panel: Panel, cx: &mut Context<Self>) {
        let Some(drag) = self.panels[panel as usize].drag.take() else {
            return;
        };
        self.held = None;
        let place = PanelPlace {
            collapsed: self.place_of(panel).collapsed,
            ..self.panel_shown(panel)
        };
        if point(px(place.x), px(place.y)) != drag.from {
            self.set_place_of(panel, place, cx);
        } else {
            self.panels[panel as usize].place = drag.home;
            self.keep_clear();
        }
        cx.notify();
    }
}

/// The eight pin corners as a 3x3 grid, the centre left empty.
const CORNER_GRID: [[Option<PinCorner>; 3]; 3] = [
    [
        Some(PinCorner::TopLeft),
        Some(PinCorner::CenterTop),
        Some(PinCorner::TopRight),
    ],
    [
        Some(PinCorner::CenterLeft),
        None,
        Some(PinCorner::CenterRight),
    ],
    [
        Some(PinCorner::BottomLeft),
        Some(PinCorner::CenterBottom),
        Some(PinCorner::BottomRight),
    ],
];

/// The colour picker's row key for the hit flash's colour, which no file key can be.
pub(super) const HIT_FLASH_KEY: &str = "<hit flash>";

// Whether the part draws text the game can recolour, the parts the flash and a shadow take.
fn is_label_part(part: &Part) -> bool {
    part.class()
        .is_some_and(|c| c.eq_ignore_ascii_case("CExLabel") || c.eq_ignore_ascii_case("Label"))
}

// When the nearest ancestor the game shows only in some situations shows, so a child of the Uber
// meter says when the meter shows.
fn situational_ancestor(model: &HudModel, part: &Part) -> Option<&'static str> {
    std::iter::successors(part.parent(), |key| model.part(key)?.parent())
        .filter_map(|key| model.part(key)?.code_toggled())
        .find(|toggled| toggled.shows_for().is_some())
        .map(|toggled| toggled.shows_when())
}

// Whether the part's capabilities allow a row of `kind`; Layer shows for any readable part.
fn offered(kind: PropertyKind, caps: PartCaps) -> bool {
    match kind {
        PropertyKind::Position => caps.move_x() || caps.move_y(),
        PropertyKind::Size => caps.resize_wide() || caps.resize_tall(),
        PropertyKind::Layer
        | PropertyKind::Colour
        | PropertyKind::Number
        | PropertyKind::Text
        | PropertyKind::Alignment
        | PropertyKind::Toggle
        | PropertyKind::Choice
        | PropertyKind::Font => true,
        PropertyKind::Hide => caps.hide(),
        PropertyKind::Image | _ => false,
    }
}

// Whether `spec` is the class's colour slot: its Stage 1 Colour row, the one the canvas draws.
fn is_slot(spec: &PropertySpec) -> bool {
    spec.kind() == PropertyKind::Colour && spec.stage() <= 1
}

// A choice in a Font row's list: a HUD font to name, or a family to give the row's font.
enum Pick {
    Font(&'static str, String),
    Family(&'static str, String),
}

impl Pick {
    fn font(key: &'static str, name: &str) -> Pick {
        Pick::Font(key, name.to_owned())
    }

    fn family(key: &'static str, name: &str) -> Pick {
        Pick::Family(key, name.to_owned())
    }
}

fn starts_with_ignoring_case(text: &str, prefix: &str) -> bool {
    text.get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

// Where a setting is written, "From <file>, line <n>": a long path truncates, the line number
// never does.
fn origin(row: &SettingRow, muted: Hsla) -> AnyElement {
    let line = tr!("hud_maker.panel.origin.line", line = row.line);
    h_flex()
        .id(ids::hud_maker_setting_origin(&row.key))
        .aria_label(tr!(
            "hud_maker.panel.origin.full",
            file = row.file,
            line = row.line
        ))
        .test_support()
        .gap_1()
        .text_xs()
        .text_color(muted)
        .child(
            div()
                .min_w_0()
                .truncate()
                .child(tr!("hud_maker.panel.origin.file", file = row.file)),
        )
        .child(labelled(
            div().flex_none(),
            ids::hud_maker_setting_line(&row.key),
            line,
        ))
        .into_any_element()
}
