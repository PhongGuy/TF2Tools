//! Correlates run logs with the session's damage log: each run's first damage and settle time,
//! and the damage count inside each idle window.

use std::collections::HashMap;

use crate::session::parse_damage;

#[derive(Debug, Clone, PartialEq)]
pub struct RunPaint {
    pub run: usize,
    /// First damage of the run's pid, from the spawn.
    pub first_ms: Option<f64>,
    /// Last damage before `quiet_ms` without damage, from the spawn.
    pub settled_ms: Option<f64>,
    /// The pid's damages within `[spawned, ended]`.
    pub damages: usize,
    /// Other pids' damages within the same window.
    pub other_damages: usize,
}

/// The pid's damages in `[from, to)`.
#[derive(Debug, Clone, PartialEq)]
pub struct IdleCount {
    pub pid: u32,
    pub frames: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub runs: Vec<RunPaint>,
    pub idle: Vec<IdleCount>,
}

/// The `key=value` words of a line, each split at its first `=`.
fn fields(line: &str) -> HashMap<&str, &str> {
    line.split_whitespace()
        .filter_map(|word| word.split_once('='))
        .collect()
}

fn field<T: std::str::FromStr>(
    kv: &HashMap<&str, &str>,
    name: &str,
    line: &str,
) -> Result<T, String> {
    let value = kv
        .get(name)
        .ok_or_else(|| format!("missing {name} in: {line}"))?;
    value.parse().map_err(|_| format!("bad {name} in: {line}"))
}

/// The first damage followed by `quiet_ms` of silence; the run's end closes the last gap.
fn settle(own: &[u64], ended: u64, quiet_ms: u64) -> Option<u64> {
    own.iter().enumerate().find_map(|(i, &at)| {
        let next = own.get(i + 1).copied().unwrap_or(ended);
        (next.saturating_sub(at) >= quiet_ms).then_some(at)
    })
}

/// `runs` holds `run=` lines (pid, spawned_unix_ms, ended_unix_ms) and `idle pid=` lines;
/// `damage` holds `DMG <pid> <ms>` lines. Other lines are ignored.
pub fn report(runs: &str, damage: &str, quiet_ms: u64) -> Result<Report, String> {
    let mut damages: Vec<(u32, u64)> = damage.lines().filter_map(parse_damage).collect();
    damages.sort_by_key(|&(_, ms)| ms);
    let mut out = Report {
        runs: Vec::new(),
        idle: Vec::new(),
    };
    for line in runs.lines() {
        let kv = fields(line);
        if line.starts_with("run=") {
            let pid: u32 = field(&kv, "pid", line)?;
            let spawned: u64 = field(&kv, "spawned_unix_ms", line)?;
            let ended: u64 = field(&kv, "ended_unix_ms", line)?;
            let (own, other): (Vec<_>, Vec<_>) = damages
                .iter()
                .filter(|&&(_, ms)| (spawned..=ended).contains(&ms))
                .partition(|&&(p, _)| p == pid);
            let own: Vec<u64> = own.iter().map(|&&(_, ms)| ms).collect();
            let since = |at: u64| (at - spawned) as f64;
            out.runs.push(RunPaint {
                run: field(&kv, "run", line)?,
                first_ms: own.first().map(|&at| since(at)),
                settled_ms: settle(&own, ended, quiet_ms).map(since),
                damages: own.len(),
                other_damages: other.len(),
            });
        } else if line.starts_with("idle pid=") {
            let pid: u32 = field(&kv, "pid", line)?;
            let from: u64 = field(&kv, "from_unix_ms", line)?;
            let to: u64 = field(&kv, "to_unix_ms", line)?;
            let frames = damages
                .iter()
                .filter(|&&(p, ms)| p == pid && (from..to).contains(&ms))
                .count();
            out.idle.push(IdleCount { pid, frames });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DMG: &str = "DMG 10 1000\nDMG 10 1016\nDMG 10 1400\nDMG 11 1500\nDMG 10 3000\n";

    #[test]
    fn first_damage_and_settle_per_run() {
        let runs = "run=1 pid=10 spawned_unix_ms=900 ended_unix_ms=2600 startup_ms=180.0\n\
                    startup median_ms=180.0 min_ms=180.0 max_ms=180.0 runs=1\n";
        let r = report(runs, DMG, 1000).unwrap();
        // Inside [900, 2600] pid 10 drew at 1000, 1016 and 1400, then nothing for 1200 ms.
        assert_eq!(
            r.runs,
            [RunPaint {
                run: 1,
                first_ms: Some(100.0),
                settled_ms: Some(500.0),
                damages: 3,
                other_damages: 1
            }]
        );
    }

    #[test]
    fn a_window_still_drawing_at_the_end_never_settles() {
        let runs = "run=2 pid=10 spawned_unix_ms=900 ended_unix_ms=1500\n";
        let r = report(runs, DMG, 1000).unwrap();
        assert_eq!(r.runs[0].settled_ms, None);
        assert_eq!(r.runs[0].first_ms, Some(100.0));
    }

    #[test]
    fn a_quiet_gap_mid_run_settles_at_its_start() {
        let runs = "run=3 pid=10 spawned_unix_ms=900 ended_unix_ms=3100\n";
        let r = report(runs, DMG, 1000).unwrap();
        // 1400 -> 3000 is the first gap of 1000 ms or more; 3000 still counts as a damage.
        assert_eq!(r.runs[0].settled_ms, Some(500.0));
        assert_eq!(r.runs[0].damages, 4);
    }

    #[test]
    fn damage_from_other_pids_is_counted_apart() {
        let runs = "run=4 pid=12 spawned_unix_ms=900 ended_unix_ms=2600\n";
        let r = report(runs, DMG, 1000).unwrap();
        assert_eq!(r.runs[0].first_ms, None);
        assert_eq!(r.runs[0].damages, 0);
        assert_eq!(r.runs[0].other_damages, 4);
    }

    #[test]
    fn idle_counts_the_pid_inside_its_window() {
        let runs = "idle pid=10 from_unix_ms=1010 to_unix_ms=3000\n";
        let r = report(runs, DMG, 1000).unwrap();
        assert_eq!(r.idle, [IdleCount { pid: 10, frames: 2 }]);
    }

    #[test]
    fn a_run_line_without_its_fields_is_an_error() {
        let err = report("run=5 pid=10\n", DMG, 1000).unwrap_err();
        assert!(err.contains("spawned_unix_ms"), "{err}");
    }
}
