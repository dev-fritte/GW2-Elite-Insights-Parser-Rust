//! Damage graphs: the cumulative damage per second of the fight (`SingleActorGraphsHelper`), and
//! the damage totals that include the minions of the player.
//!
//! Supported: logs of arcdps build 20260501 and later, the whole fight as one phase.

use std::collections::HashSet;

use crate::buff_events::LogContext;
use crate::damage::{UnsupportedBuild, condition_ids};
use crate::generated::state_change as sc;
use crate::offense::classify;

const RESULT_ENUM_REWORK_BUILD: i32 = 20_260_501;

/// Which hits a graph counts (`ParserHelper.DamageType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DamageKind {
    /// All hits.
    All,
    /// Hits that are not condition damage.
    Power,
    /// Condition damage.
    Condition,
}

/// Whose events a graph is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Damage the agent (and its minions) deals to foes.
    Dealt,
    /// Damage the agent takes.
    Taken,
}

/// The hits (time relative to the start of the fight, health damage) of an agent, ordered by time.
///
/// `only_to` restricts dealt damage to those targets; `barrier_negated` are the targets whose barrier
/// damage does not count.
///
/// # Errors
/// [`UnsupportedBuild`] for logs older than the result enum rework.
#[allow(clippy::implicit_hasher)]
pub fn hits(
    ctx: &LogContext<'_>,
    address: u64,
    direction: Direction,
    kind: DamageKind,
    barrier_negated: &HashSet<u64>,
    only_to: Option<&HashSet<u64>>,
) -> Result<Vec<(i64, i64)>, UnsupportedBuild> {
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
    let mut out = Vec::new();
    for e in &kept.events {
        if e.state_change != sc::COMBAT {
            continue;
        }
        let time = e.time - ctx.time_offset;
        if time < ctx.log_start || time > ctx.log_end {
            continue;
        }
        let dst = redirect(e.dst_agent);
        match direction {
            Direction::Dealt => {
                if e.iff == 0 || e.src_agent == 0 {
                    continue;
                }
                let minion = e.src_master_instid != 0 && e.src_master_instid == instid;
                if redirect(e.src_agent) != address && !minion {
                    continue;
                }
                if only_to.is_some_and(|t| !t.contains(&dst)) {
                    continue;
                }
            }
            Direction::Taken => {
                if e.dst_agent == 0 || dst != address {
                    continue;
                }
            }
        }
        let Some(mut ev) = classify(e) else {
            continue;
        };
        if !ev.has_hit {
            continue;
        }
        let condition = conditions.contains(&i64::from(e.skill_id));
        if (kind == DamageKind::Power && condition) || (kind == DamageKind::Condition && !condition)
        {
            continue;
        }
        if direction == Direction::Dealt && barrier_negated.contains(&dst) && ev.shield > 0 {
            ev.health = (ev.health - ev.shield).max(0);
        }
        out.push((time, ev.health));
    }
    Ok(out)
}

/// The cumulative damage at every full second of the fight (`ComputeDamageGraph`): entry `i` holds
/// the damage up to the time `i` seconds after the start, the last entry the total.
///
/// # Panics
/// Never for times inside `0..=duration`.
#[must_use]
pub fn damage_graph(hits: &[(i64, i64)], duration: i64) -> Vec<i64> {
    let seconds = duration / 1000;
    let len = if seconds * 1000 == duration {
        seconds + 1
    } else {
        seconds + 2
    };
    let mut values = vec![0i64; usize::try_from(len).unwrap_or(0)];
    let mut previous = 0usize;
    for &(time, health) in hits {
        // `Math.Ceiling((time - start) / 1000.0)`
        let index = usize::try_from((time + 999).div_euclid(1000)).unwrap_or(0);
        let index = index.min(values.len() - 1);
        if index != previous {
            for i in previous + 1..=index {
                values[i] = values[previous];
            }
        }
        previous = index;
        values[index] += health;
    }
    for i in previous + 1..values.len() {
        values[i] = values[previous];
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_graph_is_cumulative_per_second_and_ends_with_the_total() {
        let graph = damage_graph(&[(0, 5), (1, 10), (1000, 1), (2500, 7)], 3200);
        assert_eq!(graph, vec![5, 16, 16, 23, 23]);
    }

    #[test]
    fn an_exact_number_of_seconds_has_no_extra_entry() {
        assert_eq!(damage_graph(&[], 2000).len(), 3);
        assert_eq!(damage_graph(&[], 2001).len(), 4);
    }
}
