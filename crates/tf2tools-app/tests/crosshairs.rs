use crate::support;

use std::sync::Arc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::crosshairs::CrosshairsPage;
use tf2tools_app::picker::{PickKind, ScriptedPicker};
use tf2tools_app::previews::{CrosshairPreviews, Preview};
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{
    CrosshairEntry, CrosshairRef, LibraryKind, Tf2Install, TfClass, Weapon, WeaponSlot, weapons,
};

fn found(env: &support::Env) -> Tf2State {
    Tf2State::Found(Tf2Install::from_path(&env.game).unwrap())
}

/// Opens the Crosshairs page (the shell starts on HUDs) and waits for its load.
fn open_crosshairs(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<CrosshairsPage> {
    support::click_id(cx, s.win, ids::nav(Page::Crosshairs));
    let page = cx
        .update(|cx| s.shell.read(cx).crosshairs_page())
        .expect("a Crosshairs page");
    support::wait_real(cx, "Crosshairs to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

/// Leaves for HUDs and comes back, which builds a fresh page.
fn revisit(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<CrosshairsPage> {
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    open_crosshairs(cx, s)
}

fn search(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<CrosshairsPage>,
    text: &str,
) {
    let input = cx.update(|cx| page.read(cx).search_input().clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.set_value(text, window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn crosshairs_is_a_real_page(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_crosshairs(cx, &s);
    assert!(support::present(cx, s.win, ids::page(Page::Crosshairs)));
    assert!(!support::present(cx, s.win, ids::PLACEHOLDER_MESSAGE));
    assert_eq!(
        support::label_of(cx, s.win, ids::HEADER_TITLE).as_deref(),
        Some("Crosshairs")
    );
    support::assert_starts_under_top_bar(
        cx,
        s.win,
        ids::page(Page::Crosshairs),
        ids::CROSSHAIRS_STATUS,
    );
    assert!(support::present(cx, s.win, ids::CROSSHAIRS_SEARCH));
    assert!(support::present(cx, s.win, ids::CROSSHAIRS_LIST));
    assert!(!support::present(cx, s.win, ids::CROSSHAIRS_SWATCH));
}

#[gpui_kit::test]
fn the_list_opens_on_the_builtin_group(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_crosshairs(cx, &s);
    let builtins = Arc::clone(&s.services.builtins);
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIRS_GROUP_BUILTIN),
        Some(format!("Built-in ({})", builtins.len()))
    );
    let first = &builtins.all()[0];
    assert_eq!(
        support::label_of(cx, s.win, ids::crosshair_builtin(first.name())).as_deref(),
        Some(first.display_name())
    );
    let last = builtins.all().last().unwrap();
    assert!(
        !support::present(cx, s.win, ids::crosshair_builtin(last.name())),
        "the list builds only its visible rows"
    );
}

#[gpui_kit::test]
fn search_filters_both_groups_ignoring_case(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let zebra = support::seed_crosshair(&s.services, &env, "Zebra Dot");
    let page = open_crosshairs(cx, &s);
    search(cx, &s, &page, "ZEBRA");
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIRS_GROUP_LIBRARY).as_deref(),
        Some("My Crosshairs (1)")
    );
    assert!(!support::present(cx, s.win, ids::CROSSHAIRS_GROUP_BUILTIN));
    assert_eq!(
        support::label_of(cx, s.win, ids::crosshair_library(zebra.folder_name())).as_deref(),
        Some("Zebra Dot")
    );
    search(cx, &s, &page, "default seeker");
    assert!(support::present(cx, s.win, ids::CROSSHAIRS_GROUP_BUILTIN));
    assert!(!support::present(cx, s.win, ids::CROSSHAIRS_GROUP_LIBRARY));
    search(cx, &s, &page, "zzzz");
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIRS_NO_MATCH).as_deref(),
        Some("No crosshairs match your search")
    );
}

#[gpui_kit::test]
fn clicking_an_option_selects_it(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let zebra = support::seed_crosshair(&s.services, &env, "Zebra Dot");
    let page = open_crosshairs(cx, &s);
    let first = s.services.builtins.all()[0].clone();
    support::click_id(cx, s.win, ids::crosshair_builtin(first.name()));
    assert_eq!(
        cx.update(|cx| page.read(cx).selected().cloned()),
        Some(CrosshairRef::Builtin(first.name().to_owned()))
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIRS_SELECTED_NAME).as_deref(),
        Some(first.display_name())
    );
    search(cx, &s, &page, "zebra");
    support::click_id(cx, s.win, ids::crosshair_library(zebra.folder_name()));
    assert_eq!(
        cx.update(|cx| page.read(cx).selected().cloned()),
        Some(CrosshairRef::Library(zebra.id().clone()))
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIRS_SELECTED_NAME).as_deref(),
        Some("Zebra Dot")
    );
}

#[gpui_kit::test]
fn a_revisit_reloads_the_library(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_crosshairs(cx, &s);
    search(cx, &s, &page, "zebra");
    assert!(support::present(cx, s.win, ids::CROSSHAIRS_NO_MATCH));
    let zebra = support::seed_crosshair(&s.services, &env, "Zebra Dot");
    let page = revisit(cx, &s);
    search(cx, &s, &page, "zebra");
    assert!(support::present(
        cx,
        s.win,
        ids::crosshair_library(zebra.folder_name())
    ));
}

#[gpui_kit::test]
fn opening_the_page_decodes_the_library_previews(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let zebra = support::seed_crosshair(&s.services, &env, "Zebra Dot");
    let _page = open_crosshairs(cx, &s);
    let previews = cx.update(CrosshairPreviews::global);
    support::wait_real(cx, "the decode", |cx| {
        cx.update(|cx| previews.read(cx).pending() == 0)
    });
    let zebra = CrosshairRef::Library(zebra.id().clone());
    assert!(matches!(
        cx.update(|cx| previews.read(cx).preview(&zebra, cx)),
        Preview::Ready(_)
    ));
}

#[gpui_kit::test]
fn a_revisit_paints_cached_previews_without_new_decodes(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_crosshair(&s.services, &env, "Zebra Dot");
    let _page = open_crosshairs(cx, &s);
    // The load requested the decode; leaving may drop the page first, and the cache still fills.
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    let previews = cx.update(CrosshairPreviews::global);
    support::wait_real(cx, "the decode", |cx| {
        cx.update(|cx| previews.read(cx).pending() == 0)
    });
    let _page = open_crosshairs(cx, &s);
    support::settle(cx, s.win);
    assert_eq!(cx.update(|cx| previews.read(cx).pending()), 0);
    assert_eq!(cx.update(|cx| previews.read(cx).decodes()), 1);
}

fn background(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<CrosshairsPage>,
    name: &'static str,
) {
    let select = cx.update(|cx| page.read(cx).background_select().clone());
    cx.update_window(s.win, |_, window, cx| {
        select.update(cx, |st, cx| st.set_selected_value(&name, window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn the_preview_waits_for_a_selection(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_crosshairs(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIRS_PREVIEW).as_deref(),
        Some("No crosshair")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIRS_BACKGROUND).as_deref(),
        Some("Dark")
    );
}

#[gpui_kit::test]
fn the_preview_shows_the_selection_over_the_background(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_crosshairs(cx, &s);
    let first = s.services.builtins.all()[0].clone();
    support::click_id(cx, s.win, ids::crosshair_builtin(first.name()));
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIRS_PREVIEW),
        Some(format!("{} on Dark", first.display_name()))
    );
    background(cx, &s, &page, "badlands");
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIRS_PREVIEW),
        Some(format!("{} on badlands", first.display_name()))
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIRS_BACKGROUND).as_deref(),
        Some("badlands")
    );
}

#[gpui_kit::test]
fn import_adds_the_picked_png(cx: &mut TestAppContext) {
    let env = support::env();
    let path = env.tmp.path().join("Ring.png");
    std::fs::write(&path, support::crosshair_png([255, 0, 0, 255])).unwrap();
    let picker = Arc::new(ScriptedPicker::new([path]));
    let s = support::shell_window_with(
        cx,
        &env,
        found(&env),
        Arc::clone(&picker),
        support::no_steam(),
    );
    let page = open_crosshairs(cx, &s);
    support::click_id(cx, s.win, ids::CROSSHAIRS_IMPORT);
    support::wait_real(cx, "the toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::TOAST).as_deref(),
        Some("Crosshair imported. Ring added to your library")
    );
    assert_eq!(picker.asked(), vec![PickKind::CrosshairImage]);
    let entries = s.services.library.list_crosshairs().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name(), "Ring");
    search(cx, &s, &page, "ring");
    assert!(support::present(
        cx,
        s.win,
        ids::crosshair_library(entries[0].folder_name())
    ));

    // The reload after the import queues the new crosshair; its preview must finish Ready.
    let previews = cx.update(CrosshairPreviews::global);
    let ring = CrosshairRef::Library(entries[0].id().clone());
    support::wait_real(cx, "the new preview", |cx| {
        cx.update(|cx| {
            let p = previews.read(cx);
            p.pending() == 0 && !matches!(p.preview(&ring, cx), Preview::Pending)
        })
    });
    assert!(matches!(
        cx.update(|cx| previews.read(cx).preview(&ring, cx)),
        Preview::Ready(_)
    ));
}

#[gpui_kit::test]
fn an_import_whose_name_is_taken_keeps_both_and_names_the_new_one(cx: &mut TestAppContext) {
    let env = support::env();
    let path = env.tmp.path().join("picked/Ring.png");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, support::crosshair_png([255, 0, 0, 255])).unwrap();
    let picker = Arc::new(ScriptedPicker::new([path]));
    let s = support::shell_window_with(cx, &env, found(&env), picker, support::no_steam());
    let first = support::seed_crosshair(&s.services, &env, "Ring");
    open_crosshairs(cx, &s);
    support::click_id(cx, s.win, ids::CROSSHAIRS_IMPORT);
    support::wait_real(cx, "the toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::TOAST).as_deref(),
        Some("Crosshair imported. Added to your library as Ring (2)")
    );
    let mut names: Vec<_> = s
        .services
        .library
        .list_crosshairs()
        .unwrap()
        .iter()
        .map(|c| (c.id().clone(), c.name().to_owned()))
        .collect();
    names.sort_by(|a, b| a.1.cmp(&b.1));
    assert_eq!(names.len(), 2);
    assert_eq!(names[0], (first.id().clone(), "Ring".to_owned()));
    assert_eq!(names[1].1, "Ring (2)");
}

#[gpui_kit::test]
fn a_failed_load_records_no_timings(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    // Without its table the list query fails as a damaged library would.
    let conn = rusqlite::Connection::open(env.data.db_path()).unwrap();
    conn.execute_batch("DROP TABLE library_crosshairs").unwrap();
    drop(conn);
    let page = open_crosshairs(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::TOAST).as_deref(),
        Some(
            "Failed to load crosshairs. TF2Tools couldn't open your library. Restart TF2Tools and try again."
        )
    );
    let timings = cx.update(|cx| page.read(cx).timings());
    assert_eq!(timings.loaded, None);
    assert_eq!(timings.previews_ready, None);
}

#[gpui_kit::test]
fn import_of_a_wrong_file_toasts_the_error(cx: &mut TestAppContext) {
    let env = support::env();
    let path = env.tmp.path().join("notes.txt");
    std::fs::write(&path, b"not an image").unwrap();
    let picker = Arc::new(ScriptedPicker::new([path]));
    let s = support::shell_window_with(cx, &env, found(&env), picker, support::no_steam());
    let _page = open_crosshairs(cx, &s);
    support::click_id(cx, s.win, ids::CROSSHAIRS_IMPORT);
    support::wait_real(cx, "the toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    let toast = support::label_of(cx, s.win, ids::TOAST).unwrap();
    assert!(toast.starts_with("Failed to import crosshair. "), "{toast}");
    support::assert_plain(&toast);
    assert!(s.services.library.list_crosshairs().unwrap().is_empty());
}

#[gpui_kit::test]
fn a_cancelled_import_changes_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let picker = Arc::new(ScriptedPicker::empty());
    let s = support::shell_window_with(
        cx,
        &env,
        found(&env),
        Arc::clone(&picker),
        support::no_steam(),
    );
    let _page = open_crosshairs(cx, &s);
    support::click_id(cx, s.win, ids::CROSSHAIRS_IMPORT);
    support::settle(cx, s.win);
    assert_eq!(picker.asked(), vec![PickKind::CrosshairImage]);
    assert!(!support::present(cx, s.win, ids::TOAST));
    assert!(s.services.library.list_crosshairs().unwrap().is_empty());
}

fn tf2(env: &support::Env) -> Tf2Install {
    Tf2Install::from_path(&env.game).unwrap()
}

fn managed(env: &support::Env) -> std::path::PathBuf {
    env.game
        .join("tf")
        .join("custom")
        .join("tf2tools-crosshairs")
}

fn toast_text(cx: &mut TestAppContext, s: &support::ShellWindow) -> String {
    support::wait_real(cx, "the toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    support::label_of(cx, s.win, ids::TOAST).unwrap()
}

// Sets the shared TF2 state, as Settings or a recheck would.
fn set_tf2(cx: &mut TestAppContext, s: &support::ShellWindow, state: Tf2State) {
    cx.update(|cx| {
        s.tf2.update(cx, |tf2, cx| {
            *tf2 = state;
            cx.notify();
        })
    });
    support::settle(cx, s.win);
}

#[gpui_kit::test]
fn without_tf2_the_page_warns_and_shuts_installs(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let _page = open_crosshairs(cx, &s);
    assert!(support::present(cx, s.win, ids::CROSSHAIRS_NO_TF2));
    assert!(!support::present(cx, s.win, ids::CROSSHAIRS_INSTALL));
    assert!(support::disabled_of(cx, s.win, ids::CROSSHAIRS_UNINSTALL));
    assert!(!support::disabled_of(cx, s.win, ids::CROSSHAIRS_IMPORT));
    support::click_id(cx, s.win, ids::CROSSHAIRS_NO_TF2_SETTINGS);
    assert!(support::present(cx, s.win, ids::page(Page::Settings)));
    assert!(!managed(&env).exists());
}

#[gpui_kit::test]
fn finding_tf2_clears_the_warning_and_opens_installs(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let _page = open_crosshairs(cx, &s);
    assert!(support::present(cx, s.win, ids::CROSSHAIRS_NO_TF2));
    set_tf2(cx, &s, found(&env));
    assert!(!support::present(cx, s.win, ids::CROSSHAIRS_NO_TF2));
    assert!(!support::disabled_of(cx, s.win, ids::CROSSHAIRS_UNINSTALL));
}

#[gpui_kit::test]
fn uninstall_removes_the_folder_and_keeps_the_assignments(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let first = s.services.builtins.all()[0].name().to_owned();
    s.services
        .library
        .assign("tf_weapon_scattergun", &CrosshairRef::Builtin(first))
        .unwrap();
    s.services
        .library
        .apply_crosshairs(&tf2(&env), &s.services.builtins)
        .unwrap();
    assert!(managed(&env).exists());
    let _page = open_crosshairs(cx, &s);
    support::click_id(cx, s.win, ids::CROSSHAIRS_UNINSTALL);
    assert_eq!(toast_text(cx, &s), "Crosshairs uninstalled from TF2");
    assert!(!managed(&env).exists());
    assert_eq!(s.services.library.assignments().unwrap().len(), 1);
}

#[gpui_kit::test]
fn rival_weapon_scripts_show_in_the_conflicts_box(cx: &mut TestAppContext) {
    let env = support::env();
    let scripts = env.game.join("tf/custom/rival/scripts");
    std::fs::create_dir_all(&scripts).unwrap();
    std::fs::write(scripts.join("tf_weapon_bat.txt"), "x").unwrap();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_crosshairs(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIRS_CONFLICTS).as_deref(),
        Some("Conflicts Detected")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::crosshair_conflict("rival")).as_deref(),
        Some("rival: Crosshair scripts found in 'rival' — may override your assignments")
    );
}

#[gpui_kit::test]
fn no_rivals_no_conflicts_box(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_crosshairs(cx, &s);
    assert!(!support::present(cx, s.win, ids::CROSSHAIRS_CONFLICTS));
}

fn of_class(class: TfClass) -> Vec<&'static Weapon> {
    weapons().iter().filter(|w| w.class() == class).collect()
}

fn select_first_builtin(cx: &mut TestAppContext, s: &support::ShellWindow) -> String {
    let first = s.services.builtins.all()[0].clone();
    support::click_id(cx, s.win, ids::crosshair_builtin(first.name()));
    first.name().to_owned()
}

#[gpui_kit::test]
fn copy_and_export_wait_for_an_import_or_uninstall(cx: &mut TestAppContext) {
    let env = support::env();
    let path = env.tmp.path().join("Ring.png");
    std::fs::write(&path, support::crosshair_png([255, 0, 0, 255])).unwrap();
    let picker = Arc::new(ScriptedPicker::new([path]));
    let s = support::shell_window_with(cx, &env, found(&env), picker, support::no_steam());
    let _page = open_crosshairs(cx, &s);
    select_first_builtin(cx, &s);
    assert!(!support::disabled_of(
        cx,
        s.win,
        ids::CROSSHAIRS_COPY_TO_MAKER
    ));
    for action in [ids::CROSSHAIRS_IMPORT, ids::CROSSHAIRS_UNINSTALL] {
        // Click and draw before the executor runs the job, so the frame shows it running.
        let disabled = cx
            .update_window(s.win, |_, w, cx| {
                w.click(action, cx);
                w.render_frame(cx);
                [ids::CROSSHAIRS_COPY_TO_MAKER, ids::CROSSHAIRS_EXPORT_PNG]
                    .map(|id| support::button_disabled(w, id))
            })
            .unwrap();
        assert_eq!(
            disabled,
            [true, true],
            "Copy and Export while {action} runs"
        );
        support::wait_real(cx, "the action", |cx| {
            !support::disabled_of(cx, s.win, ids::CROSSHAIRS_COPY_TO_MAKER)
        });
    }
}

/// Opens the per-weapon list of `weapon`'s board cell.
fn open_weapons(cx: &mut TestAppContext, s: &support::ShellWindow, weapon: &Weapon) {
    support::click_id(
        cx,
        s.win,
        ids::loadout_cell_menu(weapon.class(), weapon.slot()),
    );
}

#[gpui_kit::test]
fn the_per_weapon_list_names_each_weapon_of_the_cell(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_crosshairs(cx, &s);
    assert_eq!(cx.update(|cx| page.read(cx).class()), TfClass::Scout);
    let first = of_class(TfClass::Scout)[0];
    open_weapons(cx, &s, first);
    for weapon in of_class(TfClass::Scout)
        .into_iter()
        .filter(|w| w.slot() == first.slot())
    {
        let script = weapon.script_name();
        assert_eq!(
            support::label_of(cx, s.win, ids::weapon_row(script)).as_deref(),
            Some(weapon.display_name())
        );
        assert_eq!(
            support::label_of(cx, s.win, ids::weapon_assigned(script)).as_deref(),
            Some("None")
        );
        assert!(!support::present(cx, s.win, ids::weapon_check(script)));
    }
    // The board replaces the class tabs and bulk buttons.
    assert!(!support::present(cx, s.win, ids::class_tab("scout")));
    assert!(!support::present(cx, s.win, ids::CROSSHAIRS_BULK_ALL));
}

#[gpui_kit::test]
fn set_class_switches_the_per_weapon_list(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_crosshairs(cx, &s);
    cx.update(|cx| page.update(cx, |page, cx| page.set_class(TfClass::Soldier, cx)));
    support::settle(cx, s.win);
    assert_eq!(cx.update(|cx| page.read(cx).class()), TfClass::Soldier);
    let soldier = of_class(TfClass::Soldier)
        .into_iter()
        .find(|w| w.slot() == WeaponSlot::Primary)
        .unwrap()
        .script_name();
    let scout = of_class(TfClass::Scout)[0].script_name();
    assert!(support::present(cx, s.win, ids::weapon_row(soldier)));
    assert!(!support::present(cx, s.win, ids::weapon_row(scout)));
}

#[gpui_kit::test]
fn clicking_a_row_assigns_the_selected_crosshair(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_crosshairs(cx, &s);
    let name = select_first_builtin(cx, &s);
    let weapon = of_class(TfClass::Scout)[0];
    open_weapons(cx, &s, weapon);
    support::click_id(cx, s.win, ids::weapon_row(weapon.script_name()));
    support::wait_real(cx, "the save", |_| {
        s.services.library.assignments().unwrap().len() == 1
    });
    let assignments = s.services.library.assignments().unwrap();
    assert_eq!(assignments.len(), 1);
    assert_eq!(assignments[0].weapon_script(), weapon.script_name());
    assert_eq!(
        assignments[0].crosshair(),
        &CrosshairRef::Builtin(name.clone())
    );
    let display = s
        .services
        .builtins
        .get(&name)
        .unwrap()
        .display_name()
        .to_owned();
    support::wait_real(cx, "the reload", |cx| {
        support::label_of(cx, s.win, ids::weapon_assigned(weapon.script_name())).as_deref()
            == Some(display.as_str())
    });
    assert!(support::present(
        cx,
        s.win,
        ids::weapon_check(weapon.script_name())
    ));
}

#[gpui_kit::test]
fn a_row_does_nothing_without_a_selection(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_crosshairs(cx, &s);
    let weapon = of_class(TfClass::Scout)[0];
    open_weapons(cx, &s, weapon);
    support::click_id(cx, s.win, ids::weapon_row(weapon.script_name()));
    support::settle(cx, s.win);
    assert!(s.services.library.assignments().unwrap().is_empty());
    assert!(!support::present(cx, s.win, ids::TOAST));
}

#[gpui_kit::test]
fn a_library_assignment_shows_the_crosshair_name(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let zebra = support::seed_crosshair(&s.services, &env, "Zebra Dot");
    let weapon = of_class(TfClass::Scout)[0];
    s.services
        .library
        .assign(
            weapon.script_name(),
            &CrosshairRef::Library(zebra.id().clone()),
        )
        .unwrap();
    let _page = open_crosshairs(cx, &s);
    open_weapons(cx, &s, weapon);
    assert_eq!(
        support::label_of(cx, s.win, ids::weapon_assigned(weapon.script_name())).as_deref(),
        Some("Zebra Dot")
    );
}

#[gpui_kit::test]
fn an_unknown_builtin_assignment_shows_its_raw_name(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let weapon = of_class(TfClass::Scout)[0];
    s.services
        .library
        .assign(weapon.script_name(), &CrosshairRef::Builtin("nope".into()))
        .unwrap();
    let _page = open_crosshairs(cx, &s);
    open_weapons(cx, &s, weapon);
    assert_eq!(
        support::label_of(cx, s.win, ids::weapon_assigned(weapon.script_name())).as_deref(),
        Some("nope")
    );
}

// The gallery draws each crosshair at its in-game size on a tile, so tiles are large.
#[gpui_kit::test]
fn gallery_tiles_are_large_enough_for_the_in_game_size(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_crosshairs(cx, &s);
    let first = s.services.builtins.all()[0].name().to_owned();
    let tile = support::bounds_of(cx, s.win, ids::crosshair_builtin(&first)).unwrap();
    assert!(
        tile.size.width >= gpui_kit::px(80.) && tile.size.height >= gpui_kit::px(80.),
        "tile {tile:?}"
    );
}

#[gpui_kit::test]
fn crosshair_operations_write_v5s_rows(cx: &mut TestAppContext) {
    let env = support::env();
    let path = env.tmp.path().join("Ring.png");
    std::fs::write(&path, support::crosshair_png([255, 0, 0, 255])).unwrap();
    let picker = Arc::new(ScriptedPicker::new([path.clone()]));
    let s = support::shell_window_with(cx, &env, found(&env), picker, support::no_steam());
    let first = s.services.builtins.all()[0].name().to_owned();
    s.services
        .library
        .assign("tf_weapon_scattergun", &CrosshairRef::Builtin(first))
        .unwrap();
    let _page = open_crosshairs(cx, &s);
    // Each step waits for its row, then leaves and reopens Crosshairs, so the next click lands
    // on a fresh, idle page.
    let rows_then_revisit = |cx: &mut TestAppContext, n: usize| {
        support::wait_real(cx, "the row", |_| support::log_rows(&s.services).len() == n);
        support::click_id(cx, s.win, ids::nav(Page::Settings));
        open_crosshairs(cx, &s);
    };
    support::click_id(cx, s.win, ids::CROSSHAIRS_IMPORT);
    rows_then_revisit(cx, 1);
    // A per-weapon pick saves into TF2 on its own, which logs the install.
    select_first_builtin(cx, &s);
    let scattergun = weapons()
        .iter()
        .find(|w| w.script_name() == "tf_weapon_scattergun")
        .unwrap();
    open_weapons(cx, &s, scattergun);
    support::click_id(cx, s.win, ids::weapon_row("tf_weapon_scattergun"));
    rows_then_revisit(cx, 2);
    support::click_id(cx, s.win, ids::CROSSHAIRS_UNINSTALL);
    support::wait_real(cx, "the row", |_| support::log_rows(&s.services).len() == 3);
    let rows = support::log_rows(&s.services);
    let got: Vec<_> = rows
        .iter()
        .rev()
        .map(|e| (e.category(), e.action(), e.message()))
        .collect();
    assert_eq!(
        got,
        [
            ("crosshair", "import", "Imported crosshair: Ring"),
            ("crosshair", "install", "Installed 1 crosshair assignments"),
            (
                "crosshair",
                "uninstall",
                "Uninstalled crosshair assignments"
            ),
        ]
    );
    // File paths are developer detail, kept out of the player's log.
    assert_eq!(rows[2].details(), None);
    assert_eq!(rows[1].details(), None);
}

#[gpui_kit::test]
fn a_library_change_reloads_the_open_page(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_crosshairs(cx, &s);
    let zebra = support::seed_crosshair(&s.services, &env, "Zebra Dot");
    cx.update(tf2tools_app::library_events::LibraryEvents::changed);
    support::wait_real(cx, "the reload", |cx| {
        search(cx, &s, &page, "zebra");
        support::present(cx, s.win, ids::crosshair_library(zebra.folder_name()))
    });
}

#[gpui_kit::test]
fn a_save_that_ends_after_leaving_still_reaches_the_game(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let page = open_crosshairs(cx, &s);
    select_first_builtin(cx, &s);
    // Assign and leave in one update, so the page is gone when the apply runs.
    cx.update_window(s.win, |_, window, cx| {
        window.click(ids::loadout_cell(TfClass::Scout, WeaponSlot::Primary), cx);
        s.shell
            .update(cx, |shell, cx| shell.navigate(Page::Huds, cx));
    })
    .unwrap();
    drop(page);
    support::wait_real(cx, "the apply", |_| {
        managed(&env)
            .join("scripts/tf_weapon_scattergun.txt")
            .is_file()
    });
    assert!(cx.update(|cx| s.shell.read(cx).crosshairs_page()).is_none());
}

/// Finds `entry` through the search box, since the list builds only its visible rows, and
/// selects it.
fn select_library(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<CrosshairsPage>,
    entry: &CrosshairEntry,
) {
    search(cx, s, page, entry.name());
    support::click_id(cx, s.win, ids::crosshair_library(entry.folder_name()));
    search(cx, s, page, "");
}

/// Puts `entry` on `scripts` and the first builtin on the pistol, then installs.
fn install_with_builtin(
    env: &support::Env,
    s: &support::ShellWindow,
    entry: &CrosshairEntry,
    scripts: &[&str],
) {
    let library = &s.services.library;
    let own = CrosshairRef::Library(entry.id().clone());
    library.assign_many(scripts, &own).unwrap();
    let first = s.services.builtins.all()[0].name().to_owned();
    library
        .assign("tf_weapon_pistol", &CrosshairRef::Builtin(first))
        .unwrap();
    library
        .apply_crosshairs(&tf2(env), &s.services.builtins)
        .unwrap();
}

fn confirm_text(cx: &mut TestAppContext, s: &support::ShellWindow) -> String {
    support::click_id(cx, s.win, ids::CROSSHAIRS_DELETE);
    support::label_of(cx, s.win, ids::CONFIRM_DESCRIPTION).unwrap()
}

#[gpui_kit::test]
fn delete_asks_then_removes_the_crosshair(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let zebra = support::seed_crosshair(&s.services, &env, "Zebra");
    let page = open_crosshairs(cx, &s);
    select_library(cx, &s, &page, &zebra);
    assert_eq!(
        confirm_text(cx, &s),
        "Are you sure you want to delete Zebra from your library? This cannot be undone."
    );
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    assert_eq!(
        toast_text(cx, &s),
        "Crosshair removed. Zebra has been deleted from your library"
    );
    assert!(s.services.library.list_crosshairs().unwrap().is_empty());
    let folder = env.data.library_dir(LibraryKind::Crosshairs);
    assert!(!folder.join(zebra.folder_name()).exists());
    assert!(!support::present(
        cx,
        s.win,
        ids::crosshair_library(zebra.folder_name())
    ));
    assert_eq!(cx.update(|cx| page.read(cx).selected().cloned()), None);
    assert!(!support::present(cx, s.win, ids::CROSSHAIRS_DELETE));
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIRS_PREVIEW).as_deref(),
        Some("No crosshair")
    );
}

#[gpui_kit::test]
fn cancelling_delete_keeps_the_crosshair(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let zebra = support::seed_crosshair(&s.services, &env, "Zebra");
    let page = open_crosshairs(cx, &s);
    select_library(cx, &s, &page, &zebra);
    support::click_id(cx, s.win, ids::CROSSHAIRS_DELETE);
    support::click_id(cx, s.win, ids::CONFIRM_CANCEL);
    support::settle(cx, s.win);
    assert_eq!(s.services.library.list_crosshairs().unwrap().len(), 1);
    assert!(cx.update(|cx| page.read(cx).selected().is_some()));
    assert!(!support::present(cx, s.win, ids::TOAST));
}

#[gpui_kit::test]
fn only_library_crosshairs_can_be_deleted(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let zebra = support::seed_crosshair(&s.services, &env, "Zebra");
    let page = open_crosshairs(cx, &s);
    assert!(!support::present(cx, s.win, ids::CROSSHAIRS_DELETE));
    select_first_builtin(cx, &s);
    assert!(!support::present(cx, s.win, ids::CROSSHAIRS_DELETE));
    select_library(cx, &s, &page, &zebra);
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIRS_DELETE).as_deref(),
        Some("Delete")
    );
}

#[gpui_kit::test]
fn delete_waits_for_a_running_removal(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let zebra = support::seed_crosshair(&s.services, &env, "Zebra");
    let page = open_crosshairs(cx, &s);
    select_library(cx, &s, &page, &zebra);
    // Click and draw before the executor runs the job, so the frame shows it running.
    let disabled = cx
        .update_window(s.win, |_, w, cx| {
            w.click(ids::CROSSHAIRS_UNINSTALL, cx);
            w.render_frame(cx);
            support::button_disabled(w, ids::CROSSHAIRS_DELETE)
        })
        .unwrap();
    assert!(disabled);
    support::wait_real(cx, "the removal", |cx| {
        !support::disabled_of(cx, s.win, ids::CROSSHAIRS_DELETE)
    });
}

#[gpui_kit::test]
fn deleting_an_installed_crosshair_frees_its_weapons_and_game_files(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let zebra = support::seed_crosshair(&s.services, &env, "Zebra");
    let scripts = [
        "tf_weapon_scattergun",
        "tf_weapon_rocketlauncher",
        "tf_weapon_minigun",
    ];
    install_with_builtin(&env, &s, &zebra, &scripts);
    let page = open_crosshairs(cx, &s);
    select_library(cx, &s, &page, &zebra);
    assert_eq!(
        confirm_text(cx, &s),
        "Are you sure you want to delete Zebra from your library? It is used by 3 weapons. They go back to the default crosshair. It is also removed from TF2. This cannot be undone."
    );
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    toast_text(cx, &s);
    let left = s.services.library.assignments().unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].weapon_script(), "tf_weapon_pistol");
    let thumbnails = managed(&env).join("materials/vgui/replay/thumbnails");
    assert!(
        !thumbnails
            .join(format!("{}.vtf", zebra.folder_name()))
            .exists()
    );
    assert_eq!(
        std::fs::read_dir(managed(&env).join("scripts"))
            .unwrap()
            .count(),
        1
    );
}

#[gpui_kit::test]
fn deleting_a_crosshair_that_is_not_installed_touches_no_game_files(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let zebra = support::seed_crosshair(&s.services, &env, "Zebra");
    let own = CrosshairRef::Library(zebra.id().clone());
    s.services
        .library
        .assign("tf_weapon_scattergun", &own)
        .unwrap();
    let page = open_crosshairs(cx, &s);
    select_library(cx, &s, &page, &zebra);
    assert_eq!(
        confirm_text(cx, &s),
        "Are you sure you want to delete Zebra from your library? It is used by 1 weapon. It goes back to the default crosshair. This cannot be undone."
    );
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    toast_text(cx, &s);
    assert!(s.services.library.assignments().unwrap().is_empty());
    assert!(!managed(&env).exists());
}

#[gpui_kit::test]
fn delete_works_without_tf2(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let zebra = support::seed_crosshair(&s.services, &env, "Zebra");
    let page = open_crosshairs(cx, &s);
    select_library(cx, &s, &page, &zebra);
    support::click_id(cx, s.win, ids::CROSSHAIRS_DELETE);
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    toast_text(cx, &s);
    assert!(s.services.library.list_crosshairs().unwrap().is_empty());
}

// It locks a folder with Unix permissions, which Windows does not have.
#[cfg(unix)]
#[gpui_kit::test]
fn a_failed_delete_toasts_and_keeps_the_crosshair(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt as _;

    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let zebra = support::seed_crosshair(&s.services, &env, "Zebra");
    install_with_builtin(&env, &s, &zebra, &["tf_weapon_scattergun"]);
    let page = open_crosshairs(cx, &s);
    select_library(cx, &s, &page, &zebra);
    support::click_id(cx, s.win, ids::CROSSHAIRS_DELETE);

    let scripts = managed(&env).join("scripts");
    let mode = |m| std::fs::set_permissions(&scripts, std::fs::Permissions::from_mode(m)).unwrap();
    mode(0o555);
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    let text = toast_text(cx, &s);
    mode(0o755);
    assert_eq!(
        text,
        "Failed to delete crosshair. Couldn't change your TF2 folder. Make sure TF2 is closed and TF2Tools can write to it, then try again."
    );
    support::assert_plain(&text);
    assert_eq!(s.services.library.list_crosshairs().unwrap().len(), 1);
    assert_eq!(s.services.library.assignments().unwrap().len(), 2);
    assert!(cx.update(|cx| page.read(cx).selected().is_some()));
}

#[gpui_kit::test]
fn the_prompt_is_silent_about_tf2_for_a_crosshair_not_in_the_game(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let zebra = support::seed_crosshair(&s.services, &env, "Zebra");
    let yak = support::seed_crosshair(&s.services, &env, "Yak");
    install_with_builtin(&env, &s, &yak, &["tf_weapon_scattergun"]);
    // Assigned after the install, so the managed folder exists without Zebra in it.
    let own = CrosshairRef::Library(zebra.id().clone());
    s.services
        .library
        .assign("tf_weapon_minigun", &own)
        .unwrap();
    let page = open_crosshairs(cx, &s);
    select_library(cx, &s, &page, &zebra);
    assert_eq!(
        confirm_text(cx, &s),
        "Are you sure you want to delete Zebra from your library? It is used by 1 weapon. It goes back to the default crosshair. This cannot be undone."
    );
}

#[gpui_kit::test]
fn the_prompt_names_tf2_for_a_reassigned_crosshair_still_in_the_game(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let zebra = support::seed_crosshair(&s.services, &env, "Zebra");
    install_with_builtin(&env, &s, &zebra, &["tf_weapon_scattergun"]);
    let first = s.services.builtins.all()[0].name().to_owned();
    s.services
        .library
        .assign("tf_weapon_scattergun", &CrosshairRef::Builtin(first))
        .unwrap();
    let page = open_crosshairs(cx, &s);
    select_library(cx, &s, &page, &zebra);
    assert_eq!(
        confirm_text(cx, &s),
        "Are you sure you want to delete Zebra from your library? It is also removed from TF2. This cannot be undone."
    );
}

#[gpui_kit::test]
fn a_delete_writes_an_activity_row(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let zebra = support::seed_crosshair(&s.services, &env, "Zebra");
    let page = open_crosshairs(cx, &s);
    select_library(cx, &s, &page, &zebra);
    support::click_id(cx, s.win, ids::CROSSHAIRS_DELETE);
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    support::wait_real(cx, "the delete row", |_| {
        support::log_rows(&s.services)
            .iter()
            .any(|r| r.action() == "delete")
    });
    let rows = support::log_rows(&s.services);
    let row = rows.iter().find(|r| r.action() == "delete").unwrap();
    assert_eq!(row.message(), "Deleted crosshair from library: Zebra");
}

// The game draws every crosshair 64 px square at the default crosshair scale, whatever the
// resolution, and the maker's preview uses the same size.
#[gpui_kit::test]
fn the_preview_draws_the_crosshair_at_its_in_game_size(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_crosshairs(cx, &s);
    let first = s.services.builtins.all()[0].clone();
    support::click_id(cx, s.win, ids::crosshair_builtin(first.name()));
    support::wait_real(cx, "the preview", |cx| {
        support::present(cx, s.win, ids::CROSSHAIRS_PREVIEW_CROSSHAIR)
    });
    let shown = support::bounds_of(cx, s.win, ids::CROSSHAIRS_PREVIEW_CROSSHAIR).unwrap();
    assert_eq!(
        shown.size,
        gpui_kit::size(gpui_kit::px(64.), gpui_kit::px(64.))
    );
}
