//! Turns a published GitHub release body into grouped, plain-text release notes.

use std::path::Path;

/// Release-note section titles from `.github/release.yml` and the kind each maps to.
pub const KINDS: [(&str, &str); 5] = [
    ("New features", "features"),
    ("Fixes", "fixes"),
    ("Usability and accessibility", "usability"),
    ("Performance", "performance"),
    ("Other changes", "other"),
];
pub const ITEM_MAX: usize = 500;
pub const INTRO_MAX: usize = 1000;

/// The largest notes.json the release job ships, in bytes.
pub const NOTES_LIMIT: usize = 48 * 1024;
/// The most releases one notes.json holds.
pub const MAX_RELEASES: usize = 20;

/// One release as `gh release view --json tagName,body,publishedAt,isPrerelease,isDraft` prints it.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Published {
    #[serde(rename = "tagName")]
    pub tag: String,
    pub body: String,
    pub published_at: String,
    pub is_prerelease: bool,
    pub is_draft: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
struct Release {
    version: String,
    previous: Option<String>,
    date: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    intro: Option<String>,
    groups: Vec<Group>,
}

/// Reads every `*.json` file in `dir` as one published release, in file name order.
pub fn read_releases(dir: &Path) -> Result<Vec<Published>, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut paths = Vec::new();
    for entry in entries {
        let path = entry.map_err(|e| format!("{}: {e}", dir.display()))?.path();
        if path.extension().is_some_and(|ext| ext == "json") {
            paths.push(path);
        }
    }
    paths.sort();
    paths
        .iter()
        .map(|path| {
            let text =
                std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
        })
        .collect()
}

/// Builds compact notes.json from the published v6 releases plus `page` as `version`, newest
/// first, within `limit` bytes; fails naming the version whose body has no item or cannot fit.
pub fn build(
    published: &[Published],
    page: &str,
    version: &str,
    date: &str,
    limit: usize,
) -> Result<String, String> {
    let target = semver::Version::parse(version).map_err(|e| format!("{version}: {e}"))?;
    let floor = semver::Version::new(6, 0, 0);

    // Filter before parsing so an old app's body never reaches the parser; the page always wins.
    let mut entries: Vec<(semver::Version, String, &str)> = published
        .iter()
        .filter(|p| !p.is_draft && !p.is_prerelease)
        .filter_map(|p| {
            let found = semver::Version::parse(p.tag.strip_prefix('v').unwrap_or(&p.tag)).ok()?;
            let keep =
                found.cmp_precedence(&floor).is_ge() && found.cmp_precedence(&target).is_ne();
            let day = p
                .published_at
                .get(..10)
                .unwrap_or(&p.published_at)
                .to_owned();
            keep.then_some((found, day, p.body.as_str()))
        })
        .collect();
    entries.push((target, date.to_owned(), page));
    entries.sort_by(|a, b| b.0.cmp_precedence(&a.0));

    // Links to the next older version are set over the whole set, before any release is cut.
    let previous: Vec<Option<String>> = (0..entries.len())
        .map(|i| entries.get(i + 1).map(|older| older.0.to_string()))
        .collect();
    let mut releases = Vec::new();
    for ((found, day, body), previous) in entries.into_iter().zip(previous).take(MAX_RELEASES) {
        let parsed = parse_body(body).map_err(|e| format!("{found}: {e}"))?;
        releases.push(Release {
            version: found.to_string(),
            previous,
            date: day,
            intro: parsed.intro,
            groups: parsed.groups,
        });
    }

    let mut json = serialise(&releases)?;
    while json.len() > limit && releases.len() > 1 {
        releases.pop();
        json = serialise(&releases)?;
    }
    // Trim the newest release from its end, keeping one count item in its last group.
    let mut removed = 0;
    while json.len() > limit {
        let newest = &mut releases[0];
        if removed > 0
            && let Some(group) = newest.groups.last_mut()
        {
            group.items.pop();
        }
        let real: usize = newest.groups.iter().map(|group| group.items.len()).sum();
        if real <= 1 {
            return Err(format!(
                "{version}: the release notes do not fit in {limit} bytes"
            ));
        }
        if let Some(group) = newest.groups.last_mut() {
            group.items.pop();
            removed += 1;
            if group.items.is_empty() {
                newest.groups.pop();
            }
        }
        if let Some(group) = newest.groups.last_mut() {
            group.items.push(Item {
                text: format!("...and {removed} more"),
                pr: None,
            });
        }
        json = serialise(&releases)?;
    }
    Ok(json)
}

/// The notes.json document. A struct, not `json!`, so the key order never depends on
/// serde_json's `preserve_order` feature being unified in.
#[derive(serde::Serialize)]
struct Document<'a> {
    schema: u32,
    releases: &'a [Release],
}

fn serialise(releases: &[Release]) -> Result<String, String> {
    serde_json::to_string(&Document {
        schema: 1,
        releases,
    })
    .map_err(|e| e.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Item {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Group {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<&'static str>,
    pub title: String,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Body {
    pub intro: Option<String>,
    pub groups: Vec<Group>,
}

/// The kind a section title maps to, `"other"` for a title outside the release categories.
pub fn kind_for(title: &str) -> &'static str {
    KINDS
        .iter()
        .find(|(known, _)| *known == title)
        .map_or("other", |(_, kind)| kind)
}

/// Parses a release body into its intro and grouped items, or refuses a body with no item.
pub fn parse_body(body: &str) -> Result<Body, String> {
    let text = strip_comments(&body.replace("\r\n", "\n"));
    let lines: Vec<&str> = text.lines().collect();
    let start = lines
        .iter()
        .position(|line| line.trim_end() == "## What's Changed")
        .ok_or("the body has no \"## What's Changed\" section")?;
    let intro = cap(&plain(&lines[..start].join("\n")), INTRO_MAX);
    let intro = (!intro.is_empty()).then_some(intro);

    // Bullets open items in the current group; a bullet before any `###` opens a plain one.
    let mut groups: Vec<Group> = Vec::new();
    for line in lines[start + 1..]
        .iter()
        .take_while(|line| !line.starts_with("## "))
    {
        if line.starts_with("**Full Changelog**") {
            continue;
        }
        if let Some(title) = line.strip_prefix("### ") {
            let title = title.trim().to_owned();
            groups.push(Group {
                kind: Some(kind_for(&title)),
                title,
                items: Vec::new(),
            });
        } else if let Some(rest) = line.strip_prefix("* ").or_else(|| line.strip_prefix("- ")) {
            if groups.is_empty() {
                groups.push(Group {
                    kind: None,
                    title: "Changes".to_owned(),
                    items: Vec::new(),
                });
            }
            let (text, pr) = split_credit(rest.trim());
            let text = cap(&plain(text), ITEM_MAX);
            if let Some(group) = groups.last_mut().filter(|_| !text.is_empty()) {
                group.items.push(Item { text, pr });
            }
        }
    }
    groups.retain(|group| !group.items.is_empty());
    if groups.is_empty() {
        return Err("the body's \"What's Changed\" section has no item".to_owned());
    }
    Ok(Body { intro, groups })
}

/// Removes every `<!-- ... -->`; an unclosed comment runs to the end of the text.
fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find("<!--") {
        out.push_str(&rest[..open]);
        rest = match rest[open + 4..].find("-->") {
            Some(close) => &rest[open + 4 + close + 3..],
            None => "",
        };
    }
    out.push_str(rest);
    out
}

/// Splits a trailing `by @login in <web URL>` credit off an item, taking the PR number from the URL.
fn split_credit(item: &str) -> (&str, Option<u64>) {
    let Some(at) = item.rfind("by @") else {
        return (item, None);
    };
    if at > 0 && !item[..at].ends_with(' ') {
        return (item, None);
    }
    let Some((login, url)) = item[at + 4..].split_once(" in ") else {
        return (item, None);
    };
    if login.is_empty()
        || !(url.starts_with("http://") || url.starts_with("https://"))
        || login.contains(char::is_whitespace)
        || url.contains(char::is_whitespace)
    {
        return (item, None);
    }
    let pr = url
        .rsplit_once("/pull/")
        .map(|(_, number)| number)
        .filter(|number| !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|number| number.parse().ok());
    (item[..at].trim_end(), pr)
}

/// Reduces inline Markdown to plain text: links keep their text, emphasis and code marks go,
/// bare web addresses are dropped and whitespace collapses to single spaces.
fn plain(text: &str) -> String {
    let mut text = unlink(text);
    for mark in ["**", "__", "*"] {
        text = unwrap_pairs(&text, mark);
    }
    text.replace('`', "")
        .split_whitespace()
        .filter(|word| !word.starts_with("http://") && !word.starts_with("https://"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Replaces each `[text](url)` with its text.
fn unlink(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let link = after.find("](").and_then(|mid| {
            after[mid + 2..]
                .find(')')
                .map(|close| (mid, mid + 2 + close))
        });
        match link {
            Some((mid, close)) => {
                out.push_str(&rest[..open]);
                out.push_str(&after[..mid]);
                rest = &after[close + 1..];
            }
            None => {
                out.push_str(&rest[..=open]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Drops each matched pair of `mark`, keeping what sits between; an unmatched mark stays.
fn unwrap_pairs(text: &str, mark: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find(mark) {
        let inner = &rest[open + mark.len()..];
        let Some(close) = inner.find(mark) else {
            break;
        };
        out.push_str(&rest[..open]);
        out.push_str(&inner[..close]);
        rest = &inner[close + mark.len()..];
    }
    out.push_str(rest);
    out
}

/// Cuts text to at most `max` characters, ending in `...` after any trailing whitespace is trimmed.
fn cap(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut cut: String = text.chars().take(max - 3).collect();
    cut.truncate(cut.trim_end().len());
    cut.push_str("...");
    cut
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    pub(crate) fn fixtures() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/release-notes")
    }

    fn fixture_body(tag: &str) -> String {
        let text =
            std::fs::read_to_string(fixtures().join(format!("releases/{tag}.json"))).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        value["body"].as_str().unwrap().to_owned()
    }

    fn texts(group: &Group) -> Vec<&str> {
        group.items.iter().map(|i| i.text.as_str()).collect()
    }

    #[test]
    fn a_generated_body_groups_items_by_section_and_drops_credits() {
        let body = "Intro **line** with [a link](https://example.com).\r\n\r\n## What's Changed\r\n\r\n<!-- Release notes generated using configuration in .github/release.yml at v6.3.0 -->\r\n\r\n### New features\r\n* Search the Crosshair Store by @PhongGuy in https://github.com/PhongGuy/TF2Tools/pull/1364\r\n### Fixes\r\n* Fix `thing` with [docs](https://x.example/a) by @a-b in https://github.com/PhongGuy/TF2Tools/pull/12\r\n### Weird section\r\n- Odd one\r\n\r\n## New Contributors\r\n* @mrampazz made their first contribution in https://github.com/PhongGuy/TF2Tools/pull/1444\r\n\r\n**Full Changelog**: https://github.com/PhongGuy/TF2Tools/compare/v6.2.0...v6.3.0\r\n\r\n## Downloads\r\n- not an item\r\n";
        let parsed = parse_body(body).unwrap();
        assert_eq!(parsed.intro.as_deref(), Some("Intro line with a link."));
        let shape: Vec<(Option<&str>, &str, Vec<(&str, Option<u64>)>)> = parsed
            .groups
            .iter()
            .map(|g| {
                (
                    g.kind,
                    g.title.as_str(),
                    g.items.iter().map(|i| (i.text.as_str(), i.pr)).collect(),
                )
            })
            .collect();
        assert_eq!(
            shape,
            vec![
                (
                    Some("features"),
                    "New features",
                    vec![("Search the Crosshair Store", Some(1364))]
                ),
                (
                    Some("fixes"),
                    "Fixes",
                    vec![("Fix thing with docs", Some(12))]
                ),
                (Some("other"), "Weird section", vec![("Odd one", None)]),
            ]
        );
    }

    #[test]
    fn bullets_outside_a_section_form_one_changes_group() {
        let body = "## What's Changed\n\n- **A new app:** runs natively, see https://example.com/x now.\n- __Plain__ *words*\n\n**Full Changelog**: https://github.com/PhongGuy/TF2Tools/compare/v4.8.0...v6.0.1\n\n## Downloads\n";
        let parsed = parse_body(body).unwrap();
        assert_eq!(parsed.intro, None);
        assert_eq!(parsed.groups.len(), 1);
        assert_eq!(parsed.groups[0].kind, None);
        assert_eq!(parsed.groups[0].title, "Changes");
        assert_eq!(
            texts(&parsed.groups[0]),
            ["A new app: runs natively, see now.", "Plain words"]
        );
    }

    #[test]
    fn a_template_comment_alone_is_no_intro() {
        let body = "<!-- One or two sentences on what is new in this release, written for players. -->\n\n## What's Changed\n\n- Small fixes and improvements.\n\n## Downloads\n";
        let parsed = parse_body(body).unwrap();
        assert_eq!(parsed.intro, None);
        assert_eq!(texts(&parsed.groups[0]), ["Small fixes and improvements."]);
    }

    #[test]
    fn a_comment_spanning_lines_is_removed() {
        let body =
            "Hello <!-- a\nb --> there.\n\n## What's Changed\n\n<!--\n- hidden\n-->\n- shown\n";
        let parsed = parse_body(body).unwrap();
        assert_eq!(parsed.intro.as_deref(), Some("Hello there."));
        assert_eq!(texts(&parsed.groups[0]), ["shown"]);
    }

    #[test]
    fn a_body_with_no_items_is_refused() {
        for body in [
            "No section at all.\n",
            "## What's Changed\n\n## Downloads\n",
            "## What's Changed\n\n* by @x in https://github.com/PhongGuy/TF2Tools/pull/1\n\n## Downloads\n",
        ] {
            assert!(parse_body(body).is_err(), "{body:?}");
        }
    }

    #[test]
    fn a_long_item_is_cut_to_500_characters() {
        let body = format!("## What's Changed\n\n- {}\n", "é".repeat(600));
        let parsed = parse_body(&body).unwrap();
        let text = &parsed.groups[0].items[0].text;
        assert_eq!(text.chars().count(), 500);
        assert!(text.ends_with("..."));
    }

    #[test]
    fn the_real_6_0_1_body_is_one_changes_group() {
        let parsed = parse_body(&fixture_body("v6.0.1")).unwrap();
        assert!(
            parsed
                .intro
                .as_deref()
                .unwrap()
                .starts_with("The first release of TF2Tools 6:")
        );
        assert_eq!(parsed.groups.len(), 1);
        let group = &parsed.groups[0];
        assert_eq!(
            (group.kind, group.title.as_str(), group.items.len()),
            (None, "Changes", 10)
        );
        assert_eq!(
            group.items[0].text,
            "A new app: TF2Tools now runs as a regular desktop app on Windows and Linux, with no browser engine inside."
        );
        assert_eq!(
            group.items[9].text,
            "Updates: TF2Tools checks for new versions and installs them itself on Windows and with the AppImage."
        );
        assert!(group.items.iter().all(|i| i.pr.is_none()));
    }

    #[test]
    fn the_real_6_2_0_body_maps_every_section_to_its_kind() {
        let parsed = parse_body(&fixture_body("v6.2.0")).unwrap();
        assert!(
            parsed
                .intro
                .as_deref()
                .unwrap()
                .ends_with("thanks @mrampazz for the HUD card fix!")
        );
        let shape: Vec<(Option<&str>, &str, usize)> = parsed
            .groups
            .iter()
            .map(|g| (g.kind, g.title.as_str(), g.items.len()))
            .collect();
        assert_eq!(
            shape,
            vec![
                (Some("features"), "New features", 27),
                (Some("fixes"), "Fixes", 27),
                (Some("usability"), "Usability and accessibility", 21),
                (Some("performance"), "Performance", 6),
                (Some("other"), "Other changes", 3),
            ]
        );
        let first = &parsed.groups[0].items[0];
        assert_eq!(
            (first.text.as_str(), first.pr),
            (
                "Install, update and remove GameBanana maps from the Maps page",
                Some(1253)
            )
        );
        assert!(
            parsed
                .groups
                .iter()
                .flat_map(|g| &g.items)
                .all(|i| !i.text.contains("http") && !i.text.contains(" by @"))
        );
    }

    #[test]
    fn every_real_v6_body_and_the_page_fixture_parse() {
        for tag in ["v6.0.1", "v6.0.2", "v6.1.0", "v6.2.0"] {
            let parsed = parse_body(&fixture_body(tag)).unwrap_or_else(|e| panic!("{tag}: {e}"));
            assert!(parsed.intro.is_some(), "{tag}");
        }
        let page = std::fs::read_to_string(fixtures().join("page-6.3.0.md")).unwrap();
        let parsed = parse_body(&page).unwrap();
        assert_eq!(
            parsed.groups[1].items[0].text,
            "The Maps page keeps its scroll place, see the issue"
        );
        assert_eq!(parsed.groups[1].items[0].pr, Some(1462));
    }

    #[test]
    fn the_kind_table_matches_the_release_categories() {
        let config = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.github/release.yml"),
        )
        .unwrap();
        let titles: Vec<&str> = config
            .lines()
            .filter_map(|l| l.trim().strip_prefix("- title: "))
            .collect();
        assert_eq!(titles, KINDS.map(|(title, _)| title));
        assert_eq!(
            KINDS.map(|(_, kind)| kind),
            ["features", "fixes", "usability", "performance", "other"]
        );
        assert_eq!(kind_for("Something new"), "other");
    }

    #[test]
    fn a_credit_needs_a_web_address() {
        let parsed = parse_body("## What's Changed\n\n- Done by @x in v6\n").unwrap();
        assert_eq!(texts(&parsed.groups[0]), ["Done by @x in v6"]);
        assert_eq!(parsed.groups[0].items[0].pr, None);
    }

    #[test]
    fn a_cut_drops_trailing_whitespace_before_the_dots() {
        assert_eq!(cap("abcd efgh", 8), "abcd...");
        assert_eq!(cap("abcdefghij", 8), "abcde...");
        assert_eq!(cap("short", 8), "short");
    }

    fn published(tag: &str, body: &str, date: &str, prerelease: bool, draft: bool) -> Published {
        Published {
            tag: tag.to_owned(),
            body: body.to_owned(),
            published_at: format!("{date}T12:00:00Z"),
            is_prerelease: prerelease,
            is_draft: draft,
        }
    }
    const SMALL: &str = "## What's Changed\n\n- A change.\n\n## Downloads\n";
    fn page() -> String {
        std::fs::read_to_string(fixtures().join("page-6.3.0.md")).unwrap()
    }
    fn doc(json: &str) -> serde_json::Value {
        serde_json::from_str(json).unwrap()
    }
    fn versions(value: &serde_json::Value) -> Vec<String> {
        value["releases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["version"].as_str().unwrap().to_owned())
            .collect()
    }
    fn body_of(items: usize, width: usize) -> String {
        let mut body = String::from("## What's Changed\n\n### Fixes\n");
        for n in 0..items {
            body.push_str(&format!("* {n:04} {}\n", "x".repeat(width)));
        }
        body
    }

    #[test]
    fn notes_json_keys_come_out_in_document_order() {
        let releases = read_releases(&fixtures().join("releases")).unwrap();
        let json = build(&releases, &page(), "6.3.0", "2026-12-01", NOTES_LIMIT).unwrap();
        assert!(
            json.starts_with(r#"{"schema":1,"releases":[{"version":""#),
            "{}",
            &json[..json.len().min(80)]
        );
    }

    #[test]
    fn the_real_history_builds_newest_first_with_previous_links() {
        let releases = read_releases(&fixtures().join("releases")).unwrap();
        let json = build(&releases, &page(), "6.3.0", "2026-12-01", NOTES_LIMIT).unwrap();
        let value = doc(&json);
        assert_eq!(value["schema"], 1);
        assert_eq!(
            versions(&value),
            ["6.3.0", "6.2.0", "6.1.0", "6.0.2", "6.0.1"]
        );
        let previous: Vec<serde_json::Value> = value["releases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["previous"].clone())
            .collect();
        assert_eq!(
            previous,
            [
                json!("6.2.0"),
                json!("6.1.0"),
                json!("6.0.2"),
                json!("6.0.1"),
                serde_json::Value::Null
            ]
        );
        assert_eq!(value["releases"][0]["date"], "2026-12-01");
        assert_eq!(
            value["releases"][0]["intro"],
            "Players can read what an update brings before they install it."
        );
        assert_eq!(value["releases"][1]["date"], "2026-11-25");
        assert!(json.len() <= NOTES_LIMIT);
        assert!(!json.contains('\n'), "compact JSON");
    }

    #[test]
    fn old_apps_prereleases_drafts_and_odd_tags_are_left_out() {
        let releases = vec![
            published("v6.1.0", SMALL, "2026-11-14", false, false),
            published("v6.1.1-rc.1", SMALL, "2026-11-15", true, false),
            published("v6.1.1", SMALL, "2026-11-16", false, true),
            published("v4.8.0", "Old app notes.", "2022-09-02", false, false),
            published("not-a-version", SMALL, "2022-09-02", false, false),
        ];
        let value = doc(&build(&releases, SMALL, "6.2.0", "2026-11-25", NOTES_LIMIT).unwrap());
        assert_eq!(versions(&value), ["6.2.0", "6.1.0"]);
        assert_eq!(value["releases"][0]["previous"], "6.1.0");
        assert_eq!(value["releases"][1]["previous"], serde_json::Value::Null);
    }

    #[test]
    fn the_tag_being_built_is_kept_even_as_a_prerelease_and_replaces_a_published_copy() {
        let releases = vec![
            published("v6.2.0", SMALL, "2026-11-25", false, false),
            published(
                "v6.3.0-rc.1",
                "## What's Changed\n\n- Stale text.\n",
                "2026-11-30",
                true,
                false,
            ),
        ];
        let value = doc(&build(
            &releases,
            "## What's Changed\n\n- Fresh text.\n",
            "6.3.0-rc.1",
            "2026-12-01",
            NOTES_LIMIT,
        )
        .unwrap());
        assert_eq!(versions(&value), ["6.3.0-rc.1", "6.2.0"]);
        assert_eq!(
            value["releases"][0]["groups"][0]["items"][0]["text"],
            "Fresh text."
        );
        assert_eq!(value["releases"][0]["date"], "2026-12-01");
    }

    #[test]
    fn at_most_20_releases_are_kept_and_previous_is_set_before_the_cut() {
        let releases: Vec<Published> = (0..25)
            .map(|minor| published(&format!("v6.{minor}.0"), SMALL, "2026-11-01", false, false))
            .collect();
        let value = doc(&build(&releases, SMALL, "6.25.0", "2026-12-01", NOTES_LIMIT).unwrap());
        let kept = versions(&value);
        assert_eq!(kept.len(), 20);
        assert_eq!(kept[0], "6.25.0");
        assert_eq!(kept[19], "6.6.0");
        assert_eq!(value["releases"][19]["previous"], "6.5.0");
    }

    #[test]
    fn over_the_cap_the_oldest_releases_go_first() {
        let releases: Vec<Published> = (0..6)
            .map(|minor| {
                published(
                    &format!("v6.{minor}.0"),
                    &body_of(40, 300),
                    "2026-11-01",
                    false,
                    false,
                )
            })
            .collect();
        let json = build(
            &releases,
            &body_of(40, 300),
            "6.6.0",
            "2026-12-01",
            NOTES_LIMIT,
        )
        .unwrap();
        assert!(json.len() <= NOTES_LIMIT, "{}", json.len());
        let kept = versions(&doc(&json));
        assert!((2..7).contains(&kept.len()), "{kept:?}");
        let expected: Vec<String> = (0..=6)
            .rev()
            .take(kept.len())
            .map(|m| format!("6.{m}.0"))
            .collect();
        assert_eq!(kept, expected);
    }

    #[test]
    fn a_newest_release_over_the_cap_loses_items_and_says_how_many() {
        let json = build(&[], &body_of(400, 300), "6.3.0", "2026-12-01", NOTES_LIMIT).unwrap();
        assert!(json.len() <= NOTES_LIMIT, "{}", json.len());
        let value = doc(&json);
        let items = value["releases"][0]["groups"][0]["items"]
            .as_array()
            .unwrap();
        let last = items.last().unwrap();
        let more: usize = last["text"]
            .as_str()
            .unwrap()
            .strip_prefix("...and ")
            .unwrap()
            .strip_suffix(" more")
            .unwrap()
            .parse()
            .unwrap();
        assert!(more > 0);
        assert_eq!(items.len() - 1 + more, 400);
        assert!(last.get("pr").is_none());
    }

    #[test]
    fn a_cut_that_empties_the_last_group_drops_it_and_counts_on_the_group_before() {
        let wide = "x".repeat(300);
        let full = format!(
            "## What's Changed\n\n### Fixes\n* fix 0 {wide}\n* fix 1 {wide}\n* fix 2 {wide}\n### Performance\n* fast 0\n* fast 1\n"
        );
        // The expected result is the first two fixes plus the count of the three removed items.
        let kept = format!(
            "## What's Changed\n\n### Fixes\n* fix 0 {wide}\n* fix 1 {wide}\n* ...and 3 more\n"
        );
        let expected = build(&[], &kept, "6.3.0", "2026-12-01", NOTES_LIMIT).unwrap();
        let json = build(&[], &full, "6.3.0", "2026-12-01", expected.len()).unwrap();
        assert_eq!(json, expected);
        let groups = doc(&json)["releases"][0]["groups"].clone();
        assert_eq!(groups.as_array().unwrap().len(), 1);
        assert_eq!(groups[0]["items"][2]["text"], "...and 3 more");
    }

    #[test]
    fn a_newest_release_that_cannot_fit_fails() {
        let err = build(&[], &body_of(3, 300), "6.3.0", "2026-12-01", 200).unwrap_err();
        assert!(err.contains("6.3.0"), "{err}");
    }

    #[test]
    fn a_page_or_included_body_with_no_items_fails_naming_its_version() {
        let empty = published(
            "v6.2.0",
            "## What's Changed\n\n## Downloads\n",
            "2026-11-25",
            false,
            false,
        );
        let err = build(&[empty], SMALL, "6.3.0", "2026-12-01", NOTES_LIMIT).unwrap_err();
        assert!(err.contains("6.2.0"), "{err}");
        let err = build(&[], "no section", "6.3.0", "2026-12-01", NOTES_LIMIT).unwrap_err();
        assert!(err.contains("6.3.0"), "{err}");
    }

    #[test]
    fn read_releases_reads_every_json_file_in_the_dir() {
        let releases = read_releases(&fixtures().join("releases")).unwrap();
        let mut tags: Vec<&str> = releases.iter().map(|r| r.tag.as_str()).collect();
        tags.sort();
        assert_eq!(tags, ["v6.0.1", "v6.0.2", "v6.1.0", "v6.2.0"]);
        assert!(releases.iter().all(|r| !r.is_draft && !r.is_prerelease));
    }
}
