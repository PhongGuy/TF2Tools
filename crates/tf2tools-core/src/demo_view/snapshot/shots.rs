//! Follows projectile entities through a parse, one serial per projectile life.

use std::collections::BTreeMap;

use crate::demo_view::tracks::ProjectileKind;
use crate::hud_maker::Team;

/// One live projectile at one tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ProjectileRow {
    pub serial: u32,
    pub kind: ProjectileKind,
    pub team: Option<Team>,
    pub critical: bool,
    pub origin: Option<[f32; 3]>,
}

/// One prop of a projectile update that the viewer reads.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ShotProp {
    Origin([f32; 3]),
    Team(i64),
    Critical(bool),
    PipeType(i64),
}

/// Whether entities of server class `class` are projectiles in flight.
pub(crate) fn is_projectile(class: &str) -> bool {
    class.starts_with("CTFProjectile_")
        || matches!(
            class,
            "CTFGrenadePipebombProjectile" | "CTFStunBall" | "CTFBall_Ornament"
        )
}

/// The kind of a projectile of server class `class`; `pipe_type` only matters to pipebombs.
pub(crate) fn kind_of(class: &str, pipe_type: Option<i64>) -> ProjectileKind {
    match class {
        "CTFGrenadePipebombProjectile" => match pipe_type {
            Some(1 | 2) => ProjectileKind::Sticky,
            _ => ProjectileKind::Pipe,
        },
        "CTFProjectile_Rocket" | "CTFProjectile_SentryRocket" => ProjectileKind::Rocket,
        "CTFProjectile_Arrow" | "CTFProjectile_HealingBolt" => ProjectileKind::Arrow,
        "CTFProjectile_Flare" => ProjectileKind::Flare,
        "CTFProjectile_EnergyBall" => ProjectileKind::EnergyBall,
        "CTFProjectile_EnergyRing" => ProjectileKind::EnergyRing,
        "CTFProjectile_Jar"
        | "CTFProjectile_JarMilk"
        | "CTFProjectile_JarGas"
        | "CTFProjectile_Cleaver"
        | "CTFStunBall"
        | "CTFBall_Ornament" => ProjectileKind::Thrown,
        _ => ProjectileKind::Other,
    }
}

// One live projectile: its row, plus the class and pipe type its kind is derived from.
struct Shot {
    row: ProjectileRow,
    class: String,
    pipe_type: Option<i64>,
}

/// The live projectiles of a parse, by entity slot.
#[derive(Default)]
pub(crate) struct Shots {
    live: BTreeMap<u32, Shot>,
    last_serial: u32,
}

impl Shots {
    /// An update to projectile entity `entity` of server class `class`; `enter` is an Enter.
    pub fn update(
        &mut self,
        entity: u32,
        class: &str,
        enter: bool,
        props: impl IntoIterator<Item = ShotProp>,
    ) {
        // An Enter, a first sighting or a slot that changed class is a new life with blank props.
        let fresh = enter
            || self
                .live
                .get(&entity)
                .is_none_or(|shot| shot.class != class);
        if fresh {
            self.last_serial = self.last_serial.saturating_add(1);
            let row = ProjectileRow {
                serial: self.last_serial,
                kind: kind_of(class, None),
                team: None,
                critical: false,
                origin: None,
            };
            let shot = Shot {
                row,
                class: class.to_owned(),
                pipe_type: None,
            };
            self.live.insert(entity, shot);
        }
        let Some(shot) = self.live.get_mut(&entity) else {
            return;
        };
        for prop in props {
            match prop {
                ShotProp::Origin(origin) => shot.row.origin = Some(origin),
                ShotProp::Team(number) => shot.row.team = team(number),
                ShotProp::Critical(critical) => shot.row.critical = critical,
                ShotProp::PipeType(pipe) => shot.pipe_type = Some(pipe),
            }
        }
        shot.row.kind = kind_of(&shot.class, shot.pipe_type);
    }

    /// The shot on `entity`, if any, is gone.
    pub fn end(&mut self, entity: u32) {
        self.live.remove(&entity);
    }

    /// Whether no shot is live.
    pub fn is_empty(&self) -> bool {
        self.live.is_empty()
    }

    /// Every live shot, in entity order.
    pub fn rows(&self) -> Vec<ProjectileRow> {
        self.live.values().map(|shot| shot.row).collect()
    }
}

fn team(number: i64) -> Option<Team> {
    match number {
        2 => Some(Team::Red),
        3 => Some(Team::Blue),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo_view::tracks::ProjectileKind;
    use crate::hud_maker::Team;

    const O: [f32; 3] = [100., 200., 300.];

    #[test]
    fn classes_map_to_kinds() {
        let cases = [
            ("CTFProjectile_Rocket", None, ProjectileKind::Rocket),
            ("CTFProjectile_SentryRocket", None, ProjectileKind::Rocket),
            ("CTFGrenadePipebombProjectile", None, ProjectileKind::Pipe),
            (
                "CTFGrenadePipebombProjectile",
                Some(0),
                ProjectileKind::Pipe,
            ),
            (
                "CTFGrenadePipebombProjectile",
                Some(1),
                ProjectileKind::Sticky,
            ),
            (
                "CTFGrenadePipebombProjectile",
                Some(2),
                ProjectileKind::Sticky,
            ),
            (
                "CTFGrenadePipebombProjectile",
                Some(3),
                ProjectileKind::Pipe,
            ),
            ("CTFProjectile_Arrow", None, ProjectileKind::Arrow),
            ("CTFProjectile_HealingBolt", None, ProjectileKind::Arrow),
            ("CTFProjectile_Flare", None, ProjectileKind::Flare),
            ("CTFProjectile_EnergyBall", None, ProjectileKind::EnergyBall),
            ("CTFProjectile_EnergyRing", None, ProjectileKind::EnergyRing),
            ("CTFProjectile_Jar", None, ProjectileKind::Thrown),
            ("CTFProjectile_JarMilk", None, ProjectileKind::Thrown),
            ("CTFProjectile_JarGas", None, ProjectileKind::Thrown),
            ("CTFProjectile_Cleaver", None, ProjectileKind::Thrown),
            ("CTFStunBall", None, ProjectileKind::Thrown),
            ("CTFBall_Ornament", None, ProjectileKind::Thrown),
            ("CTFProjectile_BallOfFire", None, ProjectileKind::Other),
        ];
        for (class, pipe, kind) in cases {
            assert!(is_projectile(class), "{class}");
            assert_eq!(kind_of(class, pipe), kind, "{class} {pipe:?}");
        }
        for class in [
            "CTFPlayer",
            "CTFRocketLauncher",
            "CObjectSentrygun",
            "CTFProjectile",
        ] {
            assert!(!is_projectile(class), "{class}");
        }
    }

    #[test]
    fn a_shot_carries_its_props_and_has_no_origin_until_one_arrives() {
        let mut shots = Shots::default();
        shots.update(40, "CTFProjectile_Rocket", true, [ShotProp::Team(3)]);
        assert_eq!(shots.rows()[0].origin, None);
        shots.update(
            40,
            "CTFProjectile_Rocket",
            false,
            [ShotProp::Origin(O), ShotProp::Critical(true)],
        );
        let rows = shots.rows();
        assert_eq!(rows.len(), 1);
        let r = rows[0];
        assert_eq!(
            (r.serial, r.kind, r.team, r.critical, r.origin),
            (1, ProjectileKind::Rocket, Some(Team::Blue), true, Some(O))
        );
    }

    #[test]
    fn team_numbers_other_than_two_and_three_are_none() {
        let mut shots = Shots::default();
        shots.update(1, "CTFProjectile_Flare", true, [ShotProp::Team(2)]);
        shots.update(2, "CTFProjectile_Flare", true, [ShotProp::Team(1)]);
        shots.update(3, "CTFProjectile_Flare", true, []);
        let teams: Vec<Option<Team>> = shots.rows().iter().map(|r| r.team).collect();
        assert_eq!(teams, [Some(Team::Red), None, None]);
    }

    #[test]
    fn a_pipe_type_turns_a_pipebomb_into_a_sticky() {
        let mut shots = Shots::default();
        shots.update(
            9,
            "CTFGrenadePipebombProjectile",
            true,
            [ShotProp::Origin(O)],
        );
        assert_eq!(shots.rows()[0].kind, ProjectileKind::Pipe);
        shots.update(
            9,
            "CTFGrenadePipebombProjectile",
            false,
            [ShotProp::PipeType(1)],
        );
        assert_eq!(shots.rows()[0].kind, ProjectileKind::Sticky);
        assert_eq!(shots.rows()[0].origin, Some(O));
    }

    #[test]
    fn each_life_gets_a_fresh_serial_and_an_enter_on_a_live_slot_is_a_new_life() {
        let mut shots = Shots::default();
        shots.update(
            5,
            "CTFProjectile_Rocket",
            true,
            [ShotProp::Origin(O), ShotProp::Team(2)],
        );
        assert_eq!(shots.rows()[0].serial, 1);
        shots.update(
            5,
            "CTFProjectile_Rocket",
            false,
            [ShotProp::Origin([1., 2., 3.])],
        );
        assert_eq!(shots.rows()[0].serial, 1);
        shots.update(5, "CTFProjectile_Rocket", true, []);
        let second = shots.rows()[0];
        assert_eq!(second.serial, 2);
        assert_eq!(
            (second.origin, second.team),
            (None, None),
            "a new life starts blank"
        );
        shots.end(5);
        assert!(shots.rows().is_empty());
        shots.update(5, "CTFProjectile_Flare", false, [ShotProp::Origin(O)]);
        let third = shots.rows()[0];
        assert_eq!((third.serial, third.kind), (3, ProjectileKind::Flare));
    }

    #[test]
    fn a_new_class_on_a_live_slot_is_a_new_life() {
        let mut shots = Shots::default();
        shots.update(5, "CTFProjectile_Rocket", true, [ShotProp::Origin(O)]);
        shots.update(5, "CTFProjectile_Flare", false, [ShotProp::Origin(O)]);
        let rows = shots.rows();
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].serial, rows[0].kind), (2, ProjectileKind::Flare));
    }

    #[test]
    fn end_removes_and_rows_come_in_entity_order() {
        let mut shots = Shots::default();
        for entity in [30, 10, 20] {
            shots.update(entity, "CTFProjectile_Rocket", true, [ShotProp::Origin(O)]);
        }
        shots.end(20);
        shots.end(99);
        let serials: Vec<u32> = shots.rows().iter().map(|r| r.serial).collect();
        assert_eq!(serials, [2, 1]);
    }
}
