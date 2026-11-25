//! Turns a published GitHub release body into grouped, plain-text release notes.

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

/// Splits a trailing `by @login in URL` credit off an item, taking the PR number from the URL.
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
        || login.contains(char::is_whitespace)
        || url.is_empty()
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

/// Cuts text to at most `max` characters, the last three being `...` when it was cut.
fn cap(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut cut: String = text.chars().take(max - 3).collect();
    cut.push_str("...");
    cut
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

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
}
