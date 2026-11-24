//! From arcdps records to buff simulations: the part of the C# `BuffDictionary` and the buff
//! event classes that decides what the simulators see.
//!
//! Supported: logs of arcdps build 20260501 and later, where buff applies and removes are state
//! changes. Older logs return [`Error::UnsupportedBuild`].
//!
//! Not ported: source finding (who gets credit), the stack-id corrections of `BuffsContainer`
//! (`OverrideRemovedDuration`, only used with stack ids), profession specific rewrites of buff
//! events (Weaver, Willbender, Virtuoso, Elementalist dual buffs), custom nourishment buffs.

use std::collections::HashMap;

use ei_buffs::{FULL_REMOVAL_BUT_IGNORE_FOR_WASTE, SimEvent, SimItem, Simulator, UNKNOWN_SRC};
use ei_data::buffs::{BUFFS, BuffDef, StackType};
use evtc_ingest::CombatRecord;

use crate::generated::state_change as sc;
use crate::keep::Kept;

/// `ArcDPSBuilds.BuffAppliesAndRemovesAsStateChanges`.
const STATE_CHANGE_BUILD: i32 = 20_260_501;
/// `ParserHelper.ServerDelayConstant`.
const SERVER_DELAY: i64 = 10;
/// `ParserHelper.BuffSimulatorDelayConstant`.
const SIM_DELAY: i64 = 15;
/// `ArcDPSBuilds.BuffExtensionBroken`.
const EXTENSION_BROKEN_BUILD: i32 = 20_230_905;
/// `BuffRemove.Single` / `BuffRemove.Manual` in the `is_buff_remove` byte.
const REMOVE_SINGLE: u8 = 2;
/// The `IFF` byte values that are not friend (0) or foe (1) are `IFF.Unknown`.
const IFF_FOE: u8 = 1;

/// Why buffs could not be simulated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// The log is older than the state change based buff records.
    UnsupportedBuild(i32),
    /// The simulator rejected the events of a buff.
    Simulator(i64, ei_buffs::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedBuild(b) => write!(f, "arcdps build {b} is not supported for buffs"),
            Self::Simulator(id, e) => write!(f, "buff {id}: {e}"),
        }
    }
}

impl std::error::Error for Error {}

/// The simulation of one buff on one agent.
#[derive(Debug, Clone)]
pub struct BuffResult {
    /// The definition of the buff.
    pub buff: &'static BuffDef,
    /// Time segments of the buff, see [`ei_buffs::SimItem`].
    pub items: Vec<SimItem>,
}

impl BuffResult {
    /// The graph of the buff: stack count over `[start, end]`, with zero segments for the gaps,
    /// as `(start, value)` pairs of the value changes.
    #[must_use]
    pub fn states(&self, start: i64, end: i64) -> Vec<(i64, u32)> {
        let mut out: Vec<(i64, u32)> = Vec::new();
        let mut cursor = start;
        for item in &self.items {
            if item.start > cursor {
                push_state(&mut out, cursor, 0);
            }
            push_state(&mut out, item.start, item.active_stacks);
            cursor = item.end;
        }
        if cursor < end || out.is_empty() {
            push_state(&mut out, cursor, 0);
        }
        out
    }
}

fn push_state(out: &mut Vec<(i64, u32)>, time: i64, value: u32) {
    // a zero length segment before this one is replaced
    while out.last().is_some_and(|l| l.0 == time) {
        out.pop();
    }
    if out.last().is_some_and(|l| l.1 == value) {
        return;
    }
    out.push((time, value));
}

/// What the simulation needs to know about the log.
#[derive(Debug, Clone, Copy)]
pub struct LogContext<'a> {
    /// Filtered records of the log.
    pub kept: &'a Kept,
    /// GW2 build of the log.
    pub gw2_build: u64,
    /// Time of the first log moment (0 after the time normalisation).
    pub log_start: i64,
    /// Time shift of the encounter logic (`GetLogOffset`), subtracted from every event time.
    pub time_offset: i64,
    /// Time of the last log moment.
    pub log_end: i64,
}

fn available(def: &BuffDef, gw2_build: u64, evtc_build: i32) -> bool {
    gw2_build < def.max_build
        && gw2_build >= def.min_build
        && evtc_build < def.max_evtc_build
        && evtc_build >= def.min_evtc_build
}

struct Pending {
    sim: SimEvent,
    /// Stack instance id for the extension duplicate check, 0 if none.
    instance: u32,
    initial: bool,
    /// `BuffApplyEvent.OriginalAppliedDuration`, 0 for other events.
    original_duration: i64,
    /// Extension: the new duration the record reports. Deactivate: the duration it resets to.
    extra: i64,
    /// `false` for a remove event the C# parser does not feed to the simulator.
    compliant: bool,
}

/// `CombatData.OffsetBuffExtensionEvents` for one buff on one agent: the new duration of arcdps
/// extensions counts from the original stack, so the extension has to be corrected by the time the
/// stack ran inactive.
#[allow(clippy::too_many_lines)]
fn offset_extensions(list: &mut [Pending], conditional_loss: bool) {
    for i in 0..list.len() {
        let SimEvent::Extend {
            time, extension, ..
        } = list[i].sim
        else {
            continue;
        };
        let instance = list[i].instance;
        if instance == 0 {
            continue;
        }
        let Some(apply) = list[..i].iter().rposition(|p| {
            matches!(p.sim, SimEvent::Apply { time: t, .. } if t <= time) && p.instance == instance
        }) else {
            // an apply later in the list can not precede the extension in time order
            continue;
        };
        let SimEvent::Apply {
            time: apply_time,
            duration: applied,
            ..
        } = list[apply].sim
        else {
            continue;
        };
        // the sequence: the apply, its stack events up to the extension, the previous extension
        let previous_extension = list[..i]
            .iter()
            .rposition(|p| matches!(p.sim, SimEvent::Extend { .. }) && p.instance == instance);
        let mut sequence: Vec<usize> = vec![apply];
        for (j, p) in list.iter().enumerate() {
            if p.instance == instance
                && matches!(
                    p.sim,
                    SimEvent::Activate { .. } | SimEvent::Deactivate { .. }
                )
                && p.sim.time() >= apply_time
                && p.sim.time() <= time
            {
                sequence.push(j);
            }
        }
        if let Some(p) = previous_extension
            && list[p].sim.time() >= apply_time
        {
            sequence.push(p);
        }
        sequence.sort_by_key(|&j| list[j].sim.time());

        let mut active_time = 0_i64;
        let mut previous_time = i64::MIN;
        let mut original_stack_duration = 0_i64;
        let mut added_extension = 0_i64;
        for (n, &j) in sequence.iter().enumerate() {
            let cur = &list[j];
            if n == 0 {
                original_stack_duration = cur.original_duration;
                if cur.initial {
                    active_time += cur.original_duration - applied;
                }
            } else {
                match cur.sim {
                    SimEvent::Activate { .. } => {
                        if conditional_loss {
                            added_extension += cur.sim.time() - previous_time;
                            active_time += cur.sim.time() - previous_time;
                        }
                    }
                    SimEvent::Deactivate { .. } => {
                        active_time += cur.sim.time() - previous_time;
                        original_stack_duration = cur.extra;
                    }
                    _ => {
                        // an earlier extension: a stack reset in disguise if it is not longer
                        if cur.extra <= original_stack_duration {
                            active_time = 0;
                        } else {
                            continue;
                        }
                    }
                }
            }
            previous_time = cur.sim.time();
        }
        active_time += time - previous_time;
        let new_duration = list[i].extra;
        let extended = if new_duration <= original_stack_duration {
            active_time
        } else {
            extension + added_extension
        };
        let new_duration = if new_duration <= original_stack_duration {
            new_duration
        } else {
            new_duration - active_time
        };
        list[i].sim = SimEvent::Extend {
            time,
            extension: extended,
            old_value: new_duration - extended,
            src: match list[i].sim {
                SimEvent::Extend { src, .. } => src,
                _ => UNKNOWN_SRC,
            },
            src_healing: 0,
            stack_id: instance,
        };
        list[i].compliant = new_duration >= 0;
    }
}

/// `BuffsContainer`: stacks removed with their full original duration lose the time they already
/// ran, so the simulator can match them. Only applied to logs with stack ids.
fn fix_removed_durations(list: &mut [Pending], conditional_loss: bool) {
    let qualifies = |p: &Pending| {
        matches!(p.sim, SimEvent::RemoveSingle { removed_duration, .. }
            if conditional_loss || removed_duration == i64::from(i32::MAX))
    };
    if !list.iter().any(qualifies) {
        return;
    }
    for i in 0..list.len() {
        if !qualifies(&list[i]) {
            continue;
        }
        let SimEvent::RemoveSingle {
            time: remove_time,
            removed_duration,
        } = list[i].sim
        else {
            continue;
        };
        let instance = list[i].instance;
        let Some(apply_index) = list.iter().rposition(|p| {
            matches!(p.sim, SimEvent::Apply { time, .. } if time <= remove_time)
                && p.instance == instance
        }) else {
            continue;
        };
        let SimEvent::Apply {
            time: apply_time,
            duration: applied,
            ..
        } = list[apply_index].sim
        else {
            continue;
        };
        let mut total = list[apply_index].original_duration;
        let mut previous = apply_time;
        for other in list.iter() {
            if other.instance != instance {
                continue;
            }
            let time = other.sim.time();
            match other.sim {
                SimEvent::Extend { extension, .. } => {
                    if time >= apply_time && time <= remove_time {
                        total += extension;
                    }
                }
                SimEvent::Activate { .. } => {
                    if time >= apply_time && time <= remove_time {
                        total -= time - previous;
                    }
                }
                _ => continue,
            }
            previous = time;
        }
        if total == removed_duration {
            let active_time = list[apply_index].original_duration - applied;
            let elapsed = remove_time - apply_time;
            list[i].sim = SimEvent::RemoveSingle {
                time: remove_time,
                removed_duration: (removed_duration - active_time - elapsed).max(0),
            };
        }
    }
}

/// Simulates every buff the agent at `address` has events for.
///
/// # Errors
/// [`Error::UnsupportedBuild`] for old logs, [`Error::Simulator`] if a buff cannot be simulated.
#[allow(clippy::too_many_lines)]
pub fn simulate_agent(ctx: &LogContext<'_>, address: u64) -> Result<Vec<BuffResult>, Error> {
    let kept = ctx.kept;
    let shifted: Vec<CombatRecord> = kept
        .events
        .iter()
        .map(|e| CombatRecord {
            time: e.time - ctx.time_offset,
            ..*e
        })
        .collect();
    if kept.version.build < STATE_CHANGE_BUILD {
        return Err(Error::UnsupportedBuild(kept.version.build));
    }
    let defs: HashMap<i64, &'static BuffDef> = BUFFS
        .iter()
        .filter(|d| available(d, ctx.gw2_build, kept.version.build))
        .fold(HashMap::new(), |mut m, d| {
            m.entry(d.id).or_insert(d);
            m
        });
    let max_stacks: HashMap<u32, u16> = kept
        .events
        .iter()
        .filter(|e| e.state_change == sc::BUFF_INFO)
        .map(|e| (e.skill_id, e.src_master_instid))
        .collect();
    let redirect = |a: u64| *kept.agent_redirection.get(&a).unwrap_or(&a);

    let has_stack_ids = kept
        .events
        .iter()
        .any(|e| e.state_change == sc::STACK_ACTIVE || e.state_change == sc::STACK_DEACTIVE);
    let mut per_buff: HashMap<i64, Vec<Pending>> = HashMap::new();
    let mut spawns = Vec::new();
    let mut despawns = Vec::new();
    let mut last_aware = i64::MIN;

    // Records without an agent address but with an instance id (despawns of this arcdps build)
    // are given to the agent that has this instance id and is known around that time
    // (`orphanedSrcInstidCombatItems` of `CompleteAgentsAndLogData`).
    let mut agent_instance: u16 = 0;
    let (mut first_aware, mut final_aware) = (i64::MAX, i64::MIN);
    for e in &shifted {
        let src = crate::rules::src_is_agent(e.state_change)
            && e.src_agent != 0
            && redirect(e.src_agent) == address;
        let dst = crate::rules::dst_is_agent(e.state_change)
            && e.dst_agent != 0
            && redirect(e.dst_agent) == address;
        if src || dst {
            if agent_instance == 0 {
                agent_instance = if src { e.src_instid } else { e.dst_instid };
            }
            first_aware = first_aware.min(e.time);
            final_aware = final_aware.max(e.time);
        }
    }
    let known_around = |t: i64| {
        (first_aware..=final_aware).contains(&(t - 300))
            || (first_aware..=final_aware).contains(&(t + 300))
    };
    let orphan = |address_field: u64, instid: u16, t: i64| {
        address_field == 0 && instid > 0 && instid == agent_instance && known_around(t)
    };

    for e in &shifted {
        let is_src = crate::rules::src_is_agent(e.state_change)
            && ((e.src_agent != 0 && redirect(e.src_agent) == address)
                || orphan(e.src_agent, e.src_instid, e.time));
        let is_dst = crate::rules::dst_is_agent(e.state_change)
            && ((e.dst_agent != 0 && redirect(e.dst_agent) == address)
                || orphan(e.dst_agent, e.dst_instid, e.time));
        match e.state_change {
            sc::SPAWN if is_src => spawns.push(e.time),
            sc::DESPAWN if is_src => despawns.push(e.time),
            _ => {}
        }
        if is_src || is_dst {
            last_aware = last_aware.max(e.time);
        }
        let target = match e.state_change {
            sc::BUFF_INITIAL | sc::BUFF_APPLY | sc::BUFF_CHANGE => is_dst,
            sc::BUFF_REMOVE_SINGLE
            | sc::BUFF_REMOVE_ALL
            | sc::STACK_ACTIVE
            | sc::STACK_DEACTIVE => is_src,
            _ => false,
        };
        if !target {
            continue;
        }
        let id = i64::from(e.skill_id);
        if !defs.contains_key(&id) {
            continue;
        }
        if let Converted::Event(p) = convert(e) {
            per_buff.entry(id).or_default().push(p);
        }
    }

    // `BuffDictionary.Finalize`: clear the buffs around spawn, despawn and the last time seen
    let clear = |time: i64| Pending {
        sim: SimEvent::RemoveAll {
            time,
            removed_stacks: FULL_REMOVAL_BUT_IGNORE_FOR_WASTE,
        },
        instance: 0,
        initial: false,
        original_duration: 0,
        extra: 0,
        compliant: true,
    };
    let mut results = Vec::with_capacity(per_buff.len());
    let mut ids: Vec<i64> = per_buff.keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        let mut raw = per_buff.remove(&id).unwrap_or_default();
        let def = defs[&id];
        // `CombatData.OffsetBuffExtensionEvents`, then the removal fix of `BuffsContainer`
        if kept.version.build > EXTENSION_BROKEN_BUILD {
            offset_extensions(
                &mut raw,
                def.stack_type == StackType::StackingConditionalLoss,
            );
            raw.retain(|p| !matches!(p.sim, SimEvent::Extend { extension, .. } if extension < 1));
        }
        if has_stack_ids
            && matches!(
                def.stack_type,
                StackType::Stacking | StackType::StackingConditionalLoss
            )
        {
            fix_removed_durations(
                &mut raw,
                def.stack_type == StackType::StackingConditionalLoss,
            );
        }
        // `BuffDictionary.Add` and `AddRegen`
        let regen = def.stack_type == StackType::Regeneration;
        let mut list: Vec<Pending> = Vec::with_capacity(raw.len());
        let mut last_removed_regen: Option<(i64, i64, u32)> = None; // (time, duration, instance)
        for mut p in raw {
            if !p.compliant {
                if regen
                    && has_stack_ids
                    && let SimEvent::RemoveSingle {
                        time,
                        removed_duration,
                    } = p.sim
                    && removed_duration > SIM_DELAY
                {
                    last_removed_regen = Some((time, removed_duration, p.instance));
                }
                continue;
            }
            match p.sim {
                SimEvent::Activate { .. } if !regen => continue,
                SimEvent::Deactivate { .. } => continue,
                SimEvent::Apply {
                    time,
                    ref mut overridden_duration,
                    ref mut overridden_stack_id,
                    ..
                } if regen => {
                    if let Some((t, duration, instance)) = last_removed_regen.take()
                        && time - t < SERVER_DELAY
                    {
                        *overridden_duration = duration;
                        *overridden_stack_id = instance;
                    }
                }
                _ => {}
            }
            add_to_list(&mut list, p);
        }
        for &t in &despawns {
            list.push(clear(t + SERVER_DELAY));
        }
        for &t in &spawns {
            list.push(clear(t - SERVER_DELAY));
        }
        if last_aware != i64::MIN {
            list.push(clear(last_aware + SERVER_DELAY));
        }
        list.sort_by_key(|p| p.sim.time());
        let mut capacity = usize::try_from(def.capacity).unwrap_or(0);
        if let Some(&m) = max_stacks.get(&u32::try_from(id).unwrap_or(0))
            && m > 0
        {
            capacity = usize::from(m);
        }
        let events: Vec<SimEvent> = list.iter().map(|p| p.sim).collect();
        let mut sim = Simulator::new(def.stack_type, capacity);
        sim.simulate(&events, ctx.log_start, ctx.log_end)
            .map_err(|e| Error::Simulator(id, e))?;
        results.push(BuffResult {
            buff: def,
            items: sim.generation,
        });
    }
    Ok(results)
}

enum Converted {
    Event(Pending),
    Ignored,
}

fn src_id(address: u64) -> u32 {
    if address == 0 {
        UNKNOWN_SRC
    } else {
        // only used to tell sources apart; collisions are harmless for the segments
        #[allow(clippy::cast_possible_truncation)]
        let id = (address ^ (address >> 32)) as u32;
        id
    }
}

fn pending(sim: SimEvent, instance: u32) -> Pending {
    Pending {
        sim,
        instance,
        initial: false,
        original_duration: 0,
        extra: 0,
        compliant: true,
    }
}

fn convert(e: &CombatRecord) -> Converted {
    let time = e.time;
    match e.state_change {
        sc::BUFF_INITIAL | sc::BUFF_APPLY => {
            let initial = e.state_change == sc::BUFF_INITIAL;
            Converted::Event(Pending {
                initial,
                original_duration: if initial {
                    i64::from(e.buff_dmg)
                } else {
                    i64::from(e.value)
                },
                ..pending(
                    SimEvent::Apply {
                        time,
                        duration: i64::from(e.value),
                        src: src_id(e.src_agent),
                        src_healing: 0,
                        stack_id: e.pad,
                        added_active: e.is_shields > 0,
                        overridden_duration: 0,
                        overridden_stack_id: 0,
                    },
                    e.pad,
                )
            })
        }
        sc::BUFF_CHANGE => {
            let new_duration = i64::from(e.overstack_value);
            let extension = i64::from(e.value.max(0));
            Converted::Event(Pending {
                extra: new_duration,
                ..pending(
                    SimEvent::Extend {
                        time,
                        extension,
                        old_value: new_duration - extension,
                        src: src_id(e.src_agent),
                        src_healing: 0,
                        stack_id: e.pad,
                    },
                    e.pad,
                )
            })
        }
        sc::STACK_ACTIVE => {
            let instance = u32::try_from(e.dst_agent & 0xFFFF_FFFF).unwrap_or(0);
            if instance == 0 {
                return Converted::Ignored;
            }
            Converted::Event(pending(
                SimEvent::Activate {
                    time,
                    stack_id: instance,
                },
                instance,
            ))
        }
        sc::STACK_DEACTIVE => {
            if e.pad == 0 {
                return Converted::Ignored;
            }
            Converted::Event(Pending {
                extra: i64::from(e.value),
                ..pending(SimEvent::Deactivate { time }, e.pad)
            })
        }
        sc::BUFF_REMOVE_ALL => Converted::Event(pending(
            SimEvent::RemoveAll {
                time,
                removed_stacks: i32::from(e.result),
            },
            0,
        )),
        sc::BUFF_REMOVE_SINGLE => match e.is_buff_remove {
            REMOVE_SINGLE => {
                // `OverstackOrNaturalEnd`: unknown iff and no credited source
                let natural_end = e.iff > IFF_FOE && e.dst_agent == 0;
                Converted::Event(Pending {
                    compliant: !natural_end,
                    ..pending(
                        SimEvent::RemoveSingle {
                            time,
                            removed_duration: i64::from(e.value),
                        },
                        e.pad,
                    )
                })
            }
            _ => Converted::Ignored, // BuffRemove.Manual and unknown values
        },
        _ => Converted::Ignored,
    }
}
/// `BuffDictionary.AddToList`: drops useless extensions and duplicated initial applies.
fn add_to_list(list: &mut Vec<Pending>, p: Pending) {
    match p.sim {
        SimEvent::Extend {
            extension,
            old_value,
            time,
            ..
        } => {
            if extension < 1 {
                return;
            }
            if p.instance != 0 {
                let previous = list.iter().rposition(|x| {
                    matches!(x.sim, SimEvent::Extend { .. }) && x.instance == p.instance
                });
                if let Some(i) = previous
                    && let SimEvent::Extend {
                        time: last_time,
                        old_value: last_old,
                        extension: last_ext,
                        ..
                    } = list[i].sim
                    && (time - last_time).abs() <= 1
                {
                    if (old_value - last_old).abs() <= 1 {
                        list[i] = p;
                        return;
                    }
                    if ((old_value + extension) - (last_old + last_ext)).abs() <= 1 {
                        return;
                    }
                }
            }
        }
        SimEvent::Apply { time, .. } if p.initial && p.instance != 0 => {
            let duplicated = list.iter().any(|x| {
                matches!(x.sim, SimEvent::Apply { .. })
                    && !x.initial
                    && x.instance == p.instance
                    && (x.sim.time() - time).abs() <= 1
            });
            if duplicated {
                return;
            }
        }
        _ => {}
    }
    list.push(p);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(items: Vec<SimItem>) -> BuffResult {
        BuffResult {
            buff: &BUFFS[0],
            items,
        }
    }

    fn item(start: i64, end: i64, stacks: u32) -> SimItem {
        SimItem {
            start,
            end,
            sources: Vec::new(),
            active_stacks: stacks,
        }
    }

    #[test]
    fn states_fill_gaps_with_zero_and_merge_equal_values() {
        let r = result(vec![
            item(100, 200, 1),
            item(200, 300, 1),
            item(500, 600, 2),
        ]);
        assert_eq!(
            r.states(0, 1000),
            vec![(0, 0), (100, 1), (300, 0), (500, 2), (600, 0)]
        );
    }

    #[test]
    fn states_of_an_empty_buff_are_one_zero_segment() {
        assert_eq!(result(vec![]).states(0, 1000), vec![(0, 0)]);
    }
}

#[cfg(test)]
mod fix_tests {
    use super::*;

    fn pending(sim: SimEvent, instance: u32, original_duration: i64) -> Pending {
        Pending {
            original_duration,
            ..super::pending(sim, instance)
        }
    }

    #[test]
    fn a_removal_with_the_full_duration_loses_the_elapsed_time() {
        let apply = SimEvent::Apply {
            time: 1000,
            duration: 10_000,
            src: 1,
            src_healing: 0,
            stack_id: 7,
            added_active: false,
            overridden_duration: 0,
            overridden_stack_id: 0,
        };
        let remove = SimEvent::RemoveSingle {
            time: 1800,
            removed_duration: 10_000,
        };
        let mut list = vec![pending(apply, 7, 10_000), pending(remove, 7, 0)];
        fix_removed_durations(&mut list, true);
        assert_eq!(
            list[1].sim,
            SimEvent::RemoveSingle {
                time: 1800,
                removed_duration: 9200
            }
        );
    }

    #[test]
    fn removals_that_do_not_match_the_stack_stay_as_they_are() {
        let remove = SimEvent::RemoveSingle {
            time: 1800,
            removed_duration: 500,
        };
        let mut list = vec![pending(remove, 7, 0)];
        fix_removed_durations(&mut list, true);
        assert_eq!(list[0].sim, remove);
    }
}

#[cfg(test)]
mod offset_tests {
    use super::*;

    #[test]
    fn an_extension_counts_from_the_remaining_duration_of_the_stack() {
        let apply = Pending {
            original_duration: 6000,
            ..pending(
                SimEvent::Apply {
                    time: 1000,
                    duration: 6000,
                    src: 1,
                    src_healing: 0,
                    stack_id: 5,
                    added_active: false,
                    overridden_duration: 0,
                    overridden_stack_id: 0,
                },
                5,
            )
        };
        // arcdps reports 9000 as the new duration: 6000 original plus 3000 extension
        let extend = Pending {
            extra: 9000,
            ..pending(
                SimEvent::Extend {
                    time: 5280,
                    extension: 3000,
                    old_value: 6000,
                    src: 1,
                    src_healing: 0,
                    stack_id: 5,
                },
                5,
            )
        };
        let mut list = vec![apply, extend];
        offset_extensions(&mut list, false);
        // 4280 ms of the stack have run, so 1720 ms were left before the extension
        assert!(matches!(
            list[1].sim,
            SimEvent::Extend {
                extension: 3000,
                old_value: 1720,
                ..
            }
        ));
        assert!(list[1].compliant);
    }
}
