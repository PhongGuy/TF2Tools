//! Pictures the player picks: a part's picture, written as ordinary `Set` and `Asset` changes
//! whose files are kept under `.tf2tools/images/<id>/`, named by the texture's content.

use std::collections::BTreeSet;

use sha2::{Digest, Sha256};

use super::drawn_image::code_pictures;
use super::edits::set_lines;
use super::images::game_key;
use super::{
    AssetFile, Change, ChangeTarget, ClassSchema, EditRefused, HudModel, IMAGES_DIR, Part, PartKey,
    PropertyKind, normalise_game_path,
};
use crate::texture::vtf_size;

/// Where a picked texture and its material install, and the name a key gives them.
const PICTURE_DIR: &str = "materials/vgui/replay/thumbnails/tf2tools";
const PICTURE_NAME: &str = "replay/thumbnails/tf2tools";
/// The main menu's chapter list, and the one chapter a picked background names.
const CHAPTERS: &str = "scripts/chapterbackgrounds.txt";
const MENU_CHAPTER: &str = "background_upward";

/// Whether a normalised game path is one a picture writes: a `.vmt` or `.vtf` under
/// `materials/`, or the main menu's chapter list.
pub(crate) fn is_picture_path(path: &str) -> bool {
    path == CHAPTERS
        || (path.starts_with("materials/") && (path.ends_with(".vmt") || path.ends_with(".vtf")))
}

/// How a part's picture is replaced.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PictureTarget {
    /// The part's own keys name the picture; each is set to the new one.
    Keys(Vec<&'static str>),
    /// The game's code names the picture: a material at each of these stock paths is
    /// overridden.
    Code {
        /// The lowercase `.vmt` paths overridden.
        materials: Vec<String>,
    },
}

/// One picked picture: its changes, and the bytes each change's kept source holds.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct NewPicture {
    changes: Vec<Change>,
    files: Vec<(String, Vec<u8>)>,
}

impl NewPicture {
    /// The changes, all in one group.
    pub fn changes(&self) -> &[Change] {
        &self.changes
    }

    /// Each kept source with its bytes.
    pub fn files(&self) -> &[(String, Vec<u8>)] {
        &self.files
    }

    /// The changes and the files.
    pub fn into_parts(self) -> (Vec<Change>, Vec<(String, Vec<u8>)>) {
        (self.changes, self.files)
    }
}

/// The content id of a picture: the first 16 lowercase hex digits of the SHA-256 of `bytes`.
pub fn picture_id(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .take(8)
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The lowercase `.vmt` path an image value names, as the game resolves it under
/// `materials/vgui/`; `None` when it names no file or leaves `materials/`.
pub fn picture_material(name: &str) -> Option<String> {
    game_key("materials/vgui", name, ".vmt")
}

/// An UnlitGeneric material drawing the picked texture `id`.
pub(super) fn picture_vmt(id: &str) -> Vec<u8> {
    format!(
        "\"UnlitGeneric\"\n{{\n\t\"$basetexture\" \"vgui/{PICTURE_NAME}/{id}\"\n\t\
         \"$translucent\" \"1\"\n\t\"$vertexcolor\" \"1\"\n\t\"$vertexalpha\" \"1\"\n\t\
         \"$ignorez\" \"1\"\n\t\"$no_fullbright\" \"1\"\n}}\n"
    )
    .into_bytes()
}

/// Collects a picture's files: each lands at its game path, kept under the id's folder.
pub(super) struct PictureFiles {
    id: String,
    group: u32,
    picture: NewPicture,
}

impl PictureFiles {
    pub(super) fn new(id: String, group: u32) -> PictureFiles {
        PictureFiles {
            id,
            group,
            picture: NewPicture {
                changes: Vec::new(),
                files: Vec::new(),
            },
        }
    }

    pub(super) fn id(&self) -> &str {
        &self.id
    }

    pub(super) fn add(&mut self, path: &str, bytes: Vec<u8>) {
        let source = format!("{IMAGES_DIR}/{}/{path}", self.id);
        let change = Change::asset(self.group, AssetFile::new(path, &source));
        self.picture.changes.push(change);
        self.picture.files.push((source, bytes));
    }

    pub(super) fn add_sets(&mut self, sets: Vec<Change>) {
        self.picture.changes.extend(sets);
    }

    pub(super) fn finish(self) -> NewPicture {
        self.picture
    }
}

impl HudModel {
    /// How `part`'s picture is replaced: the stock materials for a picture the game's code
    /// sets everywhere, else the part's image keys; `None` when the part has no picture the
    /// maker offers, including one the scene sample sets.
    pub fn picture_target(&self, part: &PartKey) -> Option<PictureTarget> {
        let part = self.part(part)?;
        if let Some(names) = code_pictures(part) {
            let materials = names.iter().filter_map(|n| picture_material(n)).collect();
            return Some(PictureTarget::Code { materials });
        }
        if self.sample_sets_picture(part) {
            return None;
        }
        let is = |class: &str| part.class().is_some_and(|c| c.eq_ignore_ascii_case(class));
        if is("CTFImagePanel") {
            let mut keys = vec!["teambg_2", "teambg_3"];
            if part.value("image").is_some() {
                keys.push("image");
            }
            return Some(PictureTarget::Keys(keys));
        }
        let image_row = ClassSchema::for_part(part)
            .get("image")
            .is_some_and(|row| row.kind() == PropertyKind::Image);
        image_row.then(|| PictureTarget::Keys(vec!["image"]))
    }

    /// The changes, in `group`, that make `part` draw the texture `vtf`, and the files they
    /// keep. Refuses, in order: UnknownPart; NotOffered when the part has no picture target;
    /// ReadOnly; BadValue when `vtf` is not a VTF.
    pub fn replace_picture(
        &self,
        key: &PartKey,
        vtf: Vec<u8>,
        group: u32,
    ) -> Result<NewPicture, EditRefused> {
        let part = self.part(key).ok_or(EditRefused::UnknownPart)?;
        let target = self.picture_target(key).ok_or(EditRefused::NotOffered)?;
        if !part.is_readable() {
            return Err(EditRefused::ReadOnly);
        }
        vtf_size(&vtf).ok_or(EditRefused::BadValue)?;
        let mut files = PictureFiles::new(picture_id(&vtf), group);
        let id = files.id().to_owned();
        let vmt = picture_vmt(&id);
        files.add(&format!("{PICTURE_DIR}/{id}.vtf"), vtf);
        match target {
            PictureTarget::Keys(keys) => {
                files.add(&format!("{PICTURE_DIR}/{id}.vmt"), vmt);
                let name = format!("{PICTURE_NAME}/{id}");
                for key in keys {
                    files.add_sets(set_lines(part, key, group, |_| name.clone()));
                }
                // The game draws a texture at its own size unless told to stretch it over the
                // box, as the preview always does.
                let scale_row = ClassSchema::for_part(part).get("scaleImage").is_some();
                let scaled = part.value("scaleImage").is_some_and(|l| l.text() == "1");
                if scale_row && !scaled {
                    files.add_sets(set_lines(part, "scaleImage", group, |_| "1".to_owned()));
                }
            }
            PictureTarget::Code { materials } => {
                for material in materials {
                    files.add(&material, vmt.clone());
                }
            }
        }
        Ok(files.finish())
    }

    /// `changes` without every group that picked a picture for `part`: a `Set` of one of its
    /// target keys, or an `Asset` at one of its target materials.
    pub fn reset_picture(&self, part: &PartKey, changes: &[Change]) -> Vec<Change> {
        let groups: BTreeSet<u32> = match self.picture_target(part) {
            None => return changes.to_vec(),
            Some(PictureTarget::Keys(keys)) => {
                let kept = self.reset_property(part, &keys, changes);
                // `reset_property` keeps order, so the dropped changes are those it skipped.
                let mut kept = kept.iter().peekable();
                (changes.iter())
                    .filter(|c| {
                        let stays = kept.peek() == Some(c);
                        if stays {
                            kept.next();
                        }
                        !stays
                    })
                    .map(Change::group)
                    .collect()
            }
            Some(PictureTarget::Code { materials }) => (changes.iter())
                .filter(|c| match c.target() {
                    ChangeTarget::Asset(asset) => normalise_game_path(asset.path())
                        .is_some_and(|p| materials.contains(&p.to_ascii_lowercase())),
                    ChangeTarget::Set(_)
                    | ChangeTarget::Remove(_)
                    | ChangeTarget::AddBlock(_)
                    | ChangeTarget::Option(_)
                    | ChangeTarget::Credit(_)
                    | ChangeTarget::Animation(_)
                    | ChangeTarget::AnimationAppend(_) => false,
                })
                .map(Change::group)
                .collect(),
        };
        (changes.iter())
            .filter(|c| !groups.contains(&c.group()))
            .cloned()
            .collect()
    }

    /// The changes, in `group`, that make the main menu draw the textures `narrow` (4:3) and
    /// `wide` (16:9): a chapter list naming only `background_upward`, its two materials and
    /// the two textures. Refuses with BadValue when either is not a VTF.
    pub fn set_menu_background(
        &self,
        narrow: Vec<u8>,
        wide: Vec<u8>,
        group: u32,
    ) -> Result<NewPicture, EditRefused> {
        vtf_size(&narrow).ok_or(EditRefused::BadValue)?;
        vtf_size(&wide).ok_or(EditRefused::BadValue)?;
        let id = picture_id(&[narrow.as_slice(), wide.as_slice()].concat());
        let mut files = PictureFiles::new(id.clone(), group);
        let chapters = format!("\"chapters\"\n{{\n\t\"1\"\t\"{MENU_CHAPTER}\"\n}}\n");
        files.add(CHAPTERS, chapters.into_bytes());
        for (suffix, texture) in [("", narrow), ("_widescreen", wide)] {
            let name = format!("{id}{suffix}");
            let vmt = picture_vmt(&name);
            files.add(
                &format!("materials/console/{MENU_CHAPTER}{suffix}.vmt"),
                vmt,
            );
            files.add(&format!("{PICTURE_DIR}/{name}.vtf"), texture);
        }
        Ok(files.finish())
    }

    /// `changes` without every group that wrote the main menu's chapter list.
    pub fn reset_menu_background(&self, changes: &[Change]) -> Vec<Change> {
        let groups: BTreeSet<u32> = (changes.iter())
            .filter(|c| match c.target() {
                ChangeTarget::Asset(asset) => normalise_game_path(asset.path())
                    .is_some_and(|p| p.eq_ignore_ascii_case(CHAPTERS)),
                ChangeTarget::Set(_)
                | ChangeTarget::Remove(_)
                | ChangeTarget::AddBlock(_)
                | ChangeTarget::Option(_)
                | ChangeTarget::Credit(_)
                | ChangeTarget::Animation(_)
                | ChangeTarget::AnimationAppend(_) => false,
            })
            .map(Change::group)
            .collect();
        (changes.iter())
            .filter(|c| !groups.contains(&c.group()))
            .cloned()
            .collect()
    }

    /// Whether the scene sample sets `part`'s picture: a class-menu picture the game picks, or
    /// a class button's `SubImage`.
    fn sample_sets_picture(&self, part: &Part) -> bool {
        let sample = self.scene_rule().sample();
        let parent = part.parent().and_then(|k| self.part(k)).map(|p| p.name());
        sample.code_image(part.name(), parent).is_some()
            || parent.is_some_and(|p| sample.is_class_button(p))
    }
}
