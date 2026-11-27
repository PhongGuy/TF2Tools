//! Feeds a demo's packets through tf-demo-parser, turning each tick into picker rows.

use std::collections::{BTreeMap, HashMap};
use std::io::Read as _;
use std::ops::ControlFlow;
use std::path::Path;

use tf_demo_parser::demo::data::{DemoTick, UserInfo};
use tf_demo_parser::demo::gameevent_gen::GameEvent;
use tf_demo_parser::demo::message::Message;
use tf_demo_parser::demo::message::gameevent::GameEventMessage;
use tf_demo_parser::demo::message::packetentities::UpdateType;
use tf_demo_parser::demo::packet::datatable::{ParseSendTable, ServerClass};
use tf_demo_parser::demo::packet::message::MessagePacketMeta;
use tf_demo_parser::demo::packet::stringtable::StringTableEntry;
use tf_demo_parser::demo::parser::MessageHandler;
use tf_demo_parser::demo::parser::gamestateanalyser::{
    Class, GameStateAnalyser, PlayerState, Team as ParserTeam,
};
use tf_demo_parser::demo::parser::handler::BorrowMessageHandler;
use tf_demo_parser::demo::sendprop::{SendProp, SendPropIdentifier};
use tf_demo_parser::demo::vector::Vector;
use tf_demo_parser::{Demo, DemoParser, MessageType, ParserState};

use super::kills::{Death, KillRow, PartyRow, kill_row};
use super::pick::{SENTINEL_TICK, SnapshotPicker, Step, TickFrame, TickRow, into_snapshot};
use super::shots::{ShotProp, Shots, is_projectile};
use super::{DemoSnapshot, MAX_SNAPSHOT_BYTES};
use crate::demo_view::tracks::MAX_KILLS;
use crate::error::DemoSnapshotError;
use crate::hud_maker::Team;
use crate::panic_mark::catching;
use crate::weapons::TfClass;

const FLAGS: SendPropIdentifier = SendPropIdentifier::new("DT_BasePlayer", "m_fFlags");
const VIEW_Z: SendPropIdentifier =
    SendPropIdentifier::new("DT_LocalPlayerExclusive", "m_vecViewOffset[2]");
const ROUND_STATE: SendPropIdentifier =
    SendPropIdentifier::new("DT_TeamplayRoundBasedRules", "m_iRoundState");
/// The `m_fFlags` bit Source sets while a player is crouched.
const FL_DUCKING: i64 = 2;
// Every projectile class sends its origin from one of these three tables.
const ORIGINS: [SendPropIdentifier; 3] = [
    SendPropIdentifier::new("DT_TFBaseRocket", "m_vecOrigin"),
    SendPropIdentifier::new("DT_TFWeaponBaseGrenadeProj", "m_vecOrigin"),
    SendPropIdentifier::new("DT_BaseEntity", "m_vecOrigin"),
];
const TEAM_NUM: SendPropIdentifier = SendPropIdentifier::new("DT_BaseEntity", "m_iTeamNum");
const PIPE_TYPE: SendPropIdentifier =
    SendPropIdentifier::new("DT_TFProjectile_Pipebomb", "m_iType");

// What the game-state analyser does not keep, per player entity.
#[derive(Default, Debug, Clone, Copy)]
struct Extra {
    flags: i64,
    view_z: Option<f32>,
    connected: bool,
    alive: bool,
    userinfo: bool,
    deleted: bool,
}

// Forwards everything but game events to the game-state analyser and also tracks the round
// state, the player resource's arrays, flags, the view offset, deletions and every kill.
#[derive(Default)]
struct Tracker {
    game: GameStateAnalyser,
    extra: HashMap<u32, Extra>,
    round_state: i64,
    names: BTreeMap<u32, String>,
    names_version: u64,
    shots: Shots,
    users: HashMap<u16, u32>,
    kills: Vec<KillRow>,
}

impl MessageHandler for Tracker {
    type Output = Tracker;

    fn does_handle(message_type: MessageType) -> bool {
        GameStateAnalyser::does_handle(message_type)
    }

    // The analyser would keep every game event on unbounded lists, so deaths are read here.
    fn handle_message(&mut self, message: &Message, tick: DemoTick, state: &ParserState) {
        if let Message::GameEvent(GameEventMessage { event, .. }) = message {
            if let GameEvent::PlayerDeath(death) = event {
                let death = Death {
                    user_id: death.user_id,
                    victim_entity: death.victim_ent_index,
                    attacker: death.attacker,
                    assister: death.assister,
                    weapon: death.weapon.to_string(),
                    crit_type: death.crit_type,
                    death_flags: death.death_flags,
                };
                self.death(u32::from(tick), &death);
            }
            return;
        }
        self.game.handle_message(message, tick, state);
        let Message::PacketEntities(packet) = message else {
            return;
        };
        for entity in &packet.entities {
            let Some(class) = state.server_classes.get(usize::from(entity.server_class)) else {
                continue;
            };
            let props = if is_projectile(class.name.as_str()) {
                shot_props(entity.props(state))
            } else {
                Vec::new()
            };
            self.shot_seen(
                u32::from(entity.entity_index),
                class.name.as_str(),
                entity.update_type,
                props,
            );
            match class.name.as_str() {
                "CTFPlayer" => {
                    let extra = self
                        .extra
                        .entry(u32::from(entity.entity_index))
                        .or_default();
                    match entity.update_type {
                        UpdateType::Delete => extra.deleted = true,
                        UpdateType::Enter => extra.deleted = false,
                        UpdateType::Delta | UpdateType::Leave => {}
                    }
                    for prop in entity.props(state) {
                        if prop.identifier == FLAGS {
                            extra.flags = i64::try_from(&prop.value).unwrap_or_default();
                        } else if prop.identifier == VIEW_Z {
                            extra.view_z = f32::try_from(&prop.value).ok();
                        }
                    }
                }
                // The resource arrays are tables named after the field, props "000".."033"
                // indexed by player entity.
                "CTFPlayerResource" => {
                    for prop in entity.props(state) {
                        let Some((table, index)) = prop.identifier.names() else {
                            continue;
                        };
                        let Ok(index) = index.as_str().parse::<u32>() else {
                            continue;
                        };
                        let set = i64::try_from(&prop.value).unwrap_or_default() != 0;
                        match table.as_str() {
                            "m_bConnected" => self.extra.entry(index).or_default().connected = set,
                            "m_bAlive" => self.extra.entry(index).or_default().alive = set,
                            _ => {}
                        }
                    }
                }
                "CTFGameRulesProxy" => {
                    for prop in entity.props(state) {
                        if prop.identifier == ROUND_STATE {
                            self.round_state = i64::try_from(&prop.value).unwrap_or_default();
                        }
                    }
                }
                _ => {}
            }
        }
        for id in &packet.removed_entities {
            self.removed(u32::from(*id));
            if let Some(extra) = self.extra.get_mut(&u32::from(*id)) {
                extra.deleted = true;
            }
        }
    }

    // A cleared userinfo entry is a disconnect; the analyser keeps the old info, so track it.
    fn handle_string_entry(
        &mut self,
        table: &str,
        index: usize,
        entry: &StringTableEntry,
        state: &ParserState,
    ) {
        self.game.handle_string_entry(table, index, entry, state);
        if table == "userinfo" {
            let parsed = UserInfo::parse_from_string_table(
                u16::try_from(index).unwrap_or(u16::MAX),
                entry.text.as_ref().map(|s| s.as_ref()),
                entry.extra_data.as_ref().map(|d| d.data.clone()),
            );
            let entity = u32::try_from(index).unwrap_or(u32::MAX).saturating_add(1);
            self.extra.entry(entity).or_default().userinfo = matches!(parsed, Ok(Some(_)));
            if let Ok(Some(info)) = parsed {
                let user_id = u16::from(info.player_info.user_id);
                self.user_seen(index, user_id, &info.player_info.name);
            }
        }
    }

    fn handle_data_tables(
        &mut self,
        tables: &[ParseSendTable],
        classes: &[ServerClass],
        state: &ParserState,
    ) {
        self.game.handle_data_tables(tables, classes, state);
    }

    fn handle_packet_meta(
        &mut self,
        tick: DemoTick,
        meta: &MessagePacketMeta,
        state: &ParserState,
    ) {
        self.game.handle_packet_meta(tick, meta, state);
    }

    fn into_output(self, _state: &ParserState) -> Tracker {
        self
    }
}

impl BorrowMessageHandler for Tracker {
    fn borrow_output(&self, _state: &ParserState) -> &Tracker {
        self
    }
}

// The props of one projectile update that `Shots` reads; critical comes from whichever table
// the class sends `m_bCritical` in.
fn shot_props(props: impl Iterator<Item = SendProp>) -> Vec<ShotProp> {
    props
        .filter_map(|prop| {
            if ORIGINS.contains(&prop.identifier) {
                let v = Vector::try_from(&prop.value).ok()?;
                Some(ShotProp::Origin([v.x, v.y, v.z]))
            } else if prop.identifier == TEAM_NUM {
                Some(ShotProp::Team(
                    i64::try_from(&prop.value).unwrap_or_default(),
                ))
            } else if prop.identifier == PIPE_TYPE {
                Some(ShotProp::PipeType(
                    i64::try_from(&prop.value).unwrap_or_default(),
                ))
            } else if prop
                .identifier
                .prop_name()
                .is_some_and(|name| name.as_str() == "m_bCritical")
            {
                Some(ShotProp::Critical(
                    bool::try_from(&prop.value).unwrap_or_default(),
                ))
            } else {
                None
            }
        })
        .collect()
}

fn team(team: ParserTeam) -> Option<Team> {
    match team {
        ParserTeam::Red => Some(Team::Red),
        ParserTeam::Blue => Some(Team::Blue),
        ParserTeam::Other | ParserTeam::Spectator => None,
    }
}

fn class(class: Class) -> Option<TfClass> {
    Some(match class {
        Class::Scout => TfClass::Scout,
        Class::Soldier => TfClass::Soldier,
        Class::Pyro => TfClass::Pyro,
        Class::Demoman => TfClass::Demoman,
        Class::Heavy => TfClass::Heavy,
        Class::Engineer => TfClass::Engineer,
        Class::Medic => TfClass::Medic,
        Class::Sniper => TfClass::Sniper,
        Class::Spy => TfClass::Spy,
        Class::Other => return None,
    })
}

impl Tracker {
    /// One entity update of server class `class`: Leave or Delete on a projectile, or any
    /// non-projectile class, ends the shot on that slot; any other projectile update feeds `Shots`.
    fn shot_seen(&mut self, entity: u32, class: &str, update: UpdateType, props: Vec<ShotProp>) {
        if !is_projectile(class) {
            if !self.shots.is_empty() {
                self.shots.end(entity);
            }
            return;
        }
        match update {
            UpdateType::Leave | UpdateType::Delete => self.shots.end(entity),
            UpdateType::Enter | UpdateType::Delta => {
                self.shots
                    .update(entity, class, update == UpdateType::Enter, props);
            }
        }
    }

    /// A parsed userinfo entry at string-table `index` (entity `index + 1`): maps the user id
    /// and keeps a non-empty name, so a player who left is still named.
    fn user_seen(&mut self, index: usize, user_id: u16, name: &str) {
        let entity = u32::try_from(index).unwrap_or(u32::MAX).saturating_add(1);
        self.users.insert(user_id, entity);
        if !name.is_empty() {
            self.names.insert(entity, name.to_owned());
            self.names_version += 1;
        }
    }

    /// One PlayerDeath at `tick`; pushes its row while `kills.len() <= MAX_KILLS`, so one past
    /// the cap is enough for the builder to see the overflow without the list growing further.
    fn death(&mut self, tick: u32, death: &Death) {
        if self.kills.len() > MAX_KILLS {
            return;
        }
        let row = kill_row(tick, death, &self.users, |entity| self.party(entity));
        if let Some(row) = row {
            self.kills.push(row);
        }
    }

    // A kill party as known at this moment: its name, and team and class from its player entity.
    fn party(&self, entity: u32) -> PartyRow {
        let player = self
            .game
            .state
            .players
            .iter()
            .find(|p| u32::from(p.entity) == entity);
        PartyRow {
            entity,
            name: self.names.get(&entity).cloned(),
            team: player.and_then(|p| team(p.team)),
            class: player.and_then(|p| class(p.class)),
        }
    }

    /// An id from `removed_entities`.
    fn removed(&mut self, entity: u32) {
        self.shots.end(entity);
    }

    // Every player the analyser has seen, as picker rows.
    fn frame(&self, tick: u32) -> TickFrame {
        let rows = self
            .game
            .state
            .players
            .iter()
            .map(|player| {
                let entity = u32::from(player.entity);
                let extra = self.extra.get(&entity).copied().unwrap_or_default();
                TickRow {
                    entity,
                    team: team(player.team),
                    class: class(player.class),
                    feet: [player.position.x, player.position.y, player.position.z],
                    yaw: player.view_angle,
                    pitch: player.pitch_angle,
                    view_offset_z: extra.view_z,
                    ducked: extra.flags & FL_DUCKING != 0,
                    has_userinfo: extra.userinfo,
                    deleted: extra.deleted,
                    connected: extra.connected,
                    life_alive: player.state == PlayerState::Alive,
                    resource_alive: extra.alive,
                    in_pvs: player.in_pvs,
                }
            })
            .collect();
        TickFrame {
            tick,
            round_state: self.round_state,
            rows,
            projectiles: self.shots.rows(),
            kills: Vec::new(),
        }
    }
}

/// The kill rows past `*seen`, moving `*seen` to the end.
fn kills_since(all: &[KillRow], seen: &mut usize) -> Vec<KillRow> {
    let new = all.get(*seen..).unwrap_or_default().to_vec();
    *seen = all.len();
    new
}

/// What a parse knows about tick length: the header's totals and the server's interval.
pub(crate) struct Clocking {
    pub header_ticks: u32,
    pub header_duration: f32,
    pub server_interval: f32,
    /// The last non-empty name each player entity was given.
    pub names: BTreeMap<u32, String>,
}

// Steps packets, handing each real tick's frame to `on_frame` until it breaks or the input
// ends; a cut-off demo simply ends. Signon ticks are skipped.
pub(crate) fn drive(
    bytes: &[u8],
    mut on_frame: impl FnMut(TickFrame, &Clocking) -> ControlFlow<()>,
) -> Result<Clocking, String> {
    let demo = Demo::new(bytes);
    let parser = DemoParser::new_with_analyser(demo.get_stream(), Tracker::default());
    let (header, mut ticker) = parser.ticker().map_err(|e| e.to_string())?;
    let mut clocking = Clocking {
        header_ticks: header.ticks,
        header_duration: header.duration,
        server_interval: 0.,
        names: BTreeMap::new(),
    };
    // Names are copied only when the parser saw a new one, so frames never carry strings.
    let mut names_version = 0;
    // Signon ticks are skipped before this, so their kills ride on the next real frame.
    let mut kills_seen = 0;
    loop {
        match ticker.next() {
            Ok(Some(tick)) => {
                let at = u32::from(tick.tick);
                if at >= SENTINEL_TICK {
                    continue;
                }
                clocking.server_interval = tick.state.game.state.interval_per_tick;
                if tick.state.names_version != names_version {
                    names_version = tick.state.names_version;
                    clocking.names.clone_from(&tick.state.names);
                }
                let mut frame = tick.state.frame(at);
                frame.kills = kills_since(&tick.state.kills, &mut kills_seen);
                if on_frame(frame, &clocking).is_break() {
                    break;
                }
            }
            Ok(None) => break,
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(clocking)
}

fn until_done(step: Step) -> ControlFlow<()> {
    match step {
        Step::Continue => ControlFlow::Continue(()),
        Step::Done => ControlFlow::Break(()),
    }
}

// Feeds frames to the picker until it settles; a cut-off demo ends with Ok(None).
fn pick(bytes: &[u8]) -> Result<Option<(u32, Vec<TickRow>)>, String> {
    let mut picker = SnapshotPicker::new();
    drive(bytes, |frame, _| until_done(picker.push(frame)))?;
    Ok(picker.finish())
}

/// Reads at most MAX_SNAPSHOT_BYTES of `dem` and returns its players at the picked tick.
pub fn read_demo_snapshot(dem: &Path) -> Result<DemoSnapshot, DemoSnapshotError> {
    let bytes = read_prefix(dem, MAX_SNAPSHOT_BYTES)
        .map_err(|e| DemoSnapshotError::Unreadable(e.to_string()))?;
    snapshot_from_bytes(&bytes, || {})
}

// The first `cap` bytes of the file, or all of it when shorter.
pub(crate) fn read_prefix(dem: &Path, cap: u64) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(dem)?
        .take(cap)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

// Runs `before_parse` then the parse under the panic guard, so a parser panic on a hostile
// demo becomes an error; `before_parse` lets a test force that panic.
fn snapshot_from_bytes(
    bytes: &[u8],
    before_parse: impl FnOnce(),
) -> Result<DemoSnapshot, DemoSnapshotError> {
    let picked = catching(|| {
        before_parse();
        pick(bytes)
    })
    .map_err(|_| DemoSnapshotError::Unreadable("the demo parser panicked".into()))?;
    match picked {
        Err(text) => Err(DemoSnapshotError::Unreadable(text)),
        Ok(None) => Err(DemoSnapshotError::NoPlayers),
        Ok(Some((tick, rows))) => Ok(into_snapshot(tick, rows)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_parser_panic_is_unreadable_and_leaves_the_thread_unmarked() {
        let got = snapshot_from_bytes(b"HL2DEMO\0", || panic!("forced"));
        match got {
            Err(DemoSnapshotError::Unreadable(text)) => {
                assert_eq!(text, "the demo parser panicked")
            }
            other => panic!("{other:?}"),
        }
        assert!(!crate::panic_mark::is_marked());
    }

    #[test]
    fn only_the_prefix_is_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("long.dem");
        std::fs::write(&path, vec![7u8; 4096]).unwrap();
        assert_eq!(read_prefix(&path, 1000).unwrap().len(), 1000);
        assert_eq!(read_prefix(&path, 10_000).unwrap().len(), 4096);
    }

    use super::super::shots::ShotProp;
    use tf_demo_parser::demo::message::packetentities::UpdateType;

    #[test]
    fn leave_delete_removal_and_a_new_class_end_a_shot() {
        let mut t = Tracker::default();
        let o = ShotProp::Origin([1., 2., 3.]);
        for entity in 1..=5 {
            t.shot_seen(entity, "CTFProjectile_Rocket", UpdateType::Enter, vec![o]);
        }
        assert_eq!(t.frame(10).projectiles.len(), 5);
        t.shot_seen(1, "CTFProjectile_Rocket", UpdateType::Leave, vec![]);
        t.shot_seen(2, "CTFProjectile_Rocket", UpdateType::Delete, vec![]);
        t.removed(3);
        t.shot_seen(4, "CTFPlayer", UpdateType::Delta, vec![]);
        t.shot_seen(5, "CTFProjectile_Rocket", UpdateType::Delta, vec![]);
        let left = t.frame(11).projectiles;
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].origin, Some([1., 2., 3.]));
    }
    use super::super::kills::Death;
    use crate::demo_view::tracks::MAX_KILLS;

    fn death(user_id: u16, attacker: u16) -> Death {
        Death {
            user_id,
            victim_entity: 0,
            attacker,
            assister: 0,
            weapon: "knife".into(),
            crit_type: 0,
            death_flags: 0,
        }
    }

    #[test]
    fn deaths_become_kill_rows_with_the_names_of_the_moment() {
        let mut t = Tracker::default();
        t.user_seen(0, 10, "Alice");
        t.user_seen(1, 11, "Bob");
        t.death(500, &death(10, 11));
        t.user_seen(0, 10, "Renamed");
        t.death(600, &death(99, 11));
        assert_eq!(t.kills.len(), 1, "an unknown victim is dropped");
        let k = &t.kills[0];
        assert_eq!(
            (k.tick, k.victim.entity, k.victim.name.as_deref()),
            (500, 1, Some("Alice"))
        );
        assert_eq!(
            k.killer.as_ref().map(|p| (p.entity, p.name.as_deref())),
            Some((2, Some("Bob")))
        );
        assert_eq!(
            (k.victim.team, k.victim.class),
            (None, None),
            "no player entity was seen"
        );
    }

    #[test]
    fn an_empty_name_keeps_the_old_one_and_the_new_user_id_still_maps() {
        let mut t = Tracker::default();
        t.user_seen(4, 20, "Old");
        t.user_seen(4, 21, "");
        t.death(1, &death(21, 0));
        assert_eq!(t.kills[0].victim.entity, 5);
        assert_eq!(t.kills[0].victim.name.as_deref(), Some("Old"));
    }

    #[test]
    fn the_kill_list_stops_one_past_the_cap() {
        let mut t = Tracker::default();
        t.user_seen(0, 10, "Alice");
        for tick in 0..(MAX_KILLS as u32 + 5) {
            t.death(tick, &death(10, 0));
        }
        assert_eq!(t.kills.len(), MAX_KILLS + 1);
    }

    #[test]
    fn each_frame_gets_only_the_kills_since_the_last() {
        let mut t = Tracker::default();
        t.user_seen(0, 10, "Alice");
        let mut seen = 0;
        t.death(1, &death(10, 0));
        t.death(1, &death(10, 0));
        assert_eq!(kills_since(&t.kills, &mut seen).len(), 2);
        assert!(kills_since(&t.kills, &mut seen).is_empty());
        t.death(2, &death(10, 0));
        let next = kills_since(&t.kills, &mut seen);
        assert_eq!((next.len(), next[0].tick), (1, 2));
    }
}
