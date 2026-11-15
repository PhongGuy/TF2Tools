//! The third-party licence and credit notices, compiled into the binary from the repo's `licenses/` folder
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
    ("mastercomfig-MIT.txt", "mastercomfig (MIT)"),
    ("hud-db-MIT.txt", "hud-db HUD list (MIT)"),
    ("GameBanana-notice.txt", "GameBanana"),
    ("serveme-notice.txt", "serveme.tf"),
    ("logstf-notice.txt", "logs.tf"),
    ("Valve-notice.txt", "Valve and Team Fortress 2"),
    ("rars-Apache-2.0.txt", "rars RAR reader (Apache-2.0)"),
    ("crosshairs-notice.txt", "Built-in crosshairs"),
    ("comfig-app-MIT.txt", "mastercomfig app preset text (MIT)"),
    (THIRD_PARTY, "Rust crates TF2Tools is built from"),
];

/// The notices cargo-about generates for every linked crate; thousands of lines, so shown last.
const THIRD_PARTY: &str = "THIRD-PARTY-NOTICES.txt";

/// A notice over this many lines is long: Settings lays out only its visible lines.
const LONG_LINES: usize = 500;

/// How many characters a long notice's row holds; its lines are rewrapped to fit.
pub const WRAP_COLUMNS: usize = 80;

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

    /// Whether the text runs past `LONG_LINES`, so it is shown as a list of rows.
    pub fn is_long(&self) -> bool {
        self.text.lines().nth(LONG_LINES).is_some()
    }

    /// The text as rows of at most `WRAP_COLUMNS` characters.
    pub fn lines(&self) -> Vec<String> {
        wrap_lines(&self.text, WRAP_COLUMNS)
    }
}

/// Splits `text` into rows of at most `columns` characters. A longer line breaks at its spaces,
/// losing its indent and runs of spaces, and a word longer than a row is cut.
pub fn wrap_lines(text: &str, columns: usize) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        if line.chars().count() <= columns {
            out.push(line.to_owned());
            continue;
        }
        let mut row = String::new();
        for mut word in line.split_whitespace() {
            loop {
                let used = row.chars().count();
                if used + usize::from(used > 0) + word.chars().count() <= columns {
                    if used > 0 {
                        row.push(' ');
                    }
                    row.push_str(word);
                    break;
                }
                if used > 0 {
                    out.push(std::mem::take(&mut row));
                    continue;
                }
                let cut = word
                    .char_indices()
                    .nth(columns)
                    .map_or(word.len(), |(i, _)| i);
                out.push(word[..cut].to_owned());
                word = &word[cut..];
                if word.is_empty() {
                    break;
                }
            }
        }
        if !row.is_empty() {
            out.push(row);
        }
    }
    out
}

/// Every embedded notice, ordered by file name with the third-party crate notices last.
pub fn all() -> Vec<Notice> {
    let mut files: Vec<String> = Licences::iter().map(|f| f.into_owned()).collect();
    files.sort_by(|a, b| (a == THIRD_PARTY, a).cmp(&(b == THIRD_PARTY, b)));
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
