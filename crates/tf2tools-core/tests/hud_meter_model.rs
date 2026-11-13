//! The Item meter reads the chosen weapon's file: its box, its children and its edits.

use crate::common;

use common::hud_stock::stock_files_with;
use tf2tools_core::{
    BoardMode, GameSituation, HudModel, MeterKind, PartKey, Platform, Scene, Screen,
    SituationSwitch,
};

const SD: Screen = Screen::new(640, 480);
const METER: &str = "HudItemEffectMeter";
const COUNT: &str = "HudItemEffectMeter/ItemEffectMeterCount";
const LABEL: &str = "HudItemEffectMeter/ItemEffectMeterLabel";
const DEMO: &str = "resource/ui/huditemeffectmeter_demoman.res";

fn stock() -> HudModel {
    HudModel::load(&stock_files_with(&[]), Platform::Linux).unwrap()
}

fn has(m: &HudModel, key: &str) -> bool {
    m.part(&PartKey::new(key)).is_some()
}

#[test]
fn plain_is_the_default_and_reads_the_plain_file() {
    let m = stock();
    assert_eq!(m.meter(), MeterKind::Plain);
    assert!(has(&m, LABEL) && !has(&m, COUNT));
    let label = m.part(&PartKey::new(LABEL)).unwrap();
    assert_eq!(label.own_block().0, "resource/ui/huditemeffectmeter.res");
}

#[test]
fn a_chosen_meter_reads_its_own_file_and_box() {
    let m = stock().with_meter(MeterKind::Sword);
    assert_eq!(m.meter(), MeterKind::Sword);
    assert!(has(&m, COUNT), "the count is the demoman file's");
    let label = m.part(&PartKey::new(LABEL)).unwrap();
    assert_eq!(label.own_block().0, DEMO);
    let root = m.part(&PartKey::new(METER)).unwrap();
    assert_eq!(
        root.value("ypos").unwrap().text(),
        "r92",
        "the file's root beats hudlayout"
    );
    assert!(root.is_readable());
    let s = GameSituation::default().with_switch(SituationSwitch::Meter, true);
    let l = m.layout_in(SD, &s);
    let b = l.get(&PartKey::new(METER)).unwrap();
    assert_eq!(
        (b.x(), b.y(), b.wide(), b.tall()),
        (640 - 162, 480 - 92, 100, 50)
    );
    assert_eq!(l.get(&PartKey::new(COUNT)).unwrap().hidden(), None);
}

#[test]
fn the_meter_survives_scene_mode_and_changes() {
    let m = stock().with_meter(MeterKind::Sword);
    assert_eq!(m.with_scene(Scene::Scoreboard).meter(), MeterKind::Sword);
    assert!(has(
        &m.with_scene(Scene::Scoreboard).with_scene(Scene::Hud),
        COUNT
    ));
    assert_eq!(m.with_board_mode(BoardMode::Mvm).meter(), MeterKind::Sword);
    assert_eq!(m.with_changes(&[]).model().meter(), MeterKind::Sword);
    assert_eq!(m.with_meter(MeterKind::Plain).meter(), MeterKind::Plain);
}

#[test]
fn an_edit_to_the_chosen_meter_writes_its_file() {
    let m = stock().with_meter(MeterKind::Sword);
    let changes = m.move_part(&PartKey::new(COUNT), 4, 0, SD, 1).unwrap();
    let edited = m.with_changes(&changes);
    assert_eq!(
        (edited.dropped(), edited.touched()),
        (0, [DEMO.to_owned()].as_slice())
    );
    let x = edited
        .model()
        .part(&PartKey::new(COUNT))
        .unwrap()
        .value("xpos")
        .unwrap();
    assert_eq!(x.text(), "77");
    assert!(
        !edited
            .model()
            .with_meter(MeterKind::Plain)
            .part(&PartKey::new(LABEL))
            .unwrap()
            .own_block()
            .0
            .contains("demoman"),
        "the plain meter is untouched"
    );
}

#[test]
fn a_meter_neither_the_hud_nor_the_game_has_is_unreadable_not_a_problem() {
    let files = stock_files_with(&[(DEMO, None)]);
    let m = HudModel::load(&files, Platform::Linux)
        .unwrap()
        .with_meter(MeterKind::Sword);
    assert!(!m.part(&PartKey::new(METER)).unwrap().is_readable());
    assert!(m.problems().is_empty(), "{:?}", m.problems());
    let killstreak = stock().with_meter(MeterKind::Killstreak);
    assert!(!killstreak.part(&PartKey::new(METER)).unwrap().is_readable());
    assert!(killstreak.problems().is_empty());
}

const PLAIN: &str = "resource/ui/huditemeffectmeter.res";
const BAR: &str = "HudItemEffectMeter/ItemEffectMeter";

// A plain meter shaped as some HUDs ship it: its own label and bar, no #base, while other
// meter files #base it.
const OWN_PLAIN: &str = r##""Resource/UI/HudItemEffectMeter.res"
{
  HudItemEffectMeter { xpos c-60 ypos c103 wide 120 tall 8 }
  ItemEffectMeterLabel { ControlName CExLabel xpos 0 ypos 0 wide 120 tall 8 labelText "#TF_Ball" }
  ItemEffectMeter { ControlName ContinuousProgressBar xpos 0 ypos 0 wide 120 tall 8 }
}
"##;

fn copy_with(files: &[(&str, Option<&str>)]) -> HudModel {
    HudModel::load(
        &stock_files_with(files).with_install_folder("myhud"),
        Platform::Linux,
    )
    .unwrap()
}

#[test]
fn a_meter_file_other_meters_base_keeps_its_own_controls() {
    let cleaver = "#base \"huditemeffectmeter.res\"\n\"C\" { HudItemEffectMeter { xpos 1 } }";
    let spy = "#base \"huditemeffectmeter_demoman.res\"";
    let m = copy_with(&[
        (PLAIN, Some(OWN_PLAIN)),
        ("resource/ui/huditemeffectmeter_cleaver.res", Some(cleaver)),
        (
            "resource/ui/huditemeffectmeter_sodapopper.res",
            Some(cleaver),
        ),
        ("resource/ui/huditemeffectmeter_spy.res", Some(spy)),
        ("resource/ui/huditemeffectmeter_engineer.res", Some(spy)),
    ]);
    assert!(
        has(&m, LABEL) && has(&m, BAR),
        "the plain meter's own pieces"
    );
    assert!(
        has(&m.with_meter(MeterKind::Sword), COUNT),
        "the sword's own count"
    );
    assert!(
        has(&m.with_meter(MeterKind::Revolver), COUNT),
        "a meter reading only a meter file it #bases"
    );
}

// The stock Phlogistinator and Thermal Thruster files, each a #base of the plain file.
const PYRO: &str = "#base \"HudItemEffectMeter.res\"\n\"Resource/UI/HudItemEffectMeter_Pyro.res\" { HudItemEffectMeter { xpos r214 } }";
const ROCKET_PACK: &str = "#base \"HudItemEffectMeter.res\"\n\"Resource/UI/HudRocketPack.res\" {
  HudItemEffectMeter { xpos r95 ypos r105 wide 94 tall 45 }
  ItemEffectMeter { ControlName ContinuousProgressBar xpos 26 ypos 28 wide 22 tall 8 fgcolor White }
  ItemEffectMeter2 { ControlName ContinuousProgressBar xpos 52 ypos 28 wide 22 tall 8 fgcolor White }
}";

fn stock_with_bases() -> HudModel {
    let files = stock_files_with(&[
        ("resource/ui/huditemeffectmeter_pyro.res", Some(PYRO)),
        ("resource/ui/hudrocketpack.res", Some(ROCKET_PACK)),
    ]);
    HudModel::load(&files, Platform::Linux).unwrap()
}

#[test]
fn the_stock_phlogistinator_meter_draws_the_plain_file_it_bases() {
    let m = stock_with_bases().with_meter(MeterKind::Flamethrower);
    let label = m
        .part(&PartKey::new(LABEL))
        .expect("the plain file's label");
    assert_eq!(label.value("xpos").unwrap().file(), PLAIN);
    let root = m.part(&PartKey::new(METER)).unwrap();
    assert_eq!(
        root.value("xpos").unwrap().text(),
        "r214",
        "its own box wins"
    );
}

#[test]
fn the_meter_bars_draw_full_in_the_white_the_meter_code_sets() {
    let m = stock_with_bases().with_meter(MeterKind::RocketPack);
    assert!(has(&m, LABEL), "the label from the plain file");
    for key in [BAR, "HudItemEffectMeter/ItemEffectMeter2"] {
        let bar = m.part(&PartKey::new(key)).unwrap();
        assert_eq!(
            m.fill_colour(bar),
            Some(tf2tools_core::Rgba::WHITE),
            "{key}"
        );
    }
    let sword = stock().with_meter(MeterKind::Sword);
    let bar = sword.part(&PartKey::new(BAR)).unwrap();
    assert_eq!(sword.fill_colour(bar), Some(tf2tools_core::Rgba::WHITE));
}
