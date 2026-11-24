//! Offensive and defensive statistics of an agent: the counters of `OffensiveStatistics` and
//! `DefensePerTargetStatistics` that come from health damage events.
//!
//! Supported: logs of arcdps build 20260501 and later. Not ported: down contribution and crowd
//! control, the status durations, boon strips and cleanses, dodges, and breakbar damage taken.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use ei_data::buffs::Classification;

use crate::buff_events::LogContext;
use crate::damage::{UnsupportedBuild, condition_ids};
use crate::generated::state_change as sc;

const RESULT_ENUM_REWORK_BUILD: i32 = 20_260_501;

/// A health damage event of the C# parser (`HealthDamageEvent` and its kinds).
#[derive(Debug, Default, Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct Event {
    /// A direct damage event (else buff damage or no damage at all).
    pub(crate) direct: bool,
    /// Interrupt, kill or down: no damage.
    pub(crate) no_damage: bool,
    pub(crate) health: i64,
    pub(crate) shield: i64,
    pub(crate) has_hit: bool,
    pub(crate) crit: bool,
    pub(crate) glance: bool,
    pub(crate) leech: bool,
    pub(crate) blind: bool,
    pub(crate) absorbed: bool,
    pub(crate) blocked: bool,
    pub(crate) evaded: bool,
    pub(crate) interrupted: bool,
    pub(crate) downed: bool,
    pub(crate) killed: bool,
    pub(crate) over_ninety: bool,
    pub(crate) flanking: bool,
    pub(crate) against_moving: bool,
    pub(crate) against_downed: bool,
}

/// Builds the event of a combat record (`CombatEventFactory`), if it is one.
pub(crate) fn classify(e: &evtc_ingest::CombatRecord) -> Option<Event> {
    let result = e.result;
    let mut ev = Event {
        over_ninety: e.is_ninety > 0,
        flanking: e.is_flanking > 0,
        against_moving: e.is_moving & 2 > 0,
        against_downed: e.is_offcycle == 1,
        ..Event::default()
    };
    match result {
        5 | 8 | 9 => {
            ev.no_damage = true;
            ev.interrupted = result == 5;
            ev.killed = result == 8;
            ev.downed = result == 9;
            return Some(ev);
        }
        10..=12 => return None,
        _ => {}
    }
    if e.is_buff == 0 {
        if !matches!(result, 0..=4 | 6 | 7 | 13) {
            return None;
        }
        ev.direct = true;
        ev.health = i64::from(e.value);
        ev.shield = if e.is_shields > 0 {
            i64::from(e.overstack_value)
        } else {
            0
        };
        ev.absorbed = matches!(result, 6 | 13);
        ev.blind = result == 7;
        ev.blocked = result == 3;
        ev.crit = result == 1;
        ev.evaded = result == 4;
        ev.glance = result == 2;
        ev.has_hit = matches!(result, 0..=2);
    } else {
        if !matches!(result, 14..=18 | 6 | 13) {
            return None;
        }
        ev.health = i64::from(e.buff_dmg);
        ev.leech = matches!(result, 16 | 18);
        ev.absorbed = matches!(result, 6 | 13);
        ev.shield = if e.is_shields > 0 {
            if e.overstack_value > 0 {
                i64::from(e.overstack_value)
            } else {
                ev.health
            }
        } else {
            0
        };
        ev.has_hit = matches!(result, 14 | 15 | 17) || ev.leech;
    }
    Some(ev)
}

/// `SkillItem.CanCrit` for the builds of the supported logs: the skills of
/// `SkillItemOverrides.NonCritableSkills` never crit (all their build limits lie far in the past).
fn can_crit(skill_id: u32) -> bool {
    !matches!(
        skill_id,
        9292 | 9284
            | 5492
            | 13014
            | 30770
            | 52370
            | 31686
            | 56883
            | 56885
            | 29604
            | 28388
            | 13907
            | 29560
            | 13906
            | 13752
            | 22499
            | 21795
            | 43630
            | 45534
            | 10606
    )
}
/// Counters of `OffensiveStatistics`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct OffensiveStats {
    pub total_damage_count: i64,
    pub total_damage: i64,
    pub direct_event_count: i64,
    pub direct_event_damage: i64,
    pub connected_count: i64,
    pub connected_damage: i64,
    pub connected_direct_count: i64,
    pub connected_direct_damage: i64,
    pub power_count: i64,
    pub power_damage: i64,
    pub power_above_90_count: i64,
    pub power_above_90_damage: i64,
    pub life_leech_count: i64,
    pub life_leech_damage: i64,
    pub condition_count: i64,
    pub condition_damage: i64,
    pub condition_above_90_count: i64,
    pub condition_above_90_damage: i64,
    pub critable_count: i64,
    pub critical_count: i64,
    pub critical_damage: i64,
    pub flanking: i64,
    pub glancing: i64,
    pub against_moving: i64,
    pub missed: i64,
    pub blocked: i64,
    pub evaded: i64,
    pub interrupts: i64,
    pub invulned: i64,
    pub killed: i64,
    pub downed: i64,
    pub against_downed_count: i64,
    pub against_downed_damage: i64,
}

/// Counters of `DefensePerTargetStatistics` that come from damage events.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
#[allow(missing_docs)]
pub struct DefensiveStats {
    pub damage_taken: i64,
    pub damage_taken_count: i64,
    pub condition_taken: i64,
    pub condition_taken_count: i64,
    pub power_taken: i64,
    pub power_taken_count: i64,
    pub strike_taken: i64,
    pub strike_taken_count: i64,
    pub life_leech_taken: i64,
    pub life_leech_taken_count: i64,
    pub downed_taken: i64,
    pub downed_taken_count: i64,
    pub barrier: i64,
    pub barrier_count: i64,
    pub blocked: i64,
    pub missed: i64,
    pub evaded: i64,
    pub invulned: i64,
    pub interrupted: i64,
    pub dead_count: i64,
    pub dc_count: i64,
    pub boon_strips: i64,
    /// Seconds.
    pub boon_strips_time: f64,
    pub condition_cleanses: i64,
    /// Seconds.
    pub condition_cleanses_time: f64,
}

/// Counters of `SupportStatistics` that come from buff removals.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
#[allow(missing_docs)]
pub struct SupportStats {
    pub condition_cleanse: i64,
    /// Seconds.
    pub condition_cleanse_time: f64,
    pub condition_cleanse_self: i64,
    /// Seconds.
    pub condition_cleanse_time_self: f64,
    pub boon_strips: i64,
    /// Seconds.
    pub boon_strips_time: f64,
}

/// The records that can be damage the agent (or its minions) deals: the whole log is looked at
/// once per agent, the statistics then only look at these.
#[must_use]
pub fn dealt_events(kept: &crate::keep::Kept, address: u64) -> Vec<&evtc_ingest::CombatRecord> {
    let redirect = |a: u64| *kept.agent_redirection.get(&a).unwrap_or(&a);
    let instid = kept
        .events
        .iter()
        .find(|e| e.src_agent != 0 && redirect(e.src_agent) == address && e.src_instid != 0)
        .map_or(0, |e| e.src_instid);
    kept.events
        .iter()
        .filter(|e| {
            e.state_change == sc::COMBAT
                && e.src_agent != 0
                && (redirect(e.src_agent) == address
                    || (e.src_master_instid != 0 && e.src_master_instid == instid))
        })
        .collect()
}

/// Damage of the agent at `address` to foes (`only_to`: just to those targets), like
/// `OffensiveStatistics`.
///
/// # Errors
/// [`UnsupportedBuild`] for logs older than the result enum rework.
#[allow(clippy::implicit_hasher)]
pub fn offensive_stats(
    ctx: &LogContext<'_>,
    events: &[&evtc_ingest::CombatRecord],
    address: u64,
    barrier_negated: &HashSet<u64>,
    only_to: Option<&HashSet<u64>>,
) -> Result<OffensiveStats, UnsupportedBuild> {
    let kept = ctx.kept;
    if kept.version.build < RESULT_ENUM_REWORK_BUILD {
        return Err(UnsupportedBuild(kept.version.build));
    }
    let conditions = condition_ids(ctx);
    let redirect = |a: u64| *kept.agent_redirection.get(&a).unwrap_or(&a);
    let instid = kept
        .events
        .iter()
        .find(|e| e.src_agent != 0 && redirect(e.src_agent) == address && e.src_instid != 0)
        .map_or(0, |e| e.src_instid);
    let mut s = OffensiveStats::default();
    for e in events.iter().copied() {
        if e.state_change != sc::COMBAT {
            continue;
        }
        let time = e.time - ctx.time_offset;
        if time < ctx.log_start || time > ctx.log_end {
            continue;
        }
        if e.iff == 0 || e.src_agent == 0 {
            continue;
        }
        // interrupts, kills and downs of minions count for their master too
        let minion = redirect(e.src_agent) != address
            && matches!(e.result, 5 | 8 | 9)
            && e.src_master_instid != 0
            && e.src_master_instid == instid;
        if redirect(e.src_agent) != address && !minion {
            continue;
        }
        let dst = redirect(e.dst_agent);
        if only_to.is_some_and(|t| !t.contains(&dst)) {
            continue;
        }
        let Some(mut ev) = classify(e) else {
            continue;
        };
        if barrier_negated.contains(&dst) && ev.shield > 0 {
            ev.health = (ev.health - ev.shield).max(0);
        }
        if minion {
            // not the damage of the minion, just the events it caused
            s.interrupts += i64::from(ev.interrupted);
            s.killed += i64::from(ev.killed);
            s.downed += i64::from(ev.downed);
            continue;
        }
        add_offense(
            &mut s,
            &ev,
            e.skill_id,
            conditions.contains(&i64::from(e.skill_id)),
        );
    }
    Ok(s)
}

fn add_offense(s: &mut OffensiveStats, ev: &Event, skill_id: u32, condition: bool) {
    if !ev.no_damage {
        s.total_damage_count += 1;
        s.total_damage += ev.health;
        if ev.direct {
            s.direct_event_count += 1;
            s.direct_event_damage += ev.health;
        }
    }
    if ev.has_hit {
        s.connected_count += 1;
        s.connected_damage += ev.health;
        if condition {
            s.condition_count += 1;
            s.condition_damage += ev.health;
            if ev.over_ninety {
                s.condition_above_90_count += 1;
                s.condition_above_90_damage += ev.health;
            }
        } else {
            if ev.direct {
                if can_crit(skill_id) {
                    if ev.crit {
                        s.critical_count += 1;
                        s.critical_damage += ev.health;
                    }
                    s.critable_count += 1;
                }
                s.connected_direct_count += 1;
                s.connected_direct_damage += ev.health;
                if ev.flanking {
                    s.flanking += 1;
                }
                if ev.glance {
                    s.glancing += 1;
                }
            } else if ev.leech {
                s.life_leech_count += 1;
                s.life_leech_damage += ev.health;
            }
            s.power_count += 1;
            s.power_damage += ev.health;
            if ev.over_ninety {
                s.power_above_90_count += 1;
                s.power_above_90_damage += ev.health;
            }
        }
        if ev.against_downed {
            s.against_downed_count += 1;
            s.against_downed_damage += ev.health;
        }
        if ev.against_moving {
            s.against_moving += 1;
        }
    }
    s.invulned += i64::from(ev.absorbed);
    s.missed += i64::from(ev.blind);
    s.evaded += i64::from(ev.evaded);
    s.blocked += i64::from(ev.blocked);
    s.interrupts += i64::from(ev.interrupted);
    s.killed += i64::from(ev.killed);
    s.downed += i64::from(ev.downed);
}

/// Damage taken by the agent at `address` and what comes with it, like `DefensePerTargetStatistics`.
///
/// # Errors
/// [`UnsupportedBuild`] for logs older than the result enum rework.
#[allow(clippy::too_many_lines)]
pub fn defensive_stats(
    ctx: &LogContext<'_>,
    address: u64,
) -> Result<DefensiveStats, UnsupportedBuild> {
    let kept = ctx.kept;
    if kept.version.build < RESULT_ENUM_REWORK_BUILD {
        return Err(UnsupportedBuild(kept.version.build));
    }
    let conditions = condition_ids(ctx);
    let redirect = |a: u64| *kept.agent_redirection.get(&a).unwrap_or(&a);
    let mut s = DefensiveStats::default();
    for e in &kept.events {
        let time = e.time - ctx.time_offset;
        if time < ctx.log_start || time > ctx.log_end {
            continue;
        }
        if e.state_change == sc::CHANGE_DEAD && redirect(e.src_agent) == address {
            s.dead_count += 1;
            continue;
        }
        if e.state_change == sc::DESPAWN && redirect(e.src_agent) == address {
            s.dc_count += 1;
            continue;
        }
        if e.state_change != sc::COMBAT || e.dst_agent == 0 || redirect(e.dst_agent) != address {
            continue;
        }
        let Some(ev) = classify(e) else {
            continue;
        };
        if ev.has_hit {
            s.damage_taken += ev.health;
            s.damage_taken_count += 1;
            if conditions.contains(&i64::from(e.skill_id)) {
                s.condition_taken += ev.health;
                s.condition_taken_count += 1;
            } else {
                if ev.leech {
                    s.life_leech_taken += ev.health;
                    s.life_leech_taken_count += 1;
                }
                if ev.direct {
                    s.strike_taken += ev.health;
                    s.strike_taken_count += 1;
                }
                s.power_taken += ev.health;
                s.power_taken_count += 1;
            }
            if ev.shield > 0 {
                s.barrier += ev.shield;
                s.barrier_count += 1;
            }
            if ev.against_downed {
                s.downed_taken += ev.health;
                s.downed_taken_count += 1;
            }
        }
        s.blocked += i64::from(ev.blocked);
        s.missed += i64::from(ev.blind);
        s.invulned += i64::from(ev.absorbed);
        s.evaded += i64::from(ev.evaded);
        s.interrupted += i64::from(ev.interrupted);
    }
    // boons that were stripped and conditions that were cleansed, by anybody who is known
    let (boons, condis) = (
        buff_ids(ctx, Classification::Boon),
        buff_ids(ctx, Classification::Condition),
    );
    let mut boon_time: HashMap<i64, i64> = HashMap::new();
    let mut condi_time: HashMap<i64, i64> = HashMap::new();
    for e in &kept.events {
        let time = e.time - ctx.time_offset;
        // a removal record: the source holds the buff, the destination removes it
        if e.state_change != sc::BUFF_REMOVE_ALL
            || e.src_agent == 0
            || redirect(e.src_agent) != address
            || e.dst_agent == 0
            || time < ctx.log_start
            || time > ctx.log_end
        {
            continue;
        }
        let id = i64::from(e.skill_id);
        let removed = i64::from(e.value);
        if boons.contains(&id) && redirect(e.dst_agent) != address {
            s.boon_strips += 1;
            let t = boon_time.entry(id).or_insert(0);
            *t = (*t + removed).min(ctx.log_end);
        } else if condis.contains(&id) {
            s.condition_cleanses += 1;
            let t = condi_time.entry(id).or_insert(0);
            *t = (*t + removed).min(ctx.log_end);
        }
    }
    s.boon_strips_time = seconds(boon_time.values().sum());
    s.condition_cleanses_time = seconds(condi_time.values().sum());
    Ok(s)
}

/// Milliseconds as seconds with three digits (ParserHelper.TimeDigit).
fn seconds(ms: i64) -> f64 {
    #[allow(clippy::cast_precision_loss)]
    let value = ms as f64 / 1000.0;
    (value * 1000.0).round() / 1000.0
}

/// The ids of the buffs of a classification that exist in the log.
fn buff_ids(ctx: &LogContext<'_>, classification: Classification) -> Rc<HashSet<i64>> {
    crate::damage::classified_ids(ctx, classification)
}

/// Strips and cleanses the agent at `address` did, like `SupportStatistics`. `players` are the
/// addresses of the squad players.
///
/// # Errors
/// [`UnsupportedBuild`] for logs older than the result enum rework.
pub fn support_stats(
    ctx: &LogContext<'_>,
    address: u64,
    players: &[u64],
) -> Result<SupportStats, UnsupportedBuild> {
    let kept = ctx.kept;
    if kept.version.build < RESULT_ENUM_REWORK_BUILD {
        return Err(UnsupportedBuild(kept.version.build));
    }
    let redirect = |a: u64| *kept.agent_redirection.get(&a).unwrap_or(&a);
    let (boons, condis) = (
        buff_ids(ctx, Classification::Boon),
        buff_ids(ctx, Classification::Condition),
    );
    let instid = kept
        .events
        .iter()
        .find(|e| e.src_agent != 0 && redirect(e.src_agent) == address && e.src_instid != 0)
        .map_or(0, |e| e.src_instid);
    let mut s = SupportStats::default();
    // milliseconds per (ally, buff), clamped like SupportPerAllyStatistics
    let mut times: HashMap<(u64, i64), i64> = HashMap::new();
    let mut counts: HashMap<u64, i64> = HashMap::new();
    let mut strip_times: HashMap<i64, i64> = HashMap::new();
    for e in &kept.events {
        let time = e.time - ctx.time_offset;
        if e.state_change != sc::BUFF_REMOVE_ALL
            || e.dst_agent == 0
            || time < ctx.log_start
            || time > ctx.log_end
        {
            continue;
        }
        let by_actor = redirect(e.dst_agent) == address
            || (e.dst_master_instid != 0 && e.dst_master_instid == instid);
        if !by_actor {
            continue;
        }
        let id = i64::from(e.skill_id);
        let removed = i64::from(e.value);
        let dst = redirect(e.src_agent);
        // IFF: 0 friend, 1 foe, 2 unknown
        if condis.contains(&id) && e.iff == 0 && players.contains(&dst) {
            *counts.entry(dst).or_insert(0) += 1;
            let t = times.entry((dst, id)).or_insert(0);
            *t = (*t + removed).min(ctx.log_end);
        } else if boons.contains(&id) && e.iff != 0 {
            s.boon_strips += 1;
            let t = strip_times.entry(id).or_insert(0);
            *t = (*t + removed).min(ctx.log_end);
        }
    }
    let (mut cleanse_ms, mut self_ms) = (0, 0);
    for (ally, count) in &counts {
        let ms: i64 = times
            .iter()
            .filter(|((a, _), _)| a == ally)
            .map(|(_, t)| *t)
            .sum();
        if *ally == address {
            s.condition_cleanse_self += count;
            self_ms += ms;
        } else {
            s.condition_cleanse += count;
            cleanse_ms += ms;
        }
    }
    s.condition_cleanse_time = seconds(cleanse_ms);
    s.condition_cleanse_time_self = seconds(self_ms);
    s.boon_strips_time = seconds(strip_times.values().sum());
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(is_buff: u8, result: u8, value: i32, buff_dmg: i32) -> evtc_ingest::CombatRecord {
        evtc_ingest::CombatRecord {
            time: 0,
            src_agent: 1,
            dst_agent: 2,
            value,
            buff_dmg,
            overstack_value: 0,
            skill_id: 1,
            src_instid: 0,
            dst_instid: 0,
            src_master_instid: 0,
            dst_master_instid: 0,
            iff: 1,
            is_buff,
            result,
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
    fn direct_results_set_the_flags_of_the_event() {
        let crit = classify(&record(0, 1, 100, 0)).unwrap();
        assert!(crit.direct && crit.has_hit && crit.crit && crit.health == 100);
        let block = classify(&record(0, 3, 0, 0)).unwrap();
        assert!(block.blocked && !block.has_hit);
        assert!(classify(&record(0, 10, 5, 0)).is_none());
    }

    #[test]
    fn buff_results_use_the_buff_damage_and_leech_counts_as_hit() {
        let tick = classify(&record(1, 14, 0, 40)).unwrap();
        assert!(!tick.direct && tick.has_hit && tick.health == 40);
        let leech = classify(&record(1, 16, 0, 7)).unwrap();
        assert!(leech.leech && leech.has_hit);
        assert!(classify(&record(1, 0, 9, 9)).is_none());
    }

    #[test]
    fn interrupts_kills_and_downs_are_no_damage_events() {
        let killed = classify(&record(0, 8, 0, 0)).unwrap();
        assert!(killed.no_damage && killed.killed);
        let mut s = OffensiveStats::default();
        add_offense(&mut s, &killed, 1, false);
        assert_eq!((s.killed, s.total_damage_count), (1, 0));
    }

    #[test]
    fn the_listed_skills_never_crit() {
        assert!(!can_crit(13014));
        assert!(can_crit(12345));
    }
}
