//! The main menu's pictures: the background the chapter list names and the class picture the
//! character list names, as image values for `HudFiles::image`.

use crate::error::HudFilesError;
use crate::fonts::Platform;
use crate::fonts::keyvalues::{KvPair, KvValue};

use super::files::HudFiles;
use super::layout::Screen;

const CHAPTERS: &str = "scripts/chapterbackgrounds.txt";
const CHARACTERS: &str = "scripts/characterbackgrounds.txt";

impl HudFiles {
    /// The picture behind the main menu: the first chapter the chapter list names, as the
    /// image value `../console/<name>`, with `_widescreen` added when `widescreen` and that
    /// material exists. `None` when the list is absent or names nothing.
    pub fn menu_background(
        &self,
        widescreen: bool,
        platform: Platform,
    ) -> Result<Option<String>, HudFilesError> {
        let (pairs, _) = self.load_kv(CHAPTERS, platform)?;
        // The game picks a chapter at random; the preview takes the first one listed.
        let chapters = pairs
            .iter()
            .find(|p| p.key.eq_ignore_ascii_case("chapters") && block(p).is_some())
            .or_else(|| pairs.iter().find(|p| block(p).is_some()))
            .and_then(block)
            .unwrap_or_default();
        let Some(name) = chapters.iter().find_map(|p| match &p.value {
            KvValue::Text(t) if !t.trim().is_empty() => Some(t.trim()),
            _ => None,
        }) else {
            return Ok(None);
        };
        let wide = format!("{name}_widescreen");
        let wide_exists = self.contains(&format!("materials/console/{wide}.vmt"));
        let chosen = if widescreen && wide_exists {
            &wide
        } else {
            name
        };
        Ok(Some(format!("../console/{chosen}")))
    }

    /// The class picture the main menu shows outside holidays: the `image` of the first entry
    /// in the character list with a non-empty `image`, no `holiday_restriction` and a `weight`
    /// above 0 (1 when absent). `None` when there is none.
    pub fn menu_character(&self, platform: Platform) -> Result<Option<String>, HudFilesError> {
        let (pairs, _) = self.load_kv(CHARACTERS, platform)?;
        let entries = pairs.iter().find_map(block).unwrap_or_default();
        // With no holiday, war or operation running, only holiday entries and weights under 1
        // are barred; a war restriction does not bar an entry. The game reads the weight as a
        // whole number, so text reads as 0 and a fraction drops its decimals.
        let allowed = entries.iter().filter_map(block).find_map(|keys| {
            let weight = text(keys, "weight").map_or(1, |w| super::expr::atoi(w.as_bytes()));
            let barred = text(keys, "holiday_restriction").is_some() || weight <= 0;
            let image = text(keys, "image").map(str::trim)?;
            (!barred && !image.is_empty()).then(|| image.to_owned())
        });
        Ok(allowed)
    }
}

impl Screen {
    /// Whether the game counts the screen as wide, at least 16:10, as the main menu's
    /// `if_wider` condition does.
    pub fn is_wider(self) -> bool {
        u64::from(self.wide()) * 10 >= u64::from(self.tall()) * 16
    }
}

fn block(pair: &KvPair) -> Option<&[KvPair]> {
    match &pair.value {
        KvValue::Block(children) => Some(children),
        KvValue::Text(_) => None,
    }
}

/// The first text value named `name`, ignoring case.
fn text<'a>(pairs: &'a [KvPair], name: &str) -> Option<&'a str> {
    pairs.iter().find_map(|pair| match &pair.value {
        KvValue::Text(t) if pair.key.eq_ignore_ascii_case(name) => Some(t.as_str()),
        _ => None,
    })
}
