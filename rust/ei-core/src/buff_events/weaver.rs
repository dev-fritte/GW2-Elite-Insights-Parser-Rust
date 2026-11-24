//! Port of `WeaverHelper.TransformWeaverAttunements` and `ElementalistHelper.RemoveDualBuffs`.
//!
//! arcdps reports the attunements of a Weaver as several buffs at the same moment (a basic
//! attunement, or a major and a minor one, or a dual one). The C# parser replaces them by one
//! combined attunement buff per moment; elementalists that are not Weavers lose the dual buffs.

use std::collections::{HashMap, HashSet};

use ei_buffs::SimEvent;
use ei_data::skill_ids as id;

use super::{Pending, SERVER_DELAY, pending};

/// The kind of elementalist an agent is, as far as the buff events care.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementalistKind {
    /// Not an elementalist.
    None,
    /// An elementalist that is not a Weaver (the dual attunements are removed).
    Other,
    /// A Weaver (the attunements are translated).
    Weaver,
}

const DUALS: [i64; 4] = [
    id::DUAL_FIRE_ATTUNEMENT,
    id::DUAL_WATER_ATTUNEMENT,
    id::DUAL_AIR_ATTUNEMENT,
    id::DUAL_EARTH_ATTUNEMENT,
];
const MINORS: [i64; 4] = [
    id::FIRE_MINOR_ATTUNEMENT,
    id::WATER_MINOR_ATTUNEMENT,
    id::AIR_MINOR_ATTUNEMENT,
    id::EARTH_MINOR_ATTUNEMENT,
];
const MAJORS: [i64; 4] = [
    id::FIRE_MAJOR_ATTUNEMENT,
    id::WATER_MAJOR_ATTUNEMENT,
    id::AIR_MAJOR_ATTUNEMENT,
    id::EARTH_MAJOR_ATTUNEMENT,
];
const BASICS: [i64; 4] = [
    id::FIRE_ATTUNEMENT_BUFF,
    id::WATER_ATTUNEMENT_BUFF,
    id::AIR_ATTUNEMENT_BUFF,
    id::EARTH_ATTUNEMENT_BUFF,
];

/// `true` for the attunement buffs the Weaver translation reads.
pub(super) fn is_attunement(buff: i64) -> bool {
    BASICS.contains(&buff)
        || MAJORS.contains(&buff)
        || MINORS.contains(&buff)
        || DUALS.contains(&buff)
}

fn basic_to_dual(buff: i64) -> Option<i64> {
    BASICS.iter().position(|&b| b == buff).map(|i| DUALS[i])
}

/// The four combined attunements a basic or major attunement can lead to: index 0 fire, 1 water,
/// 2 air, 3 earth; the entries are the combinations with that element as primary.
fn primary_set(element: usize) -> [i64; 4] {
    match element {
        0 => [
            id::FIRE_WATER_ATTUNEMENT,
            id::FIRE_AIR_ATTUNEMENT,
            id::FIRE_EARTH_ATTUNEMENT,
            id::DUAL_FIRE_ATTUNEMENT,
        ],
        1 => [
            id::WATER_FIRE_ATTUNEMENT,
            id::WATER_AIR_ATTUNEMENT,
            id::WATER_EARTH_ATTUNEMENT,
            id::DUAL_WATER_ATTUNEMENT,
        ],
        2 => [
            id::AIR_FIRE_ATTUNEMENT,
            id::AIR_WATER_ATTUNEMENT,
            id::AIR_EARTH_ATTUNEMENT,
            id::DUAL_AIR_ATTUNEMENT,
        ],
        _ => [
            id::EARTH_FIRE_ATTUNEMENT,
            id::EARTH_WATER_ATTUNEMENT,
            id::EARTH_AIR_ATTUNEMENT,
            id::DUAL_EARTH_ATTUNEMENT,
        ],
    }
}

/// The combinations with the given element as secondary one.
fn secondary_set(element: usize) -> [i64; 4] {
    match element {
        0 => [
            id::WATER_FIRE_ATTUNEMENT,
            id::AIR_FIRE_ATTUNEMENT,
            id::EARTH_FIRE_ATTUNEMENT,
            id::DUAL_FIRE_ATTUNEMENT,
        ],
        1 => [
            id::FIRE_WATER_ATTUNEMENT,
            id::AIR_WATER_ATTUNEMENT,
            id::EARTH_WATER_ATTUNEMENT,
            id::DUAL_WATER_ATTUNEMENT,
        ],
        2 => [
            id::FIRE_AIR_ATTUNEMENT,
            id::WATER_AIR_ATTUNEMENT,
            id::EARTH_AIR_ATTUNEMENT,
            id::DUAL_AIR_ATTUNEMENT,
        ],
        _ => [
            id::FIRE_EARTH_ATTUNEMENT,
            id::WATER_EARTH_ATTUNEMENT,
            id::AIR_EARTH_ATTUNEMENT,
            id::DUAL_EARTH_ATTUNEMENT,
        ],
    }
}

/// `TranslateWeaverAttunement`: the combined attunement for the apply events of one moment, or 0.
fn translate(buffs: &[i64]) -> i64 {
    if buffs.len() == 1
        && let Some(dual) = basic_to_dual(buffs[0])
    {
        return dual;
    }
    let mut major: Option<[i64; 4]> = None;
    let mut minor: Option<[i64; 4]> = None;
    for &b in buffs {
        if DUALS.contains(&b) {
            return b;
        }
        if let Some(i) = BASICS.iter().position(|&x| x == b) {
            major = Some(primary_set(i));
        } else if let Some(i) = MAJORS.iter().position(|&x| x == b) {
            major = Some(primary_set(i));
        } else if let Some(i) = MINORS.iter().position(|&x| x == b) {
            minor = Some(secondary_set(i));
        }
    }
    let (Some(major), Some(minor)) = (major, minor) else {
        return 0;
    };
    let common: Vec<i64> = major
        .iter()
        .copied()
        .filter(|m| minor.contains(m))
        .collect();
    // C# throws if the intersection is not exactly one element; there is nothing to return then
    if common.len() == 1 { common[0] } else { 0 }
}

fn buff_of(p: &Pending, buff_ids: &HashMap<usize, i64>) -> i64 {
    buff_ids.get(&p.seq).copied().unwrap_or(0)
}

/// Removes the dual attunements of an elementalist that is not a Weaver.
pub(super) fn remove_duals(per_buff: &mut HashMap<i64, Vec<Pending>>) {
    for dual in DUALS {
        per_buff.remove(&dual);
    }
}

/// `TransformWeaverAttunements` on the events of one Weaver.
pub(super) fn transform(per_buff: &mut HashMap<i64, Vec<Pending>>, agent: u32) {
    let attunements: HashSet<i64> = BASICS
        .iter()
        .chain(&MAJORS)
        .chain(&MINORS)
        .chain(&DUALS)
        .copied()
        .collect();
    // all attunement events in file order, with the buff they belong to
    let mut buff_ids: HashMap<usize, i64> = HashMap::new();
    let mut events: Vec<&Pending> = Vec::new();
    for (&buff, list) in per_buff.iter() {
        if attunements.contains(&buff) {
            for p in list {
                buff_ids.insert(p.seq, buff);
                events.push(p);
            }
        }
    }
    if events.is_empty() {
        return;
    }
    events.sort_by_key(|p| p.seq);

    // `GroupByTime`: a group per moment; the first group at time 0 is never matched again
    let mut keys: Vec<i64> = Vec::new();
    let mut groups: HashMap<i64, Vec<usize>> = HashMap::new(); // key -> seqs
    for p in &events {
        let time = p.sim.time();
        let key = keys
            .iter()
            .copied()
            .find(|&k| (k - time).abs() < SERVER_DELAY)
            .unwrap_or(0);
        if key != 0 {
            groups.entry(key).or_default().push(p.seq);
        } else {
            if !keys.contains(&time) {
                keys.push(time);
            }
            groups.insert(time, vec![p.seq]);
        }
    }

    let mut invalid: HashSet<usize> = HashSet::new();
    let mut added: Vec<(i64, Pending)> = Vec::new();
    let mut previous = 0_i64;
    for key in &keys {
        let seqs = &groups[key];
        let in_group: Vec<&Pending> = seqs
            .iter()
            .filter_map(|s| events.iter().find(|p| p.seq == *s).copied())
            .collect();
        let applies: Vec<(&Pending, i64)> = in_group
            .iter()
            .filter(|p| matches!(p.sim, SimEvent::Apply { .. }))
            .map(|p| (*p, buff_of(p, &buff_ids)))
            .collect();
        let current = translate(&applies.iter().map(|a| a.1).collect::<Vec<_>>());
        invalid.extend(seqs.iter().copied());
        if current == 0 {
            continue;
        }
        let instance = applies.first().map_or(0, |a| a.0.instance);
        added.push((
            current,
            Pending {
                original_duration: i64::from(i32::MAX),
                ..pending(
                    SimEvent::Apply {
                        time: *key,
                        duration: i64::from(i32::MAX),
                        src: agent,
                        src_healing: 0,
                        stack_id: instance,
                        added_active: true,
                        overridden_duration: 0,
                        overridden_stack_id: 0,
                    },
                    instance,
                )
            },
        ));
        if previous != 0 {
            added.push((
                previous,
                pending(
                    SimEvent::RemoveAll {
                        time: *key,
                        removed_stacks: 1,
                    },
                    0,
                ),
            ));
        }
        previous = current;
    }
    for list in per_buff.values_mut() {
        list.retain(|p| !invalid.contains(&p.seq));
    }
    for (buff, p) in added {
        per_buff.entry(buff).or_default().push(p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lone_basic_attunement_becomes_the_dual_one() {
        assert_eq!(
            translate(&[id::FIRE_ATTUNEMENT_BUFF]),
            id::DUAL_FIRE_ATTUNEMENT
        );
    }

    #[test]
    fn a_major_and_a_minor_attunement_combine() {
        assert_eq!(
            translate(&[id::FIRE_MAJOR_ATTUNEMENT, id::WATER_MINOR_ATTUNEMENT]),
            id::FIRE_WATER_ATTUNEMENT
        );
        assert_eq!(
            translate(&[id::EARTH_MINOR_ATTUNEMENT, id::AIR_MAJOR_ATTUNEMENT]),
            id::AIR_EARTH_ATTUNEMENT
        );
    }

    #[test]
    fn a_dual_attunement_wins_and_an_incomplete_pair_gives_nothing() {
        assert_eq!(
            translate(&[id::FIRE_MAJOR_ATTUNEMENT, id::DUAL_AIR_ATTUNEMENT]),
            id::DUAL_AIR_ATTUNEMENT
        );
        assert_eq!(translate(&[id::FIRE_MAJOR_ATTUNEMENT]), 0);
    }
}
