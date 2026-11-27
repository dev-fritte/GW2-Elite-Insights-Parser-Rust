//! `LogLogic.SpecialDamageEventProcess`: damage events that an encounter rewrites after parsing.

use crate::generated::state_change as sc;
use crate::keep::Kept;
use crate::offense::classify;
use std::collections::{HashMap, HashSet};

/// `SkillIDs.FanaticalResilience`.
const FANATICAL_RESILIENCE: u32 = 38_226;
/// `TargetID.Samarog`.
const SAMAROG: u16 = 17_188;
/// `ParserHelper.ServerDelayConstant`.
const SERVER_DELAY: i64 = 10;

/// Applies the rewrites of the encounter to the kept records.
pub(crate) fn process(kept: &mut Kept) {
    if kept.trigger_id == SAMAROG {
        fanatical_resilience(kept);
    }
}

/// `Samarog.SpecialDamageEventProcess`: a hit that deals no damage to an agent with Fanatical
/// Resilience was absorbed. The record becomes an absorbed hit (`MakeIntoAbsorbed`).
fn fanatical_resilience(kept: &mut Kept) {
    let redirect = |kept: &Kept, a: u64| *kept.agent_redirection.get(&a).unwrap_or(&a);
    let mut buffed: HashSet<u64> = HashSet::new();
    for e in &kept.events {
        if e.skill_id == FANATICAL_RESILIENCE
            && matches!(e.state_change, sc::BUFF_APPLY | sc::BUFF_INITIAL)
            && e.dst_agent != 0
        {
            buffed.insert(redirect(kept, e.dst_agent));
        }
    }
    // the segments of the buff per agent (`GetBuffApplyRemoveSequence`, pairs of apply and remove)
    let mut segments: HashMap<u64, Vec<(i64, i64)>> = HashMap::new();
    for &agent in &buffed {
        // (time, is apply)
        let main: Vec<(i64, bool)> = kept
            .events
            .iter()
            .filter(|e| e.skill_id == FANATICAL_RESILIENCE)
            .filter_map(|e| match e.state_change {
                sc::BUFF_APPLY | sc::BUFF_INITIAL
                    if e.dst_agent != 0 && redirect(kept, e.dst_agent) == agent =>
                {
                    Some((e.time, true))
                }
                sc::BUFF_REMOVE_ALL if e.src_agent != 0 && redirect(kept, e.src_agent) == agent => {
                    Some((e.time, false))
                }
                _ => None,
            })
            .collect();
        let mut filtered: Vec<(i64, bool)> = Vec::new();
        let mut need_start = true;
        for (i, &(time, apply)) in main.iter().enumerate() {
            if need_start && apply {
                need_start = false;
                if let Some(&(previous, _)) = filtered.last()
                    && time - previous < SERVER_DELAY
                {
                    filtered.pop();
                    continue;
                }
                filtered.push((time, true));
            } else if !need_start && !apply && main.get(i + 1).is_none_or(|next| next.1) {
                need_start = true;
                filtered.push((time, false));
            }
        }
        let list = filtered
            .chunks(2)
            .map(|pair| (pair[0].0, pair.get(1).map_or(i64::MAX, |p| p.0)))
            .collect();
        segments.insert(agent, list);
    }
    let redirection = kept.agent_redirection.clone();
    for e in &mut kept.events {
        if e.state_change != sc::COMBAT || e.dst_agent == 0 {
            continue;
        }
        let dst = *redirection.get(&e.dst_agent).unwrap_or(&e.dst_agent);
        let Some(list) = segments.get(&dst) else {
            continue;
        };
        let Some(ev) = classify(e) else {
            continue;
        };
        if ev.has_hit && ev.health == 0 && list.iter().any(|&(s, end)| e.time >= s && e.time <= end)
        {
            // an absorbed hit: no result of a hit, no shield damage
            e.result = 6;
            e.is_shields = 0;
        }
    }
}
