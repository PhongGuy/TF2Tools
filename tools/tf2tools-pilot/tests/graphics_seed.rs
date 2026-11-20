use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::PathBuf;

use tf2tools_core::{
    AddonName, BuiltinCrosshairs, DataDir, Database, Endpoints, Library, MastercomfigStatus,
    ModuleChoice, Preset, Profile, SlotMode, StoreClient, Tf2Install,
};
use tf2tools_pilot::seed::{seed, seed_graphics};
use tf2tools_pilot::{GRAPHICS_VAR, graphics_requested};

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

// The Graphics page needs a managed mastercomfig, the whole catalogue and choices off the
// preset in every control kind before any row, changed bar or Reset shows.
#[test]
fn the_graphics_seed_manages_mastercomfig_with_changed_choices() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("com.tf2tools.app");
    let game = tmp.path().join("fake-tf2/Team Fortress 2");
    seed(&data, &game, &assets()).unwrap();
    seed_graphics(&data, &game).unwrap();

    let dir = DataDir::at(&data);
    let db = Database::open(&dir).unwrap();
    let lib = Library::new(db.clone(), dir.clone());
    let tf2 = Tf2Install::from_path(&game).unwrap();
    assert_eq!(db.settings().unwrap().tf2_path(), Some(game.as_path()));
    match lib.mastercomfig_status(&tf2).unwrap() {
        MastercomfigStatus::Managed { version, preset } => {
            assert_eq!(version, "9.100.1");
            assert_eq!(preset, Preset::High);
        }
        other => panic!("expected a managed mastercomfig, got {other:?}"),
    }

    let catalog = StoreClient::new(db, dir, Endpoints::default())
        .cached_mastercomfig_catalog("9.100.1")
        .expect("the 9.100.1 catalogue is cached");
    let modules: usize = catalog.categories().iter().map(|c| c.modules().len()).sum();
    assert_eq!(modules, 56);
    assert_eq!(catalog.resolve("shading", Preset::High), Some("high"));

    let choices = lib.mastercomfig_choices().unwrap().unwrap();
    assert_eq!(choices.preset(), Preset::High);
    let value = |v: &str| ModuleChoice::Value(v.to_owned());
    let expected = BTreeMap::from([
        ("vsync".to_owned(), value("on")),
        ("motion_blur".to_owned(), value("high")),
        ("fpscap".to_owned(), value("144")),
        ("decals".to_owned(), ModuleChoice::NotSet),
    ]);
    assert_eq!(choices.modules(), &expected);
    let cfg = std::fs::read_to_string(game.join("tf/cfg/overrides/modules.cfg")).unwrap();
    assert!(cfg.contains("fpscap=144"), "{cfg}");
}

#[test]
fn the_plain_seed_leaves_mastercomfig_out() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("com.tf2tools.app");
    let game = tmp.path().join("fake-tf2/Team Fortress 2");
    seed(&data, &game, &assets()).unwrap();
    let dir = DataDir::at(&data);
    let lib = Library::new(Database::open(&dir).unwrap(), dir);
    let tf2 = Tf2Install::from_path(&game).unwrap();
    assert_eq!(
        lib.mastercomfig_status(&tf2).unwrap(),
        MastercomfigStatus::NotInstalled
    );
}

#[test]
fn seeding_graphics_twice_changes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("com.tf2tools.app");
    let game = tmp.path().join("fake-tf2/Team Fortress 2");
    seed(&data, &game, &assets()).unwrap();
    seed_graphics(&data, &game).unwrap();
    let cfg = game.join("tf/cfg/overrides/modules.cfg");
    let before = std::fs::read(&cfg).unwrap();
    seed_graphics(&data, &game).unwrap();
    assert_eq!(std::fs::read(&cfg).unwrap(), before);
}

#[test]
fn only_one_turns_the_graphics_seed_on() {
    assert_eq!(GRAPHICS_VAR, "TF2TOOLS_PILOT_GRAPHICS");
    assert!(graphics_requested(Some(OsStr::new("1"))));
    for value in [None, Some(OsStr::new("")), Some(OsStr::new("0"))] {
        assert!(!graphics_requested(value), "{value:?}");
    }
}

fn seeded_with_graphics() -> (tempfile::TempDir, Library, Tf2Install) {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("com.tf2tools.app");
    let game = tmp.path().join("fake-tf2/Team Fortress 2");
    seed(&data, &game, &assets()).unwrap();
    seed_graphics(&data, &game).unwrap();
    let dir = DataDir::at(&data);
    let lib = Library::new(Database::open(&dir).unwrap(), dir);
    let tf2 = Tf2Install::from_path(&game).unwrap();
    (tmp, lib, tf2)
}

fn profile(lib: &Library, name: &str) -> Profile {
    lib.profiles()
        .unwrap()
        .into_iter()
        .find(|p| p.name() == name)
        .unwrap_or_else(|| panic!("no {name} profile"))
}

#[test]
fn the_graphics_seed_adds_potato_and_screenshots() {
    let (_tmp, lib, tf2) = seeded_with_graphics();
    let potato = profile(&lib, "Potato");
    assert_eq!(potato.hud(), &SlotMode::Leave);
    assert_eq!(potato.sounds(), &SlotMode::Leave);
    assert_eq!(potato.crosshairs(), &SlotMode::Leave);
    let SlotMode::Set(graphics) = potato.graphics() else {
        panic!("{:?}", potato.graphics())
    };
    assert_eq!(graphics.choices.preset(), Preset::Low);
    let value = |v: &str| ModuleChoice::Value(v.to_owned());
    let expected = BTreeMap::from([
        ("fpscap".to_owned(), value("60")),
        ("shadows".to_owned(), value("off")),
    ]);
    assert_eq!(graphics.choices.modules(), &expected);
    let SlotMode::Set(addons) = potato.addons() else {
        panic!("{:?}", potato.addons())
    };
    let pyroland = AddonName::new("mastercomfig-addon-no-pyroland.vpk").unwrap();
    assert_eq!(addons.on, std::slice::from_ref(&pyroland));
    assert!(addons.only_these);

    let shots = profile(&lib, "Screenshots");
    assert_eq!(shots.hud(), &SlotMode::None);
    assert_eq!(shots.sounds(), &SlotMode::None);
    assert_eq!(shots.crosshairs(), &SlotMode::None);
    let SlotMode::Set(ultra) = shots.graphics() else {
        panic!("{:?}", shots.graphics())
    };
    assert_eq!(ultra.choices.preset(), Preset::Ultra);
    assert!(ultra.choices.modules().is_empty());
    assert_eq!(shots.addons(), &SlotMode::Leave);

    // Potato turns Low on, hands its two changes to the app and keeps only no-pyroland.
    let builtins = BuiltinCrosshairs::load(&assets().join("crosshairs")).unwrap();
    let report = lib
        .apply_profile(potato.id(), &tf2, &builtins, false)
        .unwrap();
    assert!(report.failed().is_empty(), "{report:?}");
    assert!(report.skipped_steps().is_empty(), "{report:?}");
    assert_eq!(report.pending_modules(), Some(&graphics.choices));
    let custom = tf2.custom_dir();
    assert!(custom.join(pyroland.as_str()).is_file());
    assert!(
        !custom
            .join("mastercomfig-addon-no-soundscapes.vpk")
            .exists()
    );
    assert!(!custom.join("mastercomfig-addon-no-tutorial.vpk").exists());
}

#[test]
fn competitive_leaves_the_seeded_mastercomfig_alone() {
    let (_tmp, lib, tf2) = seeded_with_graphics();
    let comp = profile(&lib, "Competitive");
    let builtins = BuiltinCrosshairs::load(&assets().join("crosshairs")).unwrap();
    let report = lib
        .apply_profile(comp.id(), &tf2, &builtins, false)
        .unwrap();
    assert!(report.failed().is_empty(), "{report:?}");
    assert!(matches!(
        lib.mastercomfig_status(&tf2).unwrap(),
        MastercomfigStatus::Managed {
            preset: Preset::High,
            ..
        }
    ));
    assert_eq!(lib.active_profile().unwrap().as_ref(), Some(comp.id()));
}
