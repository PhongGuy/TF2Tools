//! A loopback stand-in for logs.tf and Steam's profile XML, so a seeded QA boot shows a Logs
//! page without the network. The shared fake serves it under `/logstf`: three synthetic 2v2
//! logs among four made-up players.

use tf2tools_core::SteamId;

use crate::demos_fake::SEEDED_MAP;
use crate::fake_http;

/// The four players every fake log holds: (Steam3 id, name).
pub const PLAYERS: [(&str, &str); 4] = [
    ("[U:1:4000000001]", "Pilot A"),
    ("[U:1:4000000002]", "Pilot B"),
    ("[U:1:4000000003]", "Pilot C"),
    ("[U:1:4000000004]", "Pilot D"),
];
/// A map no demos.tf fake row names, so its log's demo is missing.
pub const UNMATCHED_MAP: &str = "pl_pilot";
/// The newest fake log; it shares map and time with demo 9006.
pub const NEWEST_LOG: u64 = 7003;

/// The logs.tf routes of the shared fake on 127.0.0.1; it serves until the process exits.
pub struct LogsFake {
    base: String,
}

impl LogsFake {
    /// Starts the shared fake if needed; every call names the same listener.
    pub fn start() -> std::io::Result<LogsFake> {
        let root = fake_http::root()?;
        Ok(LogsFake {
            base: format!("{root}/logstf"),
        })
    }

    /// `http://127.0.0.1:<port>/logstf`, the value for `LogsEndpoints::loopback`.
    pub fn base(&self) -> &str {
        &self.base
    }
}

/// Every log runs this long, in seconds.
const LENGTH: u32 = 1800;

struct Log {
    id: u64,
    map: &'static str,
    /// Seconds before the fake's start time.
    age: i64,
}

// Newest first, as logs.tf lists them. 7003 sits on demo 9006's time; 7001 is far from any demo.
const LOGS: [Log; 3] = [
    Log {
        id: 7003,
        map: SEEDED_MAP,
        age: 3600,
    },
    Log {
        id: 7002,
        map: UNMATCHED_MAP,
        age: 7200,
    },
    Log {
        id: 7001,
        map: SEEDED_MAP,
        age: 30 * 3600,
    },
];

fn id64(steam3: &str) -> u64 {
    SteamId::from_steam3(steam3).map_or(0, SteamId::id64)
}

// Routes are matched exactly on the path after `/logstf`; the list honours `player` and `offset`.
pub(crate) fn answer(path: &str, query: &str) -> (&'static str, &'static str, Vec<u8>) {
    let not_found = ("404 Not Found", "text/plain", b"not found".to_vec());
    let json = |body: String| ("200 OK", "application/json", body.into_bytes());
    if path == "/api/v1/log" {
        let param = |key: &str| {
            query
                .split('&')
                .find_map(|pair| pair.strip_prefix(key)?.strip_prefix('='))
        };
        let player = param("player").and_then(|v| v.parse::<u64>().ok());
        let known = player.is_some_and(|p| PLAYERS.iter().any(|(s3, _)| id64(s3) == p));
        let first_page = param("offset").is_none_or(|v| v == "0");
        return json(list_json(known, first_page));
    }
    if let Some(id) = path.strip_prefix("/json/") {
        let log = id
            .parse::<u64>()
            .ok()
            .and_then(|id| LOGS.iter().find(|log| log.id == id));
        return match log {
            Some(log) => json(log_json(log)),
            None => not_found,
        };
    }
    if let Some(rest) = path.strip_prefix("/steam/profiles/") {
        let asked = rest.trim_end_matches('/').parse::<u64>().ok();
        let body = match asked {
            Some(id) if id == id64(PLAYERS[0].0) => format!(
                "<profile><steamID64>{id}</steamID64><steamID><![CDATA[{}]]></steamID></profile>",
                PLAYERS[0].1
            ),
            _ => "<response><error>The specified profile could not be found.</error></response>"
                .to_owned(),
        };
        return ("200 OK", "text/xml", body.into_bytes());
    }
    not_found
}

// Every string is fixed ASCII, so `format!` needs no escaping.
fn list_json(known: bool, first_page: bool) -> String {
    let start = fake_http::start_time();
    let total = if known { LOGS.len() } else { 0 };
    let rows: Vec<String> = if known && first_page {
        LOGS.iter()
            .map(|log| {
                format!(
                    r#"{{"id":{id},"title":"Pilot {id}","map":"{map}","date":{date}}}"#,
                    id = log.id,
                    map = log.map,
                    date = start - log.age,
                )
            })
            .collect()
    } else {
        Vec::new()
    };
    format!(r#"{{"total":{total},"logs":[{}]}}"#, rows.join(","))
}

// A and B play Red, C and D Blue; each splits the log evenly over two classes.
fn log_json(log: &Log) -> String {
    const CLASSES: [(&str, &str); 4] = [
        ("scout", "soldier"),
        ("demoman", "medic"),
        ("pyro", "engineer"),
        ("heavyweapons", "sniper"),
    ];
    let half = LENGTH / 2;
    let players: Vec<String> = PLAYERS
        .iter()
        .zip(CLASSES)
        .enumerate()
        .map(|(k, ((steam3, _), (first, second)))| {
            let team = if k < 2 { "Red" } else { "Blue" };
            let n = k as u32 + 1;
            let class = |kind: &str| {
                format!(
                    r#"{{"type":"{kind}","total_time":{half},"kills":{n},"deaths":1,"assists":1,"dmg":{dmg}}}"#,
                    dmg = 500 * n
                )
            };
            format!(
                r#""{steam3}":{{"team":"{team}","class_stats":[{},{}],"kills":{kills},"deaths":2,"assists":2,"dmg":{dmg}}}"#,
                class(first),
                class(second),
                kills = 2 * n,
                dmg = 1000 * n,
            )
        })
        .collect();
    let names: Vec<String> = PLAYERS
        .iter()
        .map(|(steam3, name)| format!(r#""{steam3}":"{name}""#))
        .collect();
    format!(
        r#"{{"length":{LENGTH},"teams":{{"Red":{{"score":3,"kills":6,"dmg":3000}},"Blue":{{"score":2,"kills":14,"dmg":7000}}}},"players":{{{players}}},"names":{{{names}}},"info":{{"title":"Pilot {id}","map":"{map}","date":{date},"total_length":{LENGTH}}}}}"#,
        players = players.join(","),
        names = names.join(","),
        id = log.id,
        map = log.map,
        date = fake_http::start_time() - log.age,
    )
}
