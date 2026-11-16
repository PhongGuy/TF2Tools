//! Measures what each library kind takes: items are imported into a fresh library (HUDs and
//! GameBanana add-ons are stored packed), then one row per kind prints the item count, the
//! unpacked bytes and the bytes on disk, and for the plain kinds what a zstd-3 zip of each
//! item would take.
//!
//! Usage: `library_compress_sizes --data <dir> --game <dir> [--hud <zip>]... [--sound <wav|vpk>]...
//! [--crosshair <vtf|png>]... [--addon <mastercomfig-addon-*.vpk>]... [--gb-addon <archive>]...`.
//! Refuses a `--data` that holds `tf2tools.db` and a `--game` that is not empty.

use std::error::Error;
use std::fs::File;
use std::io::{self, Cursor};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use md5::{Digest, Md5};
use tf2tools_core::{
    AddonName, AddonSection, DataDir, Database, Endpoints, GbId, Library, LibraryKind, Provenance,
    StoreClient, StoreConfig, Tf2Install,
};
use walkdir::WalkDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const USAGE: &str = "usage: library_compress_sizes --data <dir> --game <dir> [--hud <zip>]... \
    [--sound <file>]... [--crosshair <file>]... [--addon <vpk>]... [--gb-addon <archive>]...";
/// Where a packed HUD and a packed add-on copy keep their files, under the item folder.
const HUD_ZIP: &str = ".tf2tools/files.zip";
const GB_ZIP: &str = "copy.zip";
/// The add-on ids the mock GameBanana serves, one per archive in argument order.
const FIRST_GB_ID: u64 = 900_001;

struct Args {
    data: PathBuf,
    game: PathBuf,
    huds: Vec<PathBuf>,
    sounds: Vec<PathBuf>,
    crosshairs: Vec<PathBuf>,
    addons: Vec<PathBuf>,
    gb_addons: Vec<PathBuf>,
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Args, String> {
    let (mut data, mut game) = (None, None);
    let (mut huds, mut sounds, mut crosshairs, mut addons, mut gb_addons) =
        (vec![], vec![], vec![], vec![], vec![]);
    while let Some(flag) = args.next() {
        let value = PathBuf::from(args.next().ok_or(format!("{flag} needs a value"))?);
        match flag.as_str() {
            "--data" => data = Some(value),
            "--game" => game = Some(value),
            "--hud" => huds.push(value),
            "--sound" => sounds.push(value),
            "--crosshair" => crosshairs.push(value),
            "--addon" => addons.push(value),
            "--gb-addon" => gb_addons.push(value),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(Args {
        data: data.ok_or("--data is required")?,
        game: game.ok_or("--game is required")?,
        huds,
        sounds,
        crosshairs,
        addons,
        gb_addons,
    })
}

/// Errors unless `data` is fresh and `game` is missing or empty, so a run never touches a
/// real library or game folder.
fn refuse_used_dirs(args: &Args) -> Result<(), String> {
    let db = DataDir::at(&args.data).db_path();
    if db.exists() {
        return Err(format!(
            "{} already exists; --data must be fresh",
            db.display()
        ));
    }
    let used = std::fs::read_dir(&args.game).is_ok_and(|mut d| d.next().is_some());
    if used {
        return Err(format!("{} is not empty", args.game.display()));
    }
    Ok(())
}

/// The loose bytes of a file or folder and the size of a zstd-3 zip of it, written to memory.
fn zipped_size(path: &Path) -> io::Result<(u64, u64)> {
    let base = path.parent().unwrap_or(path);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Zstd)
        .compression_level(Some(3));
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let mut loose = 0;
    for entry in WalkDir::new(path).sort_by_file_name() {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let name = entry.path().strip_prefix(base).unwrap_or(entry.path());
        zip.start_file(name.to_string_lossy(), options)
            .map_err(io::Error::other)?;
        loose += io::copy(&mut File::open(entry.path())?, &mut zip)?;
    }
    let zipped = zip.finish().map_err(io::Error::other)?.into_inner().len();
    Ok((loose, zipped as u64))
}

/// One kind's totals; `zipped` is set only for kinds stored loose.
#[derive(Default)]
struct KindSize {
    items: usize,
    unpacked: u64,
    on_disk: u64,
    zipped: Option<u64>,
}

/// Sums the items under `dir`. An item holding `item_zip` counts that zip's entries unpacked;
/// with `zip_plain` each item is also zipped in memory.
fn kind_size(dir: &Path, item_zip: Option<&str>, zip_plain: bool) -> io::Result<KindSize> {
    let mut size = KindSize {
        zipped: zip_plain.then_some(0),
        ..KindSize::default()
    };
    let Ok(items) = std::fs::read_dir(dir) else {
        return Ok(size);
    };
    for item in items {
        let item = item?.path();
        size.items += 1;
        for entry in WalkDir::new(&item) {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }
            let len = entry.metadata()?.len();
            size.on_disk += len;
            let packed = item_zip.is_some_and(|z| entry.path() == item.join(z));
            size.unpacked += if packed {
                unpacked_bytes(entry.path())?
            } else {
                len
            };
        }
        if let Some(zipped) = size.zipped.as_mut() {
            *zipped += zipped_size(&item)?.1;
        }
    }
    Ok(size)
}

fn unpacked_bytes(zip: &Path) -> io::Result<u64> {
    let mut archive = ZipArchive::new(File::open(zip)?).map_err(io::Error::other)?;
    let mut bytes = 0;
    for i in 0..archive.len() {
        bytes += archive.by_index_raw(i).map_err(io::Error::other)?.size();
    }
    Ok(bytes)
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map_or_else(|| "item".to_owned(), |s| s.to_string_lossy().into_owned())
}

fn md5_hex(bytes: &[u8]) -> String {
    Md5::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Serves `archive` as add-on `gb_id` from a loopback GameBanana, downloads it as an Effects
/// add-on and installs every payload it offers.
async fn install_gb_archive(
    server: &MockServer,
    client: &StoreClient,
    library: &Library,
    tf2: &Tf2Install,
    gb_id: u64,
    archive: &Path,
) -> Result<(), Box<dyn Error>> {
    let body = std::fs::read(archive)?;
    let route = format!("/dl/addon/{gb_id}/0");
    Mock::given(method("GET"))
        .and(path(route.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone()))
        .mount(server)
        .await;
    let file = serde_json::json!({
        "_idRow": 100, "_sFile": archive.file_name().map(|n| n.to_string_lossy()),
        "_nFilesize": body.len(), "_tsDateAdded": 100, "_sMd5Checksum": md5_hex(&body),
        "_sAvResult": "clean", "_sDownloadUrl": format!("{}{route}", server.uri()),
    });
    Mock::given(method("GET"))
        .and(path(format!("/apiv11/Mod/{gb_id}/ProfilePage")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "_idRow": gb_id, "_sName": stem(archive), "_aSubmitter": {"_sName": "measure"},
            "_sProfileUrl": format!("https://gamebanana.com/mods/{gb_id}"), "_aFiles": [file],
        })))
        .mount(server)
        .await;
    let profile = client.addon_catalog().profile(GbId::new(gb_id)).await?;
    let section = AddonSection::by_key("effects").ok_or("no effects section")?;
    let download = client.download_gb_addon(section, &profile, &|_| {}).await?;
    let chosen: Vec<String> = download
        .payloads()
        .iter()
        .map(|p| p.name().to_owned())
        .collect();
    library.install_gb_addon(download, &chosen, tf2, &|| false)?;
    Ok(())
}

async fn run(args: &Args) -> Result<(), Box<dyn Error>> {
    refuse_used_dirs(args)?;
    let dir = DataDir::at(&args.data);
    std::fs::create_dir_all(dir.root())?;
    let db = Database::open(&dir)?;
    let library = Library::new(db.clone(), dir.clone());
    std::fs::create_dir_all(args.game.join("tf").join("custom"))?;
    let tf2 = Tf2Install::from_path(&args.game)?;

    // A refused import is reported and skipped, so one odd file does not end the run.
    let skip = |what: &str, path: &Path, e: &dyn Error| {
        println!("skipped {what} {}: {e}", path.display());
    };
    for hud in &args.huds {
        if let Err(e) = library.import_hud(hud, Provenance::Local) {
            skip("hud", hud, &e);
        }
    }
    for sound in &args.sounds {
        if let Err(e) = library.import_sound(sound, &stem(sound), Provenance::Local) {
            skip("sound", sound, &e);
        }
    }
    for crosshair in &args.crosshairs {
        if let Err(e) = library.import_crosshair(crosshair, &stem(crosshair), Provenance::Local) {
            skip("crosshair", crosshair, &e);
        }
    }
    for addon in &args.addons {
        let name = addon
            .file_name()
            .and_then(|n| AddonName::new(n.to_string_lossy()));
        let Some(name) = name else {
            println!("skipped addon {}: not an add-on name", addon.display());
            continue;
        };
        if let Err(e) = library.import_addon(addon, &name, "measure") {
            skip("addon", addon, &e);
        }
    }

    let server = MockServer::start().await;
    let endpoints = Endpoints::default().with_gamebanana(format!("{}/apiv11", server.uri()));
    let config = StoreConfig::default().with_request_spacing(std::time::Duration::ZERO);
    let client = StoreClient::new(db, dir.clone(), endpoints).with_config(config);
    for (gb_id, archive) in (FIRST_GB_ID..).zip(&args.gb_addons) {
        let installed = install_gb_archive(&server, &client, &library, &tf2, gb_id, archive);
        if let Err(e) = installed.await {
            skip("gb-addon", archive, e.as_ref());
        }
    }

    let library_root = dir.root().join("library");
    let kinds = [
        (
            "huds",
            dir.library_dir(LibraryKind::Huds),
            Some(HUD_ZIP),
            false,
        ),
        ("sounds", dir.library_dir(LibraryKind::Sounds), None, true),
        (
            "crosshairs",
            dir.library_dir(LibraryKind::Crosshairs),
            None,
            true,
        ),
        ("addons", dir.library_dir(LibraryKind::Addons), None, true),
        (
            "gb-addons",
            library_root.join("gb-addons"),
            Some(GB_ZIP),
            false,
        ),
    ];
    println!("kind\titems\tunpacked\ton_disk\tzipped\tratio");
    for (name, path, item_zip, zip_plain) in kinds {
        let size = kind_size(&path, item_zip, zip_plain)?;
        let stored = size.zipped.unwrap_or(size.on_disk);
        let ratio = size.unpacked as f64 / stored.max(1) as f64;
        let zipped = size
            .zipped
            .map_or_else(|| "-".to_owned(), |z| z.to_string());
        println!(
            "{name}\t{}\t{}\t{}\t{zipped}\t{ratio:.2}x",
            size.items, size.unpacked, size.on_disk
        );
    }
    let space = library.space()?;
    println!(
        "Library::space: on disk {}, unpacked {}",
        space.on_disk(),
        space.unpacked()
    );
    Ok(())
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> ExitCode {
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    match run(&args).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("library_compress_sizes: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_flags_collect_per_kind() {
        let args = parse_args(
            [
                "--data",
                "d",
                "--game",
                "g",
                "--hud",
                "a.zip",
                "--hud",
                "b.zip",
                "--gb-addon",
                "c.7z",
            ]
            .into_iter()
            .map(String::from),
        )
        .unwrap();
        assert_eq!(args.huds, [PathBuf::from("a.zip"), PathBuf::from("b.zip")]);
        assert_eq!(args.gb_addons, [PathBuf::from("c.7z")]);
        assert!(args.sounds.is_empty() && args.crosshairs.is_empty() && args.addons.is_empty());
    }

    #[test]
    fn a_folder_or_a_file_zips_with_zstd() {
        let t = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(t.path().join("item")).unwrap();
        std::fs::write(t.path().join("item/a.wav"), vec![0u8; 100_000]).unwrap();
        let (loose, zipped) = zipped_size(&t.path().join("item")).unwrap();
        assert_eq!(loose, 100_000);
        assert!(zipped < 10_000, "{zipped}");
        let (loose, _) = zipped_size(&t.path().join("item/a.wav")).unwrap();
        assert_eq!(loose, 100_000);
    }
}
