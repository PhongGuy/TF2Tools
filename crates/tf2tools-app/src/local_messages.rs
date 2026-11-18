//! Local library, profile, Settings and startup failures in player words. The messages never
//! carry paths or raw error text; the raw error rides along as `detail` for the developer log.

use tf2tools_core::{
    ApplyProfileError, CopyHudError, DetectError, FontInstallError, GbAddonInstallError,
    GbMapInstallError, GbMapRemoveError, HudLoadError, ImportError, InstallError, KeepHudError,
    LibraryError, LogError, LogsCacheError, MakeHudError, PaintError, PngReadError, PngWriteError,
    ProfileError, QueryError, SettingsError, StorageError, UpdateCopyError, WavError,
};

use gpui_kit::SharedString;

use crate::audio::AudioError;
use crate::i18n::{list, plural, tr};
use crate::pages::graphics::jobs::{close_tf2, close_tf2_text};
use crate::services::{OpError, StartupError};
use crate::store_messages::{library_update, store_read};

/// What a caught panic tells the player.
pub fn internal() -> SharedString {
    tr!("messages.shared.internal")
}

fn library_disk() -> SharedString {
    tr!("messages.local.library_disk")
}
pub(crate) fn library_open() -> SharedString {
    tr!("messages.shared.library_open")
}
fn gone() -> SharedString {
    tr!("messages.local.gone")
}
fn unusable() -> SharedString {
    tr!("messages.local.unusable")
}
pub(crate) fn tf2_write() -> SharedString {
    tr!("messages.shared.tf2_write")
}
fn new_external_hud() -> SharedString {
    tr!("messages.local.new_external_hud")
}
fn profile_gone() -> SharedString {
    tr!("messages.local.profile_gone")
}
fn profiles_open() -> SharedString {
    tr!("messages.local.profiles_open")
}
/// The game's HUD files are missing or unreadable.
pub fn hud_files() -> SharedString {
    tr!("messages.shared.hud_files")
}
/// The game folder moved away since TF2Tools found it, so nothing was written.
pub(crate) fn game_moved() -> SharedString {
    tr!("messages.shared.game_moved")
}
fn unknown() -> SharedString {
    tr!("messages.local.unknown")
}

/// What the player reads about a failed local action, in the language it was built in, and the
/// raw error for the developer log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalFailure {
    pub message: SharedString,
    pub detail: String,
}

impl LocalFailure {
    /// A failure with its player message and its raw developer detail.
    pub fn new(message: impl Into<SharedString>, detail: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            detail: detail.into(),
        }
    }

    /// Writes `detail` to the developer log as "{what}: {detail}".
    pub fn log(&self, what: &str) {
        tracing::warn!("{what}: {}", self.detail);
    }

    /// The toast the player reads under `action`; logs its English developer line first.
    #[track_caller]
    pub fn into_op(self, action: SharedString) -> OpError {
        let op = OpError::Core {
            action,
            message: self.message,
            detail: self.detail,
            at: std::panic::Location::caller(),
        };
        tracing::warn!("{op}");
        op
    }
}

/// A failed add-on install, reinstall or remove in player words.
pub fn describe_gb_install(err: &GbAddonInstallError) -> LocalFailure {
    let message = match err {
        GbAddonInstallError::Tf2Running => close_tf2_text(),
        GbAddonInstallError::GameMissing { .. } => game_moved(),
        GbAddonInstallError::Io { .. } => tr!("messages.local.gb_install.io"),
        GbAddonInstallError::NotInLibrary { .. } => tr!("messages.local.gb_install.not_in_library"),
        GbAddonInstallError::Database { .. } => library_update(),
        GbAddonInstallError::UnknownPayload { .. } | GbAddonInstallError::NothingChosen | _ => {
            unknown()
        }
    };
    LocalFailure::new(message, err.to_string())
}

/// The toast for a failed add-on operation titled `action`: "Close TF2 first" when TF2 runs.
#[track_caller]
pub fn gb_install_op(err: &GbAddonInstallError, action: SharedString) -> OpError {
    let action = if matches!(err, GbAddonInstallError::Tf2Running) {
        close_tf2()
    } else {
        action
    };
    describe_gb_install(err).into_op(action)
}

/// The toast title of a map installed while TF2 runs: the game only sees it after a restart.
pub fn map_restart() -> SharedString {
    tr!("messages.shared.map_restart")
}

/// Added to a map toast's description when files beside the maps were left out.
pub fn map_skipped() -> SharedString {
    tr!("messages.shared.map_skipped")
}

/// A failed map install or update in player words.
pub fn describe_gb_map_install(err: &GbMapInstallError) -> LocalFailure {
    let message = match err {
        GbMapInstallError::Tf2Running => close_tf2_text(),
        GbMapInstallError::GameMissing { .. } => game_moved(),
        GbMapInstallError::AlreadyPresent { names } => already_have(names, false).into(),
        GbMapInstallError::Io { .. } => tr!("messages.local.gb_map_install.io"),
        GbMapInstallError::Database { .. } => library_update(),
        GbMapInstallError::NothingChosen | GbMapInstallError::UnknownMap { .. } | _ => unknown(),
    };
    LocalFailure::new(message, err.to_string())
}

/// The toast for a failed map operation titled `action`: "Close TF2 first" when TF2 runs.
#[track_caller]
pub fn gb_map_install_op(err: &GbMapInstallError, action: SharedString) -> OpError {
    let action = if matches!(err, GbMapInstallError::Tf2Running) {
        close_tf2()
    } else {
        action
    };
    describe_gb_map_install(err).into_op(action)
}

/// A failed map remove in player words.
pub fn describe_gb_map_remove(err: &GbMapRemoveError) -> LocalFailure {
    let message = match err {
        GbMapRemoveError::Tf2Running => close_tf2_text(),
        GbMapRemoveError::Io { .. } => tr!("messages.local.gb_map_remove.io"),
        GbMapRemoveError::NotInLibrary { .. } => {
            tr!("messages.local.gb_map_remove.not_in_library")
        }
        GbMapRemoveError::Database { .. } => library_update(),
        GbMapRemoveError::NotAMap { .. } | _ => unknown(),
    };
    LocalFailure::new(message, err.to_string())
}

/// The toast for a failed map remove titled `action`: "Close TF2 first" when TF2 runs.
#[track_caller]
pub fn gb_map_remove_op(err: &GbMapRemoveError, action: SharedString) -> OpError {
    let action = if matches!(err, GbMapRemoveError::Tf2Running) {
        close_tf2()
    } else {
        action
    };
    describe_gb_map_remove(err).into_op(action)
}

/// "You already have cp_a, cp_b and cp_c." When `serveme`, one of them came from the
/// serveme.tf tab, which is where the player removes it.
pub fn already_have(names: &[String], serveme: bool) -> String {
    let list = list(names);
    match serveme {
        true => tr!("messages.local.already_have.serveme", list = list),
        false => tr!("messages.local.already_have.plain", list = list),
    }
    .into()
}

/// A failed read or change of the library in player words; `detail` is the error's own text.
pub fn describe_library(err: &LibraryError) -> LocalFailure {
    let message = match err {
        LibraryError::Io { .. } => library_disk(),
        LibraryError::NotFound { .. } => gone(),
        LibraryError::InvalidWeaponScript { .. } => {
            tr!("messages.local.library.unknown_weapon")
        }
        LibraryError::Vpk(_) => {
            tr!("messages.local.library.vpk")
        }
        LibraryError::Vtf(_) => {
            tr!("messages.local.library.vtf")
        }
        LibraryError::Database { .. } => library_open(),
        _ => unknown(),
    };
    LocalFailure::new(message, err.to_string())
}

/// A failed storage measure in player words; `detail` is the error's own text.
pub fn describe_storage(err: &StorageError) -> LocalFailure {
    let message = match err {
        StorageError::Io { .. } => library_disk(),
        StorageError::Database { .. } => library_open(),
        _ => unknown(),
    };
    LocalFailure::new(message, err.to_string())
}

/// A copy's files could not be read to show a newly chosen option of its own.
pub fn hud_options_unreadable() -> SharedString {
    tr!("messages.shared.hud_options_unreadable")
}

/// A HUD Maker save refused because the copy was updated after the page opened it.
pub fn hud_base_changed() -> SharedString {
    tr!("messages.shared.hud_base_changed")
}

/// A failed HUD Maker save in player words; `detail` is the error's own text.
pub fn describe_make_hud(err: &MakeHudError) -> LocalFailure {
    let message = match err {
        MakeHudError::Io { .. } => tr!("messages.local.make_hud.io"),
        MakeHudError::NameTaken { name } => tr!("messages.local.make_hud.name_taken", name = name),
        MakeHudError::EmptyName => tr!("messages.local.make_hud.empty_name"),
        MakeHudError::NotFound { .. } => {
            tr!("messages.local.make_hud.not_found")
        }
        MakeHudError::NotMade { .. } => {
            tr!("messages.local.make_hud.not_made")
        }
        MakeHudError::NoSpace { needed, .. } => no_space_text(SpaceFor::Save, *needed).into(),
        MakeHudError::Files(_) => hud_files(),
        MakeHudError::Database { .. } => library_update(),
        MakeHudError::BaseChanged => hud_base_changed(),
        _ => unknown(),
    };
    LocalFailure::new(message, err.to_string())
}

/// A made HUD that failed to reopen in the HUD Maker, in player words.
pub fn describe_hud_load(err: &HudLoadError) -> LocalFailure {
    let message = match err {
        HudLoadError::Damaged { .. } | HudLoadError::Newer | HudLoadError::NotMade { .. } => {
            tr!("messages.local.hud_load.cant_edit")
        }
        HudLoadError::NotFound { .. } => gone(),
        HudLoadError::Files(_) => hud_files(),
        HudLoadError::Io { .. } => library_disk(),
        HudLoadError::Database { .. } => library_open(),
        _ => unknown(),
    };
    LocalFailure::new(message, err.to_string())
}

/// The sentence about saved changes that no longer fit, and about a TF2 update since the save;
/// `None` when every change fit.
pub fn left_out(dropped: usize, tf2_updated: bool) -> Option<String> {
    if dropped == 0 {
        return None;
    }
    let text = plural("messages.local.left_out", count(dropped));
    Some(match tf2_updated {
        true => format!("{text} {}", tr!("messages.local.save_again")),
        false => text.into(),
    })
}

// A count as the plural rules take it; no real count reaches `u64::MAX`.
fn count(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

/// The toast after reopening a made HUD.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReopenNotice {
    pub title: SharedString,
    pub text: String,
    /// True when changes were left out; a TF2 update alone is only a note.
    pub warning: bool,
}

/// What reopening a made HUD tells the player: a warning when changes were left out, a note
/// when TF2 updated its files under changes that all still apply, else nothing.
pub fn reopen_notice(dropped: usize, tf2_updated: bool) -> Option<ReopenNotice> {
    if let Some(text) = left_out(dropped, tf2_updated) {
        return Some(ReopenNotice {
            title: tr!("messages.local.left_out_title"),
            text,
            warning: true,
        });
    }
    tf2_updated.then(|| ReopenNotice {
        title: tr!("messages.local.reopen.tf2_updated_title"),
        text: tr!("messages.local.reopen.tf2_updated").into(),
        warning: false,
    })
}

/// The sentence added to the reopen notice when `count` borrowed pictures were left out, else
/// `None`.
pub fn pictures_left_out(count: usize) -> Option<SharedString> {
    (count > 0).then(|| tr!("messages.local.reopen.pictures_left_out"))
}

/// What the player reads once a copy is made: it is theirs to edit, and whose work it is.
pub fn copy_made_text(copy_name: &str, author: &str) -> String {
    let terms = terms_note(author);
    tr!("messages.local.copy_made", copy = copy_name, terms = terms).into()
}

/// The installed HUD's Edit made a copy of `base`: the player edits that, not the original.
pub fn editing_copy_text(base: &str, author: &str) -> String {
    let terms = terms_note(author);
    tr!("messages.local.editing_copy", base = base, terms = terms).into()
}

// Whose work a copied HUD is, and where its sharing terms live.
fn terms_note(author: &str) -> SharedString {
    match author {
        "" => tr!("messages.local.terms.no_author"),
        author => tr!("messages.local.terms.author", author = author),
    }
}

/// The action a disk-space refusal names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpaceFor {
    Save,
    Copy,
    Update,
}

/// A refused copy, save or update for want of disk room; the size is shown in whole
/// 1024-based megabytes, rounded up.
pub fn no_space_text(for_: SpaceFor, needed_bytes: u64) -> String {
    let mb = needed_bytes.div_ceil(1024 * 1024);
    match for_ {
        SpaceFor::Save => tr!("messages.local.no_space.save", mb = mb),
        SpaceFor::Copy => tr!("messages.local.no_space.copy", mb = mb),
        SpaceFor::Update => tr!("messages.local.no_space.update", mb = mb),
    }
    .into()
}

fn hud_gone() -> SharedString {
    tr!("messages.local.hud_gone")
}

/// A failed "Edit a copy" in player words; `detail` is the error's own text.
pub fn describe_copy_hud(err: &CopyHudError) -> LocalFailure {
    let message = match err {
        CopyHudError::NotFound { .. } => hud_gone(),
        CopyHudError::BaseMissing { .. } => {
            tr!("messages.local.copy_hud.base_missing")
        }
        CopyHudError::MadeHud { .. } => {
            tr!("messages.local.copy_hud.made_hud")
        }
        CopyHudError::NameTaken { .. } => {
            tr!("messages.local.copy_hud.name_taken")
        }
        CopyHudError::NoSpace { needed, .. } => no_space_text(SpaceFor::Copy, *needed).into(),
        CopyHudError::Io { .. } => library_disk(),
        CopyHudError::Database { .. } => library_open(),
        _ => unknown(),
    };
    LocalFailure::new(message, err.to_string())
}

/// The line on a copy's card when its base's newer version is only in the HUD Store.
pub fn copy_in_store_text(hud: &str) -> String {
    tr!("messages.local.copy_in_store", hud = hud).into()
}

// At most this many left-out changes are named; the rest are counted.
const NAMED_CHANGES: usize = 5;

// A change in player words, as a whole phrase: what about `part` it changed. An unknown key
// is named as it is.
fn key_word(part: &str, key: &str) -> SharedString {
    match key {
        "xpos" | "ypos" => tr!("messages.local.key_word.position", part = part),
        "wide" | "tall" => tr!("messages.local.key_word.size", part = part),
        "zpos" => tr!("messages.local.key_word.layer", part = part),
        "visible" | "enabled" => tr!("messages.local.key_word.visibility", part = part),
        "fgcolor" | "fgcolor_override" | "bgcolor_override" | "drawcolor" => {
            tr!("messages.local.key_word.colour", part = part)
        }
        "font" => tr!("messages.local.key_word.font", part = part),
        "labelText" => tr!("messages.local.key_word.text", part = part),
        key => tr!("messages.local.key_word.fallback", key = key, part = part),
    }
}

/// What updating copy `hud` tells the player: a note when every change still applies, a
/// warning naming the `(part, key)` changes left out. `both_changed` counts kept changes whose
/// target the update also changed.
pub fn copy_updated_notice(
    hud: &str,
    left_out: &[(&str, Option<&str>)],
    both_changed: usize,
) -> ReopenNotice {
    let lead = tr!("messages.local.copy_updated.lead", hud = hud);
    let (title, mut text, warning) = if left_out.is_empty() {
        let text = format!("{lead} {}", tr!("messages.local.copy_updated.all_apply"));
        (tr!("messages.local.copy_updated.title"), text, false)
    } else {
        // The first few changes are named and the rest counted as one last item of the list.
        let mut items: Vec<SharedString> = (left_out.iter().take(NAMED_CHANGES))
            .map(|(part, key)| match key {
                Some(key) => key_word(part, key),
                None => (*part).into(),
            })
            .collect();
        let rest = left_out.len().saturating_sub(NAMED_CHANGES);
        if rest > 0 {
            items.push(plural("messages.local.copy_updated.more", count(rest)));
        }
        // `plural` fills only the count, so the list goes in afterwards.
        let sentence = plural(
            "messages.local.copy_updated.no_longer_fit",
            count(left_out.len()),
        );
        let sentence = rust_i18n::replace_patterns(&sentence, &["list"], &[list(&items).into()]);
        let text = format!("{lead} {sentence}");
        (tr!("messages.local.left_out_title"), text, true)
    };
    if both_changed > 0 {
        let both = plural(
            "messages.local.copy_updated.both_changed",
            count(both_changed),
        );
        text = format!("{text} {both}");
    }
    ReopenNotice {
        title,
        text,
        warning,
    }
}

/// A failed update of a HUD copy in player words; `detail` is the error's own text.
pub fn describe_update_copy(err: &UpdateCopyError) -> LocalFailure {
    let message = match err {
        UpdateCopyError::NotFound { .. } => hud_gone(),
        UpdateCopyError::NotACopy { .. } => tr!("messages.local.update_copy.not_a_copy"),
        UpdateCopyError::SourceGone { .. } => {
            tr!("messages.local.update_copy.source_gone")
        }
        UpdateCopyError::NotSameSource { .. } | UpdateCopyError::MadeHud { .. } => {
            tr!("messages.local.update_copy.not_same_source")
        }
        UpdateCopyError::NoSpace { needed, .. } => no_space_text(SpaceFor::Update, *needed).into(),
        UpdateCopyError::Load(e) => describe_hud_load(e).message,
        UpdateCopyError::Save(e) => describe_make_hud(e).message,
        _ => unknown(),
    };
    LocalFailure::new(message, err.to_string())
}

/// Which maker action a `PaintError` came from; the same error can read differently per step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintStep {
    /// Saving a drawing as a new crosshair.
    SaveNew,
    /// Saving over the crosshair the drawing was opened from.
    Resave,
    /// Opening a painted crosshair for editing.
    Open,
}

/// A failed save or open in the Crosshair Maker; `detail` is the error's own text.
pub fn describe_paint(err: &PaintError, step: PaintStep) -> LocalFailure {
    let message = match (err, step) {
        (PaintError::Io { .. }, PaintStep::Open) => library_disk(),
        (PaintError::Io { .. }, _) => tr!("messages.local.paint.io"),
        (PaintError::Database { .. }, PaintStep::SaveNew) => library_update(),
        // One sentence true whether the re-save failed before or after its files were written.
        (PaintError::Database { .. }, PaintStep::Resave) => {
            tr!("messages.local.paint.resave_database")
        }
        (PaintError::Database { .. }, PaintStep::Open) => library_open(),
        (PaintError::NameTaken { name }, _) => {
            tr!("messages.local.paint.name_taken", name = name)
        }
        (PaintError::NotFound { .. }, PaintStep::Open) => gone(),
        (PaintError::NotFound { .. }, _) => {
            tr!("messages.local.paint.not_found")
        }
        (PaintError::Unreadable { .. }, _) => tr!("messages.local.paint.unreadable"),
        (PaintError::NotPainted { .. }, _) => {
            tr!("messages.local.paint.not_painted")
        }
        (PaintError::EmptyName, _) => tr!("messages.local.paint.empty_name"),
        _ => unknown(),
    };
    LocalFailure::new(message, err.to_string())
}

/// A failed import of a picked file in player words; `detail` is the error's own text.
pub fn describe_import(err: &ImportError) -> LocalFailure {
    let message = match err {
        ImportError::Io { .. } => tr!("messages.local.import.io"),
        ImportError::Archive(_) => {
            tr!("messages.local.import.archive")
        }
        ImportError::NotAHud { .. } => {
            tr!("messages.local.import.not_a_hud")
        }
        ImportError::UnsupportedFile { .. }
        | ImportError::NotAVpk { .. }
        | ImportError::UnsupportedProvenance => unusable(),
        ImportError::Wav(WavError::Unsupported { .. }) => {
            tr!("messages.local.import.wav_unsupported")
        }
        ImportError::Wav(_) => {
            tr!("messages.local.import.wav")
        }
        ImportError::Vtf(_) => {
            tr!("messages.local.import.vtf")
        }
        ImportError::Database { .. } => library_update(),
        _ => unknown(),
    };
    LocalFailure::new(message, err.to_string())
}

/// A PNG that could not be opened, in player words; `detail` is the error's own text.
pub fn describe_png_read(err: &PngReadError) -> LocalFailure {
    let message = match err {
        PngReadError::Io { .. } => tr!("messages.local.png_read.io"),
        PngReadError::FileTooLarge { .. } | PngReadError::TooLarge { .. } => {
            tr!("messages.local.png_read.too_large")
        }
        PngReadError::Invalid { .. } => {
            tr!("messages.local.png_read.invalid")
        }
        _ => unknown(),
    };
    LocalFailure::new(message, err.to_string())
}

/// A PNG that could not be saved, in player words; `detail` is the error's own text.
pub fn describe_png_write(err: &PngWriteError) -> LocalFailure {
    let message = match err {
        PngWriteError::Io { .. } => {
            tr!("messages.local.png_write.io")
        }
        PngWriteError::Encode { .. } | _ => unknown(),
    };
    LocalFailure::new(message, err.to_string())
}

/// A failed copy into, or removal from, tf/custom; `detail` is the error's own text.
pub fn describe_tf2_install(err: &InstallError) -> LocalFailure {
    let message = match err {
        InstallError::Io { .. } => tf2_write(),
        InstallError::NotInLibrary { .. } => gone(),
        InstallError::Database { .. } => library_update(),
        InstallError::ExternalHuds(_) => new_external_hud(),
        InstallError::GameMissing { .. } => game_moved(),
        _ => unknown(),
    };
    LocalFailure::new(message, err.to_string())
}

/// A failed copy of a HUD into the library; nothing left TF2, and `detail` is the error's own text.
pub fn describe_keep_hud(err: &KeepHudError) -> LocalFailure {
    let message = match err {
        KeepHudError::Io { .. } => tr!("messages.local.keep_hud.io"),
        KeepHudError::NotCopyable { .. } => tr!("messages.local.keep_hud.not_copyable"),
        KeepHudError::Database { .. } => library_update(),
        _ => unknown(),
    };
    LocalFailure::new(message, err.to_string())
}

/// A failed read or save of a profile in player words; `detail` is the error's own text.
pub fn describe_profile(err: &ProfileError) -> LocalFailure {
    let message = match err {
        ProfileError::NotFound { .. } => profile_gone(),
        ProfileError::EmptyName => tr!("messages.local.profile.empty_name"),
        ProfileError::NameTaken { name } => {
            let message = crate::pages::profiles::model::taken_message(name);
            return LocalFailure::new(message, err.to_string());
        }
        ProfileError::ItemNotFound { .. } => tr!("messages.local.profile.item_not_found"),
        ProfileError::InvalidWeaponScript { .. } => tr!("messages.local.profile.unknown_weapon"),
        ProfileError::Library(e) => {
            return LocalFailure::new(describe_library(e).message, err.to_string());
        }
        ProfileError::Database { .. } => profiles_open(),
        _ => unknown(),
    };
    LocalFailure::new(message, err.to_string())
}

/// A profile activation that failed before any step ran; `detail` is the error's own text.
pub fn describe_apply_profile(err: &ApplyProfileError) -> LocalFailure {
    let message = match err {
        ApplyProfileError::NotFound { .. } => profile_gone(),
        ApplyProfileError::Missing(_) => tr!("messages.local.apply_profile.missing"),
        ApplyProfileError::ExternalHuds(_) => new_external_hud(),
        ApplyProfileError::Io { .. } => tf2_write(),
        ApplyProfileError::Database { .. } => profiles_open(),
        ApplyProfileError::GameMissing { .. } => game_moved(),
        _ => unknown(),
    };
    LocalFailure::new(message, err.to_string())
}

/// A failed read or save of the settings in player words; `detail` is the error's own text.
pub fn describe_settings(err: &SettingsError) -> LocalFailure {
    let message = match err {
        SettingsError::Database { .. } => {
            tr!("messages.local.settings.database")
        }
        SettingsError::NonUtf8Path { .. } => {
            tr!("messages.local.settings.non_utf8_path")
        }
        _ => unknown(),
    };
    LocalFailure::new(message, err.to_string())
}

/// A TF2 folder the player typed or picked that can't be used; `detail` is the error's own text.
pub fn describe_tf2_folder(err: &DetectError) -> LocalFailure {
    let message = match err {
        DetectError::NotTf2 { .. } => {
            tr!("messages.local.tf2_folder.not_tf2")
        }
        DetectError::Io { .. } => {
            tr!("messages.local.tf2_folder.io")
        }
        _ => unknown(),
    };
    LocalFailure::new(message, err.to_string())
}

/// A failed auto-detect over the Steam library. Every cause reads the same, since the player's
/// next step is always to pick the folder by hand.
pub fn describe_detect(err: &DetectError) -> LocalFailure {
    LocalFailure::new(tr!("messages.local.detect.failed"), err.to_string())
}

/// A failed read of the stores' sync status; `detail` is the error's own text.
pub fn describe_store_status(err: &QueryError) -> LocalFailure {
    LocalFailure::new(store_read(), err.to_string())
}

/// A failed install of a HUD's fonts in player words; `detail` is the error's own text.
pub fn describe_font_install(err: &FontInstallError) -> LocalFailure {
    let message = match err {
        FontInstallError::Io { .. } => {
            tr!("messages.local.font_install.io")
        }
        FontInstallError::Host { .. } | FontInstallError::Register { .. } => {
            tr!("messages.local.font_install.host")
        }
        FontInstallError::Refresh { .. } => {
            tr!("messages.local.font_install.refresh")
        }
        _ => unknown(),
    };
    LocalFailure::new(message, err.to_string())
}

/// A sound that couldn't be played in player words; `detail` is the error's own text.
pub fn describe_audio(err: &AudioError) -> LocalFailure {
    let message = match err {
        AudioError::Device(_) => {
            tr!("messages.local.audio.device")
        }
        AudioError::Decode(_) => tr!("messages.local.audio.decode"),
        AudioError::Closed => {
            tr!("messages.local.audio.closed")
        }
    };
    LocalFailure::new(message, err.to_string())
}

/// A failed read or clear of the activity log in player words; `detail` is the error's own text.
pub fn describe_log(err: &LogError) -> LocalFailure {
    let message = match err {
        LogError::Database { .. } => {
            tr!("messages.local.log.database")
        }
        _ => unknown(),
    };
    LocalFailure::new(message, err.to_string())
}

/// A failed read or write of the logs.tf cache; `detail` is the error's own text.
pub fn describe_logs_cache(err: &LogsCacheError) -> LocalFailure {
    let message = match err {
        LogsCacheError::Database { .. } => tr!("messages.local.logs_cache.database"),
        _ => unknown(),
    };
    LocalFailure::new(message, err.to_string())
}

/// A background job that panicked or was cancelled; `detail` is the JoinError's text.
pub fn describe_join(err: &tokio::task::JoinError) -> LocalFailure {
    LocalFailure::new(internal(), err.to_string())
}

/// Why the app could not start, in player words; `detail` is the error's own text.
pub fn describe_startup(err: &StartupError) -> LocalFailure {
    let message = match err {
        StartupError::NoAssets(_) | StartupError::Catalog { .. } => {
            tr!("messages.local.startup.no_assets")
        }
        // Also covers a failed settings read right after the open.
        StartupError::Database { .. } => {
            tr!("messages.local.startup.database")
        }
        StartupError::NewerDatabase { .. } => {
            tr!("messages.local.startup.newer_database")
        }
    };
    LocalFailure::new(message, err.to_string())
}
