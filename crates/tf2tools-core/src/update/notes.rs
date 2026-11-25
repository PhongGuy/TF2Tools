//! Signed release notes: the `notes.json` the manifest hashes, read strictly, cleaned of
//! control and bidi characters, and narrowed to the versions between the installed one
//! and the offer.

use std::cmp::Ordering;

use serde::Deserialize;

/// How fully the shown notes cover the gap between the installed and the offered version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NotesStatus {
    /// Every release from the one after the installed version up to the offer is shown.
    Complete,
    /// Some notes are shown, but the offer or an older release in the gap is missing.
    Partial,
    /// No notes are shown: none were published, they failed a check, or none are in range.
    Unavailable,
}

/// The kind of change a group of notes holds; unknown kinds read as `Other`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NotesKind {
    /// New features.
    Features,
    /// Bug fixes.
    Fixes,
    /// Usability improvements.
    Usability,
    /// Performance improvements.
    Performance,
    /// Any other kind, including kinds this version does not know.
    Other,
}

/// One release's notes: its version, the release it follows, its date, an optional intro
/// and its groups of changes, all cleaned and length-capped.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ReleaseNotes {
    version: String,
    parsed: semver::Version,
    previous: Option<String>,
    parsed_previous: Option<semver::Version>,
    date: String,
    intro: Option<String>,
    groups: Vec<NotesGroup>,
}

impl ReleaseNotes {
    /// The release's version as semver text, such as `6.3.0`.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// The published release this one follows, or None for the first release.
    pub fn previous(&self) -> Option<&str> {
        self.previous.as_deref()
    }

    /// The release date as the notes give it, such as `2026-12-01`.
    pub fn date(&self) -> &str {
        &self.date
    }

    /// A short paragraph introducing the release, if it has one.
    pub fn intro(&self) -> Option<&str> {
        self.intro.as_deref()
    }

    /// The release's groups of changes in their published order.
    pub fn groups(&self) -> &[NotesGroup] {
        &self.groups
    }
}

/// A titled group of changes within one release, such as "Fixes".
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct NotesGroup {
    kind: Option<NotesKind>,
    title: String,
    items: Vec<NotesItem>,
}

impl NotesGroup {
    /// The group's kind, or None when the notes give it no kind.
    pub fn kind(&self) -> Option<NotesKind> {
        self.kind
    }

    /// The group's heading as published.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// The group's changes in their published order.
    pub fn items(&self) -> &[NotesItem] {
        &self.items
    }
}

/// One change: its text and the pull request that made it, when known.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct NotesItem {
    text: String,
    pr: Option<u32>,
}

impl NotesItem {
    /// The change described in one line.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The pull request number, when the notes name one.
    pub fn pr(&self) -> Option<u32> {
        self.pr
    }
}

#[derive(Deserialize)]
struct RawNotes {
    schema: Option<u64>,
    releases: Vec<RawRelease>,
}

#[derive(Deserialize)]
struct RawRelease {
    version: String,
    previous: Option<String>,
    date: String,
    intro: Option<String>,
    groups: Vec<RawGroup>,
}

#[derive(Deserialize)]
struct RawGroup {
    kind: Option<String>,
    title: String,
    items: Vec<RawItem>,
}

#[derive(Deserialize)]
struct RawItem {
    text: String,
    pr: Option<u32>,
}

const TEXT_MAX: usize = 500;
const TITLE_MAX: usize = 100;
const INTRO_MAX: usize = 1000;
const FIELD_MAX: usize = 64;

/// Reads a notes.json document; Err with the cause unless `schema` is 1 and every version
/// and present `previous` is semver. Every string comes back cleaned and capped.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn parse(bytes: &[u8]) -> Result<Vec<ReleaseNotes>, String> {
    let raw: RawNotes = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    match raw.schema {
        Some(1) => {}
        Some(other) => return Err(format!("unsupported notes schema {other}")),
        None => return Err("notes have no schema".to_owned()),
    }
    raw.releases.into_iter().map(release).collect()
}

fn release(raw: RawRelease) -> Result<ReleaseNotes, String> {
    let semver = |s: &str| semver::Version::parse(s).map_err(|e| format!("bad version {s:?}: {e}"));
    let parsed = semver(&raw.version)?;
    let parsed_previous = raw.previous.as_deref().map(semver).transpose()?;
    let intro = raw
        .intro
        .map(|i| clean(&i, INTRO_MAX))
        .filter(|i| !i.trim().is_empty());
    let groups = raw
        .groups
        .into_iter()
        .map(|g| NotesGroup {
            kind: g.kind.as_deref().map(kind),
            title: clean(&g.title, TITLE_MAX),
            items: g
                .items
                .into_iter()
                .map(|i| NotesItem {
                    text: clean(&i.text, TEXT_MAX),
                    pr: i.pr,
                })
                .collect(),
        })
        .collect();
    Ok(ReleaseNotes {
        version: clean(&raw.version, FIELD_MAX),
        parsed,
        previous: raw.previous.map(|p| clean(&p, FIELD_MAX)),
        parsed_previous,
        date: clean(&raw.date, FIELD_MAX),
        intro,
        groups,
    })
}

fn kind(name: &str) -> NotesKind {
    match name {
        "features" => NotesKind::Features,
        "fixes" => NotesKind::Fixes,
        "usability" => NotesKind::Usability,
        "performance" => NotesKind::Performance,
        _ => NotesKind::Other,
    }
}

/// Keeps the releases with `installed < version <= offered`, newest first, and rates how
/// fully they cover that gap. A stable install never sees prerelease entries.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn in_range(
    all: Vec<ReleaseNotes>,
    installed: &semver::Version,
    offered: &semver::Version,
) -> (Vec<ReleaseNotes>, NotesStatus) {
    let stable = installed.pre.is_empty();
    let mut shown: Vec<ReleaseNotes> = all
        .into_iter()
        .filter(|r| !(stable && !r.parsed.pre.is_empty()))
        .filter(|r| {
            r.parsed.cmp_precedence(installed) == Ordering::Greater
                && r.parsed.cmp_precedence(offered) != Ordering::Greater
        })
        .collect();
    shown.sort_by(|a, b| b.parsed.cmp_precedence(&a.parsed));
    let (Some(newest), Some(oldest)) = (shown.first(), shown.last()) else {
        return (shown, NotesStatus::Unavailable);
    };
    // Complete only when the offer itself is listed and the oldest entry reaches back to
    // the installed version, so no release in the gap is silently missing.
    let reaches_offer = newest.parsed.cmp_precedence(offered) == Ordering::Equal;
    let reaches_installed = oldest
        .parsed_previous
        .as_ref()
        .is_none_or(|p| p.cmp_precedence(installed) != Ordering::Greater);
    let status = if reaches_offer && reaches_installed {
        NotesStatus::Complete
    } else {
        NotesStatus::Partial
    };
    (shown, status)
}

/// Makes untrusted text safe to show on one line: tab and newline become a space, other
/// control characters and bidi overrides are removed, and the result is cut to `max`
/// characters ending in `...`.
pub(crate) fn clean(text: &str, max: usize) -> String {
    let kept: Vec<char> = text
        .chars()
        .filter_map(|c| match c {
            '\t' | '\n' => Some(' '),
            '\u{0}'..='\u{1f}' | '\u{7f}'..='\u{9f}' => None,
            '\u{061c}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2066}'..='\u{2069}' => None,
            c => Some(c),
        })
        .collect();
    if kept.len() <= max {
        return kept.into_iter().collect();
    }
    let mut cut: String = kept[..max.saturating_sub(3)].iter().collect();
    cut.push_str("...");
    cut
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> semver::Version {
        semver::Version::parse(s).unwrap()
    }

    fn entry(version: &str, previous: Option<&str>) -> String {
        let previous = previous.map_or("null".to_owned(), |p| format!("\"{p}\""));
        format!(
            r#"{{"version":"{version}","previous":{previous},"date":"2026-11-01","groups":[{{"kind":"fixes","title":"Fixes","items":[{{"text":"Fix in {version}","pr":7}}]}}]}}"#
        )
    }

    fn doc(entries: &[String]) -> Vec<u8> {
        format!(r#"{{"schema":1,"releases":[{}]}}"#, entries.join(",")).into_bytes()
    }

    fn shown(releases: &[ReleaseNotes]) -> Vec<&str> {
        releases.iter().map(ReleaseNotes::version).collect()
    }

    fn history() -> Vec<ReleaseNotes> {
        parse(&doc(&[
            entry("6.3.0", Some("6.2.0")),
            entry("6.2.0", Some("6.1.0")),
            entry("6.1.0", Some("6.0.2")),
            entry("6.0.2", Some("6.0.1")),
            entry("6.0.1", None),
        ]))
        .unwrap()
    }

    #[test]
    fn the_golden_notes_json_parses() {
        let notes = parse(include_bytes!(
            "../../tests/fixtures/release-notes/notes.json"
        ))
        .unwrap();
        assert_eq!(shown(&notes), ["6.3.0", "6.2.0", "6.1.0", "6.0.2", "6.0.1"]);
        assert_eq!(notes[0].previous(), Some("6.2.0"));
        assert_eq!(notes[4].previous(), None);
        let first = &notes[0].groups()[0];
        assert_eq!(first.kind(), Some(NotesKind::Features));
        assert_eq!(first.items()[0].pr(), Some(1461));
        assert_eq!(notes[4].groups()[0].kind(), None);
        assert_eq!(notes[4].groups()[0].title(), "Changes");
    }

    #[test]
    fn fields_are_read_and_kinds_mapped() {
        let bytes = br#"{"schema":1,"extra":1,"releases":[{"version":"6.3.0","previous":"6.2.0","date":"2026-12-01","intro":"Hello.","extra":true,"groups":[
            {"kind":"features","title":"New features","items":[{"text":"A","pr":1}]},
            {"kind":"usability","title":"U","items":[{"text":"B"}]},
            {"kind":"performance","title":"P","items":[{"text":"C"}]},
            {"kind":"brand-new","title":"Mystery","items":[{"text":"D"}]},
            {"title":"Changes","items":[{"text":"E"}]}]}]}"#;
        let notes = parse(bytes).unwrap();
        let r = &notes[0];
        assert_eq!(
            (r.version(), r.previous(), r.date(), r.intro()),
            ("6.3.0", Some("6.2.0"), "2026-12-01", Some("Hello."))
        );
        let kinds: Vec<Option<NotesKind>> = r.groups().iter().map(NotesGroup::kind).collect();
        assert_eq!(
            kinds,
            [
                Some(NotesKind::Features),
                Some(NotesKind::Usability),
                Some(NotesKind::Performance),
                Some(NotesKind::Other),
                None
            ]
        );
        assert_eq!(r.groups()[3].title(), "Mystery");
        assert_eq!(r.groups()[0].items()[0].text(), "A");
        assert_eq!(r.groups()[0].items()[0].pr(), Some(1));
        assert_eq!(r.groups()[1].items()[0].pr(), None);
    }

    #[test]
    fn a_missing_previous_and_intro_are_none() {
        let notes = parse(
            br#"{"schema":1,"releases":[{"version":"6.0.1","date":"2026-11-04","groups":[]}]}"#,
        )
        .unwrap();
        assert_eq!(notes[0].previous(), None);
        assert_eq!(notes[0].intro(), None);
    }

    #[test]
    fn anything_but_schema_1_or_a_bad_version_is_refused() {
        for bad in [
            &br#"{"schema":2,"releases":[]}"#[..],
            br#"{"releases":[]}"#,
            br#"{"schema":1,"releases":[{"version":"six","date":"x","groups":[]}]}"#,
            br#"{"schema":1,"releases":[{"version":"6.1.0","previous":"v6.0.0","date":"x","groups":[]}]}"#,
            br#"{"schema":1,"releases":[{"version":"6.1.0","date":"x","groups":[{"title":"T","items":[{"text":"a","pr":-1}]}]}]}"#,
            b"not json",
        ] {
            assert!(parse(bad).is_err(), "{}", String::from_utf8_lossy(bad));
        }
    }

    #[test]
    fn clean_strips_controls_and_bidi_and_caps_length() {
        assert_eq!(clean("a\tb\nc\u{7}d\u{85}e\u{7f}", 100), "a b cde");
        assert_eq!(clean("x\u{202E}y\u{2066}z\u{200F}\u{061C}w", 100), "xyzw");
        let long = clean(&"é".repeat(600), 500);
        assert_eq!(long.chars().count(), 500);
        assert!(long.ends_with("..."));
        assert_eq!(clean("short", 500), "short");
    }

    #[test]
    fn parsed_strings_are_cleaned_with_their_caps() {
        let (text, title, intro) = ("t".repeat(600), "T".repeat(150), "i".repeat(1200));
        let bytes = format!(
            r#"{{"schema":1,"releases":[{{"version":"6.1.0","date":"2026\u202e-01","intro":"{intro}","groups":[{{"title":"{title}","items":[{{"text":"{text}"}}]}}]}}]}}"#
        );
        let notes = parse(bytes.as_bytes()).unwrap();
        let r = &notes[0];
        assert_eq!(r.date(), "2026-01");
        assert_eq!(r.intro().unwrap().chars().count(), 1000);
        assert_eq!(r.groups()[0].title().chars().count(), 100);
        assert_eq!(r.groups()[0].items()[0].text().chars().count(), 500);
    }

    #[test]
    fn only_versions_after_the_installed_one_up_to_the_offer_are_shown_newest_first() {
        let (list, status) = in_range(history(), &v("6.1.0"), &v("6.3.0"));
        assert_eq!(shown(&list), ["6.3.0", "6.2.0"]);
        assert_eq!(status, NotesStatus::Complete);
    }

    #[test]
    fn the_file_order_does_not_matter() {
        let notes = parse(&doc(&[
            entry("6.2.0", Some("6.1.0")),
            entry("6.3.0", Some("6.2.0")),
        ]))
        .unwrap();
        let (list, status) = in_range(notes, &v("6.1.0"), &v("6.3.0"));
        assert_eq!(shown(&list), ["6.3.0", "6.2.0"]);
        assert_eq!(status, NotesStatus::Complete);
    }

    #[test]
    fn a_list_that_stops_short_of_the_installed_version_is_partial() {
        let notes = parse(&doc(&[
            entry("6.3.0", Some("6.2.0")),
            entry("6.2.0", Some("6.1.0")),
        ]))
        .unwrap();
        let (list, status) = in_range(notes, &v("6.0.1"), &v("6.3.0"));
        assert_eq!(shown(&list), ["6.3.0", "6.2.0"]);
        assert_eq!(status, NotesStatus::Partial);
    }

    #[test]
    fn a_list_without_the_offered_version_is_partial() {
        let (list, status) = in_range(history(), &v("6.1.0"), &v("6.4.0"));
        assert_eq!(shown(&list), ["6.3.0", "6.2.0"]);
        assert_eq!(status, NotesStatus::Partial);
    }

    #[test]
    fn the_first_release_completes_the_list() {
        let (list, status) = in_range(history(), &v("6.0.0"), &v("6.3.0"));
        assert_eq!(list.len(), 5);
        assert_eq!(status, NotesStatus::Complete);
    }

    #[test]
    fn nothing_in_range_is_unavailable() {
        let (list, status) = in_range(history(), &v("6.3.0"), &v("6.3.0"));
        assert!(list.is_empty());
        assert_eq!(status, NotesStatus::Unavailable);
    }

    #[test]
    fn a_stable_install_never_lists_prereleases() {
        let notes = parse(&doc(&[
            entry("6.3.0", Some("6.3.0-rc.1")),
            entry("6.3.0-rc.1", Some("6.2.0")),
        ]))
        .unwrap();
        let (list, _) = in_range(notes.clone(), &v("6.2.0"), &v("6.3.0"));
        assert_eq!(shown(&list), ["6.3.0"]);
        let (list, _) = in_range(notes, &v("6.3.0-rc.0"), &v("6.3.0"));
        assert_eq!(shown(&list), ["6.3.0", "6.3.0-rc.1"]);
    }
}
