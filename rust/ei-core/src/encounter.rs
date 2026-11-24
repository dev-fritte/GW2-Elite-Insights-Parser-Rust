//! The start of the encounter logic: when the fight begins (`LogLogic.GetLogOffset`).
//!
//! Port of the generic rule of `LogLogic.GetLogOffset` and its helpers in `LogLogicTimeUtils`.
//! Encounters with their own rule are listed in [`log_offset`].

use std::collections::HashMap;

use evtc_ingest::AgentTable;

use crate::generated::state_change as sc;
use ei_data::skill_ids as skill;
use ei_data::species_ids as species;

use crate::keep::Kept;
use crate::players::{AgentKind, agent_kind};
use crate::rules::{dst_is_agent, src_is_agent};

/// `ParserHelper.TimeThresholdConstant`.
const TIME_THRESHOLD: i64 = 150;
/// `ParserHelper.MinimumInCombatDuration`.
const MINIMUM_IN_COMBAT_DURATION: i64 = 2200;
/// `ParserHelper.ServerDelayConstant`.
const SERVER_DELAY: i64 = 10;
/// `TargetID.Instance`.
const INSTANCE_ID: u32 = 2;

/// Why the offset of a log cannot be determined (`MissingKeyActorsException`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MissingKeyActors;

impl std::fmt::Display for MissingKeyActors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("main target not found")
    }
}

impl std::error::Error for MissingKeyActors {}

/// An agent of the table with its first and last time in the records.
#[derive(Debug, Clone, Copy)]
struct Agent {
    address: u64,
    first: i64,
    last: i64,
}

impl Agent {
    fn aware_at(&self, time: i64) -> bool {
        self.first <= time && self.last >= time
    }
}

/// First and last time of every agent address in the kept records (`UpdateAgentData`).
fn spans(kept: &Kept) -> HashMap<u64, (i64, i64)> {
    let redirect = |a: u64| *kept.agent_redirection.get(&a).unwrap_or(&a);
    let mut spans: HashMap<u64, (i64, i64)> = HashMap::new();
    let mut touch = |address: u64, time: i64| {
        let span = spans.entry(address).or_insert((time, time));
        span.0 = span.0.min(time);
        span.1 = span.1.max(time);
    };
    for e in &kept.events {
        if src_is_agent(e.state_change) && e.src_agent != 0 {
            touch(redirect(e.src_agent), e.time);
        }
        if dst_is_agent(e.state_change) && e.dst_agent != 0 {
            touch(redirect(e.dst_agent), e.time);
        }
    }
    spans
}

/// Non player characters of one species, ordered by first appearance.
fn species_agents(table: AgentTable<'_>, spans: &HashMap<u64, (i64, i64)>, id: u32) -> Vec<Agent> {
    let mut agents: Vec<Agent> = table
        .iter()
        .filter(|a| agent_kind(*a) == AgentKind::Npc && a.profession() == id)
        .filter_map(|a| {
            spans.get(&a.address()).map(|&(first, last)| Agent {
                address: a.address(),
                first,
                last,
            })
        })
        .collect();
    agents.sort_by_key(|a| a.first);
    agents
}

fn is_non_zero_damage(e: &evtc_ingest::CombatRecord) -> bool {
    if e.state_change != sc::COMBAT {
        return false;
    }
    if e.is_buff == 0 {
        e.value > 0
    } else {
        e.buff_dmg > 0
    }
}

fn matches_src(
    e: &evtc_ingest::CombatRecord,
    target: &Agent,
    redirect: &impl Fn(u64) -> u64,
) -> bool {
    src_is_agent(e.state_change)
        && e.src_agent != 0
        && redirect(e.src_agent) == target.address
        && target.aware_at(e.time)
}

fn matches_dst(
    e: &evtc_ingest::CombatRecord,
    target: &Agent,
    redirect: &impl Fn(u64) -> u64,
) -> bool {
    dst_is_agent(e.state_change)
        && e.dst_agent != 0
        && redirect(e.dst_agent) == target.address
        && target.aware_at(e.time)
}

/// `GetEnterCombatTime`: when the given species entered combat after the log start update.
fn enter_combat_time(
    kept: &Kept,
    table: AgentTable<'_>,
    spans: &HashMap<u64, (i64, i64)>,
    mut upper_limit: i64,
    ids: &[u32],
    agent: u64,
) -> Result<i64, MissingKeyActors> {
    let redirect = |a: u64| *kept.agent_redirection.get(&a).unwrap_or(&a);
    let mut start = i64::MAX;
    for &id in ids {
        let all = species_agents(table, spans, id);
        let target = all
            .iter()
            .find(|a| agent != 0 && a.address == agent)
            .or_else(|| all.iter().find(|a| a.aware_at(upper_limit)))
            .or_else(|| all.iter().find(|a| a.first > upper_limit))
            .copied()
            .ok_or(MissingKeyActors)?;
        // `GetPostLogStartNPCUpdateDamageEventTime`
        upper_limit = kept
            .events
            .iter()
            .find(|e| {
                (matches_src(e, &target, &redirect) || matches_dst(e, &target, &redirect))
                    && is_non_zero_damage(e)
                    && e.time >= upper_limit - SERVER_DELAY
            })
            .map_or(upper_limit, |e| e.time);
        let enter = kept.events.iter().find(|e| {
            e.state_change == sc::ENTER_COMBAT
                && matches_src(e, &target, &redirect)
                && e.time <= upper_limit + TIME_THRESHOLD
        });
        start = match enter {
            Some(enter) => {
                let exit = kept.events.iter().any(|e| {
                    e.state_change == sc::EXIT_COMBAT
                        && matches_src(e, &target, &redirect)
                        && e.time <= enter.time
                });
                start.min(if exit { target.first } else { enter.time })
            }
            None => start.min(target.first),
        };
    }
    Ok(if start == i64::MAX { 0 } else { start })
}

/// The generic `LogLogic.GetLogOffset`: 0 for instance logs, else the time the main target entered
/// combat, as far as the log start update tells.
fn generic_log_offset(
    kept: &Kept,
    table: AgentTable<'_>,
    spans: &HashMap<u64, (i64, i64)>,
    trigger_id: u32,
) -> Result<i64, MissingKeyActors> {
    if trigger_id == INSTANCE_ID {
        return Ok(0);
    }
    let Some(update) = kept
        .events
        .iter()
        .find(|e| e.state_change == sc::LOG_NPC_UPDATE)
    else {
        return Ok(0);
    };
    enter_combat_time(
        kept,
        table,
        spans,
        update.time,
        &[trigger_id],
        update.dst_agent,
    )
}

/// The first agent of a species (`AgentData.TryGetFirstAgentItem`).
fn first_agent(table: AgentTable<'_>, spans: &HashMap<u64, (i64, i64)>, id: i32) -> Option<Agent> {
    u32::try_from(id)
        .ok()
        .and_then(|id| species_agents(table, spans, id).first().copied())
}

/// Context shared by the encounter rules.
struct Ctx<'a> {
    kept: &'a Kept,
    table: AgentTable<'a>,
    spans: &'a HashMap<u64, (i64, i64)>,
}

impl Ctx<'_> {
    fn redirect(&self, a: u64) -> u64 {
        *self.kept.agent_redirection.get(&a).unwrap_or(&a)
    }

    fn first_with(
        &self,
        f: impl Fn(&evtc_ingest::CombatRecord) -> bool,
    ) -> Option<&evtc_ingest::CombatRecord> {
        self.kept.events.iter().find(|e| f(e))
    }

    fn src(&self, e: &evtc_ingest::CombatRecord, a: &Agent) -> bool {
        matches_src(e, a, &|x| self.redirect(x))
    }

    fn dst(&self, e: &evtc_ingest::CombatRecord, a: &Agent) -> bool {
        matches_dst(e, a, &|x| self.redirect(x))
    }

    fn agent(&self, id: i32) -> Result<Agent, MissingKeyActors> {
        first_agent(self.table, self.spans, id).ok_or(MissingKeyActors)
    }
}

/// `Cairn.GetLogOffset`: loss of the spawn protection. The fallback through the first end of a cast
/// is not ported; the generic start is used instead.
fn cairn_offset(c: &Ctx<'_>) -> Result<i64, MissingKeyActors> {
    let cairn = c.agent(species::target_id::CAIRN)?;
    Ok(c.first_with(|e| {
        e.state_change == sc::BUFF_REMOVE_ALL
            && i64::from(e.skill_id) == skill::SPAWN_PROTECTION
            && c.src(e, &cairn)
    })
    .map_or(0, |e| e.time))
}

/// `Xera.GetLogOffset`.
fn xera_offset(c: &Ctx<'_>) -> Result<i64, MissingKeyActors> {
    let xera = c.agent(species::target_id::XERA)?;
    let Some(enter) = c.first_with(|e| e.state_change == sc::ENTER_COMBAT && c.src(e, &xera))
    else {
        return Ok(0);
    };
    let Ok(fake) = c.agent(species::target_id::FAKE_XERA) else {
        return Ok(enter.time);
    };
    let last = |state: u8| {
        c.kept
            .events
            .iter()
            .rev()
            .find(|e| e.state_change == state && c.src(e, &fake))
    };
    Ok(if let Some(death) = last(sc::CHANGE_DEAD) {
        death.time + 1000
    } else if let Some(exit) = last(sc::EXIT_COMBAT) {
        exit.time + 1000
    } else {
        fake.last
    })
}

/// `Sabir.GetLogOffset`.
fn sabir_offset(c: &Ctx<'_>) -> Result<i64, MissingKeyActors> {
    let sabir = c.agent(species::target_id::SABIR)?;
    if let Some(enter) = c.first_with(|e| e.state_change == sc::ENTER_COMBAT && c.src(e, &sabir)) {
        return Ok(enter.time);
    }
    if c.first_with(|e| e.state_change == sc::LOG_NPC_UPDATE)
        .is_none()
    {
        return Ok(0);
    }
    Ok(c.first_with(|e| c.dst(e, &sabir) && is_non_zero_damage(e))
        .map_or(c.kept.log_end_time, |e| e.time))
}

/// `Arkk.GetLogOffset`: the start buff, else the spawn.
fn arkk_offset(c: &Ctx<'_>) -> Result<i64, MissingKeyActors> {
    let arkk = c.agent(species::target_id::ARKK)?;
    let start_buff = c.first_with(|e| {
        i64::from(e.skill_id) == skill::ARKK_START_BUFF
            && matches!(
                e.state_change,
                sc::BUFF_APPLY | sc::BUFF_CHANGE | sc::BUFF_INITIAL
            )
            && c.src(e, &arkk)
    });
    if let Some(e) = start_buff {
        return Ok(e.time);
    }
    // `GetLogOffsetBySpawn`
    let start = arkk.first.max(0);
    Ok(c.first_with(|e| {
        e.state_change == sc::SPAWN && c.src(e, &arkk) && e.time <= start + TIME_THRESHOLD
    })
    .map_or(start, |spawn| start.max(spawn.time)))
}

/// `Escort.GetLogOffset`.
fn escort_offset(c: &Ctx<'_>) -> Result<i64, MissingKeyActors> {
    let mcleod = c.agent(species::target_id::MC_LEOD_THE_SILENT)?;
    let Some(update) = c.first_with(|e| e.state_change == sc::LOG_NPC_UPDATE) else {
        return Ok(0);
    };
    if mcleod.first > MINIMUM_IN_COMBAT_DURATION {
        return Ok(0);
    }
    enter_combat_time(
        c.kept,
        c.table,
        c.spans,
        update.time,
        &[16253],
        update.dst_agent,
    )
}

/// `Dhuum.GetLogOffset`.
#[allow(clippy::unnecessary_wraps)]
fn dhuum_offset(c: &Ctx<'_>) -> Result<i64, MissingKeyActors> {
    if c.first_with(|e| e.state_change == sc::LOG_NPC_UPDATE)
        .is_none()
    {
        return Ok(0);
    }
    let orbs = [
        skill::DHUUM_ENFORCER_ORB,
        skill::DHUUM_MESSENGER_ORB,
        skill::DHUUM_SPIDER_ORB,
        skill::DHUUM_COLLECTABLE_SMALL_ORB,
    ];
    let has_missiles = c.kept.events.iter().any(|e| {
        matches!(
            e.state_change,
            sc::MISSILE_CREATE | sc::MISSILE_LAUNCH | sc::MISSILE_REMOVE
        )
    });
    if has_missiles {
        if let Some(orb) = c.first_with(|e| {
            e.state_change == sc::MISSILE_CREATE && orbs.contains(&i64::from(e.skill_id))
        }) {
            return Ok(orb.time);
        }
        return Ok(0);
    }
    let first = species_agents(c.table, c.spans, 19807)
        .into_iter()
        .map(|a| a.first)
        .min();
    Ok(first.unwrap_or(0))
}

/// An encounter specific `GetLogOffset`.
type Rule = fn(&Ctx<'_>) -> Result<i64, MissingKeyActors>;

/// The encounters that override `GetLogOffset` and are ported, by trigger id.
fn rule_for(trigger_id: u16) -> Option<Rule> {
    use species::target_id as id;
    let trigger = i32::from(trigger_id);
    Some(match trigger {
        id::CAIRN => cairn_offset,
        id::XERA => xera_offset,
        id::SABIR => sabir_offset,
        id::ARKK => arkk_offset,
        id::MC_LEOD_THE_SILENT => escort_offset,
        id::DHUUM => dhuum_offset,
        _ => return None,
    })
}

/// The offset of the encounter: the time of `kept` the fight counts from.
///
/// Only the generic rule is ported; encounters that override `GetLogOffset` in C# give the generic
/// answer here, which is wrong for them (see `docs/phase7/STATUS.md`).
///
/// # Errors
/// [`MissingKeyActors`] if the main target of the encounter is not in the log.
pub fn log_offset(kept: &Kept, table: AgentTable<'_>) -> Result<i64, MissingKeyActors> {
    let spans = spans(kept);
    let c = Ctx {
        kept,
        table,
        spans: &spans,
    };
    match rule_for(kept.trigger_id) {
        Some(rule) => rule(&c),
        None => generic_log_offset(kept, table, &spans, u32::from(kept.trigger_id)),
    }
}

/// First instance id of every agent address that is seen in the records.
fn instance_ids(kept: &Kept) -> HashMap<u64, u16> {
    let redirect = |a: u64| *kept.agent_redirection.get(&a).unwrap_or(&a);
    let mut ids: HashMap<u64, u16> = HashMap::new();
    for e in &kept.events {
        if src_is_agent(e.state_change) && e.src_agent != 0 && e.src_instid != 0 {
            ids.entry(redirect(e.src_agent)).or_insert(e.src_instid);
        }
        if dst_is_agent(e.state_change) && e.dst_agent != 0 && e.dst_instid != 0 {
            ids.entry(redirect(e.dst_agent)).or_insert(e.dst_instid);
        }
    }
    ids
}

/// The end of the fight in the time of `kept` (`LogData.SetSuccess`): by the death of the main
/// target if the players killed it, else by the last time the target is known.
///
/// Not ported: success through a chest gadget, combat exit and the rules of single encounters.
///
/// # Errors
/// [`MissingKeyActors`] if the main target is not in the log.
pub fn log_end(
    kept: &Kept,
    table: AgentTable<'_>,
    player_addresses: &[u64],
) -> Result<i64, MissingKeyActors> {
    let spans = spans(kept);
    let c = Ctx {
        kept,
        table,
        spans: &spans,
    };
    let target = c.agent(i32::from(kept.trigger_id))?;
    let instances = instance_ids(kept);
    let player_instances: Vec<u16> = player_addresses
        .iter()
        .filter_map(|a| instances.get(a).copied())
        .collect();
    let from_players = |e: &evtc_ingest::CombatRecord| {
        player_addresses.contains(&c.redirect(e.src_agent))
            || (e.src_master_instid != 0 && player_instances.contains(&e.src_master_instid))
    };
    let death = kept
        .events
        .iter()
        .rev()
        .find(|e| e.state_change == sc::CHANGE_DEAD && c.src(e, &target));
    if let Some(death) = death {
        let last_damage = kept.events.iter().rev().find(|e| {
            e.state_change == sc::COMBAT
                && e.dst_agent != 0
                && c.redirect(e.dst_agent) == target.address
                && crate::damage::health_damage(e) > 0
                && from_players(e)
        });
        let time = last_damage.map_or(death.time, |d| d.time.min(death.time));
        return Ok((time + SERVER_DELAY).min(kept.log_end_time));
    }
    Ok(target.last)
}

#[cfg(test)]
mod tests {
    use super::*;
    use evtc_ingest::CombatRecord;

    fn record(is_buff: u8, value: i32, buff_dmg: i32) -> CombatRecord {
        CombatRecord {
            time: 0,
            src_agent: 0,
            dst_agent: 0,
            value,
            buff_dmg,
            overstack_value: 0,
            skill_id: 0,
            src_instid: 0,
            dst_instid: 0,
            src_master_instid: 0,
            dst_master_instid: 0,
            iff: 0,
            is_buff,
            result: 0,
            is_activation: 0,
            is_buff_remove: 0,
            is_ninety: 0,
            is_fifty: 0,
            is_moving: 0,
            state_change: sc::COMBAT,
            is_flanking: 0,
            is_shields: 0,
            is_offcycle: 0,
            pad: 0,
        }
    }

    #[test]
    fn non_zero_damage_looks_at_the_value_of_the_record_kind() {
        assert!(is_non_zero_damage(&record(0, 5, 0)));
        assert!(!is_non_zero_damage(&record(0, 0, 5)));
        assert!(is_non_zero_damage(&record(1, 0, 5)));
        assert!(!is_non_zero_damage(&record(1, 5, 0)));
    }

    #[test]
    fn only_ported_encounters_have_a_rule() {
        assert!(rule_for(17194).is_some());
        assert!(rule_for(15438).is_none());
    }

    #[test]
    fn a_span_is_aware_inside_its_bounds() {
        let agent = Agent {
            address: 1,
            first: 10,
            last: 20,
        };
        assert!(agent.aware_at(10) && agent.aware_at(20) && !agent.aware_at(21));
    }
}
