//! The HUD Maker's Options panel: the HUD's options in core's groups, each group under its name
//! with a Reset, drawn as the side panel's body while no part is selected. Under them, the
//! Transparent viewmodels addon, which applies to every HUD and is never stored in one. A copy
//! also lists its own options under "<HUD>'s options"; a selected part shows the ones it reads.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::color_picker::ColorPicker;
use gpui_kit::component::searchable_list::SearchableListItem;
use gpui_kit::component::select::Select;
use gpui_kit::component::slider::Slider;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, Task, TestSupportExt as _, Window,
    div,
};
use tf2tools_core::{AddonName, Choice, OptionKind, OptionSpec, OptionValue, Tf2Install};

use super::HudMakerPage;
use super::tip::tipped;
use crate::ids;
use crate::library_events::LibraryEvents;
use crate::local_messages::describe_tf2_install;
use crate::pages::crosshair_maker::model::hsla_of_rgba;
use crate::services::{OpError, Services, require_tf2, run_blocking};
use crate::shell::confirm::{Confirm, confirm};
use crate::shell::toast::toast_error;
use crate::widgets::labelled;

const RESTART: &str = "Takes effect after TF2 restarts.";
const ALREADY_ON: &str = "This HUD already does this.";
const VIEWMODELS: &str = "mastercomfig-addon-transparent-viewmodels.vpk";
const GET_VIEWMODELS: &str = "Get Transparent Viewmodels from the Addon store first.";
const NO_TF2: &str =
    "TF2 wasn't found. Set the TF2 folder in Settings to use Transparent viewmodels.";
/// A Choice option with more choices than this shows as a menu instead of buttons.
pub(super) const MAX_CHOICE_BUTTONS: usize = 8;

/// One choice of a copy's own option, as a row of its menu.
#[derive(Debug, Clone)]
pub(super) struct ChoiceRow {
    option: SharedString,
    key: SharedString,
    label: SharedString,
}

impl ChoiceRow {
    pub(super) fn of(option: &str, choice: &Choice) -> ChoiceRow {
        ChoiceRow {
            option: option.to_owned().into(),
            key: choice.key().to_owned().into(),
            label: choice.label().to_owned().into(),
        }
    }
}

impl SearchableListItem for ChoiceRow {
    type Value = SharedString;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    // Kit rows carry no test id, so each row draws its own for headless clicks and the pilot.
    fn render(&self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .id(ids::hud_maker_option_choice(&self.option, &self.key))
            .test_support()
            .child(self.label.clone())
    }

    fn value(&self) -> &SharedString {
        &self.key
    }
}

/// Where the Transparent viewmodels addon stands: not in the library, in the library without
/// TF2 to put it in, in the library and off or on, or being turned `on` or off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Viewmodels {
    Absent,
    NoTf2,
    Off,
    On,
    Busy { on: bool },
}

impl HudMakerPage {
    /// The HUD section: per group in core's order, its name and Reset over its option rows,
    /// then the viewmodels switch and the recipe sections.
    pub(super) fn hud_rows(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(editor) = self.editor() else {
            return Vec::new();
        };
        let options = editor.options();
        // Groups keep the order their first option has in core's table.
        let mut groups: Vec<&'static str> = Vec::new();
        for spec in &options {
            if !groups.contains(&spec.group()) {
                groups.push(spec.group());
            }
        }
        let mut section = (v_flex().id(ids::HUD_MAKER_OPTIONS).test_support().gap_3())
            .children(self.preview_bar(cx))
            .children(self.menu_background_rows(cx));
        for group in groups {
            // A preview holds every control: the rows show its values and wait for Apply or
            // Cancel, since an edit would end it anyway.
            let held = editor.previewing();
            let changed = editor.option_group_changed(group) && !held;
            let specs: Vec<_> = options.iter().filter(|s| s.group() == group).collect();
            // A restart several options share is said once, under the group's name.
            let shared = specs.len() > 1 && specs.iter().all(|s| s.restart());
            let muted = cx.theme().muted_foreground;
            let mut rows = v_flex()
                .gap_2()
                .child(self.group_heading(group, changed, cx))
                .when(shared, |el| {
                    el.child(labelled(
                        div().text_xs().text_color(muted),
                        ids::hud_maker_option_group_note(group),
                        RESTART,
                    ))
                });
            // Colours sit two to a line, so the sixteen item colours keep the panel short
            // enough to leave the lower HUD clickable.
            let mut swatches = h_flex().flex_wrap().gap_y_2();
            for spec in specs {
                let Some(row) = self.option_row(spec, !shared, cx) else {
                    continue;
                };
                if matches!(spec.kind(), OptionKind::Colour { .. }) {
                    swatches = swatches.child(div().w_1_2().min_w_0().child(row));
                } else {
                    rows = rows.child(row);
                }
            }
            section = section.child(rows.child(swatches));
        }
        vec![
            section
                .children(self.hud_option_section(cx))
                .child(self.reset_all_row(cx))
                .child(self.viewmodels_rows(cx))
                .child(self.recipe_rows(cx))
                .into_any_element(),
        ]
    }

    // The viewmodels switch with its "Applies to every HUD" note, or where to get the addon.
    // The note names the scope, so the block has no heading line of its own: one more line
    // would push the docked panel over the lower HUD at common window sizes.
    fn viewmodels_rows(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let note = |id, text| labelled(div().text_xs().text_color(muted), id, text);
        let (on, busy) = match self.viewmodels {
            Viewmodels::Absent => {
                return note(ids::HUD_MAKER_VIEWMODELS.into(), GET_VIEWMODELS).into_any_element();
            }
            Viewmodels::NoTf2 => {
                return note(ids::HUD_MAKER_VIEWMODELS.into(), NO_TF2).into_any_element();
            }
            Viewmodels::Off => (false, false),
            Viewmodels::On => (true, false),
            Viewmodels::Busy { on } => (on, true),
        };
        let page = cx.entity().downgrade();
        let switch = Switch::new(ids::HUD_MAKER_VIEWMODELS)
            .small()
            .checked(on)
            .label("Transparent viewmodels")
            .disabled(busy)
            .on_click(move |&on, window, cx| {
                let _ = page.update(cx, |page, cx| page.toggle_viewmodels(on, window, cx));
            });
        v_flex()
            .gap_1()
            .child(switch)
            .child(note(
                ids::hud_maker_option_note("viewmodels"),
                "Applies to every HUD",
            ))
            .into_any_element()
    }

    /// True while a viewmodels install or removal runs and the switch waits for it.
    pub fn viewmodels_held(&self) -> bool {
        matches!(self.viewmodels, Viewmodels::Busy { .. })
    }

    /// Reads the viewmodels addon's state off the UI thread; called when a HUD lands and when
    /// the library changes, never per render. A running toggle's own result wins, so a read
    /// while it runs does nothing.
    pub(super) fn read_viewmodels(&mut self, cx: &mut Context<Self>) {
        if self.viewmodels_held() {
            return;
        }
        let tf2 = require_tf2(cx).ok();
        let read = run_blocking(cx, move |s| Ok(viewmodels_state(s, tf2.as_ref())));
        self.land_viewmodels(read, cx);
    }

    // Installs or removes the addon in tf/custom at once; the HUD's changes are untouched. The
    // switch holds the asked state, disabled, until the fresh state lands; a failure toasts.
    fn toggle_viewmodels(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.viewmodels_held() {
            return;
        }
        let tf2 = match require_tf2(cx) {
            Ok(tf2) => tf2,
            Err(err) => return toast_error(window, cx, &err),
        };
        let Some(name) = AddonName::new(VIEWMODELS) else {
            return;
        };
        self.viewmodels = Viewmodels::Busy { on };
        cx.notify();
        let running = run_blocking(cx, move |s| {
            let done = match on {
                true => (s.library.install_addon(&name, &tf2).map(drop))
                    .map_err(|e| describe_tf2_install(&e).into_op("Failed to install addon")),
                false => (s.library.uninstall_addon(&name, &tf2).map(drop))
                    .map_err(|e| describe_tf2_install(&e).into_op("Failed to uninstall addon")),
            };
            Ok((done, viewmodels_state(s, Some(&tf2))))
        });
        let win = self.win;
        let toggled = cx.spawn(async move |_, cx| {
            let (done, state) = running
                .await
                .unwrap_or_else(|err| (Err(err), Viewmodels::Absent));
            cx.update(|cx| match &done {
                Ok(()) => LibraryEvents::changed(cx),
                Err(err) => {
                    let _ = win.update(cx, |_, window, cx| toast_error(window, cx, err));
                }
            });
            Ok(state)
        });
        self.land_viewmodels(toggled, cx);
    }

    // Takes the state `task` gives when no later read or toggle started since.
    fn land_viewmodels(&mut self, task: Task<Result<Viewmodels, OpError>>, cx: &mut Context<Self>) {
        self.viewmodels_reads = self.viewmodels_reads.wrapping_add(1);
        let read = self.viewmodels_reads;
        cx.spawn(async move |page, cx| {
            let state = task.await.unwrap_or(Viewmodels::Absent);
            let _ = page.update(cx, |page, cx| {
                if page.viewmodels_reads == read {
                    page.viewmodels = state;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    // "Reset all options" under every option group, live only while some option holds a
    // choice and no preview runs.
    fn reset_all_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let changed = (self.editor()).is_some_and(|e| e.options_changed() && !e.previewing());
        let reset = Button::new(ids::HUD_MAKER_OPTIONS_RESET_ALL)
            .ghost()
            .xsmall()
            .icon(IconName::RotateCcw)
            .label("Reset all options")
            .disabled(!changed)
            .on_click(cx.listener(|page, _, window, cx| page.reset_all_options(window, cx)));
        h_flex().justify_end().child(reset).into_any_element()
    }

    // Asks first, since it drops every option at once; a dialog closed unanswered counts as no.
    // The answer applies only to the HUD that was open when asked.
    pub(super) fn reset_all_options(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let answer = confirm(Confirm::reset_all_options(), window, cx);
        let load = self.loads;
        let win = self.win;
        cx.spawn(async move |page, cx| {
            if !answer.recv().await.unwrap_or(false) {
                return;
            }
            let _ = win.update(cx, |_, window, cx| {
                let _ = page.update(cx, |page, cx| {
                    if page.loads == load {
                        page.edit_selected(window, cx, |e| {
                            e.reset_all_options();
                        });
                    }
                });
            });
        })
        .detach();
    }

    // The group's name beside its Reset, which is live only while the group holds a choice.
    fn group_heading(
        &self,
        group: &'static str,
        changed: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let name = labelled(
            div()
                .text_sm()
                .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                .text_color(theme.foreground),
            ids::hud_maker_option_group(group),
            group,
        );
        let reset_id = ids::hud_maker_option_reset(group);
        let reset = Button::new(reset_id.clone())
            .ghost()
            .xsmall()
            .icon(IconName::RotateCcw)
            .accessibility_label(format!("Reset {group}"))
            .disabled(!changed)
            .on_click(cx.listener(move |page, _, window, cx| {
                page.edit_selected(window, cx, |e| {
                    e.reset_option_group(group);
                })
            }));
        h_flex()
            .justify_between()
            .items_center()
            .child(name)
            .child(tipped(
                &reset_id,
                "Reset these options to the HUD's own",
                reset,
            ))
            .into_any_element()
    }

    // One option's control with its note under it, the restart line only when `own_restart`;
    // kinds the panel does not draw are left out.
    fn option_row(
        &self,
        spec: &'static OptionSpec,
        own_restart: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let control = match spec.kind() {
            OptionKind::Toggle => self.option_switch(spec, cx),
            OptionKind::Number { .. } => self.option_slider_row(spec, cx)?,
            OptionKind::Colour { .. } => self.option_colour_row(spec, cx),
            _ => return None,
        };
        let note = [
            (own_restart && spec.restart()).then_some(RESTART),
            spec.tooltip(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
        let muted = cx.theme().muted_foreground;
        Some(
            v_flex()
                .gap_1()
                .child(control)
                .when(!note.is_empty(), |el| {
                    el.child(labelled(
                        div().text_xs().text_color(muted),
                        ids::hud_maker_option_note(spec.id()),
                        SharedString::from(note),
                    ))
                })
                .into_any_element(),
        )
    }

    // A switch whose click chooses the next state as one undo step. A switch this HUD already
    // has on and cannot turn off reads on and stays still.
    fn option_switch(&self, spec: &'static OptionSpec, cx: &mut Context<Self>) -> AnyElement {
        let id = spec.id();
        let editor = self.editor();
        let locked = editor.is_some_and(|e| !e.pristine().option_can_turn_off(id));
        let held = editor.is_some_and(|e| e.previewing());
        let on = locked
            || editor.and_then(|e| e.shown_option_value(id)) == Some(OptionValue::Bool(true));
        let page = cx.entity().downgrade();
        let switch = Switch::new(ids::hud_maker_option(id))
            .small()
            .checked(on)
            .label(spec.label())
            .disabled(locked || held)
            .on_click(move |&on, window, cx| {
                let _ = page.update(cx, |page, cx| {
                    page.edit_selected(window, cx, |e| {
                        let _ = e.set_option(id, OptionValue::Bool(on));
                    })
                });
            });
        match locked {
            true => tipped(&ids::hud_maker_option(id), ALREADY_ON, switch),
            false => switch.into_any_element(),
        }
    }

    // The option's name over its slider, with the value in force beside the slider.
    fn option_slider_row(
        &self,
        spec: &'static OptionSpec,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let id = spec.id();
        let slider = self.option_slider(id)?;
        let value = match self.editor().and_then(|e| e.shown_option_value(id)) {
            Some(OptionValue::Int(v)) => v.to_string(),
            _ => String::new(),
        };
        let held = self.editor().is_some_and(|e| e.previewing());
        let theme = cx.theme();
        Some(
            v_flex()
                .gap_1()
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.foreground)
                        .child(spec.label()),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(
                            div()
                                .id(ids::hud_maker_option(id))
                                .test_support()
                                .flex_1()
                                .min_w_0()
                                .child(Slider::new(&slider).disabled(held)),
                        )
                        .child(labelled(
                            div().w_8().text_sm().text_color(theme.muted_foreground),
                            ids::hud_maker_option_value(id),
                            SharedString::from(value),
                        )),
                )
                .into_any_element(),
        )
    }

    // A swatch of the colour in force beside the option's name, its hex as a tooltip; pressing
    // it opens the option picker on this option, under the swatch. The picker's own trigger sits
    // clipped in a zero-size holder at the swatch's foot, so it takes no room and no clicks.
    fn option_colour_row(&self, spec: &'static OptionSpec, cx: &mut Context<Self>) -> AnyElement {
        let id = spec.id();
        let theme = cx.theme();
        let shown = match self.editor().and_then(|e| e.shown_option_value(id)) {
            Some(OptionValue::Rgba(rgba)) => Some(rgba),
            _ => None,
        };
        let fill = shown.map_or(theme.background, hsla_of_rgba);
        let hex = shown.map(|[r, g, b, _]| SharedString::from(format!("#{r:02X}{g:02X}{b:02X}")));
        let held = self.editor().is_some_and(|e| e.previewing());
        let swatch = div()
            .id(ids::hud_maker_option(id))
            .aria_label(spec.label())
            .test_support()
            .flex_none()
            .w_10()
            .h_6()
            .rounded(theme.radius)
            .border_1()
            .border_color(theme.border)
            .bg(fill)
            .when_some(hex, |el, hex| {
                el.tooltip(move |window, cx| Tooltip::new(hex.clone()).build(window, cx))
            })
            .when(held, |el| el.opacity(0.5))
            .when(!held, |el| {
                el.cursor_pointer().on_click(
                    cx.listener(move |page, _, window, cx| {
                        page.choose_option_colour(id, window, cx)
                    }),
                )
            });
        let picker = (self.active_option() == Some(id)).then(|| {
            div()
                .id(ids::HUD_MAKER_OPTION_PICKER)
                .test_support()
                .absolute()
                .left_0()
                .bottom_0()
                .size_0()
                .overflow_hidden()
                .child(
                    ColorPicker::new(self.option_colour_state())
                        .icon(Icon::empty())
                        .accessibility_label(spec.label()),
                )
        });
        h_flex()
            .gap_2()
            .items_center()
            .child(div().relative().child(swatch).children(picker))
            .child(
                div()
                    .text_sm()
                    .text_color(theme.foreground)
                    .child(spec.label()),
            )
            .into_any_element()
    }

    // A copy's own options under "<HUD>'s options": the misfit line, then per group in spec
    // order its name and Reset over its rows. `None` for a HUD with none to show.
    fn hud_option_section(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let options = self.editor()?.hud_options()?.clone();
        let misfits = options.misfits();
        if options.specs().is_empty() && misfits == 0 {
            return None;
        }
        let theme = cx.theme();
        let (muted, foreground) = (theme.muted_foreground, theme.foreground);
        let misfit_line = match misfits {
            0 => None,
            1 => Some("1 option doesn't fit this version of the HUD.".to_owned()),
            n => Some(format!("{n} options don't fit this version of the HUD.")),
        };
        let mut section = v_flex()
            .id(ids::HUD_MAKER_HUD_OPTIONS)
            .test_support()
            .gap_3()
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                    .text_color(foreground)
                    .child(format!("{}'s options", self.saved_name)),
            )
            .when_some(misfit_line, |el, line| {
                el.child(labelled(
                    div().text_xs().text_color(muted),
                    ids::HUD_MAKER_HUD_OPTIONS_MISFITS,
                    line,
                ))
            });
        let mut groups: Vec<&str> = Vec::new();
        for spec in options.specs() {
            if !groups.contains(&spec.group()) {
                groups.push(spec.group());
            }
        }
        for group in groups {
            let mut rows = v_flex().gap_2().child(self.hud_group_heading(group, cx));
            for spec in options.specs().iter().filter(|s| s.group() == group) {
                rows = rows.children(self.hud_option_row(spec, cx));
            }
            section = section.child(rows);
        }
        Some(section.into_any_element())
    }

    // A HUD option group's name beside its Reset, which is live only while it holds a choice.
    fn hud_group_heading(&self, group: &str, cx: &mut Context<Self>) -> AnyElement {
        let changed =
            (self.editor()).is_some_and(|e| e.hud_option_group_changed(group) && !e.previewing());
        let name = labelled(
            div()
                .text_sm()
                .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                .text_color(cx.theme().foreground),
            ids::hud_maker_hud_option_group(group),
            group.to_owned(),
        );
        let reset_id = ids::hud_maker_hud_option_reset(group);
        let owned = group.to_owned();
        let reset = Button::new(reset_id.clone())
            .ghost()
            .xsmall()
            .icon(IconName::RotateCcw)
            .accessibility_label(format!("Reset {group}"))
            .disabled(!changed)
            .on_click(cx.listener(move |page, _, window, cx| {
                page.edit_selected(window, cx, |e| {
                    e.reset_hud_option_group(&owned);
                })
            }));
        h_flex()
            .justify_between()
            .items_center()
            .child(name)
            .child(tipped(
                &reset_id,
                "Reset these options to the HUD's own",
                reset,
            ))
            .into_any_element()
    }

    /// "This HUD's options" under a selected part's sections: the copy's own options that
    /// change it; `None` when none do.
    pub(super) fn part_hud_options(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let editor = self.editor()?;
        let options = editor.hud_options()?.clone();
        let part = editor.selected()?.clone();
        let specs = options.for_part(editor.model(), &part);
        if specs.is_empty() {
            return None;
        }
        let heading = div()
            .text_sm()
            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
            .text_color(cx.theme().foreground)
            .child("This HUD's options");
        let rows: Vec<AnyElement> = (specs.into_iter())
            .filter_map(|spec| self.hud_option_row(spec, cx))
            .collect();
        Some(
            v_flex()
                .id(ids::HUD_MAKER_PART_OPTIONS)
                .test_support()
                .gap_2()
                .child(heading)
                .children(rows)
                .into_any_element(),
        )
    }

    /// One of the copy's own options: its control by kind, its tooltip as a note under it.
    /// The Options panel and a part's panel both draw these, never at the same time.
    pub(super) fn hud_option_row(
        &self,
        spec: &OptionSpec,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let editor = self.editor()?;
        let value = editor.hud_option_value(spec.id());
        let held = editor.previewing();
        let control = match spec.kind() {
            OptionKind::Toggle => self.hud_switch(spec, value, held, cx),
            OptionKind::Choice(choices) => self.hud_choices(spec, choices, value, held, cx),
            OptionKind::Number { .. } => self.hud_slider(spec, value, held, cx)?,
            OptionKind::Colour { .. } => self.hud_colour(spec, value, held, cx),
            _ => return None,
        };
        let muted = cx.theme().muted_foreground;
        Some(
            v_flex()
                .gap_1()
                .child(control)
                .when_some(spec.tooltip(), |el, note| {
                    el.child(labelled(
                        div().text_xs().text_color(muted),
                        ids::hud_maker_option_note(spec.id()),
                        SharedString::from(note.to_owned()),
                    ))
                })
                .into_any_element(),
        )
    }

    // A switch whose click chooses the next state as one undo step.
    fn hud_switch(
        &self,
        spec: &OptionSpec,
        value: Option<OptionValue>,
        held: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = spec.id().to_owned();
        let page = cx.entity().downgrade();
        Switch::new(ids::hud_maker_option(&id))
            .small()
            .checked(value == Some(OptionValue::Bool(true)))
            .label(spec.label().to_owned())
            .disabled(held)
            .on_click(move |&on, window, cx| {
                let _ = page.update(cx, |page, cx| {
                    page.set_hud_option(&id, OptionValue::Bool(on), window, cx)
                });
            })
            .into_any_element()
    }

    // The option's name over one small button per choice, the chosen one filled, or over a
    // menu when there are too many choices to lay out.
    fn hud_choices(
        &self,
        spec: &OptionSpec,
        choices: &[Choice],
        value: Option<OptionValue>,
        held: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = spec.id();
        let name = div()
            .text_sm()
            .text_color(cx.theme().foreground)
            .child(spec.label().to_owned());
        let chosen = match value {
            Some(OptionValue::Pick(key)) => Some(key),
            _ => None,
        };
        let control = match self.hud_select(id) {
            Some(select) => div()
                .id(ids::hud_maker_option(id))
                .test_support()
                .child(
                    Select::new(&select)
                        .small()
                        .disabled(held)
                        .accessibility_label(spec.label().to_owned()),
                )
                .into_any_element(),
            None => {
                let buttons = choices.iter().map(|choice| {
                    let (option, key) = (id.to_owned(), choice.key().to_owned());
                    let picked = chosen.as_deref() == Some(choice.key());
                    let button = Button::new(ids::hud_maker_option_choice(id, &key))
                        .xsmall()
                        .label(choice.label().to_owned())
                        .accessibility_label(choice.label().to_owned())
                        .disabled(held)
                        .on_click(cx.listener(move |page, _, window, cx| {
                            let value = OptionValue::Pick(key.clone());
                            page.set_hud_option(&option, value, window, cx)
                        }));
                    match picked {
                        true => button.primary(),
                        false => button.outline(),
                    }
                });
                h_flex()
                    .id(ids::hud_maker_option(id))
                    .test_support()
                    .flex_wrap()
                    .gap_1()
                    .children(buttons)
                    .into_any_element()
            }
        };
        v_flex()
            .gap_1()
            .child(name)
            .child(control)
            .into_any_element()
    }

    // The option's name over its slider, with the value in force beside it. Only the release
    // records a value, since each value means rebuilding the HUD's files.
    fn hud_slider(
        &self,
        spec: &OptionSpec,
        value: Option<OptionValue>,
        held: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let id = spec.id();
        let slider = self.hud_slider_state(id)?;
        let value = match value {
            Some(OptionValue::Int(v)) => v.to_string(),
            _ => String::new(),
        };
        let theme = cx.theme();
        Some(
            v_flex()
                .gap_1()
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.foreground)
                        .child(spec.label().to_owned()),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(
                            div()
                                .id(ids::hud_maker_option(id))
                                .test_support()
                                .flex_1()
                                .min_w_0()
                                .child(Slider::new(&slider).disabled(held)),
                        )
                        .child(labelled(
                            div().w_8().text_sm().text_color(theme.muted_foreground),
                            ids::hud_maker_option_value(id),
                            SharedString::from(value),
                        )),
                )
                .into_any_element(),
        )
    }

    // A swatch of the colour in force beside the option's name; pressing it opens the option
    // picker on this option, which records the picked colour once it closes.
    fn hud_colour(
        &self,
        spec: &OptionSpec,
        value: Option<OptionValue>,
        held: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = spec.id().to_owned();
        let theme = cx.theme();
        let shown = match value {
            Some(OptionValue::Rgba(rgba)) => Some(rgba),
            _ => None,
        };
        let fill = shown.map_or(theme.background, hsla_of_rgba);
        let hex = shown.map(|[r, g, b, _]| SharedString::from(format!("#{r:02X}{g:02X}{b:02X}")));
        let active = self.active_hud_option() == Some(id.as_str());
        let open = id.clone();
        let swatch = div()
            .id(ids::hud_maker_option(&id))
            .aria_label(spec.label().to_owned())
            .test_support()
            .flex_none()
            .w_10()
            .h_6()
            .rounded(theme.radius)
            .border_1()
            .border_color(theme.border)
            .bg(fill)
            .when_some(hex, |el, hex| {
                el.tooltip(move |window, cx| Tooltip::new(hex.clone()).build(window, cx))
            })
            .when(held, |el| el.opacity(0.5))
            .when(!held, |el| {
                el.cursor_pointer()
                    .on_click(cx.listener(move |page, _, window, cx| {
                        page.choose_hud_option_colour(&open, window, cx)
                    }))
            });
        let picker = active.then(|| {
            div()
                .id(ids::HUD_MAKER_OPTION_PICKER)
                .test_support()
                .absolute()
                .left_0()
                .bottom_0()
                .size_0()
                .overflow_hidden()
                .child(
                    ColorPicker::new(self.option_colour_state())
                        .icon(Icon::empty())
                        .accessibility_label(spec.label().to_owned()),
                )
        });
        h_flex()
            .gap_2()
            .items_center()
            .child(div().relative().child(swatch).children(picker))
            .child(
                div()
                    .text_sm()
                    .text_color(theme.foreground)
                    .child(spec.label().to_owned()),
            )
            .into_any_element()
    }
}

// The addon's state for `tf2`; no library copy or an unreadable library reads Absent, and a
// library copy with no TF2 to put it in reads NoTf2.
fn viewmodels_state(s: &Services, tf2: Option<&Tf2Install>) -> Viewmodels {
    match s.library.list_addons(tf2) {
        Ok(addons) => match addons.iter().find(|a| a.name().as_str() == VIEWMODELS) {
            Some(_) if tf2.is_none() => Viewmodels::NoTf2,
            Some(a) if a.is_installed() => Viewmodels::On,
            Some(_) => Viewmodels::Off,
            None => Viewmodels::Absent,
        },
        Err(err) => {
            tracing::warn!(error = %err, "reading the transparent viewmodels addon failed");
            Viewmodels::Absent
        }
    }
}
