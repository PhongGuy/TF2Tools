use crate::support;

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::{AnyWindowHandle, Bounds, Context, Entity, Pixels, TestAppContext, Window, div, px};
use tf2tools_app::ids;
use tf2tools_app::services::{Tf2State, install_tf2_state};
use tf2tools_app::weapon_icons::WeaponIcons;
use tf2tools_app::widgets::weapon_assigner::{
    AssignRequested, AssignTarget, AssignedCell, AssignerState, ClassChanged, ClearRequested,
    WeaponAssigner, slot_colour,
};
use tf2tools_core::{Tf2Install, TfClass, Weapon, WeaponSlot, weapons};

struct Host {
    assigner: Entity<WeaponAssigner>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.assigner.clone())
    }
}

type Events = Rc<RefCell<Vec<AssignRequested>>>;

fn open(
    cx: &mut TestAppContext,
    env: &support::Env,
    compact: bool,
) -> (AnyWindowHandle, Entity<WeaponAssigner>, Events) {
    support::init(cx);
    support::install_services(cx, support::services(env, support::no_steam()));
    let (win, host) = cx
        .update(|cx| {
            gpui_kit::open_window(support::window_options(), cx, |_, cx| {
                let assigner = cx.new(|cx| WeaponAssigner::new(compact, cx));
                cx.new(|_| Host { assigner })
            })
        })
        .unwrap();
    let assigner = cx.update(|cx| host.read(cx).assigner.clone());
    let events: Events = Rc::default();
    let sink = Rc::clone(&events);
    cx.update(|cx| {
        cx.subscribe(&assigner, move |_, e: &AssignRequested, _| {
            sink.borrow_mut().push(e.clone())
        })
        .detach()
    });
    support::settle(cx, win);
    (win, assigner, events)
}

fn scripts(class: TfClass, slot: Option<WeaponSlot>) -> Vec<&'static str> {
    weapons()
        .iter()
        .filter(|w| w.class() == class && slot.is_none_or(|s| w.slot() == s))
        .map(Weapon::script_name)
        .collect()
}

fn state(cx: &mut TestAppContext, a: &Entity<WeaponAssigner>, state: AssignerState) {
    cx.update(|cx| a.update(cx, |a, cx| a.set_state(state, cx)));
}

#[gpui_kit::test]
fn it_lists_the_scout_weapons_with_their_cells(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, assigner, _) = open(cx, &env, false);
    cx.update(|cx| {
        assigner.update(cx, |a, cx| {
            a.set_assigned(
                vec![AssignedCell {
                    script: "tf_weapon_scattergun".into(),
                    label: "Dot".into(),
                    crosshair: None,
                }],
                cx,
            )
        })
    });
    for script in scripts(TfClass::Scout, None) {
        assert!(
            support::present(cx, win, ids::weapon_row(script)),
            "{script}"
        );
    }
    assert_eq!(
        support::label_of(cx, win, ids::weapon_assigned("tf_weapon_scattergun")).as_deref(),
        Some("Dot")
    );
    assert!(support::present(
        cx,
        win,
        ids::weapon_check("tf_weapon_scattergun")
    ));
    assert_eq!(
        support::label_of(cx, win, ids::weapon_assigned("tf_weapon_pistol_scout")).as_deref(),
        Some("None")
    );
}

#[gpui_kit::test]
fn a_row_emits_only_with_a_selection_and_not_busy(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, assigner, events) = open(cx, &env, false);
    support::click_id(cx, win, ids::weapon_row("tf_weapon_scattergun"));
    assert!(events.borrow().is_empty());
    state(
        cx,
        &assigner,
        AssignerState {
            has_selection: true,
            busy: true,
            running: None,
        },
    );
    support::click_id(cx, win, ids::weapon_row("tf_weapon_scattergun"));
    assert!(events.borrow().is_empty());
    state(
        cx,
        &assigner,
        AssignerState {
            has_selection: true,
            ..AssignerState::default()
        },
    );
    support::click_id(cx, win, ids::weapon_row("tf_weapon_scattergun"));
    assert_eq!(
        *events.borrow(),
        [AssignRequested {
            target: AssignTarget::Weapon("tf_weapon_scattergun"),
            class: TfClass::Scout,
            scripts: vec!["tf_weapon_scattergun"],
        }]
    );
}

#[gpui_kit::test]
fn bulk_buttons_emit_the_class_slot_scripts(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, assigner, events) = open(cx, &env, false);
    support::click_id(cx, win, ids::class_tab("soldier"));
    assert_eq!(cx.update(|cx| assigner.read(cx).class()), TfClass::Soldier);
    // On the page a bulk click without a selection still emits; the host toasts.
    support::click_id(cx, win, ids::CROSSHAIRS_BULK_PRIMARY);
    support::click_id(cx, win, ids::CROSSHAIRS_BULK_ALL);
    let got = events.borrow().clone();
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].target, AssignTarget::Bulk(Some(WeaponSlot::Primary)));
    assert_eq!(
        got[0].scripts,
        scripts(TfClass::Soldier, Some(WeaponSlot::Primary))
    );
    assert_eq!(got[1].target, AssignTarget::Bulk(None));
    assert_eq!(got[1].class, TfClass::Soldier);
    assert_eq!(got[1].scripts, scripts(TfClass::Soldier, None));
}

#[gpui_kit::test]
fn compact_bulk_waits_for_a_selection(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, assigner, events) = open(cx, &env, true);
    support::click_id(cx, win, ids::CROSSHAIRS_BULK_ALL);
    assert!(events.borrow().is_empty());
    state(
        cx,
        &assigner,
        AssignerState {
            has_selection: true,
            ..AssignerState::default()
        },
    );
    support::click_id(cx, win, ids::CROSSHAIRS_BULK_MELEE);
    assert_eq!(
        events.borrow()[0].scripts,
        scripts(TfClass::Scout, Some(WeaponSlot::Melee))
    );
}

#[gpui_kit::test]
fn a_busy_host_disables_the_other_bulk_buttons(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, assigner, events) = open(cx, &env, false);
    let running = Some(AssignTarget::Bulk(Some(WeaponSlot::Primary)));
    state(
        cx,
        &assigner,
        AssignerState {
            has_selection: true,
            busy: true,
            running,
        },
    );
    support::click_id(cx, win, ids::CROSSHAIRS_BULK_SECONDARY);
    assert!(events.borrow().is_empty());
}

#[gpui_kit::test]
fn only_a_real_class_change_emits_class_changed(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, assigner, _) = open(cx, &env, false);
    let changes: Rc<RefCell<Vec<TfClass>>> = Rc::default();
    let sink = Rc::clone(&changes);
    cx.update(|cx| {
        cx.subscribe(&assigner, move |_, e: &ClassChanged, _| {
            sink.borrow_mut().push(e.class)
        })
        .detach()
    });
    support::click_id(cx, win, ids::class_tab("soldier"));
    support::click_id(cx, win, ids::class_tab("soldier"));
    state(cx, &assigner, AssignerState::default());
    cx.update(|cx| assigner.update(cx, |a, cx| a.set_assigned(Vec::new(), cx)));
    cx.update(|cx| assigner.update(cx, |a, cx| a.set_class(TfClass::Medic, cx)));
    cx.update(|cx| assigner.update(cx, |a, cx| a.set_class(TfClass::Medic, cx)));
    assert_eq!(*changes.borrow(), [TfClass::Soldier, TfClass::Medic]);
}

#[gpui_kit::test]
fn the_assigning_row_spins_in_place_of_its_check(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, assigner, _) = open(cx, &env, false);
    let (scattergun, pistol) = ("tf_weapon_scattergun", "tf_weapon_pistol_scout");
    cx.update(|cx| {
        assigner.update(cx, |a, cx| {
            a.set_assigned(
                vec![AssignedCell {
                    script: scattergun.into(),
                    label: "Dot".into(),
                    crosshair: None,
                }],
                cx,
            )
        })
    });
    state(
        cx,
        &assigner,
        AssignerState {
            has_selection: true,
            busy: true,
            running: Some(AssignTarget::Weapon(scattergun)),
        },
    );
    let row = support::bounds_of(cx, win, ids::weapon_row(scattergun)).unwrap();
    let spinner = support::bounds_of(cx, win, ids::weapon_spinner(scattergun))
        .expect("the assigning row spins");
    assert!(row.contains(&spinner.center()), "{spinner:?} in {row:?}");
    assert_eq!(
        (spinner.size.width, spinner.size.height),
        (px(14.), px(14.))
    );
    assert!(!support::present(cx, win, ids::weapon_check(scattergun)));
    assert!(!support::present(cx, win, ids::weapon_spinner(pistol)));
    state(
        cx,
        &assigner,
        AssignerState {
            has_selection: true,
            ..AssignerState::default()
        },
    );
    assert!(!support::present(cx, win, ids::weapon_spinner(scattergun)));
    assert!(support::present(cx, win, ids::weapon_check(scattergun)));
}

/// A window hosting a new assigner over the services and icons already installed.
fn assigner_window(
    cx: &mut TestAppContext,
    compact: bool,
) -> (AnyWindowHandle, Entity<WeaponAssigner>) {
    let (win, host) = cx
        .update(|cx| {
            gpui_kit::open_window(support::window_options(), cx, |_, cx| {
                let assigner = cx.new(|cx| WeaponAssigner::new(compact, cx));
                cx.new(|_| Host { assigner })
            })
        })
        .unwrap();
    let assigner = cx.update(|cx| host.read(cx).assigner.clone());
    support::settle(cx, win);
    (win, assigner)
}

fn found(env: &support::Env) -> Tf2State {
    Tf2State::Found(Tf2Install::from_path(&env.game).unwrap())
}

/// Services, TF2 in `state`, and the game icons on a fake loader that skips `skip`.
fn install_icons(
    cx: &mut TestAppContext,
    env: &support::Env,
    state: Tf2State,
    skip: &'static [&'static str],
) -> (Entity<Tf2State>, Entity<WeaponIcons>, support::IconLoads) {
    support::init(cx);
    support::install_services(cx, support::services(env, support::no_steam()));
    let tf2 = cx.update(|cx| install_tf2_state(cx, state));
    let (loader, loads) = support::fake_icon_loader(support::icon_png(env), skip);
    let icons = cx.update(|cx| WeaponIcons::install_with(cx, loader));
    (tf2, icons, loads)
}

fn loaded(cx: &mut TestAppContext, icons: &Entity<WeaponIcons>) -> bool {
    cx.update(|cx| icons.read(cx).icon("tf_weapon_scattergun").is_some())
}

fn assert_framed(cx: &mut TestAppContext, win: AnyWindowHandle, script: &str) {
    let frame = support::bounds_of(cx, win, ids::weapon_icon(script))
        .unwrap_or_else(|| panic!("{script}: the framed icon"));
    assert_eq!(size_of(frame), (px(36.), px(28.)), "{script}");
    assert!(
        !support::present(cx, win, ids::weapon_slot_icon(script)),
        "{script}"
    );
    let row = support::bounds_of(cx, win, ids::weapon_row(script)).unwrap();
    assert!(row.contains(&frame.center()), "{script}");
}

fn assert_square(cx: &mut TestAppContext, win: AnyWindowHandle, script: &str) {
    let square = support::bounds_of(cx, win, ids::weapon_slot_icon(script))
        .unwrap_or_else(|| panic!("{script}: the slot square"));
    assert_eq!(size_of(square), (px(16.), px(16.)), "{script}");
    assert!(
        !support::present(cx, win, ids::weapon_icon(script)),
        "{script}"
    );
}

#[gpui_kit::test]
fn a_game_icon_is_framed_in_place_of_the_slot_square(cx: &mut TestAppContext) {
    let env = support::env();
    let (_tf2, icons, _) = install_icons(cx, &env, found(&env), &[]);
    let (win, _assigner) = assigner_window(cx, false);
    support::wait_real(cx, "the game icons", |cx| loaded(cx, &icons));
    support::settle(cx, win);
    for script in scout_scripts() {
        assert_framed(cx, win, script);
    }
}

#[gpui_kit::test]
fn a_weapon_the_game_lacks_keeps_its_slot_square(cx: &mut TestAppContext) {
    let env = support::env();
    let (_tf2, icons, _) = install_icons(cx, &env, found(&env), &["tf_weapon_bat"]);
    let (win, _assigner) = assigner_window(cx, false);
    support::wait_real(cx, "the game icons", |cx| loaded(cx, &icons));
    support::settle(cx, win);
    assert_square(cx, win, "tf_weapon_bat");
    assert_framed(cx, win, "tf_weapon_scattergun");
}

#[gpui_kit::test]
fn before_tf2_is_found_rows_show_the_slot_square(cx: &mut TestAppContext) {
    let env = support::env();
    let (_tf2, _icons, loads) = install_icons(cx, &env, Tf2State::Missing { saved: None }, &[]);
    let (win, _assigner) = assigner_window(cx, false);
    for script in scout_scripts() {
        assert_square(cx, win, script);
    }
    assert!(loads.lock().unwrap().is_empty());
}

#[gpui_kit::test]
fn without_a_tf2_state_rows_show_the_slot_square(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, _assigner, _) = open(cx, &env, false);
    for script in scout_scripts() {
        assert_square(cx, win, script);
    }
}

#[gpui_kit::test]
fn two_lists_share_one_load(cx: &mut TestAppContext) {
    let env = support::env();
    let (_tf2, icons, loads) = install_icons(cx, &env, found(&env), &[]);
    let (first, _a) = assigner_window(cx, false);
    let (second, _b) = assigner_window(cx, false);
    support::wait_real(cx, "the game icons", |cx| loaded(cx, &icons));
    support::settle(cx, first);
    support::settle(cx, second);
    assert_framed(cx, first, "tf_weapon_scattergun");
    assert_framed(cx, second, "tf_weapon_scattergun");
    assert_eq!(loads.lock().unwrap().len(), 1);
}

fn scout_scripts() -> Vec<&'static str> {
    scripts(TfClass::Scout, None)
}

fn size_of(b: Bounds<Pixels>) -> (Pixels, Pixels) {
    (b.size.width, b.size.height)
}

#[gpui_kit::test]
fn compact_rows_show_neither_icon(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, _assigner, _) = open(cx, &env, true);
    let script = "tf_weapon_scattergun";
    assert!(support::present(cx, win, ids::weapon_row(script)));
    assert!(!support::present(cx, win, ids::weapon_slot_icon(script)));
    assert!(!support::present(cx, win, ids::weapon_icon(script)));
}

// A compact list over a found TF2 starts no icon load. Had the compact list made the icons
// entity while TF2 was found, the entity's first look at TF2 would have started a load, so
// `loads` would be 1; made here after TF2 goes missing, it starts none.
#[gpui_kit::test]
fn a_compact_list_never_loads_icons(cx: &mut TestAppContext) {
    let env = support::env();
    support::init(cx);
    support::install_services(cx, support::services(&env, support::no_steam()));
    let tf2 = cx.update(|cx| install_tf2_state(cx, found(&env)));
    let (_win, _assigner) = assigner_window(cx, true);
    cx.update(|cx| {
        tf2.update(cx, |t, cx| {
            *t = Tf2State::Missing { saved: None };
            cx.notify();
        })
    });
    let loads = cx.update(|cx| WeaponIcons::global(cx).read(cx).loads());
    assert_eq!(loads, 0);
}

// Icons that land after the list opens replace the squares, and a row is as tall with its
// framed icon as with its slot square, so nothing jumps.
#[gpui_kit::test]
fn icons_landing_later_replace_the_squares_without_a_jump(cx: &mut TestAppContext) {
    let env = support::env();
    let script = "tf_weapon_scattergun";
    let (tf2, icons, _) = install_icons(cx, &env, Tf2State::Detecting, &[]);
    let (win, _assigner) = assigner_window(cx, false);
    assert_square(cx, win, script);
    let before = support::bounds_of(cx, win, ids::weapon_row(script)).unwrap();

    let state = found(&env);
    cx.update(|cx| {
        tf2.update(cx, |t, cx| {
            *t = state;
            cx.notify();
        })
    });
    support::wait_real(cx, "the game icons", |cx| loaded(cx, &icons));
    support::settle(cx, win);
    assert_framed(cx, win, script);
    let after = support::bounds_of(cx, win, ids::weapon_row(script)).unwrap();
    assert_eq!(after.size.height, before.size.height);
}

// The frame takes the slot dot's colour: primary in the theme's primary, secondary blue,
// melee and the rest orange.
#[test]
fn each_slot_frames_its_icon_in_the_slot_colour() {
    let primary: gpui_kit::Hsla = gpui_kit::rgb(0x123456).into();
    assert_eq!(slot_colour(WeaponSlot::Primary, primary), primary);
    assert_eq!(
        slot_colour(WeaponSlot::Secondary, primary),
        gpui_kit::rgb(0x3b82f6).into()
    );
    assert_eq!(
        slot_colour(WeaponSlot::Melee, primary),
        gpui_kit::rgb(0xf97316).into()
    );
}

#[gpui_kit::test]
fn the_compact_list_shows_a_bar_clear_of_its_rows(cx: &mut TestAppContext) {
    use support::scroll::{assert_bar_on_right_edge, assert_clear_of_bar, assert_track_scrolls};
    let env = support::env();
    let (win, _assigner, _) = open(cx, &env, true);
    let rows = scripts(TfClass::Scout, None);
    assert_bar_on_right_edge(cx, win, ids::WEAPON_LIST, ids::WEAPON_LIST_SCROLLBAR);
    assert_clear_of_bar(
        cx,
        win,
        ids::weapon_row(rows[0]),
        ids::WEAPON_LIST_SCROLLBAR,
    );
    assert_track_scrolls(cx, win, ids::WEAPON_LIST, ids::weapon_row(rows[0]));
}

type Clears = Rc<RefCell<Vec<ClearRequested>>>;

fn clears(cx: &mut TestAppContext, a: &Entity<WeaponAssigner>) -> Clears {
    let clears: Clears = Rc::default();
    let sink = Rc::clone(&clears);
    cx.update(|cx| {
        cx.subscribe(a, move |_, e: &ClearRequested, _| {
            sink.borrow_mut().push(e.clone())
        })
        .detach()
    });
    clears
}

/// The scattergun assigned "Dot", and the clear buttons switched on when `clearable`.
fn scattergun_cell(cx: &mut TestAppContext, a: &Entity<WeaponAssigner>, clearable: bool) {
    cx.update(|cx| {
        a.update(cx, |a, cx| {
            a.set_assigned(
                vec![AssignedCell {
                    script: "tf_weapon_scattergun".into(),
                    label: "Dot".into(),
                    crosshair: None,
                }],
                cx,
            );
            a.set_clearable(clearable, cx);
        })
    });
}

#[gpui_kit::test]
fn the_clear_button_shows_only_when_clearable_and_assigned(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, assigner, _) = open(cx, &env, false);
    scattergun_cell(cx, &assigner, false);
    assert!(!support::present(
        cx,
        win,
        ids::weapon_clear("tf_weapon_scattergun")
    ));
    scattergun_cell(cx, &assigner, true);
    assert!(support::present(
        cx,
        win,
        ids::weapon_clear("tf_weapon_scattergun")
    ));
    assert!(!support::present(
        cx,
        win,
        ids::weapon_clear("tf_weapon_pistol_scout")
    ));
}

// The row is dimmed and inert without a selection, yet its clear button still asks; with a
// selection the click stops at the button, so the row behind it never assigns.
#[gpui_kit::test]
fn a_clear_click_asks_with_or_without_a_selection_and_never_assigns(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, assigner, assigns) = open(cx, &env, false);
    let cleared = clears(cx, &assigner);
    scattergun_cell(cx, &assigner, true);
    support::click_id(cx, win, ids::weapon_clear("tf_weapon_scattergun"));
    assert_eq!(
        *cleared.borrow(),
        [ClearRequested {
            script: "tf_weapon_scattergun"
        }]
    );
    state(
        cx,
        &assigner,
        AssignerState {
            has_selection: true,
            ..AssignerState::default()
        },
    );
    support::click_id(cx, win, ids::weapon_clear("tf_weapon_scattergun"));
    assert_eq!(cleared.borrow().len(), 2);
    assert!(
        assigns.borrow().is_empty(),
        "the row behind the button assigned"
    );
}

#[gpui_kit::test]
fn a_busy_host_keeps_the_clear_button_inert(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, assigner, _) = open(cx, &env, false);
    let cleared = clears(cx, &assigner);
    scattergun_cell(cx, &assigner, true);
    state(
        cx,
        &assigner,
        AssignerState {
            busy: true,
            ..AssignerState::default()
        },
    );
    assert!(support::disabled_of(
        cx,
        win,
        ids::weapon_clear("tf_weapon_scattergun")
    ));
    support::click_id(cx, win, ids::weapon_clear("tf_weapon_scattergun"));
    assert!(cleared.borrow().is_empty());
}

// A clearable list keeps an empty box where an unassigned row's clear button would sit, so the
// assigned labels line up; a list without clear buttons keeps no such box.
#[gpui_kit::test]
fn a_clearable_list_keeps_its_trailing_column_aligned(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, assigner, _) = open(cx, &env, false);
    let right = |cx: &mut TestAppContext, script: &str| {
        support::bounds_of(cx, win, ids::weapon_assigned(script))
            .unwrap()
            .right()
    };
    scattergun_cell(cx, &assigner, false);
    let plain = right(cx, "tf_weapon_scattergun");
    assert_eq!(right(cx, "tf_weapon_pistol_scout"), plain);
    scattergun_cell(cx, &assigner, true);
    let clearable = right(cx, "tf_weapon_scattergun");
    assert!(clearable < plain);
    assert_eq!(right(cx, "tf_weapon_pistol_scout"), clearable);
}
