//! Font fixtures for the HUD font check: synthetic sfnt files, a HUD that declares and ships
//! them, and a `FontHost` that never touches the machine.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tf2tools_core::{FontHost, FontHostError};

/// The fixture HUD's fonts as (family, HUD-relative file): two ordinary fonts, then Marlett,
/// a protected system name the check lists but never installs.
pub const FONTS: &[(&str, &str)] = &[
    ("Surface", "resource/fonts/surface.ttf"),
    ("Code Pro", "resource/fonts/codepro.ttf"),
    ("Marlett", "resource/fonts/marlett.ttf"),
];

/// Every file of a HUD called `name` that declares and ships `fonts`, as (relative path, bytes).
pub fn font_hud_files(name: &str, fonts: &[(&str, &str)]) -> Vec<(String, Vec<u8>)> {
    let mut names = String::new();
    let mut files = String::new();
    for (ix, (family, path)) in fonts.iter().enumerate() {
        names.push_str(&format!(
            "\"F{ix}\" {{ \"1\" {{ \"name\" \"{family}\" }} }}\n"
        ));
        files.push_str(&format!("\"{}\" \"{path}\"\n", ix + 1));
    }
    let scheme = format!(
        "\"Scheme\"\n{{\n\"Fonts\"\n{{\n{names}}}\n\"CustomFontFiles\"\n{{\n{files}}}\n}}\n"
    );
    let info = format!("\"{name}\"\n{{\n\t\"ui_version\"\t\"3\"\n}}\n");
    let mut out = vec![
        ("info.vdf".to_owned(), info.into_bytes()),
        (
            "resource/ui/hudlayout.res".to_owned(),
            b"\"Resource/HudLayout.res\"\n{\n}\n".to_vec(),
        ),
        ("resource/clientscheme.res".to_owned(), scheme.into_bytes()),
    ];
    for (family, path) in fonts {
        out.push(((*path).to_owned(), sfnt_bytes(&[(1, family), (4, family)])));
    }
    out
}

/// Writes the HUD of `font_hud_files` into `dir`, replacing files already there.
pub fn write_font_hud(dir: &Path, name: &str, fonts: &[(&str, &str)]) {
    for (rel, bytes) in font_hud_files(name, fonts) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
}

/// The HUD of `font_hud_files` as a zip archive with everything under `<name>/`.
pub fn font_hud_zip_bytes(name: &str, fonts: &[(&str, &str)]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    for (rel, bytes) in font_hud_files(name, fonts) {
        zip.start_file(format!("{name}/{rel}"), options).unwrap();
        zip.write_all(&bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

/// One TrueType face with no glyphs whose `name` table holds `names` as (name ID, text)
/// Windows Unicode records; core's `ttf-parser` reads it.
pub fn sfnt_bytes(names: &[(u16, &str)]) -> Vec<u8> {
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
    let mut out = Vec::new();
    out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    out.extend_from_slice(&4u16.to_be_bytes());
    out.extend_from_slice(&[0, 64, 0, 2, 0, 0]);
    let mut offset = 12 + 16 * tables.len();
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
    out
}

fn name_table(names: &[(u16, &str)]) -> Vec<u8> {
    let mut storage: Vec<u8> = Vec::new();
    let mut table = Vec::new();
    table.extend_from_slice(&0u16.to_be_bytes());
    table.extend_from_slice(&(names.len() as u16).to_be_bytes());
    table.extend_from_slice(&((6 + 12 * names.len()) as u16).to_be_bytes());
    for (id, text) in names {
        let bytes: Vec<u8> = text.encode_utf16().flat_map(u16::to_be_bytes).collect();
        for field in [3, 1, 0x0409, *id, bytes.len() as u16, storage.len() as u16] {
            table.extend_from_slice(&field.to_be_bytes());
        }
        storage.extend_from_slice(&bytes);
    }
    table.extend_from_slice(&storage);
    table
}

/// Installed names come from `names` and `fonts_dir` is `dir`; register and refresh are
/// counted, and listing or refreshing can be made to fail.
#[derive(Default)]
pub struct FakeFontHost {
    pub names: Vec<String>,
    pub dir: PathBuf,
    pub fail_names: bool,
    pub fail_refresh: bool,
    pub registered: Mutex<Vec<String>>,
    pub refreshes: Mutex<usize>,
}

impl FakeFontHost {
    pub fn new(names: &[&str], dir: &Path) -> Self {
        Self {
            names: names.iter().map(|n| (*n).to_owned()).collect(),
            dir: dir.to_path_buf(),
            ..Self::default()
        }
    }
}

impl FontHost for FakeFontHost {
    fn installed_names(&self) -> Result<Vec<String>, FontHostError> {
        if self.fail_names {
            return Err(FontHostError::new("fc-list is missing"));
        }
        Ok(self.names.clone())
    }

    fn fonts_dir(&self) -> Result<PathBuf, FontHostError> {
        Ok(self.dir.clone())
    }

    fn register(&self, _: &Path, full_name: &str, _: bool) -> Result<(), FontHostError> {
        self.registered.lock().unwrap().push(full_name.to_owned());
        Ok(())
    }

    fn refresh(&self) -> Result<(), FontHostError> {
        if self.fail_refresh {
            return Err(FontHostError::new("fc-cache failed"));
        }
        *self.refreshes.lock().unwrap() += 1;
        Ok(())
    }
}
