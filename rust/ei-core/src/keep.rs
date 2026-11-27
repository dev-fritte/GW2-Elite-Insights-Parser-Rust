//! Which combat records the C# parser keeps, and the values it derives while reading them.
//!
//! Port of the loop in `EvtcParser.ParseCombatList` together with `IsValid`,
//! `IsSupportedStateChange` and `EvtcVersionEvent.SetFromCombatItem`. The result is checked
//! against the layer B columns of the oracle (`docs/phase1/ORACLE.md`).

use std::collections::HashMap;
use std::fmt;

use evtc_ingest::{CombatRecord, parse_dotnet_int};

use crate::generated::state_change as sc;
use crate::rules::{dst_is_agent, has_time, src_is_agent};

/// Trigger id of instance logs (`TargetID.Instance`).
pub const TRIGGER_INSTANCE: u16 = 2;

/// Signature of the "Healing Stats" extension (`HealingStatsExtensionHandler.EXT_HealingStats`).
const EXT_HEALING_STATS: u32 = 0x9c9b_3c99;

/// Largest log duration the C# parser accepts: 24 hours in milliseconds.
const MAX_LOG_MS: i64 = 86_400_000;

/// Limits and switches of the parser settings that influence which records are kept.
#[derive(Debug, Clone, Copy)]
pub struct KeepSettings {
    /// Logs shorter than this many milliseconds are rejected (`EvtcParserSettings.TooShortLimit`).
    pub too_short_limit: i64,
    /// Whether extension records are parsed at all (`EvtcParserSettings.ParseExtensions`).
    pub parse_extensions: bool,
}

impl Default for KeepSettings {
    fn default() -> Self {
        Self {
            too_short_limit: 2200,
            parse_extensions: true,
        }
    }
}

/// Build and revision of the arcdps version, as `EvtcVersionEvent` tracks them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArcVersion {
    /// Build number; starts as the number from the file header.
    pub build: i32,
    /// Revision; `-1` as long as no arc build record was seen.
    pub revision: i32,
}

impl ArcVersion {
    /// Applies an arc build record (`EvtcVersionEvent.SetFromCombatItem`): the 48 bytes of the
    /// record after `time` are a text like `<build>.<revision>-<x>-<platform>`.
    pub fn set_from_record(&mut self, c: &CombatRecord) {
        let old_build = self.build;
        if let Some((build, revision)) = Self::parse_record(c) {
            self.build = build;
            self.revision = revision;
        } else {
            self.build = old_build;
            self.revision = 0;
        }
    }

    fn parse_record(c: &CombatRecord) -> Option<(i32, i32)> {
        let mut bytes = Vec::with_capacity(48);
        bytes.extend_from_slice(&c.src_agent.to_le_bytes());
        bytes.extend_from_slice(&c.dst_agent.to_le_bytes());
        bytes.extend_from_slice(&c.value.to_le_bytes());
        bytes.extend_from_slice(&c.buff_dmg.to_le_bytes());
        bytes.extend_from_slice(&c.overstack_value.to_le_bytes());
        bytes.extend_from_slice(&c.skill_id.to_le_bytes());
        bytes.extend_from_slice(&c.src_instid.to_le_bytes());
        bytes.extend_from_slice(&c.dst_instid.to_le_bytes());
        bytes.extend_from_slice(&c.src_master_instid.to_le_bytes());
        bytes.extend_from_slice(&c.dst_master_instid.to_le_bytes());
        bytes.extend_from_slice(&[
            c.iff,
            c.is_buff,
            c.result,
            c.is_activation,
            c.is_buff_remove,
            c.is_ninety,
            c.is_fifty,
            c.is_moving,
        ]);
        // `Encoding.UTF8.GetString(bytes).TrimEnd('\0')`; the text is ASCII in practice
        let text = String::from_utf8_lossy(&bytes);
        let text = text.trim_end_matches('\0');
        // NOTE: in the C# `SplitResult` `Tail` is the left part and `Head` the right part.
        let (build_text, rest) = split_once(text, '.');
        let build = parse_dotnet_int(build_text.as_bytes())?;
        let (revision_text, _) = split_once(rest, '-');
        let revision = parse_dotnet_int(revision_text.as_bytes())?;
        Some((build, revision))
    }
}

/// `StringExt.SplitOnce`: split at the first occurrence; without one the tail is empty.
fn split_once(text: &str, split: char) -> (&str, &str) {
    text.split_once(split).unwrap_or((text, ""))
}

/// Why a log is rejected while its records are read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeepError {
    /// No record survived the filter (`EvtcCombatEventException`).
    NoEvents,
    /// The log is shorter than the limit (`TooShortException`).
    TooShort {
        /// Duration of the log in milliseconds.
        duration: i64,
    },
    /// The log is longer than 24 hours (`TooLongException`).
    TooLong,
}

impl fmt::Display for KeepError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoEvents => f.write_str("no combat events found"),
            Self::TooShort { duration } => write!(f, "log too short ({duration} ms)"),
            Self::TooLong => f.write_str("log too long"),
        }
    }
}

impl std::error::Error for KeepError {}

/// The kept records and the values `ParseCombatList` returns next to them.
#[derive(Debug, Clone)]
pub struct Kept {
    /// Kept records in file order, times relative to the first timed record.
    pub events: Vec<CombatRecord>,
    /// Trigger id after the instance correction.
    pub trigger_id: u16,
    /// Map id of the last map id record, or `-1`.
    pub map_id: i32,
    /// Raw time of the first timed record, or `i64::MIN` if there was none.
    pub log_start_offset: i64,
    /// Relative time of the last timed record.
    pub log_end_time: i64,
    /// GW2 build of the last non-zero build record, or 0.
    pub gw2_build: u64,
    /// Arcdps version after applying the arc build records.
    pub version: ArcVersion,
    /// `arcdps_agent_redirection`: source address to new address, from agent change records.
    pub agent_redirection: HashMap<u64, u64>,
    /// Signatures of the extensions that were announced in the log.
    pub extensions: Vec<u32>,
}

fn is_extension(state_change: u8) -> bool {
    state_change == sc::EXTENSION || state_change == sc::EXTENSION_COMBAT
}

fn is_supported_state_change(state_change: u8) -> bool {
    !matches!(
        state_change,
        sc::UNKNOWN
            | sc::REPL_INFO
            | sc::STAT_RESET
            | sc::API_DELAYED
            | sc::IDLE
            | sc::AGENT_CHANGE
            | sc::EARLY_EXIT
    )
}

fn is_effect(state_change: u8) -> bool {
    matches!(
        state_change,
        sc::EFFECT_51
            | sc::EFFECT_45
            | sc::EFFECT_AGENT_CREATE
            | sc::EFFECT_AGENT_REMOVE
            | sc::EFFECT_GROUND_CREATE
            | sc::EFFECT_GROUND_REMOVE
            | sc::EFFECT_MISSILE_CREATE
    )
}

fn is_missile(state_change: u8) -> bool {
    matches!(
        state_change,
        sc::MISSILE_CREATE | sc::MISSILE_LAUNCH | sc::MISSILE_REMOVE
    )
}

/// The "Healing Stats" extension marks healing and barrier records by value, not by state change.
fn healing_stats_has_agents(c: &CombatRecord) -> bool {
    (c.is_buff == 0 && c.value < 0) || (c.is_buff != 0 && c.value == 0 && c.buff_dmg < 0)
}

/// `CombatItem.SrcIsAgent(extensions)` and `DstIsAgent(extensions)`: records of an announced
/// extension ask the extension handler.
fn src_dst_is_agent(c: &CombatRecord, extensions: &[u32]) -> bool {
    if is_extension(c.state_change) && c.pad != 0 && extensions.contains(&c.pad) {
        return healing_stats_has_agents(c);
    }
    src_is_agent(c.state_change) || dst_is_agent(c.state_change)
}

/// `LogType.Map` of `SquadCombatStartEvent.GetLogType`: the low 32 bits of `dst_agent` as an
/// `int`, mapped through `GetLogType` (values below 4 become the enum value, the rest `Unknown`).
fn start_event_is_map_log(c: &CombatRecord) -> bool {
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    let value = c.dst_agent as u32 as i32;
    value < 4 && value.to_le_bytes()[0] == 2
}

/// Applies the C# record filter and time normalisation.
///
/// # Errors
/// [`KeepError`] when no record is kept or the duration is outside the accepted range.
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)] // mirrors C# casts
pub fn keep_events(
    records: &[CombatRecord],
    header_trigger_id: u16,
    header_build: i32,
    settings: KeepSettings,
) -> Result<Kept, KeepError> {
    let mut id = header_trigger_id;
    // -1: not decided yet, 0: boss log, 1: instance log (stop at the log end record)
    let mut stop_at_log_end: i32 = if id == TRIGGER_INSTANCE { 1 } else { -1 };
    let mut keep_only_extensions = false;
    let mut current_map: i64 = -1;
    let mut map_id: i32 = -1;
    let mut log_start_offset = i64::MIN;
    let mut log_end_time = 0i64;
    let mut gw2_build = 0u64;
    let mut version = ArcVersion {
        build: header_build,
        revision: -1,
    };
    let mut redirection = HashMap::new();
    let mut extensions: Vec<u32> = Vec::new();
    let mut kept: Vec<CombatRecord> = Vec::with_capacity(records.len());
    let mut extension_positions: Vec<usize> = Vec::new();

    for original in records {
        let mut c = *original;
        if stop_at_log_end == -1 && c.state_change == sc::SQUAD_COMBAT_START {
            if start_event_is_map_log(&c) {
                id = TRIGGER_INSTANCE;
                stop_at_log_end = 1;
            } else {
                stop_at_log_end = 0;
            }
        }
        if c.state_change == sc::MAP_ID {
            map_id = c.src_agent as i32;
            current_map = i64::from(map_id);
        }
        if c.state_change == sc::MAP_CHANGE {
            current_map = i64::from(c.src_agent as i32);
        }
        if c.state_change == sc::AGENT_CHANGE {
            redirection.insert(c.src_agent, c.dst_agent);
        }
        let valid = is_valid(&c, current_map, &mut extensions, id, map_id, settings);
        if !valid || (keep_only_extensions && !is_extension(c.state_change)) {
            continue;
        }
        if c.state_change == sc::ARC_BUILD {
            version.set_from_record(&c);
            continue;
        }
        if has_time(c.state_change) {
            if log_start_offset == i64::MIN {
                log_start_offset = c.time;
            }
            c.time = c.time.wrapping_sub(log_start_offset);
            log_end_time = c.time;
        }
        if is_extension(c.state_change) {
            extension_positions.push(kept.len());
        }
        if c.state_change == sc::GW_BUILD && c.src_agent != 0 {
            gw2_build = c.src_agent;
        }
        if c.state_change == sc::SQUAD_COMBAT_END && stop_at_log_end <= 0 {
            keep_only_extensions = true;
        }
        kept.push(c);
    }
    // Records of announced extensions are shifted afterwards (every record of the Healing Stats
    // extension has a time); the others, e.g. the announcement records, are not kept anyway.
    for position in extension_positions {
        let c = &mut kept[position];
        if c.pad != 0 && extensions.contains(&c.pad) {
            c.time = c.time.wrapping_sub(log_start_offset);
        }
    }

    if kept.is_empty() {
        return Err(KeepError::NoEvents);
    }
    if log_end_time < settings.too_short_limit {
        return Err(KeepError::TooShort {
            duration: log_end_time,
        });
    }
    if log_end_time > MAX_LOG_MS {
        return Err(KeepError::TooLong);
    }
    let mut kept = Kept {
        events: kept,
        trigger_id: id,
        map_id,
        log_start_offset,
        log_end_time,
        gw2_build,
        version,
        agent_redirection: redirection,
        extensions,
    };
    crate::special::process(&mut kept);
    Ok(kept)
}

/// `EvtcParser.IsValid`. Announcing records of supported extensions register the extension.
fn is_valid(
    c: &CombatRecord,
    current_map: i64,
    extensions: &mut Vec<u32>,
    id: u16,
    map_id: i32,
    settings: KeepSettings,
) -> bool {
    if !is_supported_state_change(c.state_change) {
        return false;
    }
    if map_id != -1 && i64::from(map_id) != current_map && src_dst_is_agent(c, extensions) {
        return false;
    }
    if id == TRIGGER_INSTANCE && c.state_change == sc::BUFF_INITIAL {
        return false;
    }
    if c.state_change == sc::HEALTH_UPDATE && c.dst_agent > 20_000 {
        // more than 200 % health (`dst_agent` is the health percentage times 100)
        return false;
    }
    if is_extension(c.state_change) {
        if !settings.parse_extensions {
            return false;
        }
        if c.pad == 0 && c.state_change == sc::EXTENSION {
            // the first record of an addon is its announcement and is consumed by the handler
            let signature = (c.src_agent & 0xFFFF_FFFF) as u32;
            let revision = (c.src_agent & 0x00FF_FFFF_0000_0000) >> 32;
            if signature == EXT_HEALING_STATS
                && (revision == 1 || revision == 2)
                && !extensions.contains(&signature)
            {
                extensions.push(signature);
            }
            return false;
        }
        return extensions.contains(&c.pad);
    }
    if c.src_instid == 0
        && c.dst_agent == 0
        && c.src_agent == 0
        && c.dst_instid == 0
        && c.iff >= 2
        && !is_effect(c.state_change)
        && !is_missile(c.state_change)
    {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(time: i64, state_change: u8) -> CombatRecord {
        CombatRecord {
            time,
            src_agent: 1,
            dst_agent: 0,
            value: 0,
            buff_dmg: 0,
            overstack_value: 0,
            skill_id: 0,
            src_instid: 1,
            dst_instid: 0,
            src_master_instid: 0,
            dst_master_instid: 0,
            iff: 0,
            is_buff: 0,
            result: 0,
            is_activation: 0,
            is_buff_remove: 0,
            is_ninety: 0,
            is_fifty: 0,
            is_moving: 0,
            state_change,
            is_flanking: 0,
            is_shields: 0,
            is_offcycle: 0,
            pad: 0,
        }
    }

    fn settings() -> KeepSettings {
        KeepSettings {
            too_short_limit: 10,
            parse_extensions: true,
        }
    }

    #[test]
    fn times_become_relative_to_the_first_timed_record() {
        let records = [
            rec(1000, sc::COMBAT),
            rec(1500, sc::COMBAT),
            rec(2000, sc::COMBAT),
        ];
        let kept = keep_events(&records, 15429, 20_260_920, settings()).unwrap();
        assert_eq!(kept.log_start_offset, 1000);
        assert_eq!(kept.log_end_time, 1000);
        assert_eq!(
            kept.events.iter().map(|e| e.time).collect::<Vec<_>>(),
            [0, 500, 1000]
        );
    }

    #[test]
    fn untimed_records_keep_their_payload_in_the_time_field() {
        let records = [
            rec(77, sc::LANGUAGE),
            rec(1000, sc::COMBAT),
            rec(1100, sc::COMBAT),
        ];
        let mut lang = records;
        lang[0].src_agent = 3;
        let kept = keep_events(&lang, 1, 1, settings()).unwrap();
        assert_eq!(kept.events[0].time, 77);
        assert_eq!(kept.events[1].time, 0);
    }

    #[test]
    fn unsupported_and_empty_records_are_dropped() {
        let mut empty = rec(1, sc::COMBAT);
        empty.src_agent = 0;
        empty.src_instid = 0;
        empty.iff = 2;
        let records = [
            rec(1, sc::IDLE),
            rec(2, sc::UNKNOWN),
            empty,
            rec(1000, sc::COMBAT),
            rec(1100, sc::COMBAT),
        ];
        let kept = keep_events(&records, 1, 1, settings()).unwrap();
        assert_eq!(kept.events.len(), 2);
    }

    #[test]
    fn map_changes_drop_agent_records_of_other_maps() {
        let mut map = rec(1000, sc::MAP_ID);
        map.src_agent = 1062;
        let mut other = rec(1050, sc::MAP_CHANGE);
        other.src_agent = 5;
        let mut back = rec(1090, sc::MAP_CHANGE);
        back.src_agent = 1062;
        let records = [
            map,
            rec(1010, sc::COMBAT),
            other,
            rec(1060, sc::COMBAT),
            back,
            rec(1100, sc::COMBAT),
        ];
        let kept = keep_events(&records, 1, 1, settings()).unwrap();
        assert_eq!(kept.map_id, 1062);
        let times: Vec<i64> = kept.events.iter().map(|e| e.time).collect();
        assert_eq!(times, [0, 10, 50, 90, 100]); // the combat record at 1060 was dropped
    }

    #[test]
    fn short_and_empty_logs_are_rejected() {
        assert_eq!(
            keep_events(&[], 1, 1, settings()).unwrap_err(),
            KeepError::NoEvents
        );
        let records = [rec(1000, sc::COMBAT), rec(1003, sc::COMBAT)];
        assert_eq!(
            keep_events(&records, 1, 1, settings()).unwrap_err(),
            KeepError::TooShort { duration: 3 }
        );
    }

    #[test]
    fn map_start_event_turns_a_boss_log_into_an_instance_log() {
        let mut start = rec(1000, sc::SQUAD_COMBAT_START);
        start.dst_agent = 2;
        let records = [start, rec(1100, sc::COMBAT), rec(1200, sc::BUFF_INITIAL)];
        let kept = keep_events(&records, 15429, 1, settings()).unwrap();
        assert_eq!(kept.trigger_id, TRIGGER_INSTANCE);
        assert_eq!(kept.events.len(), 2); // buff initial is dropped in instance logs
    }

    #[test]
    fn arc_build_record_sets_build_and_revision() {
        let mut c = rec(0, sc::ARC_BUILD);
        c.src_instid = 0; // the payload is only the text
        // "20260920.204859-611-x64" spread over the 48 byte payload
        let text = b"20260920.204859-611-x64";
        let mut raw = [0u8; 48];
        raw[..text.len()].copy_from_slice(text);
        c.src_agent = u64::from_le_bytes(raw[0..8].try_into().unwrap());
        c.dst_agent = u64::from_le_bytes(raw[8..16].try_into().unwrap());
        c.value = i32::from_le_bytes(raw[16..20].try_into().unwrap());
        c.buff_dmg = i32::from_le_bytes(raw[20..24].try_into().unwrap());
        let mut v = ArcVersion {
            build: 5,
            revision: -1,
        };
        v.set_from_record(&c);
        assert_eq!(
            v,
            ArcVersion {
                build: 20_260_920,
                revision: 204_859
            }
        );
        // garbage keeps the build and sets revision 0
        let mut bad = ArcVersion {
            build: 5,
            revision: 9,
        };
        bad.set_from_record(&rec(0, sc::ARC_BUILD));
        assert_eq!(
            bad,
            ArcVersion {
                build: 5,
                revision: 0
            }
        );
    }
}
