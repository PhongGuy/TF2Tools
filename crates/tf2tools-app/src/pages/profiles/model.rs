//! The Profiles page's pure pieces: badge and step labels, the search rule, the activate
//! toast and log texts, and where a missing item can be downloaded again.

use gpui_kit::SharedString;
use tf2tools_core::{
    ApplyProfileReport, ApplyStep, DraftItem, MissingKind, Profile, ProfileCrosshair, ProfileId,
    ProfileItem, Provenance, SlotMode, SoundId, SoundKind, StoreItemId, StoreKind,
};

use crate::i18n::{names, tr};

/// Shown when a save ends the profile's active state, so the game no longer matches it.
pub fn not_applied() -> SharedString {
    tr!("profiles.not_applied")
}

/// The crosshair badge: "1 crosshair", "N crosshairs".
pub fn crosshair_count(n: usize) -> String {
    crate::i18n::plural("profiles.crosshairs", u64::try_from(n).unwrap_or(u64::MAX)).to_string()
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

/// No HUD, no sounds and no crosshairs.
pub fn is_empty(profile: &Profile) -> bool {
    hud_item(profile).is_none()
        && hit(profile).is_none()
        && kill(profile).is_none()
        && crosshair_list(profile).is_empty()
}

/// Whether a finished load closes the editor open on `open_now`: only when that editor was
/// already open as the load started (`open_at_start`) and the loaded ids lack it.
pub fn load_closes_editor<'a>(
    open_at_start: Option<&ProfileId>,
    open_now: &ProfileId,
    loaded: impl IntoIterator<Item = &'a ProfileId>,
) -> bool {
    open_at_start == Some(open_now) && !loaded.into_iter().any(|id| id == open_now)
}

/// The search rule: the trimmed query, ignoring case, is in the name; blank matches all.
pub fn matches(name: &str, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty() || name.to_lowercase().contains(&query)
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
        ApplyStep::Graphics => tr!("nav.graphics"),
        ApplyStep::Addons => tr!("nav.addons"),
        _ => tr!("profiles.part.item"),
    }
}

/// The Missing Items row's kind: "HUD", "HITSOUND", "KILLSOUND", "CROSSHAIR".
pub fn kind_label(kind: &MissingKind) -> SharedString {
    match kind {
        MissingKind::Hud => "HUD".into(),
        MissingKind::Hitsound => tr!("profiles.missing.kind.hitsound"),
        MissingKind::Killsound => tr!("profiles.missing.kind.killsound"),
        MissingKind::Crosshair { .. } => tr!("profiles.missing.kind.crosshair"),
        MissingKind::Addon { .. } | _ => tr!("profiles.missing.kind.item"),
    }
}

/// The slots the apply installed something into, in step order, as `step_label`s: a slot the
/// profile fills, whose step is in `applied`, and that was not skipped (Crosshairs: at least one
/// crosshair was not skipped).
pub fn installed_parts(profile: &Profile, report: &ApplyProfileReport) -> Vec<SharedString> {
    let skipped = |kind: &MissingKind| report.skipped().iter().any(|m| m.kind() == kind);
    let skipped_crosshairs = report
        .skipped()
        .iter()
        .filter(|m| matches!(m.kind(), MissingKind::Crosshair { .. }))
        .count();
    let filled = [
        (
            ApplyStep::Hud,
            hud_item(profile).is_some() && !skipped(&MissingKind::Hud),
        ),
        (
            ApplyStep::Hitsound,
            hit(profile).is_some() && !skipped(&MissingKind::Hitsound),
        ),
        (
            ApplyStep::Killsound,
            kill(profile).is_some() && !skipped(&MissingKind::Killsound),
        ),
        (
            ApplyStep::Crosshairs,
            crosshair_list(profile).len() > skipped_crosshairs,
        ),
    ];
    filled
        .into_iter()
        .filter(|(step, installed)| *installed && report.applied().contains(step))
        .map(|(step, _)| step_label(step))
        .collect()
}

/// The success toast's title: "activated" when the game now matches the profile, "applied"
/// when missing items were skipped and so the profile is not marked Active.
pub fn success_title(name: &str, skipped: bool) -> String {
    if skipped {
        tr!("profiles.toast.applied", name = name).into()
    } else {
        tr!("profiles.toast.activated", name = name).into()
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

/// The success toast's description: v5's "Installed: ..." or "All customizations cleared";
/// with skipped slots, which ones kept what the player had.
pub fn activated_description(installed: &[SharedString], kept: &[SharedString]) -> String {
    let join = |parts: &[SharedString]| crate::i18n::separated(parts);
    let lead: SharedString = match (installed.is_empty(), kept.is_empty()) {
        (true, true) => return tr!("profiles.toast.cleared").into(),
        (true, false) => tr!("profiles.toast.nothing_new"),
        (false, _) => tr!("profiles.toast.installed", list = join(installed)),
    };
    if kept.is_empty() {
        return lead.into();
    }
    tr!("profiles.toast.kept", lead = lead, list = join(kept)).into()
}

/// The failure toast's description, naming the failed steps only; their raw errors stay in the
/// activate row's details.
pub fn failed_description(failed: &[(ApplyStep, String)]) -> String {
    let steps = failed
        .iter()
        .map(|(step, _)| step_label(*step))
        .collect::<Vec<_>>();
    let steps = crate::i18n::separated(&steps);
    tr!("profiles.toast.failed_steps", steps = steps).into()
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
