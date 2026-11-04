//! GameBanana's API v11: index and file-listing URLs and the parsers ported from v5's
//! `services/gamebanana.rs`.

use serde_json::Value;

use super::StoreKind;

/// TF2's GameBanana game id.
const TF2_GAME_ID: u32 = 297;
pub(crate) const PAGE_SIZE: u32 = 15;
const CROSSHAIR_CATEGORY: u32 = 1105;
const HUD_CATEGORY: u32 = 1649;
const HITSOUND_CATEGORY: i64 = 381;
const KILLSOUND_CATEGORY: i64 = 2630;

/// One `store_sounds` row.
#[derive(Debug)]
pub(crate) struct SoundRecord {
    pub(crate) gb_id: i64,
    pub(crate) name: String,
    pub(crate) author: String,
    pub(crate) category: &'static str,
    pub(crate) preview_url: Option<String>,
    pub(crate) thumbnail_url: Option<String>,
    pub(crate) like_count: i64,
    pub(crate) view_count: i64,
    pub(crate) tags_json: String,
    pub(crate) profile_url: String,
    pub(crate) date_added: i64,
}

/// One `store_crosshairs` or `store_huds_gb` row; the two tables share their columns.
#[derive(Debug)]
pub(crate) struct ModRecord {
    pub(crate) gb_id: i64,
    pub(crate) name: String,
    pub(crate) author: String,
    pub(crate) thumbnail_url: Option<String>,
    pub(crate) images_json: String,
    pub(crate) like_count: i64,
    pub(crate) view_count: i64,
    pub(crate) tags_json: String,
    pub(crate) profile_url: String,
    pub(crate) date_added: i64,
}

#[derive(Debug)]
pub(crate) enum Records {
    Sounds(Vec<SoundRecord>),
    Mods(Vec<ModRecord>),
}

impl Records {
    pub(crate) fn len(&self) -> usize {
        match self {
            Records::Sounds(rows) => rows.len(),
            Records::Mods(rows) => rows.len(),
        }
    }
}

/// One parsed index page.
#[derive(Debug)]
pub(crate) struct IndexPage {
    pub(crate) records: Records,
    /// `_nRecordCount`: the catalogue size GameBanana reports.
    pub(crate) total: u32,
    /// True unless `_bIsComplete` says this was the last page.
    pub(crate) more: bool,
}

/// The index URL for `page` of a GameBanana store: sounds newest-first across every
/// category, mods most-liked-first filtered on the crosshair or HUD category. None for a
/// non-GameBanana kind.
pub(crate) fn index_url(base: &str, kind: StoreKind, page: u32) -> Option<String> {
    let model = model(kind)?;
    let category = match kind {
        StoreKind::Crosshairs => Some(CROSSHAIR_CATEGORY),
        StoreKind::GameBananaHuds => Some(HUD_CATEGORY),
        _ => None,
    };
    let sort = match kind {
        StoreKind::Sounds => "Generic_Newest",
        _ => "Generic_MostLiked",
    };
    let mut url = format!(
        "{base}/{model}/Index?_nPage={page}&_nPerpage={PAGE_SIZE}\
         &_aFilters[Generic_Game]={TF2_GAME_ID}&_sSort={sort}"
    );
    if let Some(category) = category {
        url.push_str(&format!("&_aFilters[Generic_Category]={category}"));
    }
    Some(url)
}

/// The API model a GameBanana store's items live under: sounds are `Sound`, HUDs and
/// crosshairs are `Mod`. None for a non-GameBanana kind.
fn model(kind: StoreKind) -> Option<&'static str> {
    match kind {
        StoreKind::Sounds => Some("Sound"),
        StoreKind::Crosshairs | StoreKind::GameBananaHuds => Some("Mod"),
        StoreKind::CuratedHuds | StoreKind::Addons => None,
    }
}

/// The URL listing item `id`'s files. None for a non-GameBanana kind.
pub(crate) fn files_url(base: &str, kind: StoreKind, id: u64) -> Option<String> {
    let model = model(kind)?;
    Some(format!(
        "{base}/{model}/{id}?_csvProperties=_idRow,_sName,_aFiles"
    ))
}

/// One downloadable file from an item's `_aFiles`.
#[derive(Debug)]
pub(crate) struct ItemFile {
    pub(crate) file_name: String,
    pub(crate) size: Option<u64>,
    pub(crate) md5: Option<String>,
    pub(crate) url: String,
}

/// Parses an item's file listing, skipping files without a download address; an error
/// only when the body is not JSON.
pub(crate) fn parse_files(body: &[u8]) -> Result<Vec<ItemFile>, serde_json::Error> {
    let json: Value = serde_json::from_slice(body)?;
    let files = json["_aFiles"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    Ok(files
        .iter()
        .filter_map(|file| {
            let url = text(&file["_sDownloadUrl"]);
            if url.is_empty() {
                return None;
            }
            let md5 = text(&file["_sMd5Checksum"]);
            Some(ItemFile {
                file_name: text(&file["_sFile"]),
                size: file["_nFilesize"].as_u64(),
                md5: (!md5.is_empty()).then_some(md5),
                url,
            })
        })
        .collect())
}

/// Parses an index page body for `kind`; an error only when the body is not JSON.
pub(crate) fn parse_index(kind: StoreKind, body: &[u8]) -> Result<IndexPage, serde_json::Error> {
    let json: Value = serde_json::from_slice(body)?;
    let metadata = &json["_aMetadata"];
    let records = json["_aRecords"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let records = if kind == StoreKind::Sounds {
        Records::Sounds(records.iter().filter_map(parse_sound).collect())
    } else {
        Records::Mods(records.iter().filter_map(parse_mod).collect())
    };
    Ok(IndexPage {
        records,
        total: u32::try_from(int(&metadata["_nRecordCount"])).unwrap_or(0),
        more: !metadata["_bIsComplete"].as_bool().unwrap_or(true),
    })
}

fn parse_sound(record: &Value) -> Option<SoundRecord> {
    // Index pages can mix models; only Sound records belong in the sound store.
    let model = record["_sModelName"].as_str().unwrap_or("");
    if !model.is_empty() && model != "Sound" {
        return None;
    }
    let gb_id = int(&record["_idRow"]);
    if gb_id == 0 {
        return None;
    }
    Some(SoundRecord {
        gb_id,
        name: text(&record["_sName"]),
        author: text(&record["_aSubmitter"]["_sName"]),
        category: sound_category(record),
        preview_url: optional_text(&record["_aPreviewMedia"]["_aMetadata"]["_sAudioUrl"]),
        thumbnail_url: optional_text(&record["_aSubmitter"]["_sAvatarUrl"]),
        like_count: int(&record["_nLikeCount"]),
        view_count: int(&record["_nViewCount"]),
        tags_json: tags_json(&record["_aTags"]),
        profile_url: text(&record["_sProfileUrl"]),
        date_added: int(&record["_tsDateAdded"]),
    })
}

/// v5's label: the root category name decides first, then the hit/killsound category ids.
fn sound_category(record: &Value) -> &'static str {
    let root = record["_aRootCategory"]["_sName"]
        .as_str()
        .unwrap_or("")
        .to_lowercase();
    if root.contains("kill") {
        "killsound"
    } else if root.contains("hit") {
        "hitsound"
    } else {
        match int(&record["_aCategory"]["_idRow"]) {
            HITSOUND_CATEGORY => "hitsound",
            KILLSOUND_CATEGORY => "killsound",
            _ => "sound",
        }
    }
}

fn parse_mod(record: &Value) -> Option<ModRecord> {
    let gb_id = int(&record["_idRow"]);
    if gb_id == 0 {
        return None;
    }
    // The gallery prefers the 530 px rendition, then the original (most screenshots
    // carry only that and a 100 px one), then 220 px; the card thumbnail is the first 100 px.
    let mut images = Vec::new();
    let mut thumbnail_url = None;
    for image in record["_aPreviewMedia"]["_aImages"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        let base = image["_sBaseUrl"].as_str().unwrap_or("");
        if let Some(file) = image["_sFile530"]
            .as_str()
            .or_else(|| image["_sFile"].as_str())
            .or_else(|| image["_sFile220"].as_str())
        {
            images.push(format!("{base}/{file}"));
        }
        if thumbnail_url.is_none() {
            thumbnail_url = image["_sFile100"]
                .as_str()
                .map(|file| format!("{base}/{file}"));
        }
    }
    Some(ModRecord {
        gb_id,
        name: text(&record["_sName"]),
        author: text(&record["_aSubmitter"]["_sName"]),
        thumbnail_url,
        images_json: json_list(&images),
        like_count: int(&record["_nLikeCount"]),
        view_count: int(&record["_nViewCount"]),
        tags_json: tags_json(&record["_aTags"]),
        profile_url: text(&record["_sProfileUrl"]),
        date_added: int(&record["_tsDateAdded"]),
    })
}

fn int(value: &Value) -> i64 {
    value.as_i64().unwrap_or(0)
}

fn text(value: &Value) -> String {
    value.as_str().unwrap_or("").to_owned()
}

fn optional_text(value: &Value) -> Option<String> {
    value.as_str().map(str::to_owned)
}

/// The string entries of a tag array as a JSON list; anything else is skipped.
fn tags_json(value: &Value) -> String {
    let tags: Vec<String> = value
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(optional_text)
        .collect();
    json_list(&tags)
}

fn json_list(items: &[String]) -> String {
    serde_json::to_string(items).unwrap_or_else(|_| "[]".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mod_images_fall_back_to_the_original_file_when_no_530_rendition_exists() {
        // GameBanana gives most screenshots only `_sFile` and `_sFile100`; mixed with a full entry.
        let record = serde_json::json!({
            "_idRow": 456577,
            "_aPreviewMedia": { "_aImages": [
                {"_sBaseUrl": "https://img/mods", "_sFile": "a.jpg", "_sFile530": "530-a.jpg",
                 "_sFile220": "220-a.jpg", "_sFile100": "100-a.jpg"},
                {"_sBaseUrl": "https://img/mods", "_sFile": "b.jpg", "_sFile100": "100-b.jpg"},
                {"_sBaseUrl": "https://img/mods", "_sFile220": "220-c.jpg", "_sFile100": "100-c.jpg"},
                {"_sBaseUrl": "https://img/mods", "_sFile": "d.jpg", "_sFile100": "100-d.jpg"}
            ]}
        });
        let parsed = parse_mod(&record).expect("a record with an id parses");
        assert_eq!(
            parsed.images_json,
            r#"["https://img/mods/530-a.jpg","https://img/mods/b.jpg","https://img/mods/220-c.jpg","https://img/mods/d.jpg"]"#
        );
        assert_eq!(
            parsed.thumbnail_url.as_deref(),
            Some("https://img/mods/100-a.jpg")
        );
    }
}
