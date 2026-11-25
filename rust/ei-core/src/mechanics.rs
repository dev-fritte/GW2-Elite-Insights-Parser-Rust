//! Mechanics (`EIData/Mechanics`): the ones every encounter has (`LogLogic` constructor): the
//! status changes of the players and their resurrection casts.
//!
//! The mechanics of the single encounters are not ported.

use crate::generated::state_change as sc;
use crate::keep::Kept;
use crate::players::Player;
use ei_data::skill_ids as skill;

/// `DamageResult.CrowdControl` of the result enum.
const CROWD_CONTROL_RESULT: u8 = 12;

/// The crowd control mechanics: name and the generic skills arcdps gives that kind of crowd control.
const CROWD_CONTROL: [(&str, &[i64]); 6] = [
    ("Knck.Dwn", &[skill::ARC_DPS_GENERIC_KNOCKDOWN]),
    ("Knck.Pll", &[skill::ARC_DPS_GENERIC_KNOCKBACK_PULL]),
    ("Flt", &[skill::ARC_DPS_GENERIC_FLOAT]),
    ("Lnch", &[skill::ARC_DPS_GENERIC_LAUNCH]),
    (
        "Lckt",
        &[
            skill::ARC_DPS_GENERIC_LOCK_OUT,
            skill::ARC_DPS_GENERIC_STAGGER,
            skill::ARC_DPS_GENERIC_FEAR,
        ],
    ),
    (
        "Wtr.Flt.Snk",
        &[
            skill::ARC_DPS_GENERIC_WATER_FLOAT_SINK_DEFUNC,
            skill::ARC_DPS_GENERIC_FLOAT_WATER,
            skill::ARC_DPS_GENERIC_SINK,
        ],
    ),
];

/// One mechanic with the events it found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MechanicRow {
    /// Short name of the mechanic, as in the JSON (`name`).
    pub name: &'static str,
    /// The events: time in the fight and index into the players.
    pub events: Vec<(i64, usize)>,
}

/// The mechanics of every log: dead, downed, got up, resurrect, disconnected and respawned.
///
/// Times are relative to the start of the fight, which lasts `duration`; only events inside the
/// fight count. Mechanics without events are not listed.
#[must_use]
pub fn base_mechanics(
    kept: &Kept,
    players: &[Player],
    offset: i64,
    duration: i64,
) -> Vec<MechanicRow> {
    let redirect = |a: u64| *kept.agent_redirection.get(&a).unwrap_or(&a);
    let index_of = |address: u64| players.iter().position(|p| p.address == redirect(address));
    let mut rows: Vec<MechanicRow> = [
        ("Dead", sc::CHANGE_DEAD),
        ("Downed", sc::CHANGE_DOWN),
        ("Got up", sc::CHANGE_UP),
        ("DC", sc::DESPAWN),
        ("Resp", sc::SPAWN),
    ]
    .into_iter()
    .map(|(name, state)| {
        let events = kept
            .events
            .iter()
            .filter(|e| e.state_change == state && e.src_agent != 0)
            .filter_map(|e| Some((e.time - offset, index_of(e.src_agent)?)))
            .filter(|(time, _)| (0..=duration).contains(time))
            .collect();
        MechanicRow { name, events }
    })
    .collect();
    // the start of the resurrection cast: an animation that stops at the very time it starts is
    // no cast
    let resurrect = kept
        .events
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            e.state_change == sc::ANIMATION_START
                && i64::from(e.skill_id) == skill::RESURRECT
                && e.src_agent != 0
        })
        .filter_map(|(i, e)| {
            let player = index_of(e.src_agent)?;
            let instant = kept.events[i + 1..]
                .iter()
                .find(|n| {
                    n.src_agent == e.src_agent
                        && n.skill_id == e.skill_id
                        && matches!(n.state_change, sc::ANIMATION_START | sc::ANIMATION_STOP)
                })
                .is_some_and(|n| n.state_change == sc::ANIMATION_STOP && n.time == e.time);
            (!instant).then_some((e.time - offset, player))
        })
        .filter(|(time, _)| (0..=duration).contains(time))
        .collect();
    rows.push(MechanicRow {
        name: "Res",
        events: resurrect,
    });
    // crowd control the players take
    for (name, ids) in CROWD_CONTROL {
        let events = kept
            .events
            .iter()
            .filter(|e| {
                e.state_change == sc::COMBAT
                    && e.result == CROWD_CONTROL_RESULT
                    && e.is_buff == 0
                    && e.dst_agent != 0
                    && ids.contains(&i64::from(e.skill_id))
            })
            .filter_map(|e| Some((e.time - offset, index_of(e.dst_agent)?)))
            .filter(|(time, _)| (0..=duration).contains(time))
            .collect();
        rows.push(MechanicRow { name, events });
    }
    rows.retain(|r| !r.events.is_empty());
    rows
}
