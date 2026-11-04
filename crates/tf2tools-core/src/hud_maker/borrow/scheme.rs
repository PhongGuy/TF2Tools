//! The scheme names a borrowed part uses: colours, fonts, borders and the font files behind
//! the fonts. A name the player's scheme lacks is copied, an identical one reused and a
//! different one renamed so the player's own HUD keeps its look.

use std::collections::HashSet;

use super::{BorrowRefused, Brings, Change, LeftBehind, Run};
use crate::fonts::keyvalues::{KvPair, KvValue};
use crate::hud_maker::model::SCHEME_FILE;
use crate::hud_maker::{
    AddBlock, BlockLine, ChangeTarget, EditRefused, Rgba, SetKey, check_hud_path, font_family,
};

/// The highest suffix a renamed name may take.
const LAST_SUFFIX: u32 = 99;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Kind {
    Colour,
    Font,
    Border,
}

/// What a value of `key` can name, going by the key: `fgcolor` a colour, `font` a font.
fn kind_of(key: &str) -> Option<Kind> {
    let key = key.to_ascii_lowercase();
    if key.contains("font") {
        Some(Kind::Font)
    } else if key.contains("border") {
        Some(Kind::Border)
    } else if key.contains("color") || key.contains("colour") {
        Some(Kind::Colour)
    } else {
        None
    }
}

fn block_in<'a>(pairs: &'a [KvPair], name: &str) -> &'a [KvPair] {
    pairs
        .iter()
        .find_map(|p| match &p.value {
            KvValue::Block(c) if p.key.eq_ignore_ascii_case(name) => Some(c.as_slice()),
            _ => None,
        })
        .unwrap_or_default()
}

fn definition<'a>(section: &'a [KvPair], name: &str) -> Option<&'a [KvPair]> {
    section.iter().find_map(|p| match &p.value {
        KvValue::Block(c) if p.key.eq_ignore_ascii_case(name) => Some(c.as_slice()),
        _ => None,
    })
}

/// For one name: whether the source's scheme knows it, whether the player's has the same
/// definition, and whether a candidate name is free in the player's scheme.
type Lookup<'a> = (bool, bool, Box<dyn Fn(&str) -> bool + 'a>);

/// A colour name that is a name, not a literal number.
fn is_name(value: &str) -> bool {
    !value.is_empty() && !value.starts_with(|c: char| c.is_ascii_digit() || "+-.".contains(c))
}

impl Run<'_> {
    /// Copies, reuses or renames each scheme name the changes so far use, then brings the font
    /// files the copied fonts need.
    pub(super) fn carry_scheme(&mut self) -> Result<(), BorrowRefused> {
        let uses = self.scheme_uses();
        if uses.is_empty() {
            return Ok(());
        }
        let source_scheme = self.source.model.scheme();
        let target_scheme = self.target.scheme();
        let pairs = match (self.source.files).load_kv(SCHEME_FILE, self.source.model.platform()) {
            Ok((pairs, _)) => pairs,
            Err(_) => Vec::new(),
        };
        let source_root = pairs
            .iter()
            .find_map(|p| match &p.value {
                KvValue::Block(c) => Some(c.as_slice()),
                KvValue::Text(_) => None,
            })
            .unwrap_or_default();

        let mut writes = SchemeWrites::default();
        let mut renames: Vec<(Kind, String, String)> = Vec::new();
        let mut taken: HashSet<(Kind, String)> = HashSet::new();
        let mut copied_fonts: Vec<&[KvPair]> = Vec::new();
        for (kind, name) in uses {
            // The name the source's scheme knows, and whether the player's has the same.
            let (known, same, free): Lookup = match kind {
                Kind::Colour => {
                    let theirs = source_scheme.named_colour(&name);
                    let mine = target_scheme.named_colour(&name);
                    (
                        is_name(&name) && theirs.is_some(),
                        mine.is_some() && mine == theirs,
                        Box::new(|n| target_scheme.named_colour(n).is_none()),
                    )
                }
                Kind::Font => {
                    let theirs = source_scheme.font_def(&name);
                    (
                        theirs.is_some(),
                        target_scheme.font_def(&name).is_some()
                            && target_scheme.font_def(&name) == theirs,
                        Box::new(|n| target_scheme.font_def(n).is_none()),
                    )
                }
                Kind::Border => {
                    let theirs = source_scheme.border_def(&name);
                    (
                        theirs.is_some(),
                        target_scheme.border_def(&name).is_some()
                            && target_scheme.border_def(&name) == theirs,
                        Box::new(|n| target_scheme.border_def(n).is_none()),
                    )
                }
            };
            if !known || same {
                continue;
            }
            let absent = free(&name) && !taken.contains(&(kind, name.to_ascii_lowercase()));
            let written = if absent {
                name.clone()
            } else {
                let fresh = (2..=LAST_SUFFIX)
                    .map(|n| format!("{name}_{n}"))
                    .find(|c| free(c) && !taken.contains(&(kind, c.to_ascii_lowercase())))
                    .ok_or(BorrowRefused::NoRoom)?;
                renames.push((kind, name.clone(), fresh.clone()));
                self.renamed.push((name.clone(), fresh.clone()));
                fresh
            };
            taken.insert((kind, written.to_ascii_lowercase()));
            match kind {
                Kind::Colour => {
                    let rgba = source_scheme.named_colour(&name).unwrap_or(Rgba::WHITE);
                    let [r, g, b, a] = rgba.to_array();
                    writes
                        .colours
                        .push((written.clone(), format!("{r} {g} {b} {a}")));
                    self.brings.push(Brings::Colour(written));
                }
                Kind::Font => {
                    let def = definition(block_in(source_root, "Fonts"), &name);
                    if let Some(def) = def {
                        copied_fonts.push(def);
                        writes.blocks.push(("Fonts", written.clone(), def));
                    }
                    self.brings.push(Brings::Font(written));
                }
                Kind::Border => {
                    if let Some(def) = definition(block_in(source_root, "Borders"), &name) {
                        writes.blocks.push(("Borders", written.clone(), def));
                    }
                    self.brings.push(Brings::Border(written));
                }
            }
        }

        if !renames.is_empty() {
            self.rename_values(&renames);
        }
        self.write_scheme(&writes)?;
        self.carry_font_files(&copied_fonts, source_root)
    }

    /// Every (kind, name) the changes' values use, first use first, each once.
    fn scheme_uses(&self) -> Vec<(Kind, String)> {
        let mut uses: Vec<(Kind, String)> = Vec::new();
        let mut note = |key: &str, value: &str| {
            let Some(kind) = kind_of(key) else { return };
            let value = value.trim();
            if !uses
                .iter()
                .any(|(k, n)| *k == kind && n.eq_ignore_ascii_case(value))
            {
                uses.push((kind, value.to_owned()));
            }
        };
        for change in &self.changes {
            match change.target() {
                ChangeTarget::Set(s) => note(s.key(), s.value()),
                ChangeTarget::AddBlock(a) => {
                    a.lines().iter().for_each(|l| note(l.key(), l.value()))
                }
                _ => {}
            }
        }
        uses
    }

    /// Points every copied value that named a renamed entry at its new name.
    fn rename_values(&mut self, renames: &[(Kind, String, String)]) {
        let swap = |key: &str, value: &str| -> Option<String> {
            let kind = kind_of(key)?;
            renames
                .iter()
                .find(|(k, old, _)| *k == kind && old.eq_ignore_ascii_case(value.trim()))
                .map(|(_, _, new)| new.clone())
        };
        self.rewrite_values(&swap);
    }

    /// Adds the colours and block definitions to the player's scheme: a colour line into
    /// `Colors`, a font or border as a block with its nested blocks.
    fn write_scheme(&mut self, writes: &SchemeWrites) -> Result<(), BorrowRefused> {
        if writes.colours.is_empty() && writes.blocks.is_empty() {
            return Ok(());
        }
        let doc = self
            .target
            .document(SCHEME_FILE)
            .ok_or(BorrowRefused::NoRoom)?;
        let root = doc
            .entries()
            .iter()
            .find(|e| e.text().is_none())
            .ok_or(BorrowRefused::NoRoom)?
            .key()
            .to_owned();
        let mut made: HashSet<&str> = HashSet::new();
        let mut ensure = |changes: &mut Vec<Change>, section: &'static str, group: u32| {
            if doc.find(&[&root, section]).is_empty() && made.insert(section) {
                let add = AddBlock::new(SCHEME_FILE, &[&root], section, None, Vec::new());
                changes.push(Change::add_block(group, add));
            }
        };
        for (name, rgb) in &writes.colours {
            ensure(&mut self.changes, "Colors", self.group);
            let set = SetKey::new(SCHEME_FILE, &[&root, "Colors"], name, None, None, rgb);
            self.changes.push(Change::set(self.group, set));
        }
        for (section, name, def) in &writes.blocks {
            ensure(&mut self.changes, section, self.group);
            add_tree(&mut self.changes, self.group, &[&root, section], name, def);
        }
        Ok(())
    }

    /// For each copied font's family the player's scheme lacks, brings the font file the source
    /// ships for it, or lists the family when there is none.
    fn carry_font_files(
        &mut self,
        fonts: &[&[KvPair]],
        _source_root: &[KvPair],
    ) -> Result<(), BorrowRefused> {
        let mut families: Vec<String> = Vec::new();
        for font in fonts {
            for variant in font.iter().filter_map(|p| match &p.value {
                KvValue::Block(c) => Some(c),
                KvValue::Text(_) => None,
            }) {
                let family = variant.iter().find_map(|p| match &p.value {
                    KvValue::Text(t) if p.key.eq_ignore_ascii_case("name") => Some(t.trim()),
                    _ => None,
                });
                if let Some(family) = family.filter(|f| !f.is_empty())
                    && !families.iter().any(|f| f.eq_ignore_ascii_case(family))
                {
                    families.push(family.to_owned());
                }
            }
        }
        let have: Vec<String> = (self.target.scheme().families().iter())
            .map(|f| f.to_ascii_lowercase())
            .collect();
        for family in families {
            if have.contains(&family.to_ascii_lowercase()) {
                continue;
            }
            let found = self.shipped_font(&family);
            let Some((name, bytes)) = found else {
                self.left_behind.push(LeftBehind::MissingFont(family));
                continue;
            };
            let so_far = self.target.with_changes(&self.changes).into_model();
            match so_far.add_font_file(&name, &bytes, self.group) {
                Ok((file, changes)) => {
                    self.assets.insert(file.source(), bytes);
                    self.changes.extend(changes);
                    self.brings.push(Brings::FontFile(family));
                }
                Err(EditRefused::BadValue) => {
                    self.left_behind.push(LeftBehind::MissingFont(family))
                }
                Err(_) => return Err(BorrowRefused::NoRoom),
            }
        }
        Ok(())
    }

    /// The source's font file for `family` as (file name, bytes): a `CustomFontFiles` path under
    /// `resource/` that passes the path rule and holds that family.
    fn shipped_font(&self, family: &str) -> Option<(String, Vec<u8>)> {
        (self.source.model.scheme().font_files().iter()).find_map(|path| {
            let path = check_hud_path(path).filter(|p| p.starts_with("resource/"))?;
            let bytes = self.source.files.read(&path).ok()??.into_bytes();
            let found = font_family(&bytes)?;
            found.eq_ignore_ascii_case(family).then(|| {
                let name = path.rsplit('/').next().unwrap_or_default().to_owned();
                (name, bytes)
            })
        })
    }
}

#[derive(Default)]
struct SchemeWrites<'a> {
    colours: Vec<(String, String)>,
    /// (section, name, definition) for fonts and borders.
    blocks: Vec<(&'static str, String, &'a [KvPair])>,
}

/// Adds the block `name` with the text lines of `def`, then each nested block inside it.
fn add_tree(changes: &mut Vec<Change>, group: u32, parent: &[&str], name: &str, def: &[KvPair]) {
    let lines = def
        .iter()
        .filter_map(|p| match &p.value {
            KvValue::Text(t) => Some(BlockLine::new(&p.key, t, None)),
            KvValue::Block(_) => None,
        })
        .collect();
    changes.push(Change::add_block(
        group,
        AddBlock::new(SCHEME_FILE, parent, name, None, lines),
    ));
    let mut inside: Vec<&str> = parent.to_vec();
    inside.push(name);
    for pair in def {
        if let KvValue::Block(children) = &pair.value {
            add_tree(changes, group, &inside, &pair.key, children);
        }
    }
}
