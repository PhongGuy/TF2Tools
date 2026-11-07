//! Showing and editing the events that animate a part.

use crate::support;

use gpui_kit::TestAppContext;
use support::maker_page::{VALUE, click_in_panel, editor, open_maker};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_maker::animations::anim_sentence;
use tf2tools_app::pages::hud_maker::editor::{AnimEdit, AnimRefused};
use tf2tools_core::{AnimCommand, ChangeTarget, Interp, PartKey};

fn body(e: &tf2tools_app::pages::hud_maker::editor::HudEditor, event: &str) -> String {
    e.model().animations().event(event).unwrap().body()
}

#[test]
fn sentences_read_as_player_words() {
    let fade = AnimCommand::Animate {
        panel: "P".into(),
        field: "Alpha".into(),
        value: "0".into(),
        interp: Interp::Linear,
        start: 0.2,
        duration: 0.2,
    };
    assert_eq!(
        anim_sentence(&fade),
        "Fades to 0 over 0.2 s, starting at 0.2 s"
    );
    let run = AnimCommand::RunEvent {
        event: "Loop".into(),
        delay: 0.25,
    };
    assert_eq!(anim_sentence(&run), "Runs Loop at 0.25 s");
}

#[test]
fn each_edit_is_one_undo_step_holding_the_whole_event() {
    let mut e = editor();
    e.select(Some(PartKey::new(VALUE)));
    let names: Vec<String> = e
        .animations_selected()
        .iter()
        .map(|a| a.name().to_owned())
        .collect();
    assert_eq!(names, ["HudHealthDyingPulse", "TestNudge", "Risky"]);
    e.edit_animation(
        "HudHealthDyingPulse",
        0,
        AnimEdit::Value("255 255 0 255".into()),
    )
    .unwrap();
    e.edit_animation("HudHealthDyingPulse", 1, AnimEdit::Curve(Interp::Spline))
        .unwrap();
    assert_eq!(e.changes().len(), 2);
    assert!(
        matches!(e.changes()[1].target(), ChangeTarget::Animation(a) if a.event() == "HudHealthDyingPulse")
    );
    assert!(body(&e, "HudHealthDyingPulse").contains("\"FgColor\" \"255 255 0 255\" Linear 0 0.1"));
    assert!(body(&e, "HudHealthDyingPulse").contains("\"FgColor\" \"TanDark\" Spline 0.1 0.1"));
    assert!(e.undo());
    assert!(body(&e, "HudHealthDyingPulse").contains("Accel"));
}

#[test]
fn rows_add_and_delete_and_bad_edits_change_nothing() {
    let mut e = editor();
    e.select(Some(PartKey::new(VALUE)));
    e.add_animation_row("HudMedicCharged", "Alpha").unwrap();
    assert!(body(&e, "HudMedicCharged").contains("Animate \"PlayerStatusHealthValue\" \"Alpha\""));
    e.delete_animation_row("HudHealthDyingPulse", 2).unwrap();
    assert_eq!(body(&e, "HudHealthDyingPulse").lines().count(), 2);
    let before = e.changes().len();
    assert_eq!(
        e.edit_animation("Risky", 1, AnimEdit::Start(1.0)),
        Err(AnimRefused::RunsCommands)
    );
    assert_eq!(
        e.edit_animation("TestNudge", 9, AnimEdit::Start(1.0)),
        Err(AnimRefused::NoSuchRow)
    );
    assert_eq!(
        e.edit_animation("TestNudge", 0, AnimEdit::Start(-1.0)),
        Err(AnimRefused::BadValue)
    );
    assert_eq!(
        e.edit_animation("Nope", 0, AnimEdit::Start(1.0)),
        Err(AnimRefused::NoSuchEvent)
    );
    assert_eq!(
        e.edit_animation("HudHealthDyingPulseLoop", 0, AnimEdit::Delay(1.0)),
        Err(AnimRefused::NotNamed)
    );
    assert_eq!(
        e.delete_animation_row("HudHealthDyingPulseLoop", 0),
        Err(AnimRefused::NotNamed)
    );
    assert_eq!(e.changes().len(), before);
}

#[gpui_kit::test]
fn the_section_lists_rows_deletes_and_locks_console_commands(cx: &mut TestAppContext) {
    let (_env, s, page) = open_maker(cx);
    cx.update(|cx| {
        page.update(cx, |p, cx| {
            p.editor_mut().unwrap().select(Some(PartKey::new(VALUE)));
            cx.notify();
        })
    });
    support::settle(cx, s.win);
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_anim_row("HudHealthDyingPulse", 0)).as_deref(),
        Some("Turns colour to HUDRedTeamSolid over 0.1 s, starting at 0 s")
    );
    click_in_panel(cx, &s, ids::hud_maker_anim_delete("HudHealthDyingPulse", 2));
    assert_eq!(
        cx.update(|cx| page.read(cx).editor().unwrap().changes().len()),
        1
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_maker_anim_locked("Risky")).as_deref(),
        Some("This animation runs game commands, so it can't be changed here.")
    );
    assert!(!support::present(
        cx,
        s.win,
        ids::hud_maker_anim_delete("Risky", 1)
    ));
}

fn select_value(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &gpui_kit::Entity<tf2tools_app::pages::hud_maker::HudMakerPage>,
    key: &str,
) {
    support::maker_page::on_page(cx, s, page, |p, _, cx| {
        p.editor_mut().unwrap().select(Some(PartKey::new(key)));
        cx.notify();
    });
}

fn pick(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &gpui_kit::Entity<tf2tools_app::pages::hud_maker::HudMakerPage>,
    rgb: [u8; 3],
) {
    use gpui_kit::AppContext as _;
    use gpui_kit::component::color_picker::ColorPickerEvent;
    let picker = cx.update(|cx| page.read(cx).colour_state().clone());
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
fn the_picker_recolours_a_row_and_one_drag_is_one_undo_step(cx: &mut TestAppContext) {
    let (_env, s, page) = open_maker(cx);
    select_value(cx, &s, &page, VALUE);
    click_in_panel(cx, &s, ids::hud_maker_anim_edit("HudHealthDyingPulse", 0));
    click_in_panel(cx, &s, ids::hud_maker_anim_colour("HudHealthDyingPulse", 0));
    pick(cx, &s, &page, [255, 0, 0]);
    pick(cx, &s, &page, [0, 255, 0]);
    let read = |cx: &mut TestAppContext| {
        cx.update(|cx| {
            let e = page.read(cx).editor().unwrap();
            (e.changes().len(), body(e, "HudHealthDyingPulse"))
        })
    };
    let (changes, text) = read(cx);
    assert_eq!(changes, 1, "one drag is one undo step");
    assert!(
        text.contains("\"FgColor\" \"0 255 0 255\" Linear 0 0.1"),
        "{text}"
    );
    support::maker_page::on_page(cx, &s, &page, |p, w, cx| p.undo(w, cx));
    let (changes, text) = read(cx);
    assert_eq!(changes, 0);
    assert!(text.contains("\"HUDRedTeamSolid\""), "{text}");
}

#[gpui_kit::test]
fn a_part_no_event_names_has_no_animations_section(cx: &mut TestAppContext) {
    let (_env, s, page) = open_maker(cx);
    select_value(cx, &s, &page, support::maker_page::IMAGE);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_PANEL_BODY));
    assert!(!support::present(cx, s.win, ids::HUD_MAKER_ANIMATIONS));
    select_value(cx, &s, &page, VALUE);
    assert!(support::present(cx, s.win, ids::HUD_MAKER_ANIMATIONS));
}
