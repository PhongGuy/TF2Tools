//! Kills as the parse reads them: raw names and weapon, the parties' team and class at the death.

use std::collections::HashMap;

use crate::hud_maker::Team;
use crate::weapons::TfClass;

/// One side of a kill, as the parse knew the player at the death's tick.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PartyRow {
    pub entity: u32,
    pub name: Option<String>,
    pub team: Option<Team>,
    pub class: Option<TfClass>,
}

/// One kill event, with the raw weapon string the game sent.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct KillRow {
    pub tick: u32,
    pub killer: Option<PartyRow>,
    pub assister: Option<PartyRow>,
    pub victim: PartyRow,
    pub weapon: String,
    pub critical: bool,
    pub feigned: bool,
}

/// The fields of one PlayerDeath event that a kill row reads.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Death {
    pub user_id: u16,
    pub victim_entity: u32,
    pub attacker: u16,
    pub assister: u16,
    pub weapon: String,
    pub crit_type: u16,
    pub death_flags: u16,
}

/// The `crit_type` value the game sends for a full critical hit.
const CRIT_FULL: u16 = 2;
/// The `death_flags` bit set on a Dead Ringer's fake death.
const DEATH_FEIGNED: u16 = 32;
/// The user id the game sends when nobody assisted.
const NO_ASSISTER: u16 = u16::MAX;

/// The kill row for one death, or None when the victim cannot be resolved.
pub(crate) fn kill_row(
    tick: u32,
    death: &Death,
    users: &HashMap<u16, u32>,
    party: impl Fn(u32) -> PartyRow,
) -> Option<KillRow> {
    let victim = match death.victim_entity {
        0 => *users.get(&death.user_id)?,
        entity => entity,
    };
    // The world and suicides have no killer; an assist only counts from a third player.
    let killer = match death.attacker {
        0 => None,
        id if id == death.user_id => None,
        id => users.get(&id).copied(),
    };
    let assister = match death.assister {
        0 | NO_ASSISTER => None,
        id => users
            .get(&id)
            .copied()
            .filter(|&e| e != victim && Some(e) != killer),
    };
    Some(KillRow {
        tick,
        killer: killer.map(&party),
        assister: assister.map(&party),
        victim: party(victim),
        weapon: death.weapon.clone(),
        critical: death.crit_type == CRIT_FULL,
        feigned: death.death_flags & DEATH_FEIGNED != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud_maker::Team;
    use crate::weapons::TfClass;

    fn users() -> HashMap<u16, u32> {
        HashMap::from([(10, 1), (11, 2), (12, 3)])
    }

    fn party(entity: u32) -> PartyRow {
        PartyRow {
            entity,
            name: Some(format!("p{entity}")),
            team: Some(Team::Red),
            class: Some(TfClass::Scout),
        }
    }

    fn death(user_id: u16, attacker: u16, assister: u16) -> Death {
        Death {
            user_id,
            victim_entity: 0,
            attacker,
            assister,
            weapon: "scattergun".into(),
            crit_type: 0,
            death_flags: 0,
        }
    }

    fn entities(row: &KillRow) -> (Option<u32>, Option<u32>, u32) {
        (
            row.killer.as_ref().map(|p| p.entity),
            row.assister.as_ref().map(|p| p.entity),
            row.victim.entity,
        )
    }

    #[test]
    fn a_plain_kill_maps_user_ids_to_entities() {
        let row = kill_row(77, &death(10, 11, 12), &users(), party).expect("kill");
        assert_eq!(entities(&row), (Some(2), Some(3), 1));
        assert_eq!((row.tick, row.weapon.as_str()), (77, "scattergun"));
        assert_eq!(row.killer.expect("killer").name.as_deref(), Some("p2"));
        assert!(!row.critical && !row.feigned);
    }

    #[test]
    fn the_victim_entity_index_wins_over_the_user_id() {
        let mut d = death(10, 11, 0);
        d.victim_entity = 7;
        assert_eq!(
            kill_row(1, &d, &users(), party)
                .expect("kill")
                .victim
                .entity,
            7
        );
    }

    #[test]
    fn world_unknown_and_self_attackers_give_no_killer() {
        for attacker in [0, 99, 10] {
            let row = kill_row(1, &death(10, attacker, 0), &users(), party).expect("kill");
            assert_eq!(entities(&row), (None, None, 1), "{attacker}");
        }
    }

    #[test]
    fn an_unresolvable_victim_drops_the_kill() {
        assert!(kill_row(1, &death(99, 11, 0), &users(), party).is_none());
    }

    #[test]
    fn only_a_third_known_player_assists() {
        for assister in [0, 65_535, 99, 11, 10] {
            let row = kill_row(1, &death(10, 11, assister), &users(), party).expect("kill");
            assert_eq!(row.assister, None, "{assister}");
        }
        let row = kill_row(1, &death(10, 11, 12), &users(), party).expect("kill");
        assert_eq!(row.assister.map(|p| p.entity), Some(3));
    }

    #[test]
    fn crit_type_two_is_critical_and_death_flag_32_is_feigned() {
        let flags = |crit_type, death_flags| {
            let mut d = death(10, 11, 0);
            (d.crit_type, d.death_flags) = (crit_type, death_flags);
            let row = kill_row(1, &d, &users(), party).expect("kill");
            (row.critical, row.feigned)
        };
        assert_eq!(flags(2, 0), (true, false));
        assert_eq!(flags(1, 0), (false, false));
        assert_eq!(flags(0, 32), (false, true));
        assert_eq!(flags(0, 33), (false, true));
        assert_eq!(flags(0, 1), (false, false));
    }
}
