//! GameBanana Mod Index URLs and the listing parser behind the Addon Store.

use serde_json::Value;

use super::addon_catalog::{
    AddonCard, AddonCategory, AddonCredit, AddonCreditGroup, AddonFile, AddonProfile, AddonSort,
};
use super::gamebanana::{TF2_GAME_ID, int, optional_text, preview_images, text};
use crate::GbId;

/// Records asked for per Index page, the most GameBanana serves.
pub(crate) const INDEX_PAGE_SIZE: u32 = 50;

/// The Mod Index URL for `page` of one category, filtered to TF2.
pub(crate) fn index_url(base: &str, category: u32, sort: AddonSort, page: u32) -> String {
    let sort = match sort {
        AddonSort::MostDownloaded => "Generic_MostDownloaded",
        AddonSort::Newest => "Generic_Newest",
    };
    format!(
        "{base}/Mod/Index?_nPage={page}&_nPerpage={INDEX_PAGE_SIZE}\
         &_aFilters[Generic_Category]={category}&_aFilters[Generic_Game]={TF2_GAME_ID}\
         &_sSort={sort}"
    )
}

/// The Search URL for `page` of mods matching `text` in TF2. Search spans every category,
/// so the caller filters records by root category.
pub(crate) fn search_url(base: &str, text: &str, page: u32) -> String {
    let text = encode_query(text);
    format!(
        "{base}/Util/Search/Results?_sModelName=Mod&_idGameRow={TF2_GAME_ID}\
         &_sSearchString={text}&_nPage={page}"
    )
}

/// The child categories of `parent`, A to Z. Naming the game here returns the game's root
/// categories instead of the children, so it is left out.
pub(crate) fn categories_url(base: &str, parent: u32) -> String {
    format!("{base}/Mod/Categories?_idCategoryRow={parent}&_sSort=a_to_z")
}

/// Percent-encodes everything outside RFC 3986's unreserved set, `/` and spaces included.
fn encode_query(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Parses a `Mod/Categories` reply, a bare array. Rows without an id or a name are skipped.
pub(crate) fn parse_categories(body: &[u8]) -> Result<Vec<AddonCategory>, serde_json::Error> {
    let rows: Vec<Value> = serde_json::from_slice(body)?;
    Ok(rows
        .iter()
        .filter_map(|row| {
            let id = u32::try_from(int(&row["_idRow"]))
                .ok()
                .filter(|&id| id > 0)?;
            let name = text(&row["_sName"]);
            (!name.is_empty()).then(|| AddonCategory {
                id,
                name,
                items: u64::try_from(int(&row["_nItemCount"])).unwrap_or(0),
            })
        })
        .collect())
}

/// The id at the end of a category profile URL such as `.../mods/cats/1105`, with or
/// without a trailing slash.
pub(crate) fn category_id_from_url(url: &str) -> Option<u32> {
    let url = url.strip_suffix('/').unwrap_or(url);
    url.rsplit('/').next()?.parse().ok()
}

/// One parsed Index or Search page.
#[derive(Debug, Default)]
pub(crate) struct ListingPage {
    pub(crate) records: Vec<AddonRecord>,
    /// `_aMetadata._bIsComplete`: no page follows this one.
    pub(crate) complete: bool,
}

/// One listed item: its card plus the root category the section filter checks.
#[derive(Debug, Clone)]
pub(crate) struct AddonRecord {
    pub(crate) card: AddonCard,
    pub(crate) root_category: Option<u32>,
}

/// Parses an Index or Search reply. Records without an id are skipped, and a reply
/// without metadata counts as complete so a caller never pages past it forever.
pub(crate) fn parse_listing(body: &[u8]) -> Result<ListingPage, serde_json::Error> {
    let reply: Value = serde_json::from_slice(body)?;
    let records = reply["_aRecords"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(parse_record)
        .collect();
    let complete = reply["_aMetadata"]["_bIsComplete"]
        .as_bool()
        .unwrap_or(true);
    Ok(ListingPage { records, complete })
}

fn parse_record(record: &Value) -> Option<AddonRecord> {
    let id = u64::try_from(int(&record["_idRow"]))
        .ok()
        .filter(|&id| id > 0)?;
    // Ratings mark mature content even on a record GameBanana shows or hides outright.
    let mature = record["_sInitialVisibility"] == "warn" || record["_bHasContentRatings"] == true;
    let subcategory = &record["_aSubCategory"];
    let card = AddonCard {
        id: GbId::new(id),
        name: text(&record["_sName"]),
        author: text(&record["_aSubmitter"]["_sName"]),
        thumbnail_url: preview_images(record).1,
        like_count: int(&record["_nLikeCount"]),
        date_added: int(&record["_tsDateAdded"]),
        category_id: subcategory["_sProfileUrl"]
            .as_str()
            .and_then(category_id_from_url),
        category_name: optional_text(&subcategory["_sName"]),
        obsolete: record["_bIsObsolete"] == true,
        mature,
        profile_url: text(&record["_sProfileUrl"]),
    };
    Some(AddonRecord {
        card,
        root_category: record["_aRootCategory"]["_sProfileUrl"]
            .as_str()
            .and_then(category_id_from_url),
    })
}

/// The ProfilePage URL of mod `id`.
pub(crate) fn profile_url(base: &str, id: GbId) -> String {
    format!("{base}/Mod/{id}/ProfilePage")
}

/// A parsed ProfilePage plus whether GameBanana has trashed, withheld or hidden the item.
#[derive(Debug)]
pub(crate) struct ParsedProfile {
    pub(crate) profile: AddonProfile,
    pub(crate) gone: bool,
}

/// Parses a ProfilePage reply; an error only when the body is not a JSON object with an id.
pub(crate) fn parse_profile(body: &[u8]) -> Result<ParsedProfile, String> {
    let record: Value = serde_json::from_slice(body).map_err(|e| e.to_string())?;
    let id = u64::try_from(int(&record["_idRow"]))
        .ok()
        .filter(|&id| id > 0)
        .ok_or_else(|| "the profile has no _idRow".to_owned())?;
    // GameBanana sends ratings as a code-to-label map; an array of labels is read too.
    let rated = &record["_aContentRatings"];
    let mut ratings: Vec<String> = match (rated.as_object(), rated.as_array()) {
        (Some(map), _) => map.values().filter_map(optional_text).collect(),
        (None, Some(list)) => list.iter().filter_map(optional_text).collect(),
        (None, None) => Vec::new(),
    };
    ratings.sort();
    let credits = record["_aCredits"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|group| AddonCreditGroup {
            name: text(&group["_sGroupName"]),
            credits: group["_aAuthors"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .map(|author| AddonCredit {
                    name: text(&author["_sName"]),
                    role: text(&author["_sRole"]),
                })
                .collect(),
        })
        .collect();
    let mut files: Vec<AddonFile> = record["_aFiles"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(parse_file)
        .collect();
    files.sort_by_key(|file| std::cmp::Reverse(file.date_added));
    let profile = AddonProfile {
        id: GbId::new(id),
        name: text(&record["_sName"]),
        author: text(&record["_aSubmitter"]["_sName"]),
        profile_url: text(&record["_sProfileUrl"]),
        description: html_to_text(record["_sText"].as_str().unwrap_or("")),
        images: preview_images(&record).0,
        download_count: u64::try_from(int(&record["_nDownloadCount"])).unwrap_or(0),
        like_count: int(&record["_nLikeCount"]),
        obsolete: record["_bIsObsolete"] == true,
        mature: record["_sInitialVisibility"] == "warn" || !ratings.is_empty(),
        ratings,
        credits,
        files,
    };
    let gone = ["_bIsTrashed", "_bIsWithheld", "_bIsPrivate"]
        .iter()
        .any(|flag| record[flag] == true);
    Ok(ParsedProfile { profile, gone })
}

/// One downloadable file, skipped like `gamebanana::parse_files` when it has no address.
fn parse_file(file: &Value) -> Option<AddonFile> {
    let download_url = text(&file["_sDownloadUrl"]);
    if download_url.is_empty() {
        return None;
    }
    Some(AddonFile {
        id: u64::try_from(int(&file["_idRow"])).unwrap_or(0),
        name: text(&file["_sFile"]),
        size: file["_nFilesize"].as_u64(),
        date_added: int(&file["_tsDateAdded"]),
        md5: optional_text(&file["_sMd5Checksum"]).filter(|md5| !md5.is_empty()),
        av_result: text(&file["_sAvResult"]),
        download_url,
    })
}

/// Turns a GameBanana description into plain lines. Tags go first and entities are decoded
/// after, so an escaped `&lt;3&gt;` stays text. `<br>` and a closing paragraph, div or list
/// item end a line; headings, lists, quotes, tables and rules start their own line.
pub(crate) fn html_to_text(html: &str) -> String {
    let mut stripped = String::with_capacity(html.len());
    let mut rest = html;
    // Whether only blanks follow the last line break; tracked as text is pushed, so a block
    // tag never rescans what came before it.
    let mut at_line_start = true;
    // As in a browser, a newline in the source is a space; only tags break lines.
    let push_text = |out: &mut String, at_line_start: &mut bool, text: &str| {
        for c in text.chars() {
            let c = if c == '\n' || c == '\r' { ' ' } else { c };
            *at_line_start &= c.is_whitespace();
            out.push(c);
        }
    };
    while let Some(open) = rest.find('<') {
        push_text(&mut stripped, &mut at_line_start, &rest[..open]);
        let Some(close) = rest[open..].find('>') else {
            // An unclosed `<` is not a tag, so it stays as text.
            push_text(&mut stripped, &mut at_line_start, &rest[open..]);
            rest = "";
            break;
        };
        let tag = &rest[open + 1..open + close];
        let closing = tag.starts_with('/');
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .map(|c| c.to_ascii_lowercase())
            .collect();
        let ends_line = closing && matches!(name.as_str(), "p" | "div" | "li");
        let block = matches!(
            name.as_str(),
            "h1" | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "ul"
                | "ol"
                | "li"
                | "p"
                | "div"
                | "blockquote"
                | "table"
                | "tr"
                | "hr"
        );
        if name == "br" || ends_line {
            stripped.push('\n');
            at_line_start = true;
        } else if block {
            // A block starts on a fresh line, but adds no blank one when already there.
            if !at_line_start {
                stripped.push('\n');
                at_line_start = true;
            }
        } else if !closing && matches!(name.as_str(), "td" | "th") {
            // Cells in a row stay apart; the space is trimmed off a line's first cell.
            stripped.push(' ');
        }
        rest = &rest[open + close + 1..];
    }
    push_text(&mut stripped, &mut at_line_start, rest);

    let decoded = decode_entities(&stripped);
    let lines: Vec<&str> = decoded.lines().map(str::trim).collect();
    let mut text = String::with_capacity(decoded.len());
    let mut newlines = 0;
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            newlines += 1;
        }
        if !line.is_empty() {
            // A run of blank lines shrinks to one empty line between paragraphs.
            text.push_str(&"\n".repeat(newlines.min(2)));
            text.push_str(line);
            newlines = 0;
        }
    }
    text.trim().to_owned()
}

/// Decodes the entities descriptions use, in one pass so `&amp;lt;` stays `&lt;`.
fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        // An entity is at most ten bytes long, so look for its ';' only that far;
        // scanning to the end at every '&' is quadratic on hostile text.
        let window = &rest.as_bytes()[..rest.len().min(11)];
        let end = window.iter().position(|&b| b == b';');
        let decoded = end.and_then(|end| {
            let entity = &rest[1..end];
            let c = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "#39" | "apos" => Some('\''),
                "nbsp" => Some(' '),
                _ => entity
                    .strip_prefix('#')
                    .and_then(|n| match n.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok(),
                        None => n.parse().ok(),
                    })
                    .and_then(char::from_u32),
            }?;
            Some((c, end))
        });
        match decoded {
            Some((c, end)) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/store")
            .join(name);
        std::fs::read(&path).unwrap()
    }

    #[test]
    fn a_category_id_is_the_last_segment_of_its_url() {
        assert_eq!(
            category_id_from_url("https://gamebanana.com/mods/cats/1105"),
            Some(1105)
        );
        assert_eq!(
            category_id_from_url("https://gamebanana.com/mods/cats/1105/"),
            Some(1105)
        );
        assert_eq!(
            category_id_from_url("https://gamebanana.com/mods/cats/"),
            None
        );
        assert_eq!(
            category_id_from_url("https://gamebanana.com/mods/cats/x9"),
            None
        );
        assert_eq!(category_id_from_url(""), None);
    }

    #[test]
    fn the_index_url_filters_on_category_and_game_with_fifty_a_page() {
        assert_eq!(
            index_url("https://gb/apiv11", 1644, AddonSort::MostDownloaded, 3),
            "https://gb/apiv11/Mod/Index?_nPage=3&_nPerpage=50&_aFilters[Generic_Category]=1644\
             &_aFilters[Generic_Game]=297&_sSort=Generic_MostDownloaded"
        );
        assert!(index_url("b", 7951, AddonSort::Newest, 1).ends_with("&_sSort=Generic_Newest"));
    }

    #[test]
    fn a_real_effects_page_parses_with_and_without_a_subcategory() {
        let body = fixture("gb-addons-effects-newest.json");
        let raw: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let raw = raw["_aRecords"].as_array().unwrap();
        let page = parse_listing(&body).unwrap();
        assert_eq!(page.records.len(), raw.len());
        assert!(!page.complete);
        assert!(
            page.records.iter().any(|r| r.card.category_id.is_none()),
            "a record without _aSubCategory"
        );
        assert!(
            page.records
                .iter()
                .any(|r| r.card.category_id == Some(1105)),
            "a crosshair"
        );
        assert!(page.records.iter().all(|r| r.root_category == Some(1090)));
        for (record, raw) in page.records.iter().zip(raw) {
            assert!(record.card.id.get() > 0 && !record.card.name.is_empty());
            let warn = raw["_sInitialVisibility"] == "warn" || raw["_bHasContentRatings"] == true;
            assert_eq!(record.card.mature, warn, "{}", record.card.id);
            assert_eq!(record.card.obsolete, raw["_bIsObsolete"] == true);
        }
        assert!(
            page.records.iter().any(|r| r.card.mature),
            "the page holds a mature record"
        );
    }

    #[test]
    fn a_card_takes_the_first_100px_rendition_as_its_thumbnail() {
        let body = serde_json::json!({"_aMetadata": {"_bIsComplete": true}, "_aRecords": [{
            "_idRow": 7, "_sName": "Toon", "_aSubmitter": {"_sName": "vex"}, "_nLikeCount": 4,
            "_sProfileUrl": "https://gamebanana.com/mods/7", "_tsDateAdded": 10,
            "_aSubCategory": {"_sName": "Muzzle Flashes", "_sProfileUrl": "https://gamebanana.com/mods/cats/1093"},
            "_aPreviewMedia": {"_aImages": [{"_sBaseUrl": "https://img", "_sFile": "a.jpg", "_sFile100": "100-a.jpg"}]}
        }]});
        let page = parse_listing(body.to_string().as_bytes()).unwrap();
        let card = &page.records[0].card;
        assert_eq!(card.thumbnail_url.as_deref(), Some("https://img/100-a.jpg"));
        assert_eq!(
            (card.author.as_str(), card.like_count, card.date_added),
            ("vex", 4, 10)
        );
        assert_eq!(
            (card.category_id, card.category_name.as_deref()),
            (Some(1093), Some("Muzzle Flashes"))
        );
        assert!(page.complete && !card.mature && !card.obsolete);
    }

    #[test]
    fn a_record_is_mature_when_warned_or_rated_whatever_its_visibility() {
        let record = |id: u64, visibility: &str, rated: bool| {
            serde_json::json!({
                "_idRow": id, "_sName": "x", "_aSubmitter": {"_sName": "vex"}, "_nLikeCount": 0, "_tsDateAdded": 1,
                "_sProfileUrl": format!("https://gamebanana.com/mods/{id}"),
                "_sInitialVisibility": visibility, "_bHasContentRatings": rated,
            })
        };
        let body = serde_json::json!({"_aRecords": [
            record(1, "show", false), record(2, "warn", false), record(3, "hide", true), record(4, "show", true),
        ]});
        let page = parse_listing(body.to_string().as_bytes()).unwrap();
        let mature: Vec<_> = page
            .records
            .iter()
            .map(|r| (r.card.id.get(), r.card.mature))
            .collect();
        assert_eq!(mature, [(1, false), (2, true), (3, true), (4, true)]);
    }

    #[test]
    fn a_reply_without_metadata_counts_as_complete_and_a_non_json_body_fails() {
        assert!(parse_listing(br#"{"_aRecords": []}"#).unwrap().complete);
        assert!(parse_listing(b"<html>busy</html>").is_err());
    }

    #[test]
    fn the_search_url_encodes_the_text_and_names_the_game() {
        assert_eq!(
            search_url("b", "pink & red/x", 2),
            "b/Util/Search/Results?_sModelName=Mod&_idGameRow=297&_sSearchString=pink%20%26%20red%2Fx&_nPage=2"
        );
        assert!(search_url("b", "café", 1).contains("_sSearchString=caf%C3%A9&"));
    }

    #[test]
    fn a_flood_of_bare_ampersands_decodes_quickly_and_unchanged() {
        let text = "&".repeat(200_000);
        let started = std::time::Instant::now();
        assert_eq!(decode_entities(&text), text);
        assert!(started.elapsed() < std::time::Duration::from_millis(100));
    }

    #[test]
    fn the_categories_url_never_names_the_game() {
        let url = categories_url("b", 1644);
        assert_eq!(url, "b/Mod/Categories?_idCategoryRow=1644&_sSort=a_to_z");
        assert!(!url.contains("_idGameRow"));
    }

    #[test]
    fn the_search_fixture_carries_the_fields_filtering_needs() {
        let body = fixture("gb-addons-search-pink.json");
        let raw: serde_json::Value = serde_json::from_slice(&body).unwrap();
        for record in raw["_aRecords"].as_array().unwrap() {
            assert!(
                record.get("_sInitialVisibility").is_some(),
                "{}",
                record["_idRow"]
            );
            assert!(
                record["_aRootCategory"]["_sProfileUrl"].is_string(),
                "{}",
                record["_idRow"]
            );
        }
        let page = parse_listing(&body).unwrap();
        assert!(page.records.iter().all(|r| r.root_category.is_some()));
        assert!(
            page.records.iter().any(|r| r.root_category == Some(7951)),
            "pink finds skins"
        );
    }

    #[test]
    fn the_real_category_lists_parse() {
        let guis = parse_categories(&fixture("gb-addons-categories-1644.json")).unwrap();
        assert!(guis.iter().any(|c| c.id() == 1649 && c.name() == "HUDs"));
        let skins = parse_categories(&fixture("gb-addons-categories-7951.json")).unwrap();
        assert!(
            skins
                .iter()
                .any(|c| c.id() == 7962 && c.name().contains("Scout"))
        );
        let scout = parse_categories(&fixture("gb-addons-categories-7962.json")).unwrap();
        assert!(scout.iter().any(|c| c.id() == 7972));
        assert!(
            guis.iter()
                .chain(&skins)
                .chain(&scout)
                .all(|c| c.id() > 0 && !c.name().is_empty())
        );
    }

    #[test]
    fn html_becomes_plain_lines() {
        assert_eq!(
            html_to_text(
                "<p>Hi &amp; bye</p><br>Line<br/>2 &lt;3&gt; &quot;q&quot; it&#39;s<b>!</b>&nbsp;x"
            ),
            "Hi & bye\n\nLine\n2 <3> \"q\" it's! x"
        );
        assert_eq!(html_to_text("<p>a</p><p></p><p></p><p>b</p>"), "a\n\nb");
    }

    // GameBanana's editor writes headings, lists and rules with no break between them.
    #[test]
    fn block_tags_break_lines() {
        assert_eq!(
            html_to_text("<h1>This Mod Features...</h1><h2>Bot Tweaks</h2><ul><li>a</li></ul>"),
            "This Mod Features...\nBot Tweaks\na"
        );
        assert_eq!(
            html_to_text("(no knowledge)<h3>WANT MORE MODS!</h3><b>JOIN MY DISCORD!</b>"),
            "(no knowledge)\nWANT MORE MODS!\nJOIN MY DISCORD!"
        );
        assert_eq!(html_to_text("a<hr>b<ol><li>c</li></ol>d"), "a\nb\nc\nd");
        assert_eq!(
            html_to_text("<blockquote>q</blockquote>after<table><tr><td>x</td></tr></table>y"),
            "q\nafter\nx\ny"
        );
    }

    // Any uploader controls the text, so block tags after runs of spaces must stay linear.
    #[test]
    fn hostile_block_tags_stay_linear() {
        let html = format!("a{}", " <ul>".repeat(200_000));
        let start = std::time::Instant::now();
        assert_eq!(html_to_text(&html), "a");
        assert!(
            start.elapsed() < std::time::Duration::from_secs(2),
            "{:?}",
            start.elapsed()
        );
    }

    #[test]
    fn table_cells_stay_apart() {
        assert_eq!(
            html_to_text(
                "<table><tr><td>x</td><td>y</td></tr><tr><th>z</th><th>w</th></tr></table>"
            ),
            "x y\nz w"
        );
    }

    #[test]
    fn breaks_collapse_to_one_blank_line() {
        assert_eq!(html_to_text("a<br><br><br><br>b<h2>c</h2>"), "a\n\nb\nc");
        // Newlines in the HTML source are layout, not breaks.
        assert_eq!(
            html_to_text("<ul>\n<li>a</li>\n\n<li>b</li>\n</ul>"),
            "a\nb"
        );
    }

    #[test]
    fn a_real_profile_lists_files_downloads_and_credits() {
        let body = fixture("gb-addons-profile-12592.json");
        let raw: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let parsed = parse_profile(&body).unwrap();
        let profile = &parsed.profile;
        assert!(!parsed.gone);
        assert_eq!(profile.id().get(), 12592);
        assert!(profile.download_count() > 0);
        assert!(
            profile
                .files()
                .iter()
                .any(|f| f.id() == 392820 && f.name() == "toon_muzzle_flashes_v12.rar")
        );
        assert!(
            profile
                .files()
                .windows(2)
                .all(|w| w[0].date_added() >= w[1].date_added())
        );
        let groups = raw["_aCredits"].as_array().map_or(0, Vec::len);
        assert_eq!(profile.credits().len(), groups);
        assert!(!profile.description().contains('<'));
    }

    #[test]
    fn a_rated_profile_is_mature_and_names_its_ratings() {
        let parsed = parse_profile(&fixture("gb-addons-profile-193786.json")).unwrap();
        assert!(parsed.profile.mature());
        assert!(!parsed.profile.ratings().is_empty());
    }
}
