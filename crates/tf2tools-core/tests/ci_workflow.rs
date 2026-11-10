//! The CI workflow's shape: a Windows job that lints and tests what Linux does, pinned like the gate.

use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn ci() -> String {
    std::fs::read_to_string(repo().join(".github/workflows/ci.yml")).expect("ci.yml")
}

/// Job `name`'s lines: from `  name:` under `jobs:` to the next two-space key.
fn job(text: &str, name: &str) -> String {
    let header = format!("  {name}:");
    let mut out = Vec::new();
    for line in text.lines().skip_while(|l| *l != "jobs:").skip(1) {
        let key = line.starts_with("  ") && !line.starts_with("   ") && !line.trim().is_empty();
        if key && !out.is_empty() {
            break;
        }
        if line == header || !out.is_empty() {
            out.push(line);
        }
    }
    assert!(!out.is_empty(), "no job {name}");
    out.join("\n")
}

fn uses(job: &str) -> Vec<String> {
    job.lines()
        .filter_map(|l| l.trim().strip_prefix("- uses: ").map(str::to_owned))
        .collect()
}

#[test]
fn a_windows_job_lints_and_tests_the_workspace() {
    let windows = job(&ci(), "windows");
    for want in [
        "runs-on: windows-2025",
        "shell: bash",
        "AWS_LC_SYS_PREBUILT_NASM: \"1\"",
        "persist-credentials: false",
        "components: clippy",
        "run: cargo clippy --locked --workspace --all-targets --all-features -- -Dwarnings",
        "run: cargo test --locked --workspace --no-fail-fast",
    ] {
        assert!(
            windows.lines().any(|l| l.trim() == want),
            "missing `{want}`:\n{windows}"
        );
    }
}

/// Lint and test unify dev-dependency features into the app, so only a plain check sees the
/// release binary as release.yml builds it.
#[test]
fn the_gate_checks_the_app_binary_without_dev_features() {
    let gate = job(&ci(), "gate");
    let want = "run: cargo check --locked -p tf2tools-app --bin tf2tools";
    assert!(
        gate.lines().any(|l| l.trim() == want),
        "missing `{want}`:\n{gate}"
    );
}

#[test]
fn the_windows_job_uses_the_gates_pinned_actions() {
    let text = ci();
    assert_eq!(uses(&job(&text, "windows")), uses(&job(&text, "gate")));
}

#[test]
fn checkouts_keep_bytes_exact_on_windows() {
    let attributes =
        std::fs::read_to_string(repo().join(".gitattributes")).expect(".gitattributes");
    assert!(
        attributes.lines().any(|l| l.trim() == "* -text"),
        "{attributes}"
    );
}
