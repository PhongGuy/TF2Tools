//! The release workflow's shape: pinned actions, one write-scoped job, a release build without the
//! update test seam, signing secrets confined to the draft job, and player-facing release text.

use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(path: &str) -> String {
    std::fs::read_to_string(repo().join(path)).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

fn workflow() -> String {
    read(".github/workflows/release.yml")
}

/// Everything before `jobs:`.
fn preamble(text: &str) -> String {
    text.lines()
        .take_while(|l| *l != "jobs:")
        .collect::<Vec<_>>()
        .join("\n")
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

#[test]
fn every_action_is_pinned_by_a_full_commit_sha() {
    let text = workflow();
    let uses: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter_map(|l| {
            l.strip_prefix("- uses: ")
                .or_else(|| l.strip_prefix("uses: "))
        })
        .collect();
    assert!(uses.len() >= 10, "{uses:?}");
    for entry in uses {
        let (action, comment) = entry.split_once(" # ").unwrap_or((entry, ""));
        let (_, sha) = action
            .split_once('@')
            .unwrap_or_else(|| panic!("unpinned: {entry}"));
        assert!(
            sha.len() == 40
                && sha
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "not a commit sha: {entry}"
        );
        assert!(!comment.is_empty(), "no version comment: {entry}");
    }
}

#[test]
fn it_runs_on_version_tags_and_by_hand_only() {
    let pre = preamble(&workflow());
    assert!(pre.contains("tags: [\"v*\"]"), "{pre}");
    assert!(pre.contains("workflow_dispatch:"), "{pre}");
    assert!(
        !pre.contains("pull_request") && !pre.contains("branches"),
        "{pre}"
    );
}

#[test]
fn only_the_draft_job_may_write_contents() {
    let text = workflow();
    assert!(preamble(&text).contains("permissions:\n  contents: read"));
    assert_eq!(text.matches("contents: write").count(), 1);
    assert!(job(&text, "draft").contains("contents: write"));
}

#[test]
fn the_jobs_run_gate_then_linux_then_a_tag_only_draft() {
    let text = workflow();
    assert!(job(&text, "linux").contains("needs: gate"));
    let draft = job(&text, "draft");
    assert!(draft.contains("needs: [gate, linux, windows]"), "{draft}");
    assert!(
        draft.contains("if: startsWith(github.ref, 'refs/tags/v')"),
        "{draft}"
    );
}

#[test]
fn the_gate_runs_the_checks_and_the_trailer_check_over_full_history() {
    let gate = job(&workflow(), "gate");
    assert!(gate.contains("run: make check"), "{gate}");
    assert!(
        gate.contains("run: scripts/check-trailers.sh HEAD"),
        "{gate}"
    );
    assert!(gate.contains("fetch-depth: 0"), "{gate}");
}

#[test]
fn the_linux_job_builds_a_release_without_the_test_seam() {
    let text = workflow();
    let linux = job(&text, "linux");
    assert!(linux.contains("runs-on: ubuntu-22.04"), "{linux}");
    assert!(linux.contains("TF2TOOLS_RELEASE_BUILD: \"1\""), "{linux}");
    assert!(linux.contains("run: make package-check"), "{linux}");
    assert!(
        linux.contains("grep -aq TF2TOOLS_UPDATE_TEST_PUBKEY target/release/tf2tools"),
        "{linux}"
    );
    assert!(!text.contains("--features") && !text.contains("--all-features"));
    // `make check` lints with --all-features, which the release guard turns into a build error.
    assert!(!preamble(&text).contains("TF2TOOLS_RELEASE_BUILD"));
    assert!(!job(&text, "gate").contains("TF2TOOLS_RELEASE_BUILD"));
}

#[test]
fn the_deb_version_is_fixed_and_rechecked_before_upload() {
    let linux = job(&workflow(), "linux");
    let order = [
        "run: make package-check",
        "scripts/test-deb-prerelease.sh",
        "scripts/deb-prerelease.sh target/packages/tf2tools_*_amd64.deb",
        "scripts/test-package.sh target/packages",
        "actions/upload-artifact@",
    ];
    let at: Vec<usize> = order
        .iter()
        .map(|s| linux.find(s).unwrap_or_else(|| panic!("missing {s}")))
        .collect();
    assert!(at.windows(2).all(|w| w[0] < w[1]), "out of order: {at:?}");
}

#[test]
fn only_the_packages_are_uploaded_and_released() {
    let text = workflow();
    let linux = job(&text, "linux");
    assert!(
        linux.contains("path: target/packages/tf2tools_*"),
        "{linux}"
    );
    assert!(linux.contains("if-no-files-found: error"), "{linux}");
    let draft = job(&text, "draft");
    let create = step(&draft, "Create the draft release");
    let uploads = RELEASED.map(|f| format!("dist/{f}")).join(" ");
    assert!(
        create.contains(&format!("{uploads} dist/SHA256SUMS")),
        "{create}"
    );
}

/// Every release file but SHA256SUMS, in upload order. The first four are what the updater and the
/// README depend on; the rest are the unversioned copies.
const RELEASED: [&str; 7] = [
    "tf2tools_*",
    "TF2Tools.exe",
    "tf2tools-update.json",
    "tf2tools-update.json.minisig",
    "tf2tools-windows-setup.exe",
    "tf2tools-linux-x86_64.AppImage",
    "tf2tools-linux-amd64.deb",
];

#[test]
fn the_draft_adds_unversioned_copies_and_checksums_after_verifying() {
    let draft = job(&workflow(), "draft");
    let extras = step(&draft, "Add the unversioned copies and checksums");
    for copy in [
        "cp TF2Tools.exe tf2tools-windows-setup.exe",
        "cp tf2tools_*_x86_64.AppImage tf2tools-linux-x86_64.AppImage",
        "cp tf2tools_*_amd64.deb tf2tools-linux-amd64.deb",
    ] {
        assert!(extras.contains(copy), "missing `{copy}`:\n{extras}");
    }
    // Bare names, so `sha256sum -c SHA256SUMS` works in a folder of downloads.
    let names = RELEASED.join(" ");
    assert!(
        extras.contains(&format!("sha256sum {names} > SHA256SUMS")),
        "{extras}"
    );
    let at = |s: &str| draft.find(s).unwrap_or_else(|| panic!("missing {s}"));
    assert!(at("Verify it against the compiled-in keys") < at("Add the unversioned copies"));
    assert!(at("Add the unversioned copies") < at("Create the draft release"));
}

#[test]
fn the_draft_writes_whats_changed_from_full_history() {
    let draft = job(&workflow(), "draft");
    assert!(draft.contains("fetch-depth: 0"), "{draft}");
    let create = step(&draft, "Create the draft release");
    assert!(
        create.contains(
            "scripts/release-notes.sh \"$GITHUB_REF_NAME\" packaging/release-notes.md > \"$RUNNER_TEMP/notes.md\""
        ),
        "{create}"
    );
}

#[test]
fn the_release_notes_template_has_the_sections_in_order() {
    let notes = read("packaging/release-notes.md");
    // The title comes from `gh release create --title`, and the intro is written by hand.
    assert!(!notes.lines().any(|l| l.starts_with("# ")), "{notes}");
    assert!(notes.starts_with("<!-- "), "{notes}");
    let mut last = 0;
    for part in [
        "## What's Changed",
        "\n{changes}\n",
        "## Downloads",
        "| Platform | File |",
        "| Windows x64 |",
        "| Linux x86_64 |",
        "no version in its name",
        "Check a download with `sha256sum -c SHA256SUMS`.",
        "## Before you install",
        "## Updates",
    ] {
        let at = notes
            .find(part)
            .unwrap_or_else(|| panic!("missing {part:?}"));
        assert!(at >= last, "{part:?} is out of order");
        last = at;
    }
    for words in [
        "tf2tools_{version}_x86_64.AppImage",
        "tf2tools_{version}_amd64.deb",
        "allow it to run as a program",
        "software installer",
        "checking their signature",
        "does not update itself",
    ] {
        assert!(notes.contains(words), "missing {words:?}");
    }
}

#[test]
fn the_signing_secrets_reach_only_the_signing_step() {
    let text = workflow();
    // Named as in the maintainer's other cargo-packager projects; the closing braces keep
    // the key's name from matching inside the password's.
    for secret in [
        "secrets.CARGO_PACKAGER_SIGN_PRIVATE_KEY }}",
        "secrets.CARGO_PACKAGER_SIGN_PRIVATE_KEY_PASSWORD }}",
    ] {
        assert_eq!(text.matches(secret).count(), 1, "{secret}");
        assert!(job(&text, "draft").contains(secret), "{secret}");
    }
    assert!(!text.contains("set -x"));
    for line in text.lines().map(str::trim) {
        assert!(
            !(line.contains("echo")
                && (line.contains("MINISIGN") || line.contains("SIGN_PRIVATE"))),
            "echoes a secret: {line}"
        );
    }
}

#[test]
fn the_draft_verifies_against_the_compiled_in_keys_and_stays_a_draft() {
    let draft = job(&workflow(), "draft");
    assert!(
        draft.contains("make_manifest -- verify --dir dist"),
        "{draft}"
    );
    assert!(
        !draft.contains("--key"),
        "verify must use UPDATE_PUBLIC_KEYS"
    );
    assert!(
        draft.contains("gh release create \"$GITHUB_REF_NAME\" --draft --verify-tag"),
        "{draft}"
    );
    assert!(draft.contains("packaging/release-notes.md"), "{draft}");
}

#[test]
fn checkouts_never_keep_credentials() {
    let text = workflow();
    let checkouts = text.matches("actions/checkout@").count();
    assert_eq!(checkouts, 4);
    assert_eq!(
        text.matches("persist-credentials: false").count(),
        checkouts
    );
}

#[test]
fn the_release_page_text_is_for_players() {
    let notes = read("packaging/release-notes.md");
    let download = "https://github.com/PhongGuy/TF2Tools/releases/download/v{version}";
    assert!(
        notes.contains(&format!("{download}/tf2tools_{{version}}_x86_64.AppImage")),
        "{notes}"
    );
    assert!(
        notes.contains(&format!("{download}/tf2tools_{{version}}_amd64.deb")),
        "{notes}"
    );
    for snippet in ["```", "$ "] {
        assert!(!notes.contains(snippet), "player text holds {snippet:?}");
    }
    // Whole words, so ordinary prose such as "adapt" or "make sure" passes.
    let words: Vec<String> = notes
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '-')
        .map(str::to_ascii_lowercase)
        .collect();
    for command in [
        "sudo", "chmod", "apt", "apt-get", "dpkg", "cargo", "terminal",
    ] {
        assert!(
            !words.iter().any(|w| w == command),
            "player text holds {command:?}"
        );
    }
}

/// The `- name: {name}` step's lines within `job`, up to the next step.
fn step(job: &str, name: &str) -> String {
    let header = format!("- name: {name}");
    let mut lines = job.lines().skip_while(|l| l.trim() != header);
    let first = lines.next().unwrap_or_else(|| panic!("no step {name}"));
    let rest = lines.take_while(|l| !l.trim_start().starts_with("- "));
    std::iter::once(first)
        .chain(rest)
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_signer_is_built_without_the_secrets_and_signing_only_runs_it() {
    let draft = job(&workflow(), "draft");
    let build = step(&draft, "Build the manifest signer");
    assert!(
        build.contains("cargo build --locked -p tf2tools-core --example make_manifest"),
        "{build}"
    );
    assert!(!build.contains("secrets."), "{build}");
    let sign = step(&draft, "Sign the update manifest");
    assert!(
        sign.contains("TF2TOOLS_MINISIGN_KEY: ${{ secrets.CARGO_PACKAGER_SIGN_PRIVATE_KEY }}"),
        "{sign}"
    );
    assert!(
        sign.contains("run: target/debug/examples/make_manifest sign --dir dist"),
        "{sign}"
    );
    assert!(!sign.contains("cargo"), "signing must not compile: {sign}");
    assert!(draft.find("Build the manifest signer") < draft.find("Sign the update manifest"));
}

#[test]
fn the_draft_first_checks_the_tag_matches_the_workspace_version() {
    let draft = job(&workflow(), "draft");
    let check = step(&draft, "The tag matches the workspace version");
    assert!(check.contains("Cargo.toml"), "{check}");
    assert!(check.contains("exit 1"), "{check}");
    let at = |s: &str| draft.find(s).unwrap_or_else(|| panic!("missing {s}"));
    assert!(at("The tag matches the workspace version") < at("actions/download-artifact@"));
    assert!(at("The tag matches the workspace version") < at("Sign the update manifest"));
}

#[test]
fn the_release_page_names_each_download_once() {
    let notes = read("packaging/release-notes.md");
    assert_eq!(notes.matches("Ubuntu and Debian").count(), 1, "{notes}");
}

#[test]
fn the_runbook_names_every_secret_and_gated_step() {
    let runbook = read("RELEASING.md");
    let text = workflow();
    // Every secret the workflow reads is explained in the runbook.
    for part in text.split("secrets.").skip(1) {
        let name: String = part
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        assert!(runbook.contains(&name), "runbook lacks {name}");
    }
    for needle in [
        "minisign -G",
        "UPDATE_PUBLIC_KEYS",
        "make_manifest",
        "Co-authored-by",
        "scripts/check-trailers.sh",
        "6.0.0~alpha",
        "TF2Tools.exe",
        "--draft",
        "Valve",
        "What's Changed",
        "scripts/release-notes.sh",
        "SHA256SUMS",
        "tf2tools-windows-setup.exe",
        "tf2tools-linux-x86_64.AppImage",
        "tf2tools-linux-amd64.deb",
    ] {
        assert!(runbook.contains(needle), "runbook lacks {needle:?}");
    }
}

#[test]
fn the_windows_job_builds_the_installer_without_the_test_seam() {
    let windows = job(&workflow(), "windows");
    for want in [
        "needs: gate",
        "runs-on: windows-2025",
        "shell: bash",
        "TF2TOOLS_RELEASE_BUILD: \"1\"",
        "AWS_LC_SYS_PREBUILT_NASM: \"1\"",
        "run: cargo install cargo-packager --version 0.11.8 --locked",
        "run: cargo build --release --locked -p tf2tools-app --bin tf2tools",
        "name: windows-installer",
        "path: target/packages/TF2Tools.exe",
        "if-no-files-found: error",
    ] {
        assert!(
            windows.lines().any(|l| l.trim() == want),
            "missing `{want}`:\n{windows}"
        );
    }
    assert!(windows.contains("grep -aq TF2TOOLS_UPDATE_TEST_PUBKEY target/release/tf2tools.exe"));
    assert!(windows.contains("--formats nsis"), "{windows}");
    assert!(windows.contains("TF2Tools.exe"), "{windows}");
    assert!(
        windows.contains("TF2TOOLS_WINDOWS_EXE=\"$(cygpath -m \"$PWD\")/target/release/tf2tools.exe\" cargo test --locked -p tf2tools-app --test integration packaging:: -- --ignored"),
        "{windows}"
    );
    // cargo test runs from the package root, so a relative exe path would miss the build.
    assert!(
        !windows.contains("TF2TOOLS_WINDOWS_EXE=target/"),
        "{windows}"
    );
    assert!(
        windows.lines().any(|l| l.trim() == "timeout-minutes: 90"),
        "{windows}"
    );
    assert!(!windows.contains("secrets."), "{windows}");
}

#[test]
fn the_draft_signs_only_when_both_platforms_are_present() {
    let draft = job(&workflow(), "draft");
    assert!(draft.contains("name: windows-installer"), "{draft}");
    let present = step(&draft, "Both platforms are present");
    assert!(present.contains("dist/TF2Tools.exe"), "{present}");
    assert!(present.contains("_x86_64.AppImage"), "{present}");
    assert!(present.contains("exit 1"), "{present}");
    assert!(draft.find("Both platforms are present") < draft.find("Sign the update manifest"));
}

#[test]
fn the_draft_fails_unless_the_manifest_holds_the_windows_installer() {
    let draft = job(&workflow(), "draft");
    let verify = step(&draft, "Verify it against the compiled-in keys");
    // make_manifest verify prints one `<platform>: <file> matches` line per signed asset.
    assert!(
        verify.contains("windows-x86_64-nsis: TF2Tools.exe matches"),
        "{verify}"
    );
    assert!(verify.contains("exit 1"), "{verify}");
}

#[test]
fn the_release_page_offers_windows_and_explains_its_warning() {
    let notes = read("packaging/release-notes.md");
    assert!(
        notes.contains(
            "https://github.com/PhongGuy/TF2Tools/releases/download/v{version}/TF2Tools.exe"
        ),
        "{notes}"
    );
    for words in ["Windows protected your PC", "More info", "Run anyway"] {
        assert!(notes.contains(words), "missing {words:?}");
    }
    assert!(!notes.contains("4.8 until"), "{notes}");
    assert!(
        notes.contains("TF2Tools 6 installs next to TF2Tools 4. Once it works, you can remove TF2Tools 4 in Windows Settings > Apps."),
        "{notes}"
    );
}
