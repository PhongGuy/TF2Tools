//! The game situation menu on the HUD scene's strip: the class, health, ammo and charge, the
//! weapon held and the yes/no facts the preview shows the HUD in. It never edits the HUD.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::{ActiveTheme as _, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    Anchor, AnyElement, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Subscription,
    TestSupportExt as _, Window, div, px,
};
use tf2tools_core::{GameSituation, MeterKind, SituationSwitch, TfClass, WeaponKind};

use super::HudMakerPage;
use super::tip::tipped;
use crate::ids;
use crate::widgets::labelled;

/// A number the situation menu sets with a slider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SituationSlider {
    Health,
    Ammo,
    Charge,
}

impl SituationSlider {
    fn slug(self) -> &'static str {
        match self {
            SituationSlider::Health => "health",
            SituationSlider::Ammo => "ammo",
            SituationSlider::Charge => "charge",
        }
    }
}

const SWITCHES: [(SituationSwitch, &str); 8] = [
    (SituationSwitch::Meter, "Weapon with a meter"),
    (SituationSwitch::Aiming, "Aiming at a player"),
    (SituationSwitch::Effect, "Effect on you"),
    (SituationSwitch::Powerup, "Mannpower powerup"),
    (SituationSwitch::Typing, "Typing in chat"),
    (SituationSwitch::Portrait, "2D class portrait"),
    (SituationSwitch::JustHit, "Just hit someone"),
    (SituationSwitch::Koth, "King of the Hill map"),
];

const MINMODE_TIP: &str = "See the HUD as it looks with TF2's minimal HUD setting on. While this is on, a part with its own minimal-HUD place changes only there.";

const WEAPONS: [(WeaponKind, &str); 2] = [
    (WeaponKind::Clip, "Weapon with a clip"),
    (WeaponKind::NoClip, "Weapon without a clip"),
];

// The meter list's height cap, so 21 weapons never run the menu off a small window.
const METERS_HEIGHT: f32 = 300.;

/// The menu's open flags (the menu and its meter list) and its three sliders, which commit
/// when let go.
pub(super) struct SituationMenu {
    open: bool,
    meters_open: bool,
    sliders: [Entity<SliderState>; 3],
    _subscriptions: Vec<Subscription>,
}

impl SituationMenu {
    pub(super) fn new(window: &mut Window, cx: &mut Context<HudMakerPage>) -> SituationMenu {
        let s = GameSituation::default();
        let sliders = [
            (SituationSlider::Health, health_slider(&s)),
            (SituationSlider::Ammo, percent_slider(s.ammo_percent())),
            (SituationSlider::Charge, percent_slider(s.charge())),
        ]
        .map(|(which, state)| (which, cx.new(|_| state)));
        let subscriptions = (sliders.iter())
            .map(|(which, state)| {
                let which = *which;
                cx.subscribe_in(
                    state,
                    window,
                    move |page, _, ev: &SliderEvent, window, cx| {
                        if let SliderEvent::Release(v) = ev {
                            page.slide_situation(which, v.start(), window, cx);
                        }
                    },
                )
            })
            .collect();
        SituationMenu {
            open: false,
            meters_open: false,
            sliders: sliders.map(|(_, state)| state),
            _subscriptions: subscriptions,
        }
    }

    fn slider(&self, which: SituationSlider) -> &Entity<SliderState> {
        &self.sliders[which as usize]
    }
}

// Health runs from 1 to the class's overheal; the class decides the top, so it is rebuilt.
fn health_slider(s: &GameSituation) -> SliderState {
    SliderState::new()
        .min(1.)
        .max(f32::from(s.overheal_cap()))
        .step(1.)
        .default_value(f32::from(s.health()))
}

fn percent_slider(at: u8) -> SliderState {
    SliderState::new()
        .min(0.)
        .max(100.)
        .step(1.)
        .default_value(f32::from(at))
}

// A slider value as a whole number within `max`; a NaN reads as 0.
fn whole(v: f32, max: u16) -> u16 {
    if v.is_nan() {
        return 0;
    }
    v.round().clamp(0., f32::from(max)) as u16
}

impl HudMakerPage {
    /// The slider for `which` in the situation menu.
    pub fn situation_slider(&self, which: SituationSlider) -> Entity<SliderState> {
        self.situation_menu.slider(which).clone()
    }

    /// Shows the HUD in situation `s`, commits typed values first and stops playback; the
    /// sliders follow. Nothing in the HUD changes.
    pub fn set_situation(&mut self, s: GameSituation, window: &mut Window, cx: &mut Context<Self>) {
        self.flush(cx);
        let Some(editor) = self.editor_mut() else {
            return;
        };
        let class_changed = editor.situation().class() != s.class();
        editor.set_situation(s.clone());
        self.stop_playing();
        let [health, ammo, charge] = self.situation_menu.sliders.clone();
        if class_changed {
            health.update(cx, |st, cx| {
                *st = health_slider(&s);
                cx.notify();
            });
        } else {
            health.update(cx, |st, cx| st.set_value(f32::from(s.health()), window, cx));
        }
        ammo.update(cx, |st, cx| {
            st.set_value(f32::from(s.ammo_percent()), window, cx)
        });
        charge.update(cx, |st, cx| st.set_value(f32::from(s.charge()), window, cx));
        self.refresh(window, cx);
    }

    // A let-go slider sets its number in the situation.
    fn slide_situation(
        &mut self,
        which: SituationSlider,
        at: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(s) = self.editor().map(|e| e.situation().clone()) else {
            return;
        };
        let next = match which {
            SituationSlider::Health => s.with_health(whole(at, u16::MAX)),
            SituationSlider::Ammo => s.with_ammo_percent(whole(at, 100) as u8),
            SituationSlider::Charge => s.with_charge(whole(at, 100) as u8),
        };
        self.set_situation(next, window, cx);
    }

    // One button named for the class and health shown; its menu sets the situation.
    pub(super) fn situation_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let s = self.editor()?.situation().clone();
        let trigger = Button::new(ids::HUD_MAKER_SITUATION)
            .outline()
            .small()
            .icon(IconName::ChevronsUpDown)
            .label(format!("{}, {} health", s.class().name(), s.health()))
            .tooltip("The moment in game the preview shows");
        let pick =
            |id, label: &'static str, on: bool, next: GameSituation, cx: &mut Context<Self>| {
                Button::new(id)
                    .ghost()
                    .xsmall()
                    .label(label)
                    .selected(on)
                    .on_click(cx.listener(move |page, _, window, cx| {
                        page.situation_menu.open = false;
                        page.set_situation(next.clone(), window, cx);
                    }))
            };
        let classes = TfClass::ALL.map(|c| {
            let next = s.clone().with_class(c);
            pick(
                ids::hud_maker_situation_class(c),
                c.name(),
                s.class() == c,
                next,
                cx,
            )
        });
        let weapons = WEAPONS.map(|(w, label)| {
            let next = s.clone().with_weapon(w);
            pick(
                ids::hud_maker_situation_weapon(w),
                label,
                s.weapon() == w,
                next,
                cx,
            )
        });
        let switches = SWITCHES.map(|(sw, label)| {
            let on = s.is_on(sw);
            let next = s.clone().with_switch(sw, !on);
            pick(ids::hud_maker_situation_switch(sw), label, on, next, cx)
        });
        // The minimal HUD is a model setting, not a moment, so it flips the editor's flag.
        let minmode = self.minmode();
        let minmode = Button::new(ids::HUD_MAKER_MINMODE)
            .ghost()
            .xsmall()
            .label("Minimal HUD")
            .selected(minmode)
            .on_click(cx.listener(move |page, _, window, cx| {
                page.situation_menu.open = false;
                page.set_minmode(!minmode, window, cx);
            }));
        let minmode = tipped(&ids::HUD_MAKER_MINMODE.into(), MINMODE_TIP, minmode);
        let muted = cx.theme().muted_foreground;
        let slider = |which: SituationSlider, label: &'static str, value: String| {
            v_flex()
                .gap_1()
                .child(div().text_xs().text_color(muted).child(label))
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(
                            div()
                                .id(ids::hud_maker_situation_slider(which.slug()))
                                .test_support()
                                .flex_1()
                                .child(Slider::new(self.situation_menu.slider(which))),
                        )
                        .child(labelled(
                            div().w_10().text_xs().text_color(muted),
                            ids::hud_maker_situation_value(which.slug()),
                            SharedString::from(value),
                        )),
                )
        };
        let reset = Button::new(ids::HUD_MAKER_SITUATION_RESET)
            .ghost()
            .xsmall()
            .label("Back to the usual moment")
            .on_click(cx.listener(|page, _, window, cx| {
                page.situation_menu.open = false;
                page.situation_menu.meters_open = false;
                page.set_situation(GameSituation::default(), window, cx);
                page.set_meter(MeterKind::Plain, false, window, cx);
            }));
        let meter = self.editor()?.meter();
        let meter_row = Button::new(ids::HUD_MAKER_SITUATION_METER)
            .ghost()
            .xsmall()
            .label(format!("Meter: {}", meter.label()))
            .selected(self.situation_menu.meters_open)
            .on_click(cx.listener(|page, _, _, cx| {
                page.situation_menu.meters_open = !page.situation_menu.meters_open;
                cx.notify();
            }));
        // Picking a meter also turns its switch on, so the part shows.
        let meters = (self.situation_menu.meters_open).then(|| {
            // A lazy iterator, not an array: 21 buttons by value overflow a debug stack.
            let items = MeterKind::ALL.into_iter().map(|kind| {
                Button::new(ids::hud_maker_situation_meter(kind))
                    .ghost()
                    .xsmall()
                    .label(kind.label())
                    .selected(kind == meter)
                    .on_click(cx.listener(move |page, _, window, cx| {
                        page.situation_menu.open = false;
                        page.situation_menu.meters_open = false;
                        page.set_meter(kind, true, window, cx);
                    }))
            });
            v_flex()
                .id(ids::HUD_MAKER_SITUATION_METERS)
                .test_support()
                .max_h(px(METERS_HEIGHT))
                .overflow_y_scroll()
                .gap_1()
                .items_start()
                .children(items)
        });
        // The open meter list takes the menu's whole body, so its 21 rows have room.
        if let Some(meters) = meters {
            let body = v_flex().w_72().gap_2().child(meter_row).child(meters);
            return Some(self.situation_popover(trigger, body, cx));
        }
        let body = v_flex()
            .w_72()
            .gap_2()
            .child(h_flex().flex_wrap().gap_1().children(classes))
            .child(slider(
                SituationSlider::Health,
                "Health",
                s.health().to_string(),
            ))
            .child(slider(
                SituationSlider::Ammo,
                "Ammo left",
                format!("{}%", s.ammo_percent()),
            ))
            .child(slider(
                SituationSlider::Charge,
                "Medic charge",
                format!("{}%", s.charge()),
            ))
            .child(h_flex().flex_wrap().gap_1().children(weapons))
            .child(
                h_flex()
                    .flex_wrap()
                    .gap_1()
                    .children(switches)
                    .child(minmode),
            )
            .child(meter_row)
            .child(reset);
        Some(self.situation_popover(trigger, body, cx))
    }

    // The menu's popover under its trigger, holding `body`.
    fn situation_popover(
        &self,
        trigger: Button,
        body: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let page = cx.entity().downgrade();
        Popover::new("hud-maker-situation")
            .anchor(Anchor::BottomLeft)
            .open(self.situation_menu.open)
            .on_open_change(move |&open, _, cx| {
                let _ = page.update(cx, |page, cx| {
                    page.situation_menu.open = open;
                    // The next open shows the whole menu, not a list left open.
                    if !open {
                        page.situation_menu.meters_open = false;
                    }
                    cx.notify();
                });
            })
            .trigger(trigger)
            .child(body)
            .into_any_element()
    }
}
