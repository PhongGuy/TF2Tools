//! Playing animation events to a frame at a moment.

use crate::common;

use common::hud_anims::{BONUS, HEALTH_KEY, HEALTH_RES, VALUE, anim_model, health_with_bonus};
use std::time::{Duration, Instant};
use tf2tools_core::{AnimationChange, Change, Fired, HudModel, Interp, PartKey, Rgba, Screen};

const HD: Screen = Screen::new(1920, 1080);

fn fire(event: &str) -> Vec<Fired> {
    vec![Fired::new(event, Some(PartKey::new(HEALTH_KEY)))]
}

fn with(model: &HudModel, event: &str, body: &str) -> HudModel {
    model
        .with_changes(&[Change::animation(1, AnimationChange::new(event, body))])
        .into_model()
}

#[test]
fn curves_shape_progress_like_the_engine() {
    let close = |a: f32, b: f32| (a - b).abs() < 1e-4;
    assert!(close(Interp::Linear.at(0.25, 0), 0.25));
    assert!(close(Interp::Accel.at(0.5, 0), 0.25));
    assert!(close(Interp::Deaccel.at(0.25, 0), 0.5));
    assert!(close(Interp::Spline.at(0.25, 0), 0.15625));
    assert!(close(Interp::Pulse(1.0).at(0.0, 0), 1.0));
    assert!(close(Interp::Pulse(1.0).at(0.5, 0), 0.0));
    assert!(close(Interp::Bias(0.5).at(0.25, 0), 0.25));
    assert!(close(Interp::Gain(0.5).at(0.3, 0), 0.3));
    assert!(close(Interp::Bounce.at(1.0, 0), 1.0));
    let f = Interp::Flicker(0.5);
    assert_eq!(f.at(0.3, 7), f.at(0.3, 7), "a seed replays the same");
    assert!([0.0, 1.0].contains(&f.at(0.3, 7)));
}

#[test]
fn a_colour_pulse_blends_from_the_part_s_colour() {
    let model = anim_model(&[]);
    let layout = model.layout(HD);
    let frame = model
        .animations()
        .frame(&model, &layout, &fire("HudHealthDyingPulse"), 0.025);
    // TanDark 117 107 94 towards HUDRedTeamSolid 180 92 77, a quarter of the way.
    assert_eq!(
        frame.get(&PartKey::new(VALUE)).unwrap().fg(),
        Some(Rgba::new(133, 103, 90, 255))
    );
}

#[test]
fn a_loop_restarts_the_event() {
    let model = anim_model(&[]);
    let layout = model.layout(HD);
    // The loop reruns the pulse at 0.25; 0.025 later it is a quarter of the way again.
    let frame = model
        .animations()
        .frame(&model, &layout, &fire("HudHealthDyingPulse"), 0.275);
    assert_eq!(
        frame.get(&PartKey::new(VALUE)).unwrap().fg(),
        Some(Rgba::new(133, 103, 90, 255))
    );
    assert_eq!(model.animations().length(&fire("HudHealthDyingPulse")), 2.0);
}

#[test]
fn alpha_starts_from_the_part_s_own_alpha() {
    let health = health_with_bonus();
    let model = anim_model(&[(HEALTH_RES, Some(&health))]);
    let layout = model.layout(HD);
    let frame = model
        .animations()
        .frame(&model, &layout, &fire("HudHealthBonusPulse"), 0.1);
    assert_eq!(frame.get(&PartKey::new(BONUS)).unwrap().alpha(), Some(100));
}

#[test]
fn stop_event_freezes_what_it_stops() {
    let model = anim_model(&[])
        .with_changes(&[
            Change::animation(
                1,
                AnimationChange::new(
                    "Fade",
                    "Animate PlayerStatusHealthValue Alpha \"0\" Linear 0 1",
                ),
            ),
            Change::animation(2, AnimationChange::new("Halt", "StopEvent Fade 0.5")),
        ])
        .into_model();
    let layout = model.layout(HD);
    let fired = [fire("Fade"), fire("Halt")].concat();
    let frame = model.animations().frame(&model, &layout, &fired, 1.0);
    assert_eq!(frame.get(&PartKey::new(VALUE)).unwrap().alpha(), Some(128));
}

#[test]
fn a_zero_delay_self_loop_returns() {
    let model = with(&anim_model(&[]), "Spin", "RunEvent Spin 0");
    let layout = model.layout(HD);
    let began = Instant::now();
    let _ = model
        .animations()
        .frame(&model, &layout, &fire("Spin"), 5.0);
    assert!(began.elapsed() < Duration::from_secs(2));
}

#[test]
fn a_missing_colour_name_is_magenta_and_unplayed_fields_are_listed() {
    let body = "Animate PlayerStatusHealthValue FgColor \"NoSuchColour\" Linear 0 0\nAnimate PlayerStatusHealthValue Blur \"3\" Linear 0 1";
    let model = with(&anim_model(&[]), "Odd", body);
    let layout = model.layout(HD);
    let frame = model.animations().frame(&model, &layout, &fire("Odd"), 0.5);
    assert_eq!(
        frame.get(&PartKey::new(VALUE)).unwrap().fg(),
        Some(Rgba::new(255, 0, 255, 255))
    );
    assert_eq!(frame.in_game_only(), ["Blur"]);
}

#[test]
fn panels_are_found_only_within_the_starting_part() {
    let model = anim_model(&[]);
    let layout = model.layout(HD);
    let frame = model
        .animations()
        .frame(&model, &layout, &fire("HudMedicCharged"), 0.05);
    assert!(
        frame
            .get(&PartKey::new("HudMedicCharge/ChargeLabel"))
            .is_none()
    );
    let whole = [Fired::new("HudMedicCharged", None)];
    let frame = model.animations().frame(&model, &layout, &whole, 0.05);
    assert!(
        frame
            .get(&PartKey::new("HudMedicCharge/ChargeLabel"))
            .unwrap()
            .fg()
            .is_some()
    );
}

#[test]
fn a_moved_part_moves_in_the_laid_out_frame() {
    let model = with(
        &anim_model(&[]),
        "Shift",
        "Animate PlayerStatusHealthValue xpos \"10\" Linear 0 0",
    );
    let layout = model.layout(HD);
    let frame = model
        .animations()
        .frame(&model, &layout, &fire("Shift"), 0.1);
    let moved = frame.laid_out(&layout);
    // 10 units at 1080 high are 22 px; the health panel sits at x 0.
    assert_eq!(moved.get(&PartKey::new(VALUE)).unwrap().x(), 22);
    assert_eq!(layout.get(&PartKey::new(VALUE)).unwrap().x(), 171);
}

#[test]
fn a_chain_across_every_file_is_measured_without_running_out_of_stack() {
    use common::hud_anims::{MANIFEST_PATH, anim_files};
    let (files, per) = (64, 4000);
    let list: String = (0..files)
        .map(|f| format!("file scripts/c{f}.txt\n"))
        .collect();
    let manifest = format!("m {{\n{list}}}");
    let scripts: Vec<(String, String)> = (0..files)
        .map(|f| {
            let text: String = (0..per)
                .map(|i| {
                    let n = f * per + i;
                    format!("event C{n} {{ RunEvent C{} 0.0009765625 }}\n", n + 1)
                })
                .collect();
            (format!("scripts/c{f}.txt"), text)
        })
        .collect();
    let mut extra: Vec<(&str, Option<&str>)> = vec![(MANIFEST_PATH, Some(&manifest))];
    extra.extend(scripts.iter().map(|(p, t)| (p.as_str(), Some(t.as_str()))));
    let model = HudModel::load(&anim_files(&extra), tf2tools_core::Platform::Linux).unwrap();
    assert_eq!(model.animations().events().len(), files * per);
    let length = model.animations().length(&[Fired::new("C0", None)]);
    // Each hop waits 2^-10 s, which sums exactly: 256,000 hops make 250 s.
    assert_eq!(length, 250.0);
}
