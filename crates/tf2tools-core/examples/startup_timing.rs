//! Times the cold path from launch to a loaded store: open (and migrate) the database, then read
//! every store's cached items. Prints one summary line per scenario.
//!
//! Usage: `startup_timing --v5-db <dir> [--runs 10] [--scratch <dir>]`, where `<dir>` holds a v5
//! `tf2tools.db` and its optional `-wal`/`-shm` files. The v5 files are only ever copied.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tf2tools_core::{DataDir, Database, Endpoints, StoreClient, StoreKind};

const DB_FILES: [&str; 3] = ["tf2tools.db", "tf2tools.db-wal", "tf2tools.db-shm"];
const SYNTHETIC_SOUNDS: usize = 7515;

/// The median of `samples`; with an even count, the mean of the two middle values.
/// Panics on an empty slice.
fn median(samples: &[Duration]) -> Duration {
    assert!(!samples.is_empty(), "median of no samples");
    let mut sorted = samples.to_vec();
    sorted.sort();
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted[mid]
    } else {
        (sorted[mid - 1] + sorted[mid]) / 2
    }
}

/// Creates `dst`'s root and copies `tf2tools.db` from `src` into it, plus `-wal`/`-shm` when
/// present. Fails when `tf2tools.db` itself is missing.
fn copy_v5_database(src: &Path, dst: &DataDir) -> std::io::Result<()> {
    std::fs::create_dir_all(dst.root())?;
    for (i, name) in DB_FILES.iter().enumerate() {
        let from = src.join(name);
        // The database is required; the WAL and shared-memory files exist only after an unclean close.
        if i == 0 || from.exists() {
            std::fs::copy(&from, dst.root().join(name))?;
        }
    }
    Ok(())
}

/// Creates the current schema in `dir` and fills `store_sounds` with `n` plausible rows.
fn seed_synthetic_sounds(dir: &DataDir, n: usize) -> Result<(), Box<dyn Error>> {
    drop(Database::open(dir)?);
    let mut conn = rusqlite::Connection::open(dir.db_path())?;
    let tx = conn.transaction()?;
    {
        let mut insert = tx.prepare(
            "INSERT INTO store_sounds (gb_id, name, author, category, preview_url, thumbnail_url,
                 like_count, view_count, tags_json, profile_url, date_added, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        )?;
        for i in 0..n {
            let id = 100_000 + i as i64;
            let category = if i % 2 == 0 { "hitsound" } else { "killsound" };
            insert.execute(rusqlite::params![
                id,
                format!("Synthetic sound {i}"),
                format!("author{}", i % 97),
                category,
                format!("https://files.example/{id}.mp3"),
                format!("https://images.example/{id}.jpg"),
                (i % 50) as i64,
                (i * 7 % 5000) as i64,
                r#"["synthetic"]"#,
                format!("https://gamebanana.com/sounds/{id}"),
                1_600_000_000 + id,
                "2026-10-16T12:00:00Z",
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// One cold open-and-load: how long each half took and the rows per store.
struct Load {
    open: Duration,
    load: Duration,
    /// Item counts in `StoreKind::ALL` order.
    rows: [usize; 5],
}

/// Times opening `dir`'s database and reading every store's items. The client sends no request, and both
/// it and the database are dropped on return so the next open starts cold.
fn open_and_load(dir: &DataDir) -> Result<Load, Box<dyn Error>> {
    let start = Instant::now();
    let db = Database::open(dir)?;
    let open = start.elapsed();

    // Building the client sets up an HTTP pool, which is not part of reading the cache.
    let client = StoreClient::new(db, dir.clone(), Endpoints::default());
    let start = Instant::now();
    let mut rows = [0; 5];
    for (count, kind) in rows.iter_mut().zip(StoreKind::ALL) {
        *count = client.store_items(kind)?.len();
    }
    let load = start.elapsed();
    Ok(Load { open, load, rows })
}

struct Args {
    v5_db: PathBuf,
    runs: usize,
    scratch: Option<PathBuf>,
}

fn parse_args() -> Result<Args, Box<dyn Error>> {
    let mut v5_db = None;
    let mut runs = 10;
    let mut scratch = None;
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--v5-db" => v5_db = Some(PathBuf::from(value()?)),
            "--runs" => runs = value()?.parse()?,
            "--scratch" => scratch = Some(PathBuf::from(value()?)),
            other => return Err(format!("unknown argument {other}").into()),
        }
    }
    let v5_db = v5_db.ok_or("usage: startup_timing --v5-db <dir> [--runs 10] [--scratch <dir>]")?;
    if runs == 0 {
        return Err("--runs must be at least 1".into());
    }
    Ok(Args {
        v5_db,
        runs,
        scratch,
    })
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn report(scenario: &str, loads: &[Load]) {
    let totals: Vec<Duration> = loads.iter().map(|l| l.open + l.load).collect();
    let opens: Vec<Duration> = loads.iter().map(|l| l.open).collect();
    let reads: Vec<Duration> = loads.iter().map(|l| l.load).collect();
    // Printed as sounds, curated HUDs, GameBanana HUDs, crosshairs, addons.
    let r = loads[0].rows;
    println!(
        "scenario={scenario} runs={} total_median_ms={:.3} total_min_ms={:.3} total_max_ms={:.3} \
         open_median_ms={:.3} load_median_ms={:.3} rows={},{},{},{},{}",
        loads.len(),
        ms(median(&totals)),
        ms(totals.iter().min().copied().unwrap_or_default()),
        ms(totals.iter().max().copied().unwrap_or_default()),
        ms(median(&opens)),
        ms(median(&reads)),
        r[2],
        r[0],
        r[1],
        r[3],
        r[4],
    );
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = parse_args()?;
    // Every scenario works on copies under the scratch root, so the v5 files stay untouched.
    let scratch = match &args.scratch {
        Some(dir) => {
            std::fs::create_dir_all(dir)?;
            tempfile::tempdir_in(dir)?
        }
        None => tempfile::tempdir()?,
    };
    let root = scratch.path();

    // First open: each run migrates a fresh copy; the copy itself is outside the timer.
    let mut loads = Vec::with_capacity(args.runs);
    for run in 0..args.runs {
        let dir = DataDir::at(root.join(format!("first-open-{run}")));
        copy_v5_database(&args.v5_db, &dir)?;
        loads.push(open_and_load(&dir)?);
    }
    report("v5-copy-first-open", &loads);

    // Reopen: migrate one copy untimed, then time repeated opens of the migrated file.
    let dir = DataDir::at(root.join("reopen"));
    copy_v5_database(&args.v5_db, &dir)?;
    open_and_load(&dir)?;
    let loads = (0..args.runs)
        .map(|_| open_and_load(&dir))
        .collect::<Result<Vec<_>, _>>()?;
    report("v5-copy-reopen", &loads);

    let dir = DataDir::at(root.join("synthetic"));
    seed_synthetic_sounds(&dir, SYNTHETIC_SOUNDS)?;
    let loads = (0..args.runs)
        .map(|_| open_and_load(&dir))
        .collect::<Result<Vec<_>, _>>()?;
    report("synthetic-7515", &loads);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tf2tools_core::{DataDir, StoreKind};

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn median_of_an_odd_sample_is_the_middle_value() {
        assert_eq!(median(&[ms(9), ms(1), ms(5)]), ms(5));
    }

    #[test]
    fn median_of_an_even_sample_is_the_mean_of_the_two_middles() {
        assert_eq!(median(&[ms(4), ms(1), ms(10), ms(2)]), ms(3));
    }

    #[test]
    fn copy_v5_database_copies_the_wal_and_tolerates_its_absence() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("tf2tools.db"), b"db").unwrap();
        std::fs::write(src.join("tf2tools.db-wal"), b"wal").unwrap();
        let with_wal = DataDir::at(tmp.path().join("a"));
        copy_v5_database(&src, &with_wal).unwrap();
        let root = tmp.path().join("a");
        assert_eq!(std::fs::read(root.join("tf2tools.db")).unwrap(), b"db");
        assert_eq!(std::fs::read(root.join("tf2tools.db-wal")).unwrap(), b"wal");
        assert!(!root.join("tf2tools.db-shm").exists());

        std::fs::remove_file(src.join("tf2tools.db-wal")).unwrap();
        let without = DataDir::at(tmp.path().join("b"));
        copy_v5_database(&src, &without).unwrap();
        assert!(!tmp.path().join("b").join("tf2tools.db-wal").exists());
    }

    #[test]
    fn a_synthetic_database_loads_every_seeded_sound() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = DataDir::at(tmp.path().join("synthetic"));
        seed_synthetic_sounds(&dir, 7515).unwrap();
        let load = open_and_load(&dir).unwrap();
        let sounds = StoreKind::ALL
            .iter()
            .position(|k| *k == StoreKind::Sounds)
            .unwrap();
        assert_eq!(load.rows[sounds], 7515);
        assert_eq!(load.rows.iter().sum::<usize>(), 7515);
        assert!(load.open + load.load > Duration::ZERO);
    }
}
