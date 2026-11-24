//! Damage of an agent: the health damage events the C# parser builds from combat records
//! (`CombatEventFactory.AddDirectDamageEvent` and `AddBuffDamageDamageEvent`) summed up like
//! `DamageStatistics.ComputeDamageFrom` does.
//!
//! Supported: logs of arcdps build 20260501 and later (`ResultEnumRework`). Not ported: damage of
//! minions, damage taken, breakbar damage, encounter specific changes of damage events.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use ei_data::buffs::{BUFFS, Classification};

use crate::buff_events::LogContext;

const RESULT_ENUM_REWORK_BUILD: i32 = 20_260_501;
/// DamageResult.BreakbarDamage.
const BREAKBAR_DAMAGE: u8 = 10;

/// `DamageResult` values of direct damage that has a health value.
fn is_direct_hit(result: u8) -> bool {
    // normal, crit, glance, block, evade, absorb, blind, invert
    matches!(result, 0..=4 | 6 | 7 | 13)
}

/// `DamageResult` values of buff damage that has a health value.
fn is_buff_hit(result: u8) -> bool {
    // cycle, not cycle and its variants, absorb, invert
    matches!(result, 14..=18 | 6 | 13)
}

/// Species ids of the targets whose barrier damage an encounter does not count as damage:
/// Twin Largos (Nikare and Kenut) and Sabir. The key is the trigger id of the log.
#[must_use]
pub fn barrier_negated_species(trigger_id: u16) -> &'static [u32] {
    match trigger_id {
        21_105 | 21_089 => &[21_105, 21_089],
        21_964 => &[21_964],
        _ => &[],
    }
}

/// Summed damage of an agent to foes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DamageTotals {
    /// All health damage (`ActorDamage`).
    pub damage: i64,
    /// Direct damage and buff damage that is not condition damage (`ActorPowerDamage`).
    pub power: i64,
    /// Buff damage of condition buffs (`ActorConditionDamage`).
    pub condition: i64,
    /// Direct damage only (`ActorStrikeDamage`).
    pub strike: i64,
    /// Breakbar damage in tenths of a point: the sum of `value` of the breakbar damage records.
    pub breakbar_tenths: i64,
}

impl DamageTotals {
    /// Breakbar damage as the C# JSON has it (each event rounded to one decimal, the sum too).
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn breakbar(&self) -> f64 {
        self.breakbar_tenths as f64 / 10.0
    }

    /// Damage per second like `DamageStatistics`: rounded to even, 0 for an empty span.
    #[must_use]
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    pub fn per_second(value: i64, start: i64, end: i64) -> i64 {
        let seconds = (end - start) as f64 / 1000.0;
        if seconds > 0.0 {
            (value as f64 / seconds).round_ties_even() as i64
        } else {
            0
        }
    }
}

/// Why the damage of a log cannot be computed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnsupportedBuild(pub i32);

impl std::fmt::Display for UnsupportedBuild {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "arcdps build {} is not supported for damage", self.0)
    }
}

impl std::error::Error for UnsupportedBuild {}

/// The ids of the buffs of a classification that exist in the builds of a log (cached: the buff
/// table has thousands of entries and is asked for once per player and target).
pub(crate) fn classified_ids(
    ctx: &LogContext<'_>,
    classification: Classification,
) -> Rc<HashSet<i64>> {
    type Key = (u64, i32, Classification);
    thread_local! {
        static CACHE: RefCell<HashMap<Key, Rc<HashSet<i64>>>> = RefCell::new(HashMap::new());
    }
    let key = (ctx.gw2_build, ctx.kept.version.build, classification);
    CACHE.with(|cache| {
        cache
            .borrow_mut()
            .entry(key)
            .or_insert_with(|| {
                Rc::new(
                    BUFFS
                        .iter()
                        .filter(|d| {
                            ctx.gw2_build < d.max_build
                                && ctx.gw2_build >= d.min_build
                                && ctx.kept.version.build < d.max_evtc_build
                                && ctx.kept.version.build >= d.min_evtc_build
                        })
                        .filter(|d| d.classification == classification)
                        .map(|d| d.id)
                        .collect(),
                )
            })
            .clone()
    })
}

/// The ids of the condition buffs of a log (`SkillEvent.ConditionDamageBased`).
pub(crate) fn condition_ids(ctx: &LogContext<'_>) -> Rc<HashSet<i64>> {
    classified_ids(ctx, Classification::Condition)
}
/// Damage of the agent at `address` to everything that is not friendly, between the start of the
/// log and `ctx.log_end` (times with `ctx.time_offset` applied).
///
/// `barrier_negated` holds the addresses of targets whose barrier damage does not count as health
/// damage (`LogLogicUtils.NegateDamageAgainstBarrier`, used by some encounters).
///
/// With `only_to`, just the damage to that target counts.
///
/// # Errors
/// [`UnsupportedBuild`] for logs older than the result enum rework.
#[allow(clippy::implicit_hasher)]
pub fn actor_damage(
    ctx: &LogContext<'_>,
    events: &[&evtc_ingest::CombatRecord],
    address: u64,
    barrier_negated: &HashSet<u64>,
    only_to: Option<&HashSet<u64>>,
) -> Result<DamageTotals, UnsupportedBuild> {
    let kept = ctx.kept;
    if kept.version.build < RESULT_ENUM_REWORK_BUILD {
        return Err(UnsupportedBuild(kept.version.build));
    }
    let conditions = condition_ids(ctx);
    let redirect = |a: u64| *kept.agent_redirection.get(&a).unwrap_or(&a);
    let mut totals = DamageTotals::default();
    for e in events.iter().copied() {
        if e.state_change != crate::generated::state_change::COMBAT {
            continue;
        }
        let time = e.time - ctx.time_offset;
        if time < ctx.log_start || time > ctx.log_end {
            continue;
        }
        // `ToFriendly`: iff 0 is friend
        if e.iff == 0 || e.src_agent == 0 || redirect(e.src_agent) != address {
            continue;
        }
        if only_to.is_some_and(|t| !t.contains(&redirect(e.dst_agent))) {
            continue;
        }
        if e.result == BREAKBAR_DAMAGE {
            totals.breakbar_tenths += i64::from(e.value);
            continue;
        }
        if e.is_buff == 0 {
            if is_direct_hit(e.result) {
                let mut health = i64::from(e.value);
                if barrier_negated.contains(&redirect(e.dst_agent)) && e.is_shields > 0 {
                    health = (health - i64::from(e.overstack_value)).max(0);
                }
                totals.damage += health;
                totals.power += health;
                totals.strike += health;
            }
        } else if is_buff_hit(e.result) {
            let mut health = i64::from(e.buff_dmg);
            if barrier_negated.contains(&redirect(e.dst_agent)) && e.is_shields > 0 {
                let shield = if e.overstack_value > 0 {
                    i64::from(e.overstack_value)
                } else {
                    health
                };
                health = (health - shield).max(0);
            }
            totals.damage += health;
            if conditions.contains(&i64::from(e.skill_id)) {
                totals.condition += health;
            } else {
                totals.power += health;
            }
        }
    }
    Ok(totals)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn results_with_a_health_value() {
        assert!(is_direct_hit(0) && is_direct_hit(4) && is_direct_hit(13));
        assert!(!is_direct_hit(5) && !is_direct_hit(10) && !is_direct_hit(12));
        assert!(is_buff_hit(14) && is_buff_hit(18) && is_buff_hit(6));
        assert!(!is_buff_hit(10) && !is_buff_hit(8));
    }
}

#[cfg(test)]
mod per_second_tests {
    use super::DamageTotals;

    #[test]
    fn dps_rounds_to_even_and_handles_empty_spans() {
        // 1500 damage in 1000 ms is 1500; 5 damage in 2000 ms is 2.5, rounded to 2
        assert_eq!(DamageTotals::per_second(1500, 0, 1000), 1500);
        assert_eq!(DamageTotals::per_second(5, 0, 2000), 2);
        assert_eq!(DamageTotals::per_second(7, 0, 2000), 4);
        assert_eq!(DamageTotals::per_second(100, 500, 500), 0);
    }

    #[test]
    fn breakbar_is_kept_in_tenths() {
        let totals = DamageTotals {
            breakbar_tenths: 6005,
            ..DamageTotals::default()
        };
        assert!((totals.breakbar() - 600.5).abs() < 1e-9);
    }
}

/// The health damage of a combat record, 0 if it is not a damage record (`HealthDamage`).
#[must_use]
pub fn health_damage(e: &evtc_ingest::CombatRecord) -> i64 {
    if e.is_buff == 0 {
        if is_direct_hit(e.result) {
            i64::from(e.value)
        } else {
            0
        }
    } else if is_buff_hit(e.result) {
        i64::from(e.buff_dmg)
    } else {
        0
    }
}
