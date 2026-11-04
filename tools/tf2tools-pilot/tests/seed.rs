use std::path::PathBuf;

use tf2tools_core::{
    CrosshairRef, DataDir, Database, HudFiles, HudModel, HudStatus, Library, MissingKind, PartKey,
    Platform, Provenance, SoundKind, Tf2Install,
};
use tf2tools_pilot::seed::{EMPTY_STORE_VAR, seed, seed_with};

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

#[test]
fn the_seed_builds_a_library_v5_can_open() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("com.tf2tools.app");
    let game = tmp.path().join("fake-tf2/Team Fortress 2");
    seed(&data, &game, &assets()).unwrap();

    let dir = DataDir::at(&data);
    let db = Database::open(&dir).unwrap();
    let lib = Library::new(db.clone(), dir);
    let tf2 = Tf2Install::from_path(&game).unwrap();
    assert_eq!(db.settings().unwrap().tf2_path(), Some(game.as_path()));

    let mut huds: Vec<_> = lib
        .list_huds()
        .unwrap()
        .iter()
        .map(|h| h.folder_name().to_owned())
        .collect();
    huds.sort();
    assert_eq!(huds, ["budhud", "flawhud", "rayshud"]);
    match lib.hud_status(&tf2).unwrap() {
        HudStatus::Managed { folder_name, .. } => assert_eq!(folder_name, "rayshud"),
        other => panic!("expected rayshud installed, got {other:?}"),
    }

    let mut sounds: Vec<_> = lib
        .list_sounds()
        .unwrap()
        .iter()
        .map(|s| s.name().to_owned())
        .collect();
    sounds.sort();
    assert_eq!(sounds, ["Beep", "Bell", "Classic Ding", "Quake Hit"]);
    let installed = lib.installed_sounds(&tf2).unwrap();
    assert!(installed.slot(SoundKind::Hitsound).managed().is_some());
    assert!(installed.slot(SoundKind::Killsound).managed().is_some());

    let crosshairs = lib.list_crosshairs().unwrap();
    let mut names: Vec<_> = crosshairs.iter().map(|c| c.name().to_owned()).collect();
    names.sort();
    assert_eq!(names, ["My Circle", "My Dot"]);
    let assignments = lib.assignments().unwrap();
    assert_eq!(assignments.len(), 2);
    let rocket = assignments
        .iter()
        .find(|a| a.weapon_script() == "tf_weapon_rocketlauncher")
        .unwrap();
    assert!(matches!(rocket.crosshair(), CrosshairRef::Library(_)));
    assert!(game.join("tf/custom/tf2tools-crosshairs").is_dir());
}

#[test]
fn seeding_an_existing_data_dir_changes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("com.tf2tools.app");
    let game = tmp.path().join("fake-tf2/Team Fortress 2");
    seed(&data, &game, &assets()).unwrap();
    let before = std::fs::read(data.join("tf2tools.db")).unwrap();
    seed(&data, &game, &assets()).unwrap();
    assert_eq!(std::fs::read(data.join("tf2tools.db")).unwrap(), before);
    let lib = Library::new(
        Database::open(&DataDir::at(&data)).unwrap(),
        DataDir::at(&data),
    );
    assert_eq!(lib.list_huds().unwrap().len(), 3);
}

#[test]
fn the_seed_has_log_rows_for_the_activity_log() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("com.tf2tools.app");
    seed(
        &data,
        &tmp.path().join("fake-tf2/Team Fortress 2"),
        &assets(),
    )
    .unwrap();
    let db = Database::open(&DataDir::at(&data)).unwrap();
    let rows = tf2tools_core::ActivityLog::new(db)
        .query(&tf2tools_core::LogQuery::new(100))
        .unwrap();
    assert_eq!(rows.len(), 12);
    for level in [
        tf2tools_core::LogLevel::Info,
        tf2tools_core::LogLevel::Warning,
        tf2tools_core::LogLevel::Error,
    ] {
        assert!(rows.iter().any(|r| r.level() == level), "{level:?}");
    }
    assert!(rows.iter().filter(|r| r.details().is_some()).count() >= 2);
}

#[test]
fn the_seed_installs_v5_s_three_addons() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("com.tf2tools.app");
    let game = tmp.path().join("fake-tf2/Team Fortress 2");
    seed(&data, &game, &assets()).unwrap();
    let dir = DataDir::at(&data);
    let lib = Library::new(Database::open(&dir).unwrap(), dir);
    let tf2 = Tf2Install::from_path(&game).unwrap();
    let addons = lib.list_addons(Some(&tf2)).unwrap();
    let mut names: Vec<_> = addons.iter().map(|a| a.display_name().to_owned()).collect();
    names.sort();
    assert_eq!(names, ["No Pyroland", "No Soundscapes", "No Tutorial"]);
    assert!(
        addons
            .iter()
            .all(|a| a.is_installed() && a.installed_tag() == "9.100.1")
    );
}

#[test]
fn the_seed_has_its_profiles_with_competitive_active() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("com.tf2tools.app");
    let game = tmp.path().join("fake-tf2/Team Fortress 2");
    seed(&data, &game, &assets()).unwrap();
    let dir = DataDir::at(&data);
    let lib = Library::new(Database::open(&dir).unwrap(), dir);
    let profiles = lib.profiles().unwrap();
    let names: Vec<_> = profiles.iter().map(|p| p.name().to_owned()).collect();
    assert_eq!(names, ["Arena", "Casual", "Competitive", "Vanilla"]);
    let profiles = &profiles[1..];
    let comp = &profiles[1];
    assert_eq!(lib.active_profile().unwrap().as_ref(), Some(comp.id()));
    assert_eq!(comp.hud().map(|h| h.folder_name()), Some("rayshud"));
    assert_eq!(comp.crosshairs().len(), 2);
    assert_eq!(profiles[0].hud().map(|h| h.folder_name()), Some("flawhud"));
    assert!(profiles[0].killsound().is_none());
    assert!(profiles[2].hud().is_none() && profiles[2].crosshairs().is_empty());
    let tf2 = Tf2Install::from_path(&game).unwrap();
    assert!(tf2.custom_dir().join("rayshud").is_dir());
}

// README screenshots show the seeded stores, so no row may credit a real creator or item.
#[test]
fn the_seeded_store_credits_no_real_creator() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("com.tf2tools.app");
    let game = tmp.path().join("fake-tf2/Team Fortress 2");
    seed(&data, &game, &assets()).unwrap();

    let conn = rusqlite::Connection::open(DataDir::at(&data).db_path()).unwrap();
    let mut rows = Vec::new();
    for table in [
        "store_sounds",
        "store_huds_gb",
        "store_crosshairs",
        "store_huds",
    ] {
        let mut stmt = conn
            .prepare(&format!("SELECT name || ' / ' || author FROM {table}"))
            .unwrap();
        let found: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert!(!found.is_empty(), "{table} is empty");
        rows.extend(found);
    }
    for real in [
        "sevin",
        "heks",
        "colly",
        "dakasin",
        "flowerx",
        "wachaczi",
        "7hud",
        "berryhud",
        "broken hud",
        "center hud",
        "colourhud",
        "flawhud",
        "musical events",
        "russian mge",
        "understandable pyro",
        "quake iii",
    ] {
        for row in &rows {
            assert!(!row.to_lowercase().contains(real), "{row:?} names {real:?}");
        }
    }
}

// Missing Items QA needs a profile whose store HUD is not in the library, as after a
// v5 user deletes a downloaded HUD.
#[test]
fn the_arena_profile_misses_its_store_hud() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("com.tf2tools.app");
    seed(
        &data,
        &tmp.path().join("fake-tf2/Team Fortress 2"),
        &assets(),
    )
    .unwrap();
    let dir = DataDir::at(&data);
    let lib = Library::new(Database::open(&dir).unwrap(), dir);
    let arena = lib
        .profiles()
        .unwrap()
        .into_iter()
        .find(|p| p.name() == "Arena")
        .expect("an Arena profile");
    let missing = lib.missing_items(arena.id()).unwrap();
    assert_eq!(missing.len(), 1, "{missing:?}");
    assert!(matches!(missing[0].kind(), MissingKind::Hud));
    assert_eq!(
        missing[0].provenance(),
        &Provenance::Curated {
            hud_id: "arenahud".into()
        }
    );
    assert!(
        lib.list_huds()
            .unwrap()
            .iter()
            .all(|h| h.folder_name() != "arenahud")
    );
}

fn meta_keys(data: &std::path::Path) -> Vec<String> {
    let conn = rusqlite::Connection::open(DataDir::at(data).db_path()).unwrap();
    let mut stmt = conn.prepare("SELECT key FROM store_cache_meta").unwrap();
    stmt.query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn rows(data: &std::path::Path, table: &str) -> i64 {
    let conn = rusqlite::Connection::open(DataDir::at(data).db_path()).unwrap();
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

// Skeleton and loading-card QA needs one store with no rows and nothing marking a sync.
#[test]
fn the_empty_store_option_leaves_that_store_unsynced() {
    assert_eq!(EMPTY_STORE_VAR, "TF2TOOLS_PILOT_EMPTY_STORE");
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("com.tf2tools.app");
    let game = tmp.path().join("fake-tf2/Team Fortress 2");
    seed_with(&data, &game, &assets(), Some("sounds")).unwrap();
    assert_eq!(rows(&data, "store_sounds"), 0);
    let keys = meta_keys(&data);
    assert!(keys.iter().all(|k| !k.starts_with("sounds_")), "{keys:?}");
    assert!(rows(&data, "store_huds_gb") > 0 && rows(&data, "store_huds") > 0);
    assert!(
        keys.iter().any(|k| k == "crosshairs_last_synced"),
        "{keys:?}"
    );
}

#[test]
fn an_unknown_empty_store_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("com.tf2tools.app");
    let game = tmp.path().join("fake-tf2/Team Fortress 2");
    assert!(seed_with(&data, &game, &assets(), Some("hats")).is_err());
}

#[test]
fn the_seeded_game_holds_the_stock_hud_files() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("com.tf2tools.app");
    let game = tmp.path().join("fake-tf2/Team Fortress 2");
    seed(&data, &game, &assets()).unwrap();
    let files = HudFiles::open_game(&game.join("tf")).unwrap();
    let model = HudModel::load(&files, Platform::Linux).unwrap();
    assert!(
        model
            .part(&PartKey::new(
                "HudPlayerStatus/HudPlayerHealth/PlayerStatusHealthValue"
            ))
            .is_some()
    );
}
