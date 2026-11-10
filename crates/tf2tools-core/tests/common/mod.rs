//! Helpers shared by the integration tests. Each test binary uses a subset.
#![allow(dead_code)]

pub mod fake_host;
pub mod hud_anims;
pub mod hud_borrow;
pub mod hud_copy;
pub mod hud_fonts;
pub mod hud_pictures;
pub mod hud_schema;
pub mod hud_stock;
pub mod hud_switches;
pub mod sfnt;
pub mod update;

use rusqlite::Connection;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::TempDir;
use tf2tools_core::{
    AddonProfile, AddonSection, BuiltinCrosshairs, CrosshairEntry, DataDir, Database,
    DownloadError, DownloadProgress, Endpoints, GbAddonDownload, GbAddonFetchError, GbId, HudEntry,
    Library, LibraryEntry, ModuleCatalog, Provenance, SoundEntry, StoreClient, StoreConfig,
    StoreFile, StoreItemId, StoreKind, Tf2Install,
};
use wiremock::MockServer;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

/// A `DataDir` under a fresh temporary directory; its root does not exist yet.
/// Keep the `TempDir` alive for the whole test.
pub fn temp_data_dir() -> (TempDir, DataDir) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = DataDir::at(tmp.path().join("com.tf2tools.app"));
    (tmp, dir)
}

/// The schema of a real v5 database at `user_version` 9.
pub const V5_SCHEMA: &str = include_str!("../fixtures/v5-schema.sql");

/// (name, type, not null, default, primary-key position) from `pragma_table_info`.
pub type Column = (String, String, bool, Option<String>, i64);

/// Tables and indexes with their columns as sets, so column order does not matter.
#[derive(Debug, PartialEq, Eq)]
pub struct SchemaShape {
    pub objects: BTreeSet<(String, String, String)>,
    pub columns: BTreeMap<String, BTreeSet<Column>>,
}

pub fn schema_shape(conn: &Connection) -> SchemaShape {
    let mut stmt = conn
        .prepare("SELECT type, name, tbl_name FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'")
        .unwrap();
    let objects: BTreeSet<(String, String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let mut columns = BTreeMap::new();
    for (kind, name, _) in &objects {
        if kind != "table" {
            continue;
        }
        let mut info = conn
            .prepare("SELECT name, type, \"notnull\", dflt_value, pk FROM pragma_table_info(?1)")
            .unwrap();
        let cols: BTreeSet<Column> = info
            .query_map([name], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get::<_, i64>(2)? != 0,
                    r.get(3)?,
                    r.get(4)?,
                ))
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        columns.insert(name.clone(), cols);
    }
    SchemaShape { objects, columns }
}

/// An in-memory database built from the v5 fixture.
pub fn v5_fixture_connection() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(V5_SCHEMA).unwrap();
    conn
}

/// The indexes migration 10 adds, as (name, table).
pub const M10_INDEXES: [(&str, &str); 13] = [
    ("idx_store_sounds_likes", "store_sounds"),
    ("idx_store_sounds_views", "store_sounds"),
    ("idx_store_sounds_date", "store_sounds"),
    ("idx_store_sounds_cat_likes", "store_sounds"),
    ("idx_store_crosshairs_likes", "store_crosshairs"),
    ("idx_store_crosshairs_views", "store_crosshairs"),
    ("idx_store_crosshairs_date", "store_crosshairs"),
    ("idx_store_huds_gb_likes", "store_huds_gb"),
    ("idx_store_huds_gb_views", "store_huds_gb"),
    ("idx_store_huds_gb_date", "store_huds_gb"),
    ("idx_library_sounds_gb", "library_sounds"),
    ("idx_library_crosshairs_gb", "library_crosshairs"),
    ("idx_library_huds_gb", "library_huds"),
];

/// The tables migration 12 adds, as the schema should hold them.
const M12_TABLES: &str = "
    CREATE TABLE store_maps (
        name TEXT PRIMARY KEY,
        times_played INTEGER NOT NULL DEFAULT 0,
        first_played TEXT,
        last_played TEXT,
        size INTEGER NOT NULL DEFAULT 0,
        fetched_at TEXT NOT NULL
    );
    CREATE TABLE downloaded_maps (
        name TEXT PRIMARY KEY,
        size INTEGER NOT NULL,
        downloaded_at TEXT NOT NULL
    );
";

/// The tables migration 13 adds, as the schema should hold them.
const M13_TABLES: &str = "
    CREATE TABLE mastercomfig_state (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        version TEXT NOT NULL,
        preset TEXT NOT NULL,
        modules TEXT NOT NULL
    );
    CREATE TABLE mastercomfig_release (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        tag TEXT NOT NULL,
        base_url TEXT NOT NULL,
        override_urls TEXT NOT NULL
    );
";

/// The table migration 14 adds, as the schema should hold it.
const M14_TABLES: &str = "
    CREATE TABLE library_gb_addons (
        gb_id INTEGER PRIMARY KEY, section TEXT NOT NULL, name TEXT NOT NULL,
        author TEXT NOT NULL, file_id INTEGER NOT NULL, file_date INTEGER NOT NULL,
        latest_file_date INTEGER NOT NULL, checked_at INTEGER NOT NULL,
        payloads TEXT NOT NULL, files TEXT NOT NULL, installed INTEGER NOT NULL,
        gone INTEGER NOT NULL DEFAULT 0
    );
";

/// The v5 fixture's shape plus migration 10's indexes, plus migration 12's map tables,
/// migration 13's mastercomfig tables and migration 14's add-on table: the schema v6 opens to.
pub fn current_shape() -> SchemaShape {
    let conn = v5_fixture_connection();
    conn.execute_batch(M12_TABLES).unwrap();
    conn.execute_batch(M13_TABLES).unwrap();
    conn.execute_batch(M14_TABLES).unwrap();
    let mut shape = schema_shape(&conn);
    for (name, table) in M10_INDEXES {
        shape
            .objects
            .insert(("index".to_owned(), name.to_owned(), table.to_owned()));
    }
    shape
}

/// Writes a v5 database at `dir.db_path()`: the fixture schema in WAL mode, one or more
/// synthetic rows in every table, and v5's settings and sync checkpoint keys.
pub fn write_v5_database(dir: &DataDir) {
    std::fs::create_dir_all(dir.root()).unwrap();
    let conn = Connection::open(dir.db_path()).unwrap();
    conn.pragma_update_and_check(None, "journal_mode", "WAL", |r| r.get::<_, String>(0))
        .unwrap();
    conn.execute_batch(V5_SCHEMA).unwrap();
    conn.execute_batch(
        "PRAGMA foreign_keys=ON;
        INSERT INTO settings (key, value) VALUES
            ('tf2_path', '/games/Team Fortress 2'), ('volume', '35'), ('theme', 'light'),
            ('sync_cache_hours', '24'), ('installed_hud_folder', 'rayshud'), ('active_profile_id', 'p1');
        INSERT INTO library_huds (id, folder_name, name, source, store_hud_id, has_thumbnail, has_gallery,
            created_at, updated_at, author, store_gb_id)
            VALUES ('h1', 'rayshud', 'rayshud', 'store', 'rayshud', 1, 0, '2026-07-01T10:00:00Z', '2026-07-01T10:00:00Z', 'raysfire', NULL);
        INSERT INTO library_sounds (id, folder_name, name, original_filename, created_at, source, store_gb_id, sound_type)
            VALUES ('s1', 'boop', 'Boop', 'boop.wav', '2026-07-01T10:00:00Z', 'gamebanana', 12345, 'wav');
        INSERT INTO library_crosshairs (id, folder_name, name, source_type, created_at, source, store_gb_id)
            VALUES ('c1', 'dot', 'Dot', 'vtf', '2026-07-01T10:00:00Z', 'local', NULL);
        INSERT INTO crosshair_assignments (weapon_script, crosshair_source, crosshair_ref)
            VALUES ('tf_weapon_scattergun', 'builtin', 'ELAdot');
        INSERT INTO store_huds (hud_id, name, author, repo, hash, data_json, fetched_at)
            VALUES ('rayshud', 'rayshud', 'raysfire', 'raysfire/rayshud', 'abc', '{}', '2026-07-04T17:45:05Z');
        INSERT INTO store_cache_meta (key, value) VALUES
            ('sounds_sync_page', '12'), ('sounds_sync_start', '2026-07-04T17:44:56.764Z'),
            ('sounds_sync_fetched', '165'), ('huds_last_synced', '2026-07-04T17:45:57.430Z'),
            ('curated_last_synced', '2026-07-04T17:45:05.720Z');
        INSERT INTO logs (timestamp, level, category, action, message, details)
            VALUES ('2026-07-04T17:44:56.764Z', 'info', 'app', 'startup', 'TF2Tools started', NULL);
        INSERT INTO store_sounds (gb_id, name, fetched_at) VALUES (1, 'Ding', '2026-07-04T17:50:00Z');
        INSERT INTO store_crosshairs (gb_id, name, fetched_at) VALUES (2, 'Cross', '2026-07-04T17:45:00Z');
        INSERT INTO store_huds_gb (gb_id, name, fetched_at) VALUES (3, 'budhud', '2026-07-04T17:45:30Z');
        INSERT INTO profiles (id, name, hud_folder_name, created_at, updated_at)
            VALUES ('p1', 'Main', 'rayshud', '2026-07-01T10:00:00Z', '2026-07-01T10:00:00Z');
        INSERT INTO profile_crosshair_assignments (profile_id, weapon_script, crosshair_source, crosshair_ref)
            VALUES ('p1', 'tf_weapon_scattergun', 'builtin', 'ELAdot');
        INSERT INTO store_addons (asset_name, display_name, download_url, release_tag, fetched_at)
            VALUES ('mastercomfig-addon-flat-mouse.vpk', 'Flat mouse', 'https://example.invalid/a.vpk', '9.9.9', '2026-07-04T17:44:58Z');
        INSERT INTO library_addons (asset_name, display_name, installed_tag, created_at, updated_at)
            VALUES ('mastercomfig-addon-flat-mouse.vpk', 'Flat mouse', '9.9.9', '2026-07-01T10:00:00Z', '2026-07-01T10:00:00Z');",
    )
    .unwrap();
}

/// Every user table's full contents, rendered as text rows, keyed by table name.
pub fn dump_tables(conn: &Connection) -> BTreeMap<String, Vec<Vec<String>>> {
    let shape = schema_shape(conn);
    let mut out = BTreeMap::new();
    for table in shape.columns.keys() {
        let names: Vec<String> = shape.columns[table].iter().map(|c| c.0.clone()).collect();
        let select = names
            .iter()
            .map(|n| format!("quote(\"{n}\")"))
            .collect::<Vec<_>>()
            .join(", ");
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {select} FROM \"{table}\" ORDER BY {select}"
            ))
            .unwrap();
        let rows: Vec<Vec<String>> = stmt
            .query_map([], |r| {
                (0..names.len()).map(|i| r.get::<_, String>(i)).collect()
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        out.insert(table.clone(), rows);
    }
    out
}

/// A temporary TF2 game directory, `<tmp>/Team Fortress 2/tf/custom`; returns the game dir.
pub fn temp_tf2_tree() -> (TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let game = tmp.path().join("Team Fortress 2");
    std::fs::create_dir_all(game.join("tf/custom")).expect("tf2 tree");
    (tmp, game)
}

/// Bytes of a single-file (version 1) VPK holding `entries` as (path inside the pak, data).
/// Every entry's data follows the directory tree in the same file (archive index 0x7fff).
pub fn vpk_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut tree = Vec::new();
    let mut data = Vec::new();
    for &(path, bytes) in entries {
        let (dir, file) = path.rsplit_once('/').unwrap_or((" ", path));
        let (name, ext) = file.rsplit_once('.').expect("entry needs an extension");
        for part in [ext, dir, name] {
            tree.extend_from_slice(part.as_bytes());
            tree.push(0);
        }
        tree.extend_from_slice(&0u32.to_le_bytes()); // crc, not checked by readers
        tree.extend_from_slice(&0u16.to_le_bytes()); // preload length
        tree.extend_from_slice(&0x7fffu16.to_le_bytes());
        tree.extend_from_slice(&(data.len() as u32).to_le_bytes());
        tree.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        tree.extend_from_slice(&0xffffu16.to_le_bytes());
        tree.extend_from_slice(&[0, 0]); // end of names, end of paths
        data.extend_from_slice(bytes);
    }
    tree.push(0); // end of extensions
    let mut out = Vec::new();
    out.extend_from_slice(&0x55aa_1234u32.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&(tree.len() as u32).to_le_bytes());
    out.extend(tree);
    out.extend(data);
    out
}

/// A stock weapon script as TF2 ships it in `tf2_misc_dir.vpk`: plain and ICE-encrypted.
pub const STOCK_WEAPON_TXT: &[u8] = include_bytes!("../fixtures/crosshair/stock_weapon.txt");
pub const STOCK_WEAPON_CTX: &[u8] = include_bytes!("../fixtures/crosshair/stock_weapon.ctx");

/// Writes `tf/tf2_misc_dir.vpk` holding the encrypted stock script as `scripts/<name>.ctx` for
/// each of `names`, replacing any earlier one.
pub fn write_stock_scripts(tf2: &Tf2Install, names: &[&str]) {
    let paths: Vec<String> = names.iter().map(|n| format!("scripts/{n}.ctx")).collect();
    let entries: Vec<(&str, &[u8])> = paths
        .iter()
        .map(|p| (p.as_str(), STOCK_WEAPON_CTX))
        .collect();
    write_file(&tf2.tf_dir().join("tf2_misc_dir.vpk"), &vpk_bytes(&entries));
}

/// `STOCK_WEAPON_TXT` with its crosshair block pointed at `texture`, as apply writes it.
pub fn patched_stock_script(texture: &str) -> String {
    let stock = std::str::from_utf8(STOCK_WEAPON_TXT).unwrap();
    let start = stock.find("\"crosshair\"\r\n\t\t{").unwrap() + "\"crosshair\"\r\n\t\t".len();
    let end = stock[start..].find('}').unwrap() + start + 1;
    format!(
        "{}{{\n\t\t\t\"file\"\t\t\"vgui/replay/thumbnails/{texture}\"\n\t\t\t\"x\"\t\t\"0\"\n\t\t\t\"y\"\t\t\"0\"\n\t\t\t\"width\"\t\t\"64\"\n\t\t\t\"height\"\t\"64\"\n\t\t}}{}",
        &stock[..start],
        &stock[end..]
    )
}

const GAMEINFO_ONE_VPK: &str =
    r#""GameInfo" { FileSystem { SearchPaths { game+mod+vgui tf/tf2_misc.vpk } } }"#;

/// A `_dir.vpk` holding `path` whose header claims `claimed` bytes, though only 3 follow.
pub fn vpk_claiming(path: &str, claimed: u32) -> Vec<u8> {
    let mut bytes = vpk_bytes(&[(path, b"abc")]);
    // The length field follows the header, the three NUL-ended names (one byte longer than the
    // path) and crc, preload, archive and offset.
    let at = 12 + path.len() + 1 + 4 + 2 + 2 + 4;
    assert_eq!(
        bytes[at..at + 4],
        3u32.to_le_bytes(),
        "the vpk length field should hold the real 3 bytes before it is overwritten"
    );
    bytes[at..at + 4].copy_from_slice(&claimed.to_le_bytes());
    bytes
}

/// A game folder whose gameinfo lists one VPK, `tf/tf2_misc_dir.vpk`, holding `vpk`.
pub fn game_with_vpk(vpk: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let tf = dir.path().join("tf");
    write_file(&tf.join("gameinfo.txt"), GAMEINFO_ONE_VPK.as_bytes());
    write_file(&tf.join("tf2_misc_dir.vpk"), vpk);
    (dir, tf)
}

/// Bytes of a RAR archive of `version` holding `entries`, built by `rars` itself, so names
/// must be safe; `password` encrypts every entry.
pub fn rar_bytes(
    version: rars::ArchiveVersion,
    entries: &[(&str, &[u8])],
    password: Option<&[u8]>,
) -> Vec<u8> {
    let mut builder = rars::Builder::new(version).password(password.map(<[u8]>::to_vec));
    for &(name, bytes) in entries {
        builder
            .add_bytes(name.as_bytes().to_vec(), bytes.to_vec(), None, None)
            .unwrap();
    }
    let mut out = Vec::new();
    builder
        .write_to(&mut out, &rars::WriterResources::default(), None)
        .unwrap();
    out
}

/// CRC-32 (IEEE), bitwise; fixtures are tiny.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// What every entry of `rar4_stored` and `rar5_stored` holds.
pub const STORED_DATA: &[u8] = b"escaped\n";

/// A stored RAR4 archive with `names` written verbatim; `host` 2 is Windows, 3 Unix.
pub fn rar4_stored(names: &[&str], host: u8) -> Vec<u8> {
    let crc = crc32(STORED_DATA);
    let len = STORED_DATA.len() as u32;
    let mut out = b"Rar!\x1a\x07\x00".to_vec();
    let mut main = vec![0x73u8];
    main.extend(0u16.to_le_bytes());
    main.extend(13u16.to_le_bytes());
    main.extend(0u16.to_le_bytes());
    main.extend(0u32.to_le_bytes());
    out.extend(((crc32(&main) & 0xffff) as u16).to_le_bytes());
    out.extend(&main);
    for name in names {
        let n = name.as_bytes();
        let attr: u32 = if host == 3 { 0o100644 } else { 0x20 };
        let mut body = vec![0x74u8];
        body.extend(0x8000u16.to_le_bytes());
        body.extend((32 + n.len() as u16).to_le_bytes());
        body.extend(len.to_le_bytes());
        body.extend(len.to_le_bytes());
        body.push(host);
        body.extend(crc.to_le_bytes());
        body.extend(0x2100_0000u32.to_le_bytes());
        body.push(20);
        body.push(0x30);
        body.extend((n.len() as u16).to_le_bytes());
        body.extend(attr.to_le_bytes());
        body.extend(n);
        out.extend(((crc32(&body) & 0xffff) as u16).to_le_bytes());
        out.extend(&body);
        out.extend(STORED_DATA);
    }
    let end = [0x7bu8, 0x00, 0x40, 0x07, 0x00];
    out.extend(((crc32(&end) & 0xffff) as u16).to_le_bytes());
    out.extend(end);
    out
}

fn vint(mut v: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let c = (v & 0x7f) as u8;
        v >>= 7;
        out.push(c | if v != 0 { 0x80 } else { 0 });
        if v == 0 {
            return out;
        }
    }
}

fn rar5_header(body: &[u8]) -> Vec<u8> {
    let mut h = vint(body.len() as u64);
    h.extend(body);
    let mut out = crc32(&h).to_le_bytes().to_vec();
    out.extend(h);
    out
}

/// A stored RAR5 archive with `names` written verbatim; `host` 0 is Windows, 1 Unix.
pub fn rar5_stored(names: &[&str], host: u64) -> Vec<u8> {
    let crc = crc32(STORED_DATA);
    let len = STORED_DATA.len() as u64;
    let mut out = b"Rar!\x1a\x07\x01\x00".to_vec();
    out.extend(rar5_header(&[vint(1), vint(0), vint(0)].concat()));
    for name in names {
        let n = name.as_bytes();
        let attr = if host == 1 { 0o100644 } else { 0x20 };
        let body = [
            vint(2),
            vint(0x0002),
            vint(len),
            vint(0x0004),
            vint(len),
            vint(attr),
            crc.to_le_bytes().to_vec(),
            vint(0),
            vint(host),
            vint(n.len() as u64),
            n.to_vec(),
        ]
        .concat();
        out.extend(rar5_header(&body));
        out.extend(STORED_DATA);
    }
    out.extend(rar5_header(&[vint(5), vint(0), vint(0)].concat()));
    out
}

/// Bytes of a zip holding `entries`; a name ending in `/` becomes a directory entry.
pub fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    for &(name, bytes) in entries {
        if let Some(dir) = name.strip_suffix('/') {
            zip.add_directory(dir, options).unwrap();
        } else {
            zip.start_file(name, options).unwrap();
            zip.write_all(bytes).unwrap();
        }
    }
    zip.finish().unwrap().into_inner()
}

/// Writes a 7z archive at `path` holding `entries` as files.
pub fn write_7z(path: &std::path::Path, entries: &[(&str, &[u8])]) {
    let mut writer = sevenz_rust2::ArchiveWriter::create(path).unwrap();
    for &(name, bytes) in entries {
        writer
            .push_archive_entry(sevenz_rust2::ArchiveEntry::new_file(name), Some(bytes))
            .unwrap();
    }
    writer.finish().unwrap();
}

/// A canonical WAV: RIFF header, a 16-byte `fmt ` chunk, then `data_len` zero bytes of `data`.
pub fn wav_bytes(
    format_tag: u16,
    channels: u16,
    sample_rate: u32,
    bits: u16,
    data_len: u32,
) -> Vec<u8> {
    let block_align = channels * bits / 8;
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&format_tag.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * u32::from(block_align)).to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&bits.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.resize(out.len() + data_len as usize, 0);
    out
}

/// A library over a fresh database in a temporary data dir, and a temporary TF2 install
/// at `<tmp>/Team Fortress 2`. Keep the env alive for the whole test.
pub struct LibraryEnv {
    pub tmp: TempDir,
    pub dir: DataDir,
    pub lib: Library,
    pub tf2: Tf2Install,
}

pub fn library_env() -> LibraryEnv {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = DataDir::at(tmp.path().join("com.tf2tools.app"));
    let db = Database::open(&dir).expect("open database");
    let game = tmp.path().join("Team Fortress 2");
    std::fs::create_dir_all(game.join("tf/custom")).expect("tf2 tree");
    let tf2 = Tf2Install::from_path(&game).expect("tf2 install");
    let lib = Library::new(db, dir.clone());
    LibraryEnv { tmp, dir, lib, tf2 }
}

/// `library_env` whose `tf/tf2_misc_dir.vpk` holds a stock script for every assignable weapon,
/// so crosshair applies can patch full scripts.
pub fn crosshair_env() -> LibraryEnv {
    let env = library_env();
    let names: Vec<&str> = tf2tools_core::weapons()
        .iter()
        .map(|w| w.script_name())
        .collect();
    write_stock_scripts(&env.tf2, &names);
    env
}

/// A plain connection to the library database, for seeding store rows and reading columns.
pub fn sql(dir: &DataDir) -> Connection {
    Connection::open(dir.db_path()).expect("sqlite")
}

/// The value of a `settings` key, or None when the key is absent.
pub fn setting(dir: &DataDir, key: &str) -> Option<String> {
    sql(dir)
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
            r.get(0)
        })
        .ok()
}

pub fn set_setting(dir: &DataDir, key: &str, value: &str) {
    sql(dir)
        .execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            [key, value],
        )
        .expect("set setting");
}

/// Writes `bytes` at `path`, creating parent directories, and returns the path.
pub fn write_file(path: &Path, bytes: &[u8]) -> PathBuf {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(path, bytes).expect("write");
    path.to_path_buf()
}

/// Every file under `root` as a sorted `/`-separated relative path; empty when `root` is absent.
pub fn tree(root: &Path) -> Vec<String> {
    fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries {
            let entry = entry.expect("dir entry");
            let name = entry.file_name().to_string_lossy().into_owned();
            let rel = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            if entry.file_type().expect("file type").is_dir() {
                walk(&entry.path(), &rel, out);
            } else {
                out.push(rel);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, "", &mut out);
    out.sort();
    out
}

/// A recorded store response from `tests/fixtures/store/` (see `record.sh` beside it).
pub fn store_fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/store")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Production store behaviour without the waits: no request spacing, 1 ms 429 backoff.
pub fn fast_config() -> StoreConfig {
    StoreConfig::default()
        .with_request_spacing(Duration::ZERO)
        .with_retry_backoff(Duration::from_millis(1))
}

/// A fresh database and library in a temporary data dir, a wiremock server standing in
/// for GameBanana (`/apiv11`), the GitHub API (`/gh-api`), github.com (`/gh`) and preview
/// media (`/preview/`), and a store client pointed at it with `fast_config`.
pub struct StoreEnv {
    pub tmp: TempDir,
    pub dir: DataDir,
    pub lib: Library,
    pub server: MockServer,
    pub client: StoreClient,
}

pub async fn store_env() -> StoreEnv {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = DataDir::at(tmp.path().join("com.tf2tools.app"));
    let db = Database::open(&dir).expect("open database");
    let server = MockServer::start().await;
    let uri = server.uri();
    let endpoints = Endpoints::default()
        .with_gamebanana(format!("{uri}/apiv11"))
        .with_github_api(format!("{uri}/gh-api"))
        .with_github(format!("{uri}/gh"))
        .with_preview_prefixes([format!("{uri}/preview/")])
        .with_image_prefixes([format!("{uri}/img/")])
        .with_serveme(format!("{uri}/serveme"))
        .with_map_downloads(format!("{uri}/fastdl/maps"))
        .with_mastercomfig_data(format!("{uri}/gh-raw/mastercomfig"));
    let client = StoreClient::new(db.clone(), dir.clone(), endpoints).with_config(fast_config());
    let lib = Library::new(db, dir.clone());
    StoreEnv {
        tmp,
        dir,
        lib,
        server,
        client,
    }
}

/// The store's timestamp shape for `hours` before now.
pub fn hours_ago(hours: i64) -> String {
    (chrono::Utc::now() - chrono::TimeDelta::hours(hours))
        .to_rfc3339_opts(chrono::SecondsFormat::Micros, false)
}

/// Panics unless `text` is RFC 3339 UTC written as `+00:00` with microseconds.
pub fn assert_store_timestamp(text: &str) {
    assert_eq!(text.len(), 32, "{text}");
    assert!(text.ends_with("+00:00"), "{text}");
    chrono::DateTime::parse_from_rfc3339(text).expect("rfc 3339");
}

/// A `store_cache_meta` value, or None when the key is absent.
pub fn meta(dir: &DataDir, key: &str) -> Option<String> {
    sql(dir)
        .query_row(
            "SELECT value FROM store_cache_meta WHERE key = ?1",
            [key],
            |r| r.get(0),
        )
        .ok()
}

pub fn set_meta(dir: &DataDir, key: &str, value: &str) {
    sql(dir)
        .execute(
            "INSERT OR REPLACE INTO store_cache_meta (key, value) VALUES (?1, ?2)",
            [key, value],
        )
        .expect("set meta");
}

/// The number of rows in `table`.
pub fn count(dir: &DataDir, table: &str) -> i64 {
    sql(dir)
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .expect("count")
}

/// Lowercase hex md5 of `bytes`, as GameBanana's `_sMd5Checksum` carries it.
pub fn md5_hex(bytes: &[u8]) -> String {
    use md5::{Digest, Md5};
    Md5::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// True when `<data>/tmp` holds nothing: no scratch dir or partial download survived.
pub fn tmp_is_empty(dir: &DataDir) -> bool {
    std::fs::read_dir(dir.tmp_dir())
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(true)
}

/// Serves `body` at `/dl/<gb_id>` and an item-files answer listing it with `md5`, and
/// returns that file as `item_files` builds it.
pub async fn gb_file(
    env: &StoreEnv,
    kind: StoreKind,
    gb_id: u64,
    file_name: &str,
    body: Vec<u8>,
    md5: &str,
) -> StoreFile {
    let model = if kind == StoreKind::Sounds {
        "Sound"
    } else {
        "Mod"
    };
    let dl = format!("/dl/{gb_id}");
    Mock::given(method("GET"))
        .and(path(dl.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
        .mount(&env.server)
        .await;
    let listing = serde_json::json!({"_idRow": gb_id, "_sName": "item", "_aFiles": [{
        "_idRow": 1, "_sFile": file_name, "_nFilesize": 1,
        "_sDownloadUrl": format!("{}{dl}", env.server.uri()), "_sMd5Checksum": md5
    }]});
    Mock::given(method("GET"))
        .and(path(format!("/apiv11/{model}/{gb_id}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(listing))
        .mount(&env.server)
        .await;
    let id = StoreItemId::GameBanana(GbId::new(gb_id));
    env.client
        .item_files(kind, &id)
        .await
        .expect("item files")
        .remove(0)
}

/// Runs a download and collects its progress events.
pub async fn download_collecting(
    env: &StoreEnv,
    kind: StoreKind,
    file: &StoreFile,
) -> (
    Result<Vec<LibraryEntry>, DownloadError>,
    Vec<DownloadProgress>,
) {
    let seen = std::sync::Mutex::new(Vec::new());
    let result = env
        .client
        .download(kind, file, &|p: DownloadProgress| {
            seen.lock().unwrap().push(p)
        })
        .await;
    (result, seen.into_inner().unwrap())
}

/// Serves `body` at `/dl/addon/<gb_id>/<i>` and returns its ProfilePage `_aFiles` record:
/// dated `date`, scanned "clean", sized and checksummed from the body.
pub async fn gb_addon_file(
    env: &StoreEnv,
    gb_id: u64,
    i: usize,
    name: &str,
    body: &[u8],
    date: i64,
) -> serde_json::Value {
    let route = format!("/dl/addon/{gb_id}/{i}");
    Mock::given(method("GET"))
        .and(path(route.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body.to_vec()))
        .mount(&env.server)
        .await;
    serde_json::json!({
        "_idRow": 100 + i, "_sFile": name, "_nFilesize": body.len(), "_tsDateAdded": date,
        "_sMd5Checksum": md5_hex(body), "_sAvResult": "clean",
        "_sDownloadUrl": format!("{}{route}", env.server.uri()),
    })
}

/// Mounts Mod `gb_id`'s ProfilePage named `name` with `files` and reads it back.
pub async fn gb_addon_profile(
    env: &StoreEnv,
    gb_id: u64,
    name: &str,
    files: Vec<serde_json::Value>,
) -> AddonProfile {
    Mock::given(method("GET"))
        .and(path(format!("/apiv11/Mod/{gb_id}/ProfilePage")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "_idRow": gb_id, "_sName": name, "_aSubmitter": {"_sName": "vex"},
            "_sProfileUrl": format!("https://gamebanana.com/mods/{gb_id}"), "_aFiles": files,
        })))
        .mount(&env.server)
        .await;
    env.client
        .addon_catalog()
        .profile(GbId::new(gb_id))
        .await
        .expect("profile")
}

/// Downloads `profile` as a `section` add-on through `client`, collecting its progress.
pub async fn gb_addon_download(
    client: &StoreClient,
    section: &str,
    profile: &AddonProfile,
) -> (
    Result<GbAddonDownload, GbAddonFetchError>,
    Vec<DownloadProgress>,
) {
    let seen = std::sync::Mutex::new(Vec::new());
    let section = AddonSection::by_key(section).expect("section");
    let result = client
        .download_gb_addon(section, profile, &|p: DownloadProgress| {
            seen.lock().unwrap().push(p)
        })
        .await;
    (result, seen.into_inner().unwrap())
}

/// Requests the server received for downloads, as opposed to listings.
pub async fn download_requests(env: &StoreEnv) -> usize {
    env.server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path().starts_with("/dl/"))
        .count()
}

/// GameBanana provenance with no author, as profile rows read it back.
pub fn gb(id: u64) -> Provenance {
    Provenance::GameBanana {
        gb_id: GbId::new(id),
        author: String::new(),
    }
}

/// Imports a minimal HUD whose library folder is `folder`.
pub fn import_hud_as(env: &LibraryEnv, folder: &str, provenance: Provenance) -> HudEntry {
    let vdf = format!("{folder}/info.vdf");
    let res = format!("{folder}/resource/clientscheme.res");
    let bytes = zip_bytes(&[
        (vdf.as_str(), b"hud".as_slice()),
        (res.as_str(), b"scheme".as_slice()),
    ]);
    let archive = write_file(&env.tmp.path().join(format!("in/{folder}.zip")), &bytes);
    env.lib
        .import_hud(&archive, provenance)
        .expect("import hud")
}

/// Imports a short WAV sound named `name`.
pub fn import_wav_as(env: &LibraryEnv, name: &str, provenance: Provenance) -> SoundEntry {
    let src = write_file(
        &env.tmp.path().join(format!("in/{name}.wav")),
        &wav_bytes(1, 2, 44_100, 16, 8),
    );
    env.lib
        .import_sound(&src, name, provenance)
        .expect("import sound")
}

/// Imports a 16x16 PNG crosshair named `name`.
pub fn import_png_as(env: &LibraryEnv, name: &str, provenance: Provenance) -> CrosshairEntry {
    let img = image::RgbaImage::from_pixel(16, 16, image::Rgba([0, 255, 0, 255]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    img.write_to(&mut bytes, image::ImageFormat::Png)
        .expect("png");
    let src = write_file(
        &env.tmp.path().join(format!("in/{name}.png")),
        &bytes.into_inner(),
    );
    env.lib
        .import_crosshair(&src, name, provenance)
        .expect("import crosshair")
}

/// The builtin crosshair catalog bundled in the repo.
pub fn builtin_catalog() -> BuiltinCrosshairs {
    BuiltinCrosshairs::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/crosshairs"))
        .expect("catalog")
}

pub const V5_PROFILE_ID: &str = "9b2f6c1e-0000-4000-8000-000000000001";

/// A profile exactly as v5 writes it; its HUD and library crosshair are not in this library.
pub fn seed_v5_profile(env: &LibraryEnv) {
    sql(&env.dir)
        .execute_batch(
            "INSERT INTO profiles (id, name, hud_folder_name, hud_name, hud_source,
                 hud_store_gb_id, hud_store_hud_id, hitsound_folder_name, hitsound_name,
                 hitsound_source, hitsound_store_gb_id, killsound_folder_name, killsound_name,
                 killsound_source, killsound_store_gb_id, created_at, updated_at)
             VALUES ('9b2f6c1e-0000-4000-8000-000000000001', 'Comp', 'rayshud', 'rayshud',
                 'hud-db', NULL, 'rayshud', 'ding-1a2b3c4d', 'Ding', 'gamebanana', 7001,
                 NULL, NULL, NULL, NULL,
                 '2025-03-01T10:00:00+00:00', '2025-03-01T10:00:00+00:00');
             INSERT INTO profile_crosshair_assignments
                 (profile_id, weapon_script, crosshair_source, crosshair_ref, crosshair_store_gb_id)
             VALUES ('9b2f6c1e-0000-4000-8000-000000000001', 'tf_weapon_scattergun',
                     'builtin', 'cpma_9', NULL),
                    ('9b2f6c1e-0000-4000-8000-000000000001', 'tf_weapon_rocketlauncher',
                     'library', 'dot-5e6f7a8b', 9001);",
        )
        .expect("seed v5 profile");
}

/// A `mastercomfig-base.vpk` whose `cfg/comfig/echo.cfg` names `version` as a release does.
pub fn mastercomfig_base(version: &str) -> Vec<u8> {
    let echo =
        format!("alias version_comfig\"echo mastercomfig version: {version} | July 3, 2026\"\n");
    vpk_bytes(&[("cfg/comfig/echo.cfg", echo.as_bytes())])
}

/// Records mastercomfig as TF2Tools-managed at `version` on `preset`, as an install leaves it.
pub fn set_mastercomfig_state(dir: &DataDir, version: &str, preset: &str) {
    sql(dir)
        .execute(
            "INSERT OR REPLACE INTO mastercomfig_state (id, version, preset, modules)
             VALUES (1, ?1, ?2, '{}')",
            [version, preset],
        )
        .expect("state row");
}

/// mastercomfig 9.100.1's module list and preset levels, trimmed (see fixtures/mastercomfig/NOTICE.txt).
pub const MC_MODULES: &[u8] = include_bytes!("../fixtures/mastercomfig/modules.json");
pub const MC_PRESET_MODULES: &[u8] = include_bytes!("../fixtures/mastercomfig/preset_modules.json");

/// The fixture catalog, tagged 9.100.1.
pub fn module_catalog() -> ModuleCatalog {
    ModuleCatalog::parse("9.100.1", MC_MODULES, MC_PRESET_MODULES).expect("fixture catalog")
}
