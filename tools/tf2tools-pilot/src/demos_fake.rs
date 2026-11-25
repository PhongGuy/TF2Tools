//! A loopback stand-in for demos.tf, so a seeded QA boot shows a Demos.tf list and can open a
//! demo without the network. It serves six fixed rows and header-only demo files.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::demo_view::{BspOptions, bsp, demo_header};

/// The map most rows name; `seed_map` writes it to the fake game.
pub const SEEDED_MAP: &str = "cp_pilot";
/// The map one row names and the seed never writes, so the page shows a missing map.
pub const MISSING_MAP: &str = "koth_pilot";

/// A running fake on 127.0.0.1; its thread serves until the process exits.
pub struct DemosFake {
    base: String,
}

impl DemosFake {
    /// Binds 127.0.0.1:0 and serves on one detached thread until the process exits.
    pub fn start() -> std::io::Result<DemosFake> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let base = format!("http://{}/demostf", listener.local_addr()?);
        let served = base.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                // One connection at a time; a failed one is dropped and never stops the loop.
                let _ = serve(stream, &served);
            }
        });
        Ok(DemosFake { base })
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
/// Bounds the request head, so a client that never ends it cannot grow the buffer.
const HEAD_LIMIT: usize = 8 * 1024;

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

// Reads the head up to its blank line (it may span several reads) under a read timeout, so an
// idle client cannot hold the only thread, then answers and closes.
fn serve(mut stream: TcpStream, base: &str) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    let mut head = Vec::new();
    let mut chunk = [0u8; 1024];
    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = stream.read(&mut chunk)?;
        if n == 0 || head.len() + n > HEAD_LIMIT {
            return Ok(());
        }
        head.extend_from_slice(&chunk[..n]);
    }
    let head = String::from_utf8_lossy(&head);
    let target = head
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("GET "))
        .and_then(|rest| rest.split(' ').next())
        .unwrap_or("");
    let (status, kind, body) = answer(target, base);
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(&body)?;
    stream.flush()
}

// Routes are matched exactly after `/demostf`; the list honours only `before_id`.
fn answer(target: &str, base: &str) -> (&'static str, &'static str, Vec<u8>) {
    let not_found = ("404 Not Found", "text/plain", b"not found".to_vec());
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let Some(path) = path.strip_prefix("/demostf") else {
        return not_found;
    };
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
    let now = now();
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
