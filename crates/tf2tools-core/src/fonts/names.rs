//! Font names: the family, typographic family and full names a font file declares.

use std::io::{Error, ErrorKind};
use std::path::Path;

/// OS font families that are never installed; a HUD file carrying one is skipped whole.
pub const PROTECTED_FONT_NAMES: &[&str] = &[
    "Marlett",
    "Arial",
    "Verdana",
    "Tahoma",
    "Courier New",
    "Times New Roman",
    "Lucida Console",
    "Segoe UI",
    "Symbol",
    "Webdings",
    "Wingdings",
    "MS Sans Serif",
    "Trebuchet MS",
    "Georgia",
    "Impact",
];

/// Font files come from downloaded HUDs, so reads and collection walks are bounded: the cap
/// sits well above the largest real CJK collections, and no real TTC holds 256 faces.
pub(crate) const MAX_FONT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_FACES: u32 = 256;

/// Family (1), full name (4) and typographic family (16).
const NAME_IDS: [u16; 3] = [1, 4, 16];

/// True when any of `names` equals a protected family once case, whitespace and control
/// characters are dropped; fontconfig matches families ignoring case and blanks, so a padded
/// or NUL-terminated "Marlett" would still take its place.
pub(crate) fn is_protected(names: &[String]) -> bool {
    let fold = |name: &str| -> String {
        name.chars()
            .filter(|c| !c.is_whitespace() && !c.is_control())
            .flat_map(char::to_lowercase)
            .collect()
    };
    names.iter().any(|name| {
        let name = fold(name);
        PROTECTED_FONT_NAMES.iter().any(|p| fold(p) == name)
    })
}

/// Mac Roman bytes 0x80 to 0xFF; the lower half is ASCII.
const MAC_ROMAN_HIGH: [char; 128] = [
    'Ä', 'Å', 'Ç', 'É', 'Ñ', 'Ö', 'Ü', 'á', 'à', 'â', 'ä', 'ã', 'å', 'ç', 'é', 'è', //
    'ê', 'ë', 'í', 'ì', 'î', 'ï', 'ñ', 'ó', 'ò', 'ô', 'ö', 'õ', 'ú', 'ù', 'û', 'ü', //
    '†', '°', '¢', '£', '§', '•', '¶', 'ß', '®', '©', '™', '´', '¨', '≠', 'Æ', 'Ø', //
    '∞', '±', '≤', '≥', '¥', 'µ', '∂', '∑', '∏', 'π', '∫', 'ª', 'º', 'Ω', 'æ', 'ø', //
    '¿', '¡', '¬', '√', 'ƒ', '≈', '∆', '«', '»', '…', '\u{a0}', 'À', 'Ã', 'Õ', 'Œ', 'œ', //
    '–', '—', '“', '”', '‘', '’', '÷', '◊', 'ÿ', 'Ÿ', '⁄', '€', '‹', '›', 'ﬁ', 'ﬂ', //
    '‡', '·', '‚', '„', '‰', 'Â', 'Ê', 'Á', 'Ë', 'È', 'Í', 'Î', 'Ï', 'Ì', 'Ó', 'Ô', //
    '\u{f8ff}', 'Ò', 'Ú', 'Û', 'Ù', 'ı', 'ˆ', '˜', '¯', '˘', '˙', '˚', '¸', '˝', '˛', 'ˇ', //
];

/// A record's text: Unicode records through ttf-parser, Macintosh Roman (platform 1,
/// encoding 0) through the table above, since fontconfig reads those names too.
fn record_text(record: &ttf_parser::name::Name) -> Option<String> {
    if record.platform_id == ttf_parser::PlatformId::Macintosh && record.encoding_id == 0 {
        let decode = |b: u8| match b {
            0..=0x7F => char::from(b),
            _ => MAC_ROMAN_HIGH[usize::from(b - 0x80)],
        };
        return Some(record.name.iter().copied().map(decode).collect());
    }
    record.to_string()
}

/// The names a font file declares across all its faces.
#[derive(Debug)]
pub(crate) struct FontNames {
    /// IDs 1, 16 and 4 of every face in record order, deduplicated case-insensitively.
    pub(crate) names: Vec<String>,
    /// The first full name (ID 4) of face 0.
    pub(crate) full_name: Option<String>,
    /// The file has CFF outlines (`OTTO` signature).
    pub(crate) open_type: bool,
    /// Some ID 1, 16 or 4 record is in an encoding not decoded here (ShiftJIS, ISO, Mac
    /// non-Roman), so the system may read a name `names` lacks.
    pub(crate) undecoded: bool,
}

/// Reads the `name` table of every face (`ttf_parser::fonts_in_collection`, else one face). An
/// unreadable file is its I/O error; bytes no face's `name` table parses from, or a file over
/// the size cap, are `InvalidData`.
pub(crate) fn font_names(path: &Path) -> std::io::Result<FontNames> {
    names_in(&read_capped(path)?)
}

/// [`font_names`] over bytes already read, so a caller can check and copy the same bytes.
pub(crate) fn names_in(data: &[u8]) -> std::io::Result<FontNames> {
    let faces = face_count(data);
    let mut names: Vec<String> = Vec::new();
    // Folded spellings already kept, so a hostile table of many distinct names stays linear.
    let mut seen = std::collections::HashSet::new();
    let mut full_name = None;
    let mut undecoded = false;
    let mut parsed_any = false;
    for index in 0..faces {
        let Some(table) = name_table(data, index) else {
            continue;
        };
        parsed_any = true;
        // Each record is fetched by index, so one whose text lies outside the table is skipped
        // rather than ending the walk the way the table's iterator does.
        let records = (0..table.names.len()).filter_map(|i| table.names.get(i));
        for record in records {
            if !NAME_IDS.contains(&record.name_id) {
                continue;
            }
            let Some(text) = record_text(&record) else {
                undecoded = true;
                continue;
            };
            if text.is_empty() {
                continue;
            }
            if index == 0 && record.name_id == 4 && full_name.is_none() {
                full_name = Some(text.clone());
            }
            if seen.insert(text.to_lowercase()) {
                names.push(text);
            }
        }
    }
    if !parsed_any {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "no font face with a name table parses",
        ));
    }
    Ok(FontNames {
        names,
        full_name,
        open_type: data.starts_with(b"OTTO"),
        undecoded,
    })
}

/// The faces to walk, bounded: a collection's face count is untrusted, and ttf-parser asserts
/// on one whose offsets run past 4 GiB, so a count the file cannot hold offsets for is 0.
fn face_count(data: &[u8]) -> u32 {
    match ttf_parser::fonts_in_collection(data) {
        Some(count) if count as usize > data.len().saturating_sub(12) / 4 => 0,
        Some(count) => count.min(MAX_FACES),
        None => 1,
    }
}

/// Face `index`'s `name` table. Only the table directory and `name` are read, so the other
/// tables' parsers never see untrusted bytes.
fn name_table(data: &[u8], index: u32) -> Option<ttf_parser::name::Table<'_>> {
    ttf_parser::RawFace::parse(data, index)
        .ok()
        .and_then(|raw| raw.table(ttf_parser::Tag::from_bytes(b"name")))
        .and_then(ttf_parser::name::Table::parse)
}

/// The primary family of the first face: its first decodable, non-empty Windows-platform
/// family (name ID 1), since Windows resolves a scheme name through those, else its first
/// family on any platform; `None` for bytes over the size cap or with no such record.
pub(crate) fn family_in(data: &[u8]) -> Option<String> {
    if !readable(data) {
        return None;
    }
    let table = name_table(data, 0)?;
    let families = || {
        (0..table.names.len())
            .filter_map(|i| table.names.get(i))
            .filter(|record| record.name_id == 1)
            .filter_map(|record| Some((record.platform_id, record_text(&record)?)))
            .filter(|(_, text)| !text.is_empty())
    };
    (families().find(|(platform, _)| *platform == ttf_parser::PlatformId::Windows))
        .or_else(|| families().next())
        .map(|(_, text)| text)
}

/// Every family a font file answers to: the family (1) and typographic family (16) records
/// of every face, on every platform and language, deduplicated case-insensitively, with
/// [`family_in`]'s primary family first; empty for bytes over the size cap or with none.
pub(crate) fn families_in(data: &[u8]) -> Vec<String> {
    if !readable(data) {
        return Vec::new();
    }
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    let tables = (0..face_count(data)).filter_map(|index| name_table(data, index));
    let records = tables.flat_map(|table| {
        (0..table.names.len())
            .filter_map(move |i| table.names.get(i))
            .filter(|record| matches!(record.name_id, 1 | 16))
            .filter_map(|record| record_text(&record))
    });
    for text in family_in(data).into_iter().chain(records) {
        if !text.is_empty() && seen.insert(text.to_lowercase()) {
            out.push(text);
        }
    }
    out
}

/// Within the size cap and holding a face count the bytes can back.
fn readable(data: &[u8]) -> bool {
    data.len() as u64 <= MAX_FONT_BYTES && face_count(data) > 0
}

/// Reads the whole font file, an `InvalidData` error when it is not a regular file or is over
/// `MAX_FONT_BYTES`.
pub(crate) fn read_capped(path: &Path) -> std::io::Result<Vec<u8>> {
    super::read_regular_capped(path, MAX_FONT_BYTES)?.ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "not a regular font file within the size cap",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::test_sfnt::{sfnt_bytes, sfnt_mac_bytes, sfnt_raw_bytes, ttc_bytes};

    fn write(dir: &Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn flags_a_name_record_in_an_undecoded_encoding() {
        let foo: Vec<u8> = "Foo".encode_utf16().flat_map(u16::to_be_bytes).collect();
        let readable = names_in(&sfnt_raw_bytes(&[([3, 1, 0x0409, 1], &foo)])).unwrap();
        assert!(!readable.undecoded);
        let mixed = names_in(&sfnt_raw_bytes(&[
            ([3, 1, 0x0409, 1], &foo),
            ([3, 2, 0x0411, 1], b"Marlett"),
            ([2, 0, 0, 2], b"Regular"),
        ]))
        .unwrap();
        assert_eq!(mixed.names, ["Foo"]);
        assert!(mixed.undecoded);
        // ISO platform text is never decoded, but name ID 2 is never read either.
        let style_only = names_in(&sfnt_raw_bytes(&[([2, 0, 0, 2], b"Regular")])).unwrap();
        assert!(style_only.names.is_empty());
        assert!(
            !style_only.undecoded,
            "an unread name ID cannot hide a name"
        );
    }

    #[test]
    fn reads_family_typographic_family_and_full_name() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write(
            tmp.path(),
            "surface.ttf",
            &sfnt_bytes(
                &[
                    (1, "Surface"),
                    (2, "Regular"),
                    (16, "Surface Pro"),
                    (4, "Surface SemiBold"),
                ],
                false,
            ),
        );
        let names = font_names(&path).unwrap();
        assert_eq!(names.names, ["Surface", "Surface Pro", "Surface SemiBold"]);
        assert_eq!(names.full_name.as_deref(), Some("Surface SemiBold"));
        assert!(!names.open_type);
    }

    #[test]
    fn duplicate_names_collapse_case_insensitively() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write(
            tmp.path(),
            "lato.ttf",
            &sfnt_bytes(&[(1, "Lato"), (4, "lato")], false),
        );
        let names = font_names(&path).unwrap();
        assert_eq!(names.names, ["Lato"]);
        assert_eq!(names.full_name.as_deref(), Some("lato"));
    }

    #[test]
    fn otto_files_are_open_type() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write(
            tmp.path(),
            "code.otf",
            &sfnt_bytes(&[(1, "Code Pro"), (4, "code-pro-bold")], true),
        );
        let names = font_names(&path).unwrap();
        assert!(names.open_type);
        assert_eq!(names.names, ["Code Pro", "code-pro-bold"]);
    }

    #[test]
    fn every_face_of_a_collection_counts() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write(
            tmp.path(),
            "pair.ttc",
            &ttc_bytes(&[&[(1, "A Fam"), (4, "A Fam Bold")], &[(1, "B Fam")]]),
        );
        let names = font_names(&path).unwrap();
        assert_eq!(names.names, ["A Fam", "A Fam Bold", "B Fam"]);
        assert_eq!(names.full_name.as_deref(), Some("A Fam Bold"));
    }

    #[test]
    fn bytes_that_are_no_font_are_invalid_data() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write(tmp.path(), "broken.ttf", b"not a font");
        assert_eq!(
            font_names(&path).unwrap_err().kind(),
            std::io::ErrorKind::InvalidData
        );
        let missing = tmp.path().join("gone.ttf");
        assert_eq!(
            font_names(&missing).unwrap_err().kind(),
            std::io::ErrorKind::NotFound
        );
    }

    #[test]
    fn protected_names_are_the_os_families() {
        assert_eq!(PROTECTED_FONT_NAMES.len(), 15);
        assert!(PROTECTED_FONT_NAMES.contains(&"Marlett"));
        assert!(is_protected(&["Lato".into(), "marlett".into()]));
        assert!(!is_protected(&["TF2 Build".into(), "Arial Black".into()]));
    }

    #[test]
    fn truncated_and_garbage_bytes_never_panic() {
        let tmp = tempfile::tempdir().unwrap();
        let font = sfnt_bytes(&[(1, "Surface"), (4, "Surface Bold")], false);
        let ttc = ttc_bytes(&[&[(1, "A Fam")], &[(1, "B Fam")]]);
        let mut noise = 0x2545_F491u32;
        let garbage: Vec<u8> = (0..4096)
            .map(|_| {
                noise ^= noise << 13;
                noise ^= noise >> 17;
                noise ^= noise << 5;
                noise as u8
            })
            .collect();
        for bytes in [&font, &ttc, &garbage] {
            for len in 0..bytes.len().min(1024) {
                let path = write(tmp.path(), "cut.ttf", &bytes[..len]);
                if let Err(e) = font_names(&path) {
                    assert_eq!(e.kind(), std::io::ErrorKind::InvalidData, "length {len}");
                }
            }
        }
    }

    #[test]
    fn a_collection_claiming_billions_of_faces_is_bounded() {
        let tmp = tempfile::tempdir().unwrap();
        let mut bytes = ttc_bytes(&[&[(1, "A Fam")]]);
        bytes[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
        let path = write(tmp.path(), "huge.ttc", &bytes);
        let started = std::time::Instant::now();
        let err = font_names(&path).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }

    #[test]
    fn a_file_over_the_size_cap_is_not_read() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write(tmp.path(), "big.ttf", &sfnt_bytes(&[(1, "Big")], false));
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(MAX_FONT_BYTES + 1)
            .unwrap();
        assert_eq!(
            font_names(&path).unwrap_err().kind(),
            std::io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn a_record_outside_the_table_hides_no_later_names() {
        let tmp = tempfile::tempdir().unwrap();
        let mut bytes = sfnt_bytes(&[(1, "Lost"), (1, "Kept"), (4, "Kept Bold")], false);
        // The `name` entry is the fourth in the table directory; its offset follows the tag
        // and checksum. Record 0's length sits 6 + 8 bytes into the table.
        let entry = 12 + 16 * 3;
        let offset = u32::from_be_bytes(bytes[entry + 8..entry + 12].try_into().unwrap());
        let length = offset as usize + 6 + 8;
        bytes[length..length + 2].copy_from_slice(&0xFFFFu16.to_be_bytes());
        let path = write(tmp.path(), "bad.ttf", &bytes);
        let names = font_names(&path).unwrap();
        assert_eq!(names.names, ["Kept", "Kept Bold"]);
        assert_eq!(names.full_name.as_deref(), Some("Kept Bold"));
    }

    #[test]
    fn family_in_reads_the_first_faces_family() {
        let font = sfnt_bytes(&[(4, "Big Bold"), (1, ""), (1, "Big"), (1, "Other")], false);
        assert_eq!(family_in(&font).as_deref(), Some("Big"));
        let ttc = ttc_bytes(&[&[(4, "Only Full")], &[(1, "B Fam")]]);
        assert_eq!(family_in(&ttc), None, "the first face has no family");
        let mut huge = ttc_bytes(&[&[(1, "A Fam")]]);
        huge[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
        assert_eq!(family_in(&huge), None);
        for len in 0..font.len() {
            let _ = family_in(&font[..len]);
        }
    }

    /// tf.ttf's shape: a Unicode-platform family, then Mac Roman and Windows ones naming
    /// another family, each with a full name beside it.
    fn two_family_font() -> Vec<u8> {
        let utf16 = |t: &str| -> Vec<u8> { t.encode_utf16().flat_map(u16::to_be_bytes).collect() };
        let (cs, tf) = (utf16("Counter-Strike"), utf16("Team Fortress"));
        sfnt_raw_bytes(&[
            ([0, 0, 0, 1], &cs),
            ([0, 0, 0, 4], &utf16("Counter-Strike Full")),
            ([1, 0, 0, 1], b"Team Fortress"),
            ([3, 1, 0x0409, 1], &tf),
            ([3, 1, 0x0409, 16], &utf16("Team Fortress Pro")),
            ([3, 1, 0x0407, 1], &utf16("team fortress")),
        ])
    }

    #[test]
    fn families_in_reads_every_family_with_the_windows_one_first() {
        assert_eq!(
            families_in(&two_family_font()),
            ["Team Fortress", "Counter-Strike", "Team Fortress Pro"]
        );
        assert_eq!(
            family_in(&two_family_font()).as_deref(),
            Some("Team Fortress")
        );
        let ttc = ttc_bytes(&[&[(1, "A Fam")], &[(1, "B Fam"), (16, "B Group")]]);
        assert_eq!(families_in(&ttc), ["A Fam", "B Fam", "B Group"]);
        assert!(families_in(b"not a font").is_empty());
        let font = two_family_font();
        for len in 0..font.len() {
            let _ = families_in(&font[..len]);
        }
    }

    #[test]
    fn empty_names_are_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write(
            tmp.path(),
            "empty.ttf",
            &sfnt_bytes(&[(4, ""), (1, "Real"), (4, "Real Bold")], false),
        );
        let names = font_names(&path).unwrap();
        assert_eq!(names.names, ["Real", "Real Bold"]);
        assert_eq!(names.full_name.as_deref(), Some("Real Bold"));
    }

    #[test]
    fn mac_roman_names_are_read() {
        let tmp = tempfile::tempdir().unwrap();
        // 0x8E is e-acute in Mac Roman; name ID 2 is not read.
        let path = write(
            tmp.path(),
            "mac.ttf",
            &sfnt_mac_bytes(&[(1, b"Marlett"), (2, b"Regular"), (4, b"Caf\x8E Bold")]),
        );
        let names = font_names(&path).unwrap();
        assert_eq!(names.names, ["Marlett", "Caf\u{e9} Bold"]);
        assert_eq!(names.full_name.as_deref(), Some("Caf\u{e9} Bold"));
        assert!(is_protected(&names.names));
    }

    #[test]
    fn protection_ignores_case_blanks_and_nuls() {
        for name in [
            "Marlett ",
            "Marlett\0",
            " m a r l e t t",
            "TimesNewRoman",
            "times\tnew roman",
            "COURIERNEW",
        ] {
            assert!(is_protected(&[name.to_owned()]), "{name:?}");
        }
        assert!(!is_protected(&["Arial Black".into(), "Marlett Pro".into()]));
    }
}
