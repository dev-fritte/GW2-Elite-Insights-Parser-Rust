//! Phases of the fight: the breakbar phases of `LogLogic.GetBreakbarPhases`.
//!
//! The phases that depend on the encounter (`GetPhases` of the logics) are not ported.

use evtc_ingest::CombatRecord;

use crate::encounter::Target;
use crate::generated::state_change as sc;
use crate::keep::Kept;

/// `ParserHelper.ServerDelayConstant`.
const SERVER_DELAY: i64 = 10;
/// `ParserHelper.PhaseTimeLimit`: shorter phases are dropped.
const PHASE_TIME_LIMIT: i64 = 1000;
/// `ParserHelper.BreakbarPhaseTimeBuildup`: how long before the breakbar turns active the phase starts.
const BREAKBAR_BUILDUP: i64 = 2000;

/// A phase in which a target has an active breakbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BreakbarPhase {
    /// Start of the phase.
    pub start: i64,
    /// End of the phase.
    pub end: i64,
    /// Index of the target in the target list.
    pub target: usize,
    /// How long before the breakbar became active the phase starts.
    pub offset: i64,
}

/// A segment of time with a start before its end.
type Segment = (i64, i64);

fn add_segment(list: &mut Vec<Segment>, start: i64, end: i64) {
    if start < end {
        list.push((start, end));
    }
}

/// The times a target is neither down, dead nor gone (`SingleActor.FillStatus`).
fn active_segments(events: &[(i64, u8)], first: i64, last: i64) -> Vec<Segment> {
    let mut actives = Vec::new();
    if events.is_empty() {
        add_segment(&mut actives, first, last);
        return actives;
    }
    let mut events = events.to_vec();
    events.sort_by_key(|e| e.0);
    for (i, &(time, kind)) in events.iter().enumerate() {
        let next = events.get(i + 1).map_or(last, |n| n.0);
        match kind {
            sc::CHANGE_DOWN | sc::CHANGE_DEAD | sc::DESPAWN => {
                if i == 0 {
                    add_segment(&mut actives, first, time);
                }
            }
            _ => add_segment(&mut actives, time, next),
        }
    }
    actives
}

/// The times the breakbar of a target is active (`GetAgentBreakbarStatus`).
fn breakbar_actives(events: &[(i64, i64)], last: i64) -> Vec<Segment> {
    let mut actives = Vec::new();
    for (i, &(time, state)) in events.iter().enumerate() {
        let next = match events.get(i + 1) {
            Some(n) => n.0,
            None if last - time >= SERVER_DELAY => last,
            None => continue,
        };
        // `BreakbarState.Active`
        if state == 0 {
            add_segment(&mut actives, time, next);
        }
    }
    actives
}

/// The breakbar phases of the targets, in the order of the targets and their breakbar segments.
/// Times are relative to the start of the fight, which lasts `duration`.
#[must_use]
pub fn breakbar_phases(
    kept: &Kept,
    targets: &[Target],
    offset: i64,
    duration: i64,
) -> Vec<BreakbarPhase> {
    let redirect = |a: u64| *kept.agent_redirection.get(&a).unwrap_or(&a);
    let of_target = |t: &Target, e: &CombatRecord| {
        e.src_agent != 0 && t.addresses.contains(&redirect(e.src_agent))
    };
    let forbidden =
        ei_data::targets::logic(i32::from(kept.trigger_id)).map_or(&[][..], |l| l.no_breakbar);
    let mut phases = Vec::new();
    for (index, target) in targets.iter().enumerate() {
        if forbidden.contains(&target.species) {
            continue;
        }
        let target_start = target.first.max(0);
        let target_end = target.last.min(duration);
        let status: Vec<(i64, u8)> = kept
            .events
            .iter()
            .filter(|e| {
                matches!(
                    e.state_change,
                    sc::CHANGE_DOWN | sc::CHANGE_UP | sc::CHANGE_DEAD | sc::SPAWN | sc::DESPAWN
                ) && of_target(target, e)
            })
            .map(|e| (e.time - offset, e.state_change))
            .collect();
        let actives = active_segments(&status, target.first, target.last);
        let breakbar_states: Vec<(i64, i64)> = kept
            .events
            .iter()
            .filter(|e| e.state_change == sc::BREAKBAR_STATE && of_target(target, e))
            .map(|e| (e.time - offset, i64::from(e.value)))
            .collect();
        for (start, end) in breakbar_actives(&breakbar_states, target.last) {
            let intersects = actives.iter().any(|&(s, e)| !(e < start || end < s));
            if (end - start).abs() < SERVER_DELAY || !intersects {
                continue;
            }
            let phase_start = (start - BREAKBAR_BUILDUP).max(target_start);
            let phase_end = end.min(target_end);
            if phase_end - phase_start < PHASE_TIME_LIMIT {
                continue;
            }
            phases.push(BreakbarPhase {
                start: phase_start,
                end: phase_end,
                target: index,
                offset: start - phase_start,
            });
        }
    }
    phases
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_target_without_status_events_is_active_while_it_is_known() {
        assert_eq!(active_segments(&[], 5, 50), vec![(5, 50)]);
    }

    #[test]
    fn the_time_before_the_first_death_is_active() {
        let events = [(40, sc::CHANGE_DEAD)];
        assert_eq!(active_segments(&events, 0, 100), vec![(0, 40)]);
    }

    #[test]
    fn only_the_active_breakbar_states_become_segments() {
        // active, immune, active until the end
        let states = [(100, 0), (200, 2), (300, 0)];
        assert_eq!(breakbar_actives(&states, 500), vec![(100, 200), (300, 500)]);
    }

    #[test]
    fn a_state_in_the_last_ten_milliseconds_adds_no_segment() {
        assert_eq!(breakbar_actives(&[(495, 0)], 500), Vec::<Segment>::new());
    }
}
