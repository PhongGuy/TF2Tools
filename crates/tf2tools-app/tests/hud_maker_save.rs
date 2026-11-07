//! Saving and installing from the HUD Maker.

use crate::support;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{HudEntry, LibraryKind, MakeTarget, PartKey, Platform, Tf2Install};

const HV: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";

fn stock_shell(cx: &mut TestAppContext, env: &support::Env) -> support::ShellWindow {
    support::seed_stock_game(&env.game);
    support::shell_window(
        cx,
        env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    )
}

fn open_maker(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<HudMakerPage> {
    support::click_nav(cx, s.win, Page::HudMaker);
    let page = cx.update(|cx| s.shell.read(cx).hud_maker_page()).unwrap();
    support::wait_real(cx, "the HUD to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

fn edit(cx: &mut TestAppContext, page: &Entity<HudMakerPage>, colour: &str) {
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            let e = p.editor_mut().unwrap();
            e.select(Some(PartKey::new(HV)));
            e.recolour_selected(colour).unwrap();
            cx.notify();
        })
    });
}

fn set_name(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    text: &str,
) {
    let input = cx.update(|cx| page.read(cx).name_input().clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.set_value(text, window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
}

fn save(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    button: &'static str,
) {
    // Save as new sits in the toolbar's overflow menu, and so does Save in a narrow toolbar.
    if button == ids::HUD_MAKER_SAVE_AS_NEW || !support::present(cx, s.win, button) {
        support::click_id(cx, s.win, ids::HUD_MAKER_MORE);
    }
    support::click_id(cx, s.win, button);
    support::wait_real(cx, "the save", |cx| {
        cx.update(|cx| !page.read(cx).is_saving())
    });
    support::settle(cx, s.win);
}

fn huds(s: &support::ShellWindow) -> Vec<HudEntry> {
    s.services.library.list_huds().unwrap()
}

fn saved(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> Option<String> {
    cx.update(|cx| page.read(cx).saved_hud().map(|id| id.to_string()))
}

#[gpui_kit::test]
fn save_is_disabled_until_the_hud_has_a_name(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    assert!(support::disabled_of(cx, s.win, ids::HUD_MAKER_SAVE));
    set_name(cx, &s, &page, "   ");
    assert!(support::disabled_of(cx, s.win, ids::HUD_MAKER_SAVE));
    set_name(cx, &s, &page, "My HUD");
    assert!(!support::disabled_of(cx, s.win, ids::HUD_MAKER_SAVE));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_SAVE_AS_NEW));
}

#[gpui_kit::test]
fn saving_puts_a_made_hud_in_the_library(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    edit(cx, &page, "255 0 0 255");
    set_name(cx, &s, &page, " My HUD ");
    save(cx, &s, &page, ids::HUD_MAKER_SAVE);
    let list = huds(&s);
    assert_eq!(
        list.iter().map(HudEntry::name).collect::<Vec<_>>(),
        ["My HUD"]
    );
    let folder = env
        .data
        .library_dir(LibraryKind::Huds)
        .join(list[0].folder_name());
    assert!(folder.join(".tf2tools/maker.json").is_file());
    assert!(folder.join("resource/ui/hudplayerhealth.res").is_file());
    assert!(cx.update(|cx| page.read(cx).can_leave()));
    assert_eq!(saved(cx, &page), Some(list[0].id().to_string()));
    assert_eq!(
        support::label_of(cx, s.win, ids::TOAST).as_deref(),
        Some("HUD saved. My HUD is in your HUDs library.")
    );
    set_name(cx, &s, &page, "Renamed");
    assert!(
        !cx.update(|cx| page.read(cx).can_leave()),
        "a rename alone is unsaved"
    );
    assert!(!support::disabled_of(cx, s.win, ids::HUD_MAKER_SAVE));
}

#[gpui_kit::test]
fn saving_again_keeps_one_hud_and_save_as_new_adds_another(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    edit(cx, &page, "255 0 0 255");
    set_name(cx, &s, &page, "My HUD");
    save(cx, &s, &page, ids::HUD_MAKER_SAVE);
    let first = saved(cx, &page);
    edit(cx, &page, "0 255 0 255");
    save(cx, &s, &page, ids::HUD_MAKER_SAVE);
    assert_eq!(huds(&s).len(), 1);
    assert_eq!(saved(cx, &page), first);
    set_name(cx, &s, &page, "My Copy");
    save(cx, &s, &page, ids::HUD_MAKER_SAVE_AS_NEW);
    assert_eq!(huds(&s).len(), 2);
    assert_ne!(saved(cx, &page), first);
}

#[gpui_kit::test]
fn a_taken_name_shows_under_the_field(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    s.services
        .library
        .save_made_hud(
            &MakeTarget::New,
            "Taken",
            &support::hud_stock::stock_files(),
            Platform::Linux,
            &[],
        )
        .unwrap();
    let page = open_maker(cx, &s);
    edit(cx, &page, "255 0 0 255");
    set_name(cx, &s, &page, "Taken");
    save(cx, &s, &page, ids::HUD_MAKER_SAVE);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUD_MAKER_NAME_ERROR).as_deref(),
        Some("You already have a HUD called Taken. Pick another name.")
    );
    assert_eq!(huds(&s).len(), 1);
    assert!(!cx.update(|cx| page.read(cx).can_leave()));
    set_name(cx, &s, &page, "Other");
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_NAME_ERROR));
}

#[gpui_kit::test]
fn a_hud_deleted_while_open_saves_as_new_after_a_warning(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    edit(cx, &page, "255 0 0 255");
    set_name(cx, &s, &page, "My HUD");
    save(cx, &s, &page, ids::HUD_MAKER_SAVE);
    let id = huds(&s)[0].id().clone();
    s.services.library.delete_hud(&id).unwrap();
    edit(cx, &page, "0 255 0 255");
    save(cx, &s, &page, ids::HUD_MAKER_SAVE);
    // The first save's success toast is still up, so look for the warning among the toasts.
    let warning =
        "Failed to save HUD. This HUD was deleted. Save it under a name to keep it as a new one.";
    assert!(
        support::toast_labels(cx, s.win)
            .iter()
            .any(|t| t == warning)
    );
    assert_eq!(saved(cx, &page), None);
    save(cx, &s, &page, ids::HUD_MAKER_SAVE);
    assert_eq!(huds(&s).len(), 1);
}

#[gpui_kit::test]
fn install_needs_a_saved_unchanged_hud_and_lands_in_tf_custom(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    assert!(support::disabled_of(cx, s.win, ids::HUD_MAKER_INSTALL));
    edit(cx, &page, "255 0 0 255");
    set_name(cx, &s, &page, "My HUD");
    save(cx, &s, &page, ids::HUD_MAKER_SAVE);
    assert!(!support::disabled_of(cx, s.win, ids::HUD_MAKER_INSTALL));
    support::click_id(cx, s.win, ids::HUD_MAKER_INSTALL);
    let folder = huds(&s)[0].folder_name().to_owned();
    let target = env.game.join("tf/custom").join(&folder);
    support::wait_real(cx, "the install", |_| target.join("info.vdf").is_file());
    assert!(target.join("resource/ui/hudplayerhealth.res").is_file());
    assert!(
        !target.join(".tf2tools").exists(),
        "installs leave .tf2tools out"
    );
    support::settle(cx, s.win);
    // The save's toast is still up, so look for the install's among all toasts.
    assert!(
        support::toast_labels(cx, s.win)
            .contains(&"HUD installed. My HUD is now active".to_owned())
    );
    edit(cx, &page, "0 255 0 255");
    assert!(support::disabled_of(cx, s.win, ids::HUD_MAKER_INSTALL));
}

#[gpui_kit::test]
fn install_asks_before_removing_a_hud_outside_the_library(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    edit(cx, &page, "255 0 0 255");
    set_name(cx, &s, &page, "My HUD");
    save(cx, &s, &page, ids::HUD_MAKER_SAVE);
    let custom = env.game.join("tf/custom");
    std::fs::create_dir_all(custom.join("manual")).unwrap();
    std::fs::write(custom.join("manual/info.vdf"), "mine").unwrap();

    support::click_id(cx, s.win, ids::HUD_MAKER_INSTALL);
    support::wait_real(cx, "the question", |cx| {
        support::present(cx, s.win, ids::EXTERNAL_HUDS_DIALOG)
    });
    support::click_id(cx, s.win, ids::EXTERNAL_HUDS_CANCEL);
    let folder = huds(&s)[0].folder_name().to_owned();
    support::wait_real(cx, "Install to come back", |cx| {
        !support::disabled_of(cx, s.win, ids::HUD_MAKER_INSTALL)
    });
    assert!(custom.join("manual/info.vdf").is_file());
    assert!(!custom.join(&folder).exists());

    support::click_id(cx, s.win, ids::HUD_MAKER_INSTALL);
    support::wait_real(cx, "the question again", |cx| {
        support::present(cx, s.win, ids::EXTERNAL_HUDS_DIALOG)
    });
    support::click_id(cx, s.win, ids::EXTERNAL_HUDS_KEEP);
    let target = custom.join(&folder);
    support::wait_real(cx, "the install", |_| target.join("info.vdf").is_file());
    assert!(huds(&s).iter().any(|h| h.folder_name() == "manual"));
    assert!(!custom.join("manual").exists());
}

fn saved_rows(s: &support::ShellWindow) -> usize {
    support::log_rows(&s.services)
        .iter()
        .filter(|row| row.message().starts_with("Saved HUD"))
        .count()
}

fn mouse(cx: &mut TestAppContext, s: &support::ShellWindow) -> gpui_kit::Point<gpui_kit::Pixels> {
    cx.update_window(s.win, |_, w, _| w.mouse_position())
        .unwrap()
}

/// One press and release at `at`, with no frame drawn after, so the next press still meets the
/// buttons as the last frame drew them.
fn click_raw(
    w: &mut gpui_kit::Window,
    at: gpui_kit::Point<gpui_kit::Pixels>,
    cx: &mut gpui_kit::App,
) {
    use gpui_kit::InputEvent as _;
    let left = gpui_kit::MouseButton::Left;
    let events = [
        gpui_kit::MouseMoveEvent {
            position: at,
            pressed_button: None,
            modifiers: Default::default(),
        }
        .to_platform_input(),
        gpui_kit::MouseDownEvent {
            button: left,
            position: at,
            modifiers: Default::default(),
            click_count: 1,
            first_mouse: false,
        }
        .to_platform_input(),
        gpui_kit::MouseUpEvent {
            button: left,
            position: at,
            modifiers: Default::default(),
            click_count: 1,
        }
        .to_platform_input(),
    ];
    for event in events {
        let _ = w.dispatch_event(event, cx);
    }
}

fn saved_once(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<HudMakerPage> {
    let page = open_maker(cx, s);
    edit(cx, &page, "255 0 0 255");
    set_name(cx, s, &page, "My HUD");
    save(cx, s, &page, ids::HUD_MAKER_SAVE);
    page
}

#[gpui_kit::test]
fn a_double_click_on_save_saves_once(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = saved_once(cx, &s);
    assert_eq!(saved_rows(&s), 1);
    cx.update_window(s.win, |_, w, cx| w.hover(ids::HUD_MAKER_SAVE, cx))
        .unwrap();
    let at = mouse(cx, &s);
    cx.update_window(s.win, |_, w, cx| {
        click_raw(w, at, cx);
        click_raw(w, at, cx);
    })
    .unwrap();
    support::wait_real(cx, "the save", |cx| {
        cx.update(|cx| !page.read(cx).is_saving())
    });
    support::settle(cx, s.win);
    assert_eq!(saved_rows(&s), 2);
}

#[gpui_kit::test]
fn save_and_save_as_new_wait_for_a_running_install(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let _page = saved_once(cx, &s);
    // The install's result cannot land inside this one update, so the frame shows it running.
    let disabled = cx
        .update_window(s.win, |_, w, cx| {
            w.click(ids::HUD_MAKER_INSTALL, cx);
            w.click(ids::HUD_MAKER_MORE, cx);
            w.render_frame(cx);
            (
                support::button_disabled(w, ids::HUD_MAKER_SAVE),
                support::button_disabled(w, ids::HUD_MAKER_SAVE_AS_NEW),
            )
        })
        .unwrap();
    assert_eq!(disabled, (true, true));
    let target = env.game.join("tf/custom").join(huds(&s)[0].folder_name());
    support::wait_real(cx, "the install", |_| target.join("info.vdf").is_file());
    support::settle(cx, s.win);
    assert!(!support::disabled_of(cx, s.win, ids::HUD_MAKER_SAVE));
}

#[gpui_kit::test]
fn a_save_click_that_lands_during_an_install_is_ignored(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = saved_once(cx, &s);
    let install = bounds(cx, &s, ids::HUD_MAKER_INSTALL).center();
    let save_at = bounds(cx, &s, ids::HUD_MAKER_SAVE).center();
    // Both clicks meet the frame drawn before the install, where Save is still enabled.
    let saving = cx
        .update_window(s.win, |_, w, cx| {
            click_raw(w, install, cx);
            click_raw(w, save_at, cx);
            page.read(cx).is_saving()
        })
        .unwrap();
    assert!(
        !saving,
        "a save never starts while the install copies the HUD"
    );
    let target = env.game.join("tf/custom").join(huds(&s)[0].folder_name());
    support::wait_real(cx, "the install", |_| target.join("info.vdf").is_file());
    support::settle(cx, s.win);
    assert_eq!(saved_rows(&s), 1);
}

fn bounds(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    id: &'static str,
) -> gpui_kit::Bounds<gpui_kit::Pixels> {
    support::bounds_of(cx, s.win, id).unwrap_or_else(|| panic!("no {id}"))
}

#[gpui_kit::test]
fn a_save_that_lands_after_leaving_still_tells_the_library(cx: &mut TestAppContext) {
    use std::cell::Cell;
    use std::rc::Rc;
    use tf2tools_app::library_events::{LibraryChanged, LibraryEvents};
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    edit(cx, &page, "255 0 0 255");
    set_name(cx, &s, &page, "My HUD");
    let count = Rc::new(Cell::new(0));
    let seen = Rc::clone(&count);
    let _sub = cx.update(|cx| {
        let events = LibraryEvents::global(cx);
        cx.subscribe(&events, move |_, _: &LibraryChanged, _| {
            seen.set(seen.get() + 1)
        })
    });
    // Save, then leave in the same update, so the page is gone before the save lands.
    let shell = s.shell.clone();
    cx.update_window(s.win, |_, w, cx| {
        w.click(ids::HUD_MAKER_SAVE, cx);
        shell.update(cx, |shell, cx| shell.navigate(Page::Sounds, cx));
    })
    .unwrap();
    let page = {
        let weak = page.downgrade();
        drop(page);
        weak
    };
    support::wait_real(cx, "the save", |_| huds(&s).len() == 1);
    support::wait_real(cx, "the library event", |_| count.get() == 1);
    assert!(page.upgrade().is_none(), "the maker was dropped");
}

// Save as new under the saved HUD's own name, which core refuses as taken.
fn taken_by_save_as_new(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<HudMakerPage> {
    let page = saved_once(cx, s);
    save(cx, s, &page, ids::HUD_MAKER_SAVE_AS_NEW);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_NAME_ERROR));
    page
}

#[gpui_kit::test]
fn the_taken_name_line_clears_once_the_name_changes(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = taken_by_save_as_new(cx, &s);
    set_name(cx, &s, &page, "Other");
    set_name(cx, &s, &page, "My HUD");
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_NAME_ERROR));
}

#[gpui_kit::test]
fn the_taken_name_line_clears_when_a_save_succeeds(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = taken_by_save_as_new(cx, &s);
    save(cx, &s, &page, ids::HUD_MAKER_SAVE);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_NAME_ERROR));
}

#[test]
fn the_install_tip_says_why_install_waits() {
    use tf2tools_app::pages::hud_maker::install_tip;
    assert_eq!(install_tip(true, true), "Saving...");
    assert_eq!(install_tip(false, false), "TF2 not found");
    assert_eq!(install_tip(false, true), "Save the HUD first");
}

fn toast_boxes(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
) -> Vec<gpui_kit::Bounds<gpui_kit::Pixels>> {
    support::settle(cx, s.win);
    let toast = gpui_kit::ElementId::from(ids::TOAST);
    cx.update_window(s.win, |_, w, _| {
        gpui_kit::base::test_support::snapshots(w)
            .iter()
            .filter(|s| s.path().last() == Some(&toast))
            .map(|s| s.bounds())
            .collect()
    })
    .unwrap()
}

#[gpui_kit::test]
fn toasts_sit_above_the_toolbar_and_save_stays_clickable(cx: &mut TestAppContext) {
    for (w, h, colour) in [(1200., 900., "255 0 0 255"), (800., 600., "0 255 0 255")] {
        let env = support::env();
        let s = stock_shell(cx, &env);
        cx.simulate_window_resize(s.win, gpui_kit::size(gpui_kit::px(w), gpui_kit::px(h)));
        support::settle(cx, s.win);
        let page = open_maker(cx, &s);
        edit(cx, &page, colour);
        set_name(cx, &s, &page, "My HUD");
        save(cx, &s, &page, ids::HUD_MAKER_SAVE);
        let bar = bounds(cx, &s, ids::HUD_MAKER_TOOLBAR);
        let toasts = toast_boxes(cx, &s);
        assert!(!toasts.is_empty(), "{w}x{h}: the save shows a toast");
        for t in toasts {
            assert!(
                !t.intersects(&bar),
                "{w}x{h}: toast {t:?} over toolbar {bar:?}"
            );
        }
        // A second save goes through the Save button while the first toast is still up.
        edit(cx, &page, "0 0 255 255");
        save(cx, &s, &page, ids::HUD_MAKER_SAVE);
        assert!(
            cx.update(|cx| page.read(cx).can_leave()),
            "{w}x{h}: Save missed"
        );
    }
}

#[gpui_kit::test]
fn toasts_go_back_down_when_the_toolbar_collapses(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    edit(cx, &page, "255 0 0 255");
    set_name(cx, &s, &page, "My HUD");
    save(cx, &s, &page, ids::HUD_MAKER_SAVE);
    let lifted = toast_boxes(cx, &s)[0];
    let bar = bounds(cx, &s, ids::HUD_MAKER_TOOLBAR);
    let window = cx
        .update_window(s.win, |_, w, _| w.viewport_size())
        .unwrap();
    support::click_id(cx, s.win, ids::HUD_MAKER_TOOLBAR_COLLAPSE);
    let down = toast_boxes(cx, &s)[0];
    // Lifted, the stack's bottom margin is the toolbar's top plus 8 px; back down it is 16 px.
    let lift = f32::from(window.height - bar.top()) + 8.;
    let dropped = f32::from(down.bottom() - lifted.bottom());
    assert!(
        (dropped - (lift - 16.)).abs() <= 1.,
        "{down:?} dropped {dropped} from {lifted:?}, not {}",
        lift - 16.
    );
}

#[gpui_kit::test]
fn the_save_row_wraps_inside_the_toolbar(cx: &mut TestAppContext) {
    // 800x600 as it opens, and 1024x768 with the parts panel docked open on the right.
    for (w, h, docked) in [(800., 600., false), (1024., 768., true)] {
        let env = support::env();
        let s = stock_shell(cx, &env);
        cx.simulate_window_resize(s.win, gpui_kit::size(gpui_kit::px(w), gpui_kit::px(h)));
        support::settle(cx, s.win);
        let page = open_maker(cx, &s);
        support::maker_page::float_panels(cx, &s, &page);
        if docked {
            support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_DOCK_RIGHT);
            support::click_id(cx, s.win, ids::HUD_MAKER_PARTS_PANEL_COLLAPSE);
            assert!(support::present(cx, s.win, ids::HUD_MAKER_PARTS));
        }
        edit(cx, &page, "255 0 0 255");
        set_name(cx, &s, &page, "My HUD");
        save(cx, &s, &page, ids::HUD_MAKER_SAVE);
        let bar = bounds(cx, &s, ids::HUD_MAKER_TOOLBAR);
        let area = bounds(cx, &s, ids::HUD_MAKER_CANVAS_AREA);
        let inside = |outer: gpui_kit::Bounds<gpui_kit::Pixels>,
                      b: gpui_kit::Bounds<gpui_kit::Pixels>| {
            b.left() >= outer.left()
                && b.top() >= outer.top()
                && b.right() <= outer.right()
                && b.bottom() <= outer.bottom()
        };
        assert!(
            inside(area, bar),
            "{w}x{h}: toolbar {bar:?} outside {area:?}"
        );
        for id in [
            ids::HUD_MAKER_NAME,
            ids::HUD_MAKER_SAVE,
            ids::HUD_MAKER_MORE,
            ids::HUD_MAKER_INSTALL,
            ids::HUD_MAKER_TOOLBAR_COLLAPSE,
        ] {
            let b = bounds(cx, &s, id);
            assert!(inside(bar, b), "{w}x{h}: {id} {b:?} pokes out of {bar:?}");
        }
        support::click_id(cx, s.win, ids::HUD_MAKER_INSTALL);
        let folder = huds(&s)[0].folder_name().to_owned();
        let target = env.game.join("tf/custom").join(&folder);
        support::wait_real(cx, "the install", |_| target.join("info.vdf").is_file());
        edit(cx, &page, "0 255 0 255");
        save(cx, &s, &page, ids::HUD_MAKER_SAVE);
        assert!(
            cx.update(|cx| page.read(cx).can_leave()),
            "{w}x{h}: Save missed"
        );
    }
}

#[gpui_kit::test]
fn an_install_blocked_by_a_players_folder_says_so(cx: &mut TestAppContext) {
    let env = support::env();
    let s = stock_shell(cx, &env);
    let page = open_maker(cx, &s);
    edit(cx, &page, "255 0 0 255");
    set_name(cx, &s, &page, "My HUD");
    save(cx, &s, &page, ids::HUD_MAKER_SAVE);
    // A made HUD built on another HUD keeps that HUD's #base paths into tf/custom.
    let folder = huds(&s)[0].folder_name().to_owned();
    std::fs::write(
        env.data
            .library_dir(LibraryKind::Huds)
            .join(&folder)
            .join("resource/ui/extra.res"),
        "#base \"../../custom/basehud/resource/ui/base.res\"\n",
    )
    .unwrap();
    let mine = env.game.join("tf/custom/basehud");
    std::fs::create_dir_all(&mine).unwrap();
    std::fs::write(mine.join("notes.txt"), "mine").unwrap();
    support::click_id(cx, s.win, ids::HUD_MAKER_INSTALL);
    let target = env.game.join("tf/custom").join(&folder);
    support::wait_real(cx, "the install", |_| target.join("info.vdf").is_file());
    support::settle(cx, s.win);
    let warning = "My HUD installed. A folder named basehud is already in your custom \
                   folder, so parts of this HUD may not show. Move or rename that folder \
                   and install again.";
    assert!(
        support::toast_labels(cx, s.win).contains(&warning.to_owned()),
        "{:?}",
        support::toast_labels(cx, s.win)
    );
    assert_eq!(std::fs::read(mine.join("notes.txt")).unwrap(), b"mine");
}
