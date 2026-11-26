//! Feeds a demo's packets through tf-demo-parser, turning each tick into picker rows.

use std::collections::HashMap;
use std::io::Read as _;
use std::ops::ControlFlow;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use tf_demo_parser::demo::data::{DemoTick, UserInfo};
use tf_demo_parser::demo::message::Message;
use tf_demo_parser::demo::message::packetentities::UpdateType;
use tf_demo_parser::demo::packet::datatable::{ParseSendTable, ServerClass};
use tf_demo_parser::demo::packet::message::MessagePacketMeta;
use tf_demo_parser::demo::packet::stringtable::StringTableEntry;
use tf_demo_parser::demo::parser::MessageHandler;
use tf_demo_parser::demo::parser::gamestateanalyser::{
    Class, GameStateAnalyser, PlayerState, Team as ParserTeam,
};
use tf_demo_parser::demo::parser::handler::BorrowMessageHandler;
use tf_demo_parser::demo::sendprop::SendPropIdentifier;
use tf_demo_parser::{Demo, DemoParser, MessageType, ParserState};

use super::pick::{SENTINEL_TICK, SnapshotPicker, Step, TickFrame, TickRow, into_snapshot};
use super::{DemoSnapshot, MAX_SNAPSHOT_BYTES};
use crate::demo_view::tracks::{
    DemoTracks, MAX_DEMO_BYTES, MAX_TRACK_SAMPLES, TrackBuilder, seconds_per_tick,
};
use crate::error::{DemoSnapshotError, DemoTracksError};
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

// Forwards everything to the game-state analyser and also tracks the round state, the
// player resource's connected and alive arrays, flags, the view offset and deletions.
#[derive(Default)]
struct Tracker {
    game: GameStateAnalyser,
    extra: HashMap<u32, Extra>,
    round_state: i64,
}

impl MessageHandler for Tracker {
    type Output = Tracker;

    fn does_handle(message_type: MessageType) -> bool {
        GameStateAnalyser::does_handle(message_type)
    }

    fn handle_message(&mut self, message: &Message, tick: DemoTick, state: &ParserState) {
        self.game.handle_message(message, tick, state);
        let Message::PacketEntities(packet) = message else {
            return;
        };
        for entity in &packet.entities {
            let Some(class) = state.server_classes.get(usize::from(entity.server_class)) else {
                continue;
            };
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
        }
    }
}

/// What a parse knows about tick length: the header's totals and the server's interval.
pub(crate) struct Clocking {
    pub header_ticks: u32,
    pub header_duration: f32,
    pub server_interval: f32,
}

impl Clocking {
    /// The tick length the tracks and the clock use.
    pub fn seconds_per_tick(&self) -> f32 {
        seconds_per_tick(
            self.server_interval,
            self.header_duration,
            self.header_ticks,
        )
    }
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
    };
    loop {
        match ticker.next() {
            Ok(Some(tick)) => {
                let at = u32::from(tick.tick);
                if at >= SENTINEL_TICK {
                    continue;
                }
                clocking.server_interval = tick.state.game.state.interval_per_tick;
                if on_frame(tick.state.frame(at), &clocking).is_break() {
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

/// Reads at most MAX_DEMO_BYTES of `dem` and builds every player's tracks over it. `cancel` is
/// checked before and after the read and at every frame.
pub fn read_demo_tracks(dem: &Path, cancel: &AtomicBool) -> Result<DemoTracks, DemoTracksError> {
    let unreadable = |e: std::io::Error| DemoTracksError::Unreadable(e.to_string());
    if cancel.load(Ordering::Relaxed) {
        return Err(DemoTracksError::Cancelled);
    }
    let file_capped = std::fs::metadata(dem).map_err(unreadable)?.len() > MAX_DEMO_BYTES;
    let bytes = read_prefix(dem, MAX_DEMO_BYTES).map_err(unreadable)?;
    if cancel.load(Ordering::Relaxed) {
        return Err(DemoTracksError::Cancelled);
    }
    let tracks = tracks_from_bytes(&bytes, file_capped, cancel, MAX_TRACK_SAMPLES, || {});
    drop(bytes);
    tracks
}

// Builds the tracks under the panic guard, as `snapshot_from_bytes` does; the sample cap is a
// parameter so tests can hit it with a small demo.
fn tracks_from_bytes(
    bytes: &[u8],
    file_capped: bool,
    cancel: &AtomicBool,
    sample_cap: usize,
    before_parse: impl FnOnce(),
) -> Result<DemoTracks, DemoTracksError> {
    let built = catching(|| -> Result<_, String> {
        before_parse();
        let mut builder = TrackBuilder::new(sample_cap);
        let mut cancelled = false;
        let clocking = drive(bytes, |frame, clocking| {
            if cancel.load(Ordering::Relaxed) {
                cancelled = true;
                return ControlFlow::Break(());
            }
            until_done(builder.push(&frame, clocking.seconds_per_tick()))
        })?;
        let spt = clocking.seconds_per_tick();
        Ok((
            cancelled,
            builder.finish(spt, clocking.header_ticks, file_capped),
        ))
    })
    .map_err(|_| DemoTracksError::Unreadable("the demo parser panicked".into()))?;
    match built {
        Err(text) => Err(DemoTracksError::Unreadable(text)),
        Ok((true, _)) => Err(DemoTracksError::Cancelled),
        Ok((false, None)) => Err(DemoTracksError::NoPlayers),
        Ok((false, Some(tracks))) => Ok(tracks),
    }
}

// The first `cap` bytes of the file, or all of it when shorter.
fn read_prefix(dem: &Path, cap: u64) -> std::io::Result<Vec<u8>> {
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
    fn a_parser_panic_while_building_tracks_is_unreadable() {
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let got = tracks_from_bytes(b"HL2DEMO\0", false, &cancel, 100, || panic!("forced"));
        match got {
            Err(crate::error::DemoTracksError::Unreadable(text)) => {
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
}
