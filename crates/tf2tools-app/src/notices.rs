//! The third-party licence notices, compiled into the binary from the repo's `licenses/` folder
//! so every build carries them whatever the packager ships. Settings shows them.

use rust_embed::RustEmbed;

// `debug-embed` is on for the workspace, so dev builds embed the files too.
#[derive(RustEmbed)]
#[folder = "../../licenses/"]
struct Licences;

/// Plain titles for the notices we know; any other file is titled after its name.
const TITLES: &[(&str, &str)] = &[
    ("Inter-OFL.txt", "Inter font"),
    ("Lucide-ISC.txt", "Lucide icons"),
    ("HUD-option-schemas-MIT.txt", "HUD option schemas (MIT)"),
];

/// One licence notice: its file under `licenses/`, a plain title and the full text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    file: String,
    title: String,
    text: String,
}

impl Notice {
    /// The file name under `licenses/`, such as `Inter-OFL.txt`.
    pub fn file(&self) -> &str {
        &self.file
    }

    /// What the notice covers, in player words.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// The licence text, word for word.
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// Every embedded notice, ordered by file name.
pub fn all() -> Vec<Notice> {
    let mut files: Vec<String> = Licences::iter().map(|f| f.into_owned()).collect();
    files.sort();
    files
        .into_iter()
        .filter_map(|file| {
            let bytes = Licences::get(&file)?.data;
            Some(Notice {
                title: title_for(&file),
                text: String::from_utf8_lossy(&bytes).into_owned(),
                file,
            })
        })
        .collect()
}

/// The plain title for a notice file: a known one's title, else the file stem with dashes as spaces.
pub fn title_for(file: &str) -> String {
    if let Some((_, title)) = TITLES.iter().find(|(known, _)| *known == file) {
        return (*title).to_owned();
    }
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    stem.replace('-', " ")
}
