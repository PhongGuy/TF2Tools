//! GameBanana Mod Index URLs and the listing parser behind the Addon Store.
#![cfg_attr(not(test), expect(dead_code, reason = "the catalog arrives in Task 3"))]

use serde_json::Value;

use super::addon_catalog::{AddonCard, AddonSort};
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
}
