//! A loopback stand-in for demos.tf, so a seeded QA boot shows a Demos.tf list and can open a
//! demo without the network. The shared fake serves it under `/demostf`: six fixed rows and
//! header-only demo files.

use std::path::Path;

use crate::demo_view::{BspOptions, bsp, demo_header};
use crate::fake_http;

/// The map most rows name; `seed_map` writes it to the fake game.
pub const SEEDED_MAP: &str = "cp_pilot";
/// The map one row names and the seed never writes, so the page shows a missing map.
pub const MISSING_MAP: &str = "koth_pilot";

/// The demos.tf routes of the shared fake on 127.0.0.1; it serves until the process exits.
pub struct DemosFake {
    base: String,
}

impl DemosFake {
    /// Starts the shared fake if needed; every call names the same listener.
    pub fn start() -> std::io::Result<DemosFake> {
        let root = fake_http::root()?;
        Ok(DemosFake {
            base: format!("{root}/demostf"),
        })
    }

    /// `http://127.0.0.1:<port>/demostf`, the value for `DemosEndpoints::loopback`.
    pub fn base(&self) -> &str {
        &self.base
    }
}

/// Writes `bsp(&BspOptions::default())` to `<game>/tf/maps/<SEEDED_MAP>.bsp`, creating dirs.
pub fn seed_map(game: &Path) -> std::io::Result<()> {
    let maps = game.join("tf/maps");
    std::fs::create_dir_all(&maps)?;
    std::fs::write(
        maps.join(format!("{SEEDED_MAP}.bsp")),
        bsp(&BspOptions::default()),
    )
}

/// Where a row's file lives on the fake.
#[derive(Clone, Copy, PartialEq)]
enum Store {
    Files,
    Freezer,
    Private,
}

struct Row {
    id: u64,
    map: &'static str,
    players: u32,
    store: Store,
}

const fn row(id: u64, map: &'static str, players: u32, store: Store) -> Row {
    Row {
        id,
        map,
        players,
        store,
    }
}

const ROWS: [Row; 6] = [
    row(9006, SEEDED_MAP, 12, Store::Files),
    row(9005, SEEDED_MAP, 12, Store::Freezer),
    row(9004, MISSING_MAP, 8, Store::Files),
    row(9003, SEEDED_MAP, 12, Store::Private),
    row(9002, SEEDED_MAP, 18, Store::Files),
    row(9001, SEEDED_MAP, 12, Store::Files),
];

/// Every row runs this long, in seconds.
const DURATION: u32 = 1800;
// Routes are matched exactly on the path after `/demostf`; the list honours only `before_id`.
pub(crate) fn answer(path: &str, query: &str, base: &str) -> (&'static str, &'static str, Vec<u8>) {
    let not_found = ("404 Not Found", "text/plain", b"not found".to_vec());
    if path == "/demos/" {
        let before = query
            .split('&')
            .find_map(|pair| pair.strip_prefix("before_id="))
            .and_then(|v| v.parse::<u64>().ok());
        let json = list_json(base, before);
        return ("200 OK", "application/json", json.into_bytes());
    }
    let file = path
        .strip_prefix("/files/")
        .map(|id| (Store::Files, id))
        .or_else(|| {
            path.strip_prefix("/freezer/")
                .map(|id| (Store::Freezer, id))
        });
    let row = file.and_then(|(store, name)| {
        let id = name.strip_suffix(".dem")?.parse::<u64>().ok()?;
        ROWS.iter().find(|r| r.id == id && r.store == store)
    });
    match row {
        Some(row) => (
            "200 OK",
            "application/octet-stream",
            demo_header(row.map, DURATION as f32),
        ),
        None => not_found,
    }
}

// Every string is fixed ASCII, so `format!` needs no escaping.
fn list_json(base: &str, before: Option<u64>) -> String {
    let now = fake_http::start_time();
    let rows: Vec<String> = ROWS
        .iter()
        .enumerate()
        .filter(|(_, row)| before.is_none_or(|b| row.id < b))
        .map(|(k, row)| {
            let (url, private_until) = match row.store {
                Store::Files => (format!("{base}/files/{}.dem", row.id), 0),
                Store::Freezer => (format!("{base}/freezer/{}.dem", row.id), 0),
                Store::Private => (String::new(), now + 86_400),
            };
            format!(
                r#"{{"id":{id},"url":"{url}","name":"pilot {id}","map":"{map}","time":{time},"duration":{DURATION},"server":"Pilot Server","red":"RED","blue":"BLU","playerCount":{players},"private_until":{private_until}}}"#,
                id = row.id,
                map = row.map,
                time = now - 3600 * (k as i64 + 1),
                players = row.players,
            )
        })
        .collect();
    format!("[{}]", rows.join(","))
}
