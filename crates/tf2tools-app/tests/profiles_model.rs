use crate::support;

use gpui_kit::{SharedString, TestAppContext};
use tf2tools_app::pages::profiles::model::{ApplyOutcome, modules_no_catalog, outcome_toast};
use tf2tools_app::pages::profiles::model::{
    failed_description, kept_parts, redownload_target, report_details, step_label,
};
use tf2tools_app::pages::profiles::probe::Tf2Probe;
use tf2tools_app::shell::toast::ToastKind;
use tf2tools_core::ApplySkipReason;
use tf2tools_core::{ApplyStep, GbId, MissingKind, Provenance, StoreItemId, StoreKind};

fn parts(names: &[&'static str]) -> Vec<SharedString> {
    names.iter().map(|&n| n.into()).collect()
}

fn gb(id: u64) -> Provenance {
    Provenance::GameBanana {
        gb_id: GbId::new(id),
        author: String::new(),
    }
}

#[test]
fn steps_have_v5_s_labels() {
    assert_eq!(step_label(ApplyStep::Hud), "HUD");
    assert_eq!(step_label(ApplyStep::Hitsound), "Hitsound");
    assert_eq!(step_label(ApplyStep::Killsound), "Killsound");
    assert_eq!(step_label(ApplyStep::Crosshairs), "Crosshairs");
    assert_eq!(step_label(ApplyStep::Graphics), "Graphics");
    assert_eq!(step_label(ApplyStep::Addons), "Add-ons");
}

#[test]
fn kept_parts_name_each_skipped_slot_once_in_step_order() {
    let scattergun = MissingKind::Crosshair {
        weapon_script: "tf_weapon_scattergun".into(),
    };
    let minigun = MissingKind::Crosshair {
        weapon_script: "tf_weapon_minigun".into(),
    };
    let kinds = [
        MissingKind::Killsound,
        MissingKind::Hud,
        scattergun,
        minigun,
    ];
    assert_eq!(kept_parts(&kinds), ["HUD", "Killsound", "Crosshairs"]);
    assert!(kept_parts(&[]).is_empty());
}

#[test]
fn failures_name_each_step_in_plain_words() {
    let failed = [
        (
            ApplyStep::Hud,
            "cannot read or write /x/tf/custom/budhud: Permission denied (os error 13)".to_owned(),
        ),
        (ApplyStep::Crosshairs, "gone".to_owned()),
    ];
    let text = failed_description(&failed);
    assert_eq!(
        text,
        "Couldn't put these in place: HUD, Crosshairs. Make sure TF2 is closed and TF2Tools can write to your TF2 folder, then try again."
    );
    support::assert_plain(&text);
}

#[test]
fn report_details_leave_out_empty_parts() {
    use ApplyStep::*;
    assert_eq!(
        report_details(&[Hud, Hitsound, Killsound, Crosshairs], &[], &[]),
        "Applied: HUD, Hitsound, Killsound, Crosshairs"
    );
    assert_eq!(
        report_details(&[Hitsound], &["rayshud"], &[(Crosshairs, "gone".into())]),
        "Applied: Hitsound; Skipped: rayshud; Failed: Crosshairs (gone)"
    );
    assert_eq!(
        report_details(&[], &[], &[(Hud, "denied".into())]),
        "Failed: HUD (denied)"
    );
    assert_eq!(report_details(&[], &[], &[]), "Applied: nothing");
}

#[test]
fn store_items_can_be_downloaded_again() {
    let curated = Provenance::Curated {
        hud_id: "budhud".into(),
    };
    assert_eq!(
        redownload_target(&MissingKind::Hud, &curated),
        Some((
            StoreKind::CuratedHuds,
            StoreItemId::Curated("budhud".into())
        ))
    );
    assert_eq!(
        redownload_target(&MissingKind::Hud, &gb(7)),
        Some((
            StoreKind::GameBananaHuds,
            StoreItemId::GameBanana(GbId::new(7))
        ))
    );
    assert_eq!(
        redownload_target(&MissingKind::Killsound, &gb(8)),
        Some((StoreKind::Sounds, StoreItemId::GameBanana(GbId::new(8))))
    );
    let crosshair = MissingKind::Crosshair {
        weapon_script: "tf_weapon_scattergun".into(),
    };
    assert_eq!(
        redownload_target(&crosshair, &gb(9)),
        Some((StoreKind::Crosshairs, StoreItemId::GameBanana(GbId::new(9))))
    );
    assert_eq!(
        redownload_target(&MissingKind::Hitsound, &Provenance::Local),
        None
    );
}

// The default probe scans a temp proc tree here, never the machine's /proc.
#[gpui_kit::test]
fn the_default_probe_scans_the_proc_root_and_can_be_pinned(cx: &mut TestAppContext) {
    support::init(cx);
    let proc = tempfile::tempdir().unwrap();
    cx.update(|cx| Tf2Probe::set_proc_root(cx, proc.path().to_path_buf()));
    assert!(!cx.update(|cx| Tf2Probe::get(cx))(), "an empty proc root");
    let pid = proc.path().join("4242");
    std::fs::create_dir_all(&pid).unwrap();
    std::fs::write(pid.join("comm"), "tf_linux64\n").unwrap();
    assert!(cx.update(|cx| Tf2Probe::get(cx))(), "a running tf_linux64");
    cx.update(|cx| Tf2Probe::install(cx, || false));
    assert!(!cx.update(|cx| Tf2Probe::get(cx))());
    cx.update(|cx| Tf2Probe::install(cx, || true));
    assert!(cx.update(|cx| Tf2Probe::get(cx))());
}

const APPLIED: &str = "Comp applied. TF2 picks it up the next time it starts.";

fn outcome(active: bool) -> ApplyOutcome {
    ApplyOutcome {
        name: "Comp".into(),
        active,
        ..ApplyOutcome::default()
    }
}

#[test]
fn a_clean_apply_is_one_success_line() {
    assert_eq!(
        outcome_toast(&outcome(true)),
        (ToastKind::Success, APPLIED.into(), String::new())
    );
}

#[test]
fn outcome_toast_warns_when_the_profile_stayed_inactive() {
    let (kind, title, description) = outcome_toast(&outcome(false));
    assert_eq!(
        (kind, title.as_str(), description.as_str()),
        (ToastKind::Warning, APPLIED, "")
    );
}

#[test]
fn kept_items_and_skipped_steps_follow_as_sentences() {
    let mut o = outcome(false);
    o.kept = parts(&["HUD"]);
    o.skipped_steps = vec![(
        ApplyStep::Graphics,
        ApplySkipReason::MastercomfigNotInstalled,
    )];
    let (kind, title, description) = outcome_toast(&o);
    assert_eq!((kind, title.as_str()), (ToastKind::Warning, APPLIED));
    assert_eq!(
        description,
        "Kept what you had for: HUD (missing from your library). Graphics was left as it is: mastercomfig isn't installed, so install it on the Graphics page first."
    );
    let mut o = outcome(false);
    o.skipped_steps = vec![
        (ApplyStep::Graphics, ApplySkipReason::MastercomfigNotManaged),
        (ApplyStep::Addons, ApplySkipReason::MastercomfigNotInstalled),
    ];
    assert_eq!(
        outcome_toast(&o).2,
        "Graphics was left as it is: TF2Tools didn't install your mastercomfig, so take it over on the Graphics page first. Add-ons was left as it is: add-ons need mastercomfig, which isn't installed."
    );
}

#[test]
fn the_movement_line_and_the_note_end_the_description() {
    let mut o = outcome(true);
    o.movement = Some("Your movement keys are back to normal.".into());
    o.note = Some("A copy of manual is in your library.".into());
    assert_eq!(
        outcome_toast(&o),
        (
            ToastKind::Warning,
            APPLIED.into(),
            "Your movement keys are back to normal. A copy of manual is in your library.".into()
        )
    );
}

#[test]
fn a_hud_kept_out_of_its_folder_warns_after_the_kept_line() {
    let mut o = outcome(true);
    o.blocked = Some("m0rehud".into());
    assert_eq!(
        outcome_toast(&o),
        (
            ToastKind::Warning,
            APPLIED.into(),
            "A folder named m0rehud is already in your custom folder, so parts of this HUD may not show. Move or rename that folder and install again.".into()
        )
    );
    o.kept = parts(&["Hitsound"]);
    assert!(outcome_toast(&o).2.starts_with(
        "Kept what you had for: Hitsound (missing from your library). A folder named m0rehud"
    ));
}

#[test]
fn a_failure_names_its_steps_and_the_graphics_reason() {
    let mut o = outcome(false);
    o.name = "Potato".into();
    o.failed = vec![(ApplyStep::Graphics, modules_no_catalog().to_string())];
    assert_eq!(
        outcome_toast(&o),
        (
            ToastKind::Error,
            "Couldn't apply Potato".into(),
            "Graphics: Open the Graphics page once so TF2Tools can read your mastercomfig's settings.".into()
        )
    );
    o.failed.insert(0, (ApplyStep::Hud, "denied".into()));
    assert_eq!(
        outcome_toast(&o).2,
        "Couldn't put these in place: HUD. Make sure TF2 is closed and TF2Tools can write to your TF2 folder, then try again. Graphics: Open the Graphics page once so TF2Tools can read your mastercomfig's settings."
    );
}
