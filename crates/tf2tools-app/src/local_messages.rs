//! Local library, profile, Settings and startup failures in player words. The messages never
//! carry paths or raw error text; the raw error rides along as `detail` for the developer log.

use tf2tools_core::{
    ApplyProfileError, ArchiveError, CopyHudError, DetectError, FontInstallError, HudLoadError,
    ImportError, InstallError, KeepHudError, LibraryError, LogError, MakeHudError, PaintError,
    PngReadError, PngWriteError, ProfileError, QueryError, SettingsError, UpdateCopyError,
    WavError,
};

use crate::audio::AudioError;
use crate::services::{OpError, StartupError};
use crate::store_messages::{LIBRARY_UPDATE, STORE_READ};

/// What a caught panic tells the player.
pub const INTERNAL: &str = "TF2Tools hit a problem it didn't expect. Try again, and restart TF2Tools if it keeps happening.";

const LIBRARY_DISK: &str = "TF2Tools couldn't read or save its files. Make sure your disk has free space and TF2Tools can write to its folder, then try again.";
const LIBRARY_OPEN: &str = "TF2Tools couldn't open your library. Restart TF2Tools and try again.";
const GONE: &str = "This item is no longer in your library. Pick another one.";
const UNUSABLE: &str = "TF2Tools can't use this kind of file. Pick another file.";
const TF2_WRITE: &str = "Couldn't change your TF2 folder. Make sure TF2 is closed and TF2Tools can write to it, then try again.";
const NEW_EXTERNAL_HUD: &str =
    "Another HUD turned up in your TF2 folder, so nothing was changed. Try again.";
const PROFILE_GONE: &str = "This profile no longer exists. Pick another profile.";
const PROFILES_OPEN: &str = "TF2Tools couldn't open your profiles. Restart TF2Tools and try again.";
/// The game's HUD files are missing or unreadable.
pub const HUD_FILES: &str =
    "TF2Tools can't find TF2's HUD files. Check the TF2 folder in Settings.";
/// The game folder moved away since TF2Tools found it, so nothing was written.
pub(crate) const GAME_MOVED: &str =
    "TF2 wasn't found where it used to be. Check the TF2 folder in Settings.";
const UNKNOWN: &str =
    "Something went wrong. Try again, and restart TF2Tools if it keeps happening.";

/// What the player reads about a failed local action, and the raw error for the developer log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalFailure {
    pub message: String,
    pub detail: String,
}

impl LocalFailure {
    /// A failure with its player message and its raw developer detail.
    pub fn new(message: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            detail: detail.into(),
        }
    }

    /// Writes `detail` to the developer log as "{what}: {detail}".
    pub fn log(&self, what: &str) {
        tracing::warn!("{what}: {}", self.detail);
    }

    /// Logs the raw detail under `action`, then returns the toast the player reads.
    pub fn into_op(self, action: &'static str) -> OpError {
        self.log(action);
        OpError::Core {
            action,
            message: self.message,
        }
    }
}

/// A failed read or change of the library in player words; `detail` is the error's own text.
pub fn describe_library(err: &LibraryError) -> LocalFailure {
    let message = match err {
        LibraryError::Io { .. } => LIBRARY_DISK,
        LibraryError::NotFound { .. } => GONE,
        LibraryError::InvalidWeaponScript { .. } => {
            "TF2Tools doesn't know this weapon. Pick another weapon."
        }
        LibraryError::Vpk(_) => "This sound file couldn't be read. Delete it and add it again.",
        LibraryError::Vtf(_) => {
            "This crosshair image couldn't be read. Delete it and add it again."
        }
        LibraryError::Database { .. } => LIBRARY_OPEN,
        _ => UNKNOWN,
    };
    LocalFailure::new(message, err.to_string())
}

/// A copy's files could not be read to show a newly chosen option of its own.
pub const HUD_OPTIONS_UNREADABLE: &str = "This HUD's files couldn't be read, so the canvas can't show that option yet. Your choice is kept.";

/// A HUD Maker save refused because the copy was updated after the page opened it.
pub const HUD_BASE_CHANGED: &str =
    "This HUD was updated while it was open. Close it and open it again to keep editing.";

/// A failed HUD Maker save in player words; `detail` is the error's own text.
pub fn describe_make_hud(err: &MakeHudError) -> LocalFailure {
    let message = match err {
        MakeHudError::Io { .. } => "Couldn't save the HUD: your disk may be full or the folder isn't writable. Your changes are still here.".to_owned(),
        MakeHudError::NameTaken { name } => {
            format!("You already have a HUD called {name}. Pick another name.")
        }
        MakeHudError::EmptyName => "Give your HUD a name, then save it.".to_owned(),
        MakeHudError::NotFound { .. } => {
            "This HUD was deleted. Save it under a name to keep it as a new one.".to_owned()
        }
        MakeHudError::NotMade { .. } => {
            "This HUD wasn't made in the HUD Maker, so it can't be saved over.".to_owned()
        }
        MakeHudError::NoSpace { needed, .. } => no_space_text("save", *needed),
        MakeHudError::Files(_) => HUD_FILES.to_owned(),
        MakeHudError::Database { .. } => LIBRARY_UPDATE.to_owned(),
        MakeHudError::BaseChanged => HUD_BASE_CHANGED.to_owned(),
        _ => UNKNOWN.to_owned(),
    };
    LocalFailure::new(message, err.to_string())
}

/// A made HUD that failed to reopen in the HUD Maker, in player words.
pub fn describe_hud_load(err: &HudLoadError) -> LocalFailure {
    let message = match err {
        HudLoadError::Damaged { .. } | HudLoadError::Newer | HudLoadError::NotMade { .. } => {
            "This HUD can't be opened for editing. You can still install it."
        }
        HudLoadError::NotFound { .. } => GONE,
        HudLoadError::Files(_) => HUD_FILES,
        HudLoadError::Io { .. } => LIBRARY_DISK,
        HudLoadError::Database { .. } => LIBRARY_OPEN,
        _ => UNKNOWN,
    };
    LocalFailure::new(message, err.to_string())
}

/// The sentence about saved changes that no longer fit, and about a TF2 update since the save;
/// `None` when every change fit.
pub fn left_out(dropped: usize, tf2_updated: bool) -> Option<String> {
    if dropped == 0 {
        return None;
    }
    let text = format!("{dropped} of your changes no longer fit and were left out.");
    Some(match tf2_updated {
        true => format!("{text} Save it again to bring it up to date."),
        false => text,
    })
}

/// The toast after reopening a made HUD.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReopenNotice {
    pub title: &'static str,
    pub text: String,
    /// True when changes were left out; a TF2 update alone is only a note.
    pub warning: bool,
}

/// What reopening a made HUD tells the player: a warning when changes were left out, a note
/// when TF2 updated its files under changes that all still apply, else nothing.
pub fn reopen_notice(dropped: usize, tf2_updated: bool) -> Option<ReopenNotice> {
    if let Some(text) = left_out(dropped, tf2_updated) {
        return Some(ReopenNotice {
            title: "Some changes were left out",
            text,
            warning: true,
        });
    }
    tf2_updated.then(|| ReopenNotice {
        title: "TF2 updated its HUD files",
        text: "Your changes still apply. Check the HUD looks right.".to_owned(),
        warning: false,
    })
}

/// What the player reads once a copy is made: it is theirs to edit, and whose work it is.
pub fn copy_made_text(copy_name: &str, author: &str) -> String {
    let whose = match author {
        "" => "another author's work".to_owned(),
        author => format!("{author}'s work"),
    };
    format!(
        "{copy_name} is yours to edit. It's {whose}: check the HUD's page for its terms before you share yours."
    )
}

/// A refused copy, save or update for want of disk room; `verb` names the action and the
/// size is shown in whole megabytes, rounded up.
pub fn no_space_text(verb: &str, needed_bytes: u64) -> String {
    let mb = needed_bytes.div_ceil(1_000_000);
    format!("There isn't enough free disk space to {verb} this HUD. It needs about {mb} MB.")
}

const HUD_GONE: &str = "This HUD isn't in your library any more.";

/// A failed "Edit a copy" in player words; `detail` is the error's own text.
pub fn describe_copy_hud(err: &CopyHudError) -> LocalFailure {
    let message = match err {
        CopyHudError::NotFound { .. } => HUD_GONE.to_owned(),
        CopyHudError::BaseMissing { .. } => {
            "This HUD's files are missing from your library. Download or import it again."
                .to_owned()
        }
        CopyHudError::MadeHud { .. } => {
            "This HUD was made in the HUD Maker. Open it with Edit instead.".to_owned()
        }
        CopyHudError::NameTaken { .. } => {
            "There are too many copies of this HUD. Rename or delete one, then try again."
                .to_owned()
        }
        CopyHudError::NoSpace { needed, .. } => no_space_text("copy", *needed),
        CopyHudError::Io { .. } => LIBRARY_DISK.to_owned(),
        CopyHudError::Database { .. } => LIBRARY_OPEN.to_owned(),
        _ => UNKNOWN.to_owned(),
    };
    LocalFailure::new(message, err.to_string())
}

/// The line on a copy's card when its base's newer version is only in the HUD Store.
pub fn copy_in_store_text(hud: &str) -> String {
    format!("A newer version of {hud} is in the HUD Store. Download it, then choose Update here.")
}

// At most this many left-out changes are named; the rest are counted.
const NAMED_CHANGES: usize = 5;

// A change's key in player words: what about the part it changed.
fn key_word(key: &str) -> &str {
    match key {
        "xpos" | "ypos" => "position",
        "wide" | "tall" => "size",
        "zpos" => "layer",
        "visible" | "enabled" => "visibility",
        "fgcolor" | "fgcolor_override" | "bgcolor_override" | "drawcolor" => "colour",
        "font" => "font",
        "labelText" => "text",
        other => other,
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
    let lead = format!("{hud} now uses the newest version of its HUD.");
    let (title, mut text, warning) = if left_out.is_empty() {
        let text = format!("{lead} All your changes still apply.");
        ("HUD updated", text, false)
    } else {
        let named: Vec<String> = (left_out.iter().take(NAMED_CHANGES))
            .map(|(part, key)| match key {
                Some(key) => format!("the {} of {part}", key_word(key)),
                None => (*part).to_owned(),
            })
            .collect();
        let rest = left_out.len().saturating_sub(NAMED_CHANGES);
        let mut list = named.join(", ");
        if rest > 0 {
            list = format!("{list} and {rest} more");
        }
        let n = left_out.len();
        let text = format!("{lead} {n} of your changes no longer fit: {list}.");
        ("Some changes were left out", text, true)
    };
    if both_changed > 0 {
        text = format!(
            "{text} {both_changed} of your changes replaced something the update also changed."
        );
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
        UpdateCopyError::NotFound { .. } => HUD_GONE.to_owned(),
        UpdateCopyError::NotACopy { .. } => "Only a copy of a HUD can be updated.".to_owned(),
        UpdateCopyError::SourceGone { .. } => {
            "The newer version isn't in your library any more. Download or import it again."
                .to_owned()
        }
        UpdateCopyError::NotSameSource { .. } | UpdateCopyError::MadeHud { .. } => {
            "That HUD isn't a newer version of this copy's HUD.".to_owned()
        }
        UpdateCopyError::NoSpace { needed, .. } => no_space_text("update", *needed),
        UpdateCopyError::Load(e) => describe_hud_load(e).message,
        UpdateCopyError::Save(e) => describe_make_hud(e).message,
        _ => UNKNOWN.to_owned(),
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
        (PaintError::Io { .. }, PaintStep::Open) => LIBRARY_DISK.to_owned(),
        (PaintError::Io { .. }, _) => "Couldn't save the crosshair: your disk may be full or the folder isn't writable. Your drawing is still here.".to_owned(),
        (PaintError::Database { .. }, PaintStep::SaveNew) => LIBRARY_UPDATE.to_owned(),
        // One sentence true whether the re-save failed before or after its files were written.
        (PaintError::Database { .. }, PaintStep::Resave) => "Couldn't update your crosshair library. Your drawing is still here, so try saving again.".to_owned(),
        (PaintError::Database { .. }, PaintStep::Open) => LIBRARY_OPEN.to_owned(),
        (PaintError::NameTaken { name }, _) => {
            format!("You already have a crosshair called {name}. Pick another name.")
        }
        (PaintError::NotFound { .. }, PaintStep::Open) => GONE.to_owned(),
        (PaintError::NotFound { .. }, _) => {
            "This crosshair was deleted. Save it under a name to keep it as a new one.".to_owned()
        }
        (PaintError::Unreadable { .. }, _) => "This crosshair can't be opened for editing. You can still use it, or delete it and draw a new one.".to_owned(),
        (PaintError::NotPainted { .. }, _) => {
            "This crosshair wasn't drawn in the Crosshair Maker, so it can't be edited here."
                .to_owned()
        }
        (PaintError::EmptyName, _) => "Give your crosshair a name, then save it.".to_owned(),
        _ => UNKNOWN.to_owned(),
    };
    LocalFailure::new(message, err.to_string())
}

/// A failed import of a picked file in player words; `detail` is the error's own text.
pub fn describe_import(err: &ImportError) -> LocalFailure {
    let message = match err {
        ImportError::Io { .. } => {
            "Couldn't read that file or save it to your library. Make sure your disk has free space, then try again."
        }
        ImportError::Archive(ArchiveError::Rar { .. }) => {
            "This file is a RAR archive, which TF2Tools can't open. Pick a ZIP file instead."
        }
        ImportError::Archive(_) => "This archive couldn't be unpacked. Pick another file.",
        ImportError::NotAHud { .. } => "This file doesn't contain a HUD. Pick another file.",
        ImportError::UnsupportedFile { .. }
        | ImportError::NotAVpk { .. }
        | ImportError::UnsupportedProvenance => UNUSABLE,
        ImportError::Wav(WavError::Unsupported { .. }) => {
            "TF2 can't play this sound's format. Save it as a CD-quality stereo WAV, then try again."
        }
        ImportError::Wav(_) => "This sound file couldn't be read. Pick another file.",
        ImportError::Vtf(_) => "This image couldn't be read. Pick another PNG or VTF file.",
        ImportError::Database { .. } => LIBRARY_UPDATE,
        _ => UNKNOWN,
    };
    LocalFailure::new(message, err.to_string())
}

/// A PNG that could not be opened, in player words; `detail` is the error's own text.
pub fn describe_png_read(err: &PngReadError) -> LocalFailure {
    let message = match err {
        PngReadError::Io { .. } => "Couldn't read that file. Pick another file.",
        PngReadError::FileTooLarge { .. } | PngReadError::TooLarge { .. } => {
            "This picture is too big. Pick a smaller one."
        }
        PngReadError::Invalid { .. } => {
            "This file isn't a PNG TF2Tools can read. Pick another file."
        }
        _ => UNKNOWN,
    };
    LocalFailure::new(message, err.to_string())
}

/// A PNG that could not be saved, in player words; `detail` is the error's own text.
pub fn describe_png_write(err: &PngWriteError) -> LocalFailure {
    let message = match err {
        PngWriteError::Io { .. } => {
            "Couldn't save the PNG there. Pick another folder, then try again."
        }
        _ => UNKNOWN,
    };
    LocalFailure::new(message, err.to_string())
}

/// A failed copy into, or removal from, tf/custom; `detail` is the error's own text.
pub fn describe_tf2_install(err: &InstallError) -> LocalFailure {
    let message = match err {
        InstallError::Io { .. } => TF2_WRITE,
        InstallError::NotInLibrary { .. } => GONE,
        InstallError::Database { .. } => LIBRARY_UPDATE,
        InstallError::ExternalHuds(_) => NEW_EXTERNAL_HUD,
        InstallError::GameMissing { .. } => GAME_MOVED,
        _ => UNKNOWN,
    };
    LocalFailure::new(message, err.to_string())
}

/// A failed copy of a HUD into the library; nothing left TF2, and `detail` is the error's own text.
pub fn describe_keep_hud(err: &KeepHudError) -> LocalFailure {
    let message = match err {
        KeepHudError::Io { .. } => {
            "Couldn't copy your HUD into your library, so nothing was removed. Make sure your drive has free space, then try again."
        }
        KeepHudError::NotCopyable { .. } => {
            "Your HUD holds a shortcut TF2Tools can't copy, so nothing was removed."
        }
        KeepHudError::Database { .. } => LIBRARY_UPDATE,
        _ => UNKNOWN,
    };
    LocalFailure::new(message, err.to_string())
}

/// A failed read or save of a profile in player words; `detail` is the error's own text.
pub fn describe_profile(err: &ProfileError) -> LocalFailure {
    let message = match err {
        ProfileError::NotFound { .. } => PROFILE_GONE,
        ProfileError::EmptyName => "Give the profile a name, then try again.",
        ProfileError::NameTaken { name } => {
            let message = crate::pages::profiles::model::taken_message(name);
            return LocalFailure::new(message, err.to_string());
        }
        ProfileError::ItemNotFound { .. } => {
            "An item you picked is no longer in your library. Pick another one, then try again."
        }
        ProfileError::InvalidWeaponScript { .. } => {
            "TF2Tools doesn't know one of this profile's weapons. Edit the profile and try again."
        }
        ProfileError::Library(e) => {
            return LocalFailure::new(describe_library(e).message, err.to_string());
        }
        ProfileError::Database { .. } => PROFILES_OPEN,
        _ => UNKNOWN,
    };
    LocalFailure::new(message, err.to_string())
}

/// A profile activation that failed before any step ran; `detail` is the error's own text.
pub fn describe_apply_profile(err: &ApplyProfileError) -> LocalFailure {
    let message = match err {
        ApplyProfileError::NotFound { .. } => PROFILE_GONE,
        ApplyProfileError::Missing(_) => {
            "Some items in this profile are missing. Download them again, then try again."
        }
        ApplyProfileError::ExternalHuds(_) => NEW_EXTERNAL_HUD,
        ApplyProfileError::Io { .. } => TF2_WRITE,
        ApplyProfileError::Database { .. } => PROFILES_OPEN,
        ApplyProfileError::GameMissing { .. } => GAME_MOVED,
        _ => UNKNOWN,
    };
    LocalFailure::new(message, err.to_string())
}

/// A failed read or save of the settings in player words; `detail` is the error's own text.
pub fn describe_settings(err: &SettingsError) -> LocalFailure {
    let message = match err {
        SettingsError::Database { .. } => {
            "TF2Tools couldn't read or save its settings. Restart TF2Tools and try again."
        }
        SettingsError::NonUtf8Path { .. } => {
            "TF2Tools can't save this folder's name. Pick a TF2 folder whose name uses plain letters."
        }
        _ => UNKNOWN,
    };
    LocalFailure::new(message, err.to_string())
}

/// A TF2 folder the player typed or picked that can't be used; `detail` is the error's own text.
pub fn describe_tf2_folder(err: &DetectError) -> LocalFailure {
    let message = match err {
        DetectError::NotTf2 { .. } => {
            "This folder isn't a TF2 install. Pick the folder TF2 is installed in, then try again."
        }
        DetectError::Io { .. } => {
            "Couldn't set up this TF2 folder. Make sure TF2Tools can write to it, then try again."
        }
        _ => UNKNOWN,
    };
    LocalFailure::new(message, err.to_string())
}

/// A failed auto-detect over the Steam library. Every cause reads the same, since the player's
/// next step is always to pick the folder by hand.
pub fn describe_detect(err: &DetectError) -> LocalFailure {
    LocalFailure::new(
        "Couldn't search your Steam library for TF2. Select your TF2 folder manually.",
        err.to_string(),
    )
}

/// A failed read of the stores' sync status; `detail` is the error's own text.
pub fn describe_store_status(err: &QueryError) -> LocalFailure {
    LocalFailure::new(STORE_READ, err.to_string())
}

/// A failed install of a HUD's fonts in player words; `detail` is the error's own text.
pub fn describe_font_install(err: &FontInstallError) -> LocalFailure {
    let message = match err {
        FontInstallError::Io { .. } => {
            "Couldn't copy the HUD's fonts. Make sure your disk has free space, then try again."
        }
        FontInstallError::Host { .. } | FontInstallError::Register { .. } => {
            "Your system didn't accept the HUD's fonts. Restart TF2Tools and try again."
        }
        FontInstallError::Refresh { .. } => {
            "The fonts were copied. Log out and back in for TF2 to see them."
        }
        _ => UNKNOWN,
    };
    LocalFailure::new(message, err.to_string())
}

/// A sound that couldn't be played in player words; `detail` is the error's own text.
pub fn describe_audio(err: &AudioError) -> LocalFailure {
    let message = match err {
        AudioError::Device(_) => {
            "Couldn't play sound on this computer. Check that your speakers or headphones are connected, then try again."
        }
        AudioError::Decode(_) => "This sound couldn't be played. Pick another sound.",
        AudioError::Closed => {
            "Sound playback stopped working. Restart TF2Tools to play sounds again."
        }
    };
    LocalFailure::new(message, err.to_string())
}

/// A failed read or clear of the activity log in player words; `detail` is the error's own text.
pub fn describe_log(err: &LogError) -> LocalFailure {
    let message = match err {
        LogError::Database { .. } => {
            "TF2Tools couldn't open the activity log. Restart TF2Tools and try again."
        }
        _ => UNKNOWN,
    };
    LocalFailure::new(message, err.to_string())
}

/// Why the app could not start, in player words; `detail` is the error's own text.
pub fn describe_startup(err: &StartupError) -> LocalFailure {
    let message = match err {
        StartupError::NoAssets(_) | StartupError::Catalog { .. } => {
            "TF2Tools is missing some of its own files. Reinstall TF2Tools from the release page."
        }
        // Also covers a failed settings read right after the open.
        StartupError::Database { .. } => {
            "TF2Tools couldn't open your saved library. Restart your computer and try again. If it keeps happening, ask for help on the TF2Tools GitHub page."
        }
        StartupError::NewerDatabase { .. } => {
            "Your library was saved by a newer TF2Tools. Install the latest TF2Tools from the release page to open it."
        }
    };
    LocalFailure::new(message, err.to_string())
}
