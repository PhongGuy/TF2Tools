//! The public types can cross threads, so the app can run format work off the UI thread.

use tf2tools_core::{
    AddonEntry, AddonName, ApplyReport, ArchiveError, Assignment, BuiltinCrosshair,
    BuiltinCrosshairs, CatalogError, CrosshairEntry, CrosshairFormat, CrosshairId, CrosshairRef,
    FitChange, GbId, HudEntry, HudId, HudStatus, ImportError, InstallError, InstalledCrosshairs,
    InstalledSound, InstalledSounds, Library, LibraryEntry, LibraryError, PaintError, PaintTarget,
    PaintedSource, PngReadError, PngWriteError, Provenance, RgbaImage, SkipReason,
    SkippedAssignment, SoundConflict, SoundEntry, SoundFormat, SoundId, SoundKind, TfClass,
    VpkError, VtfError, WavError, WavInfo, Weapon, WeaponSlot,
};

fn assert_send_sync<T: Send + Sync + std::fmt::Debug + 'static>() {}

#[test]
fn format_types_are_send_sync_debug() {
    assert_send_sync::<RgbaImage>();
    assert_send_sync::<WavInfo>();
    assert_send_sync::<Weapon>();
    assert_send_sync::<TfClass>();
    assert_send_sync::<WeaponSlot>();
    assert_send_sync::<BuiltinCrosshairs>();
    assert_send_sync::<BuiltinCrosshair>();
    assert_send_sync::<ArchiveError>();
    assert_send_sync::<VpkError>();
    assert_send_sync::<WavError>();
    assert_send_sync::<VtfError>();
    assert_send_sync::<CatalogError>();
    assert_send_sync::<PngReadError>();
    assert_send_sync::<PngWriteError>();
    assert_send_sync::<FitChange>();
}

#[test]
fn library_types_are_send_sync_debug() {
    assert_send_sync::<Library>();
    assert_send_sync::<PaintError>();
    assert_send_sync::<PaintTarget>();
    assert_send_sync::<PaintedSource>();
    assert_send_sync::<Provenance>();
    assert_send_sync::<HudId>();
    assert_send_sync::<SoundId>();
    assert_send_sync::<CrosshairId>();
    assert_send_sync::<GbId>();
    assert_send_sync::<HudEntry>();
    assert_send_sync::<HudStatus>();
    assert_send_sync::<SoundEntry>();
    assert_send_sync::<SoundFormat>();
    assert_send_sync::<SoundKind>();
    assert_send_sync::<InstalledSounds>();
    assert_send_sync::<CrosshairEntry>();
    assert_send_sync::<CrosshairRef>();
    assert_send_sync::<Assignment>();
    assert_send_sync::<ApplyReport>();
    assert_send_sync::<InstalledCrosshairs>();
    assert_send_sync::<AddonName>();
    assert_send_sync::<AddonEntry>();
    assert_send_sync::<LibraryEntry>();
    assert_send_sync::<ImportError>();
    assert_send_sync::<InstallError>();
    assert_send_sync::<LibraryError>();
    assert_send_sync::<SkipReason>();
    assert_send_sync::<SkippedAssignment>();
    assert_send_sync::<InstalledSound>();
    assert_send_sync::<SoundConflict>();
    assert_send_sync::<CrosshairFormat>();
}

#[test]
fn profile_types_are_send_sync_debug() {
    use tf2tools_core::{
        Profile, ProfileCrosshair, ProfileCrosshairRef, ProfileError, ProfileId, ProfileItem,
    };
    assert_send_sync::<Profile>();
    assert_send_sync::<ProfileId>();
    assert_send_sync::<ProfileItem>();
    assert_send_sync::<ProfileCrosshair>();
    assert_send_sync::<ProfileCrosshairRef>();
    assert_send_sync::<ProfileError>();
}

#[test]
fn draft_types_are_send_sync_debug() {
    use tf2tools_core::{DraftCrosshair, DraftSlot, HudId, ProfileDraft};
    assert_send_sync::<ProfileDraft>();
    assert_send_sync::<DraftSlot<HudId>>();
    assert_send_sync::<DraftCrosshair>();
}

#[test]
fn apply_types_are_send_sync_debug() {
    use tf2tools_core::{
        ApplyProfileError, ApplyProfileReport, ApplyStep, MissingItem, MissingKind,
    };
    assert_send_sync::<ApplyStep>();
    assert_send_sync::<ApplyProfileReport>();
    assert_send_sync::<ApplyProfileError>();
    assert_send_sync::<MissingItem>();
    assert_send_sync::<MissingKind>();
}

#[test]
fn log_types_are_send_sync_debug() {
    use tf2tools_core::{ActivityLog, LogEntry, LogError, LogLevel, LogQuery, NewLogEntry};
    assert_send_sync::<ActivityLog>();
    assert_send_sync::<LogLevel>();
    assert_send_sync::<NewLogEntry>();
    assert_send_sync::<LogEntry>();
    assert_send_sync::<LogQuery>();
    assert_send_sync::<LogError>();
}

#[test]
fn font_check_types_are_send_sync_debug() {
    assert_send_sync::<tf2tools_core::HudFonts>();
    assert_send_sync::<tf2tools_core::FontCheck>();
    assert_send_sync::<tf2tools_core::FontCheckError>();
    assert_send_sync::<tf2tools_core::FontHostError>();
    assert_send_sync::<tf2tools_core::Platform>();
}

#[test]
fn font_install_types_are_send_sync_debug() {
    assert_send_sync::<tf2tools_core::FontInstallReport>();
    assert_send_sync::<tf2tools_core::FontInstallError>();
    assert_send_sync::<tf2tools_core::FontSkipReason>();
}
