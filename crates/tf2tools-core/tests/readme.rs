use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn repo_file(name: &str) -> String {
    let path = repo_root().join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn contributing_holds_the_developer_guide() {
    let guide = repo_file("CONTRIBUTING.md");
    for needle in [
        "# Contributing to TF2Tools",
        "## Requirements",
        "libasound2-dev",
        "make demo",
        "fc-list",
        "make uninstall",
        "make install",
        "backup-v5-",
        "make package-check",
        "tf2tools_<version>_amd64.deb",
        "make gate",
        "make readme-shots",
        "scripts/readme-screenshots.sh",
        "## Layout",
        "tools/tf2tools-pilot",
        "[RELEASING.md](RELEASING.md)",
        "[README.md](README.md)",
    ] {
        assert!(guide.contains(needle), "CONTRIBUTING.md lacks {needle:?}");
    }
    assert!(
        !guide.contains("cannot open it"),
        "the v5 claim is wrong; see migrations.rs step 10"
    );
}

const DEVELOPERS: &str = "\n## For developers\n";
const IMAGES: [&str; 11] = [
    "huds",
    "sounds",
    "crosshairs",
    "hud-store",
    "sound-store",
    "crosshair-store",
    "addon-store",
    "profiles",
    "settings",
    "crosshair-maker",
    "hud-maker",
];
const IMAGE_BUDGET: u64 = 2_000_000;
const EXE: &str = "https://github.com/PhongGuy/TF2Tools/releases/latest/download/TF2Tools.exe";
const LATEST: &str = "https://github.com/PhongGuy/TF2Tools/releases/latest/download";

// Everything above the developer section is for players.
fn player_part(readme: &str) -> &str {
    let (players, _) = readme
        .split_once(DEVELOPERS)
        .expect("a For developers section");
    players
}

// The target of every Markdown link and image, in order: the text between "](" and ")".
fn link_targets(text: &str) -> Vec<&str> {
    text.match_indices("](")
        .map(|(i, _)| {
            let rest = &text[i + 2..];
            &rest[..rest.find(')').expect("a closed link")]
        })
        .collect()
}

// The target of every image: links whose "[" is preceded by "!".
fn image_targets(text: &str) -> Vec<&str> {
    text.match_indices("![")
        .map(|(i, _)| {
            let rest = &text[i..];
            let open = rest.find("](").expect("an image target");
            let close = rest[open..].find(')').expect("a closed image");
            &rest[open + 2..open + close]
        })
        .collect()
}

#[test]
fn the_readme_has_the_player_sections_in_order() {
    let readme = repo_file("README.md");
    assert!(readme.starts_with("# TF2Tools\n"), "title");
    let mut last = 0;
    for heading in [
        "## What it does",
        "### HUDs",
        "### Hitsounds and killsounds",
        "### A crosshair for every weapon",
        "### mastercomfig addons",
        "### Graphics",
        "### The stores",
        "### Make your own crosshair",
        "### Make your own HUD",
        "### Maps",
        "### Profiles",
        "### Settings",
        "## Download and install",
        "### Windows",
        "### Linux",
        "### First start",
        "### Updates",
        "## Quick start",
        "## Coming from an older version",
        "## Privacy",
        "## License",
        "## For developers",
    ] {
        let at = readme
            .find(&format!("\n{heading}\n"))
            .unwrap_or_else(|| panic!("missing {heading:?}"));
        assert!(at > last, "{heading:?} is out of order");
        last = at;
    }
}

#[test]
fn the_download_links_come_first_and_match_the_release_assets() {
    let readme = repo_file("README.md");
    let first_image = readme.find("![").expect("a hero image");
    assert!(readme.find(EXE).expect("the Windows download") < first_image);
    // The unversioned AppImage, so the top link downloads the file instead of opening a page.
    assert!(
        readme
            .find(&format!(
                "**[Download for Linux]({LATEST}/tf2tools-linux-x86_64.AppImage)**"
            ))
            .expect("the Linux download")
            < first_image
    );
    let linux = readme
        .split_once("\n### Linux\n")
        .and_then(|(_, rest)| rest.split_once("\n### "))
        .expect("a Linux section")
        .0;
    for direct in ["tf2tools-linux-amd64.deb", "tf2tools-linux-x86_64.AppImage"] {
        assert!(
            linux.contains(&format!("]({LATEST}/{direct})")),
            "the Linux section lacks a direct {direct} link"
        );
    }
    let release = repo_file(".github/workflows/release.yml");
    for (asset, in_readme) in [
        ("TF2Tools.exe", "TF2Tools.exe"),
        (
            "tf2tools-linux-x86_64.AppImage",
            "tf2tools-linux-x86_64.AppImage",
        ),
        ("tf2tools-linux-amd64.deb", "tf2tools-linux-amd64.deb"),
    ] {
        assert!(
            release.contains(asset),
            "release.yml no longer names {asset}"
        );
        assert!(readme.contains(in_readme), "README lacks {in_readme}");
    }
}

#[test]
fn every_relative_link_and_image_exists() {
    let readme = repo_file("README.md");
    for target in link_targets(&readme) {
        if target.starts_with("https://") {
            continue;
        }
        let path = target.split('#').next().unwrap();
        assert!(repo_root().join(path).exists(), "broken link {target:?}");
    }
}

#[test]
fn the_screenshots_are_the_eleven_webp_images_within_budget() {
    let readme = repo_file("README.md");
    let mut used: Vec<String> = image_targets(&readme)
        .iter()
        .map(|t| t.to_string())
        .collect();
    used.sort();
    let mut expected: Vec<String> = IMAGES
        .iter()
        .map(|n| format!(".github/readme/{n}.webp"))
        .collect();
    expected.sort();
    assert_eq!(used, expected, "README images");

    let mut on_disk: Vec<String> = std::fs::read_dir(repo_root().join(".github/readme"))
        .unwrap()
        .map(|e| {
            format!(
                ".github/readme/{}",
                e.unwrap().file_name().to_string_lossy()
            )
        })
        .collect();
    on_disk.sort();
    assert_eq!(
        on_disk, expected,
        ".github/readme holds only the README images"
    );

    let total: u64 = expected
        .iter()
        .map(|p| std::fs::metadata(repo_root().join(p)).unwrap().len())
        .sum();
    assert!(total <= IMAGE_BUDGET, "images take {total} bytes");
}

#[test]
fn the_player_part_needs_no_terminal() {
    let readme = repo_file("README.md");
    let players = player_part(&readme).to_lowercase();
    for banned in [
        "```",
        "sudo ",
        "chmod",
        "cargo ",
        "`make",
        "apt install",
        "apt-get",
        "dpkg",
        "$ ",
        "~/",
        "/home/",
        "xdg_",
        "terminal",
        "command line",
    ] {
        assert!(
            !players.contains(banned),
            "the player part contains {banned:?}"
        );
    }
    assert!(
        readme.contains("](CONTRIBUTING.md)"),
        "no link to the developer guide"
    );
}

#[test]
fn the_crosshairs_caption_claims_only_what_the_shot_shows() {
    // The shot picks a bundled crosshair in the list; which weapons have one depends on the
    // library it was taken from, so the caption claims nothing about them.
    let readme = repo_file("README.md");
    let caption = readme
        .split_once("![The Crosshairs page")
        .expect("a Crosshairs shot")
        .1
        .split_once("](")
        .expect("a closed caption")
        .0;
    assert!(
        caption.contains("picked") && caption.contains("previewed"),
        "the caption says the preview shows the picked crosshair: {caption:?}"
    );
    assert!(
        !caption.contains("each") && !caption.contains("Scatterguns"),
        "the caption claims what the weapons use: {caption:?}"
    );
}

#[test]
fn the_install_note_points_at_the_newest_release_and_the_older_ones() {
    // Since 6.0.0 is GitHub's Latest, the links fetch it, and older versions stay on Releases.
    let readme = repo_file("README.md");
    let install = readme
        .split_once("\n## Download and install\n")
        .expect("an install section")
        .1;
    let note = install
        .split_once("\n### Windows\n")
        .expect("a Windows section")
        .0;
    for needle in [
        "newest",
        "Windows",
        "Linux",
        "TF2Tools 4",
        "](https://github.com/PhongGuy/TF2Tools/releases)",
    ] {
        assert!(note.contains(needle), "the install note lacks {needle:?}");
    }
    assert!(
        !note.contains("Pre-release"),
        "the note still sends players to a Pre-release"
    );
}

#[test]
fn the_readme_says_where_the_older_app_lives() {
    // Strangers arriving from an old link need to find TF2Tools 4 and know this one replaces it.
    let readme = repo_file("README.md");
    let intro = readme
        .split_once("\n## What it does\n")
        .expect("a feature section")
        .0;
    for needle in ["TF2Tools 6", "rebuilt", "v4 branch", "v4.8.0"] {
        assert!(intro.contains(needle), "the intro lacks {needle:?}");
    }
    assert!(readme.contains("](https://github.com/PhongGuy/TF2Tools/tree/v4)"));
}

#[test]
fn the_readme_credits_valve() {
    // The app bundles Valve's game artwork, so the README credits Valve and disclaims ties to it.
    let readme = repo_file("README.md");
    let license = readme
        .split_once("\n## License\n")
        .expect("a License section")
        .1;
    for needle in [
        "are the property of Valve Corporation",
        "are trademarks and/or registered trademarks of Valve Corporation",
        "not made, sponsored, endorsed or licensed by, or affiliated with Valve Corporation",
    ] {
        assert!(
            license.contains(needle),
            "the License section lacks {needle:?}"
        );
    }
}

#[test]
fn weapon_icons_come_from_the_game_not_the_wiki() {
    let readme = repo_file("README.md");
    let notice = repo_file("licenses/Valve-notice.txt");
    for (name, text) in [("README.md", &readme), ("Valve-notice.txt", &notice)] {
        let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            !flat.contains("wiki.teamfortress.com"),
            "{name} still names the wiki"
        );
        assert!(!flat.contains("TF2 Wiki"), "{name} still names the wiki");
        assert!(
            flat.contains("weapon icons in the crosshair list are read from your own TF2 install"),
            "{name} lacks where the icons come from"
        );
    }
    let privacy = readme
        .split_once("\n## Privacy\n")
        .expect("a Privacy section")
        .1
        .split_once("\n## ")
        .expect("a section after Privacy")
        .0;
    assert!(
        privacy.contains("The weapon icons in the crosshair list come from your own TF2 install."),
        "the Privacy section lacks the icon sentence"
    );
}

#[test]
fn the_logs_section_says_where_weapon_names_come_from() {
    let readme = repo_file("README.md");
    let (_, logs) = readme
        .split_once("### logs.tf stats")
        .expect("a logs.tf section");
    let (logs, _) = logs.split_once("\n### ").expect("a section after it");
    assert!(
        logs.contains("in-game names, read from your TF2 install"),
        "logs.tf section: {logs}"
    );
}

#[test]
fn the_logs_section_mentions_kill_feed_icons() {
    let readme = repo_file("README.md");
    let (_, logs) = readme
        .split_once("### logs.tf stats")
        .expect("a logs.tf section");
    let (logs, _) = logs.split_once("\n### ").expect("a section after it");
    assert!(
        logs.contains("kill feed icon, also read from your TF2 install"),
        "logs.tf section: {logs}"
    );
}
