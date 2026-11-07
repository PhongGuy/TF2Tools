//! Borrowing a part from another library HUD in the HUD Maker, headless.

use crate::support;

use std::collections::BTreeSet;

use gpui_kit::{AppContext as _, Entity, TestAppContext};
use support::maker_page::scroll_panel_to;
use support::scroll::{
    assert_bar_on_right_edge, assert_clear_of_bar, assert_track_scrolls_virtual,
};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::HudMakerPage;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{
    ChangeTarget, GbId, HudId, LibraryKind, PartKey, Provenance, RgbaImage, StartingPoint,
    Tf2Install, rgba_to_vtf,
};

const ROOT: &str = "HudPlayerStatus/HudPlayerHealth";
const VALUE: &str = "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue";
const HEALTH_RES: &str = r#""Resource/UI/HudPlayerHealth.res"
{
  HudPlayerHealth { ControlName EditablePanel xpos 0 ypos r120 wide 250 tall 120 visible 1 }
  PlayerStatusHealthValue { ControlName CExLabel xpos 90 ypos 52 wide 44 tall 18 font MineBig labelText %Health% }
}
"#;
const MINE_SCHEME: &str =
    r#""Scheme" { Fonts { MineBig { "1" { name "Mystery Sans" tall 20 } } } }"#;
const ANIMATIONS: &str = "event HealthPulse\n{\n\tAnimate PlayerStatusHealthValue FgColor \"255 0 0 255\" Linear 0.0 0.1\n}\n";

fn opened(cx: &mut TestAppContext) -> (support::Env, support::ShellWindow, Entity<HudMakerPage>) {
    let env = support::env();
    support::seed_stock_game(&env.game);
    let state = Tf2State::Found(Tf2Install::from_path(&env.game).unwrap());
    let s = support::shell_window(cx, &env, state);
    support::click_nav(cx, s.win, Page::HudMaker);
    let page = cx.update(|cx| s.shell.read(cx).hud_maker_page()).unwrap();
    support::wait_real(cx, "the HUD to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    (env, s, page)
}

/// The Maker open on the default HUD, and a library HUD `minehud` imported after the shell
/// started, with `files` (game path, text) written into its folder beside its health file.
fn opened_with_source_hud(
    cx: &mut TestAppContext,
    provenance: Provenance,
    files: &[(&str, &str)],
) -> (
    support::Env,
    support::ShellWindow,
    Entity<HudMakerPage>,
    HudId,
) {
    let (env, s, page) = opened(cx);
    let archive = env.tmp.path().join("in/minehud.zip");
    std::fs::create_dir_all(archive.parent().unwrap()).unwrap();
    std::fs::write(&archive, support::hud_zip_bytes("minehud")).unwrap();
    let entry = s.services.library.import_hud(&archive, provenance).unwrap();
    let dir = env
        .data
        .library_dir(LibraryKind::Huds)
        .join(entry.folder_name());
    let mut all = vec![("resource/ui/hudplayerhealth.res", HEALTH_RES)];
    all.extend_from_slice(files);
    for (path, text) in all {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    (env, s, page, entry.id().clone())
}

fn mia() -> Provenance {
    Provenance::GameBanana {
        gb_id: GbId::new(7),
        author: "Mia".to_owned(),
    }
}

fn start_borrow(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    id: &HudId,
    part: &str,
) {
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| {
            p.start_borrow(id.clone(), PartKey::new(part), window, cx)
        })
    })
    .unwrap();
}

fn wait_previewing(cx: &mut TestAppContext, s: &support::ShellWindow, page: &Entity<HudMakerPage>) {
    support::wait_real(cx, "the borrow preview", |cx| {
        cx.update(|cx| page.read(cx).editor().is_some_and(|e| e.previewing()))
    });
    support::settle(cx, s.win);
}

fn type_into(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    input: Entity<gpui_kit::component::input::InputState>,
    text: &str,
) {
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.set_value(text, window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
}

fn save_hud(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    name: &str,
) -> HudId {
    let input = cx.update(|cx| page.read(cx).name_input().clone());
    type_into(cx, s, input, name);
    support::click_id(cx, s.win, ids::HUD_MAKER_SAVE);
    support::wait_real(cx, "the save", |cx| {
        cx.update(|cx| !page.read(cx).is_saving() && page.read(cx).saved_hud().is_some())
    });
    support::settle(cx, s.win);
    cx.update(|cx| page.read(cx).saved_hud().cloned()).unwrap()
}

fn recolour(cx: &mut TestAppContext, page: &Entity<HudMakerPage>, colour: &str) {
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            let e = p.editor_mut().unwrap();
            e.select(Some(PartKey::new(VALUE)));
            e.recolour_selected(colour).unwrap();
            e.select(None);
            cx.notify();
        })
    });
}

#[gpui_kit::test]
fn borrowing_a_part_previews_it_on_the_canvas_and_cancel_leaves_the_hud_alone(
    cx: &mut TestAppContext,
) {
    let (_env, s, page, id) = opened_with_source_hud(cx, mia(), &[]);
    start_borrow(cx, &s, &page, &id, VALUE);
    wait_previewing(cx, &s, &page);
    let credited = cx.update(|cx| {
        let e = page.read(cx).editor().unwrap();
        e.shown_changes()
            .iter()
            .any(|c| matches!(c.target(), ChangeTarget::Credit(n) if n.author() == "Mia"))
    });
    assert!(credited, "the preview carries the credit");
    assert!(
        cx.update(|cx| page.read(cx).editor().unwrap().changes().is_empty()),
        "nothing is recorded before Apply"
    );
    support::click_id(cx, s.win, ids::HUD_MAKER_PREVIEW_CANCEL);
    support::settle(cx, s.win);
    assert!(cx.update(|cx| !page.read(cx).editor().unwrap().previewing()));
    assert!(cx.update(|cx| page.read(cx).editor().unwrap().changes().is_empty()));
}

#[gpui_kit::test]
fn applying_a_borrow_is_one_undo_step_that_keeps_earlier_changes(cx: &mut TestAppContext) {
    let (_env, s, page, id) = opened_with_source_hud(cx, mia(), &[]);
    recolour(cx, &page, "255 0 0 255");
    let before = cx.update(|cx| page.read(cx).editor().unwrap().changes().to_vec());
    assert!(!before.is_empty());
    start_borrow(cx, &s, &page, &id, ROOT);
    wait_previewing(cx, &s, &page);
    support::click_id(cx, s.win, ids::HUD_MAKER_PREVIEW_APPLY);
    support::settle(cx, s.win);
    let after = cx.update(|cx| page.read(cx).editor().unwrap().changes().to_vec());
    assert_eq!(
        &after[..before.len()],
        &before[..],
        "the earlier changes stay first and untouched"
    );
    let groups: BTreeSet<u32> = after[before.len()..].iter().map(|c| c.group()).collect();
    assert_eq!(groups.len(), 1, "the borrow is one group");
    assert!(
        after[before.len()..]
            .iter()
            .any(|c| matches!(c.target(), ChangeTarget::Credit(_)))
    );
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            assert!(p.editor_mut().unwrap().undo());
            cx.notify();
        })
    });
    assert_eq!(
        cx.update(|cx| page.read(cx).editor().unwrap().changes().to_vec()),
        before,
        "one undo removes the whole borrow"
    );
}

#[gpui_kit::test]
fn a_local_hud_is_credited_by_its_name_with_no_author(cx: &mut TestAppContext) {
    let (_env, s, page, id) = opened_with_source_hud(cx, Provenance::Local, &[]);
    start_borrow(cx, &s, &page, &id, VALUE);
    wait_previewing(cx, &s, &page);
    let note = cx.update(|cx| {
        page.read(cx)
            .editor()
            .unwrap()
            .shown_changes()
            .iter()
            .find_map(|c| match c.target() {
                ChangeTarget::Credit(n) => Some((n.hud().to_owned(), n.author().to_owned())),
                _ => None,
            })
    });
    let (hud, author) = note.expect("a credit");
    assert!(hud.to_lowercase().contains("minehud"), "{hud}");
    assert_eq!(author, "");
}

#[gpui_kit::test]
fn a_borrow_that_lands_after_another_hud_loads_is_dropped(cx: &mut TestAppContext) {
    let (_env, s, page, id) = opened_with_source_hud(cx, mia(), &[]);
    let other = save_hud(cx, &s, &page, "Other");
    start_borrow(cx, &s, &page, &id, VALUE);
    cx.update(|cx| page.update(cx, |p, cx| p.open(other.clone(), cx)));
    support::wait_real(cx, "the other HUD to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded() && page.read(cx).saved_hud() == Some(&other))
    });
    support::settle(cx, s.win);
    assert!(
        cx.update(|cx| !page.read(cx).editor().unwrap().previewing()),
        "no preview from the old HUD"
    );
}

/// Opens the section from the More menu and waits for the library list to land.
fn open_borrow(cx: &mut TestAppContext, s: &support::ShellWindow, page: &Entity<HudMakerPage>) {
    support::click_id(cx, s.win, ids::HUD_MAKER_MORE);
    support::click_id(cx, s.win, ids::HUD_MAKER_BORROW_OPEN);
    support::wait_real(cx, "the borrowable HUDs", |cx| {
        cx.update(|cx| page.read(cx).borrow_listed())
    });
    support::settle(cx, s.win);
}

/// Opens the section, chooses the source HUD and its health root, and waits for the preview.
fn choose_root(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
    id: &HudId,
) {
    open_borrow(cx, s, page);
    support::click_id(cx, s.win, ids::hud_maker_borrow_hud(id));
    support::wait_real(cx, "the parts list", |cx| {
        support::present(cx, s.win, ids::hud_maker_borrow_part(&PartKey::new(ROOT)))
    });
    support::click_id(cx, s.win, ids::hud_maker_borrow_part(&PartKey::new(ROOT)));
    wait_previewing(cx, s, page);
}

#[gpui_kit::test]
fn the_section_says_so_when_no_other_hud_has_parts_to_borrow(cx: &mut TestAppContext) {
    let (_env, s, page) = opened(cx);
    open_borrow(cx, &s, &page);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_BORROW));
    let empty = support::label_of(cx, s.win, ids::HUD_MAKER_BORROW_EMPTY).expect("the empty note");
    assert!(
        empty.contains("Download one from the HUD store first"),
        "{empty}"
    );
    support::click_id(cx, s.win, ids::HUD_MAKER_BORROW_CLOSE);
    support::settle(cx, s.win);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_BORROW));
}

#[gpui_kit::test]
fn choosing_a_hud_then_a_part_shows_the_credit_and_what_comes_with_it(cx: &mut TestAppContext) {
    let files = [("resource/clientscheme.res", MINE_SCHEME)];
    let (_env, s, page, id) = opened_with_source_hud(cx, mia(), &files);
    choose_root(cx, &s, &page, &id);
    let credit =
        support::label_of(cx, s.win, ids::HUD_MAKER_BORROW_CREDIT).expect("the credit line");
    assert!(
        credit.contains("Mia") && credit.contains("terms"),
        "{credit}"
    );
    let comes = support::label_of(cx, s.win, ids::HUD_MAKER_BORROW_COMES_WITH).expect("the list");
    assert!(
        comes.contains("Mystery Sans") && comes.contains("Left behind"),
        "{comes}"
    );
}

#[gpui_kit::test]
fn a_local_hud_credit_line_names_no_author(cx: &mut TestAppContext) {
    let (_env, s, page, id) = opened_with_source_hud(cx, Provenance::Local, &[]);
    choose_root(cx, &s, &page, &id);
    let credit =
        support::label_of(cx, s.win, ids::HUD_MAKER_BORROW_CREDIT).expect("the credit line");
    assert!(credit.starts_with("This copies work from"), "{credit}");
}

const ANIMATIONS_WITH_COMMAND: &str = "event HealthPulse\n{\n\tFireCommand 0 \"engine echo hi\"\n\tAnimate PlayerStatusHealthValue FgColor \"255 0 0 255\" Linear 0.0 0.1\n}\n";
const ANIM_MANIFEST: &str = "hudanimations_manifest { file scripts/hudanimations_tf.txt }";

#[gpui_kit::test]
fn a_part_s_animations_come_along_and_console_commands_stay_behind(cx: &mut TestAppContext) {
    let note = "run console commands";
    let (_env, s, page, id) = opened_with_source_hud(
        cx,
        mia(),
        &[
            ("scripts/hudanimations_manifest.txt", ANIM_MANIFEST),
            ("scripts/hudanimations_tf.txt", ANIMATIONS_WITH_COMMAND),
        ],
    );
    choose_root(cx, &s, &page, &id);
    let bar = support::label_of(cx, s.win, ids::HUD_MAKER_PREVIEW).expect("the preview bar");
    assert!(bar.contains(note), "{bar}");
    let comes = support::label_of(cx, s.win, ids::HUD_MAKER_BORROW_COMES_WITH).expect("the list");
    assert!(comes.contains("Animation HealthPulse"), "{comes}");
    support::click_id(cx, s.win, ids::HUD_MAKER_PREVIEW_CANCEL);
    let (_env2, s2, page2, id2) = opened_with_source_hud(
        cx,
        mia(),
        &[
            ("scripts/hudanimations_manifest.txt", ANIM_MANIFEST),
            ("scripts/hudanimations_tf.txt", ANIMATIONS),
        ],
    );
    choose_root(cx, &s2, &page2, &id2);
    let bar = support::label_of(cx, s2.win, ids::HUD_MAKER_PREVIEW).expect("the preview bar");
    assert!(!bar.contains(note), "{bar}");
    let comes = support::label_of(cx, s2.win, ids::HUD_MAKER_BORROW_COMES_WITH).expect("the list");
    assert!(comes.contains("Animation HealthPulse"), "{comes}");
}

#[gpui_kit::test]
fn applying_closes_the_section_and_the_saved_hud_lists_the_credit(cx: &mut TestAppContext) {
    let (env, s, page, id) = opened_with_source_hud(cx, mia(), &[]);
    choose_root(cx, &s, &page, &id);
    support::click_id(cx, s.win, ids::HUD_MAKER_PREVIEW_APPLY);
    support::settle(cx, s.win);
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_BORROW));
    assert!(
        support::toast_labels(cx, s.win)
            .iter()
            .any(|t| t.starts_with("Added"))
    );
    let saved = save_hud(cx, &s, &page, "Mixed");
    let folder = s
        .services
        .library
        .list_huds()
        .unwrap()
        .into_iter()
        .find(|h| h.id() == &saved)
        .unwrap()
        .folder_name()
        .to_owned();
    let credits = std::fs::read_to_string(
        env.data
            .library_dir(LibraryKind::Huds)
            .join(folder)
            .join("TF2Tools-credits.txt"),
    )
    .unwrap();
    assert!(
        credits.contains("Mia") && credits.contains("HudPlayerHealth"),
        "{credits}"
    );
}

const BADGE: &str = "HudPlayerStatus/HudPlayerHealth/MineBadge";
const BADGE_RES: &str = r#""Resource/UI/HudPlayerHealth.res"
{
  HudPlayerHealth { ControlName EditablePanel xpos 0 ypos r120 wide 250 tall 120 visible 1 }
  MineBadge { ControlName ImagePanel xpos 10 ypos 10 wide 20 tall 20 visible 1 enabled 1 image replay/thumbnails/minebadge scaleImage 1 }
}
"#;

/// Waits until no borrow is still being worked out, so every started borrow has landed.
fn wait_borrows_landed(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<HudMakerPage>,
) {
    support::wait_real(cx, "the borrows to land", |cx| {
        cx.update(|cx| !page.read(cx).borrowing())
    });
    support::settle(cx, s.win);
}

fn credited_part(cx: &mut TestAppContext, page: &Entity<HudMakerPage>) -> Option<String> {
    cx.update(|cx| {
        page.read(cx)
            .editor()?
            .shown_changes()
            .iter()
            .find_map(|c| match c.target() {
                ChangeTarget::Credit(n) => Some(n.part().to_owned()),
                _ => None,
            })
    })
}

#[gpui_kit::test]
fn a_borrowed_picture_draws_in_the_preview_after_apply_and_after_reopen(cx: &mut TestAppContext) {
    let (env, s, page, id) = opened_with_source_hud(cx, mia(), &[]);
    // The source HUD's badge: its health file, material and a 2x2 red texture.
    let dir = s
        .services
        .library
        .list_huds()
        .unwrap()
        .into_iter()
        .find(|h| h.id() == &id)
        .map(|h| {
            env.data
                .library_dir(LibraryKind::Huds)
                .join(h.folder_name())
        })
        .unwrap();
    let red = RgbaImage::from_pixels(2, 2, [255, 0, 0, 255].repeat(4)).unwrap();
    let thumbs = dir.join("materials/vgui/replay/thumbnails");
    std::fs::create_dir_all(&thumbs).unwrap();
    std::fs::write(dir.join("resource/ui/hudplayerhealth.res"), BADGE_RES).unwrap();
    std::fs::write(
        thumbs.join("minebadge.vmt"),
        "\"UnlitGeneric\"\n{\n\t\"$basetexture\" \"vgui/replay/thumbnails/minebadge\"\n}\n",
    )
    .unwrap();
    std::fs::write(thumbs.join("minebadge.vtf"), rgba_to_vtf(&red).unwrap()).unwrap();
    let cell = ids::hud_maker_image_cell(&PartKey::new(BADGE), 0);
    let drawn = |cx: &mut TestAppContext, what: &str| {
        support::wait_real(cx, what, |cx| {
            cx.update(|cx| page.read(cx).image(&PartKey::new(BADGE)).is_some())
        });
        assert!(support::present(cx, s.win, cell.clone()), "{what}");
    };
    start_borrow(cx, &s, &page, &id, ROOT);
    wait_previewing(cx, &s, &page);
    drawn(cx, "the badge in the preview");
    support::click_id(cx, s.win, ids::HUD_MAKER_PREVIEW_APPLY);
    drawn(cx, "the badge after Apply");
    let saved = save_hud(cx, &s, &page, "Badged");
    cx.update(|cx| page.update(cx, |p, cx| p.open(saved.clone(), cx)));
    support::wait_real(cx, "the reopen", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    drawn(cx, "the badge after reopening");
}

#[gpui_kit::test]
fn only_the_latest_of_two_borrows_shows(cx: &mut TestAppContext) {
    let (_env, s, page, id) = opened_with_source_hud(cx, mia(), &[]);
    start_borrow(cx, &s, &page, &id, VALUE);
    start_borrow(cx, &s, &page, &id, ROOT);
    wait_borrows_landed(cx, &s, &page);
    assert_eq!(credited_part(cx, &page).as_deref(), Some("HudPlayerHealth"));
}

#[gpui_kit::test]
fn a_borrow_that_lands_after_a_newer_recipe_preview_is_dropped(cx: &mut TestAppContext) {
    let (_env, s, page, id) = opened_with_source_hud(cx, mia(), &[]);
    start_borrow(cx, &s, &page, &id, VALUE);
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| {
            let recipe = p
                .editor()
                .unwrap()
                .pristine()
                .starting_point(StartingPoint::ALL[0]);
            p.start_preview(recipe, 0, window, cx)
        })
    })
    .unwrap();
    wait_borrows_landed(cx, &s, &page);
    let bar = support::label_of(cx, s.win, ids::HUD_MAKER_PREVIEW).expect("the preview bar");
    assert!(
        !bar.contains("a part from"),
        "the recipe preview stays: {bar}"
    );
    assert_eq!(credited_part(cx, &page), None);
}

#[gpui_kit::test]
fn a_borrow_that_lands_after_the_players_changes_moved_on_is_dropped(cx: &mut TestAppContext) {
    let (_env, s, page, id) = opened_with_source_hud(cx, mia(), &[]);
    start_borrow(cx, &s, &page, &id, VALUE);
    recolour(cx, &page, "255 0 0 255");
    let after = cx.update(|cx| page.read(cx).editor().unwrap().changes().to_vec());
    wait_borrows_landed(cx, &s, &page);
    assert!(cx.update(|cx| !page.read(cx).editor().unwrap().previewing()));
    assert_eq!(
        cx.update(|cx| page.read(cx).editor().unwrap().changes().to_vec()),
        after
    );
}

#[gpui_kit::test]
fn a_long_parts_list_shows_a_bar_clear_of_its_rows(cx: &mut TestAppContext) {
    // Thirty labels under the health panel, so the list runs past its tallest.
    let mut res = String::from("\"Resource/UI/HudPlayerHealth.res\"\n{\n");
    res.push_str("  HudPlayerHealth { ControlName EditablePanel xpos 0 ypos r120 wide 250 tall 120 visible 1 }\n");
    for ix in 0..30 {
        res.push_str(&format!(
            "  Probe{ix} {{ ControlName CExLabel xpos {ix} ypos 0 wide 10 tall 10 labelText x }}\n"
        ));
    }
    res.push_str("}\n");
    let files = [("resource/ui/hudplayerhealth.res", res.as_str())];
    let (_env, s, page, id) = opened_with_source_hud(cx, mia(), &files);
    open_borrow(cx, &s, &page);
    support::click_id(cx, s.win, ids::hud_maker_borrow_hud(&id));
    let root = ids::hud_maker_borrow_part(&PartKey::new(ROOT));
    support::wait_real(cx, "the parts list", |cx| {
        support::present(cx, s.win, root.clone())
    });
    let (list, bar) = (
        ids::HUD_MAKER_BORROW_PARTS,
        ids::HUD_MAKER_BORROW_PARTS_SCROLLBAR,
    );
    scroll_panel_to(cx, &s, list.into());
    assert_bar_on_right_edge(cx, s.win, list, bar);
    assert_clear_of_bar(cx, s.win, list, ids::HUD_MAKER_PANEL_BODY_SCROLLBAR);
    assert_clear_of_bar(cx, s.win, root.clone(), bar);
    assert_track_scrolls_virtual(cx, s.win, list, root);
}
