//! The logs.tf page's wording and number formats.

use std::time::Duration;

use tf2tools_app::logs_sync::{SyncProgress, ring_fill};
use tf2tools_app::pages::logs_tf::text::{
    DOWNLOAD_FAILED, EMPTY_TEXT, NO_LOGS_TEXT, accuracy_text, charge_text, count_text,
    download_reason, duration_text, event_parts, event_text, fetch_message, finding_text,
    healing_text, medigun_name, one_decimal, progress_line, progress_text, range_label, reads_zero,
    resolve_message, ring_label, seconds_text, share_text, streaks_label, summary_text,
    sync_message, tenths_text, time_played_text, whole,
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

#[test]
fn an_events_names_take_their_own_teams_colour() {
    use tf2tools_core::{EventKind, Medigun, STEAM_ID64_BASE, SteamId, Team};
    let p = |n: u64| SteamId::from_id64(STEAM_ID64_BASE + n).unwrap();
    let name_of = |id: SteamId| format!("p{}", id.id64() - STEAM_ID64_BASE);
    // p6 plays for BLU; anyone else is unknown to the log.
    let team_of = |id: SteamId| (id == p(6)).then_some(Team::Blue);
    let parts = |kind: EventKind| event_parts(&kind, &name_of, &team_of).1;
    let piece = |text: &str, team: Option<Team>| (text.to_owned(), team);
    assert_eq!(
        parts(EventKind::MedicDeath {
            team: Team::Red,
            medic: p(5),
            killer: Some(p(6)),
        }),
        [
            piece("p5", Some(Team::Red)),
            piece(" killed by ", None),
            piece("p6", Some(Team::Blue)),
        ]
    );
    assert_eq!(
        parts(EventKind::Charge {
            team: Team::Blue,
            medic: p(5),
            medigun: Medigun::Kritzkrieg,
        }),
        [piece("p5", Some(Team::Blue)), piece(" (Kritzkrieg)", None)]
    );
    assert_eq!(
        parts(EventKind::Drop {
            team: Team::Red,
            medic: p(5),
        }),
        [
            piece("p5", Some(Team::Red)),
            piece(" dropped a full charge", None)
        ]
    );
    assert_eq!(
        parts(EventKind::RoundWin { team: Team::Red }),
        [piece("Round win for ", None), piece("RED", Some(Team::Red))]
    );
}

#[test]
fn medic_numbers_read_like_logs_tf() {
    use tf2tools_core::Medigun;
    assert_eq!(healing_text(28_170, 986.11), "28170 (986/m)");
    assert_eq!(charge_text(&Medigun::MediGun, 12, 12), "Medigun: 12 (100%)");
    assert_eq!(
        charge_text(&Medigun::Kritzkrieg, 1, 4),
        "Kritzkrieg: 1 (25%)"
    );
    assert_eq!(
        charge_text(&Medigun::Other("mystery".into()), 2, 2),
        "mystery: 2 (100%)"
    );
    assert_eq!(seconds_text(63.166), "63 s");
    assert_eq!(tenths_text(6.5), "6.5 s");
}

#[test]
fn heal_targets_run_high_to_low_with_class_and_share() {
    use tf2tools_app::pages::logs_tf::tables::heal_rows;
    use tf2tools_core::TfClass;
    let log = tf2tools_core::parse_log(crate::support::logstf::SIXES).unwrap();
    let rahmed = log.medics().iter().find(|m| m.name() == "Rahmed").unwrap();
    let rows = heal_rows(rahmed);
    let top: Vec<(&str, Option<TfClass>, u32, &str)> = rows[..2]
        .iter()
        .map(|r| (r.name.as_str(), r.class, r.heals, r.share.as_str()))
        .collect();
    assert_eq!(
        top,
        [
            ("deshio", Some(TfClass::Scout), 8_495, "30%"),
            ("DrHappiness", Some(TfClass::Soldier), 6_127, "22%"),
        ]
    );
    assert!(
        rows.windows(2).all(|w| w[0].heals >= w[1].heals),
        "Heals high to low"
    );
}

#[test]
fn matrix_rows_run_k_high_to_low_and_count_every_player() {
    use tf2tools_app::pages::logs_tf::tables::matrix_rows;
    use tf2tools_core::MatrixKind;
    let log = tf2tools_core::parse_log(crate::support::logstf::SIXES).unwrap();
    let sid = crate::support::logstf::sid;
    for kind in [
        MatrixKind::Kills,
        MatrixKind::KillsAssists,
        MatrixKind::Deaths,
    ] {
        let rows = matrix_rows(&log, kind);
        assert_eq!(rows.len(), 12, "{kind:?} has every player");
        assert!(
            rows.iter().all(|(p, r)| p.steam_id() == r.steam_id()),
            "{kind:?} pairs each row with its own player"
        );
        let totals: Vec<u32> = rows.iter().map(|(_, r)| r.total()).collect();
        assert!(
            totals.windows(2).all(|w| w[0] >= w[1]),
            "{kind:?} {totals:?}"
        );
    }
    let kills = matrix_rows(&log, MatrixKind::Kills);
    assert_eq!(
        kills[0].1.steam_id(),
        sid(285_837_700),
        "39 kills tops the table"
    );
    let (_, meta) = kills
        .iter()
        .find(|(_, r)| r.steam_id() == sid(98_265_577))
        .unwrap();
    assert_eq!(
        (meta.counts(), meta.total()),
        ([0; 9], 0),
        "a medic with no kills reads as zeros"
    );
}

#[test]
fn chat_lines_carry_their_team_and_console_has_none() {
    use tf2tools_app::pages::logs_tf::tables::chat_rows;
    let body = r#"{"length": 60, "teams": {"Red": {}, "Blue": {}},
        "players": {"[U:1:5]": {"team": "Red", "class_stats": [{"type": "scout", "total_time": 60}]}},
        "chat": [{"steamid": "[U:1:5]", "name": "Five", "msg": "gl"},
                 {"steamid": "Console", "name": "Console", "msg": "hi"}]}"#;
    let log = tf2tools_core::parse_log(body).unwrap();
    let rows = chat_rows(&log);
    assert_eq!(rows[0].team, Some(tf2tools_core::Team::Red));
    assert_eq!(
        (rows[1].team, rows[1].name.as_str(), rows[1].msg.as_str()),
        (None, "Console", "hi")
    );
    assert_eq!(streaks_label(13), "Show Killstreaks (13)");
    assert_eq!(streaks_label(0), "Show Killstreaks (0)");
}

#[test]
fn download_failures_say_why() {
    use tf2tools_core::LogsFetchError;
    assert_eq!(
        download_reason(&LogsFetchError::Gone),
        "logs.tf no longer has this log."
    );
    let offline = LogsFetchError::Network {
        url: "u".into(),
        message: "m".into(),
    };
    assert_eq!(download_reason(&offline), "Check your internet connection.");
    let big = LogsFetchError::TooLarge {
        url: "u".into(),
        limit: 1,
    };
    assert_eq!(download_reason(&big), "This log is too big to download.");
    let busy = LogsFetchError::RateLimited { url: "u".into() };
    assert_eq!(
        download_reason(&busy),
        "logs.tf is busy. Try again in a few minutes."
    );
    let down = LogsFetchError::Http {
        url: "u".into(),
        status: 500,
    };
    assert_eq!(
        download_reason(&down),
        "logs.tf isn't responding right now. Try again later."
    );
    assert_eq!(
        download_reason(&LogsFetchError::Unreadable),
        "logs.tf isn't responding right now. Try again later."
    );
    assert_eq!(DOWNLOAD_FAILED, "Couldn't download this log.");
}
