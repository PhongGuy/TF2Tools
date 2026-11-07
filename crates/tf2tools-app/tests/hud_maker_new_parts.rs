//! Adding a shadow, box, anchor or crosshair; deleting it; the hit flash.

use crate::support;

use gpui_kit::TestAppContext;
use support::maker_page::{VALUE, click_in_panel, editor, on_page, open_maker, open_maker_with};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::editor::{AnimEdit, AnimRefused, HudEditor};
use tf2tools_core::{NewPart, PartKey, RemoveRefused};

fn select(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &gpui_kit::Entity<tf2tools_app::pages::hud_maker::HudMakerPage>,
    key: &PartKey,
) {
    on_page(cx, s, page, |p, _, cx| {
        p.editor_mut().unwrap().select(Some(key.clone()));
        cx.notify();
    });
}

fn body(e: &HudEditor, event: &str) -> String {
    e.model().animations().event(event).unwrap().body()
}

#[test]
fn an_added_part_is_selected_and_one_undo_takes_it_away() {
    let mut e = editor();
    e.select(Some(PartKey::new(VALUE)));
    let shadow = e.add_part_selected(NewPart::Shadow).unwrap();
    assert_eq!(e.selected(), Some(&shadow));
    assert!(e.can_delete_selected());
    assert!(e.undo());
    assert!(e.model().part(&shadow).is_none());
    assert_eq!(e.selected(), None);
}

#[test]
fn delete_takes_only_added_parts_and_is_undoable() {
    let mut e = editor();
    e.select(Some(PartKey::new(VALUE)));
    assert_eq!(e.delete_selected(), Err(RemoveRefused::NotAdded));
    let anchor = e.add_part_selected(NewPart::Anchor).unwrap();
    e.delete_selected().unwrap();
    assert!(e.model().part(&anchor).is_none());
    assert!(e.undo());
    assert!(e.model().part(&anchor).is_some());
}

#[test]
fn delete_keeps_a_colour_edit_and_undo_brings_the_row_back() {
    let mut e = editor();
    let value = PartKey::new(VALUE);
    e.select(Some(value.clone()));
    let shadow = e.add_part_selected(NewPart::Shadow).unwrap();
    e.select(Some(value));
    e.edit_animation(
        "HudHealthDyingPulse",
        0,
        AnimEdit::Value("255 255 0 255".into()),
    )
    .unwrap();
    e.select(Some(shadow.clone()));
    e.add_animation_row("HudHealthDyingPulse", "Alpha").unwrap();
    assert!(body(&e, "HudHealthDyingPulse").contains("PlayerStatusHealthValueShadow"));
    e.delete_selected().unwrap();
    assert!(
        body(&e, "HudHealthDyingPulse").contains("\"255 255 0 255\""),
        "the colour edit stays"
    );
    assert!(
        !body(&e, "HudHealthDyingPulse").contains("PlayerStatusHealthValueShadow"),
        "the row goes"
    );
    assert!(e.undo());
    assert!(body(&e, "HudHealthDyingPulse").contains("PlayerStatusHealthValueShadow"));
    assert!(e.model().part(&shadow).is_some());
}

#[test]
fn the_flash_starts_in_the_part_s_own_colour_and_turns_off_alone() {
    let mut e = editor();
    e.select(Some(PartKey::new(VALUE)));
    e.set_hit_flash_selected(true).unwrap();
    assert!(e.hit_flash_selected());
    assert!(
        body(&e, "TF2ToolsHitMarker")
            .starts_with("Animate \"PlayerStatusHealthValue\" \"FgColor\" \"TanDark\"")
    );
    e.set_hit_flash_colour_selected("255 0 0 255").unwrap();
    assert!(
        body(&e, "TF2ToolsHitMarker")
            .starts_with("Animate \"PlayerStatusHealthValue\" \"FgColor\" \"255 0 0 255\"")
    );
    let cross = e.add_part_selected(NewPart::Crosshair).unwrap();
    e.set_hit_flash_selected(true).unwrap();
    e.add_animation_row("DamagedPlayer", "Alpha").unwrap();
    assert_eq!(
        e.edit_animation("DamagedPlayer", 1, AnimEdit::Delay(1.0)),
        Err(AnimRefused::Appended),
        "the flash's own lines are not editable"
    );
    e.set_hit_flash_selected(false).unwrap();
    assert!(!e.hit_flash_selected());
    assert!(
        body(&e, "DamagedPlayer").contains("Animate \"TF2ToolsCrosshair\" \"Alpha\""),
        "the player's edit stays"
    );
    assert!(
        body(&e, "DamagedPlayer").contains("RunEvent \"TF2ToolsHitMarker\""),
        "the other label still flashes"
    );
    assert!(!body(&e, "TF2ToolsHitMarker").contains(cross.as_str().rsplit('/').next().unwrap()));
    assert!(e.undo());
    assert!(e.hit_flash_selected());
}

#[gpui_kit::test]
fn the_panel_adds_parts_flashes_and_deletes(cx: &mut TestAppContext) {
    let (_env, s, page) = open_maker(cx);
    select(cx, &s, &page, &PartKey::new(VALUE));
    click_in_panel(cx, &s, ids::HUD_MAKER_HIT_FLASH.into());
    assert!(cx.update(|cx| page.read(cx).editor().unwrap().hit_flash_selected()));
    click_in_panel(cx, &s, ids::HUD_MAKER_ADD_PART.into());
    support::click_id(cx, s.win, ids::hud_maker_add_part(NewPart::Anchor));
    let anchor = cx
        .update(|cx| page.read(cx).editor().unwrap().selected().cloned())
        .unwrap();
    assert!(support::present(cx, s.win, ids::hud_maker_anchor(&anchor)));
    click_in_panel(cx, &s, ids::HUD_MAKER_DELETE_PART.into());
    assert!(!support::present(cx, s.win, ids::hud_maker_anchor(&anchor)));
}

#[gpui_kit::test]
fn a_flash_the_game_commands_lock_says_so_and_changes_nothing(cx: &mut TestAppContext) {
    let anims = "event DamagedPlayer { FireCommand 0 \"engine echo hit\" }";
    let (_env, s, page) = open_maker_with(cx, anims);
    select(cx, &s, &page, &PartKey::new(VALUE));
    click_in_panel(cx, &s, ids::HUD_MAKER_HIT_FLASH.into());
    assert!(
        support::toast_labels(cx, s.win)
            .iter()
            .any(|t| t == "This animation runs game commands, so it can't be changed here.")
    );
    let e = |cx: &mut TestAppContext| {
        cx.update(|cx| {
            (
                page.read(cx).editor().unwrap().changes().len(),
                page.read(cx).editor().unwrap().hit_flash_selected(),
            )
        })
    };
    assert_eq!(e(cx), (0, false));
}
