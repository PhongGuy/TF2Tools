//! The shared loadout board over a Loadout the library does not hold: clicks become events for
//! the host, ids carry the host's scope, and nothing reaches the library or the game.

use crate::support;

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::{AnyWindowHandle, Context, Entity, TestAppContext, Window, div};
use tf2tools_app::ids::{self, IdScope};
use tf2tools_app::loadout::{BoardTarget, Loadout};
use tf2tools_app::previews::CrosshairPreviews;
use tf2tools_app::services::Services;
use tf2tools_app::widgets::loadout_board::{BoardBusy, BoardEvent, LoadoutBoard, OnBoard};
use tf2tools_core::{CrosshairRef, TfClass, WeaponSlot};

const SCOPE: IdScope = IdScope::PROFILE_CROSSHAIRS;

type Events = Rc<RefCell<Vec<BoardEvent>>>;

struct Host {
    loadout: Loadout,
    focus: Option<(TfClass, WeaponSlot)>,
    busy: BoardBusy,
    previews: Entity<CrosshairPreviews>,
    events: Events,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let events = Rc::clone(&self.events);
        let on: OnBoard = Rc::new(move |event, _, _| events.borrow_mut().push(event));
        let name_of = |_: &CrosshairRef| "Dot".to_owned();
        let board = LoadoutBoard {
            loadout: &self.loadout,
            scope: SCOPE,
            turned: false,
            short_names: false,
            selected: None,
            focus: self.focus,
            busy: self.busy,
            previews: &self.previews,
            name_of: &name_of,
        };
        let details = board.details(div().into_any_element(), Rc::clone(&on), cx);
        div()
            .size_full()
            .child(board.board(on, cx))
            .children(details)
    }
}

fn open(
    cx: &mut TestAppContext,
    env: &support::Env,
    loadout: Loadout,
) -> (AnyWindowHandle, Entity<Host>, Events, Services) {
    support::init(cx);
    let services = support::services(env, support::no_steam());
    support::install_services(cx, services.clone());
    let events: Events = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&events);
    let (win, host) = cx
        .update(|cx| {
            gpui_kit::open_window(support::window_options(), cx, |_, cx| {
                let previews = CrosshairPreviews::global(cx);
                cx.new(|_| Host {
                    loadout,
                    focus: None,
                    busy: BoardBusy::Idle,
                    previews,
                    events: sink,
                })
            })
        })
        .unwrap();
    support::settle(cx, win);
    (win, host, events, services)
}

fn custom_entries(env: &support::Env) -> usize {
    std::fs::read_dir(env.game.join("tf/custom"))
        .unwrap()
        .count()
}

fn set(cx: &mut TestAppContext, host: &Entity<Host>, edit: impl FnOnce(&mut Host)) {
    cx.update(|cx| {
        host.update(cx, |h, cx| {
            edit(h);
            cx.notify();
        })
    });
}

#[gpui_kit::test]
fn cells_show_the_loadout_they_are_given(cx: &mut TestAppContext) {
    let env = support::env();
    let mut loadout = Loadout::default();
    let dot = CrosshairRef::Builtin("dot".to_owned());
    loadout.assign(BoardTarget::Cell(TfClass::Heavy, WeaponSlot::Primary), &dot);
    let (win, _, _, _) = open(cx, &env, loadout);
    let heavy = SCOPE.id(ids::loadout_cell(TfClass::Heavy, WeaponSlot::Primary));
    let scout = SCOPE.id(ids::loadout_cell(TfClass::Scout, WeaponSlot::Melee));
    assert_eq!(support::label_of(cx, win, heavy).as_deref(), Some("Dot"));
    assert_eq!(
        support::label_of(cx, win, scout).as_deref(),
        Some("Default")
    );
}

#[gpui_kit::test]
fn clicks_become_events_and_nothing_reaches_the_game(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, _, events, services) = open(cx, &env, Loadout::default());
    let before = custom_entries(&env);
    support::click_id(
        cx,
        win,
        SCOPE.id(ids::loadout_cell(TfClass::Heavy, WeaponSlot::Primary)),
    );
    support::click_id(cx, win, SCOPE.id(ids::loadout_class(TfClass::Sniper)));
    support::click_id(cx, win, SCOPE.id(ids::loadout_slot(WeaponSlot::Melee)));
    support::click_id(cx, win, SCOPE.id(ids::CROSSHAIRS_BOARD_EVERYTHING));
    assert_eq!(
        *events.borrow(),
        vec![
            BoardEvent::Assign(BoardTarget::Cell(TfClass::Heavy, WeaponSlot::Primary)),
            BoardEvent::Assign(BoardTarget::Class(TfClass::Sniper)),
            BoardEvent::Assign(BoardTarget::Slot(WeaponSlot::Melee)),
            BoardEvent::Assign(BoardTarget::Everything),
        ]
    );
    assert!(services.library.assignments().unwrap().is_empty());
    assert_eq!(custom_entries(&env), before);
}

#[gpui_kit::test]
fn the_cell_menu_opens_without_assigning(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, _, events, _) = open(cx, &env, Loadout::default());
    let menu = SCOPE.id(ids::loadout_cell_menu(TfClass::Pyro, WeaponSlot::Secondary));
    support::click_id(cx, win, menu);
    assert_eq!(
        *events.borrow(),
        vec![BoardEvent::Open(TfClass::Pyro, WeaponSlot::Secondary)]
    );
}

#[gpui_kit::test]
fn details_show_for_the_focus_and_report_reset_and_close(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, host, events, _) = open(cx, &env, Loadout::default());
    assert!(!support::present(
        cx,
        win,
        SCOPE.id(ids::CROSSHAIRS_DETAILS)
    ));
    set(cx, &host, |h| {
        h.focus = Some((TfClass::Medic, WeaponSlot::Secondary))
    });
    assert!(support::present(cx, win, SCOPE.id(ids::CROSSHAIRS_DETAILS)));
    support::click_id(cx, win, SCOPE.id(ids::CROSSHAIRS_DETAILS_RESET));
    support::click_id(cx, win, SCOPE.id(ids::CROSSHAIRS_DETAILS_CLOSE));
    assert_eq!(
        *events.borrow(),
        vec![
            BoardEvent::Reset(BoardTarget::Cell(TfClass::Medic, WeaponSlot::Secondary)),
            BoardEvent::Close,
        ]
    );
}

#[gpui_kit::test]
fn a_busy_board_blocks_reset(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, host, events, _) = open(cx, &env, Loadout::default());
    set(cx, &host, |h| {
        h.focus = Some((TfClass::Spy, WeaponSlot::Melee));
        h.busy = BoardBusy::Other;
    });
    support::click_id(cx, win, SCOPE.id(ids::CROSSHAIRS_DETAILS_RESET));
    assert!(events.borrow().is_empty());
}

#[gpui_kit::test]
fn a_scoped_board_draws_none_of_the_page_ids(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, _, _, _) = open(cx, &env, Loadout::default());
    assert!(support::present(cx, win, SCOPE.id(ids::CROSSHAIRS_BOARD)));
    assert!(!support::present(cx, win, ids::CROSSHAIRS_BOARD));
    let cell = ids::loadout_cell(TfClass::Heavy, WeaponSlot::Primary);
    assert!(!support::present(cx, win, cell));
    assert!(!support::present(
        cx,
        win,
        ids::loadout_class(TfClass::Heavy)
    ));
}
