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
/// `RewardIDs` of the chests of the Icebrood Saga strikes.
const ICEBROOD_REWARDS: [u64; 8] = [993, 1035, 1028, 1091, 1032, 1007, 1052, 1031];
/// `RewardTypes.OldRaidReward2`.
const OLD_RAID_REWARD_2: i32 = 60_685;
/// `RewardTypes.CurrentRaidReward`.
const CURRENT_RAID_REWARD: i32 = 22_797;
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
pub(crate) fn spans(kept: &Kept) -> HashMap<u64, (i64, i64)> {
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
        .filter(|a| match agent_kind(*a) {
            AgentKind::Npc => a.profession() == id,
            // gadgets carry their species in the low half of the profession
            AgentKind::Gadget => a.profession() & 0xFFFF == id,
            _ => false,
        })
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
    /// The encounter offset that is subtracted from the time of records.
    offset: i64,
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

/// `GetLogOffsetForTarget`: the start of the target, but not before the log start.
fn offset_for_target(target: &Agent) -> i64 {
    target.first.max(0)
}

/// `GetLogOffsetBySpawn`.
fn offset_by_spawn(c: &Ctx<'_>, target: &Agent) -> i64 {
    let start = offset_for_target(target);
    c.first_with(|e| {
        e.state_change == sc::SPAWN && c.src(e, target) && e.time <= start + TIME_THRESHOLD
    })
    .map_or(start, |spawn| start.max(spawn.time))
}

/// `GetEnterCombatForStart`: the first combat entry of the target from `start` on, if it is not
/// followed by an earlier exit.
fn enter_combat_for_start(c: &Ctx<'_>, target: &Agent, start: i64) -> Option<i64> {
    let enter = c.first_with(|e| {
        e.state_change == sc::ENTER_COMBAT && c.src(e, target) && e.time >= start
    })?;
    let exit =
        c.first_with(|e| e.state_change == sc::EXIT_COMBAT && c.src(e, target) && e.time >= start);
    exit.is_none_or(|x| x.time > enter.time)
        .then_some(enter.time)
}

/// `GetLogOffsetByInvulnStart`: the end of the invulnerability a target spawns with.
fn offset_by_invuln_start(c: &Ctx<'_>, target: &Agent, invuln: i64) -> i64 {
    let start = offset_for_target(target);
    let Some(remove) = c.first_with(|e| {
        i64::from(e.skill_id) == invuln
            && e.state_change == sc::BUFF_REMOVE_ALL
            && c.src(e, target)
            && e.time >= start
    }) else {
        return start;
    };
    let apply = c.first_with(|e| {
        i64::from(e.skill_id) == invuln
            && c.dst(e, target)
            && matches!(
                e.state_change,
                sc::BUFF_APPLY | sc::BUFF_CHANGE | sc::BUFF_INITIAL
            )
            && e.time >= start
    });
    match apply {
        Some(apply) if apply.time <= remove.time => {
            // applied at the start rather than already present
            if apply.state_change != sc::BUFF_INITIAL && apply.time <= target.first + TIME_THRESHOLD
            {
                return remove.time;
            }
            if enter_combat_for_start(c, target, start)
                .is_some_and(|enter| enter + SERVER_DELAY >= apply.time)
            {
                return remove.time;
            }
        }
        _ => {
            if enter_combat_for_start(c, target, start).is_some_and(|enter| enter >= remove.time) {
                return remove.time;
            }
        }
    }
    start
}

/// The generic rule with the context of the encounter rules.
fn generic(c: &Ctx<'_>) -> Result<i64, MissingKeyActors> {
    generic_log_offset(c.kept, c.table, c.spans, u32::from(c.kept.trigger_id))
}

/// The enter combat time of the species in `ids` after the log start update, 0 without update.
fn enter_combat_after_update(c: &Ctx<'_>, ids: &[u32]) -> Result<i64, MissingKeyActors> {
    match c.first_with(|e| e.state_change == sc::LOG_NPC_UPDATE) {
        Some(update) => {
            enter_combat_time(c.kept, c.table, c.spans, update.time, ids, update.dst_agent)
        }
        None => Ok(0),
    }
}

/// `River.GetLogOffset`: the first enervator.
fn river_offset(c: &Ctx<'_>) -> Result<i64, MissingKeyActors> {
    if c.first_with(|e| e.state_change == sc::LOG_NPC_UPDATE)
        .is_none()
    {
        return Ok(0);
    }
    species_agents(c.table, c.spans, 19863)
        .first()
        .map(|a| a.first)
        .ok_or(MissingKeyActors)
}

/// `Ensolyss.GetLogOffset`.
fn ensolyss_offset(c: &Ctx<'_>) -> Result<i64, MissingKeyActors> {
    let ensolyss = c.agent(species::target_id::ENSOLYSS)?;
    let start = offset_by_invuln_start(c, &ensolyss, skill::DETERMINED762);
    let caustic = c.first_with(|e| {
        i64::from(e.skill_id) == skill::CAUSTIC_EXPLOSION_ENSOLYSS
            && e.state_change == sc::ANIMATION_START
            && c.src(e, &ensolyss)
            && e.time > start
    });
    if caustic.is_some_and(|e| e.time <= start + 3000) {
        return Ok(0);
    }
    Ok(start)
}

/// `MAMA.GetLogOffset`: the generic start, refined by MAMA entering combat.
fn mama_offset(c: &Ctx<'_>) -> Result<i64, MissingKeyActors> {
    let start = generic(c)?;
    let Some(mama) = species_agents(c.table, c.spans, 17021).first().copied() else {
        return Ok(start);
    };
    Ok(
        c.first_with(|e| c.src(e, &mama) && e.state_change == sc::ENTER_COMBAT && e.time >= start)
            .map_or(start, |e| e.time),
    )
}

/// `Siax.GetLogOffset`.
fn siax_offset(c: &Ctx<'_>) -> Result<i64, MissingKeyActors> {
    let siax = c.agent(species::target_id::SIAX)?;
    Ok(offset_by_spawn(c, &siax))
}

/// `Skorvald.GetLogOffset`.
fn skorvald_offset(c: &Ctx<'_>) -> Result<i64, MissingKeyActors> {
    let Some(update) = c.first_with(|e| e.state_change == sc::LOG_NPC_UPDATE) else {
        return Ok(0);
    };
    let skorvald = c.agent(species::target_id::SKORVALD)?;
    let upper = c
        .kept
        .events
        .iter()
        .find(|e| {
            (c.src(e, &skorvald) || c.dst(e, &skorvald))
                && is_non_zero_damage(e)
                && e.time >= update.time - SERVER_DELAY
        })
        .map_or(update.time, |e| e.time);
    // percentage above zero: the stored value is hundredths of a percent
    let health = c.first_with(|e| {
        e.state_change == sc::HEALTH_UPDATE && c.src(e, &skorvald) && e.dst_agent / 100 > 0
    });
    let enter = c.first_with(|e| {
        e.state_change == sc::ENTER_COMBAT && c.src(e, &skorvald) && e.time <= upper + SERVER_DELAY
    });
    Ok(match health {
        Some(h) => h.time.min(enter.map_or(i64::MAX, |e| e.time)),
        None => 0,
    })
}

/// `Kanaxai.GetLogOffset`.
fn kanaxai_offset(c: &Ctx<'_>) -> Result<i64, MissingKeyActors> {
    let kanaxai = c.agent(species::target_id::KANAXAI_SCYTHE_OF_HOUSE_AURKUS_CM)?;
    Ok(offset_by_invuln_start(c, &kanaxai, skill::DETERMINED762))
}

/// `GreerTheBlightbringer.GetLogOffset`.
fn greer_offset(c: &Ctx<'_>) -> Result<i64, MissingKeyActors> {
    enter_combat_after_update(c, &[26725, 26771, 26742])
}

/// `UraTheSteamshrieker.GetLogOffset`.
fn ura_offset(c: &Ctx<'_>) -> Result<i64, MissingKeyActors> {
    if c.first_with(|e| e.state_change == sc::LOG_NPC_UPDATE)
        .is_none()
    {
        return Ok(0);
    }
    let start = enter_combat_after_update(c, &[u32::from(c.kept.trigger_id)])?;
    // the moment two deterrence buffs are active at once
    let mut active: Vec<(u64, i64)> = Vec::new();
    for e in &c.kept.events {
        if i64::from(e.skill_id) != skill::DETERRENCE {
            continue;
        }
        if matches!(
            e.state_change,
            sc::BUFF_APPLY | sc::BUFF_CHANGE | sc::BUFF_INITIAL
        ) {
            active.retain(|a| a.0 != e.dst_agent);
            active.push((e.dst_agent, e.time));
        } else if matches!(e.state_change, sc::BUFF_REMOVE_ALL | sc::BUFF_REMOVE_SINGLE) {
            active.retain(|a| a.0 != e.src_agent);
        } else {
            continue;
        }
        if active.len() == 2 {
            break;
        }
    }
    Ok(if active.len() == 2 {
        start.min(active.iter().map(|a| a.1).max().unwrap_or(start))
    } else {
        start
    })
}

/// `GuardiansGlade.GetLogOffset`.
fn kela_offset(c: &Ctx<'_>) -> Result<i64, MissingKeyActors> {
    if c.first_with(|e| e.state_change == sc::LOG_NPC_UPDATE)
        .is_none()
    {
        return Ok(0);
    }
    let enter = enter_combat_after_update(c, &[u32::from(c.kept.trigger_id)])?;
    let fixation = c.first_with(|e| {
        i64::from(e.skill_id) == skill::FIXATED_KELA
            && matches!(
                e.state_change,
                sc::BUFF_APPLY | sc::BUFF_CHANGE | sc::BUFF_INITIAL
            )
    });
    Ok(match fixation {
        Some(f) if f.time <= enter => f.time,
        _ => enter,
    })
}

/// `NexusOfEternity.GetLogOffset`.
fn vloxx_offset(c: &Ctx<'_>) -> Result<i64, MissingKeyActors> {
    let Some(update) = c.first_with(|e| e.state_change == sc::LOG_NPC_UPDATE) else {
        return Ok(0);
    };
    let start = enter_combat_after_update(c, &[u32::from(c.kept.trigger_id)])?;
    let vloxx = species_agents(c.table, c.spans, 28106)
        .into_iter()
        .find(|a| a.address == update.dst_agent)
        .ok_or(MissingKeyActors)?;
    let targetable =
        c.first_with(|e| e.state_change == sc::TARGETABLE && c.src(e, &vloxx) && e.dst_agent > 0);
    Ok(targetable.map_or(start, |t| start.min(t.time)))
}

/// `GetFirstDamageEventTime`: the first damage event that involves the target.
fn first_damage_time(c: &Ctx<'_>, target: &Agent) -> i64 {
    c.first_with(|e| (c.src(e, target) || c.dst(e, target)) && is_non_zero_damage(e))
        .map_or(target.first, |e| e.time)
}

/// `StatueOfIce.GetLogOffset`: the first cast of the Broken King.
fn broken_king_offset(c: &Ctx<'_>) -> Result<i64, MissingKeyActors> {
    let king = c.agent(species::target_id::BROKEN_KING)?;
    if c.first_with(|e| e.state_change == sc::LOG_NPC_UPDATE)
        .is_none()
    {
        return Ok(0);
    }
    let cast = c.first_with(|e| {
        e.state_change == sc::ANIMATION_START
            && i64::from(e.skill_id) == skill::BROKEN_KING_FIRST_CAST
            && c.src(e, &king)
    });
    Ok(cast.map_or_else(|| first_damage_time(c, &king), |e| e.time))
}

/// `Qadim.GetLogOffset`: the initial cast, if a flame attack follows it.
fn qadim_offset(c: &Ctx<'_>) -> Result<i64, MissingKeyActors> {
    c.agent(species::target_id::QADIM)?;
    let start = c.first_with(|e| {
        i64::from(e.skill_id) == skill::QADIM_INITIAL_CAST && e.state_change == sc::ANIMATION_START
    });
    let check = c.first_with(|e| {
        matches!(
            i64::from(e.skill_id),
            skill::FLAME_SLASH3 | skill::FLAME_SLASH | skill::FLAME_WAVE
        ) && e.state_change == sc::ANIMATION_START
    });
    let (Some(start), Some(check)) = (start, check) else {
        return Ok(0);
    };
    Ok(if check.time - start.time > 0 {
        start.time
    } else {
        0
    })
}

/// `OldLionsCourt.GetLogOffset`: the generic start, moved to the moment Vermilion's breakbar
/// becomes active.
fn old_lions_court_offset(c: &Ctx<'_>) -> Result<i64, MissingKeyActors> {
    let start = generic(c)?;
    let vermilion = first_agent(c.table, c.spans, species::target_id::PROTOTYPE_VERMILION_CM)
        .or_else(|| first_agent(c.table, c.spans, species::target_id::PROTOTYPE_VERMILION));
    let Some(vermilion) = vermilion else {
        return Ok(start);
    };
    // breakbar state 0 is `Active`
    let active = c.first_with(|e| {
        e.state_change == sc::BREAKBAR_STATE && c.src(e, &vermilion) && e.value == 0
    });
    Ok(active.map_or(start, |e| e.time))
}

/// `SpiritRace.GetLogOffset`: the first movement of the wall of ghosts.
#[allow(clippy::unnecessary_wraps)]
fn spirit_race_offset(c: &Ctx<'_>) -> Result<i64, MissingKeyActors> {
    let wall = first_agent(c.table, c.spans, species::target_id::WALL_OF_GHOSTS);
    let Some(wall) = wall else {
        return Ok(0);
    };
    let moving =
        c.first_with(|e| e.state_change == sc::VELOCITY && c.src(e, &wall) && e.dst_agent != 0);
    Ok(moving.map_or(0, |e| e.time))
}

/// `TempleOfFebe.GetLogOffset`: the spawn of Cerus, or his first appearance.
fn cerus_offset(c: &Ctx<'_>) -> Result<i64, MissingKeyActors> {
    if c.first_with(|e| e.state_change == sc::LOG_NPC_UPDATE)
        .is_none()
    {
        return Ok(0);
    }
    let enter = enter_combat_after_update(c, &[u32::from(c.kept.trigger_id)])?;
    let cerus = c.agent(25989)?;
    let spawn = c.first_with(|e| e.state_change == sc::SPAWN && c.src(e, &cerus));
    Ok(match spawn {
        Some(s) if enter >= s.time => s.time,
        _ => cerus.first,
    })
}

/// `Deimos.GetShackledPrisonerFirstOrDefault`: the first agent with the maximum health of the
/// shackled prisoner.
fn shackled_prisoner(c: &Ctx<'_>) -> Option<Agent> {
    let update = c.first_with(|e| {
        e.state_change == sc::MAX_HEALTH_UPDATE
            && e.dst_agent == 1_000_980
            && c.spans.contains_key(&c.redirect(e.src_agent))
            && c.table
                .iter()
                .any(|a| a.address() == c.redirect(e.src_agent) && a.profession() != 0)
    })?;
    let address = c.redirect(update.src_agent);
    let &(first, last) = c.spans.get(&address)?;
    Some(Agent {
        address,
        first,
        last,
    })
}

/// `Deimos.GetLogOffset`: the loss of his spawn protection, or the shackled prisoner's first health
/// update if the pre event ran before.
fn deimos_offset(c: &Ctx<'_>) -> Result<i64, MissingKeyActors> {
    /// `PreEventConsiderationConstant`.
    const PRE_EVENT: i64 = 5000;
    let deimos = c.agent(species::target_id::DEIMOS)?;
    let Some(remove) = c.first_with(|e| {
        e.state_change == sc::BUFF_REMOVE_ALL
            && i64::from(e.skill_id) == skill::SPAWN_PROTECTION
            && c.dst(e, &deimos)
    }) else {
        return Ok(0);
    };
    let mut start = remove.time;
    if start > PRE_EVENT
        && let Some(prisoner) = shackled_prisoner(c)
    {
        let green = c.first_with(|e| {
            e.state_change == sc::BUFF_APPLY
                && i64::from(e.skill_id) == skill::DEIMOS_SELECTED_BY_GREEN
        });
        let health = c.first_with(|e| e.state_change == sc::HEALTH_UPDATE && c.src(e, &prisoner));
        if let (Some(green), Some(health)) = (green, health)
            && green.time < start
            && health.dst_agent == 10_000
        {
            start = health.time;
        }
    }
    Ok(start)
}

/// `BanditTrio.GetLogOffset`: the first appearance of Berg, Zane and Narella. The C# check for
/// bandits in the box in front of the trio never matches (`IsInBoundingBox` compares against the
/// first corner twice, so the box is a point), and is left out.
fn bandit_trio_offset(c: &Ctx<'_>) -> Result<i64, MissingKeyActors> {
    use species::target_id as id;
    if c.first_with(|e| e.state_change == sc::LOG_NPC_UPDATE)
        .is_none()
    {
        return Ok(0);
    }
    let mut start = i64::MAX;
    for trio in [id::BERG, id::ZANE, id::NARELLA] {
        start = start.min(c.agent(trio)?.first);
    }
    Ok(start)
}

/// The GUID an `ID_TO_GUID` record carries, in the byte order of `effect_guids`.
fn record_guid(e: &evtc_ingest::CombatRecord) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&e.src_agent.to_le_bytes());
    bytes[8..].copy_from_slice(&e.dst_agent.to_le_bytes());
    bytes
}

/// Whether the record is an effect record.
fn is_effect(e: &evtc_ingest::CombatRecord) -> bool {
    matches!(
        e.state_change,
        sc::EFFECT_45
            | sc::EFFECT_51
            | sc::EFFECT_AGENT_CREATE
            | sc::EFFECT_AGENT_REMOVE
            | sc::EFFECT_GROUND_CREATE
            | sc::EFFECT_GROUND_REMOVE
            | sc::EFFECT_MISSILE_CREATE
    )
}

/// `ConjuredAmalgamate.GetLogOffset`: the first arm smash effect, found through the GUID table of
/// the log.
fn conjured_amalgamate_offset(c: &Ctx<'_>) -> Result<i64, MissingKeyActors> {
    let smash = c
        .first_with(|e| {
            e.state_change == sc::ID_TO_GUID
                && record_guid(e) == ei_data::effect_guids::CA_ARM_SMASH
        })
        .and_then(|guid| c.first_with(|e| is_effect(e) && e.skill_id == guid.skill_id));
    let Some(smash) = smash else {
        return generic(c);
    };
    match c.first_with(|e| e.state_change == sc::LOG_NPC_UPDATE) {
        Some(update) => {
            let ca = c.agent(species::target_id::CONJURED_AMALGAMATE)?;
            let first_damage = c
                .first_with(|e| {
                    (c.src(e, &ca) || c.dst(e, &ca))
                        && is_non_zero_damage(e)
                        && e.time >= update.time - SERVER_DELAY
                })
                .map_or(update.time, |e| e.time);
            // we couldn't have hit CA before the initial smash
            Ok(if smash.time > first_damage {
                update.time
            } else {
                smash.time
            })
        }
        None => Ok(if smash.time > 6000 { 0 } else { smash.time }),
    }
}

/// `HarvestTemple.GetLogOffset`: the first void amalgamate.
#[allow(clippy::unnecessary_wraps)]
fn harvest_temple_offset(c: &Ctx<'_>) -> Result<i64, MissingKeyActors> {
    if c.first_with(|e| e.state_change == sc::LOG_NPC_UPDATE)
        .is_none()
    {
        return Ok(0);
    }
    let first = species_agents(c.table, c.spans, 24_375)
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
        19828 => river_offset,
        id::ENSOLYSS => ensolyss_offset,
        id::MAMA => mama_offset,
        id::SIAX => siax_offset,
        id::SKORVALD => skorvald_offset,
        id::KANAXAI_SCYTHE_OF_HOUSE_AURKUS_CM => kanaxai_offset,
        id::GREER => greer_offset,
        26712 => ura_offset,
        27124 => kela_offset,
        id::VLOXX => vloxx_offset,
        id::BROKEN_KING => broken_king_offset,
        id::QADIM => qadim_offset,
        id::PROTOTYPE_VERMILION | id::PROTOTYPE_VERMILION_CM => old_lions_court_offset,
        id::ETHEREAL_BARRIER_GADGET => spirit_race_offset,
        25989 => cerus_offset,
        id::DEIMOS => deimos_offset,
        id::BERG => bandit_trio_offset,
        43_974 => conjured_amalgamate_offset,
        43_488 => harvest_temple_offset,
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
        offset: 0,
    };
    match rule_for(kept.trigger_id) {
        Some(rule) => rule(&c),
        None => generic_log_offset(kept, table, &spans, u32::from(kept.trigger_id)),
    }
}

/// The reward chest of an encounter, by the trigger id of the log (`LogLogic.ChestID`), as the
/// position the chest gadget stands at.
fn chest_position(trigger_id: u16) -> Option<[f32; 3]> {
    use ei_data::species_ids::positions as p;
    Some(match trigger_id {
        15_438 => p::GUARDIAN_CHEST_POSITION,
        15_429 => p::GORSEVAL_CHEST_POSITION,
        15_375 => p::SABETHA_CHEST_POSITION,
        16_123 => p::SLOTHASOR_CHEST_POSITION,
        16_088 => p::CHEST_OF_PRISON_CAMP_POSITION,
        16_115 => p::MATTHIAS_CHEST_POSITION,
        16_253 => p::SIEGE_CHEST_POSITION,
        16_235 => p::KEEP_CONSTRUCT_CHEST_POSITION,
        16_246 => p::XERA_CHEST_POSITION,
        17_194 => p::CAIRN_CHEST_POSITION,
        17_154 => p::SAULS_TREASURE_CHEST_POSITION,
        17_172 => p::RECREATION_ROOM_CHEST_POSITION,
        17_188 => p::SAMAROG_CHEST_POSITION,
        19_450 => p::DHUUM_CHEST_POSITION,
        19_828 => p::CHEST_OF_SOULS_POSITION,
        19_767 => p::CHEST_OF_DESMINA_POSITION,
        43_974 => p::CA_CHEST_POSITION,
        20_934 => p::QADIMS_CHEST_POSITION,
        21_105 => p::TWIN_LARGOS_CHEST_POSITION,
        22_006 => p::ADINAS_CHEST_POSITION,
        22_000 => p::QADIM_THE_PEERLESS_CHEST_POSITION,
        21_964 => p::SABIRS_CHEST_POSITION,
        26_774 => p::DECIMAS_CHEST_POSITION,
        26_725 => p::GREERS_CHEST_POSITION,
        26_712 => p::URAS_CHEST_POSITION,
        _ => return None,
    })
}

/// `FindChestGadgets`: the gadget that stands at the chest position, never moves and has the
/// chest hitbox. Returns the first time the gadget is known (`SetSuccessByChestGadget`).
fn chest_gadget_time(c: &Ctx<'_>, position: [f32; 3], hitbox_width: u32) -> Option<i64> {
    // `InchDistanceThresholdSquared`
    const THRESHOLD_SQUARED: f32 = std::f32::consts::SQRT_2 * std::f32::consts::SQRT_2;
    let xy = |dst: u64| {
        let bytes = dst.to_le_bytes();
        (
            f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            f32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
        )
    };
    let gadgets: Vec<Agent> = c
        .table
        .iter()
        .filter(|a| agent_kind(*a) == AgentKind::Gadget && a.hitbox_width() == hitbox_width)
        .filter_map(|a| {
            c.spans.get(&a.address()).map(|&(first, last)| Agent {
                address: a.address(),
                first,
                last,
            })
        })
        .collect();
    let moving = |address: u64| {
        c.kept.events.iter().any(|e| {
            e.state_change == sc::VELOCITY && c.redirect(e.src_agent) == address && {
                let (x, y) = xy(e.dst_agent);
                let z = f32::from_bits(e.value.cast_unsigned());
                (x * x + y * y + z * z).sqrt() >= 1e-6
            }
        })
    };
    let at_position = |address: u64| {
        c.kept.events.iter().any(|e| {
            e.state_change == sc::POSITION && c.redirect(e.src_agent) == address && {
                let (x, y) = xy(e.dst_agent);
                (x - position[0]).powi(2) + (y - position[1]).powi(2) < THRESHOLD_SQUARED
            }
        })
    };
    gadgets
        .iter()
        .filter(|g| at_position(g.address) && !moving(g.address))
        .map(|g| g.first)
        .min()
}

/// The end of the fight and whether the encounter was a success (`LogLogic.CheckSuccess`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct End {
    success: bool,
    time: i64,
}

/// The apply events of a buff on an agent, in record order (`BuffApplyEvent`s).
fn buff_applies<'a>(
    c: &'a Ctx<'a>,
    buff: i64,
    target: &'a Agent,
) -> impl Iterator<Item = &'a evtc_ingest::CombatRecord> {
    c.kept.events.iter().filter(move |e| {
        i64::from(e.skill_id) == buff
            && matches!(e.state_change, sc::BUFF_APPLY | sc::BUFF_INITIAL)
            && c.dst(e, target)
    })
}

/// The generic success: reward chest, then the death of the target, else a failure at the last
/// time the target is known.
fn generic_end(c: &Ctx<'_>, target: &Agent, player_addresses: &[u64]) -> End {
    let kept = c.kept;
    // Icebrood Saga strikes: the reward of one of their chests (IcebroodSagaRaidEncounter)
    if matches!(
        i32::from(kept.trigger_id),
        22_154 | 22_492 | 22_343 | 22_481 | 22_521 | 22_711
    ) && let Some(reward) = c.first_with(|e| {
        e.state_change == sc::REWARD
            && ICEBROOD_REWARDS.contains(&e.dst_agent)
            && e.time - c.offset > 0
    }) {
        return End {
            success: true,
            time: reward.time,
        };
    }
    // raid wings: the weekly reward record ends the fight (RaidWingLogic.CheckSuccess)
    if chest_position(kept.trigger_id).is_some()
        && let Some(reward) = c.first_with(|e| {
            e.state_change == sc::REWARD && e.value == CURRENT_RAID_REWARD && e.time - c.offset > 0
        })
    {
        return End {
            success: true,
            time: reward.time,
        };
    }
    if let Some(position) = chest_position(kept.trigger_id)
        && let Some(time) = chest_gadget_time(c, position, 100)
    {
        return End {
            success: true,
            time,
        };
    }
    let from_players =
        |e: &evtc_ingest::CombatRecord| player_addresses.contains(&c.redirect(e.src_agent));
    let death = kept
        .events
        .iter()
        .rev()
        .find(|e| e.state_change == sc::CHANGE_DEAD && c.src(e, target));
    if let Some(death) = death {
        let last_damage = kept.events.iter().rev().find(|e| {
            e.state_change == sc::COMBAT
                && e.dst_agent != 0
                && c.redirect(e.dst_agent) == target.address
                && crate::damage::health_damage(e) > 0
                && from_players(e)
        });
        return End {
            success: true,
            time: last_damage.map_or(death.time, |d| d.time.min(death.time)),
        };
    }
    End {
        success: false,
        time: target.last,
    }
}

/// `HarvestTemple.CheckSuccess`: the grand strike chest gadget ends the fight; without it the
/// fight fails at the last time of the targets, which the logic stretches to the end of the log.
/// Not ported: the delayed reward through the targetable state of Soo-Won (no chest in the log).
fn harvest_temple_end(c: &Ctx<'_>) -> End {
    match chest_gadget_time(
        c,
        ei_data::species_ids::positions::GRAND_STRIKE_CHEST_HARVEST_TEMPLE_POSITION,
        2,
    ) {
        Some(time) => End {
            success: true,
            time,
        },
        None => End {
            success: false,
            time: c.kept.log_end_time,
        },
    }
}

/// `XunlaiJadeJunkyard.CheckSuccess`: after the generic check, the third lasting invulnerability
/// of Ankka.
fn ankka_end(c: &Ctx<'_>, target: &Agent, players: &[u64]) -> End {
    let base = generic_end(c, target, players);
    if base.success {
        return base;
    }
    let applies: Vec<&evtc_ingest::CombatRecord> = buff_applies(c, skill::DETERMINED895, target)
        .filter(|e| e.state_change == sc::BUFF_APPLY && e.value > i32::MAX / 2 && e.time >= 5000)
        .collect();
    match applies.as_slice() {
        [_, _, third] => End {
            success: true,
            time: third.time,
        },
        _ => base,
    }
}

/// `KainengOverlook.CheckSuccess`: Minister Li resurrects when the encounter is won.
fn minister_li_end(c: &Ctx<'_>, players: &[u64]) -> Option<End> {
    let _ = players;
    let li = first_agent(c.table, c.spans, species::target_id::MINISTER_LI_CM)
        .or_else(|| first_agent(c.table, c.spans, species::target_id::MINISTER_LI))?;
    Some(match buff_applies(c, skill::RESURRECTION, &li).next() {
        Some(apply) => End {
            success: true,
            time: apply.time,
        },
        None => End {
            success: false,
            time: li.last,
        },
    })
}

/// `Kanaxai.CheckSuccess`: the invulnerability she gets at the end, unless she despawns.
fn kanaxai_end(c: &Ctx<'_>, target: &Agent) -> End {
    let gain = buff_applies(c, skill::DETERMINED762, target).find(|e| e.time - c.offset > 0);
    if let Some(gain) = gain {
        let despawned = c.kept.events.iter().any(|e| {
            e.state_change == sc::DESPAWN
                && c.src(e, target)
                && (e.time - gain.time).abs() < SERVER_DELAY
        });
        if !despawned {
            return End {
                success: true,
                time: gain.time,
            };
        }
    }
    End {
        success: false,
        time: target.last,
    }
}

/// The time the players killed a target: its last death record, but not later than the last damage
/// of the players to it (`SetSuccessByDeath`), or `None` if it has no death record.
fn kill_time(c: &Ctx<'_>, target: &Agent, player_addresses: &[u64]) -> Option<i64> {
    let death = c
        .kept
        .events
        .iter()
        .rev()
        .find(|e| e.state_change == sc::CHANGE_DEAD && c.src(e, target))?;
    let last_damage = c.kept.events.iter().rev().find(|e| {
        e.state_change == sc::COMBAT
            && e.dst_agent != 0
            && c.redirect(e.dst_agent) == target.address
            && crate::damage::health_damage(e) > 0
            && player_addresses.contains(&c.redirect(e.src_agent))
    });
    Some(last_damage.map_or(death.time, |d| d.time.min(death.time)))
}

/// `Golem.CheckSuccess`: death of the golem, or health below 2 percent.
fn golem_end(c: &Ctx<'_>, target: &Agent) -> End {
    let death = c
        .kept
        .events
        .iter()
        .rev()
        .find(|e| e.state_change == sc::CHANGE_DEAD && c.src(e, target));
    if let Some(death) = death {
        return End {
            success: true,
            time: death.time,
        };
    }
    let last_health = c
        .kept
        .events
        .iter()
        .rev()
        .find(|e| e.state_change == sc::HEALTH_UPDATE && c.src(e, target));
    if let Some(health) = last_health {
        // the stored value is hundredths of a percent, rounded to two decimals
        #[allow(clippy::cast_precision_loss)]
        let percent = ((health.dst_agent as f64) / 100.0 * 100.0).round() / 100.0;
        if percent < 2.0 {
            let last_damage = c.kept.events.iter().rev().find(|e| {
                e.state_change == sc::COMBAT
                    && e.dst_agent != 0
                    && c.redirect(e.dst_agent) == target.address
                    && crate::damage::health_damage(e) > 0
            });
            return End {
                success: true,
                time: last_damage.map_or(target.last, |d| d.time),
            };
        }
    }
    End {
        success: false,
        time: target.last,
    }
}

/// `OldLionsCourt.CheckSuccess`: all three prototypes have to be killed.
fn old_lions_court_end(c: &Ctx<'_>, target: &Agent, players: &[u64]) -> End {
    use species::target_id as id;
    let cm = first_agent(c.table, c.spans, id::PROTOTYPE_VERMILION_CM).is_some();
    let ids = if cm {
        [
            id::PROTOTYPE_VERMILION_CM,
            id::PROTOTYPE_INDIGO_CM,
            id::PROTOTYPE_ARSENITE_CM,
        ]
    } else {
        [
            id::PROTOTYPE_VERMILION,
            id::PROTOTYPE_INDIGO,
            id::PROTOTYPE_ARSENITE,
        ]
    };
    let targets: Vec<Agent> = ids
        .iter()
        .flat_map(|&i| species_agents(c.table, c.spans, u32::try_from(i).unwrap_or(0)))
        .collect();
    let kills: Vec<i64> = targets
        .iter()
        .filter_map(|t| kill_time(c, t, players))
        .collect();
    if !targets.is_empty() && kills.len() == targets.len() {
        return End {
            success: true,
            time: kills.into_iter().max().unwrap_or(target.last),
        };
    }
    End {
        success: false,
        time: target.last,
    }
}

/// `AiKeeperOfThePeak.CheckSuccess`: the long invulnerability Ai gets at the end.
fn ai_end(c: &Ctx<'_>) -> Option<End> {
    use species::target_id as id;
    let dark = first_agent(c.table, c.spans, id::DARK_AI_KEEPER_OF_THE_PEAK);
    let elemental = first_agent(c.table, c.spans, id::AI_KEEPER_OF_THE_PEAK);
    let ai = dark.or(elemental)?;
    let gain = buff_applies(c, skill::DETERMINED895, &ai)
        .find(|e| i64::from(e.value) > i64::from(i32::MAX / 4));
    Some(match gain {
        Some(g) => End {
            success: true,
            time: g.time,
        },
        None => End {
            success: false,
            time: ai.last,
        },
    })
}

/// `AetherbladeHideout.CheckSuccess`: the last invulnerability of Mai Trin after Scarlet's echo
/// appeared.
fn aetherblade_end(c: &Ctx<'_>, target: &Agent) -> End {
    use species::target_id as id;
    let base = End {
        success: false,
        time: target.last,
    };
    let echo = first_agent(c.table, c.spans, id::ECHO_OF_SCARLET_BRIAR_CM)
        .or_else(|| first_agent(c.table, c.spans, id::ECHO_OF_SCARLET_BRIAR_NM));
    let Some(echo) = echo else {
        return base;
    };
    match buff_applies(c, skill::DETERMINED895, target).last() {
        Some(apply) if apply.time > echo.first => End {
            success: true,
            time: apply.time,
        },
        _ => End {
            success: false,
            time: echo.last,
        },
    }
}

/// `TwistedCastle.CheckSuccess`: the old raid reward; without it the fight just ends with the log.
fn twisted_castle_end(c: &Ctx<'_>, offset: i64) -> End {
    let end = c.kept.log_end_time;
    let reward = c.first_with(|e| {
        e.state_change == sc::REWARD
            && e.value == OLD_RAID_REWARD_2
            && e.time - offset > 0
            && e.time < end
    });
    match reward {
        Some(r) => End {
            success: true,
            time: r.time,
        },
        None => End {
            success: false,
            time: end,
        },
    }
}
/// `GetBuffApplyRemoveSequence` (beginning with an apply, with a dummy remove at the end) as
/// segments: the times the buff was on the target.
fn buff_segments(c: &Ctx<'_>, buff: i64, target: &Agent) -> Vec<(i64, i64)> {
    let mut segments = Vec::new();
    let mut open: Option<i64> = None;
    let events: Vec<&evtc_ingest::CombatRecord> = c
        .kept
        .events
        .iter()
        .filter(|e| {
            i64::from(e.skill_id) == buff
                && matches!(
                    e.state_change,
                    sc::BUFF_APPLY | sc::BUFF_INITIAL | sc::BUFF_REMOVE_ALL
                )
                && c.dst(e, target)
        })
        .collect();
    for (i, e) in events.iter().enumerate() {
        let is_apply = e.state_change != sc::BUFF_REMOVE_ALL;
        match open {
            None if is_apply => {
                // an apply right after the previous remove cancels that remove
                match segments.last() {
                    Some(&(start, end)) if e.time - end < SERVER_DELAY => {
                        segments.pop();
                        open = Some(start);
                    }
                    _ => open = Some(e.time),
                }
            }
            // only the last remove before another apply counts
            Some(start)
                if !is_apply
                    && events
                        .get(i + 1)
                        .is_none_or(|n| n.state_change != sc::BUFF_REMOVE_ALL) =>
            {
                segments.push((start, e.time));
                open = None;
            }
            _ => {}
        }
    }
    if let Some(start) = open {
        segments.push((start, target.last));
    }
    segments
}

/// `StatueOfDarkness.HasIntersectingLastGrasps`: the time both eyes were in their last grasp.
fn intersecting_last_grasps(c: &Ctx<'_>, fate: &Agent, judgement: &Agent) -> Option<i64> {
    let last_judge = *buff_segments(c, skill::LAST_GRASP_JUDGMENT, judgement).last()?;
    let last_fate = *buff_segments(c, skill::LAST_GRASP_FATE, fate).last()?;
    let intersects = !(last_fate.1 < last_judge.0 || last_judge.1 < last_fate.0);
    intersects.then(|| last_judge.0.max(last_fate.0))
}

/// `StatueOfDarkness.CheckSuccess`: both eyes dead (the later death counts), else both eyes in
/// their last grasp at once; without either the fight ends with the last eye.
fn statue_of_darkness_end(c: &Ctx<'_>, fate: &Agent, player_addresses: &[u64]) -> End {
    let Ok(judgement) = c.agent(species::target_id::EYE_OF_JUDGEMENT) else {
        return generic_end(c, fate, player_addresses);
    };
    let kills = [
        kill_time(c, fate, player_addresses),
        kill_time(c, &judgement, player_addresses),
    ];
    if let [Some(a), Some(b)] = kills {
        return End {
            success: true,
            time: a.max(b),
        };
    }
    if let Some(time) = intersecting_last_grasps(c, fate, &judgement) {
        return End {
            success: true,
            time,
        };
    }
    End {
        success: false,
        time: fate.last.max(judgement.last),
    }
}

/// `SingleActor.GetHealth` of the agent at `address`.
#[must_use]
pub fn target_health(kept: &Kept, table: AgentTable<'_>, address: u64) -> Option<u64> {
    let spans = spans(kept);
    let c = Ctx {
        kept,
        table,
        spans: &spans,
        offset: 0,
    };
    let &(first, last) = spans.get(&address)?;
    actor_health(
        &c,
        &Agent {
            address,
            first,
            last,
        },
    )
}

/// `SingleActor.GetHealth`: the maximum health the agent has around the middle of its time (or
/// shortly before its last hit).
fn actor_health(c: &Ctx<'_>, agent: &Agent) -> Option<u64> {
    let updates: Vec<(i64, u64)> = c
        .kept
        .events
        .iter()
        .filter(|e| e.state_change == sc::MAX_HEALTH_UPDATE && c.src(e, agent))
        .map(|e| (e.time, e.dst_agent))
        .collect();
    let last_damage = c.kept.events.iter().rev().find(|e| {
        e.state_change == sc::COMBAT
            && e.dst_agent != 0
            && c.redirect(e.dst_agent) == agent.address
            && crate::damage::health_damage(e) > 0
    });
    let mut check = i64::midpoint(agent.first, agent.last);
    if let Some(d) = last_damage {
        check = check.max(agent.first.max(d.time - 5000));
    }
    updates
        .iter()
        .rev()
        .find(|(time, _)| *time < check + SERVER_DELAY)
        .or_else(|| updates.iter().max_by_key(|(_, hp)| *hp))
        .map(|&(_, hp)| hp)
}

/// `Eparch.CheckSuccess`: the first (normal mode) or third (challenge mode) determined buff the
/// Eparch gets.
fn eparch_end(c: &Ctx<'_>) -> Result<End, MissingKeyActors> {
    /// `GW2Builds.June2024LonelyTowerCMRelease`.
    const CM_RELEASE: u64 = 163_807;
    /// `GW2Builds.June2024Balance`.
    const JUNE_2024_BALANCE: u64 = 164_824;
    /// `GW2Builds.April2026Balancepocalypse`.
    const APRIL_2026_BALANCE: u64 = 198_816;
    // eparchs that never cast (apart from drawing and stowing the weapon) are dummies
    let eparch = species_agents(c.table, c.spans, 26_231)
        .into_iter()
        .find(|a| {
            c.kept.events.iter().any(|e| {
                e.state_change == sc::ANIMATION_START
                    && c.src(e, a)
                    && !matches!(
                        i64::from(e.skill_id),
                        skill::WEAPON_DRAW | skill::WEAPON_STOW
                    )
            })
        })
        .ok_or(MissingKeyActors)?;
    let build = c.kept.gw2_build;
    let release: u64 = if build >= APRIL_2026_BALANCE {
        16_961_832
    } else if build >= JUNE_2024_BALANCE {
        22_833_236
    } else {
        32_618_906
    };
    let threshold = release * 95 / 100;
    let health_cm =
        build >= CM_RELEASE && actor_health(c, &eparch).is_some_and(|hp| hp >= threshold);
    let fake_cm = [26_270, 26_260]
        .iter()
        .any(|&id| !species_agents(c.table, c.spans, id).is_empty());
    let cm = health_cm || fake_cm;
    let applies: Vec<i64> = buff_applies(c, skill::DETERMINED762, &eparch)
        .map(|e| e.time)
        .collect();
    let wanted = if cm { 2 } else { 0 };
    Ok(match applies.get(wanted) {
        Some(&time) => End {
            success: true,
            time,
        },
        None => End {
            success: false,
            time: eparch.last,
        },
    })
}

/// The end of the fight in the time of `kept` (`LogData.SetSuccess`): a success ends 10 ms after
/// the time the encounter logic names (at most the end of the log), a failure at the time itself.
///
/// Ported: the generic check (reward chest of the raid encounters, then the death of the main
/// target), Ankka, Minister Li and Kanaxai. Not ported: combat exit and the `CheckSuccess` rules of
/// the other encounters.
///
/// # Errors
/// [`MissingKeyActors`] if the main target is not in the log.
pub fn log_end(
    kept: &Kept,
    table: AgentTable<'_>,
    player_addresses: &[u64],
    offset: i64,
) -> Result<i64, MissingKeyActors> {
    log_end_detail(kept, table, player_addresses, offset).map(|(time, _)| time)
}

/// The end of the fight like [`log_end`] and whether the encounter succeeded.
///
/// # Errors
/// [`MissingKeyActors`] if the main target is not in the log.
pub fn log_end_detail(
    kept: &Kept,
    table: AgentTable<'_>,
    player_addresses: &[u64],
    offset: i64,
) -> Result<(i64, bool), MissingKeyActors> {
    let spans = spans(kept);
    let c = Ctx {
        kept,
        table,
        spans: &spans,
        offset,
    };
    // the spirit race has no target to check, only the old raid reward
    if i32::from(kept.trigger_id) == species::target_id::ETHEREAL_BARRIER_GADGET {
        let end = twisted_castle_end(&c, offset);
        return Ok((
            if end.success {
                (end.time + SERVER_DELAY).min(kept.log_end_time)
            } else {
                end.time
            },
            end.success,
        ));
    }
    let target = c.agent(i32::from(kept.trigger_id))?;
    let end = match i32::from(kept.trigger_id) {
        species::target_id::ANKKA => ankka_end(&c, &target, player_addresses),
        species::target_id::MINISTER_LI | species::target_id::MINISTER_LI_CM => {
            minister_li_end(&c, player_addresses)
                .unwrap_or_else(|| generic_end(&c, &target, player_addresses))
        }
        // Harvest Temple: the chest, else a failure at the end of the log
        43_488 | 24_375 => harvest_temple_end(&c),
        // `Deimos.HandleDeimosAndItsGadgets` stretches the target to the end of the log (the 10 %
        // body struct is merged into it); the rules of the merged struct are not ported
        species::target_id::DEIMOS => generic_end(
            &c,
            &Agent {
                last: kept.log_end_time,
                ..target
            },
            player_addresses,
        ),
        species::target_id::KANAXAI_SCYTHE_OF_HOUSE_AURKUS_CM => kanaxai_end(&c, &target),
        species::target_id::PROTOTYPE_VERMILION | species::target_id::PROTOTYPE_VERMILION_CM => {
            old_lions_court_end(&c, &target, player_addresses)
        }
        species::target_id::AI_KEEPER_OF_THE_PEAK => {
            ai_end(&c).unwrap_or_else(|| generic_end(&c, &target, player_addresses))
        }
        species::target_id::MAI_TRIN_RAID => aetherblade_end(&c, &target),
        species::target_id::HAUNTING_STATUE => twisted_castle_end(&c, offset),
        species::target_id::EPARCH_LONELY_TOWER => eparch_end(&c)?,
        species::target_id::EYE_OF_FATE => statue_of_darkness_end(&c, &target, player_addresses),
        16169 | 16202 | 16178 | 16198 | 16177 | 16199 | 19676 | 19645 | 16174 | 16176 => {
            golem_end(&c, &target)
        }
        _ => generic_end(&c, &target, player_addresses),
    };
    Ok((
        if end.success {
            (end.time + SERVER_DELAY).min(kept.log_end_time)
        } else {
            end.time
        },
        end.success,
    ))
}

/// Whether the log is a challenge mode, for the logics whose mode the achievements depend on
/// (`GetLogMode` of Kaineng Overlook, Cosmic Observatory, Temple of Febe, Greer and Decima).
#[must_use]
pub fn challenge_mode(kept: &Kept, table: AgentTable<'_>) -> bool {
    /// `GW2Builds.DagdaNMHPChangedAndCMRelease`.
    const DAGDA_CM_RELEASE: u64 = 153_978;
    let spans = spans(kept);
    let c = Ctx {
        kept,
        table,
        spans: &spans,
        offset: 0,
    };
    let health = |id: i32| c.agent(id).ok().and_then(|agent| actor_health(&c, &agent));
    match i32::from(kept.trigger_id) {
        species::target_id::MINISTER_LI_CM => true,
        24_485 => c.agent(species::target_id::MINISTER_LI_CM).is_ok(),
        species::target_id::GREER => c.agent(species::target_id::EREG).is_ok(),
        species::target_id::DECIMA_CM => true,
        species::target_id::ANKKA => {
            health(species::target_id::ANKKA).is_some_and(|hp| hp > 40_000_000)
        }
        // Cerus of the Temple of Febe: challenge or legendary challenge mode
        species::target_id::CERUS => {
            health(species::target_id::CERUS).is_some_and(|hp| hp > 80_000_000)
        }
        species::target_id::DAGDA => {
            let Ok(dagda) = c.agent(species::target_id::DAGDA) else {
                return false;
            };
            if kept.gw2_build < DAGDA_CM_RELEASE
                || actor_health(&c, &dagda).is_none_or(|hp| hp <= 50_000_000)
            {
                return false;
            }
            // the demonic aura was removed but the timer never applied: not a true challenge mode
            let removed = c.kept.events.iter().any(|e| {
                e.state_change == sc::BUFF_REMOVE_ALL
                    && i64::from(e.skill_id) == skill::DAGDA_DEMONIC_AURA
                    && e.src_agent != 0
                    && c.redirect(e.src_agent) == dagda.address
            });
            let timer = c.kept.events.iter().any(|e| {
                matches!(e.state_change, sc::BUFF_APPLY | sc::BUFF_INITIAL)
                    && i64::from(e.skill_id) == skill::DAGDA_DEMONIC_AURA_TIMER
                    && e.dst_agent != 0
                    && c.redirect(e.dst_agent) == dagda.address
            });
            !removed || timer
        }
        _ => false,
    }
}

/// Files a regrouped group of agents: a single agent keeps its place, a merged one is new.
fn flush(
    group: Option<(usize, Target, usize)>,
    singles: &mut Vec<(usize, Target)>,
    merged: &mut Vec<Target>,
) {
    match group {
        Some((index, target, 1)) => singles.push((index, target)),
        Some((_, target, _)) => merged.push(target),
        None => {}
    }
}

/// A target of the encounter (`LogLogic.Targets`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// Address of the agent (of the first one if several were regrouped).
    pub address: u64,
    /// Addresses of all agents that are this target.
    pub addresses: Vec<u64>,
    /// Species id.
    pub species: i32,
    /// First time of the agent, with the offset applied.
    pub first: i64,
    /// Last time of the agent, with the offset applied.
    pub last: i64,
}

/// Agents of the species of the encounter that its logic sets to `IgnoredSpecies` or to a species
/// that is no target: the fake Ensolysses (`Ensolyss.IgnoreFakeEnsolysses`), the dummy Eparchs
/// (`Eparch.EIEvtcParse`) and the flux anomalies of Skorvald that gain determined
/// (`Skorvald.DetectUnknownAnomalies`).
fn ignored_agent(kept: &Kept, address: u64, species: i32) -> bool {
    use species::target_id as id;
    let redirect = |a: u64| *kept.agent_redirection.get(&a).unwrap_or(&a);
    let of_agent =
        |e: &evtc_ingest::CombatRecord| e.src_agent != 0 && redirect(e.src_agent) == address;
    match (i32::from(kept.trigger_id), species) {
        (t, s) if t == id::ENSOLYSS && s == id::ENSOLYSS => {
            // only agents that ever had at least 7 million health are Ensolyss
            !kept.events.iter().any(|e| {
                e.state_change == sc::MAX_HEALTH_UPDATE && of_agent(e) && e.dst_agent >= 7_000_000
            })
        }
        (t, s) if t == id::EPARCH_LONELY_TOWER && s == id::EPARCH_LONELY_TOWER => {
            // an Eparch that never casts (apart from drawing and stowing the weapon) is a dummy
            !kept.events.iter().any(|e| {
                e.state_change == sc::ANIMATION_START
                    && of_agent(e)
                    && !matches!(
                        i64::from(e.skill_id),
                        skill::WEAPON_DRAW | skill::WEAPON_STOW
                    )
            })
        }
        (t, s) if t == id::SKORVALD && FLUX_ANOMALIES.contains(&s) => kept.events.iter().any(|e| {
            e.state_change == sc::BUFF_APPLY
                && i64::from(e.skill_id) == skill::DETERMINED762
                && e.dst_agent != 0
                && redirect(e.dst_agent) == address
        }),
        (t, s) if t == id::QADIM && s == id::PYRE_GUARDIAN => qadim_pyre_is_known(kept, address),
        _ => false,
    }
}

/// `Qadim.FindPyres`: a pyre guardian at one of the known positions gets its own species, so it
/// is no longer one of the plain pyre guardians.
fn qadim_pyre_is_known(kept: &Kept, address: u64) -> bool {
    const POSITIONS: [(f32, f32); 8] = [
        (-8947.0, 14728.0),
        (-10834.0, 12477.0),
        (-4356.0, 12076.0),
        (-5889.0, 14723.0),
        (-7851.0, 13550.0),
        (-8951.0, 9429.0),
        (-5716.0, 9325.0),
        (-7846.0, 10612.0),
    ];
    /// `ParserHelper.InchDistanceThresholdSquared`.
    const THRESHOLD_SQUARED: f32 = std::f32::consts::SQRT_2 * std::f32::consts::SQRT_2;
    kept.events
        .iter()
        .find(|e| {
            e.state_change == sc::POSITION
                && e.src_agent != 0
                && *kept
                    .agent_redirection
                    .get(&e.src_agent)
                    .unwrap_or(&e.src_agent)
                    == address
        })
        .is_some_and(|e| {
            let x = f32::from_bits((e.dst_agent & 0xFFFF_FFFF) as u32);
            let y = f32::from_bits((e.dst_agent >> 32) as u32);
            POSITIONS
                .iter()
                .any(|&(px, py)| (px - x).powi(2) + (py - y).powi(2) < THRESHOLD_SQUARED)
        })
}

/// The flux anomalies of Skorvald.
const FLUX_ANOMALIES: [i32; 8] = [
    17_578, 17_929, 17_695, 17_651, 17_599, 17_770, 17_851, 17_673,
];

/// The lamps of Qadim (`Qadim.FindLamps`): the gadgets that carry the lamp marker, as targets of the
/// volatile species `QadimLamp`.
#[must_use]
pub fn qadim_lamps(kept: &Kept, table: AgentTable<'_>, offset: i64) -> Vec<Target> {
    let Some(marker) = kept.events.iter().find(|e| {
        e.state_change == sc::ID_TO_GUID
            && record_guid(e) == ei_data::marker_guids::QADIM_LAMP_MARKER
    }) else {
        return Vec::new();
    };
    let spans = spans(kept);
    let mut addresses: Vec<u64> = Vec::new();
    for e in &kept.events {
        if e.state_change == sc::MARKER
            && i64::from(e.value) == i64::from(marker.skill_id)
            && e.src_agent != 0
        {
            let address = *kept
                .agent_redirection
                .get(&e.src_agent)
                .unwrap_or(&e.src_agent);
            if !addresses.contains(&address) {
                addresses.push(address);
            }
        }
    }
    addresses
        .into_iter()
        .filter(|a| {
            table
                .iter()
                .any(|x| x.address() == *a && agent_kind(x) == AgentKind::Gadget)
        })
        .filter_map(|a| {
            let &(first, last) = spans.get(&a)?;
            Some(Target {
                address: a,
                addresses: vec![a],
                species: species::target_id::QADIM_LAMP,
                first: first - offset,
                last: last - offset,
            })
        })
        .collect()
}

/// The intervals in which an attack target is targetable, with the agent that owns it
/// (`AttackTargetEvent.GetTargetableEvents`): (owner, attack target, start, end). Times have the
/// offset applied.
fn targetable_intervals(kept: &Kept, offset: i64) -> Vec<(u64, u64, i64, i64)> {
    let redirect = |a: u64| *kept.agent_redirection.get(&a).unwrap_or(&a);
    let mut out = Vec::new();
    let mut seen: Vec<u64> = Vec::new();
    for e in kept
        .events
        .iter()
        .filter(|e| e.state_change == sc::ATTACK_TARGET)
    {
        // one end of the record is the attack target (it has most targetable records), the
        // other the gadget it belongs to
        let toggles = |a: u64| {
            kept.events
                .iter()
                .filter(|t| t.state_change == sc::TARGETABLE && redirect(t.src_agent) == a)
                .count()
        };
        let (owner, target) = if toggles(redirect(e.dst_agent)) >= toggles(redirect(e.src_agent)) {
            (redirect(e.src_agent), redirect(e.dst_agent))
        } else {
            (redirect(e.dst_agent), redirect(e.src_agent))
        };
        if seen.contains(&target) {
            continue;
        }
        let changes: Vec<(i64, bool)> = kept
            .events
            .iter()
            .filter(|t| t.state_change == sc::TARGETABLE && redirect(t.src_agent) == target)
            .map(|t| (t.time - offset, t.dst_agent == 1))
            .collect();
        if !changes.iter().any(|c| c.1) {
            continue;
        }
        seen.push(target);
        for (i, &(start, on)) in changes.iter().enumerate() {
            if !on {
                continue;
            }
            let end = changes[i + 1..]
                .iter()
                .find(|c| !c.1 && c.0 > start)
                .map_or(kept.log_end_time - offset, |c| c.0);
            out.push((owner, target, start, end));
        }
    }
    out
}

/// The times an attack target of an agent becomes targetable (`Conjured Amalgamate` arms): the
/// first attack target of the agent.
#[must_use]
pub fn attack_target_on_times(kept: &Kept, offset: i64, owners: &[u64]) -> Vec<i64> {
    let intervals: Vec<_> = targetable_intervals(kept, offset)
        .into_iter()
        .filter(|(owner, ..)| owners.contains(owner))
        .collect();
    let Some(first) = intervals.first().map(|i| i.1) else {
        return Vec::new();
    };
    intervals
        .iter()
        .filter(|i| i.1 == first)
        .map(|i| i.2)
        .collect()
}

/// The hands of Adina (`Adina.FindHands`): one agent for each time a hand can be attacked.
#[must_use]
pub fn adina_hands(kept: &Kept, offset: i64) -> Vec<Target> {
    const ERUPTION: [(f32, f32); 2] = [(15_570.5, -693.117), (14_277.2, -2_202.52)];
    const THRESHOLD_SQUARED: f32 = std::f32::consts::SQRT_2 * std::f32::consts::SQRT_2;
    if i32::from(kept.trigger_id) != species::target_id::ADINA {
        return Vec::new();
    }
    let redirect = |a: u64| *kept.agent_redirection.get(&a).unwrap_or(&a);
    targetable_intervals(kept, offset)
        .into_iter()
        .map(|(hand, _, start, end)| {
            let eruption = kept
                .events
                .iter()
                .find(|e| {
                    e.state_change == sc::POSITION
                        && e.src_agent != 0
                        && redirect(e.src_agent) == hand
                })
                .is_some_and(|e| {
                    let x = f32::from_bits((e.dst_agent & 0xFFFF_FFFF) as u32);
                    let y = f32::from_bits((e.dst_agent >> 32) as u32);
                    ERUPTION
                        .iter()
                        .any(|&(px, py)| (px - x).powi(2) + (py - y).powi(2) < THRESHOLD_SQUARED)
                });
            // the new agent is only known from its first to its last record inside the interval
            let times: Vec<i64> = kept
                .events
                .iter()
                .filter(|e| e.src_agent != 0 && redirect(e.src_agent) == hand)
                .map(|e| e.time - offset)
                .filter(|t| (start..=end).contains(t))
                .collect();
            let first = times.iter().copied().min().unwrap_or(start);
            let last = times.iter().copied().max().unwrap_or(end);
            Target {
                address: hand,
                addresses: vec![hand],
                species: if eruption {
                    species::target_id::HAND_OF_ERUPTION
                } else {
                    species::target_id::HAND_OF_EROSION
                },
                first,
                last,
            }
        })
        .collect()
}

/// The agents of a species with the first time of each (offset not applied), ordered by it.
#[must_use]
pub(crate) fn species_first_seen(kept: &Kept, table: AgentTable<'_>, id: i64) -> Vec<(u64, i64)> {
    let spans = spans(kept);
    u32::try_from(id)
        .map(|id| {
            species_agents(table, &spans, id)
                .into_iter()
                .map(|a| (a.address, a.first))
                .collect()
        })
        .unwrap_or_default()
}

/// The addresses of the agents of the trash mob species of the logic (`LogLogic.TrashMobs`).
#[must_use]
pub(crate) fn trash_addresses(kept: &Kept, table: AgentTable<'_>) -> Vec<u64> {
    let Some(logic) = ei_data::targets::logic(i32::from(kept.trigger_id)) else {
        return Vec::new();
    };
    let spans = spans(kept);
    logic
        .trash
        .iter()
        .filter_map(|&id| u32::try_from(id).ok())
        .flat_map(|id| species_agents(table, &spans, id))
        .map(|a| a.address)
        .collect()
}

/// The agents of some species as (first, last, end) with the offset applied, ordered by first
/// appearance; the end is the last death of the agent if it has one, else its last time.
#[must_use]
pub fn species_lifetimes(
    kept: &Kept,
    table: AgentTable<'_>,
    offset: i64,
    ids: &[u32],
) -> Vec<(i64, i64, i64)> {
    let spans = spans(kept);
    let mut out: Vec<(i64, i64, i64)> = Vec::new();
    for &id in ids {
        for agent in species_agents(table, &spans, id) {
            let death = kept
                .events
                .iter()
                .rev()
                .find(|e| {
                    e.state_change == sc::CHANGE_DEAD
                        && e.src_agent != 0
                        && *kept
                            .agent_redirection
                            .get(&e.src_agent)
                            .unwrap_or(&e.src_agent)
                            == agent.address
                })
                .map(|e| e.time - offset);
            out.push((
                agent.first - offset,
                agent.last - offset,
                death.unwrap_or(agent.last - offset),
            ));
        }
    }
    out.sort_by_key(|o| o.0);
    out
}

/// The first times (with the offset applied) of the agents of a species, ordered by first appearance.
#[must_use]
pub fn species_first_times(kept: &Kept, table: AgentTable<'_>, offset: i64, id: u32) -> Vec<i64> {
    species_agents(table, &spans(kept), id)
        .into_iter()
        .map(|a| a.first - offset)
        .collect()
}

/// The targets of the encounter: the agents of the species its logic lists, ordered by first
/// appearance and then by the sort keys of the logic (`LogLogic.ComputeLogTargets`).
///
/// Only agents with a known species are found; the volatile species that C# derives for gadgets
/// (negative ids) are not.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn log_targets(kept: &Kept, table: AgentTable<'_>, offset: i64) -> Vec<Target> {
    let Some(logic) = ei_data::targets::logic(i32::from(kept.trigger_id)) else {
        return Vec::new();
    };
    let spans = spans(kept);
    // the instance id of every agent: the first one its records carry (`UpdateAgentData`)
    let mut instids: HashMap<u64, u16> = HashMap::new();
    // and the master it reports
    let mut masters: HashMap<u64, u16> = HashMap::new();
    for e in &kept.events {
        if src_is_agent(e.state_change) && e.src_agent != 0 && e.src_master_instid != 0 {
            masters
                .entry(
                    *kept
                        .agent_redirection
                        .get(&e.src_agent)
                        .unwrap_or(&e.src_agent),
                )
                .or_insert(e.src_master_instid);
        }
        if src_is_agent(e.state_change) && e.src_agent != 0 && e.src_instid != 0 {
            instids
                .entry(
                    *kept
                        .agent_redirection
                        .get(&e.src_agent)
                        .unwrap_or(&e.src_agent),
                )
                .or_insert(e.src_instid);
        }
        if dst_is_agent(e.state_change) && e.dst_agent != 0 && e.dst_instid != 0 {
            instids
                .entry(
                    *kept
                        .agent_redirection
                        .get(&e.dst_agent)
                        .unwrap_or(&e.dst_agent),
                )
                .or_insert(e.dst_instid);
        }
    }
    // squad combat windows: agents of different windows are never regrouped
    let mut windows = vec![i64::MIN];
    windows.extend(
        kept.events
            .iter()
            .filter(|e| {
                matches!(
                    e.state_change,
                    sc::SQUAD_COMBAT_START | sc::SQUAD_COMBAT_END
                )
            })
            .map(|e| e.time),
    );
    let window = |t: &Target| {
        let half = i64::midpoint(t.first, t.last) + offset;
        windows
            .iter()
            .rfind(|&&w| w <= half)
            .copied()
            .unwrap_or(i64::MIN)
    };
    // agents in the order of the agent table, like C# lists them before it sorts
    let mut candidates: Vec<(Target, u16, String, u16)> = Vec::new();
    for agent in table.iter() {
        let species = match agent_kind(agent) {
            AgentKind::Npc => i64::from(agent.profession()),
            AgentKind::Gadget => i64::from(agent.profession() & 0xFFFF),
            _ => continue,
        };
        let Ok(species) = i32::try_from(species) else {
            continue;
        };
        let Some(&(first, last)) = spans.get(&agent.address()) else {
            continue;
        };
        if logic.targets.contains(&species) && !ignored_agent(kept, agent.address(), species) {
            candidates.push((
                Target {
                    address: agent.address(),
                    addresses: vec![agent.address()],
                    species,
                    first: first - offset,
                    last: last - offset,
                },
                instids.get(&agent.address()).copied().unwrap_or(0),
                agent.name().into_owned(),
                masters.get(&agent.address()).copied().unwrap_or(0),
            ));
        }
    }
    // `RegroupSameAgentsAndDetermineTeams`: consecutive agents of one instance id, species and
    // name in the same combat window are one agent
    // (position in the agent table, target); regrouped agents are new and come last
    let mut singles: Vec<(usize, Target)> = Vec::new();
    let mut merged: Vec<Target> = Vec::new();
    let mut order: Vec<u16> = Vec::new();
    for (_, instid, _, _) in &candidates {
        if !order.contains(instid) {
            order.push(*instid);
        }
    }
    for instid in order {
        let mut previous: Option<&(Target, u16, String, u16)> = None;
        let mut group: Option<(usize, Target, usize)> = None;
        let mut group_window = i64::MIN;
        for (index, cand) in candidates.iter().enumerate().filter(|(_, c)| c.1 == instid) {
            let same = previous.is_some_and(|p| {
                p.0.species == cand.0.species
                    && p.2 == cand.2
                    && p.3 == cand.3
                    && group_window == window(&cand.0)
            });
            if let (Some((_, g, count)), true) = (&mut group, same) {
                g.first = g.first.min(cand.0.first);
                g.last = g.last.max(cand.0.last);
                g.addresses.push(cand.0.address);
                *count += 1;
            } else {
                flush(group.take(), &mut singles, &mut merged);
                group = Some((index, cand.0.clone(), 1));
                group_window = window(&cand.0);
            }
            previous = Some(cand);
        }
        flush(group, &mut singles, &mut merged);
    }
    singles.sort_by_key(|(index, _)| *index);
    let mut targets: Vec<Target> = singles.into_iter().map(|(_, t)| t).collect();
    targets.extend(merged);
    targets.retain(|t| t.last > 0);
    // the second Xera is merged into the first one
    if i32::from(kept.trigger_id) == species::target_id::XERA
        && let Some(second) = species_agents(table, &spans, 16_286).first()
        && let Some(first) = targets
            .iter_mut()
            .find(|t| t.species == species::target_id::XERA)
    {
        first.addresses.push(second.address);
    }
    targets.sort_by_key(|t| t.first);
    // stable, like `OrderBy`; the default key is the position in the list
    let key = |t: &Target| {
        logic
            .sort
            .iter()
            .find(|(id, _)| *id == t.species)
            .map_or(i32::MAX, |&(_, k)| k)
    };
    targets.sort_by_key(key);
    targets
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
