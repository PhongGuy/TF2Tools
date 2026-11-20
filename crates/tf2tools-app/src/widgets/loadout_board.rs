//! The loadout board: an "Everything" corner, class and slot headers and one cell per class
//! and slot, plus the open cell's details card. It draws over a `Loadout` the host owns and
//! reports every click and drop as a `BoardEvent`, so the host decides what reaches the game.

use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _, Size, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ElementId, Entity, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    MouseButton, ParentElement as _, Pixels, SharedString, StatefulInteractiveElement as _,
    Styled as _, TestSupportExt as _, Window, div, px, rgb,
};

use tf2tools_core::{CrosshairRef, TfClass, WeaponSlot};

use crate::i18n::names::{class_compact_name, class_name, slot_name, slot_short_name};
use crate::i18n::tr;
use crate::ids::{self, IdScope};
use crate::loadout::{BoardTarget, CellState, Loadout, SLOTS};
use crate::previews::CrosshairPreviews;
use crate::widgets::class_icon::class_short_name;
use crate::widgets::crosshair_picker::{DraggedCrosshair, preview_image};
use crate::widgets::weapon_assigner::slot_colour;

/// Below this board width the host turns the board, classes as rows and slots as columns.
pub const BOARD_COLUMNS: Pixels = px(560.);
/// The board's row header column and its cells' height.
const HEADER_WIDTH: Pixels = px(84.);
const CELL_HEIGHT: Pixels = px(68.);

/// What the host is doing to the board: which target spins, or that every control waits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BoardBusy {
    #[default]
    Idle,
    Assigning(BoardTarget),
    Resetting(BoardTarget),
    Other,
}

/// A click or drop on the board, for the host to act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoardEvent {
    /// A click: the host assigns its selection to the target.
    Assign(BoardTarget),
    /// A dragged tile landed on the target.
    Drop(BoardTarget, CrosshairRef),
    /// A right-click or the cell's menu button asks for the cell's weapons.
    Open(TfClass, WeaponSlot),
    /// The details' Reset.
    Reset(BoardTarget),
    /// The details' Close.
    Close,
}

/// The host's handler for every `BoardEvent`.
pub type OnBoard = Rc<dyn Fn(BoardEvent, &mut Window, &mut App)>;

/// One frame's board over the host's state.
pub struct LoadoutBoard<'a> {
    pub loadout: &'a Loadout,
    pub scope: IdScope,
    /// Classes as rows and slots as columns.
    pub turned: bool,
    /// Players' short class names in the header.
    pub short_names: bool,
    pub selected: Option<&'a CrosshairRef>,
    pub focus: Option<(TfClass, WeaponSlot)>,
    pub busy: BoardBusy,
    pub previews: &'a Entity<CrosshairPreviews>,
    pub name_of: &'a dyn Fn(&CrosshairRef) -> String,
}

impl LoadoutBoard<'_> {
    /// The board's headers and cells; headers assign their whole line, cells one class's slot.
    pub fn board(&self, on: OnBoard, cx: &App) -> AnyElement {
        let short = px(32.);
        let everything = self.header(
            ids::CROSSHAIRS_BOARD_EVERYTHING.into(),
            (
                tr!("crosshairs_page.board.everything"),
                tr!("crosshairs_page.board.everything"),
            ),
            None,
            BoardTarget::Everything,
            (Some(HEADER_WIDTH), short),
            &on,
            cx,
        );
        let primary = cx.theme().primary;
        let class_header = |class: TfClass, size| {
            let shown = if self.short_names {
                class_short_name(class)
            } else {
                class_compact_name(class)
            };
            self.header(
                ids::loadout_class(class),
                (class_name(class), shown),
                None,
                BoardTarget::Class(class),
                size,
                &on,
                cx,
            )
        };
        let slot_header = |slot: WeaponSlot, size| {
            self.header(
                ids::loadout_slot(slot),
                (slot_name(slot), slot_short_name(slot)),
                Some(slot_colour(slot, primary)),
                BoardTarget::Slot(slot),
                size,
                &on,
                cx,
            )
        };
        let mut rows = Vec::new();
        if self.turned {
            let mut head = h_flex().gap_1().child(everything);
            for slot in SLOTS {
                head = head.child(slot_header(slot, (None, short)));
            }
            rows.push(head);
            for class in TfClass::ALL {
                let mut row = h_flex()
                    .gap_1()
                    .child(class_header(class, (Some(HEADER_WIDTH), CELL_HEIGHT)));
                for slot in SLOTS {
                    row = row.child(self.cell(class, slot, &on, cx));
                }
                rows.push(row);
            }
        } else {
            let mut head = h_flex().gap_1().child(everything);
            for class in TfClass::ALL {
                head = head.child(class_header(class, (None, short)));
            }
            rows.push(head);
            for slot in SLOTS {
                let mut row = h_flex()
                    .gap_1()
                    .child(slot_header(slot, (Some(HEADER_WIDTH), CELL_HEIGHT)));
                for class in TfClass::ALL {
                    row = row.child(self.cell(class, slot, &on, cx));
                }
                rows.push(row);
            }
        }
        v_flex()
            .id(self.scope.id(ids::CROSSHAIRS_BOARD))
            .test_support()
            .gap_1()
            .children(rows)
            .into_any_element()
    }

    // A board header: a click assigns the selection to its whole line, and a crosshair
    // dropped on it does the same.
    #[allow(clippy::too_many_arguments)]
    fn header(
        &self,
        id: ElementId,
        (label, shown): (SharedString, SharedString),
        dot: Option<Hsla>,
        target: BoardTarget,
        size: (Option<Pixels>, Pixels),
        on: &OnBoard,
        cx: &App,
    ) -> AnyElement {
        let theme = cx.theme();
        let (hover, muted, primary, input) = (
            theme.accent,
            theme.muted_foreground,
            theme.primary,
            theme.input,
        );
        let running = self.busy == BoardBusy::Assigning(target);
        let (width, height) = size;
        let (click, drop) = (Rc::clone(on), Rc::clone(on));
        div()
            .map(|h| match width {
                Some(width) => h.flex_none().w(width),
                None => h.flex_1().flex_basis(px(0.)).min_w_0(),
            })
            .h(height)
            .id(self.scope.id(id))
            .aria_label(label)
            .test_support()
            .px_1()
            .flex()
            .items_center()
            .justify_center()
            .gap_1()
            .rounded(theme.radius)
            .text_xs()
            .font_weight(FontWeight::MEDIUM)
            .text_color(muted)
            .cursor_pointer()
            .when(running, |h| h.bg(input.opacity(0.3)))
            .hover(|h| h.bg(hover).text_color(primary))
            .drag_over::<DraggedCrosshair>(move |h, _, _, _| h.bg(hover).text_color(primary))
            .on_click(move |_, window, cx| click(BoardEvent::Assign(target), window, cx))
            .on_drop(move |dragged: &DraggedCrosshair, window, cx| {
                drop(
                    BoardEvent::Drop(target, dragged.crosshair.clone()),
                    window,
                    cx,
                )
            })
            .children(dot.map(|dot| div().flex_none().size(px(6.)).rounded_full().bg(dot)))
            .child(div().min_w_0().truncate().child(shown))
            .into_any_element()
    }

    // One board cell: the shared crosshair's drawing and name, "Mixed", or "Default" while
    // the game draws its own. A click or a drop assigns; a right-click or the corner button
    // opens its per-weapon list. Cells that use the selection are ringed.
    fn cell(&self, class: TfClass, slot: WeaponSlot, on: &OnBoard, cx: &App) -> AnyElement {
        let state = self.loadout.cell(class, slot);
        let label = match &state {
            CellState::Default => tr!("crosshairs_page.board.default").to_string(),
            CellState::Mixed => tr!("crosshairs_page.board.mixed").to_string(),
            CellState::Single(crosshair) => (self.name_of)(crosshair),
        };
        let drawing = match &state {
            CellState::Single(crosshair) => {
                let preview = self.previews.read(cx).preview(crosshair, cx);
                preview_image(preview, px(32.)).map(IntoElement::into_any_element)
            }
            CellState::Mixed => Some(
                Icon::new(IconName::Layers)
                    .with_size(Size::Size(px(16.)))
                    .text_color(cx.theme().muted_foreground)
                    .into_any_element(),
            ),
            _ => None,
        };
        let theme = cx.theme();
        let uses_selection =
            matches!((&state, self.selected), (CellState::Single(c), Some(s)) if c == s);
        let focused = self.focus == Some((class, slot));
        let border = if uses_selection {
            theme.primary
        } else if focused {
            theme.ring
        } else {
            theme.border
        };
        let (hover, muted, fg, accent) = (
            theme.accent,
            theme.muted_foreground,
            theme.foreground,
            theme.primary,
        );
        let target = BoardTarget::Cell(class, slot);
        let running = matches!(
            self.busy,
            BoardBusy::Assigning(t) | BoardBusy::Resetting(t) if t == target
        );
        let (open, right, click, drop) =
            (Rc::clone(on), Rc::clone(on), Rc::clone(on), Rc::clone(on));
        let menu = div()
            .id(self.scope.id(ids::loadout_cell_menu(class, slot)))
            .aria_label(weapons_title(class, slot))
            .test_support()
            .absolute()
            .top(px(2.))
            .right(px(2.))
            .size(px(18.))
            .rounded(px(4.))
            .flex()
            .items_center()
            .justify_center()
            .text_color(muted)
            .hover(|m| m.bg(theme.secondary).text_color(fg))
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                open(BoardEvent::Open(class, slot), window, cx);
            })
            .child(Icon::new(IconName::EllipsisVertical).with_size(Size::Size(px(12.))));
        v_flex()
            .id(self.scope.id(ids::loadout_cell(class, slot)))
            .aria_label(SharedString::from(label.clone()))
            .test_support()
            .relative()
            .flex_1()
            .flex_basis(px(0.))
            .min_w_0()
            .h(CELL_HEIGHT)
            .items_center()
            .justify_center()
            .gap_1()
            .px_1()
            .rounded(theme.radius)
            .border_1()
            .border_color(border)
            .bg(if focused {
                theme.secondary
            } else {
                theme.background
            })
            .cursor_pointer()
            .when(running, |c| c.opacity(0.6))
            .hover(|c| c.bg(hover))
            .drag_over::<DraggedCrosshair>(move |c, _, _, _| c.border_color(accent).bg(hover))
            .on_click(move |_, window, cx| click(BoardEvent::Assign(target), window, cx))
            .on_mouse_down(MouseButton::Right, move |_, window, cx| {
                right(BoardEvent::Open(class, slot), window, cx)
            })
            .on_drop(move |dragged: &DraggedCrosshair, window, cx| {
                drop(
                    BoardEvent::Drop(target, dragged.crosshair.clone()),
                    window,
                    cx,
                )
            })
            .child(
                div()
                    .flex_none()
                    .size(px(36.))
                    .rounded(px(4.))
                    .bg(if matches!(state, CellState::Default) {
                        cx.theme().muted
                    } else {
                        rgb(0x000000).into()
                    })
                    .flex()
                    .items_center()
                    .justify_center()
                    .children(drawing),
            )
            .child(
                div()
                    .w_full()
                    .text_center()
                    .truncate()
                    .text_xs()
                    .text_color(if matches!(state, CellState::Single(_)) {
                        fg
                    } else {
                        muted
                    })
                    .child(label),
            )
            .child(menu)
            .into_any_element()
    }

    /// The focused cell's weapons card, with Reset and Close around the host's `assigner`
    /// list. None while nothing is focused.
    pub fn details(&self, assigner: AnyElement, on: OnBoard, cx: &App) -> Option<AnyElement> {
        let (class, slot) = self.focus?;
        let target = BoardTarget::Cell(class, slot);
        let title = weapons_title(class, slot);
        let resetting = self.busy == BoardBusy::Resetting(target);
        let theme = cx.theme();
        let reset = Rc::clone(&on);
        Some(
            v_flex()
                .id(self.scope.id(ids::CROSSHAIRS_DETAILS))
                .aria_label(title.clone())
                .test_support()
                .gap_3()
                .p_4()
                .rounded_lg()
                .border_1()
                .border_color(theme.border)
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(title),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .child(tr!("crosshairs_page.details.hint")),
                                ),
                        )
                        .child(
                            Button::new(self.scope.id(ids::CROSSHAIRS_DETAILS_RESET))
                                .outline()
                                .small()
                                .icon(IconName::RotateCcw)
                                .label(tr!("crosshairs_page.button.reset"))
                                .loading(resetting)
                                .disabled(self.busy != BoardBusy::Idle && !resetting)
                                .on_click(move |_, window, cx| {
                                    reset(BoardEvent::Reset(target), window, cx)
                                }),
                        )
                        .child(
                            Button::new(self.scope.id(ids::CROSSHAIRS_DETAILS_CLOSE))
                                .ghost()
                                .small()
                                .icon(IconName::X)
                                .label(tr!("crosshairs_page.button.close"))
                                .on_click(move |_, window, cx| on(BoardEvent::Close, window, cx)),
                        ),
                )
                .child(assigner)
                .into_any_element(),
        )
    }
}

// The open cell's title, such as "Heavy primary weapons". Each catalogue writes the slot word
// itself, so its casing and grammar stay with the language.
fn weapons_title(class: TfClass, slot: WeaponSlot) -> SharedString {
    let class = class_name(class);
    match slot {
        WeaponSlot::Primary => tr!("crosshairs_page.details.title.primary", class = class),
        WeaponSlot::Secondary => tr!("crosshairs_page.details.title.secondary", class = class),
        WeaponSlot::Melee => tr!("crosshairs_page.details.title.melee", class = class),
        _ => crate::i18n::labelled(class, slot_name(slot)).into(),
    }
}
