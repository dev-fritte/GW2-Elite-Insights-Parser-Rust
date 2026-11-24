//! The whole path from the bytes of a log to the JSON the core can currently write.

use std::fmt;

use evtc_ingest::Evtc;
use serde_json::{Map, Value, json};

use crate::api::Api;
use crate::keep::{KeepError, KeepSettings, Kept, keep_events};
use crate::meta::{Metadata, extract};
use crate::players::{Player, find_players};

/// What the core knows about a log.
#[derive(Debug, Clone)]
pub struct Analysis {
    /// Log level metadata.
    pub metadata: Metadata,
    /// Squad players, sorted by character name.
    pub players: Vec<Player>,
}

/// Why a log could not be analysed.
#[derive(Debug)]
pub enum AnalysisError {
    /// The file is not a readable EVTC log.
    Ingest(evtc_ingest::Error),
    /// The record filter rejected the log.
    Keep(KeepError),
    /// A buff could not be simulated.
    Buff(crate::buff_events::Error),
    /// The damage of the log cannot be computed.
    Damage(String),
}

impl fmt::Display for AnalysisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ingest(e) => write!(f, "{e}"),
            Self::Keep(e) => write!(f, "{e}"),
            Self::Buff(e) => write!(f, "{e}"),
            Self::Damage(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for AnalysisError {}

impl From<evtc_ingest::Error> for AnalysisError {
    fn from(e: evtc_ingest::Error) -> Self {
        Self::Ingest(e)
    }
}

impl From<KeepError> for AnalysisError {
    fn from(e: KeepError) -> Self {
        Self::Keep(e)
    }
}

/// Analyses the bytes of an (already decompressed) EVTC log.
///
/// # Errors
/// [`AnalysisError`] if the log is malformed, too short, too long or has no combat records.
pub fn analyze(data: &[u8], api: &Api) -> Result<Analysis, AnalysisError> {
    let evtc = Evtc::parse(data)?;
    let header = evtc.header();
    let records: Vec<_> = evtc.events().collect();
    let kept = keep_events(
        &records,
        header.trigger_id(),
        header.build(),
        KeepSettings::default(),
    )?;
    let players = find_players(evtc.agents(), &kept, &api.specs, false);
    Ok(Analysis {
        metadata: extract(&kept, Some(&api.maps)),
        players,
    })
}

/// The JSON object of an analysis. Keys that exist in the C# JSON (`JsonLog`) carry the same name
/// and meaning there, so they can be compared field by field. Keys that the C# JSON does not
/// have start with `core`.
#[must_use]
pub fn to_json(analysis: &Analysis) -> Value {
    let m = &analysis.metadata;
    let mut o = Map::new();
    o.insert("arcVersion".into(), json!(m.arc_version));
    o.insert("arcRevision".into(), json!(m.arc_revision));
    o.insert("triggerID".into(), json!(m.trigger_id));
    o.insert("isInstanceLog".into(), json!(m.is_instance_log));
    if let Some(map_id) = m.map_id {
        o.insert("mapID".into(), json!(map_id));
    }
    o.insert("gW2Build".into(), json!(m.gw2_build));
    o.insert("language".into(), json!(m.language.name()));
    o.insert("languageID".into(), json!(m.language as u8));
    o.insert("fractalScale".into(), json!(m.fractal_scale));
    if let Some(region) = m.region {
        o.insert("region".into(), json!(region.name()));
    }
    if let Some(start) = m.log_start_unix {
        o.insert("coreLogStartUnix".into(), json!(start));
    }
    if let Some(end) = m.log_end_unix {
        o.insert("coreLogEndUnix".into(), json!(end));
    }
    let players: Vec<Value> = analysis
        .players
        .iter()
        .map(|p| {
            json!({
                "name": p.character,
                "account": p.account,
                "group": p.group,
                "profession": p.profession,
            })
        })
        .collect();
    o.insert("players".into(), Value::Array(players));
    Value::Object(o)
}

/// The buff simulation of every squad player.
#[derive(Debug, Clone)]
pub struct PlayerBuffs {
    /// The player.
    pub player: Player,
    /// All simulated buffs of the player.
    pub buffs: Vec<crate::buff_events::BuffResult>,
}

/// Simulation result of all players of a log, with the time span it covers.
#[derive(Debug, Clone)]
pub struct BuffAnalysis {
    /// Start of the simulated span.
    pub log_start: i64,
    /// End of the simulated span.
    pub log_end: i64,
    /// Buffs per player.
    pub players: Vec<PlayerBuffs>,
}

fn elementalist_kind(player: &Player) -> crate::buff_events::ElementalistKind {
    use crate::buff_events::ElementalistKind;
    match player.profession.as_str() {
        "Weaver" => ElementalistKind::Weaver,
        "Elementalist" | "Tempest" | "Catalyst" | "Evoker" => ElementalistKind::Other,
        _ => ElementalistKind::None,
    }
}

/// Simulates the buffs of all squad players of a log.
///
/// # Errors
/// [`AnalysisError`] for unreadable logs, or a buff error as `Buff`.
pub fn analyze_buffs(
    data: &[u8],
    api: &Api,
    timing: Option<Timing>,
) -> Result<BuffAnalysis, AnalysisError> {
    let evtc = Evtc::parse(data)?;
    let header = evtc.header();
    let records: Vec<_> = evtc.events().collect();
    let kept = keep_events(
        &records,
        header.trigger_id(),
        header.build(),
        KeepSettings::default(),
    )?;
    let players = find_players(evtc.agents(), &kept, &api.specs, false);
    let timing = timing.unwrap_or_else(|| core_timing(&kept, &evtc, &players));
    let ctx = crate::buff_events::LogContext {
        kept: &kept,
        gw2_build: kept.gw2_build,
        log_start: 0,
        time_offset: timing.offset,
        log_end: timing.duration,
    };
    let mut out = Vec::with_capacity(players.len());
    for player in players {
        let buffs =
            crate::buff_events::simulate_agent(&ctx, player.address, elementalist_kind(&player))
                .map_err(AnalysisError::Buff)?;
        out.push(PlayerBuffs { player, buffs });
    }
    Ok(BuffAnalysis {
        log_start: 0,
        log_end: ctx.log_end,
        players: out,
    })
}

/// The time shift and the length of the fight, in milliseconds: the time the encounter logic
/// starts the fight at, and the time from there to its end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    /// Time of the records that counts as 0.
    pub offset: i64,
    /// Length of the fight.
    pub duration: i64,
}

/// The timing the encounter logic determines for a log. If it cannot (a main target is missing),
/// the offset is 0 and the fight lasts until the last record.
fn core_timing(kept: &Kept, evtc: &Evtc<'_>, players: &[Player]) -> Timing {
    let offset = crate::encounter::log_offset(kept, evtc.agents()).unwrap_or(0);
    let addresses: Vec<u64> = players.iter().map(|p| p.address).collect();
    let end = crate::encounter::log_end(kept, evtc.agents(), &addresses, offset)
        .unwrap_or(kept.log_end_time);
    Timing {
        offset,
        duration: end - offset,
    }
}

/// Damage totals of every squad player.
#[derive(Debug, Clone)]
pub struct PlayerDamage {
    /// The player.
    pub player: Player,
    /// Damage to foes over the whole log.
    pub totals: crate::damage::DamageTotals,
    /// Damage to the main target of the encounter, if the log has one.
    pub to_main_target: Option<crate::damage::DamageTotals>,
}

/// The damage of all squad players and the length of the fight.
#[derive(Debug, Clone)]
pub struct DamageAnalysis {
    /// Length of the fight the damage was summed over.
    pub duration: i64,
    /// Damage per player.
    pub players: Vec<PlayerDamage>,
}

/// Computes the damage of all squad players of a log.
///
/// # Errors
/// [`AnalysisError`] for unreadable logs or logs of unsupported arcdps builds.
pub fn analyze_damage(
    data: &[u8],
    api: &Api,
    timing: Option<Timing>,
) -> Result<DamageAnalysis, AnalysisError> {
    let evtc = Evtc::parse(data)?;
    let header = evtc.header();
    let records: Vec<_> = evtc.events().collect();
    let kept = keep_events(
        &records,
        header.trigger_id(),
        header.build(),
        KeepSettings::default(),
    )?;
    let players = find_players(evtc.agents(), &kept, &api.specs, false);
    let timing = timing.unwrap_or_else(|| core_timing(&kept, &evtc, &players));
    let ctx = crate::buff_events::LogContext {
        kept: &kept,
        gw2_build: kept.gw2_build,
        log_start: 0,
        time_offset: timing.offset,
        log_end: timing.duration,
    };
    let species = crate::damage::barrier_negated_species(header.trigger_id());
    let negated: std::collections::HashSet<u64> = evtc
        .agents()
        .iter()
        .filter(|a| a.is_elite() == 0xFFFF_FFFF && species.contains(&a.profession()))
        .map(|a| a.address())
        .collect();
    let main_target = crate::encounter::main_target(&kept, evtc.agents());
    let mut out = Vec::with_capacity(players.len());
    for player in players {
        let totals = crate::damage::actor_damage(&ctx, player.address, &negated, None)
            .map_err(|e| AnalysisError::Damage(e.to_string()))?;
        let to_main_target = main_target
            .as_ref()
            .map(|t| crate::damage::actor_damage(&ctx, player.address, &negated, Some(t)))
            .transpose()
            .map_err(|e| AnalysisError::Damage(e.to_string()))?;
        out.push(PlayerDamage {
            player,
            totals,
            to_main_target,
        });
    }
    Ok(DamageAnalysis {
        duration: timing.duration,
        players: out,
    })
}

/// The encounter offset the core determines for a log and the end of the log.
#[derive(Debug, Clone, Copy)]
pub struct OffsetAnalysis {
    /// `GetLogOffset` of the encounter, or `None` if the main target is not in the log.
    pub offset: Option<i64>,
    /// Time of the last record (before the offset is subtracted).
    pub log_end: i64,
    /// End of the fight as the encounter logic determines it (before the offset is subtracted).
    pub fight_end: Option<i64>,
    /// Trigger id of the log.
    pub trigger_id: u16,
}

/// Determines the encounter offset of a log.
///
/// # Errors
/// [`AnalysisError`] for unreadable logs.
pub fn analyze_offset(data: &[u8]) -> Result<OffsetAnalysis, AnalysisError> {
    let evtc = Evtc::parse(data)?;
    let header = evtc.header();
    let records: Vec<_> = evtc.events().collect();
    let kept = keep_events(
        &records,
        header.trigger_id(),
        header.build(),
        KeepSettings::default(),
    )?;
    let players = find_players(
        evtc.agents(),
        &kept,
        &crate::spec::SpecList::default(),
        false,
    );
    let addresses: Vec<u64> = players.iter().map(|p| p.address).collect();
    let offset = crate::encounter::log_offset(&kept, evtc.agents()).ok();
    Ok(OffsetAnalysis {
        offset,
        fight_end: crate::encounter::log_end(&kept, evtc.agents(), &addresses, offset.unwrap_or(0))
            .ok(),
        log_end: kept.log_end_time,
        trigger_id: kept.trigger_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_data_that_is_not_a_log() {
        assert!(matches!(
            analyze(b"hello", &Api::default()),
            Err(AnalysisError::Ingest(_))
        ));
    }

    #[test]
    fn a_log_without_combat_events_is_rejected_by_the_filter() {
        let mut data = b"EVTC20260920".to_vec();
        data.extend_from_slice(&[1, 0x45, 0x3C, 0]);
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        assert!(matches!(
            analyze(&data, &Api::default()),
            Err(AnalysisError::Keep(KeepError::NoEvents))
        ));
    }
}
