use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

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

    let catalog = StoreClient::new(db, dir, tf2tools_pilot::seed::offline_endpoints())
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
    let (data, game) = dirs(tmp.path());
    seed(&data, &game, &assets()).unwrap();
    seed_graphics(&data, &game).unwrap();
    let before = seeded_state(tmp.path());
    seed_graphics(&data, &game).unwrap();
    assert_seeded_eq(&seeded_state(tmp.path()), &before);
}

// A run that stopped at any step, or left a file half-written, heals on the next run into
// exactly what a clean run leaves.
#[test]
fn a_rerun_after_a_half_failed_graphics_seed_equals_a_clean_one() {
    let tmp = tempfile::tempdir().unwrap();
    let (data, game) = dirs(tmp.path());
    seed(&data, &game, &assets()).unwrap();
    let base = raw_tree(tmp.path());
    seed_graphics(&data, &game).unwrap();
    let clean = seeded_state(tmp.path());

    let modules_cfg = game.join("tf/cfg/overrides/modules.cfg");
    let hook = game.join("tf/custom/comfig-custom/cfg/app/setup_hook.cfg");
    let cache = DataDir::at(&data).mastercomfig_dir().join("9.100.1");
    let stops: [(&str, &dyn Fn()); 5] = [
        ("after the base VPK", &|| {
            undo_after_preset(&data, &modules_cfg, &cache);
            std::fs::remove_file(&hook).unwrap();
            sql(&data, "DELETE FROM mastercomfig_state");
        }),
        ("after the preset", &|| {
            undo_after_preset(&data, &modules_cfg, &cache)
        }),
        ("mid catalogue", &|| {
            undo_after_preset(&data, &modules_cfg, &cache);
            std::fs::create_dir_all(&cache).unwrap();
            std::fs::write(cache.join("modules.json"), b"{\"trunc").unwrap();
        }),
        ("mid modules.cfg", &|| {
            delete_profiles(&data, &["Potato", "Screenshots"]);
            std::fs::write(&modules_cfg, b"").unwrap();
            sql(&data, "UPDATE mastercomfig_state SET modules = '{}'");
        }),
        ("between the profiles", &|| {
            delete_profiles(&data, &["Screenshots"])
        }),
    ];
    for (stop, undo) in stops {
        restore(tmp.path(), &base);
        seed_graphics(&data, &game).unwrap();
        undo();
        seed_graphics(&data, &game).unwrap_or_else(|e| panic!("rerun {stop}: {e:#}"));
        let healed = seeded_state(tmp.path());
        assert_seeded_eq_named(&healed, &clean, stop);
    }
}

// What a run that stopped right after `apply_preset` leaves: no catalogue, no modules.cfg,
// no saved choices and neither profile.
fn undo_after_preset(data: &Path, modules_cfg: &Path, cache: &Path) {
    delete_profiles(data, &["Potato", "Screenshots"]);
    std::fs::remove_file(modules_cfg).unwrap();
    std::fs::remove_dir_all(cache).unwrap();
    sql(data, "UPDATE mastercomfig_state SET modules = '{}'");
}

fn delete_profiles(data: &Path, names: &[&str]) {
    let dir = DataDir::at(data);
    let lib = Library::new(Database::open(&dir).unwrap(), dir);
    for name in names {
        lib.delete_profile(profile(&lib, name).id()).unwrap();
    }
}

fn sql(data: &Path, statement: &str) {
    rusqlite::Connection::open(DataDir::at(data).db_path())
        .unwrap()
        .execute(statement, [])
        .unwrap();
}

fn dirs(root: &Path) -> (PathBuf, PathBuf) {
    (
        root.join("com.tf2tools.app"),
        root.join("fake-tf2/Team Fortress 2"),
    )
}

/// Every directory (None) and file (its bytes) under `root`, keyed by relative path.
type Tree = BTreeMap<PathBuf, Option<Vec<u8>>>;

fn raw_tree(root: &Path) -> Tree {
    fn walk(root: &Path, dir: &Path, out: &mut Tree) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let rel = path.strip_prefix(root).unwrap().to_path_buf();
            if path.is_dir() {
                out.insert(rel, None);
                walk(root, &path, out);
            } else {
                out.insert(rel, Some(std::fs::read(&path).unwrap()));
            }
        }
    }
    let mut out = Tree::new();
    walk(root, root, &mut out);
    out
}

/// Puts `root` back to `tree` exactly, the database's WAL included.
fn restore(root: &Path, tree: &Tree) {
    for entry in std::fs::read_dir(root).unwrap() {
        std::fs::remove_dir_all(entry.unwrap().path()).unwrap();
    }
    for (rel, bytes) in tree {
        match bytes {
            None => std::fs::create_dir_all(root.join(rel)).unwrap(),
            Some(bytes) => std::fs::write(root.join(rel), bytes).unwrap(),
        }
    }
}

/// The seeded tree with the database swapped for a dump of its rows. The database files are
/// compared by rows, not bytes: page layout, the WAL and its shm change with every open.
/// Row ids and timestamps are masked, since each run mints its own.
struct Seeded {
    files: Tree,
    rows: Vec<String>,
}

fn seeded_state(root: &Path) -> Seeded {
    let (data, _) = dirs(root);
    let db = DataDir::at(&data).db_path();
    let db_name = db.file_name().unwrap().to_str().unwrap().to_owned();
    let mut files = raw_tree(root);
    files.retain(|rel, _| {
        let name = rel.file_name().unwrap().to_str().unwrap();
        !(name == db_name || name == format!("{db_name}-wal") || name == format!("{db_name}-shm"))
    });
    Seeded {
        files,
        rows: db_rows(&db),
    }
}

fn db_rows(path: &Path) -> Vec<String> {
    let conn = rusqlite::Connection::open(path).unwrap();
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let mut rows = Vec::new();
    for table in tables {
        let mut stmt = conn.prepare(&format!("SELECT * FROM \"{table}\"")).unwrap();
        let columns: Vec<String> = stmt.column_names().map(str::to_owned).collect();
        let mut query = stmt.query([]).unwrap();
        while let Some(row) = query.next().unwrap() {
            let cells: Vec<String> = columns
                .iter()
                .enumerate()
                .map(|(i, col)| format!("{col}={}", masked(col, row.get_ref(i).unwrap())))
                .collect();
            rows.push(format!("{table}: {}", cells.join(", ")));
        }
    }
    rows.sort();
    rows
}

fn masked(column: &str, value: rusqlite::types::ValueRef<'_>) -> String {
    use rusqlite::types::ValueRef;
    if column.ends_with("_at") || column == "timestamp" || column == "date_added" {
        return "<time>".to_owned();
    }
    match value {
        ValueRef::Text(t) => {
            let text = String::from_utf8_lossy(t);
            if is_uuid(&text) {
                "<uuid>".to_owned()
            } else {
                format!("{text:?}")
            }
        }
        other => format!("{other:?}"),
    }
}

fn is_uuid(s: &str) -> bool {
    s.len() == 36
        && s.char_indices().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        })
}

fn assert_seeded_eq(got: &Seeded, want: &Seeded) {
    assert_seeded_eq_named(got, want, "");
}

fn assert_seeded_eq_named(got: &Seeded, want: &Seeded, label: &str) {
    let extra: Vec<_> = got
        .files
        .keys()
        .filter(|p| !want.files.contains_key(*p))
        .collect();
    let missing: Vec<_> = want
        .files
        .keys()
        .filter(|p| !got.files.contains_key(*p))
        .collect();
    assert!(
        extra.is_empty() && missing.is_empty(),
        "{label}: extra {extra:?}, missing {missing:?}"
    );
    for (rel, bytes) in &want.files {
        assert!(
            got.files[rel] == *bytes,
            "{label}: {} differs",
            rel.display()
        );
    }
    assert_eq!(got.rows, want.rows, "{label}: database rows");
}

// The seed's store client and the seeded boot only ever name loopback, and keep the offline
// mask on top, so not even a bug in the mask can reach a real host.
#[test]
fn the_seed_and_seeded_boot_point_at_loopback() {
    let endpoints = tf2tools_pilot::seed::offline_endpoints();
    assert_loopback(&endpoints);
    assert!(tf2tools_pilot::seed::offline_config().loopback_only());

    let tmp = tempfile::tempdir().unwrap();
    let boot = tf2tools_pilot::seeded_boot(&tmp.path().join("com.tf2tools.app")).unwrap();
    assert_loopback(&boot.endpoints);
    assert!(boot.offline);
    assert!(!boot.auto_sync);
}

fn assert_loopback(e: &Endpoints) {
    let mut urls = vec![
        e.gamebanana(),
        e.github_api(),
        e.github(),
        e.hud_resources(),
        e.serveme(),
        e.map_downloads(),
        e.mastercomfig_data(),
    ];
    urls.extend(e.preview_prefixes().iter().map(String::as_str));
    urls.extend(e.image_prefixes().iter().map(String::as_str));
    for url in urls {
        assert!(url.starts_with("http://127.0.0.1:"), "{url}");
    }
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
