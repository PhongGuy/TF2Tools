//! Synthetic sfnt fonts for tests: four tables (`head`, `hhea`, `maxp`, `name`), no glyphs.

/// One face whose `name` table holds `names` as (name ID, text) Windows Unicode records.
/// `cff` writes the `OTTO` signature instead of TrueType's 1.0.
pub fn sfnt_bytes(names: &[(u16, &str)], cff: bool) -> Vec<u8> {
    let mut out = Vec::new();
    write_face(&mut out, 0, &windows_records(names), cff);
    out
}

/// One TrueType face whose `name` table holds only Macintosh Roman (platform 1, encoding 0)
/// records, the text given as raw Mac Roman bytes.
pub fn sfnt_mac_bytes(names: &[(u16, &[u8])]) -> Vec<u8> {
    let records: Vec<Record> = names
        .iter()
        .map(|(id, bytes)| ([1, 0, 0, *id], bytes.to_vec()))
        .collect();
    let mut out = Vec::new();
    write_face(&mut out, 0, &records, false);
    out
}

/// One TrueType face whose `name` table holds `records` as given: (platform, encoding,
/// language, name ID) and the raw stored bytes, for encodings the other builders do not write.
pub fn sfnt_raw_bytes(records: &[([u16; 4], &[u8])]) -> Vec<u8> {
    let records: Vec<Record> = records
        .iter()
        .map(|(ids, bytes)| (*ids, bytes.to_vec()))
        .collect();
    let mut out = Vec::new();
    write_face(&mut out, 0, &records, false);
    out
}

/// A name record: platform, encoding, language and name ID, then the stored bytes.
type Record = ([u16; 4], Vec<u8>);

fn windows_records(names: &[(u16, &str)]) -> Vec<Record> {
    names
        .iter()
        .map(|(id, text)| {
            let utf16 = text.encode_utf16().flat_map(u16::to_be_bytes).collect();
            ([3, 1, 0x0409, *id], utf16)
        })
        .collect()
}

/// A TrueType collection with one face per entry of `faces`.
pub fn ttc_bytes(faces: &[&[(u16, &str)]]) -> Vec<u8> {
    let header_len = 12 + 4 * faces.len();
    let mut out = Vec::new();
    out.extend_from_slice(b"ttcf");
    out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    out.extend_from_slice(&(faces.len() as u32).to_be_bytes());
    out.resize(header_len, 0);
    for (i, names) in faces.iter().enumerate() {
        let base = out.len();
        out[12 + 4 * i..16 + 4 * i].copy_from_slice(&(base as u32).to_be_bytes());
        write_face(&mut out, base, &windows_records(names), false);
    }
    out
}

// Table offsets are absolute in the file, so a face inside a collection is written at
// `base` with its records pointing past it.
fn write_face(out: &mut Vec<u8>, base: usize, names: &[Record], cff: bool) {
    let mut head = vec![0u8; 54];
    head[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    head[12..16].copy_from_slice(&0x5F0F_3CF5u32.to_be_bytes());
    head[18..20].copy_from_slice(&1000u16.to_be_bytes());
    let mut hhea = vec![0u8; 36];
    hhea[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    hhea[34..36].copy_from_slice(&1u16.to_be_bytes());
    let mut maxp = vec![0u8; 6];
    maxp[0..4].copy_from_slice(&0x0000_5000u32.to_be_bytes());
    maxp[4..6].copy_from_slice(&1u16.to_be_bytes());
    // Already in the ascending tag order the table directory requires.
    let tables: [(&[u8; 4], Vec<u8>); 4] = [
        (b"head", head),
        (b"hhea", hhea),
        (b"maxp", maxp),
        (b"name", name_table(names)),
    ];
    let signature: u32 = if cff { 0x4F54_544F } else { 0x0001_0000 };
    out.extend_from_slice(&signature.to_be_bytes());
    out.extend_from_slice(&4u16.to_be_bytes());
    out.extend_from_slice(&[0, 64, 0, 2, 0, 0]);
    let mut offset = base + 12 + 16 * tables.len();
    for (tag, data) in &tables {
        out.extend_from_slice(*tag);
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(&(offset as u32).to_be_bytes());
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        offset += data.len().next_multiple_of(4);
    }
    for (_, data) in &tables {
        out.extend_from_slice(data);
        out.resize(out.len().next_multiple_of(4), 0);
    }
}

fn name_table(names: &[Record]) -> Vec<u8> {
    let mut storage = Vec::new();
    let mut table = Vec::new();
    table.extend_from_slice(&0u16.to_be_bytes());
    table.extend_from_slice(&(names.len() as u16).to_be_bytes());
    table.extend_from_slice(&((6 + 12 * names.len()) as u16).to_be_bytes());
    for (ids, bytes) in names {
        for field in ids
            .iter()
            .copied()
            .chain([bytes.len() as u16, storage.len() as u16])
        {
            table.extend_from_slice(&field.to_be_bytes());
        }
        storage.extend_from_slice(bytes);
    }
    table.extend_from_slice(&storage);
    table
}
