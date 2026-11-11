//! The logs.tf page's wording and number formats.

use std::time::Duration;

use tf2tools_app::logs_sync::{SyncProgress, ring_fill};
use tf2tools_app::pages::logs_tf::text::{
    EMPTY_TEXT, NO_LOGS_TEXT, accuracy_text, count_text, duration_text, event_text, fetch_message,
    finding_text, medigun_name, one_decimal, progress_line, progress_text, range_label, reads_zero,
    resolve_message, ring_label, share_text, summary_text, sync_message, time_played_text, whole,
};
use tf2tools_core::{LogsRange, LogsSyncError, ResolveError};

#[test]
fn resolve_failures_read_as_the_spec_words_them() {
    assert_eq!(
        resolve_message(&ResolveError::NotFound),
        "We couldn't find that Steam profile."
    );
    let offline = ResolveError::Network {
        url: "u".into(),
        message: "m".into(),
    };
    assert_eq!(
        resolve_message(&offline),
        "Can't reach Steam. Check your internet connection."
    );
    let busy = ResolveError::RateLimited { url: "u".into() };
    assert_eq!(
        resolve_message(&busy),
        "Can't reach Steam. Check your internet connection."
    );
}

#[test]
fn sync_failures_and_progress_read_as_the_spec_words_them() {
    assert_eq!(
        progress_text(12, 87, Duration::ZERO),
        "Downloading logs: 12 of 87"
    );
    let url = || "u".to_owned();
    let cases = [
        (
            LogsSyncError::Network {
                url: url(),
                message: "m".into(),
            },
            "You're offline. Showing saved logs.",
        ),
        (
            LogsSyncError::RateLimited { url: url() },
            "logs.tf is busy. Try again in a few minutes.",
        ),
        (
            LogsSyncError::Http {
                url: url(),
                status: 503,
            },
            "logs.tf isn't responding right now. Try again later.",
        ),
        (
            LogsSyncError::TooLarge {
                url: url(),
                limit: 1,
            },
            "logs.tf isn't responding right now. Try again later.",
        ),
        (
            LogsSyncError::Malformed {
                url: url(),
                message: "m".into(),
            },
            "logs.tf sent something we didn't understand.",
        ),
        (
            LogsSyncError::Database {
                message: "m".into(),
            },
            "Couldn't save logs.tf data on this computer.",
        ),
    ];
    for (error, text) in cases {
        assert_eq!(sync_message(&error), text, "{error:?}");
    }
}

#[test]
fn grid_numbers_format_like_the_spec() {
    assert_eq!(time_played_text(0), "0m");
    assert_eq!(time_played_text(1559), "25m");
    assert_eq!(time_played_text(5400), "1h 30m");
    assert_eq!(one_decimal(0.6), "0.6");
    assert_eq!(one_decimal(2.666_666), "2.7");
    assert_eq!(whole(136.38), "136");
}

#[test]
fn the_summary_names_the_range_and_counts_its_logs() {
    assert_eq!(summary_text(LogsRange::Month1, 1), "Last month, 1 log");
    assert_eq!(
        summary_text(LogsRange::Months3, 87),
        "Last 3 months, 87 logs"
    );
    assert_eq!(summary_text(LogsRange::Months6, 2), "Last 6 months, 2 logs");
    assert_eq!(
        summary_text(LogsRange::Year1, 1_400),
        "Last year, 1,400 logs"
    );
    assert_eq!(summary_text(LogsRange::Years2, 12), "Last 2 years, 12 logs");
    assert_eq!(summary_text(LogsRange::All, 4_812), "All logs, 4,812");
    let labels: Vec<&str> = LogsRange::STOPS.into_iter().map(range_label).collect();
    assert_eq!(
        labels,
        [
            "1 month", "3 months", "6 months", "1 year", "2 years", "All"
        ]
    );
    assert!(!EMPTY_TEXT.contains("90 days"), "{EMPTY_TEXT}");
}

#[test]
fn no_wording_promises_the_last_90_days_from_today() {
    assert_eq!(NO_LOGS_TEXT, "No logs on logs.tf for this player yet.");
    assert_eq!(
        EMPTY_TEXT,
        "Track a player to see their class averages and logs from logs.tf."
    );
}

#[test]
fn durations_read_as_minutes_and_seconds() {
    assert_eq!(duration_text(1714), "28:34");
    assert_eq!(duration_text(59), "0:59");
    assert_eq!(duration_text(3600), "60:00");
}

#[test]
fn progress_reads_in_two_phases_with_an_estimate() {
    assert_eq!(count_text(0), "0");
    assert_eq!(count_text(999), "999");
    assert_eq!(count_text(4_812), "4,812");
    assert_eq!(count_text(1_234_567), "1,234,567");
    assert_eq!(finding_text(2_000, 4_812), "Finding logs: 2,000 of 4,812");
    let at = |secs| progress_text(120, 4_812, Duration::from_secs(secs));
    assert_eq!(
        at(9_600),
        "Downloading logs: 120 of 4,812, about 2 h 40 min left"
    );
    assert_eq!(at(3_600), "Downloading logs: 120 of 4,812, about 1 h left");
    assert_eq!(at(720), "Downloading logs: 120 of 4,812, about 12 min left");
    assert_eq!(at(60), "Downloading logs: 120 of 4,812, about 1 min left");
    assert_eq!(at(59), "Downloading logs: 120 of 4,812");
    assert_eq!(
        progress_line(SyncProgress::Finding {
            listed: 1_000,
            total: 4_812
        }),
        "Finding logs: 1,000 of 4,812"
    );
    assert_eq!(
        progress_line(SyncProgress::Downloading {
            done: 1,
            total: 2,
            left: Duration::ZERO
        }),
        "Downloading logs: 1 of 2"
    );
}

#[test]
fn the_ring_label_matches_the_store_rings() {
    assert_eq!(ring_label(None), "Syncing logs.tf...");
    assert_eq!(
        ring_label(Some(SyncProgress::Finding {
            listed: 0,
            total: 9
        })),
        "Syncing logs.tf..."
    );
    assert_eq!(
        ring_label(Some(SyncProgress::Downloading {
            done: 120,
            total: 1000,
            left: Duration::ZERO
        })),
        "Syncing logs.tf: 120 / 1000"
    );
}

#[test]
fn fetch_failures_read_in_player_words_or_leave_it_to_the_row() {
    use tf2tools_core::LogsFetchError;
    let network = LogsFetchError::Network {
        url: "u".into(),
        message: "m".into(),
    };
    assert_eq!(
        fetch_message(&network),
        Some("Can't load this log while you're offline.")
    );
    let server = LogsFetchError::Http {
        url: "u".into(),
        status: 503,
    };
    assert_eq!(
        fetch_message(&server),
        Some("logs.tf isn't responding right now. Try again later.")
    );
    let busy = LogsFetchError::RateLimited { url: "u".into() };
    assert_eq!(
        fetch_message(&busy),
        Some("logs.tf is busy. Try again in a few minutes.")
    );
    let cache = LogsFetchError::Cache {
        message: "m".into(),
    };
    assert_eq!(
        fetch_message(&cache),
        Some("Couldn't save logs.tf data on this computer.")
    );
    for row_says_it in [
        LogsFetchError::Gone,
        LogsFetchError::Unreadable,
        LogsFetchError::TooLarge {
            url: "u".into(),
            limit: 1,
        },
    ] {
        assert_eq!(fetch_message(&row_says_it), None, "{row_says_it:?}");
    }
}

#[test]
fn the_ring_fills_only_once_there_is_a_count() {
    let downloading = |done, total| {
        Some(SyncProgress::Downloading {
            done,
            total,
            left: Duration::ZERO,
        })
    };
    assert_eq!(ring_fill(downloading(1, 4)), Some(25.));
    assert_eq!(ring_fill(downloading(0, 0)), None, "nothing to count spins");
    let finding = Some(SyncProgress::Finding {
        listed: 0,
        total: 9,
    });
    assert_eq!(ring_fill(finding), None);
    assert_eq!(ring_fill(None), None);
}

#[test]
fn shares_and_accuracy_round_like_logs_tf() {
    assert_eq!(share_text(9, 19), "47%");
    assert_eq!(share_text(3_334, 3_896), "86%");
    assert_eq!(share_text(0, 0), "0%");
    assert_eq!(accuracy_text(133, 309), "43%");
    assert_eq!(accuracy_text(161, 290), "56%");
    assert_eq!(accuracy_text(0, 0), "-");
    assert_eq!(accuracy_text(5, 0), "-");
}

#[test]
fn arie_scout_weapons_read_like_logs_tf() {
    use tf2tools_app::pages::logs_tf::tables::weapon_rows;
    let log = tf2tools_core::parse_log(crate::support::logstf::SIXES).unwrap();
    let arie = log
        .players()
        .iter()
        .find(|p| p.steam_id() == crate::support::logstf::sid(crate::support::logstf::ARIE))
        .unwrap();
    let scout = &arie.classes()[0];
    assert_eq!(scout.class(), tf2tools_core::TfClass::Scout);
    let shown: Vec<(String, u32, String, u32, String, String)> = weapon_rows(arie, scout)
        .into_iter()
        .map(|r| {
            (
                r.name,
                r.kills,
                r.kill_share,
                r.dmg,
                r.dmg_share,
                r.accuracy,
            )
        })
        .collect();
    let row = |n: &str, k, ks: &str, d, ds: &str, a: &str| {
        (
            n.to_owned(),
            k,
            ks.to_owned(),
            d,
            ds.to_owned(),
            a.to_owned(),
        )
    };
    assert_eq!(
        shown,
        [
            row("scattergun", 7, "100%", 3_334, "86%", "56%"),
            row("pistol_scout", 0, "0%", 177, "5%", "10%"),
        ]
    );
    let pyro = arie
        .classes()
        .iter()
        .find(|c| c.class() == tf2tools_core::TfClass::Pyro)
        .unwrap();
    let flames = weapon_rows(arie, pyro)
        .into_iter()
        .find(|r| r.name == "flamethrower")
        .unwrap();
    assert_eq!(flames.accuracy, "-", "no shots, no accuracy");
}

#[test]
fn zero_cells_read_as_zero_whole_or_with_a_decimal() {
    assert!(reads_zero("0"));
    assert!(reads_zero("0.0"));
    assert!(!reads_zero("0.4"));
    assert!(!reads_zero("10"));
    assert!(!reads_zero("-"));
}

#[test]
fn events_read_in_player_words() {
    use tf2tools_core::{EventKind, Medigun, STEAM_ID64_BASE, SteamId, Team};
    let p = |n: u64| SteamId::from_id64(STEAM_ID64_BASE + n).unwrap();
    let name_of = |id: SteamId| format!("p{}", id.id64() - STEAM_ID64_BASE);
    let cases = [
        (
            EventKind::PointCap {
                team: Team::Blue,
                point: 3,
            },
            ("Point Capture", "BLU captured point 3"),
        ),
        (
            EventKind::Charge {
                team: Team::Red,
                medic: p(5),
                medigun: Medigun::MediGun,
            },
            ("Übercharge", "p5"),
        ),
        (
            EventKind::Charge {
                team: Team::Red,
                medic: p(5),
                medigun: Medigun::Kritzkrieg,
            },
            ("Übercharge", "p5 (Kritzkrieg)"),
        ),
        (
            EventKind::MedicDeath {
                team: Team::Blue,
                medic: p(5),
                killer: Some(p(6)),
            },
            ("Medic Death", "p5 killed by p6"),
        ),
        (
            EventKind::MedicDeath {
                team: Team::Blue,
                medic: p(5),
                killer: None,
            },
            ("Medic Death", "p5 died"),
        ),
        (
            EventKind::Drop {
                team: Team::Red,
                medic: p(5),
            },
            ("Drop", "p5 dropped a full charge"),
        ),
        (
            EventKind::RoundWin { team: Team::Red },
            ("Round end", "Round win for RED"),
        ),
        (EventKind::Other { name: "mvp".into() }, ("mvp", "")),
    ];
    for (kind, (ty, description)) in cases {
        assert_eq!(
            event_text(&kind, &name_of),
            (ty.to_owned(), description.to_owned()),
            "{kind:?}"
        );
    }
    assert_eq!(medigun_name(&Medigun::QuickFix), "Quick-Fix");
    assert_eq!(medigun_name(&Medigun::Other("mystery".into())), "mystery");
}
