//! Every kill of a demo as the killfeed shows it, with names and weapon cleaned for display.

use super::{PlayerId, clean_name};
use crate::demo_view::snapshot::{KillRow, PartyRow};
use crate::hud_maker::Team;
use crate::weapons::TfClass;

/// How many characters a cleaned weapon key keeps.
const WEAPON_KEEP: usize = 32;

/// One side of a kill: the player and their team and class at the death's tick.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct KillParty {
    id: PlayerId,
    name: Option<String>,
    team: Option<Team>,
    class: Option<TfClass>,
}

impl KillParty {
    /// The player's id in the demo.
    pub fn id(&self) -> PlayerId {
        self.id
    }

    /// The player's cleaned name, if the demo gave one.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// The player's side at the kill, if known.
    pub fn team(&self) -> Option<Team> {
        self.team
    }

    /// The player's class at the kill, if known.
    pub fn class(&self) -> Option<TfClass> {
        self.class
    }

    fn heap_bytes(&self) -> usize {
        self.name.as_ref().map_or(0, String::capacity)
    }
}

/// One kill as the killfeed draws it.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct DemoKill {
    tick: u32,
    killer: Option<KillParty>,
    assister: Option<KillParty>,
    victim: KillParty,
    weapon: Option<String>,
    critical: bool,
    feigned: bool,
}

impl DemoKill {
    /// The demo tick of the death.
    pub fn tick(&self) -> u32 {
        self.tick
    }

    /// Who got the kill; None for a death to the world.
    pub fn killer(&self) -> Option<&KillParty> {
        self.killer.as_ref()
    }

    /// Who assisted, if anyone.
    pub fn assister(&self) -> Option<&KillParty> {
        self.assister.as_ref()
    }

    /// Who died.
    pub fn victim(&self) -> &KillParty {
        &self.victim
    }

    /// The weapon's killfeed icon key: lowercase `[a-z0-9_]`, at most 32 characters.
    pub fn weapon(&self) -> Option<&str> {
        self.weapon.as_deref()
    }

    /// Whether the kill was a critical hit.
    pub fn critical(&self) -> bool {
        self.critical
    }

    /// Whether the death was a spy's feign.
    pub fn feigned(&self) -> bool {
        self.feigned
    }

    /// The heap this kill's strings hold.
    pub(crate) fn heap_bytes(&self) -> usize {
        self.killer.as_ref().map_or(0, KillParty::heap_bytes)
            + self.assister.as_ref().map_or(0, KillParty::heap_bytes)
            + self.victim.heap_bytes()
            + self.weapon.as_ref().map_or(0, String::capacity)
    }
}

/// `raw` as an icon key: ASCII capitals lowered, anything outside `[a-z0-9_]` dropped, cut to
/// 32 characters; None when nothing is left.
pub(crate) fn clean_weapon(raw: &str) -> Option<String> {
    let key: String = raw
        .chars()
        .map(|c| c.to_ascii_lowercase())
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_')
        .take(WEAPON_KEEP)
        .collect();
    (!key.is_empty()).then_some(key)
}

impl From<&PartyRow> for KillParty {
    fn from(row: &PartyRow) -> Self {
        Self {
            id: PlayerId(row.entity),
            name: row.name.as_deref().and_then(clean_name),
            team: row.team,
            class: row.class,
        }
    }
}

impl From<&KillRow> for DemoKill {
    fn from(row: &KillRow) -> Self {
        Self {
            tick: row.tick,
            killer: row.killer.as_ref().map(KillParty::from),
            assister: row.assister.as_ref().map(KillParty::from),
            victim: KillParty::from(&row.victim),
            weapon: clean_weapon(&row.weapon),
            critical: row.critical,
            feigned: row.feigned,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo_view::snapshot::{KillRow, PartyRow, Step, TickFrame};
    use crate::demo_view::test_support::{SPT, row};
    use crate::demo_view::tracks::build::{Caps, TrackBuilder};
    use crate::demo_view::tracks::{DemoTracks, MAX_KILLS, TrackCut};
    use crate::hud_maker::Team;
    use crate::weapons::TfClass;

    fn party(entity: u32, name: &str) -> PartyRow {
        PartyRow {
            entity,
            name: Some(name.into()),
            team: Some(Team::Blue),
            class: Some(TfClass::Sniper),
        }
    }

    fn kill(tick: u32, victim: u32) -> KillRow {
        KillRow {
            tick,
            killer: Some(party(2, "killer")),
            assister: None,
            victim: party(victim, "victim"),
            weapon: "sniperrifle".into(),
            critical: true,
            feigned: false,
        }
    }

    fn frame(tick: u32, kills: Vec<KillRow>) -> TickFrame {
        let mut f = TickFrame {
            tick,
            round_state: 4,
            rows: vec![row(1, [10., 10., 10.], 0.)],
            ..Default::default()
        };
        f.kills = kills;
        f
    }

    fn build(frames: &[TickFrame]) -> DemoTracks {
        let mut builder = TrackBuilder::with_caps(Caps::default());
        for f in frames {
            builder.push(f, SPT);
        }
        builder.finish(SPT, 0, false).expect("tracks")
    }

    fn ticks(t: &DemoTracks) -> Vec<u32> {
        t.kills().iter().map(DemoKill::tick).collect()
    }

    #[test]
    fn the_kill_cap_defaults_to_max_kills() {
        assert_eq!(Caps::default().kills, MAX_KILLS);
        assert_eq!(MAX_KILLS, 20_000);
    }

    #[test]
    fn weapons_clean_to_their_icon_key() {
        assert_eq!(clean_weapon("sniperrifle").as_deref(), Some("sniperrifle"));
        assert_eq!(
            clean_weapon("Tf_Projectile Rocket/../\u{e9}").as_deref(),
            Some("tf_projectilerocket")
        );
        assert_eq!(
            clean_weapon(&"a".repeat(200)).as_deref(),
            Some(&*"a".repeat(32))
        );
        assert_eq!(clean_weapon("!!! \u{202e}"), None);
        assert_eq!(clean_weapon(""), None);
    }

    #[test]
    fn kills_come_through_with_clean_names_and_their_flags() {
        let mut k = kill(5, 3);
        k.victim.name = Some("\u{202e}evil\u{7}".into());
        k.weapon = "SniperRifle".into();
        let t = build(&[frame(5, vec![k])]);
        assert_eq!(t.kill_count(), 1);
        let got = &t.kills()[0];
        assert_eq!(got.victim().name(), Some("evil"));
        assert_eq!(got.victim().id().entity(), 3);
        assert_eq!(
            (got.victim().team(), got.victim().class()),
            (Some(Team::Blue), Some(TfClass::Sniper))
        );
        assert_eq!(got.killer().and_then(KillParty::name), Some("killer"));
        assert_eq!(got.assister(), None);
        assert_eq!(got.weapon(), Some("sniperrifle"));
        assert!(got.critical() && !got.feigned());
    }

    #[test]
    fn a_non_advancing_frame_keeps_its_kills_and_kills_stay_sorted() {
        let frames: Vec<_> = [0, 1, 2, 1]
            .iter()
            .map(|&t| frame(t, vec![kill(t, 3)]))
            .collect();
        assert_eq!(ticks(&build(&frames)), [0, 1, 1, 2]);
    }

    #[test]
    fn same_tick_kills_keep_event_order() {
        let t = build(&[frame(5, vec![kill(5, 8), kill(5, 9)])]);
        let victims: Vec<u32> = t.kills().iter().map(|k| k.victim().id().entity()).collect();
        assert_eq!(victims, [8, 9]);
    }

    #[test]
    fn a_restart_drops_the_opening_kills() {
        let mut frames = vec![frame(90_000, vec![kill(90_000, 3)])];
        frames.extend((0..3).map(|t| frame(t, if t == 1 { vec![kill(1, 4)] } else { vec![] })));
        assert_eq!(ticks(&build(&frames)), [1]);
    }

    #[test]
    fn the_kill_cap_refuses_the_whole_frame() {
        let mut builder = TrackBuilder::with_caps(Caps {
            kills: 2,
            ..Caps::default()
        });
        let steps: Vec<Step> = (10..13)
            .map(|t| builder.push(&frame(t, vec![kill(t, 3)]), SPT))
            .collect();
        assert_eq!(steps, [Step::Continue, Step::Continue, Step::Done]);
        let t = builder.finish(SPT, 0, false).expect("tracks");
        assert_eq!(t.cut(), Some(TrackCut::SampleCap { at_tick: 11 }));
        assert_eq!((t.kill_count(), t.last_tick()), (2, 11));
    }

    #[test]
    fn heap_bytes_counts_the_kills() {
        let with = build(&[frame(0, vec![kill(0, 3)])]);
        let without = build(&[frame(0, vec![])]);
        assert!(with.heap_bytes() > without.heap_bytes() + std::mem::size_of::<DemoKill>() - 1);
    }

    #[test]
    fn a_non_advancing_frame_past_the_kill_cap_ends_the_timeline() {
        let mut builder = TrackBuilder::with_caps(Caps {
            kills: 1,
            ..Caps::default()
        });
        assert_eq!(
            builder.push(&frame(10, vec![kill(10, 3)]), SPT),
            Step::Continue
        );
        assert_eq!(builder.push(&frame(10, vec![kill(10, 4)]), SPT), Step::Done);
        let t = builder.finish(SPT, 0, false).expect("tracks");
        assert_eq!(t.cut(), Some(TrackCut::SampleCap { at_tick: 10 }));
        assert_eq!(t.kill_count(), 1);
    }
}
