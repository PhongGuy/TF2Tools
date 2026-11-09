use crate::support;

use gpui_kit::TestAppContext;
use tf2tools_app::pages::profiles::model::{
    activated_description, crosshair_count, failed_description, kept_parts, kind_label,
    load_closes_editor, matches, redownload_target, report_details, step_label, success_title,
};
use tf2tools_app::pages::profiles::probe::Tf2Probe;
use tf2tools_core::{ApplyStep, GbId, MissingKind, ProfileId, Provenance, StoreItemId, StoreKind};

fn gb(id: u64) -> Provenance {
    Provenance::GameBanana {
        gb_id: GbId::new(id),
        author: String::new(),
    }
}

#[test]
fn the_crosshair_badge_counts() {
    assert_eq!(crosshair_count(1), "1 crosshair");
    assert_eq!(crosshair_count(3), "3 crosshairs");
}

#[test]
fn search_ignores_case_and_surrounding_space() {
    assert!(matches("Competitive", "comp"));
    assert!(matches("Competitive", "  PETIT "));
    assert!(matches("Competitive", ""));
    assert!(matches("Competitive", "   "));
    assert!(!matches("Competitive", "casual"));
}

#[test]
fn steps_and_kinds_have_v5_s_labels() {
    assert_eq!(step_label(ApplyStep::Hud), "HUD");
    assert_eq!(step_label(ApplyStep::Hitsound), "Hitsound");
    assert_eq!(step_label(ApplyStep::Killsound), "Killsound");
    assert_eq!(step_label(ApplyStep::Crosshairs), "Crosshairs");
    assert_eq!(kind_label(&MissingKind::Hud), "HUD");
    assert_eq!(kind_label(&MissingKind::Hitsound), "HITSOUND");
    assert_eq!(kind_label(&MissingKind::Killsound), "KILLSOUND");
    let crosshair = MissingKind::Crosshair {
        weapon_script: "tf_weapon_scattergun".into(),
    };
    assert_eq!(kind_label(&crosshair), "CROSSHAIR");
}

#[test]
fn the_activated_toast_reads_like_v5() {
    assert_eq!(
        activated_description(&["HUD", "Hitsound", "Killsound", "Crosshairs"], &[]),
        "Installed: HUD, Hitsound, Killsound, Crosshairs"
    );
    assert_eq!(
        activated_description(&[], &[]),
        "All customizations cleared"
    );
}

#[test]
fn an_apply_with_skips_names_what_it_kept() {
    assert_eq!(
        activated_description(&["Hitsound"], &["HUD"]),
        "Installed: Hitsound. Kept what you had for: HUD (missing from your library)"
    );
    assert_eq!(
        activated_description(&[], &["HUD", "Crosshairs"]),
        "Nothing new installed. Kept what you had for: HUD, Crosshairs (missing from your library)"
    );
}

#[test]
fn a_skip_makes_the_title_say_applied_not_activated() {
    assert_eq!(success_title("Comp", false), "Profile \"Comp\" activated");
    assert_eq!(success_title("Comp", true), "Profile \"Comp\" applied");
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

// An editor opened after a load started, such as one on a just-created profile, may be newer
// than what the load read, so only one already open then can be closed by its result.
#[test]
fn a_load_closes_only_an_editor_open_when_it_started_and_missing_from_it() {
    let (made, other) = (ProfileId::new("made"), ProfileId::new("other"));
    assert!(!load_closes_editor(None, &made, [&other]));
    assert!(load_closes_editor(Some(&made), &made, [&other]));
    assert!(!load_closes_editor(Some(&made), &made, [&other, &made]));
}
