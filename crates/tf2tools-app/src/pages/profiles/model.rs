//! The Profiles page's pure pieces: badge and step labels, the search rule, the activate
//! toast and log texts, and where a missing item can be downloaded again.

use gpui_kit::SharedString;
use tf2tools_core::{
    ApplySkipReason, ApplyStep, DraftItem, MissingKind, Profile, ProfileCrosshair, ProfileId,
    ProfileItem, Provenance, SlotMode, SoundId, SoundKind, StoreItemId, StoreKind,
};

use crate::i18n::{names, tr};
use crate::shell::toast::ToastKind;

/// Shown when a save ends the profile's active state, so the game no longer matches it.
pub fn not_applied() -> SharedString {
    tr!("profiles.not_applied")
}

/// The HUD the profile sets, if its HUD slot is Set.
pub(crate) fn hud_item(profile: &Profile) -> Option<&ProfileItem> {
    profile.hud().as_set()
}

/// The hitsound the profile sets; `None` when its Sounds slot is not Set or keeps TF2's own.
pub(crate) fn hit(profile: &Profile) -> Option<&ProfileItem> {
    profile.sounds().as_set().and_then(|s| s.hit.as_ref())
}

/// The killsound the profile sets; `None` when its Sounds slot is not Set or keeps TF2's own.
pub(crate) fn kill(profile: &Profile) -> Option<&ProfileItem> {
    profile.sounds().as_set().and_then(|s| s.kill.as_ref())
}

/// The sound of `kind` the profile sets.
pub(crate) fn sound(profile: &Profile, kind: SoundKind) -> Option<&ProfileItem> {
    match kind {
        SoundKind::Hitsound => hit(profile),
        SoundKind::Killsound | _ => kill(profile),
    }
}

/// The crosshairs the profile assigns; empty unless its Crosshairs slot is Set.
pub(crate) fn crosshair_list(profile: &Profile) -> &[ProfileCrosshair] {
    profile.crosshairs().as_set().map_or(&[][..], Vec::as_slice)
}

/// A draft's hitsound and killsound; `None` is TF2's default.
type DraftSounds = (Option<DraftItem<SoundId>>, Option<DraftItem<SoundId>>);

/// The Sounds slot of a draft of `saved` whose `kind` side becomes `side`. The other side
/// is the saved sound, kept, or TF2's default when the slot was not Set.
pub(crate) fn sounds_with(
    saved: &Profile,
    kind: SoundKind,
    side: Option<DraftItem<SoundId>>,
) -> SlotMode<DraftSounds> {
    let keep = |item: Option<&ProfileItem>| item.cloned().map(DraftItem::Keep);
    SlotMode::Set(match kind {
        SoundKind::Hitsound => (side, keep(kill(saved))),
        SoundKind::Killsound | _ => (keep(hit(saved)), side),
    })
}

/// The profile among `profiles`, other than `except`, that already has `name` (trimmed, any case).
pub fn taken_by<'a>(
    profiles: &'a [Profile],
    name: &str,
    except: Option<&ProfileId>,
) -> Option<&'a Profile> {
    let name = name.trim().to_lowercase();
    if name.is_empty() {
        return None;
    }
    profiles
        .iter()
        .find(|p| Some(p.id()) != except && p.name().to_lowercase() == name)
}

/// The line under a name field holding another profile's name.
pub fn taken_message(name: &str) -> String {
    crate::i18n::tr!("messages.profiles.name_taken", name = name).into()
}

/// "HUD", "Hitsound", "Killsound", "Crosshairs", "Graphics", "Add-ons".
pub fn step_label(step: ApplyStep) -> SharedString {
    match step {
        ApplyStep::Hud => "HUD".into(),
        ApplyStep::Hitsound => names::sound_kind_name(SoundKind::Hitsound),
        ApplyStep::Killsound => names::sound_kind_name(SoundKind::Killsound),
        ApplyStep::Crosshairs => tr!("profiles.part.crosshairs"),
        ApplyStep::Graphics => tr!("profiles.part.graphics"),
        ApplyStep::Addons => tr!("profiles.part.addons"),
        _ => tr!("profiles.part.item"),
    }
}

/// The slots whose missing items were skipped, once each in step order, as `step_label`s.
pub fn kept_parts(kinds: &[MissingKind]) -> Vec<SharedString> {
    let crosshair = |k: &MissingKind| matches!(k, MissingKind::Crosshair { .. });
    [
        (ApplyStep::Hud, kinds.contains(&MissingKind::Hud)),
        (ApplyStep::Hitsound, kinds.contains(&MissingKind::Hitsound)),
        (
            ApplyStep::Killsound,
            kinds.contains(&MissingKind::Killsound),
        ),
        (ApplyStep::Crosshairs, kinds.iter().any(crosshair)),
    ]
    .into_iter()
    .filter(|(_, kept)| *kept)
    .map(|(step, _)| step_label(step))
    .collect()
}

/// The failure toast's description: the HUD, sound and crosshair failures as one sentence
/// naming their steps (raw errors stay in the activate row's details), then each Graphics or
/// Add-ons failure as "<label>: <reason>.", since those reasons tell the player what to do.
pub fn failed_description(failed: &[(ApplyStep, String)]) -> String {
    let explained = |step: &ApplyStep| matches!(step, ApplyStep::Graphics | ApplyStep::Addons);
    let steps = failed
        .iter()
        .filter(|(step, _)| !explained(step))
        .map(|(step, _)| step_label(*step))
        .collect::<Vec<_>>();
    let mut sentences = Vec::new();
    if !steps.is_empty() {
        let steps = crate::i18n::separated(&steps);
        sentences.push(tr!("profiles.toast.failed_steps", steps = steps).to_string());
    }
    sentences.extend(
        failed
            .iter()
            .filter(|(step, _)| explained(step))
            .map(|(step, reason)| crate::i18n::labelled(step_label(*step), reason)),
    );
    sentences_text(sentences)
}

/// What one Apply did, gathered from the core report and the modules phase, for its toast.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ApplyOutcome {
    pub name: String,
    /// `mark_profile_active` returned true, so the game now matches the saved profile.
    pub active: bool,
    /// Core failures, then the modules phase's.
    pub failed: Vec<(ApplyStep, String)>,
    pub skipped_steps: Vec<(ApplyStep, ApplySkipReason)>,
    /// `kept_parts` of the skipped missing items.
    pub kept: Vec<SharedString>,
    /// The folder that kept the HUD out of its place.
    pub blocked: Option<String>,
    pub movement: Option<String>,
    /// The external-HUD note.
    pub note: Option<String>,
}

/// (kind, title, description) of the one toast an Apply ends with. Any failure makes it an
/// error; otherwise every caveat is a sentence in the description and turns it into a warning,
/// as does a profile left inactive.
pub fn outcome_toast(o: &ApplyOutcome) -> (ToastKind, String, String) {
    if !o.failed.is_empty() {
        let title = tr!("profiles.toast.apply_failed", name = o.name.as_str()).to_string();
        return (ToastKind::Error, title, failed_description(&o.failed));
    }
    let mut sentences = Vec::new();
    if !o.kept.is_empty() {
        let list = crate::i18n::separated(&o.kept);
        sentences.push(tr!("profiles.toast.kept_only", list = list).to_string());
    }
    sentences.extend(o.skipped_steps.iter().map(|(step, reason)| {
        let (part, reason) = (step_label(*step), skip_reason(*step, *reason));
        tr!("profiles.toast.skipped", part = part, reason = reason).to_string()
    }));
    sentences.extend(
        o.blocked
            .as_deref()
            .map(crate::pages::huds::blocked_description),
    );
    sentences.extend(o.movement.iter().chain(&o.note).cloned());
    let description = sentences_text(sentences);
    let title = tr!("profiles.toast.applied_line", name = o.name.as_str()).to_string();
    let kind = if o.active && description.is_empty() {
        ToastKind::Success
    } else {
        ToastKind::Warning
    };
    (kind, title, description)
}

/// Why `step` was left as it is: Add-ons only ever wait on mastercomfig being installed.
pub fn skip_reason(step: ApplyStep, reason: ApplySkipReason) -> SharedString {
    match (step, reason) {
        (ApplyStep::Addons, _) => tr!("profiles.skip.addons_need_base"),
        (_, ApplySkipReason::MastercomfigNotManaged) => tr!("profiles.skip.not_managed"),
        (_, ApplySkipReason::MastercomfigNotInstalled | _) => {
            tr!("profiles.skip.graphics_not_installed")
        }
    }
}

/// The modules phase's failure when the mastercomfig settings catalogue was never read.
pub fn modules_no_catalog() -> SharedString {
    tr!("profiles.modules.no_catalog")
}

/// The modules phase's failure when modules.cfg belongs to the player, not TF2Tools.
pub fn modules_foreign() -> SharedString {
    tr!("profiles.modules.foreign")
}

/// Joins `sentences` with one space, giving each a closing full stop it lacks.
fn sentences_text(sentences: Vec<String>) -> String {
    sentences
        .into_iter()
        .map(|mut s| {
            if !s.ends_with(['.', '!', '?', '\u{3002}']) {
                s.push('.');
            }
            s
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The activate row's details: "Applied: A, B; Skipped: x, y; Failed: C (error)", empty parts
/// left out, "Applied: nothing" when all three are empty.
pub fn report_details(
    applied: &[ApplyStep],
    skipped: &[&str],
    failed: &[(ApplyStep, String)],
) -> String {
    let mut parts = Vec::new();
    if !applied.is_empty() {
        let steps: Vec<_> = applied.iter().map(|s| step_label(*s)).collect();
        parts.push(tr!(
            "profiles.activity.details.applied",
            list = crate::i18n::separated(&steps)
        ));
    }
    if !skipped.is_empty() {
        let list = crate::i18n::separated(skipped);
        parts.push(tr!("profiles.activity.details.skipped", list = list));
    }
    if !failed.is_empty() {
        let steps: Vec<_> = failed
            .iter()
            .map(|(step, err)| format!("{} ({err})", step_label(*step)))
            .collect();
        parts.push(tr!(
            "profiles.activity.details.failed",
            list = crate::i18n::separated(&steps)
        ));
    }
    if parts.is_empty() {
        tr!("profiles.activity.details.nothing").into()
    } else {
        parts
            .iter()
            .map(|p| p.as_ref())
            .collect::<Vec<_>>()
            .join("; ")
    }
}

/// Where a missing item can be downloaded again: a curated HUD, a GameBanana HUD, sound or
/// crosshair; `None` for a local import.
pub fn redownload_target(
    kind: &MissingKind,
    provenance: &Provenance,
) -> Option<(StoreKind, StoreItemId)> {
    match provenance {
        Provenance::Curated { hud_id } if *kind == MissingKind::Hud => {
            Some((StoreKind::CuratedHuds, StoreItemId::Curated(hud_id.clone())))
        }
        Provenance::GameBanana { gb_id, .. } => {
            let store = match kind {
                MissingKind::Hud => StoreKind::GameBananaHuds,
                MissingKind::Hitsound | MissingKind::Killsound => StoreKind::Sounds,
                MissingKind::Crosshair { .. } => StoreKind::Crosshairs,
                MissingKind::Addon { .. } | _ => return None,
            };
            Some((store, StoreItemId::GameBanana(*gb_id)))
        }
        Provenance::Local | Provenance::Curated { .. } | _ => None,
    }
}
