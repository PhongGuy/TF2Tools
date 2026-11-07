use crate::support;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use gpui_kit::prelude::*;
use gpui_kit::{AnyWindowHandle, Bounds, Context, Entity, Pixels, TestAppContext, Window, div, px};
use tf2tools_app::ids;
use tf2tools_app::remote_images::decode::decode_image;
use tf2tools_app::remote_images::queue::IMAGE_BUDGET;
use tf2tools_app::remote_images::{FetchFailure, ImageFetch, ImageKey, RemoteImage, RemoteImages};
use tf2tools_app::services::Services;
use tf2tools_app::widgets::weapon_assigner::{
    AssignRequested, AssignTarget, AssignedCell, AssignerState, ClassChanged, WeaponAssigner,
    slot_colour,
};
use tf2tools_core::{TfClass, Weapon, WeaponSlot, weapons};

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
    let sink = events.clone();
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
    let sink = changes.clone();
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

type Asked = Arc<Mutex<Vec<String>>>;

/// A fetcher that serves a 100x75 clear PNG for every URL (or fails each one), logging URLs.
fn icon_fetch(serve: bool) -> (ImageFetch, Asked) {
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        100,
        75,
        image::Rgba([0, 0, 0, 0]),
    ))
    .write_to(&mut out, image::ImageFormat::Png)
    .unwrap();
    let png = Arc::new(out.into_inner());
    let asked: Asked = Arc::default();
    let log = asked.clone();
    let fetch: ImageFetch = Arc::new(move |url: String| {
        log.lock().unwrap().push(url);
        let result = if serve {
            Ok(png.to_vec())
        } else {
            Err(FetchFailure::Permanent)
        };
        Box::pin(async move { result })
    });
    (fetch, asked)
}

/// A window hosting a new assigner over the services and image cache already installed.
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

/// Services plus an image cache on `fetch`; fetches cross tokio, so the executor may park.
fn install_icons(
    cx: &mut TestAppContext,
    env: &support::Env,
    fetch: ImageFetch,
) -> Entity<RemoteImages> {
    support::init(cx);
    cx.executor().allow_parking();
    support::install_services(cx, support::services(env, support::no_steam()));
    cx.update(|cx| RemoteImages::install_with(cx, fetch, IMAGE_BUDGET, decode_image))
}

fn icon_urls(cx: &mut TestAppContext, class: TfClass) -> Vec<String> {
    cx.update(|cx| {
        let store = &cx.global::<Services>().store;
        let mut urls: Vec<String> = weapons()
            .iter()
            .filter(|w| w.class() == class)
            .map(|w| store.weapon_icon_url(w).expect("every weapon is mapped"))
            .collect();
        urls.sort();
        urls
    })
}

fn asked_sorted(asked: &Asked) -> Vec<String> {
    let mut urls = asked.lock().unwrap().clone();
    urls.sort();
    urls
}

fn all_ready(cx: &mut TestAppContext, images: &Entity<RemoteImages>, urls: &[String]) -> bool {
    cx.update(|cx| {
        urls.iter().all(|u| {
            matches!(
                images.read(cx).image(&ImageKey::weapon_icon(u.clone())),
                RemoteImage::Ready(_)
            )
        })
    })
}

#[gpui_kit::test]
fn the_list_fetches_each_of_its_class_icons_once(cx: &mut TestAppContext) {
    let env = support::env();
    let (fetch, asked) = icon_fetch(true);
    let images = install_icons(cx, &env, fetch);
    let (_win, _assigner) = assigner_window(cx, false);
    let scout = icon_urls(cx, TfClass::Scout);
    support::wait_real(cx, "the scout icons", |cx| all_ready(cx, &images, &scout));
    assert_eq!(asked_sorted(&asked), scout);
    assert_eq!(
        cx.update(|cx| images.read(cx).stats().foreground_decodes),
        0
    );
}

// Switching classes asks once per switch, never from render, and a class seen before is
// drawn from the session cache without fetching again.
#[gpui_kit::test]
fn switching_classes_wants_once_and_refetches_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let (fetch, asked) = icon_fetch(true);
    let images = install_icons(cx, &env, fetch);
    let (win, assigner) = assigner_window(cx, false);
    let (scout, soldier) = (
        icon_urls(cx, TfClass::Scout),
        icon_urls(cx, TfClass::Soldier),
    );
    support::wait_real(cx, "the scout icons", |cx| all_ready(cx, &images, &scout));
    let wants = cx.update(|cx| images.read(cx).stats().wants);

    cx.update(|cx| assigner.update(cx, |a, cx| a.set_class(TfClass::Soldier, cx)));
    support::wait_real(cx, "the soldier icons", |cx| {
        all_ready(cx, &images, &soldier)
    });
    cx.update(|cx| assigner.update(cx, |a, cx| a.set_class(TfClass::Scout, cx)));
    cx.update(|cx| assigner.update(cx, |a, cx| a.set_assigned(Vec::new(), cx)));
    support::settle(cx, win);

    assert_eq!(cx.update(|cx| images.read(cx).stats().wants), wants + 2);
    let mut both = [scout.clone(), soldier].concat();
    both.sort();
    assert_eq!(asked_sorted(&asked), both);
    assert!(all_ready(cx, &images, &scout));
    assert_eq!(
        cx.update(|cx| images.read(cx).stats().foreground_decodes),
        0
    );
}

// Dropping the list hands its wants back with one empty want, so the cache stops holding
// its icons as wanted; the drop runs inside an update so the release fires before the read.
#[gpui_kit::test]
fn dropping_the_list_releases_its_icon_wants(cx: &mut TestAppContext) {
    let env = support::env();
    let (fetch, _asked) = icon_fetch(true);
    let images = install_icons(cx, &env, fetch);
    let assigner = cx.update(|cx| cx.new(|cx| WeaponAssigner::new(false, cx)));
    cx.run_until_parked();
    let wants = cx.update(|cx| images.read(cx).stats().wants);
    assert!(wants > 0, "the list wants its class icons");

    cx.update(|_| drop(assigner));
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| images.read(cx).stats().wants), wants + 1);
}

#[gpui_kit::test]
fn compact_rows_want_no_icons(cx: &mut TestAppContext) {
    let env = support::env();
    let (fetch, asked) = icon_fetch(true);
    let images = install_icons(cx, &env, fetch);
    let (win, assigner) = assigner_window(cx, true);
    cx.update(|cx| assigner.update(cx, |a, cx| a.set_class(TfClass::Spy, cx)));
    support::settle(cx, win);
    assert!(asked.lock().unwrap().is_empty());
    assert_eq!(cx.update(|cx| images.read(cx).stats().wants), 0);
}

fn scout_scripts() -> Vec<&'static str> {
    scripts(TfClass::Scout, None)
}

fn size_of(b: Bounds<Pixels>) -> (Pixels, Pixels) {
    (b.size.width, b.size.height)
}

#[gpui_kit::test]
fn a_loaded_icon_is_framed_in_place_of_the_slot_square(cx: &mut TestAppContext) {
    let env = support::env();
    let (fetch, _asked) = icon_fetch(true);
    let images = install_icons(cx, &env, fetch);
    let (win, _assigner) = assigner_window(cx, false);
    let scout = icon_urls(cx, TfClass::Scout);
    support::wait_real(cx, "the scout icons", |cx| all_ready(cx, &images, &scout));
    for script in scout_scripts() {
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
}

#[gpui_kit::test]
fn a_failed_icon_falls_back_to_the_slot_square(cx: &mut TestAppContext) {
    let env = support::env();
    let (fetch, asked) = icon_fetch(false);
    let images = install_icons(cx, &env, fetch);
    let (win, _assigner) = assigner_window(cx, false);
    let scout = icon_urls(cx, TfClass::Scout);
    support::wait_real(cx, "every scout icon to fail", |cx| {
        asked_sorted(&asked) == scout
            && cx.update(|cx| {
                scout.iter().all(|u| {
                    matches!(
                        images.read(cx).image(&ImageKey::weapon_icon(u.clone())),
                        RemoteImage::Failed
                    )
                })
            })
    });
    for script in scout_scripts() {
        let square = support::bounds_of(cx, win, ids::weapon_slot_icon(script))
            .unwrap_or_else(|| panic!("{script}: the slot square"));
        assert_eq!(size_of(square), (px(16.), px(16.)), "{script}");
        assert!(
            !support::present(cx, win, ids::weapon_icon(script)),
            "{script}"
        );
    }
}

#[gpui_kit::test]
fn without_an_image_cache_rows_show_the_slot_square(cx: &mut TestAppContext) {
    let env = support::env();
    let (win, _assigner, _) = open(cx, &env, false);
    for script in scout_scripts() {
        assert!(
            support::present(cx, win, ids::weapon_slot_icon(script)),
            "{script}"
        );
        assert!(
            !support::present(cx, win, ids::weapon_icon(script)),
            "{script}"
        );
    }
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

// A row is as tall with its slot square as with its framed icon, so nothing jumps as icons land.
#[gpui_kit::test]
fn rows_keep_their_height_when_icons_land(cx: &mut TestAppContext) {
    let env = support::env();
    let script = "tf_weapon_scattergun";
    let (fallback_win, _a, _) = open(cx, &env, false);
    let before = support::bounds_of(cx, fallback_win, ids::weapon_row(script)).unwrap();
    let (fetch, _asked) = icon_fetch(true);
    cx.executor().allow_parking();
    let images = cx.update(|cx| RemoteImages::install_with(cx, fetch, IMAGE_BUDGET, decode_image));
    let (win, _assigner) = assigner_window(cx, false);
    let scout = icon_urls(cx, TfClass::Scout);
    support::wait_real(cx, "the scout icons", |cx| all_ready(cx, &images, &scout));
    assert!(support::present(cx, win, ids::weapon_icon(script)));
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
