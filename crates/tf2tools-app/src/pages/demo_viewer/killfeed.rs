//! The Demo Viewer's kill feed: the rows shown at a tick, their words and their corner.

use gpui_kit::{Pixels, SharedString, Size};
use tf2tools_core::{DemoKill, DemoTracks, KillParty, Team, TfClass, WeaponNames};

use super::BarPlace;
use crate::i18n::tr;

/// How long a kill stays in the feed, in demo seconds.
pub const FEED_HOLD_S: f32 = 6.0;
/// The most rows the feed shows at once.
pub const FEED_ROWS: usize = 5;

/// One side of a feed row: the name it shows and the team that colours it.
#[derive(Debug, Clone, PartialEq)]
pub struct FeedParty {
    pub name: SharedString,
    pub team: Option<Team>,
}

/// One kill as the feed draws it.
#[derive(Debug, Clone, PartialEq)]
pub struct FeedRow {
    pub killer: Option<FeedParty>,
    pub assister: Option<FeedParty>,
    pub victim: FeedParty,
    /// The kill-icon key, as `DemoKill::weapon`.
    pub weapon: Option<String>,
    /// The killer's class at the kill, which `KillIconImages::for_weapon` takes.
    pub killer_class: Option<TfClass>,
}

fn window(tracks: &DemoTracks, tick: f64) -> &[DemoKill] {
    tracks.killfeed_at(tick, FEED_HOLD_S, FEED_ROWS)
}

/// What identifies the feed at `tick` without building rows: its length and its newest kill's
/// tick. The window is contiguous and ends after every kill at or before `tick`, so the pair
/// pins it down.
pub fn feed_key(tracks: &DemoTracks, tick: f64) -> (usize, Option<u32>) {
    let kills = window(tracks, tick);
    (kills.len(), kills.last().map(DemoKill::tick))
}

// A party without a name reads as the players list's numbered fallback.
fn party(p: &KillParty) -> FeedParty {
    let name = match p.name().filter(|n| !n.is_empty()) {
        Some(name) => SharedString::from(name.to_owned()),
        None => tr!("demo_viewer.camera.unnamed", entity = p.id().entity()),
    };
    FeedParty {
        name,
        team: p.team(),
    }
}

/// `killfeed_at(tick, FEED_HOLD_S, FEED_ROWS)` as rows, oldest first; a party without a name
/// reads as `demo_viewer.camera.unnamed` with its entity.
pub fn feed_rows(tracks: &DemoTracks, tick: f64) -> Vec<FeedRow> {
    window(tracks, tick)
        .iter()
        .map(|kill| FeedRow {
            killer: kill.killer().map(party),
            assister: kill.assister().map(party),
            victim: party(kill.victim()),
            weapon: kill.weapon().map(str::to_owned),
            killer_class: kill.killer().and_then(KillParty::class),
        })
        .collect()
}

/// The row's translated accessible name; a weapon reads as `names.get(key)`, else the key.
pub fn feed_words(row: &FeedRow, names: &WeaponNames) -> SharedString {
    let victim = row.victim.name.as_ref();
    let Some(killer) = &row.killer else {
        return tr!("demo_viewer.killfeed.died", victim = victim);
    };
    let weapon: SharedString = match row.weapon.as_deref() {
        Some(key) => names.get(key).unwrap_or(key).to_owned().into(),
        None => tr!("demo_viewer.killfeed.unknown_weapon"),
    };
    let (killer, weapon) = (killer.name.as_ref(), weapon.as_ref());
    match &row.assister {
        Some(assister) => tr!(
            "demo_viewer.killfeed.assisted",
            killer = killer,
            assister = assister.name.as_ref(),
            victim = victim,
            weapon = weapon
        ),
        None => tr!(
            "demo_viewer.killfeed.kill",
            killer = killer,
            victim = victim,
            weapon = weapon
        ),
    }
}

/// Whether the feed sits top right: unless the bar has more room below it than above.
pub fn feed_on_top(place: BarPlace, area: Size<Pixels>, bar: Size<Pixels>) -> bool {
    let bar_at = place.clamp(area, bar).y;
    bar_at >= area.height - bar_at - bar.height
}
