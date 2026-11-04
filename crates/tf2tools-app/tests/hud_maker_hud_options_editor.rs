//! The editor's HUD options: values, one-step undo, the reload flag and rebasing.

mod support;

use std::sync::Arc;

use support::hud_switches::{GROUP, HV, SLOT, switch_files};
use tf2tools_app::pages::hud_maker::editor::HudEditor;
use tf2tools_core::{HudModel, HudOptions, OptionValue, PartKey, Platform};

fn editor() -> (tf2tools_core::HudFiles, HudEditor) {
    let files = switch_files();
    let options = Arc::new(HudOptions::read(&files, None, Platform::Linux));
    let model = HudModel::load(&files, Platform::Linux).unwrap();
    (
        files,
        HudEditor::new(model, Vec::new()).with_hud_options(options),
    )
}

fn rebuilt(files: &tf2tools_core::HudFiles, e: &mut HudEditor) {
    let choices = e.switch_choices();
    let switched = e.hud_options().unwrap().switch(files, &choices).unwrap();
    let model = HudModel::load(switched.files(), Platform::Linux).unwrap();
    e.rebase(model, choices);
}

fn alignment(e: &HudEditor) -> Option<String> {
    e.model()
        .part(&PartKey::new(HV))
        .and_then(|p| p.value("textAlignment"))
        .map(|l| l.text().to_owned())
}

#[test]
fn a_choice_is_one_undo_step_and_flags_a_reload() {
    let (_files, mut e) = editor();
    assert!(!e.needs_switch_reload());
    let small = OptionValue::Pick("resource/ui/damage_small.res".into());
    e.set_hud_option(GROUP, small.clone()).unwrap();
    assert_eq!(
        (e.changes().len(), e.hud_option_value(GROUP)),
        (1, Some(small))
    );
    assert!(e.needs_switch_reload());
    assert!(e.undo());
    assert!(e.changes().is_empty() && !e.needs_switch_reload());
    assert!(e.redo());
    assert!(e.needs_switch_reload());
}

#[test]
fn rebasing_shows_the_switch_and_keeps_canvas_edits() {
    let (files, mut e) = editor();
    e.select(Some(PartKey::new(HV)));
    e.recolour_selected("255 0 0 255").unwrap();
    e.set_hud_option(SLOT, OptionValue::Bool(true)).unwrap();
    assert_eq!(alignment(&e), None);
    rebuilt(&files, &mut e);
    assert!(!e.needs_switch_reload());
    assert_eq!(alignment(&e).as_deref(), Some("east"));
    let colour = e
        .model()
        .part(&PartKey::new(HV))
        .unwrap()
        .value("fgcolor")
        .unwrap()
        .text()
        .to_owned();
    assert_eq!(colour, "255 0 0 255");
}

#[test]
fn a_group_reset_drops_its_choices_as_one_step() {
    let (_files, mut e) = editor();
    e.set_hud_option(SLOT, OptionValue::Bool(true)).unwrap();
    assert!(e.hud_option_group_changed("Switches"));
    assert!(e.reset_hud_option_group("Switches"));
    assert!(e.changes().is_empty());
    assert!(!e.reset_hud_option_group("Switches"));
    assert!(e.undo());
    assert_eq!(e.changes().len(), 1);
}

#[test]
fn canvas_edits_never_flag_a_reload() {
    let (_files, mut e) = editor();
    e.select(Some(PartKey::new(HV)));
    e.move_selected(4, 0).unwrap();
    assert!(!e.needs_switch_reload());
}

#[test]
fn a_rebase_for_older_choices_is_refused() {
    let (files, mut e) = editor();
    e.set_hud_option(SLOT, OptionValue::Bool(true)).unwrap();
    let older = e.switch_choices();
    let switched = e.hud_options().unwrap().switch(&files, &older).unwrap();
    let model = HudModel::load(switched.files(), Platform::Linux).unwrap();
    assert!(e.undo());
    assert!(!e.rebase(model, older));
    assert_eq!(alignment(&e), None);
    assert!(e.switch_built_for().is_empty());
}
