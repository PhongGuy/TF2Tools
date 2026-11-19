//! The manual Windows checks workflow: dispatch only, read-only, no secrets, every action pinned,
//! and it runs the real-machine tests that the normal suite ignores.

use std::path::Path;

fn workflow() -> String {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.github/workflows/windows-checks.yml");
    std::fs::read_to_string(path).expect("windows-checks.yml")
}

#[test]
fn it_runs_only_when_dispatched_and_reads_only() {
    let text = workflow();
    let preamble: String = text
        .lines()
        .take_while(|l| *l != "jobs:")
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        preamble.contains("on:\n  workflow_dispatch:\n"),
        "{preamble}"
    );
    for trigger in ["push:", "pull_request", "schedule:", "workflow_run"] {
        assert!(!preamble.contains(trigger), "triggered by {trigger}");
    }
    assert!(!text.contains("secrets."), "it needs no secrets");
    assert!(!text.contains("write"), "nothing is granted write");
}

#[test]
fn every_action_is_pinned_by_sha() {
    for line in workflow().lines().filter(|l| l.contains("uses: ")) {
        let pin = line.split('@').nth(1).unwrap_or_default();
        let sha = pin.split_whitespace().next().unwrap_or_default();
        assert!(
            sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()),
            "{line}"
        );
    }
}

#[test]
fn it_runs_both_real_machine_checks_opted_in() {
    let text = workflow();
    assert!(text.contains("TF2TOOLS_REAL_WINDOWS: \"1\""), "opted in");
    for module in ["windows_real_fonts", "windows_real_v4"] {
        let run = text
            .lines()
            .find(|l| l.contains(module))
            .unwrap_or_else(|| panic!("no step runs {module}"));
        assert!(run.contains("-- --ignored --nocapture"), "{run}");
    }
    assert!(
        text.contains("gh release download v4.8.0 --repo \"$GITHUB_REPOSITORY\""),
        "v4 comes from this repo's own release"
    );
}
