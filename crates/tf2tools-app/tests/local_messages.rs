mod support;

use std::path::PathBuf;

use tf2tools_app::assets::AssetRoot;
use tf2tools_app::audio::AudioError;
use tf2tools_app::local_messages::{
    INTERNAL, LocalFailure, PaintStep, describe_apply_profile, describe_audio, describe_detect,
    describe_font_install, describe_hud_load, describe_import, describe_library, describe_log,
    describe_make_hud, describe_paint, describe_png_read, describe_png_write, describe_profile,
    describe_settings, describe_startup, describe_store_status, describe_tf2_folder,
    describe_tf2_install, left_out, reopen_notice,
};
use tf2tools_app::services::{OpError, StartupError};
use tf2tools_core::{
    ApplyProfileError, ArchiveError, DetectError, FontInstallError, HudLoadError, ImportError,
    InstallError, LibraryError, LogError, MakeHudError, PaintError, PngReadError, PngWriteError,
    ProfileError, QueryError, SettingsError, VpkError, VtfError, WavError, inspect_wav,
};

#[test]
fn a_local_failure_toasts_its_plain_message_under_its_action() {
    let failure = LocalFailure::new("Try again.", "library database error: locked");
    assert_eq!(
        failure.into_op("Failed to load HUDs"),
        OpError::Core {
            action: "Failed to load HUDs",
            message: "Try again.".into()
        }
    );
}

#[test]
fn a_panic_reads_as_player_advice_and_keeps_its_text_for_the_log() {
    assert_eq!(
        INTERNAL,
        "TF2Tools hit a problem it didn't expect. Try again, and restart TF2Tools if it keeps happening."
    );
    let err = OpError::Internal("index out of bounds at src/pages/huds.rs:9".into());
    assert_eq!(err.title(), "Something went wrong");
    assert_eq!(err.description(), INTERNAL);
    assert_eq!(
        err.to_string(),
        "Something went wrong: index out of bounds at src/pages/huds.rs:9"
    );
    support::assert_plain(INTERNAL);
}

const LIBRARY_OPEN: &str = "TF2Tools couldn't open your library. Restart TF2Tools and try again.";
const LIBRARY_UPDATE: &str =
    "TF2Tools couldn't update your library. Restart TF2Tools and try again.";
const GONE: &str = "This item is no longer in your library. Pick another one.";
const UNUSABLE: &str = "TF2Tools can't use this kind of file. Pick another file.";

fn io(text: &str) -> std::io::Error {
    std::io::Error::other(text.to_owned())
}

/// The refusal `inspect_wav` gives a 48 kHz stereo 16-bit PCM file; `WavInfo` has no constructor.
fn wav_48_khz() -> WavError {
    let rate: u32 = 48_000;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&40u32.to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * 4).to_le_bytes());
    bytes.extend_from_slice(&4u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&4u32.to_le_bytes());
    bytes.extend_from_slice(&[0; 4]);
    let err = inspect_wav(&bytes).unwrap_err();
    assert!(matches!(err, WavError::Unsupported { .. }), "{err:?}");
    err
}

fn library_cases() -> Vec<(LibraryError, &'static str)> {
    vec![
        (
            LibraryError::Io {
                path: PathBuf::from("/home/p/.local/share/tf2tools/library/huds/budhud"),
                source: io("Permission denied (os error 13)"),
            },
            "TF2Tools couldn't read or save its files. Make sure your disk has free space and TF2Tools can write to its folder, then try again.",
        ),
        (
            LibraryError::NotFound {
                what: "HUD budhud".into(),
            },
            GONE,
        ),
        (
            LibraryError::InvalidWeaponScript {
                name: "tf_weapon_nope".into(),
            },
            "TF2Tools doesn't know this weapon. Pick another weapon.",
        ),
        (
            LibraryError::Vpk(VpkError::NoWav {
                path: PathBuf::from("/tmp/ding.vpk"),
            }),
            "This sound file couldn't be read. Delete it and add it again.",
        ),
        (
            LibraryError::Vtf(VtfError::Malformed {
                message: "bad header".into(),
            }),
            "This crosshair image couldn't be read. Delete it and add it again.",
        ),
        (
            LibraryError::Database {
                message: "no such table: library_huds".into(),
            },
            LIBRARY_OPEN,
        ),
    ]
}

fn import_cases() -> Vec<(ImportError, &'static str)> {
    vec![
        (
            ImportError::Io {
                path: PathBuf::from("/tmp/hud.zip"),
                source: io("No space left on device (os error 28)"),
            },
            "Couldn't read that file or save it to your library. Make sure your disk has free space, then try again.",
        ),
        (
            ImportError::Archive(ArchiveError::Rar {
                path: PathBuf::from("/tmp/hud.rar"),
            }),
            "This file is a RAR archive, which TF2Tools can't open. Pick a ZIP file instead.",
        ),
        (
            ImportError::Archive(ArchiveError::Malformed {
                path: PathBuf::from("/tmp/hud.zip"),
                message: "invalid Zip archive".into(),
            }),
            "This archive couldn't be unpacked. Pick another file.",
        ),
        (
            ImportError::NotAHud {
                path: PathBuf::from("/tmp/x"),
            },
            "This file doesn't contain a HUD. Pick another file.",
        ),
        (
            ImportError::UnsupportedFile {
                path: PathBuf::from("/tmp/notes.txt"),
            },
            UNUSABLE,
        ),
        (
            ImportError::NotAVpk {
                path: PathBuf::from("/tmp/x.vpk"),
            },
            UNUSABLE,
        ),
        (ImportError::UnsupportedProvenance, UNUSABLE),
        (
            ImportError::Wav(wav_48_khz()),
            "TF2 can't play this sound's format. Save it as a CD-quality stereo WAV, then try again.",
        ),
        (
            ImportError::Wav(WavError::Malformed {
                message: "no fmt chunk".into(),
            }),
            "This sound file couldn't be read. Pick another file.",
        ),
        (
            ImportError::Vtf(VtfError::InvalidPng {
                message: "bad signature".into(),
            }),
            "This image couldn't be read. Pick another PNG or VTF file.",
        ),
        (
            ImportError::Database {
                message: "database is locked".into(),
            },
            LIBRARY_UPDATE,
        ),
    ]
}

fn install_cases() -> Vec<(InstallError, &'static str)> {
    vec![
        (
            InstallError::Io {
                path: PathBuf::from(
                    "/home/p/.steam/steam/steamapps/common/Team Fortress 2/tf/custom/budhud",
                ),
                source: io("Permission denied (os error 13)"),
            },
            "Couldn't change your TF2 folder. Make sure TF2 is closed and TF2Tools can write to it, then try again.",
        ),
        (
            InstallError::NotInLibrary {
                what: "HUD budhud".into(),
            },
            GONE,
        ),
        (
            InstallError::Database {
                message: "database is locked".into(),
            },
            LIBRARY_UPDATE,
        ),
    ]
}

#[test]
fn library_errors_read_as_player_advice() {
    for (err, want) in library_cases() {
        assert_eq!(
            describe_library(&err),
            LocalFailure::new(want, err.to_string()),
            "{err:?}"
        );
    }
    for (err, want) in import_cases() {
        assert_eq!(
            describe_import(&err),
            LocalFailure::new(want, err.to_string()),
            "{err:?}"
        );
    }
    for (err, want) in install_cases() {
        assert_eq!(
            describe_tf2_install(&err),
            LocalFailure::new(want, err.to_string()),
            "{err:?}"
        );
    }
}

#[test]
fn no_library_message_carries_a_path_or_error_text() {
    for (err, _) in library_cases() {
        support::assert_plain(&describe_library(&err).message);
    }
    for (err, _) in import_cases() {
        support::assert_plain(&describe_import(&err).message);
    }
    for (err, _) in install_cases() {
        support::assert_plain(&describe_tf2_install(&err).message);
    }
}

const PROFILE_GONE: &str = "This profile no longer exists. Pick another profile.";
const PROFILES_OPEN: &str = "TF2Tools couldn't open your profiles. Restart TF2Tools and try again.";

fn profile_cases() -> Vec<(ProfileError, &'static str)> {
    vec![
        (
            ProfileError::NotFound {
                id: "comp-1".into(),
            },
            PROFILE_GONE,
        ),
        (
            ProfileError::EmptyName,
            "Give the profile a name, then try again.",
        ),
        (
            ProfileError::ItemNotFound {
                what: "HUD rayshud".into(),
            },
            "An item you picked is no longer in your library. Pick another one, then try again.",
        ),
        (
            ProfileError::InvalidWeaponScript {
                name: "tf_weapon_nope".into(),
            },
            "TF2Tools doesn't know one of this profile's weapons. Edit the profile and try again.",
        ),
        (
            ProfileError::Library(LibraryError::Database {
                message: "disk I/O error".into(),
            }),
            LIBRARY_OPEN,
        ),
        (
            ProfileError::Database {
                message: "no such table: profiles".into(),
            },
            PROFILES_OPEN,
        ),
    ]
}

fn apply_cases() -> Vec<(ApplyProfileError, &'static str)> {
    vec![
        (
            ApplyProfileError::NotFound {
                id: "comp-1".into(),
            },
            PROFILE_GONE,
        ),
        (
            ApplyProfileError::Missing(Vec::new()),
            "Some items in this profile are missing. Download them again, then try again.",
        ),
        (
            ApplyProfileError::Database {
                message: "database is locked".into(),
            },
            PROFILES_OPEN,
        ),
    ]
}

#[test]
fn profile_errors_read_as_player_advice() {
    for (err, want) in profile_cases() {
        assert_eq!(
            describe_profile(&err),
            LocalFailure::new(want, err.to_string()),
            "{err:?}"
        );
    }
    for (err, want) in apply_cases() {
        assert_eq!(
            describe_apply_profile(&err),
            LocalFailure::new(want, err.to_string()),
            "{err:?}"
        );
    }
}

#[test]
fn no_profile_message_carries_a_path_or_error_text() {
    for (err, _) in profile_cases() {
        support::assert_plain(&describe_profile(&err).message);
    }
    for (err, _) in apply_cases() {
        support::assert_plain(&describe_apply_profile(&err).message);
    }
}

const SETTINGS: &str =
    "TF2Tools couldn't read or save its settings. Restart TF2Tools and try again.";
const NOT_TF2: &str =
    "This folder isn't a TF2 install. Pick the folder TF2 is installed in, then try again.";
const DETECT: &str = "Couldn't search your Steam library for TF2. Select your TF2 folder manually.";
const FONTS_REFUSED: &str =
    "Your system didn't accept the HUD's fonts. Restart TF2Tools and try again.";

fn not_tf2() -> DetectError {
    DetectError::NotTf2 {
        path: PathBuf::from("/home/p/games/nowhere"),
    }
}

fn folder_io() -> DetectError {
    DetectError::Io {
        path: PathBuf::from("/home/p/games/tf2/tf/custom"),
        source: io("Read-only file system (os error 30)"),
    }
}

fn font_cases() -> Vec<(FontInstallError, &'static str)> {
    vec![
        (
            FontInstallError::Io {
                path: PathBuf::from("/home/p/.local/share/fonts/x.ttf"),
                source: io("No space left on device (os error 28)"),
            },
            "Couldn't copy the HUD's fonts. Make sure your disk has free space, then try again.",
        ),
        (
            FontInstallError::Host {
                reason: "no XDG_DATA_HOME".into(),
            },
            FONTS_REFUSED,
        ),
        (
            FontInstallError::Register {
                name: "TF2 Build".into(),
                reason: "AddFontResourceW returned 0".into(),
            },
            FONTS_REFUSED,
        ),
        (
            FontInstallError::Refresh {
                reason: "fc-cache exited 1".into(),
            },
            "The fonts were copied. Log out and back in for TF2 to see them.",
        ),
    ]
}

#[test]
fn settings_and_folder_errors_read_as_player_advice() {
    let db = SettingsError::Database {
        message: "database is locked".into(),
    };
    assert_eq!(
        describe_settings(&db),
        LocalFailure::new(SETTINGS, db.to_string())
    );
    let odd = SettingsError::NonUtf8Path {
        path: PathBuf::from("/home/p/games"),
    };
    assert_eq!(
        describe_settings(&odd),
        LocalFailure::new(
            "TF2Tools can't save this folder's name. Pick a TF2 folder whose name uses plain letters.",
            odd.to_string()
        )
    );
    assert_eq!(
        describe_tf2_folder(&not_tf2()),
        LocalFailure::new(NOT_TF2, not_tf2().to_string())
    );
    assert_eq!(
        describe_tf2_folder(&folder_io()),
        LocalFailure::new(
            "Couldn't set up this TF2 folder. Make sure TF2Tools can write to it, then try again.",
            folder_io().to_string()
        )
    );
    for err in [not_tf2(), folder_io()] {
        assert_eq!(
            describe_detect(&err),
            LocalFailure::new(DETECT, err.to_string())
        );
    }
    let status = QueryError::Database {
        message: "no such table: sync_status".into(),
    };
    assert_eq!(
        describe_store_status(&status),
        LocalFailure::new(
            "TF2Tools couldn't read the store. Restart TF2Tools and try again.",
            status.to_string()
        )
    );
    for (err, want) in font_cases() {
        assert_eq!(
            describe_font_install(&err),
            LocalFailure::new(want, err.to_string()),
            "{err:?}"
        );
    }
}

#[test]
fn no_settings_message_carries_a_path_or_error_text() {
    let db = SettingsError::Database {
        message: "database is locked".into(),
    };
    let odd = SettingsError::NonUtf8Path {
        path: PathBuf::from("/home/p/games"),
    };
    support::assert_plain(&describe_settings(&db).message);
    support::assert_plain(&describe_settings(&odd).message);
    for err in [not_tf2(), folder_io()] {
        support::assert_plain(&describe_tf2_folder(&err).message);
        support::assert_plain(&describe_detect(&err).message);
    }
    support::assert_plain(
        &describe_store_status(&QueryError::Database {
            message: "x".into(),
        })
        .message,
    );
    for (err, _) in font_cases() {
        support::assert_plain(&describe_font_install(&err).message);
    }
}

fn audio_cases() -> Vec<(AudioError, &'static str)> {
    vec![
        (
            AudioError::Device("ALSA: no default output device".into()),
            "Couldn't play sound on this computer. Check that your speakers or headphones are connected, then try again.",
        ),
        (
            AudioError::Decode("end of stream at byte 44".into()),
            "This sound couldn't be played. Pick another sound.",
        ),
        (
            AudioError::Closed,
            "Sound playback stopped working. Restart TF2Tools to play sounds again.",
        ),
    ]
}

#[test]
fn audio_and_log_errors_read_as_player_advice() {
    for (err, want) in audio_cases() {
        assert_eq!(
            describe_audio(&err),
            LocalFailure::new(want, err.to_string()),
            "{err:?}"
        );
        support::assert_plain(&describe_audio(&err).message);
    }
    let log = LogError::Database {
        message: "no such table: logs".into(),
    };
    let failure = describe_log(&log);
    assert_eq!(
        failure,
        LocalFailure::new(
            "TF2Tools couldn't open the activity log. Restart TF2Tools and try again.",
            log.to_string()
        )
    );
    support::assert_plain(&failure.message);
}

const ASSETS: &str =
    "TF2Tools is missing some of its own files. Reinstall TF2Tools from the release page.";
const DB_OPEN: &str = "TF2Tools couldn't open your saved library. Restart your computer and try again. If it keeps happening, ask for help on the TF2Tools GitHub page.";
const NEWER: &str = "Your library was saved by a newer TF2Tools. Install the latest TF2Tools from the release page to open it.";

fn startup_cases() -> Vec<(StartupError, &'static str)> {
    let searched = AssetRoot::resolve_from(
        Some(PathBuf::from("/nonexistent/tf2tools-assets")),
        None,
        None,
    )
    .unwrap_err();
    let db = PathBuf::from("/home/p/.local/share/tf2tools/tf2tools.db");
    vec![
        (StartupError::NoAssets(searched), ASSETS),
        (
            StartupError::Catalog {
                path: PathBuf::from("/opt/tf2tools/crosshairs"),
                message: "missing field `name`".into(),
            },
            ASSETS,
        ),
        (
            StartupError::Database {
                path: db.clone(),
                message: "file is not a database".into(),
            },
            DB_OPEN,
        ),
        (
            StartupError::NewerDatabase {
                path: db,
                message: "schema version 99, newer than the supported 10".into(),
            },
            NEWER,
        ),
    ]
}

#[test]
fn startup_errors_read_as_player_advice() {
    for (err, want) in startup_cases() {
        assert_eq!(
            describe_startup(&err),
            LocalFailure::new(want, err.to_string()),
            "{err:?}"
        );
        support::assert_plain(&describe_startup(&err).message);
    }
}

const PAINT_UNKNOWN: &str =
    "Something went wrong. Try again, and restart TF2Tools if it keeps happening.";

#[test]
fn paint_failures_read_as_player_words() {
    let disk = || PaintError::Io {
        path: PathBuf::from("/data/crosshairs/dot/source.png"),
        source: io("No space left on device"),
    };
    let db = || PaintError::Database {
        message: "database is locked".into(),
    };
    let save_disk = "Couldn't save the crosshair: your disk may be full or the folder isn't writable. Your drawing is still here.";
    let deleted = "This crosshair was deleted. Save it under a name to keep it as a new one.";
    let library_disk = "TF2Tools couldn't read or save its files. Make sure your disk has free space and TF2Tools can write to its folder, then try again.";
    let cases: Vec<(PaintError, PaintStep, &str)> = vec![
        (disk(), PaintStep::SaveNew, save_disk),
        (disk(), PaintStep::Resave, save_disk),
        (disk(), PaintStep::Open, library_disk),
        (db(), PaintStep::SaveNew, LIBRARY_UPDATE),
        (
            db(),
            PaintStep::Resave,
            "Couldn't update your crosshair library. Your drawing is still here, so try saving again.",
        ),
        (db(), PaintStep::Open, LIBRARY_OPEN),
        (
            PaintError::NameTaken { name: "Dot".into() },
            PaintStep::SaveNew,
            "You already have a crosshair called Dot. Pick another name.",
        ),
        (
            PaintError::NotFound { id: "ab12".into() },
            PaintStep::Resave,
            deleted,
        ),
        (
            PaintError::NotFound { id: "ab12".into() },
            PaintStep::Open,
            GONE,
        ),
        (
            PaintError::Unreadable { id: "ab12".into() },
            PaintStep::Open,
            "This crosshair can't be opened for editing. You can still use it, or delete it and draw a new one.",
        ),
        (
            PaintError::NotPainted { id: "ab12".into() },
            PaintStep::Open,
            "This crosshair wasn't drawn in the Crosshair Maker, so it can't be edited here.",
        ),
        (
            PaintError::EmptyName,
            PaintStep::SaveNew,
            "Give your crosshair a name, then save it.",
        ),
        (
            PaintError::InvalidDrawing {
                width: 16,
                height: 16,
            },
            PaintStep::SaveNew,
            PAINT_UNKNOWN,
        ),
        (
            PaintError::Vtf(VtfError::Encode {
                message: "too big".into(),
            }),
            PaintStep::SaveNew,
            PAINT_UNKNOWN,
        ),
    ];
    for (err, step, want) in cases {
        let failure = describe_paint(&err, step);
        assert_eq!(failure.message, want, "{err:?} at {step:?}");
        assert_eq!(failure.detail, err.to_string());
        support::assert_plain(&failure.message);
    }
}

#[test]
fn png_read_failures_read_as_player_advice() {
    let cases = [
        (
            PngReadError::Io {
                path: PathBuf::from("/home/p/x.png"),
                source: std::io::Error::other("denied"),
            },
            "Couldn't read that file. Pick another file.",
        ),
        (
            PngReadError::FileTooLarge { bytes: 40_000_000 },
            "This picture is too big. Pick a smaller one.",
        ),
        (
            PngReadError::TooLarge {
                width: 5000,
                height: 10,
            },
            "This picture is too big. Pick a smaller one.",
        ),
        (
            PngReadError::Invalid {
                message: "bad signature".into(),
            },
            "This file isn't a PNG TF2Tools can read. Pick another file.",
        ),
    ];
    for (err, want) in cases {
        let failure = describe_png_read(&err);
        assert_eq!(failure.message, want);
        assert_eq!(failure.detail, err.to_string());
        support::assert_plain(&failure.message);
    }
}

#[test]
fn png_write_failures_read_as_player_advice() {
    let io = PngWriteError::Io {
        path: PathBuf::from("/home/p/x.png"),
        source: std::io::Error::other("denied"),
    };
    let failure = describe_png_write(&io);
    assert_eq!(
        failure.message,
        "Couldn't save the PNG there. Pick another folder, then try again."
    );
    assert_eq!(failure.detail, io.to_string());
    support::assert_plain(&failure.message);
    let encode = PngWriteError::Encode {
        message: "no".into(),
    };
    support::assert_plain(&describe_png_write(&encode).message);
}

#[test]
fn every_hud_maker_save_failure_reads_as_player_words() {
    let errors = [
        MakeHudError::EmptyName,
        MakeHudError::NotMade { id: "x".into() },
        MakeHudError::Files(tf2tools_core::HudFilesError::NoGameInfo {
            path: "/game".into(),
        }),
        MakeHudError::Database {
            message: "locked".into(),
        },
    ];
    let messages: Vec<String> = errors
        .iter()
        .map(|e| describe_make_hud(e).message)
        .collect();
    for message in &messages {
        support::assert_plain(message);
    }
    assert_eq!(messages[0], "Give your HUD a name, then save it.");
    assert_eq!(
        messages[1],
        "This HUD wasn't made in the HUD Maker, so it can't be saved over."
    );
}

#[test]
fn hud_maker_save_failures_read_as_player_words() {
    let io = MakeHudError::Io {
        path: "/x".into(),
        source: std::io::Error::other("full"),
    };
    assert_eq!(
        describe_make_hud(&io).message,
        "Couldn't save the HUD: your disk may be full or the folder isn't writable. Your changes are still here."
    );
    let taken = MakeHudError::NameTaken {
        name: "Mine".into(),
    };
    assert_eq!(
        describe_make_hud(&taken).message,
        "You already have a HUD called Mine. Pick another name."
    );
    let gone = MakeHudError::NotFound { id: "x".into() };
    assert_eq!(
        describe_make_hud(&gone).message,
        "This HUD was deleted. Save it under a name to keep it as a new one."
    );
}

#[test]
fn reopening_warns_only_when_changes_were_left_out() {
    assert_eq!(reopen_notice(0, false), None);
    let updated = reopen_notice(0, true).unwrap();
    assert_eq!(
        (updated.title, updated.text.as_str(), updated.warning),
        (
            "TF2 updated its HUD files",
            "Your changes still apply. Check the HUD looks right.",
            false
        )
    );
    let dropped = reopen_notice(2, true).unwrap();
    assert_eq!(
        (dropped.title, dropped.text.as_str(), dropped.warning),
        (
            "Some changes were left out",
            "2 of your changes no longer fit and were left out. Save it again to bring it up to date.",
            true
        )
    );
}

#[test]
fn left_out_counts_and_mentions_a_tf2_update() {
    assert_eq!(left_out(0, false), None);
    assert_eq!(
        left_out(2, false).as_deref(),
        Some("2 of your changes no longer fit and were left out.")
    );
    assert_eq!(
        left_out(1, true).as_deref(),
        Some(
            "1 of your changes no longer fit and were left out. Save it again to bring it up to date."
        )
    );
    assert_eq!(
        left_out(0, true),
        None,
        "an update alone has its own notice"
    );
    let damaged = HudLoadError::Damaged { reason: "x".into() };
    assert_eq!(
        describe_hud_load(&damaged).message,
        "This HUD can't be opened for editing. You can still install it."
    );
    assert_eq!(
        describe_hud_load(&HudLoadError::Newer).message,
        "This HUD can't be opened for editing. You can still install it."
    );
}

#[test]
fn the_copy_toast_credits_the_author_in_player_words() {
    use tf2tools_app::local_messages::copy_made_text;
    assert_eq!(
        copy_made_text("rayshud (copy)", "raysfire"),
        "rayshud (copy) is yours to edit. It's raysfire's work: check the HUD's page for its terms before you share yours."
    );
    assert!(copy_made_text("x (copy)", "").contains("another author's work"));
}

#[test]
fn every_copy_failure_reads_in_player_words() {
    use tf2tools_app::local_messages::describe_copy_hud;
    use tf2tools_core::CopyHudError as E;
    let says = |e: E| describe_copy_hud(&e).message;
    assert_eq!(
        says(E::NotFound { id: "h".into() }),
        "This HUD isn't in your library any more."
    );
    assert_eq!(
        says(E::BaseMissing { id: "h".into() }),
        "This HUD's files are missing from your library. Download or import it again."
    );
    assert_eq!(
        says(E::MadeHud { id: "h".into() }),
        "This HUD was made in the HUD Maker. Open it with Edit instead."
    );
    assert_eq!(
        says(E::NameTaken { name: "x".into() }),
        "There are too many copies of this HUD. Rename or delete one, then try again."
    );
    assert_eq!(
        says(E::NoSpace {
            needed: 2_000_000,
            free: 0
        }),
        "There isn't enough free disk space to copy this HUD. It needs about 2 MB."
    );
    let io = E::Io {
        path: "/x".into(),
        source: std::io::Error::other("boom"),
    };
    assert!(says(io).starts_with("TF2Tools couldn't read or save its files."));
    assert_eq!(
        says(E::Database {
            message: "m".into()
        }),
        "TF2Tools couldn't open your library. Restart TF2Tools and try again."
    );
}

#[test]
fn a_full_disk_says_how_much_room_is_needed() {
    use tf2tools_app::local_messages::no_space_text;
    assert_eq!(
        no_space_text("copy", 557_000_001),
        "There isn't enough free disk space to copy this HUD. It needs about 558 MB."
    );
}

#[test]
fn an_update_that_keeps_everything_is_a_note() {
    use tf2tools_app::local_messages::copy_updated_notice;
    let n = copy_updated_notice("rayshud (copy)", &[], 0);
    assert_eq!((n.title, n.warning), ("HUD updated", false));
    assert_eq!(
        n.text,
        "rayshud (copy) now uses the newest version of its HUD. All your changes still apply."
    );
}

#[test]
fn left_out_changes_are_named_in_player_words() {
    use tf2tools_app::local_messages::copy_updated_notice;
    let left = [
        ("PlayerStatusHealthValue", Some("fgcolor")),
        ("AmmoInClip", Some("xpos")),
    ];
    let n = copy_updated_notice("rayshud (copy)", &left, 3);
    assert_eq!((n.title, n.warning), ("Some changes were left out", true));
    assert_eq!(
        n.text,
        "rayshud (copy) now uses the newest version of its HUD. 2 of your changes no longer fit: the colour of \
         PlayerStatusHealthValue, the position of AmmoInClip. 3 of your changes replaced something the \
         update also changed."
    );
}

#[test]
fn every_key_word_reads_plainly() {
    use tf2tools_app::local_messages::copy_updated_notice;
    let left = [
        ("A", Some("tall")),
        ("B", Some("zpos")),
        ("C", Some("visible")),
        ("D", Some("font")),
        ("E", Some("labelText")),
    ];
    let n = copy_updated_notice("x", &left, 0);
    assert!(
        n.text.ends_with(
            "the size of A, the layer of B, the visibility of C, the font of D, the text of E."
        ),
        "{}",
        n.text
    );
    let n = copy_updated_notice("x", &[("F", Some("tf2tools_odd"))], 0);
    assert!(
        n.text
            .ends_with("1 of your changes no longer fit: the tf2tools_odd of F."),
        "{}",
        n.text
    );
}

#[test]
fn both_changed_alone_is_a_note() {
    use tf2tools_app::local_messages::copy_updated_notice;
    let n = copy_updated_notice("x", &[], 2);
    assert_eq!((n.title, n.warning), ("HUD updated", false));
    assert_eq!(
        n.text,
        "x now uses the newest version of its HUD. All your changes still apply. 2 of your changes replaced \
         something the update also changed."
    );
}

#[test]
fn past_five_the_rest_are_counted() {
    use tf2tools_app::local_messages::copy_updated_notice;
    let left: Vec<(&str, Option<&str>)> = ["P1", "P2", "P3", "P4", "P5", "P6", "P7"]
        .iter()
        .map(|p| (*p, None))
        .collect();
    let n = copy_updated_notice("x", &left, 0);
    assert!(
        n.text
            .ends_with("7 of your changes no longer fit: P1, P2, P3, P4, P5 and 2 more.")
    );
}

#[test]
fn every_update_failure_reads_in_player_words() {
    use tf2tools_app::local_messages::{copy_in_store_text, describe_update_copy};
    use tf2tools_core::{HudLoadError, UpdateCopyError as E};
    let says = |e: E| describe_update_copy(&e).message;
    assert_eq!(
        says(E::NotFound { id: "h".into() }),
        "This HUD isn't in your library any more."
    );
    assert_eq!(
        says(E::NotACopy { id: "h".into() }),
        "Only a copy of a HUD can be updated."
    );
    assert_eq!(
        says(E::SourceGone { id: "h".into() }),
        "The newer version isn't in your library any more. Download or import it again."
    );
    assert_eq!(
        says(E::NotSameSource { id: "h".into() }),
        "That HUD isn't a newer version of this copy's HUD."
    );
    assert_eq!(
        says(E::MadeHud { id: "h".into() }),
        "That HUD isn't a newer version of this copy's HUD."
    );
    assert_eq!(
        says(E::NoSpace {
            needed: 1_100_000_000,
            free: 0
        }),
        "There isn't enough free disk space to update this HUD. It needs about 1100 MB."
    );
    assert_eq!(
        says(E::Load(HudLoadError::Newer)),
        "This HUD can't be opened for editing. You can still install it."
    );
    assert_eq!(
        copy_in_store_text("rayshud"),
        "A newer version of rayshud is in the HUD Store. Download it, then choose Update here."
    );
}
