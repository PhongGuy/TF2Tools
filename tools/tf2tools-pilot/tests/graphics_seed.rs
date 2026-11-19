use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::PathBuf;

use tf2tools_core::{
    DataDir, Database, Endpoints, Library, MastercomfigStatus, ModuleChoice, Preset, StoreClient,
    Tf2Install,
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
