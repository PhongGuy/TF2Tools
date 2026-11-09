//! A copy's own options in the HUD Maker: reload, save, the Options section and a part's options.

use crate::support;

use gpui_kit::component::color_picker::ColorPickerEvent;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, ScrollDelta, TestAppContext, point};
use support::hud_switches::{GROUP, HV, SLOT, SWITCH, seed_switch_copy};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_app::services::{Services, Tf2State};
use tf2tools_app::shell::route::Page;
use tf2tools_core::{
    HudEntry, LibraryKind, OptionKind, OptionValue, PartKey, Platform, Tf2Install,
};

type Opened = (
    support::Env,
    support::ShellWindow,
    Entity<HudMakerPage>,
    HudEntry,
);

fn opened(cx: &mut TestAppContext) -> Opened {
    open_copy(cx, false)
}

// Opens the switch copy in the HUD Maker; `colour` gives it the one-colour schema too.
fn open_copy(cx: &mut TestAppContext, colour: bool) -> Opened {
    let schema = colour.then_some(support::hud_switches::COLOUR_SCHEMA);
    open_with(cx, schema, |_, _| {})
}

// Opens the switch copy with `schema` as its own, after `seed` fills the library.
fn open_with(
    cx: &mut TestAppContext,
    schema: Option<&'static str>,
    seed: impl FnOnce(&Services, &support::Env),
) -> Opened {
    let env = support::env();
    support::seed_stock_game(&env.game);
    let state = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    let s = support::shell_window_tweaked(cx, &env, state, |services| {
        if let Some(json) = schema {
            support::hud_switches::with_schema(services, json);
        }
    });
    let copy = seed_switch_copy(&s.services, env.tmp.path());
    if schema.is_some() {
        support::hud_switches::give_colour_repo(&s.services, &copy);
    }
    seed(&s.services, &env);
    support::click_id(cx, s.win, ids::nav(Page::Sounds));
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    support::click_id(cx, s.win, ids::hud_edit(copy.folder_name()));
    let page = cx.update(|cx| s.shell.read(cx).hud_maker_page()).unwrap();
    support::wait_real(cx, "the HUD to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    (env, s, page, copy)
}

// Chooses through the page, as the option rows do, so the reload starts without a drawn frame.
fn set(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    id: &str,
    value: OptionValue,
) {
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| p.set_hud_option(id, value, window, cx))
    })
    .unwrap();
}

fn alignment(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> Option<String> {
    cx.update(|cx| {
        let e = page.read(cx).editor()?;
        e.model()
            .part(&PartKey::new(HV))?
            .value("textAlignment")
            .map(|l| l.text().to_owned())
    })
}

fn wait_reloaded(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) {
    support::wait_real(cx, "the switch reload", |cx| {
        cx.update(|cx| {
            page.read(cx)
                .editor()
                .is_some_and(|e| !e.needs_switch_reload())
        })
    });
}

#[gpui_kit::test]
fn a_switch_change_reloads_the_canvas_and_undo_takes_it_back(cx: &mut TestAppContext) {
    let (_env, s, page, _copy) = opened(cx);
    set(cx, &s, &page, SLOT, OptionValue::Bool(true));
    wait_reloaded(cx, &page);
    assert_eq!(alignment(cx, &page).as_deref(), Some("east"));
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| p.undo(window, cx))
    })
    .unwrap();
    wait_reloaded(cx, &page);
    assert_eq!(alignment(cx, &page), None);
}

#[gpui_kit::test]
fn undo_while_a_reload_runs_drops_it_and_keeps_the_old_model(cx: &mut TestAppContext) {
    let (_env, s, page, _copy) = opened(cx);
    // One update: nothing lands in between, so the reload is still running when undo comes.
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| {
            p.set_hud_option(SLOT, OptionValue::Bool(true), window, cx);
            assert!(p.is_switch_reloading());
            p.undo(window, cx);
            assert!(!p.is_switch_reloading());
        })
    })
    .unwrap();
    support::settle(cx, s.win);
    wait_reloaded(cx, &page);
    assert_eq!(alignment(cx, &page), None);
    assert!(cx.update(|cx| page.read(cx).editor().unwrap().changes().is_empty()));
}

#[gpui_kit::test]
fn a_new_choice_while_a_reload_runs_replaces_it(cx: &mut TestAppContext) {
    let (_env, s, page, _copy) = opened(cx);
    let (started, landed) = cx.update(|cx| page.read(cx).switch_reload_stats());
    let arrived = cx.update(|cx| page.read(cx).switch_arrivals());
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| {
            p.set_hud_option(SLOT, OptionValue::Bool(true), window, cx);
            p.set_hud_option(
                GROUP,
                OptionValue::Pick("resource/ui/damage_small.res".into()),
                window,
                cx,
            );
            assert!(p.is_switch_reloading());
            // Two jobs started; the first was dropped when the second replaced it.
            assert_eq!(p.switch_reload_stats().0, started + 2);
            assert_eq!(p.live_switch_jobs(), 1);
        })
    })
    .unwrap();
    wait_reloaded(cx, &page);
    for _ in 0..5 {
        cx.run_until_parked();
    }
    // The first job was cancelled with its task, so its result never even arrived.
    std::thread::sleep(std::time::Duration::from_millis(100));
    for _ in 0..5 {
        cx.run_until_parked();
    }
    assert_eq!(cx.update(|cx| page.read(cx).switch_arrivals()), arrived + 1);
    // Only the second job ever landed.
    assert_eq!(
        cx.update(|cx| page.read(cx).switch_reload_stats()),
        (started + 2, landed + 1)
    );
    assert_eq!(cx.update(|cx| page.read(cx).live_switch_jobs()), 0);
    assert_eq!(alignment(cx, &page).as_deref(), Some("east"));
    assert_eq!(
        cx.update(|cx| page.read(cx).editor().unwrap().switch_choices().len()),
        2
    );
}

#[gpui_kit::test]
fn a_saved_switch_keeps_the_base_bytes_as_its_original(cx: &mut TestAppContext) {
    let (env, s, page, copy) = opened(cx);
    set(
        cx,
        &s,
        &page,
        GROUP,
        OptionValue::Pick("resource/ui/damage_small.res".into()),
    );
    wait_reloaded(cx, &page);
    support::click_id(cx, s.win, ids::HUD_MAKER_SAVE);
    support::wait_real(cx, "the save", |cx| {
        cx.update(|cx| !page.read(cx).is_saving())
    });
    let dir = env
        .data
        .library_dir(LibraryKind::Huds)
        .join(copy.folder_name());
    let live = std::fs::read_to_string(dir.join("customizations/damage.res")).unwrap();
    assert!(live.contains("\n#base \"../resource/ui/damage_small.res\""));
    assert_eq!(
        std::fs::read_to_string(dir.join(".tf2tools/original/customizations/damage.res")).unwrap(),
        SWITCH
    );
    // The reopened pristine files, not the switched view, feed the next save.
    set(
        cx,
        &s,
        &page,
        GROUP,
        OptionValue::Pick("resource/ui/damage_medium.res".into()),
    );
    wait_reloaded(cx, &page);
    support::click_id(cx, s.win, ids::HUD_MAKER_SAVE);
    support::wait_real(cx, "the second save", |cx| {
        cx.update(|cx| !page.read(cx).is_saving())
    });
    assert_eq!(
        std::fs::read_to_string(dir.join("customizations/damage.res")).unwrap(),
        SWITCH
    );
    assert!(
        !dir.join(".tf2tools/original/customizations/damage.res")
            .exists()
    );
}

// Clicks `id` in the side panel, scrolling the panel down until it is wholly in view.
fn click_in_panel(cx: &mut TestAppContext, s: &support::ShellWindow, id: gpui_kit::ElementId) {
    for _ in 0..40 {
        let seen = support::present(cx, s.win, id.clone())
            && cx
                .update_window(s.win, |_, w, _| {
                    let body = w.try_find(ids::HUD_MAKER_PANEL_BODY).map(|b| b.bounds());
                    w.try_find(id.clone()).is_some_and(|e| {
                        let b = e.bounds();
                        e.visible() && body.is_none_or(|body| b.bottom() < body.bottom())
                    })
                })
                .unwrap();
        if seen {
            break;
        }
        let down = ScrollDelta::Lines(point(0., -3.));
        cx.update_window(s.win, |_, w, cx| {
            w.scroll(ids::HUD_MAKER_PANEL_BODY, down, cx)
        })
        .unwrap();
    }
    support::click_id(cx, s.win, id);
}

#[gpui_kit::test]
fn the_section_lists_the_copys_groups_and_options(cx: &mut TestAppContext) {
    let (_env, s, _page, _copy) = opened(cx);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_HUD_OPTIONS));
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_hud_option_group("Damage")).as_deref(),
        Some("Damage")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_hud_option_group("Switches")).as_deref(),
        Some("Switches")
    );
    assert_eq!(
        support::checked_of(cx, s.win, ids::hud_maker_option(SLOT)),
        Some(false)
    );
    assert!(support::present(
        cx,
        s.win,
        ids::hud_maker_option_choice(GROUP, "resource/ui/damage_small.res")
    ));
    assert!(!support::present(
        cx,
        s.win,
        ids::HUD_MAKER_HUD_OPTIONS_MISFITS
    ));
}

#[gpui_kit::test]
fn clicking_a_choice_and_a_switch_records_them(cx: &mut TestAppContext) {
    let (_env, s, page, _copy) = opened(cx);
    click_in_panel(
        cx,
        &s,
        ids::hud_maker_option_choice(GROUP, "resource/ui/damage_small.res"),
    );
    click_in_panel(cx, &s, ids::hud_maker_option(SLOT));
    wait_reloaded(cx, &page);
    let values = cx.update(|cx| {
        let e = page.read(cx).editor().unwrap();
        (e.hud_option_value(GROUP), e.hud_option_value(SLOT))
    });
    assert_eq!(
        values,
        (
            Some(OptionValue::Pick("resource/ui/damage_small.res".into())),
            Some(OptionValue::Bool(true))
        )
    );
    assert_eq!(
        support::checked_of(cx, s.win, ids::hud_maker_option(SLOT)),
        Some(true)
    );
    click_in_panel(cx, &s, ids::hud_maker_hud_option_reset("Switches"));
    assert_eq!(
        cx.update(|cx| page.read(cx).editor().unwrap().hud_option_value(SLOT)),
        Some(OptionValue::Bool(false))
    );
}

#[gpui_kit::test]
fn reset_all_asks_first_and_undo_brings_the_options_back(cx: &mut TestAppContext) {
    let (_env, s, page, _copy) = opened(cx);
    click_in_panel(cx, &s, ids::hud_maker_option(SLOT));
    wait_reloaded(cx, &page);
    let slot = |cx: &mut TestAppContext| {
        cx.update(|cx| page.read(cx).editor().unwrap().hud_option_value(SLOT))
    };
    click_in_panel(cx, &s, ids::HUD_MAKER_OPTIONS_RESET_ALL.into());
    assert_eq!(
        support::label_of(cx, s.win, ids::CONFIRM_TITLE).as_deref(),
        Some("Reset all options?")
    );
    support::click_id(cx, s.win, ids::CONFIRM_CANCEL);
    support::settle(cx, s.win);
    assert_eq!(slot(cx), Some(OptionValue::Bool(true)));

    click_in_panel(cx, &s, ids::HUD_MAKER_OPTIONS_RESET_ALL.into());
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    support::wait_real(cx, "the reset", |cx| {
        cx.update(|cx| page.read(cx).editor().unwrap().hud_option_value(SLOT))
            == Some(OptionValue::Bool(false))
    });
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| p.undo(window, cx))
    })
    .unwrap();
    assert_eq!(slot(cx), Some(OptionValue::Bool(true)));
}

#[gpui_kit::test]
fn the_default_hud_has_no_section(cx: &mut TestAppContext) {
    let env = support::env();
    support::seed_stock_game(&env.game);
    let s = support::shell_window(
        cx,
        &env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    );
    support::click_nav(cx, s.win, Page::HudMaker);
    let page = cx.update(|cx| s.shell.read(cx).hud_maker_page()).unwrap();
    support::wait_real(cx, "the HUD to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_OPTIONS));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_HUD_OPTIONS));
}

fn select(cx: &mut TestAppContext, page: &Entity<HudMakerPage>, key: &str) {
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            p.editor_mut().unwrap().select(Some(PartKey::new(key)));
            cx.notify();
        })
    });
}

#[gpui_kit::test]
fn the_health_value_shows_its_slot_switch(cx: &mut TestAppContext) {
    let (_env, s, page, _copy) = opened(cx);
    select(cx, &page, HV);
    support::settle(cx, s.win);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_PART_OPTIONS));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_HUD_OPTIONS));
    click_in_panel(cx, &s, ids::hud_maker_option(SLOT));
    wait_reloaded(cx, &page);
    assert_eq!(alignment(cx, &page).as_deref(), Some("east"));
    assert_eq!(
        cx.update(|cx| page
            .read(cx)
            .editor()
            .unwrap()
            .selected()
            .map(|k| k.as_str().to_owned())),
        Some(HV.to_owned())
    );
}

#[gpui_kit::test]
fn a_part_no_option_touches_has_no_section(cx: &mut TestAppContext) {
    let (_env, s, page, _copy) = opened(cx);
    let other = cx.update(|cx| {
        let e = page.read(cx).editor().unwrap();
        e.model()
            .parts()
            .iter()
            .find(|p| p.name() == "AmmoInClip")
            .map(|p| p.key().as_str().to_owned())
            .unwrap()
    });
    select(cx, &page, &other);
    support::settle(cx, s.win);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_PART_OPTIONS));
}

#[gpui_kit::test]
fn a_page_opened_before_an_update_refuses_to_save(cx: &mut TestAppContext) {
    let (env, s, page, copy) = opened(cx);
    let mut extra = support::hud_switches::extra();
    extra.push(("NEW.txt".into(), "new".into()));
    let refs: Vec<(&str, &str)> = extra
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    let newer = support::hud_copies::import_stock_hud(
        &s.services,
        env.tmp.path(),
        "somehud",
        &refs,
        support::hud_copies::store_item(),
    );
    let tf = Tf2Install::from_path(&env.game).unwrap().tf_dir();
    s.services
        .library
        .update_copy(copy.id(), newer.id(), &tf, Platform::Linux)
        .unwrap();
    set(
        cx,
        &s,
        &page,
        GROUP,
        OptionValue::Pick("resource/ui/damage_small.res".into()),
    );
    wait_reloaded(cx, &page);
    support::click_id(cx, s.win, ids::HUD_MAKER_SAVE);
    support::wait_real(cx, "the save", |cx| {
        cx.update(|cx| !page.read(cx).is_saving())
    });
    let said = support::toast_labels(cx, s.win);
    assert!(said.iter().any(|t| t == "This HUD was updated while it was open. Close it and open it again to keep editing."), "{said:?}");
    let dir = env
        .data
        .library_dir(LibraryKind::Huds)
        .join(copy.folder_name());
    assert!(dir.join("NEW.txt").is_file());
    assert_eq!(
        std::fs::read_to_string(dir.join("customizations/damage.res")).unwrap(),
        SWITCH
    );
    assert_eq!(
        cx.update(|cx| page.read(cx).editor().unwrap().changes().len()),
        1
    );
}

// Emits `rgb` from the option picker, as a drag or a palette pick does.
fn pick_option_colour(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    rgb: [u8; 3],
) {
    let picker = cx.update(|cx| page.read(cx).option_colour_state().clone());
    cx.update_window(s.win, |_, _, cx| {
        picker.update(cx, |_, cx| {
            cx.emit(ColorPickerEvent::Change(Some(
                tf2tools_app::pages::crosshair_maker::model::hsla_of(rgb),
            )))
        })
    })
    .unwrap();
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn a_built_in_swatch_keeps_a_picked_hud_colour_on_its_own_option(cx: &mut TestAppContext) {
    let (_env, s, page, _copy) = open_copy(cx, true);
    let (hud, built_in, before) = cx.update(|cx| {
        let e = page.read(cx).editor().unwrap();
        let colour = |kind: &OptionKind| matches!(kind, OptionKind::Colour { .. });
        let hud = (e.hud_options().unwrap().specs().iter())
            .find(|spec| colour(spec.kind()))
            .map(|spec| spec.id().to_owned())
            .unwrap();
        let built_in = (e.options().into_iter())
            .find(|spec| colour(spec.kind()))
            .map(|spec| spec.id())
            .unwrap();
        (hud, built_in, e.option_value(built_in))
    });
    click_in_panel(cx, &s, ids::hud_maker_option(&hud));
    pick_option_colour(cx, &s, &page, [10, 20, 30]);
    // A second swatch takes the picker over while the HUD colour is still waiting to be kept.
    click_in_panel(cx, &s, ids::hud_maker_option(built_in));
    let picker = cx.update(|cx| page.read(cx).option_colour_state().clone());
    assert!(cx.update(|cx| picker.read(cx).is_open()));
    pick_option_colour(cx, &s, &page, [40, 50, 60]);
    cx.update_window(s.win, |_, _, cx| {
        picker.update(cx, |st, cx| st.set_open(false, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
    wait_reloaded(cx, &page);
    let (hud_now, built_in_now) = cx.update(|cx| {
        let e = page.read(cx).editor().unwrap();
        (e.hud_option_value(&hud), e.option_value(built_in))
    });
    assert_eq!(hud_now, Some(OptionValue::Rgba([10, 20, 30, 255])));
    assert_eq!(built_in_now, Some(OptionValue::Rgba([40, 50, 60, 255])));
    assert_ne!(before, built_in_now);
}

#[gpui_kit::test]
fn a_saved_view_for_older_choices_never_replaces_newer_shown_files(cx: &mut TestAppContext) {
    let (_env, s, page, _copy) = opened(cx);
    set(
        cx,
        &s,
        &page,
        GROUP,
        OptionValue::Pick("resource/ui/damage_small.res".into()),
    );
    wait_reloaded(cx, &page);
    let newer = cx.update(|cx| page.read(cx).shown_files()).unwrap();
    // A save sent before the reload landed comes back with its view of no choices at all.
    let older = support::hud_switches::switch_files;
    cx.update(|cx| page.update(cx, |p, _| p.land_saved_files(older(), older(), &[])));
    let shown = cx.update(|cx| page.read(cx).shown_files()).unwrap();
    assert!(
        std::sync::Arc::ptr_eq(&shown, &newer),
        "the older view replaced the newer files"
    );
    // A save sent with the choices on show does replace them.
    let sent = cx.update(|cx| page.read(cx).editor().unwrap().switch_choices());
    cx.update(|cx| page.update(cx, |p, _| p.land_saved_files(older(), older(), &sent)));
    let shown = cx.update(|cx| page.read(cx).shown_files()).unwrap();
    assert!(!std::sync::Arc::ptr_eq(&shown, &newer));
}

// It locks a folder with Unix permissions, which Windows does not have.
#[cfg(unix)]
#[gpui_kit::test]
fn a_reload_that_cannot_read_the_files_says_so_in_plain_words(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt as _;
    let (env, s, page, copy) = opened(cx);
    let file = (env.data.library_dir(LibraryKind::Huds))
        .join(copy.folder_name())
        .join("customizations/damage.res");
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
    set(
        cx,
        &s,
        &page,
        GROUP,
        OptionValue::Pick("resource/ui/damage_small.res".into()),
    );
    support::wait_real(cx, "the reload to fail", |cx| {
        cx.update(|cx| !page.read(cx).is_switch_reloading())
    });
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    let said = support::toast_labels(cx, s.win);
    let line = said.iter().find(|t| t.contains("option")).expect("a toast");
    assert!(
        line.ends_with(tf2tools_app::local_messages::HUD_OPTIONS_UNREADABLE),
        "{line}"
    );
    support::assert_plain(line);
}

const VM: &str = "e:sh_vm";
const VIEWMODELS: &str = "mastercomfig-addon-transparent-viewmodels.vpk";

fn need(cx: &mut TestAppContext, s: &support::ShellWindow) -> Option<String> {
    support::label_of(cx, s.win, ids::hud_maker_option_need(VM))
}

#[gpui_kit::test]
fn a_viewmodels_option_says_where_to_get_the_addon(cx: &mut TestAppContext) {
    let (_env, s, _page, _copy) = open_with(
        cx,
        Some(support::hud_switches::VIEWMODELS_SCHEMA),
        |_, _| {},
    );
    assert!(support::present(cx, s.win, ids::hud_maker_option(VM)));
    assert_eq!(
        need(cx, &s).as_deref(),
        Some(
            "Needs Transparent Viewmodels. Get it from the Addon store, then turn it on at the end of the options."
        )
    );
}

#[gpui_kit::test]
fn a_viewmodels_option_asks_for_the_addon_until_it_is_on(cx: &mut TestAppContext) {
    let (_env, s, _page, _copy) = open_with(
        cx,
        Some(support::hud_switches::VIEWMODELS_SCHEMA),
        |services, env| {
            let vpk = support::vpk_bytes(&[("cfg/addons/x.cfg", b"v1".as_slice())]);
            support::seed_library_addon(services, env, VIEWMODELS, "9.0.0", &vpk, false);
            support::seed_managed_mastercomfig(env, "9.100.1", "medium");
        },
    );
    support::wait_real(cx, "the addon state", |cx| {
        support::checked_of(cx, s.win, ids::HUD_MAKER_VIEWMODELS).is_some()
    });
    assert_eq!(
        need(cx, &s).as_deref(),
        Some("Needs Transparent viewmodels turned on at the end of the options.")
    );
    click_in_panel(cx, &s, ids::HUD_MAKER_VIEWMODELS.into());
    support::wait_real(cx, "the addon to install", |cx| {
        support::checked_of(cx, s.win, ids::HUD_MAKER_VIEWMODELS) == Some(true)
    });
    support::settle(cx, s.win);
    assert_eq!(need(cx, &s), None);
}
