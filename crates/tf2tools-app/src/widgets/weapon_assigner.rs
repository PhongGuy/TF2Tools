//! v5's per-class weapon assignment: the class tabs and bulk buttons over the class's weapon
//! rows. It never writes; a click emits `AssignRequested` and the host decides what assigning
//! means (the Crosshairs page writes the table, the profile editor saves its crosshairs).

use std::collections::HashMap;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Selectable as _, Sizable as _, Size, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Entity, EventEmitter, Hsla, InteractiveElement as _, IntoElement,
    ObjectFit, ParentElement as _, Render, ScrollHandle, StatefulInteractiveElement as _,
    Styled as _, StyledImage as _, Subscription, TestSupportExt as _, Window, div, img, px, rgb,
};

use tf2tools_core::{CrosshairRef, TfClass, Weapon, WeaponSlot, weapons};

use crate::ids;
use crate::pages::common::ORANGE_500;
use crate::previews::CrosshairPreviews;
use crate::remote_images::{ImageKey, RemoteImage, RemoteImages};
use crate::services::Services;
use crate::widgets::Named as _;
use crate::widgets::crosshair_picker::preview_image;
use crate::widgets::labelled;
use crate::widgets::scroll::{BAR_WIDTH, scroll_area};

/// Tailwind blue-500, v5's secondary slot dot.
const BLUE_500: u32 = 0x3b82f6;

/// A slot's colour, as its dot and icon frame draw it: primary in the theme's `primary`,
/// secondary blue, melee (and any later slot) orange.
pub fn slot_colour(slot: WeaponSlot, primary: Hsla) -> Hsla {
    match slot {
        WeaponSlot::Primary => primary,
        WeaponSlot::Secondary => rgb(BLUE_500).into(),
        WeaponSlot::Melee | _ => rgb(ORANGE_500).into(),
    }
}

/// The `RemoteImages` viewer name of one assigner's icon wants.
fn icon_viewer(id: impl std::fmt::Display) -> String {
    format!("weapon-icons:{id}")
}

/// What an assign click targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignTarget {
    /// One weapon, by script.
    Weapon(&'static str),
    /// The class's weapons in this slot, or all of them.
    Bulk(Option<WeaponSlot>),
}

/// A click that asks the host to assign its selection to `scripts`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssignRequested {
    pub target: AssignTarget,
    pub class: TfClass,
    /// The target's scripts in `weapons()` order.
    pub scripts: Vec<&'static str>,
}

/// The shown class changed, by a tab click or `set_class`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassChanged {
    pub class: TfClass,
}

/// One weapon's shown assignment; weapons without a cell read "None".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssignedCell {
    pub script: String,
    pub label: String,
    /// Drawn as the row's small preview when set (not in compact mode).
    pub crosshair: Option<CrosshairRef>,
}

/// A click on an assigned row's clear button, asking the host to drop that weapon's crosshair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClearRequested {
    pub script: &'static str,
}

/// What the host tells the assigner about itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AssignerState {
    pub has_selection: bool,
    /// Some operation of the host runs; rows go inert and other bulk buttons disable.
    pub busy: bool,
    /// The target whose button spins.
    pub running: Option<AssignTarget>,
}

/// The class bar and weapon rows of v5's Crosshairs page, or of the profile editor when compact.
pub struct WeaponAssigner {
    class: TfClass,
    /// Only this slot's rows show when set.
    slot: Option<WeaponSlot>,
    /// Whether the class tabs and bulk buttons show above the rows.
    bar: bool,
    cells: Vec<AssignedCell>,
    state: AssignerState,
    compact: bool,
    /// Whether assigned rows offer a clear button.
    clearable: bool,
    previews: Entity<CrosshairPreviews>,
    _previews: Subscription,
    /// The session image cache; None in compact mode or when none is installed.
    images: Option<Entity<RemoteImages>>,
    /// Weapon script to its wiki icon URL.
    icon_urls: HashMap<&'static str, String>,
    _images: Vec<Subscription>,
    /// The compact list's scroll.
    list_scroll: ScrollHandle,
}

impl EventEmitter<AssignRequested> for WeaponAssigner {}
impl EventEmitter<ClassChanged> for WeaponAssigner {}
impl EventEmitter<ClearRequested> for WeaponAssigner {}

impl WeaponAssigner {
    /// Needs the `Services` global. Scout, no cells, default state. Compact is v5's dialog
    /// variant: the rows scroll inside 144 px with no slot icon or preview, and the bulk
    /// buttons wait for a selection.
    pub fn new(compact: bool, cx: &mut Context<Self>) -> Self {
        // Row previews draw whatever the cache holds, so each landed decode batch redraws them.
        let previews = CrosshairPreviews::global(cx);
        let observe_previews = cx.observe(&previews, |_, _, cx| cx.notify());

        // Full rows draw wiki icons as they land; on release the viewer hands its wants back.
        let images = RemoteImages::global(cx).filter(|_| !compact);
        let mut subscriptions = Vec::new();
        if let Some(images) = images.clone() {
            subscriptions.push(cx.observe(&images, |_, _, cx| cx.notify()));
            let viewer = icon_viewer(cx.entity_id());
            subscriptions.push(cx.on_release(move |_, cx| {
                images.update(cx, |images, cx| images.want(&viewer, Vec::new(), cx));
            }));
        }
        let store = &cx.global::<Services>().store;
        let icon_urls = weapons()
            .iter()
            .filter_map(|w| Some((w.script_name(), store.weapon_icon_url(w)?)))
            .collect();

        let assigner = Self {
            class: TfClass::Scout,
            slot: None,
            bar: true,
            cells: Vec::new(),
            state: AssignerState::default(),
            compact,
            clearable: false,
            list_scroll: ScrollHandle::new(),
            previews,
            _previews: observe_previews,
            images,
            icon_urls,
            _images: subscriptions,
        };
        assigner.want_icons(cx);
        assigner
    }

    // Ask the cache for the shown class's icons in row order. Called on construction and on
    // a real class change, never from render.
    fn want_icons(&self, cx: &mut Context<Self>) {
        let Some(images) = &self.images else {
            return;
        };
        let keys = weapons()
            .iter()
            .filter(|w| w.class() == self.class)
            .filter_map(|w| self.icon_urls.get(w.script_name()))
            .map(|url| ImageKey::weapon_icon(url))
            .collect();
        let viewer = icon_viewer(cx.entity_id());
        images.update(cx, |images, cx| images.want(&viewer, keys, cx));
    }

    /// The class whose weapons the list shows.
    pub fn class(&self) -> TfClass {
        self.class
    }

    /// Shows `class`'s weapons, as clicking its tab does.
    pub fn set_class(&mut self, class: TfClass, cx: &mut Context<Self>) {
        if class == self.class {
            return;
        }
        self.class = class;
        self.want_icons(cx);
        cx.emit(ClassChanged { class });
        cx.notify();
    }

    /// Shows only `slot`'s rows, or every slot's.
    pub fn set_slot(&mut self, slot: Option<WeaponSlot>, cx: &mut Context<Self>) {
        self.slot = slot;
        cx.notify();
    }

    /// Shows or hides the class tabs and bulk buttons, for a host that offers its own.
    pub fn set_bar(&mut self, bar: bool, cx: &mut Context<Self>) {
        self.bar = bar;
        cx.notify();
    }

    /// Replaces the shown assignments.
    pub fn set_assigned(&mut self, cells: Vec<AssignedCell>, cx: &mut Context<Self>) {
        self.cells = cells;
        cx.notify();
    }

    /// Off by default. On, every row with a cell shows a clear button.
    pub fn set_clearable(&mut self, clearable: bool, cx: &mut Context<Self>) {
        self.clearable = clearable;
        cx.notify();
    }

    /// Replaces what the host reports about its selection and work.
    pub fn set_state(&mut self, state: AssignerState, cx: &mut Context<Self>) {
        self.state = state;
        cx.notify();
    }

    // Asks the host to assign to `target`, naming its scripts with the same class and slot
    // filter the page's bulk assign always used.
    fn request(&self, target: AssignTarget, cx: &mut Context<Self>) {
        let class = self.class;
        let scripts = match target {
            AssignTarget::Weapon(script) => vec![script],
            AssignTarget::Bulk(slot) => weapons()
                .iter()
                .filter(|w| w.class() == class && slot.is_none_or(|slot| w.slot() == slot))
                .map(Weapon::script_name)
                .collect(),
        };
        cx.emit(AssignRequested {
            target,
            class,
            scripts,
        });
    }

    // v5's tabs row: the nine class tabs in a muted pill on the left, the bulk buttons on the
    // right, the running bulk button spinning. Where v5's row overflows the page, the bulk
    // buttons wrap to a right-aligned line of their own so they stay in the window.
    fn class_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (muted_fg, input) = (theme.muted_foreground, theme.input);
        // v5's dark trigger: muted text, and the active one raised on input/30 with a border.
        let tabs = TfClass::ALL.into_iter().map(|class| {
            let active = class == self.class;
            Button::new(ids::class_tab(&class.name().to_lowercase()))
                .ghost()
                .small()
                .label(class.name())
                .selected(active)
                .h(px(29.))
                .border_1()
                .map(|tab| {
                    if active {
                        tab.bg(input.opacity(0.3)).border_color(input)
                    } else {
                        tab.border_color(gpui_kit::transparent_black())
                            .text_color(muted_fg)
                    }
                })
                .on_click(cx.listener(move |assigner, _, _, cx| assigner.set_class(class, cx)))
        });
        let bulk = [
            (
                ids::CROSSHAIRS_BULK_ALL,
                None,
                format!("All {}", self.class.name()),
            ),
            (
                ids::CROSSHAIRS_BULK_PRIMARY,
                Some(WeaponSlot::Primary),
                "Primary".into(),
            ),
            (
                ids::CROSSHAIRS_BULK_SECONDARY,
                Some(WeaponSlot::Secondary),
                "Secondary".into(),
            ),
            (
                ids::CROSSHAIRS_BULK_MELEE,
                Some(WeaponSlot::Melee),
                "Melee".into(),
            ),
        ]
        .into_iter()
        .map(|(id, slot, label)| {
            let target = AssignTarget::Bulk(slot);
            let running = self.state.running == Some(target);
            // The page toasts a selection-less click; v5's dialog simply disables the buttons.
            let waiting = self.compact && !self.state.has_selection;
            Button::new(id)
                .secondary()
                .label(label)
                .h(px(28.))
                .px_2()
                .text_xs()
                .loading(running)
                .disabled((self.state.busy && !running) || waiting)
                .on_click(cx.listener(move |assigner, _, _, cx| assigner.request(target, cx)))
        });
        // The bulk group's auto margin right-aligns it; justify_between on top would add the
        // free space a second time and push it past the row's end.
        h_flex()
            .flex_wrap()
            .gap_2()
            .child(
                h_flex()
                    .flex_none()
                    .h(px(36.))
                    .p(px(3.))
                    .rounded_lg()
                    .bg(theme.muted)
                    .children(tabs),
            )
            .child(h_flex().flex_none().ml_auto().gap_1().children(bulk))
    }

    // The class's weapon rows, separated by dividers; compact mode scrolls them in 144 px.
    fn weapon_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let border = cx.theme().border;
        let rows = weapons()
            .iter()
            .filter(|w| w.class() == self.class && self.slot.is_none_or(|s| w.slot() == s))
            .enumerate()
            .map(|(ix, weapon)| {
                div()
                    .when(ix > 0, |row| row.border_t_1().border_color(border))
                    .child(self.weapon_row(weapon, cx))
            })
            .collect::<Vec<_>>();
        let list = v_flex()
            .id(ids::WEAPON_LIST)
            .test_support()
            .when(self.compact, |list| {
                list.track_scroll(&self.list_scroll)
                    .overflow_y_scroll()
                    .h(px(144.))
                    .pr(BAR_WIDTH)
            })
            .children(rows);
        if self.compact {
            scroll_area(ids::WEAPON_LIST_SCROLLBAR, &self.list_scroll, list).into_any_element()
        } else {
            list.into_any_element()
        }
    }

    // One weapon: its wiki icon in a slot-coloured frame, or the slot square, and dot,
    // name, the assigned crosshair's preview and name, and a check mark when assigned, or a
    // spinner in its slot while this weapon is being assigned. Inert, and its content dimmed,
    // without a selection or while the host is busy.
    fn weapon_row(&self, weapon: &'static Weapon, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (muted, primary, hover) = (theme.muted_foreground, theme.primary, theme.accent);
        let script = weapon.script_name();
        let cell = self.cells.iter().find(|c| c.script == script);
        let label = cell.map_or("None", |c| c.label.as_str()).to_owned();
        let preview = cell
            .and_then(|c| c.crosshair.as_ref())
            .filter(|_| !self.compact)
            .and_then(|c| preview_image(self.previews.read(cx).preview(c, cx), px(16.)));
        let dot = slot_colour(weapon.slot(), primary);
        let check = if self.state.running == Some(AssignTarget::Weapon(script)) {
            div()
                .id(ids::weapon_spinner(script))
                .test_support()
                .flex_none()
                .size(px(14.))
                .child(Spinner::new().with_size(Size::Size(px(14.))).color(primary))
                .into_any_element()
        } else if cell.is_some() {
            div()
                .id(ids::weapon_check(script))
                .test_support()
                .child(
                    Icon::new(IconName::Check)
                        .with_size(Size::Size(px(14.)))
                        .text_color(primary),
                )
                .into_any_element()
        } else {
            div().flex_none().size(px(14.)).into_any_element()
        };
        // The clear button works without a selection, and stops its click so the row behind
        // it never assigns. An unassigned row keeps a same-size empty box so the column lines up.
        let clear = self.clearable.then(|| {
            if cell.is_none() {
                return div().flex_none().size_5().into_any_element();
            }
            Button::new(ids::weapon_clear(script))
                .ghost()
                .xsmall()
                .icon(IconName::X)
                .named("Clear crosshair")
                .disabled(self.state.busy)
                .on_click(cx.listener(move |_, _, _, cx| {
                    cx.stop_propagation();
                    cx.emit(ClearRequested { script });
                }))
                .into_any_element()
        });
        let active = self.state.has_selection && !self.state.busy;
        let icon = (!self.compact).then(|| {
            let ready = self
                .images
                .as_ref()
                .zip(self.icon_urls.get(script))
                .map(|(i, u)| i.read(cx).image(&ImageKey::weapon_icon(u)));
            // One fixed box either way, so a row keeps its height as its icon lands.
            let slot = div().flex_none().w(px(36.)).h(px(28.));
            match ready {
                Some(RemoteImage::Ready(image)) => slot
                    .id(ids::weapon_icon(script))
                    .test_support()
                    .border_2()
                    .border_color(dot)
                    .rounded(px(4.))
                    .child(img(image).size_full().object_fit(ObjectFit::Contain))
                    .into_any_element(),
                _ => {
                    let square = cx.global::<Services>().assets.slot_icon(weapon.slot());
                    slot.flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .id(ids::weapon_slot_icon(script))
                                .test_support()
                                .size(px(16.))
                                .child(img(square).size_full()),
                        )
                        .into_any_element()
                }
            }
        });
        // Only the row's content dims; the clear button beside it still works.
        let content = h_flex()
            .flex_1()
            .min_w_0()
            .gap(px(8.))
            .when(!active, |content| content.opacity(0.5))
            .children(icon)
            .child(div().flex_none().size(px(8.)).rounded_full().bg(dot))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_sm()
                    .child(weapon.display_name()),
            )
            .children(preview)
            .child(labelled(
                div().text_xs().text_color(muted).max_w(px(180.)).truncate(),
                ids::weapon_assigned(script),
                label,
            ))
            .child(check);
        h_flex()
            .id(ids::weapon_row(script))
            .aria_label(weapon.display_name())
            .test_support()
            .gap(px(8.))
            .px_2()
            .py(px(6.))
            .child(content)
            .children(clear)
            .when(active, |row| {
                row.cursor_pointer()
                    .hover(|row| row.bg(hover))
                    .on_click(cx.listener(move |assigner, _, _, cx| {
                        assigner.request(AssignTarget::Weapon(script), cx)
                    }))
            })
    }
}

impl Render for WeaponAssigner {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let bar = self.bar.then(|| self.class_bar(cx));
        v_flex().gap_4().children(bar).child(self.weapon_list(cx))
    }
}
