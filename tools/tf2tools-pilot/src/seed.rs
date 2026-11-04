//! A small generated v5-compatible library for pilot runs and screenshots.

use std::f32::consts::TAU;
use std::fs;
use std::io::Write as _;
use std::path::Path;

use anyhow::Context as _;
use chrono::{SecondsFormat, TimeDelta, Utc};
use image::{Rgba, RgbaImage};
use tf2tools_core::{
    AddonName, BuiltinCrosshairs, CrosshairRef, DataDir, Database, DraftSlot, Library,
    ProfileDraft, Provenance, SoundKind, Tf2Install,
};

const HUDS: [&str; 3] = ["budhud", "flawhud", "rayshud"];
const INSTALLED_HUD: &str = "rayshud";
// Name and sine pitch in Hz; each sound gets its own pitch so they sound distinct.
const SOUNDS: [(&str, f32); 4] = [
    ("Classic Ding", 880.0),
    ("Beep", 1000.0),
    ("Quake Hit", 440.0),
    ("Bell", 660.0),
];
const HITSOUND: &str = "Classic Ding";
const KILLSOUND: &str = "Bell";
// v5's Addons screenshot shows these three mastercomfig addons installed at this release.
const ADDONS: [&str; 3] = [
    "mastercomfig-addon-no-pyroland.vpk",
    "mastercomfig-addon-no-soundscapes.vpk",
    "mastercomfig-addon-no-tutorial.vpk",
];
const ADDON_TAG: &str = "9.100.1";
// The Casual profile's picks, distinct from the installed setup so its badges differ.
const CASUAL_HUD: &str = "flawhud";
const CASUAL_HITSOUND: &str = "Beep";
// A curated store HUD the Arena profile names but the library lacks, for Missing Items QA.
const MISSING_HUD: (&str, &str) = ("arenahud", "Arena HUD");

/// Names one store (`curated`, `huds_gb`, `sounds`, `crosshairs` or `addons`) to seed with no
/// rows and no sync stamp, so its skeleton and loading cards show.
pub const EMPTY_STORE_VAR: &str = "TF2TOOLS_PILOT_EMPTY_STORE";
const STORE_KEYS: [&str; 5] = ["curated", "huds_gb", "sounds", "crosshairs", "addons"];

/// Writes a small v5-compatible library into the data dir at `data` and a fake TF2 game dir at `game`:
/// HUDs budhud, flawhud, rayshud (rayshud installed); sounds Classic Ding, Beep, Quake Hit, Bell
/// (Classic Ding as hitsound, Bell as killsound); crosshairs My Dot, My Circle; two assignments
/// (tf_weapon_scattergun -> the catalog's first builtin, tf_weapon_rocketlauncher -> My Dot) applied;
/// addons no-pyroland, no-soundscapes, no-tutorial at release 9.100.1, installed; tf2_path saved;
/// profiles Competitive (the seeded setup, active), Casual (flawhud, Beep), Vanilla (empty) and
/// Arena (the store HUD arenahud, which is not in the library);
/// 12 activity log rows over the last three days; store rows with cached images (see `seed_store`).
/// Does nothing when `data` already holds tf2tools.db.
pub fn seed(data: &Path, game: &Path, assets: &Path) -> anyhow::Result<()> {
    seed_with(data, game, assets, None)
}

/// Like `seed`, leaving the store keyed `empty_store` (one of `EMPTY_STORE_VAR`'s keys) with no
/// rows and no sync stamp; an unknown key is an error.
pub fn seed_with(
    data: &Path,
    game: &Path,
    assets: &Path,
    empty_store: Option<&str>,
) -> anyhow::Result<()> {
    if let Some(key) = empty_store.filter(|k| !STORE_KEYS.contains(k)) {
        anyhow::bail!(
            "TF2TOOLS_PILOT_EMPTY_STORE: unknown store {key:?}; expected one of {STORE_KEYS:?}"
        );
    }
    let dir = DataDir::at(data);
    if dir.db_path().exists() {
        return Ok(());
    }
    fs::create_dir_all(game.join("tf"))
        .with_context(|| format!("cannot create {}", game.display()))?;
    write_stock_hud(&game.join("tf"))?;
    let tf2 = Tf2Install::from_path(game)?;
    let builtins = BuiltinCrosshairs::load(&assets.join("crosshairs"))?;

    // Fixtures are generated beside the data dir and removed once imported.
    let parent = data.parent().unwrap_or(data);
    let scratch = parent.join(".tf2tools-seed-scratch");
    fs::create_dir_all(&scratch).with_context(|| format!("cannot create {}", scratch.display()))?;
    let seeded = fill(&dir, &tf2, &builtins, &scratch, empty_store);
    let cleaned = fs::remove_dir_all(&scratch)
        .with_context(|| format!("cannot remove {}", scratch.display()));
    seeded.and(cleaned)
}

// Imports and installs every seeded item, writing fixture files into `scratch`.
fn fill(
    dir: &DataDir,
    tf2: &Tf2Install,
    builtins: &BuiltinCrosshairs,
    scratch: &Path,
    empty_store: Option<&str>,
) -> anyhow::Result<()> {
    let db = Database::open(dir)?;
    let lib = Library::new(db.clone(), dir.clone());

    let mut casual = ProfileDraft::new("Casual");
    for name in HUDS {
        let zip = scratch.join(format!("{name}.zip"));
        fs::write(&zip, hud_zip(name)?)?;
        let hud = lib.import_hud(&zip, Provenance::Local)?;
        if name == CASUAL_HUD {
            casual = casual.with_hud(DraftSlot::Item(hud.id().clone()));
        }
        if name == INSTALLED_HUD {
            lib.install_hud(hud.id(), tf2)?;
        }
    }

    for (name, pitch) in SOUNDS {
        let wav = scratch.join(format!("{}.wav", name.replace(' ', "-")));
        fs::write(&wav, sine_wav(pitch))?;
        let sound = lib.import_sound(&wav, name, Provenance::Local)?;
        if name == CASUAL_HITSOUND {
            casual = casual.with_hitsound(DraftSlot::Item(sound.id().clone()));
        }
        let kind = match name {
            HITSOUND => Some(SoundKind::Hitsound),
            KILLSOUND => Some(SoundKind::Killsound),
            _ => None,
        };
        if let Some(kind) = kind {
            lib.install_sound(sound.id(), kind, tf2)?;
        }
    }

    let dot = scratch.join("dot.png");
    crosshair_png(|d| d <= 6.0).save(&dot)?;
    let ring = scratch.join("ring.png");
    crosshair_png(|d| (18.0..=22.0).contains(&d)).save(&ring)?;
    let my_dot = lib.import_crosshair(&dot, "My Dot", Provenance::Local)?;
    lib.import_crosshair(&ring, "My Circle", Provenance::Local)?;

    let first = builtins
        .all()
        .first()
        .context("the builtin crosshair catalog is empty")?;
    lib.assign(
        "tf_weapon_scattergun",
        &CrosshairRef::Builtin(first.name().to_owned()),
    )?;
    lib.assign(
        "tf_weapon_rocketlauncher",
        &CrosshairRef::Library(my_dot.id().clone()),
    )?;
    lib.apply_crosshairs(tf2, builtins)?;

    for asset in ADDONS {
        let vpk = scratch.join(asset);
        fs::write(
            &vpk,
            vpk_bytes(&[("cfg/addons/seed.cfg", b"// seeded".as_slice())]),
        )?;
        let name = AddonName::new(asset).context("invalid seeded addon name")?;
        lib.import_addon(&vpk, &name, ADDON_TAG)?;
        lib.install_addon(&name, tf2)?;
    }

    // Applying Competitive reinstalls the setup it was saved from, so tf/custom is unchanged.
    let competitive = lib.save_current_profile("Competitive", tf2)?;
    lib.create_profile(&casual)?;
    lib.create_profile(&ProfileDraft::new("Vanilla"))?;
    lib.apply_profile(competitive.id(), tf2, builtins, false)?;

    // A draft can only name library items, so the missing store HUD is written as v5 left it.
    let arena = lib.create_profile(&ProfileDraft::new("Arena"))?;
    let (hud_id, hud_name) = MISSING_HUD;
    rusqlite::Connection::open(dir.db_path())?.execute(
        "UPDATE profiles SET hud_folder_name = ?2, hud_name = ?3, hud_store_hud_id = ?2
         WHERE id = ?1",
        [arena.id().as_str(), hud_id, hud_name],
    )?;

    db.set_tf2_path(Some(tf2.game_dir()))?;
    drop(db);
    seed_logs(dir)?;
    seed_store(dir, empty_store)
}

// Sound names cycle through these, numbered so the 60 rows stay distinct.
const SOUND_NAMES: [&str; 6] = [
    "Crisp Ding",
    "Arcade Blip",
    "Soft Click",
    "Bell Tone",
    "Retro Chime",
    "Low Thump",
];
const HUD_NAMES: [&str; 6] = [
    "Arena HUD",
    "Badwater HUD",
    "Granary HUD",
    "Harvest HUD",
    "Upward HUD",
    "Well HUD",
];
const AUTHORS: [&str; 6] = [
    "redbuilder",
    "bluspy",
    "payloadpal",
    "medicmain",
    "rocketjumper",
    "sentrynest",
];
// The curated HUDs' OS traits: every OS appears, and the Linux preset still shows rows.
const CURATED_OS: [&str; 3] = [
    r#"["windows","mac","linux"]"#,
    r#"["windows","linux"]"#,
    r#"["windows","mac","linux"]"#,
];

/// v5's Helltaker card's three tags, too long to share a row, as `tags_json`.
const LONG_TAGS_JSON: &str =
    r#"["Software Used: VTFEdit","Software Used: GIMP","Software Used: Paint.NET"]"#;

/// The tags a seeded GameBanana HUD or crosshair carries, as `tags_json`, by `id % 4`: three
/// long tags, one overlong tag, one short tag or none, so the first screen shows each wrap case.
fn gb_tags(id: i64) -> &'static str {
    match id % 4 {
        3 => LONG_TAGS_JSON,
        2 => r#"["Casual Compatibility: Casual Compatible"]"#,
        1 => r#"["Minimal"]"#,
        _ => "[]",
    }
}

/// The tags a seeded sound carries, as `tags_json`: two on even ids, one on odd.
fn sound_tags(id: i64) -> &'static str {
    if id % 2 == 0 {
        r#"["Purity: Fun","Bitrate: 320 kbit/s"]"#
    } else {
        r#"["Purity: Fun"]"#
    }
}

/// Store rows for offline QA: 60 sounds, 24 GameBanana HUDs, 6 curated HUDs, 24 crosshairs and
/// 4 addons. Every image URL sits under the app's closed offline base and is pre-written into
/// the image cache, so the offline pilot shows thumbnails and never fetches. The store keyed
/// `empty` gets no rows and no stamp.
fn seed_store(dir: &DataDir, empty: Option<&str>) -> anyhow::Result<()> {
    let seeds = |key: &str| empty != Some(key);
    let base = tf2tools_app::app::CLOSED_BASE;
    let img = |kind: &str, id: &str| format!("{base}/img/{kind}/{id}.jpg");
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Micros, false);
    let mut urls = Vec::new();
    let mut conn = rusqlite::Connection::open(dir.db_path())?;
    let tx = conn.transaction()?;

    for id in (1..=60i64).filter(|_| seeds("sounds")) {
        let thumb = img("sounds", &id.to_string());
        let category = if id % 2 == 0 { "hitsound" } else { "killsound" };
        tx.execute(
            "INSERT INTO store_sounds (gb_id, name, author, category, preview_url, thumbnail_url,
                 like_count, view_count, tags_json, profile_url, date_added, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            rusqlite::params![
                id,
                format!("{} {id}", SOUND_NAMES[id as usize % SOUND_NAMES.len()]),
                AUTHORS[id as usize % AUTHORS.len()],
                category,
                format!("{base}/preview/{id}.mp3"),
                thumb,
                id * 7919 % 500,
                id * 104_729 % 200_000,
                sound_tags(id),
                format!("{base}/sounds/{id}"),
                1_700_000_000 - id,
                now
            ],
        )?;
        urls.push(thumb);
    }

    // GameBanana HUDs and crosshairs share a shape; each has a two-image gallery for the carousel.
    for (table, kind, name) in [
        ("store_huds_gb", "huds", "HUD"),
        ("store_crosshairs", "crosshairs", "Crosshair"),
    ]
    .into_iter()
    .filter(|(table, ..)| seeds(table.trim_start_matches("store_")))
    {
        for id in 1..=24i64 {
            let gallery = [img(kind, &format!("{id}-a")), img(kind, &format!("{id}-b"))];
            tx.execute(
                &format!(
                    "INSERT INTO {table} (gb_id, name, author, thumbnail_url, images_json,
                         like_count, view_count, tags_json, profile_url, date_added, fetched_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"
                ),
                rusqlite::params![
                    id,
                    format!("{name} {id:02}"),
                    AUTHORS[id as usize % AUTHORS.len()],
                    gallery[0],
                    format!(r#"["{}","{}"]"#, gallery[0], gallery[1]),
                    id * 7919 % 3000,
                    id * 104_729 % 500_000,
                    gb_tags(id),
                    format!("{base}/{kind}/{id}"),
                    1_700_000_000 - id,
                    now
                ],
            )?;
            urls.extend(gallery);
        }
    }

    // Curated screenshots come from core's hud-resources base, `{base}/img/hud-resources`.
    for (i, name) in HUD_NAMES.iter().enumerate().filter(|_| seeds("curated")) {
        let hud_id = name.to_lowercase().replace(' ', "");
        let data_json = format!(
            r#"{{"resources":["1","2"],"verified":true,"flags":["fonts","menus","scoreboards","crosshairs"],
                "traits":{{"os":{},"gamemode":["casual"],"res":["16:9"]}}}}"#,
            CURATED_OS[i % CURATED_OS.len()]
        );
        tx.execute(
            "INSERT INTO store_huds (hud_id, name, author, repo, hash, data_json, fetched_at)
             VALUES (?1, ?2, ?3, ?4, 'abc123', ?5, ?6)",
            rusqlite::params![
                hud_id,
                name,
                AUTHORS[i],
                format!("{base}/gh/curated/{hud_id}"),
                data_json,
                now
            ],
        )?;
        for res in ["1", "2"] {
            urls.push(format!("{base}/img/hud-resources/{hud_id}/{res}.webp"));
        }
    }

    for (i, asset) in ["no-pyroland", "no-soundscapes", "no-tutorial", "flat-mouse"]
        .iter()
        .enumerate()
        .filter(|_| seeds("addons"))
    {
        let asset = format!("mastercomfig-addon-{asset}.vpk");
        tx.execute(
            "INSERT INTO store_addons (asset_name, display_name, description, download_url,
                 file_size, download_count, release_tag, fetched_at)
             VALUES (?1, ?2, 'Utility addon', ?3, 2355, ?4, ?5, ?6)",
            rusqlite::params![
                asset,
                asset.trim_end_matches(".vpk"),
                format!("{base}/gh/addons/{asset}"),
                989 + i as i64,
                ADDON_TAG,
                now
            ],
        )?;
    }

    let upsert = "INSERT OR REPLACE INTO store_cache_meta (key, value) VALUES (?1, ?2)";
    for key in STORE_KEYS.into_iter().filter(|k| seeds(k)) {
        tx.execute(
            upsert,
            [format!("{key}_last_synced").as_str(), now.as_str()],
        )?;
    }
    if seeds("sounds") {
        tx.execute(upsert, ["sounds_sync_order", "newest"])?;
    }
    tx.commit()?;
    warm_images(dir, &urls)
}

// One JPEG per URL under the name core's disk cache looks up: the URL's lowercase hex SHA-256.
fn warm_images(dir: &DataDir, urls: &[String]) -> anyhow::Result<()> {
    use sha2::{Digest, Sha256};
    let cache = dir.image_cache_dir();
    fs::create_dir_all(&cache).with_context(|| format!("cannot create {}", cache.display()))?;
    for (i, url) in urls.iter().enumerate() {
        let name: String = Sha256::digest(url.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        fs::write(cache.join(name), thumbnail_jpeg(i)?)?;
    }
    Ok(())
}

// A 320x180 diagonal gradient in one of six colours picked by `i`, so neighbouring cards differ.
fn thumbnail_jpeg(i: usize) -> anyhow::Result<Vec<u8>> {
    const COLOURS: [[f32; 3]; 6] = [
        [200.0, 120.0, 40.0],
        [60.0, 130.0, 200.0],
        [90.0, 170.0, 90.0],
        [180.0, 70.0, 80.0],
        [150.0, 100.0, 190.0],
        [70.0, 160.0, 160.0],
    ];
    let colour = COLOURS[i % COLOURS.len()];
    let img = image::RgbImage::from_fn(320, 180, |x, y| {
        let shade = 0.4 + 0.6 * (x + y) as f32 / 500.0;
        image::Rgb(colour.map(|c| (c * shade) as u8))
    });
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Jpeg)?;
    Ok(out.into_inner())
}

// Minutes ago, level, category, action, message, details: oldest first, so ids follow time.
// Spread over minutes, hours and days so every relative-time unit shows.
type LogRow = (
    i64,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    Option<&'static str>,
);
const LOG_ROWS: [LogRow; 12] = [
    (
        3 * 1440 - 30,
        "info",
        "system",
        "startup",
        "TF2Tools started",
        Some("Database: /srv/pilot/data/tf2tools.db\nVersion: 6.0.0\nPlatform: linux"),
    ),
    (
        2 * 1440 + 200,
        "info",
        "hud",
        "import",
        "Imported HUD: budhud",
        None,
    ),
    (
        2 * 1440,
        "info",
        "hud",
        "install",
        "Installed HUD: rayshud",
        Some("Path: /games/Team Fortress 2/tf/custom/rayshud"),
    ),
    (
        1440 + 90,
        "warning",
        "store",
        "sync",
        "Synced 0 curated HUDs from hud-db",
        None,
    ),
    (
        1440 + 60,
        "info",
        "hud_store",
        "sync",
        "Synced 800 HUDs from GameBanana",
        None,
    ),
    (
        1440,
        "error",
        "hud_store",
        "download",
        "Failed to download HUD from store",
        Some("HTTP 503 Service Unavailable"),
    ),
    (
        7 * 60,
        "info",
        "sound",
        "import",
        "Imported sound: Classic Ding",
        None,
    ),
    (
        5 * 60,
        "info",
        "sound",
        "install",
        "Installed hitsound: Classic Ding",
        None,
    ),
    (
        3 * 60,
        "info",
        "crosshair",
        "import",
        "Imported crosshair: My Dot",
        None,
    ),
    (
        45,
        "info",
        "crosshair",
        "apply",
        "Applied 2 crosshair assignments",
        None,
    ),
    (12, "info", "settings", "update", "Settings updated", None),
    (
        2,
        "warning",
        "system",
        "startup",
        "TF2 path not found",
        None,
    ),
];

// Writes LOG_ROWS straight into the logs table, since the core log stamps every append with now.
fn seed_logs(dir: &DataDir) -> anyhow::Result<()> {
    let mut conn = rusqlite::Connection::open(dir.db_path())?;
    let tx = conn.transaction()?;
    let now = Utc::now();
    for (minutes, level, category, action, message, details) in LOG_ROWS {
        let stamp =
            (now - TimeDelta::minutes(minutes)).to_rfc3339_opts(SecondsFormat::Micros, false);
        tx.execute(
            "INSERT INTO logs (timestamp, level, category, action, message, details)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            (stamp, level, category, action, message, details),
        )?;
    }
    tx.commit()?;
    Ok(())
}

// A minimal HUD archive: `<name>/info.vdf` and `<name>/resource/ui/hudlayout.res`.
fn hud_zip(name: &str) -> anyhow::Result<Vec<u8>> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file(format!("{name}/info.vdf"), options)?;
    write!(zip, "\"{name}\"\n{{\n\t\"ui_version\"\t\"3\"\n}}\n")?;
    zip.start_file(format!("{name}/resource/ui/hudlayout.res"), options)?;
    write!(zip, "\"Resource/HudLayout.res\"\n{{\n}}\n")?;
    Ok(zip.finish()?.into_inner())
}

// The HUD Maker reads the game's own HUD files, so the fake game gets core's stock set in a
// `tf2_misc_dir.vpk` that `gameinfo.txt` names, as TF2 ships them.
fn write_stock_hud(tf: &Path) -> anyhow::Result<()> {
    let files = crate::hud_stock::stock();
    let entries: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_bytes()))
        .collect();
    fs::write(
        tf.join("gameinfo.txt"),
        "\"GameInfo\"\n{\n\tFileSystem\n\t{\n\t\tSearchPaths\n\t\t{\n\t\t\tgame+mod+vgui\ttf/tf2_misc.vpk\n\t\t}\n\t}\n}\n",
    )
    .with_context(|| format!("cannot write {}", tf.join("gameinfo.txt").display()))?;
    fs::write(tf.join("tf2_misc_dir.vpk"), vpk_bytes(&entries))
        .with_context(|| format!("cannot write {}", tf.join("tf2_misc_dir.vpk").display()))
}

// A single-archive VPK v1: a directory tree of (ext, dir, name) entries, then their data inline.
fn vpk_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut tree = Vec::new();
    let mut data = Vec::new();
    for &(path, bytes) in entries {
        let (dir, file) = path.rsplit_once('/').unwrap_or((" ", path));
        let (name, ext) = file.rsplit_once('.').expect("entry needs an extension");
        for part in [ext, dir, name] {
            tree.extend_from_slice(part.as_bytes());
            tree.push(0);
        }
        tree.extend_from_slice(&0u32.to_le_bytes());
        tree.extend_from_slice(&0u16.to_le_bytes());
        tree.extend_from_slice(&0x7fffu16.to_le_bytes());
        tree.extend_from_slice(&(data.len() as u32).to_le_bytes());
        tree.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        tree.extend_from_slice(&0xffffu16.to_le_bytes());
        tree.extend_from_slice(&[0, 0]);
        data.extend_from_slice(bytes);
    }
    tree.push(0);
    let mut out = Vec::new();
    out.extend_from_slice(&0x55aa_1234u32.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&(tree.len() as u32).to_le_bytes());
    out.extend(tree);
    out.extend(data);
    out
}

// 0.2 s of a sine at `pitch` Hz as 16-bit 44.1 kHz stereo PCM, behind a hand-written RIFF header.
fn sine_wav(pitch: f32) -> Vec<u8> {
    const RATE: u32 = 44_100;
    const CHANNELS: u16 = 2;
    const BYTES_PER_FRAME: u32 = 4;
    let frames = RATE / 5;
    let data_len = frames * BYTES_PER_FRAME;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&CHANNELS.to_le_bytes());
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * BYTES_PER_FRAME).to_le_bytes());
    out.extend_from_slice(&(BYTES_PER_FRAME as u16).to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..frames {
        let t = i as f32 / RATE as f32;
        let sample = ((TAU * pitch * t).sin() * 0.3 * f32::from(i16::MAX)) as i16;
        for _ in 0..CHANNELS {
            out.extend_from_slice(&sample.to_le_bytes());
        }
    }
    out
}

// A 64x64 white-on-transparent crosshair: pixels whose distance from the centre `lit` accepts.
fn crosshair_png(lit: impl Fn(f32) -> bool) -> RgbaImage {
    RgbaImage::from_fn(64, 64, |x, y| {
        let d = (x as f32 - 31.5).hypot(y as f32 - 31.5);
        if lit(d) {
            Rgba([255, 255, 255, 255])
        } else {
            Rgba([0, 0, 0, 0])
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_seed_has_store_rows_and_cached_images() {
        let tmp = tempfile::tempdir().unwrap();
        let assets = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        seed(&tmp.path().join("data"), &tmp.path().join("game"), &assets).unwrap();
        let dir = tf2tools_core::DataDir::at(tmp.path().join("data"));
        let conn = rusqlite::Connection::open(dir.db_path()).unwrap();
        for (table, n) in [
            ("store_sounds", 60),
            ("store_huds_gb", 24),
            ("store_huds", 6),
            ("store_crosshairs", 24),
            ("store_addons", 4),
        ] {
            let rows: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(rows, n, "{table}");
        }
        assert!(std::fs::read_dir(dir.image_cache_dir()).unwrap().count() >= 24 + 24 + 6);
    }

    #[test]
    fn the_seed_varies_store_tags_for_the_wrap_checks() {
        let tmp = tempfile::tempdir().unwrap();
        let assets = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        seed(&tmp.path().join("data"), &tmp.path().join("game"), &assets).unwrap();
        let dir = tf2tools_core::DataDir::at(tmp.path().join("data"));
        let conn = rusqlite::Connection::open(dir.db_path()).unwrap();
        let tags = |table: &str, id: i64| -> String {
            conn.query_row(
                &format!("SELECT tags_json FROM {table} WHERE gb_id = ?1"),
                [id],
                |r| r.get(0),
            )
            .unwrap()
        };
        for table in ["store_huds_gb", "store_crosshairs"] {
            assert_eq!(tags(table, 3), LONG_TAGS_JSON, "{table}");
            assert_eq!(
                tags(table, 14),
                r#"["Casual Compatibility: Casual Compatible"]"#,
                "{table}"
            );
            assert_eq!(tags(table, 17), r#"["Minimal"]"#, "{table}");
            assert_eq!(tags(table, 20), "[]", "{table}");
        }
        assert_eq!(
            tags("store_sounds", 2),
            r#"["Purity: Fun","Bitrate: 320 kbit/s"]"#
        );
        assert_eq!(tags("store_sounds", 1), r#"["Purity: Fun"]"#);
    }
}
