//! Mechanics (`EIData/Mechanics`): the ones every encounter has (`LogLogic` constructor): the
//! status changes of the players, their resurrection casts and the missed quickness and alacrity
//! applications.
//!
//! The mechanics of the single encounters are not ported.

use crate::generated::state_change as sc;
use crate::keep::Kept;
use crate::players::Player;
use ei_data::mechanics::Side;
use ei_data::skill_ids as skill;
use std::collections::HashMap;

/// `ParserHelper.BuffSimulatorDelayConstant`.
const BUFF_SIMULATOR_DELAY: i64 = 15;
/// Time after the start of a Lively Lute cast in which its quickness waves count as one.
const LUTE_WINDOW: i64 = 1100;

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
    /// The events of an enemy mechanic: time in the fight and name of the enemy.
    pub enemies: Vec<(i64, String)>,
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
    let owner = owners(kept, players);
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
            .filter(|e| e.state_change == state)
            .filter_map(|e| Some((e.time - offset, owner(e)?)))
            .filter(|(time, _)| (0..=duration).contains(time))
            .collect();
        MechanicRow {
            name,
            events,
            enemies: Vec::new(),
        }
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
        enemies: Vec::new(),
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
        rows.push(MechanicRow {
            name,
            events,
            enemies: Vec::new(),
        });
    }
    rows.extend(missed_buffs(kept, players, offset, duration));
    rows.retain(|r| !r.events.is_empty());
    rows
}

/// Finds the player a status record belongs to. Records without an agent address but with an
/// instance id (the despawns of this arcdps build) go to the player with that instance id who is
/// known around that time (`orphanedSrcInstidCombatItems` of `CompleteAgentsAndLogData`).
fn owners<'a>(
    kept: &'a Kept,
    players: &'a [Player],
) -> impl Fn(&evtc_ingest::CombatRecord) -> Option<usize> + 'a {
    let redirect = move |a: u64| *kept.agent_redirection.get(&a).unwrap_or(&a);
    // per player: instance id and first and last time in the records
    let mut known: Vec<(u16, i64, i64)> = vec![(0, i64::MAX, i64::MIN); players.len()];
    for e in &kept.events {
        for (is_agent, address, instid) in [
            (
                crate::rules::src_is_agent(e.state_change),
                e.src_agent,
                e.src_instid,
            ),
            (
                crate::rules::dst_is_agent(e.state_change),
                e.dst_agent,
                e.dst_instid,
            ),
        ] {
            if is_agent
                && address != 0
                && let Some(i) = players.iter().position(|p| p.address == redirect(address))
            {
                if known[i].0 == 0 {
                    known[i].0 = instid;
                }
                known[i].1 = known[i].1.min(e.time);
                known[i].2 = known[i].2.max(e.time);
            }
        }
    }
    move |e| {
        if e.src_agent != 0 {
            return players
                .iter()
                .position(|p| p.address == redirect(e.src_agent));
        }
        if e.src_instid == 0 {
            return None;
        }
        let t = e.time;
        known.iter().position(|&(instid, first, last)| {
            instid == e.src_instid
                && ((first..=last).contains(&(t - 300)) || (first..=last).contains(&(t + 300)))
        })
    }
}

/// Dead and disconnected time segments of a player (`SingleActor.GetStatus`, without the
/// synthetic despawns of `PlayerActorStatusHelper`).
struct Status {
    dead: Vec<(i64, i64)>,
    dc: Vec<(i64, i64)>,
}

#[derive(Clone, Copy, PartialEq)]
enum StatusKind {
    Down,
    Dead,
    Despawn,
    Other,
}

impl Status {
    fn of(
        kept: &Kept,
        address: u64,
        player: usize,
        span: (i64, i64),
        owner: &dyn Fn(&evtc_ingest::CombatRecord) -> Option<usize>,
    ) -> Self {
        let redirect = |a: u64| *kept.agent_redirection.get(&a).unwrap_or(&a);
        let (first, last) = span;
        let mut status: Vec<(i64, StatusKind)> = kept
            .events
            .iter()
            .filter(|e| owner(e) == Some(player))
            .filter_map(|e| match e.state_change {
                sc::CHANGE_DOWN => Some((e.time, StatusKind::Down)),
                sc::CHANGE_DEAD => Some((e.time, StatusKind::Dead)),
                sc::DESPAWN => Some((e.time, StatusKind::Despawn)),
                sc::CHANGE_UP | sc::SPAWN => Some((e.time, StatusKind::Other)),
                _ => None,
            })
            .collect();
        status.sort_by_key(|s| s.0);
        // a despawn followed by movement, damage, a cast or a buff is no disconnect: the player
        // is back at that moment (`PlayerActorStatusHelper`)
        let activity: Vec<i64> = kept
            .events
            .iter()
            .filter(|e| match e.state_change {
                sc::POSITION | sc::VELOCITY | sc::ROTATION | sc::ANIMATION_START => {
                    e.src_agent != 0 && redirect(e.src_agent) == address
                }
                sc::BUFF_APPLY => e.dst_agent != 0 && redirect(e.dst_agent) == address,
                sc::COMBAT => {
                    e.src_agent != 0
                        && redirect(e.src_agent) == address
                        && if e.is_buff == 0 {
                            crate::damage::is_direct_hit(e.result)
                        } else {
                            crate::damage::is_buff_hit(e.result)
                        }
                }
                _ => false,
            })
            .map(|e| e.time)
            .collect();
        let mut returns = Vec::new();
        for (i, &(time, kind)) in status.iter().enumerate() {
            if kind != StatusKind::Despawn {
                continue;
            }
            let next = status.get(i + 1).map_or(last, |n| n.0);
            if let Some(back) = activity
                .iter()
                .copied()
                .filter(|&t| t > time + 50 && t < next)
                .min()
            {
                returns.push((back, StatusKind::Other));
            }
        }
        status.extend(returns);
        status.sort_by_key(|s| s.0);
        let mut dead = Vec::new();
        let mut dc = Vec::new();
        let add = |list: &mut Vec<(i64, i64)>, start: i64, end: i64| {
            if start < end {
                list.push((start, end));
            }
        };
        add(&mut dc, i64::MIN, first);
        if status.is_empty() {
            add(&mut dc, last, i64::MAX);
            return Self { dead, dc };
        }
        for (i, &(time, kind)) in status.iter().enumerate() {
            let next = status.get(i + 1).map_or(last, |n| n.0);
            match kind {
                StatusKind::Dead => add(&mut dead, time, next),
                StatusKind::Despawn => add(&mut dc, time, next),
                StatusKind::Down => {}
                StatusKind::Other => {
                    if i == 0 && time - first > 50 {
                        add(&mut dc, first, time);
                    }
                }
            }
        }
        if let Some(&(_, kind)) = status.last() {
            if kind == StatusKind::Dead {
                add(&mut dead, last, i64::MAX);
            } else {
                add(&mut dc, last, i64::MAX);
            }
        }
        Self { dead, dc }
    }

    fn dead_or_dc(&self, time: i64) -> bool {
        let inside = |list: &[(i64, i64)]| list.iter().any(|&(s, e)| s <= time && time <= e);
        inside(&self.dead) || inside(&self.dc)
    }
}

/// Starts of the Lively Lute casts per caster (index into the players). A cast that began before
/// the log and only has its end record starts that much earlier than the end.
fn lute_casts(kept: &Kept, index_of: &dyn Fn(u64) -> Option<usize>) -> HashMap<usize, Vec<i64>> {
    let mut lute: HashMap<usize, Vec<i64>> = HashMap::new();
    let mut open: std::collections::HashSet<u64> = std::collections::HashSet::new();
    for (i, e) in kept.events.iter().enumerate() {
        if i64::from(e.skill_id) != skill::LIVELY_LUTE || e.src_agent == 0 {
            continue;
        }
        let Some(caster) = index_of(e.src_agent) else {
            continue;
        };
        match e.state_change {
            sc::ANIMATION_START => {
                let instant = kept.events[i + 1..]
                    .iter()
                    .find(|n| {
                        n.src_agent == e.src_agent
                            && n.skill_id == e.skill_id
                            && matches!(n.state_change, sc::ANIMATION_START | sc::ANIMATION_STOP)
                    })
                    .is_some_and(|n| n.state_change == sc::ANIMATION_STOP && n.time == e.time);
                open.insert(e.src_agent);
                if !instant {
                    lute.entry(caster).or_default().push(e.time);
                }
            }
            sc::ANIMATION_STOP if !open.remove(&e.src_agent) => {
                let start = e.time - i64::from(e.value);
                if start < 0 && e.value > 1 {
                    lute.entry(caster).or_default().push(start);
                }
            }
            _ => {}
        }
    }
    lute
}

/// Applications of one giver to the group, packed by `BuffSimulatorDelayConstant`: time of the
/// first one and the receivers.
type Sequence = (i64, Vec<usize>);

/// Merges the sequences that start inside a Lively Lute cast window into the first of them.
fn merge_lute_waves(groups: &mut Vec<Sequence>, casts: &[i64]) {
    for &cast in casts {
        let keys: Vec<i64> = groups
            .iter()
            .map(|g| g.0)
            .filter(|&t| t >= cast && t <= cast + LUTE_WINDOW)
            .collect();
        if keys.len() > 1 {
            let mut merged = Vec::new();
            for key in &keys[1..] {
                if let Some(pos) = groups.iter().position(|g| g.0 == *key) {
                    merged.extend(groups.remove(pos).1);
                }
            }
            if let Some(first) = groups.iter_mut().find(|g| g.0 == keys[0]) {
                first.1.extend(merged);
            }
        }
    }
}

/// The players that took part in the fight (`ParsedEvtcLog`: hit or were hit by something not
/// friendly, or gave a buff to someone else); the others are left out of the player list.
fn active_players(kept: &Kept, players: &[Player], spans: &HashMap<u64, (i64, i64)>) -> Vec<bool> {
    let redirect = |a: u64| *kept.agent_redirection.get(&a).unwrap_or(&a);
    let present: Vec<bool> = players
        .iter()
        .map(|p| spans.get(&p.address).is_none_or(|s| s.1 > 0))
        .collect();
    let mut active = vec![false; players.len()];
    let index = |address: u64| players.iter().position(|p| p.address == redirect(address));
    for e in &kept.events {
        if e.src_agent == 0 && e.dst_agent == 0 {
            continue;
        }
        let participates = match e.state_change {
            sc::COMBAT => {
                e.iff != 0
                    && if e.is_buff == 0 {
                        crate::damage::is_direct_hit(e.result)
                    } else {
                        crate::damage::is_buff_hit(e.result)
                    }
            }
            _ => false,
        };
        if participates {
            for address in [e.src_agent, e.dst_agent] {
                if let Some(i) = index(address) {
                    active[i] = true;
                }
            }
        } else if e.state_change == sc::BUFF_APPLY
            && let (Some(giver), dst) = (index(e.src_agent), index(e.dst_agent))
            && dst != Some(giver)
        {
            active[giver] = true;
        }
    }
    for (a, p) in active.iter_mut().zip(present) {
        *a &= p;
    }
    active
}

/// `Miss.Qck` and `Miss.Alac` (`PlayerMissedBuffApplyEvent`): a subgroup member that did not get
/// a quickness or alacrity application its group mates got from the same giver at the same time.
fn missed_buffs(kept: &Kept, players: &[Player], offset: i64, duration: i64) -> Vec<MechanicRow> {
    let redirect = |a: u64| *kept.agent_redirection.get(&a).unwrap_or(&a);
    let spans = crate::encounter::spans(kept);
    let span_of = |i: usize| spans.get(&players[i].address).copied().unwrap_or((0, 0));
    let owner = owners(kept, players);
    let active = active_players(kept, players, &spans);
    let index_of = |address: u64| {
        players
            .iter()
            .position(|p| p.address == redirect(address))
            .filter(|&i| active[i])
    };
    let statuses: Vec<Status> = (0..players.len())
        .map(|i| Status::of(kept, players[i].address, i, span_of(i), &owner))
        .collect();
    let overlap = |a: usize, b: usize| {
        let (a, b) = (span_of(a), span_of(b));
        a.0 <= b.1 && a.1 >= b.0
    };
    let lute = lute_casts(kept, &index_of);
    let mut rows = Vec::new();
    for (name, buff) in [
        ("Miss.Qck", skill::QUICKNESS),
        ("Miss.Alac", skill::ALACRITY),
    ] {
        // every application: (time, giver, receiver)
        let mut applies: Vec<(i64, usize, usize)> = kept
            .events
            .iter()
            .filter(|e| {
                e.state_change == sc::BUFF_APPLY
                    && i64::from(e.skill_id) == buff
                    && e.src_agent != 0
                    && e.dst_agent != 0
            })
            .filter_map(|e| Some((e.time, index_of(e.src_agent)?, index_of(e.dst_agent)?)))
            .collect();
        applies.sort_by_key(|a| a.0);
        // C# computes the list for a whole group of players at once and the first player of the
        // group to ask decides; later ones overwrite it for everybody in their window
        let mut cache: Vec<Option<Vec<i64>>> = vec![None; players.len()];
        let mut events = Vec::new();
        for p in (0..players.len()).filter(|&p| active[p]) {
            if cache[p].is_none() {
                let same_group: Vec<usize> = (0..players.len())
                    .filter(|&q| active[q] && players[q].group == players[p].group && overlap(p, q))
                    .collect();
                let mut givers: Vec<usize> = Vec::new();
                for &(_, giver, _) in &applies {
                    if players[giver].group == players[p].group && !givers.contains(&giver) {
                        givers.push(giver);
                    }
                }
                let mut sequences: Vec<Vec<Sequence>> = Vec::new();
                for giver in givers {
                    let mut groups: Vec<Sequence> = Vec::new();
                    let mut block_end = i64::MIN;
                    for a in applies
                        .iter()
                        .filter(|a| a.1 == giver && same_group.contains(&a.2))
                    {
                        if a.0 >= block_end {
                            groups.push((a.0, Vec::new()));
                            block_end = a.0 + BUFF_SIMULATOR_DELAY;
                        }
                        if let Some(g) = groups.last_mut() {
                            g.1.push(a.2);
                        }
                    }
                    if groups.iter().all(|g| g.1.len() <= 1) {
                        continue;
                    }
                    if buff == skill::QUICKNESS
                        && let Some(casts) = lute.get(&giver)
                    {
                        merge_lute_waves(&mut groups, casts);
                    }
                    sequences.push(groups);
                }
                for &q in &same_group {
                    let missed = sequences
                        .iter()
                        .flatten()
                        .filter(|(time, dsts)| !dsts.contains(&q) && !statuses[q].dead_or_dc(*time))
                        .map(|(time, _)| *time)
                        .collect();
                    cache[q] = Some(missed);
                }
            }
            for &time in cache[p].as_deref().unwrap_or(&[]) {
                events.push((time - offset, p));
            }
        }
        events.retain(|(time, _)| (0..=duration).contains(time));
        rows.push(MechanicRow {
            name,
            events,
            enemies: Vec::new(),
        });
    }
    rows
}

/// `DamageResult` values of direct hits: normal, critical, glancing.
/// `DamageResult.KillingBlow` and `DamageResult.Downed`.
const KILLING_BLOW: u8 = 8;
const DOWNED: u8 = 9;
/// `ParserHelper.ServerDelayConstant`.
const SERVER_DELAY: i64 = 10;
/// Buff id of stability.
const STABILITY: i64 = 1122;

const DIRECT_HIT_RESULTS: std::ops::RangeInclusive<u8> = 0..=2;
/// `DamageResult` values of buff damage that counts as hit: all kinds of buff cycles.
const BUFF_HIT_RESULTS: std::ops::RangeInclusive<u8> = 14..=18;

/// The fight the mechanics of the encounter logic are evaluated in.
#[derive(Debug, Clone, Copy)]
pub struct Fight {
    /// Time of the records that counts as 0.
    pub offset: i64,
    /// Length of the fight.
    pub duration: i64,
    /// Whether the encounter succeeded.
    pub success: bool,
    /// Whether the log is a challenge mode (see `encounter::challenge_mode`).
    pub challenge: bool,
}

/// The mechanics of the encounter logic of the log that data alone describes (hits on players,
/// buffs applied to players); the ones with a lambda, a sub mechanic or similar are not ported.
///
/// Times are relative to the start of the fight, which lasts `duration`.
#[must_use]
pub fn encounter_mechanics(
    kept: &Kept,
    players: &[Player],
    table: evtc_ingest::AgentTable<'_>,
    targets: &[crate::encounter::Target],
    fight: Fight,
    buffs: &dyn Fn(u64, i64) -> Vec<(i64, i64)>,
) -> Vec<MechanicRow> {
    use ei_data::mechanics::Kind;
    let Fight {
        offset, duration, ..
    } = fight;
    let trigger_id = i32::from(kept.trigger_id);
    let defs = mechanics_of_log(kept);
    if defs.is_empty() {
        return Vec::new();
    }
    let redirect = |a: u64| *kept.agent_redirection.get(&a).unwrap_or(&a);
    let spans = crate::encounter::spans(kept);
    let active = active_players(kept, players, &spans);
    let index_of = |address: u64| {
        players
            .iter()
            .position(|p| p.address == redirect(address))
            .filter(|&i| active[i])
    };
    let buff_times = BuffLookup::new(buffs);
    let has_buff = |address: u64, buff: i64, time: i64| buff_times.has(address, buff, time, time);
    let achievements = achievement_events(kept, players, &spans, &active, fight, trigger_id, &defs);
    let enemy_names = enemy_names(kept, table, targets, offset);
    let species = species_of_agents(table, &enemy_names);
    let (casts, effects) = casts_and_effects(kept, players, &defs, &enemy_names);
    let lookup = Lookup {
        names: &enemy_names,
        effects: &effects,
        players,
        table,
        casts: &casts,
        species: &species,
        has_buff: &has_buff,
    };
    defs.iter()
        .map(|m| match m.kind {
            Kind::PlayerDstHit
            | Kind::PlayerDstBuffApply
            | Kind::PlayerDstDamage
            | Kind::PlayerDstBuffRemove => MechanicRow {
                name: m.name,
                events: player_events(
                    kept,
                    m,
                    offset,
                    duration,
                    &species,
                    &index_of,
                    &mut |i, buff, from, to| buff_times.has(players[i].address, buff, from, to),
                ),
                enemies: Vec::new(),
            },
            Kind::PlayerDstEffect | Kind::PlayerSrcEffect => MechanicRow {
                name: m.name,
                events: player_effect_events(m, &effects, fight, &index_of),
                enemies: Vec::new(),
            },
            Kind::PlayerSrcBuffRemoveSingleFrom | Kind::PlayerSrcBuffRemoveFrom => MechanicRow {
                name: m.name,
                events: player_removal_events(kept, m, fight, &index_of),
                enemies: Vec::new(),
            },
            Kind::PlayerSrcHit
            | Kind::PlayerSrcAllHit
            | Kind::PlayerCastStart
            | Kind::PlayerSrcBuffApply => MechanicRow {
                name: m.name,
                events: player_source_events(kept, m, fight, &casts, &species, &index_of),
                enemies: Vec::new(),
            },
            Kind::Achievement => MechanicRow {
                name: m.name,
                events: achievement_row(m, &achievements),
                enemies: Vec::new(),
            },
            Kind::EnemyDstBuffApply
            | Kind::EnemyDstBuffRemove
            | Kind::EnemyDstBuffRemoveSingle
            | Kind::EnemyCastStart
            | Kind::EnemyCastEnd
            | Kind::EnemySrcEffect
            | Kind::Spawn => MechanicRow {
                name: m.name,
                events: Vec::new(),
                enemies: enemy_events(kept, table, m, fight, &lookup),
            },
        })
        .collect()
}

/// The mechanics of the logic of the log that are available in its builds.
fn mechanics_of_log(kept: &Kept) -> Vec<&'static ei_data::mechanics::Mechanic> {
    let trigger_id = i32::from(kept.trigger_id);
    ei_data::mechanics::MECHANICS
        .iter()
        .filter(|m| m.trigger_id == trigger_id)
        .filter(|m| {
            m.kind != ei_data::mechanics::Kind::Achievement
                || achievement_rule(trigger_id, m.ids[0]).is_some()
        })
        .filter(|m| {
            let (min, max) = m.builds;
            let (min_evtc, max_evtc) = m.evtc_builds;
            kept.gw2_build < max
                && kept.gw2_build >= min
                && kept.version.build < max_evtc
                && kept.version.build >= min_evtc
        })
        .collect()
}

/// The events of the effect mechanics on players: effects located at a player or created by one.
fn player_effect_events(
    m: &ei_data::mechanics::Mechanic,
    effects: &[Effect],
    fight: Fight,
    index_of: &dyn Fn(u64) -> Option<usize>,
) -> Vec<(i64, usize)> {
    effects
        .iter()
        .filter(|f| m.guids.contains(&f.guid))
        .filter_map(|f| {
            let agent = if m.kind == ei_data::mechanics::Kind::PlayerDstEffect {
                f.dst?
            } else {
                f.src
            };
            Some((f.time - fight.offset, index_of(agent)?))
        })
        .filter(|(time, _)| (0..=fight.duration).contains(time))
        .collect()
}

/// How many stacks a record removes: the stacks of a removal of all, one for a removal of a
/// single stack, none for anything else.
fn removed_stacks(e: &evtc_ingest::CombatRecord) -> usize {
    match e.state_change {
        sc::BUFF_REMOVE_ALL => usize::from(e.result),
        sc::BUFF_REMOVE_SINGLE if e.is_buff_remove == 2 => 1,
        _ => 0,
    }
}

/// The events of the mechanics on what a player removed from agents: the player is the source of
/// the removal (the target of its record).
fn player_removal_events(
    kept: &Kept,
    m: &ei_data::mechanics::Mechanic,
    fight: Fight,
    index_of: &dyn Fn(u64) -> Option<usize>,
) -> Vec<(i64, usize)> {
    let single = m.kind == ei_data::mechanics::Kind::PlayerSrcBuffRemoveSingleFrom;
    kept.events
        .iter()
        .filter(|e| m.ids.contains(&i64::from(e.skill_id)) && e.dst_agent != 0)
        .filter(|e| {
            single
                || (e.state_change == sc::BUFF_REMOVE_ALL
                    && (m.filter != ei_data::mechanics::Filter::ToPlayer
                        || index_of(e.src_agent).is_some()))
        })
        .filter_map(|e| Some((e, index_of(e.dst_agent)?)))
        .flat_map(|(e, i)| {
            let count = if single { removed_stacks(e) } else { 1 };
            std::iter::repeat_n((e.time - fight.offset, i), count)
        })
        .filter(|(time, _)| (0..=fight.duration).contains(time))
        .collect()
}

/// The events of an achievement mechanic: the players that lost it or kept it.
fn achievement_row(
    m: &ei_data::mechanics::Mechanic,
    achievements: &Achievements,
) -> Vec<(i64, usize)> {
    let lost = m.filter == ei_data::mechanics::Filter::Lost;
    achievements
        .get(&(m.trigger_id, m.ids[0]))
        .map(|all| {
            all.iter()
                .filter(|e| e.2 == lost)
                .map(|e| (e.0, e.1))
                .collect()
        })
        .unwrap_or_default()
}

/// An effect record: when it starts, who created it, the agent it is located at (if it follows
/// one) and which effect it is.
struct Effect {
    time: i64,
    src: u64,
    dst: Option<u64>,
    guid: [u8; 16],
}

/// The effects of the log (`EffectEvent`s): the records that start an effect, with the GUID the
/// log assigns to the effect id. Effects on a moving platform are dropped like in C#.
fn effect_records(kept: &Kept) -> Vec<Effect> {
    let guids: HashMap<u32, [u8; 16]> = kept
        .events
        .iter()
        .filter(|e| e.state_change == sc::ID_TO_GUID)
        .map(|e| (e.skill_id, crate::encounter::record_guid(e)))
        .collect();
    let redirect = |a: u64| *kept.agent_redirection.get(&a).unwrap_or(&a);
    kept.events
        .iter()
        .filter_map(|e| {
            let around = (e.dst_agent != 0).then(|| redirect(e.dst_agent));
            let (platform, dst) = match e.state_change {
                sc::EFFECT_45 => (false, around),
                sc::EFFECT_51 => (e.is_flanking > 0, around),
                sc::EFFECT_GROUND_CREATE => (e.is_flanking > 0, None),
                sc::EFFECT_AGENT_CREATE => (false, Some(redirect(e.dst_agent))),
                _ => return None,
            };
            if platform || e.skill_id == 0 {
                return None;
            }
            Some(Effect {
                time: e.time,
                src: redirect(e.src_agent),
                dst,
                guid: *guids.get(&e.skill_id)?,
            })
        })
        .collect()
}

/// Name `FindActor` gives the environment, which effects without an agent belong to.
const ENVIRONMENT: &str = "Environment";

/// The times (start, end) an agent has a buff, by agent address and buff id.
type BuffTimes = HashMap<(u64, i64), Vec<(i64, i64)>>;

/// The times an agent has a buff, asked for from the simulation once.
struct BuffLookup<'a> {
    buffs: &'a dyn Fn(u64, i64) -> Vec<(i64, i64)>,
    cache: std::cell::RefCell<BuffTimes>,
}

impl<'a> BuffLookup<'a> {
    fn new(buffs: &'a dyn Fn(u64, i64) -> Vec<(i64, i64)>) -> Self {
        Self {
            buffs,
            cache: std::cell::RefCell::default(),
        }
    }

    /// Whether the agent has the buff at the time, or at any time between `from` and `to` if
    /// they differ (`HasBuff` with a window).
    fn has(&self, address: u64, buff: i64, from: i64, to: i64) -> bool {
        self.cache
            .borrow_mut()
            .entry((address, buff))
            .or_insert_with(|| (self.buffs)(address, buff))
            .iter()
            .any(|&(start, end)| {
                if from == to {
                    start <= from && from < end
                } else {
                    start <= to && end > from
                }
            })
    }
}

/// What the mechanics on enemies look up.
struct Lookup<'a> {
    /// The players of the squad.
    players: &'a [Player],
    /// The effect records.
    effects: &'a [Effect],
    /// The agents, to tell the ones without a name.
    table: evtc_ingest::AgentTable<'a>,
    /// Name of each enemy agent.
    names: &'a HashMap<u64, String>,
    /// The animated casts.
    casts: &'a [EnemyCast],
    /// Species of each agent.
    species: &'a HashMap<u64, i64>,
    /// Whether an agent has a buff at a time (agent, buff id, time).
    has_buff: &'a dyn Fn(u64, i64, i64) -> bool,
}

/// The species of every agent; the embodiments that appear with Cerus are of their own species
/// in C# (the negative ids the stable species of the permanent embodiments have).
fn species_of_agents(
    table: evtc_ingest::AgentTable<'_>,
    enemy_names: &HashMap<u64, String>,
) -> HashMap<u64, i64> {
    let mut species = crate::encounter::species_by_address(table);
    for (address, name) in enemy_names {
        if name.ends_with("(Permanent)") {
            let permanent = species.get(address).and_then(|s| match *s {
                25_677 => Some(-53),
                25_686 => Some(-54),
                26_034 => Some(-55),
                26_049 => Some(-56),
                25_967 => Some(-57),
                25_700 => Some(-58),
                _ => None,
            });
            species.insert(*address, permanent.unwrap_or(0));
        }
    }
    species
}

/// The animated casts and the effects, as far as the mechanics need them.
fn casts_and_effects(
    kept: &Kept,
    players: &[Player],
    defs: &[&ei_data::mechanics::Mechanic],
    enemy_names: &HashMap<u64, String>,
) -> (Vec<EnemyCast>, Vec<Effect>) {
    use ei_data::mechanics::Kind;
    let uses = |kinds: &[Kind]| defs.iter().any(|m| kinds.contains(&m.kind));
    let casts = if uses(&[
        Kind::EnemyCastStart,
        Kind::EnemyCastEnd,
        Kind::PlayerCastStart,
    ]) {
        all_casts(kept, players, enemy_names)
    } else {
        Vec::new()
    };
    let effects = if uses(&[
        Kind::PlayerDstEffect,
        Kind::PlayerSrcEffect,
        Kind::EnemySrcEffect,
    ]) {
        effect_records(kept)
    } else {
        Vec::new()
    };
    (casts, effects)
}

/// The animated casts of the enemies and the players.
fn all_casts(kept: &Kept, players: &[Player], enemies: &HashMap<u64, String>) -> Vec<EnemyCast> {
    let mut casters = enemies.clone();
    let squad: std::collections::HashSet<u64> = players.iter().map(|p| p.address).collect();
    casters.extend(players.iter().map(|p| (p.address, p.character.clone())));
    enemy_casts(kept, &casters, &squad)
}

/// Whether the agent a species check looks at is of one of the species (`IsSpecies`); true for
/// the mechanics without such a check. `agent` gives the address of the target or the source.
fn species_ok(
    filter: ei_data::mechanics::Filter,
    species: &HashMap<u64, i64>,
    agent: impl Fn(Side) -> u64,
) -> bool {
    let ei_data::mechanics::Filter::Species(side, ids) = filter else {
        return true;
    };
    species.get(&agent(side)).is_some_and(|s| ids.contains(s))
}

/// The events of the mechanics that look at what players do: hits and the start of casts.
fn player_source_events(
    kept: &Kept,
    m: &ei_data::mechanics::Mechanic,
    fight: Fight,
    casts: &[EnemyCast],
    species: &HashMap<u64, i64>,
    index_of: &dyn Fn(u64) -> Option<usize>,
) -> Vec<(i64, usize)> {
    let redirect = |a: u64| *kept.agent_redirection.get(&a).unwrap_or(&a);
    let mut events: Vec<(i64, usize)> = if m.kind == ei_data::mechanics::Kind::PlayerSrcBuffApply {
        kept.events
            .iter()
            .filter(|e| {
                e.src_agent != 0
                    && e.dst_agent != 0
                    && m.ids.contains(&i64::from(e.skill_id))
                    && matches!(e.state_change, sc::BUFF_APPLY | sc::BUFF_INITIAL)
                    && species_ok(m.filter, species, |side| {
                        redirect(if side == Side::To {
                            e.dst_agent
                        } else {
                            e.src_agent
                        })
                    })
            })
            .filter_map(|e| Some((e.time - fight.offset, index_of(e.src_agent)?)))
            .collect()
    } else if m.kind == ei_data::mechanics::Kind::PlayerSrcAllHit {
        // the direct hits of the player themself, whatever the skill
        kept.events
            .iter()
            .filter(|e| {
                e.state_change == sc::COMBAT
                    && e.src_agent != 0
                    && e.is_buff == 0
                    && DIRECT_HIT_RESULTS.contains(&e.result)
                    && species_ok(m.filter, species, |_| redirect(e.dst_agent))
            })
            .filter_map(|e| Some((e.time - fight.offset, index_of(e.src_agent)?)))
            .collect()
    } else if m.kind == ei_data::mechanics::Kind::PlayerCastStart {
        casts
            .iter()
            .filter(|c| m.ids.contains(&c.skill) && cast_filter(m.filter, c))
            .filter_map(|c| Some((c.start - fight.offset, index_of(c.address)?)))
            .collect()
    } else {
        kept.events
            .iter()
            .filter(|e| {
                e.state_change == sc::COMBAT
                    && e.src_agent != 0
                    && m.ids.contains(&i64::from(e.skill_id))
                    && if e.is_buff == 0 {
                        DIRECT_HIT_RESULTS.contains(&e.result)
                    } else {
                        BUFF_HIT_RESULTS.contains(&e.result)
                    }
                    && species_ok(m.filter, species, |side| {
                        redirect(if side == Side::To {
                            e.dst_agent
                        } else {
                            e.src_agent
                        })
                    })
            })
            .filter_map(|e| Some((e.time - fight.offset, index_of(e.src_agent)?)))
            .collect()
    };
    events.retain(|(time, _)| (0..=fight.duration).contains(time));
    events
}

/// The events of a mechanic on enemies: time in the fight and name of the enemy.
fn enemy_events(
    kept: &Kept,
    table: evtc_ingest::AgentTable<'_>,
    m: &ei_data::mechanics::Mechanic,
    fight: Fight,
    lookup: &Lookup<'_>,
) -> Vec<(i64, String)> {
    use ei_data::mechanics::Kind;
    let names = lookup.names;
    let mut events: Vec<(i64, String)> = match m.kind {
        Kind::Spawn => m
            .ids
            .iter()
            .flat_map(|&id| crate::encounter::species_first_seen(kept, table, id))
            .filter_map(|(address, first)| {
                Some((first - fight.offset, names.get(&address)?.clone()))
            })
            .collect(),
        Kind::EnemyDstBuffRemoveSingle => enemy_stack_removals(kept, m, fight, lookup),
        Kind::EnemySrcEffect => enemy_effect_events(m, fight, lookup),
        Kind::EnemyDstBuffApply | Kind::EnemyDstBuffRemove => {
            enemy_buff_events(kept, m, fight, lookup)
        }
        _ => lookup
            .casts
            .iter()
            .filter(|c| {
                names.contains_key(&c.address)
                    && m.ids.contains(&c.skill)
                    && cast_filter(m.filter, c)
            })
            .map(|c| {
                let time = if m.kind == Kind::EnemyCastStart {
                    c.start
                } else {
                    c.start + c.duration
                };
                (time - fight.offset, c.caster.clone())
            })
            .collect(),
    };
    events.retain(|(time, _)| (0..=fight.duration).contains(time));
    events
}

/// The removals of single stacks of a buff from enemies, one event per stack.
fn enemy_stack_removals(
    kept: &Kept,
    m: &ei_data::mechanics::Mechanic,
    fight: Fight,
    lookup: &Lookup<'_>,
) -> Vec<(i64, String)> {
    let redirect = |a: u64| *kept.agent_redirection.get(&a).unwrap_or(&a);
    kept.events
        .iter()
        .filter(|e| m.ids.contains(&i64::from(e.skill_id)))
        .filter(|e| {
            m.filter != ei_data::mechanics::Filter::ByPlayer
                || lookup
                    .players
                    .iter()
                    .any(|p| p.address == redirect(e.dst_agent))
        })
        .filter_map(|e| Some((e, lookup.names.get(&redirect(e.src_agent))?)))
        .flat_map(|(e, name)| {
            std::iter::repeat_n((e.time - fight.offset, name.clone()), removed_stacks(e))
        })
        .collect()
}

/// The effects enemies or the environment created.
fn enemy_effect_events(
    m: &ei_data::mechanics::Mechanic,
    fight: Fight,
    lookup: &Lookup<'_>,
) -> Vec<(i64, String)> {
    lookup
        .effects
        .iter()
        .filter(|f| m.guids.contains(&f.guid))
        .filter_map(|f| {
            let name = lookup.names.get(&f.src).cloned().or_else(|| {
                // agents without a name stand for the environment
                let unnamed = lookup
                    .table
                    .iter()
                    .find(|a| a.address() == f.src)
                    .is_none_or(|a| a.name().starts_with('\0') || a.name().is_empty());
                unnamed.then(|| ENVIRONMENT.to_owned())
            })?;
            Some((f.time - fight.offset, name))
        })
        .collect()
}

/// The buffs applied to enemies or removed from them (all stacks at once).
fn enemy_buff_events(
    kept: &Kept,
    m: &ei_data::mechanics::Mechanic,
    fight: Fight,
    lookup: &Lookup<'_>,
) -> Vec<(i64, String)> {
    use ei_data::mechanics::{Filter, Kind};
    let redirect = |a: u64| *kept.agent_redirection.get(&a).unwrap_or(&a);
    kept.events
        .iter()
        .filter_map(|e| {
            let removal = e.state_change == sc::BUFF_REMOVE_ALL;
            let wanted = m.ids.contains(&i64::from(e.skill_id))
                && if m.kind == Kind::EnemyDstBuffApply {
                    matches!(e.state_change, sc::BUFF_APPLY | sc::BUFF_INITIAL)
                } else {
                    removal
                };
            let agent = if removal { e.src_agent } else { e.dst_agent };
            if !wanted || agent == 0 {
                return None;
            }
            let other = if removal { e.dst_agent } else { e.src_agent };
            if !species_ok(m.filter, lookup.species, |side| {
                redirect(if side == Side::To { agent } else { other })
            }) {
                return None;
            }
            if let Filter::SpeciesBuff {
                species: wanted,
                buff,
                present,
                delay,
            } = m.filter
            {
                let target = redirect(agent);
                let is_species = lookup.species.get(&target).is_some_and(|s| *s == wanted);
                let time = e.time - fight.offset - delay;
                if !is_species || (lookup.has_buff)(target, buff, time) != present {
                    return None;
                }
            }
            Some((
                e.time - fight.offset,
                lookup.names.get(&redirect(agent))?.clone(),
            ))
        })
        .collect()
}

/// Achievement events (time, player index, lost) per (trigger id, achievement id).
type Achievements = HashMap<(i32, i64), Vec<(i64, usize, bool)>>;

/// How an achievement is lost.
enum Loss {
    /// A hit by one of the skills.
    Hit(&'static [i64]),
    /// A damage event of one of the skills, whatever its result.
    AnyHit(&'static [i64]),
    /// An application of the buff.
    Buff(i64),
    /// Temple of Febe: despair hits, death or disconnect.
    Febe,
    /// Decima: fulgent fence, exposed under fluxlance or thunder, death or disconnect.
    Decima,
}

/// The loss of an achievement that follows the usual template (`InsertAchievementEligibityEventAndRemovePhase`
/// and `AddSuccessBasedAchievementEligibityEvents`): the first loss in the encounter ends the
/// eligibility, otherwise it is kept if the encounter succeeded.
struct Rule {
    /// Name of the C# logic class.
    logic: &'static str,
    /// Achievement id.
    id: i64,
    /// Only challenge mode encounters count.
    challenge: bool,
    /// What loses it.
    loss: Loss,
    /// Inverted bookkeeping of the C# code: a loss only counts in a successful encounter and is
    /// reported as not lost, the remaining ones are reported as lost.
    inverted: bool,
}

const RULES: &[Rule] = &[
    Rule {
        logic: "TempleOfFebe",
        id: 9,
        challenge: true,
        inverted: false,
        loss: Loss::Febe,
    },
    Rule {
        logic: "DecimaTheStormsinger",
        id: 15,
        challenge: true,
        inverted: false,
        loss: Loss::Decima,
    },
    Rule {
        logic: "SoullessHorror",
        id: 11,
        challenge: false,
        inverted: false,
        loss: Loss::Hit(&[skill::INNER_VORTEX_SLASH, skill::OUTER_VORTEX_SLASH]),
    },
    Rule {
        logic: "OldLionsCourt",
        id: 5,
        challenge: false,
        inverted: false,
        loss: Loss::Hit(&[
            skill::BOILING_AETHER_RED_BLUE_NM,
            skill::BOILING_AETHER_RED_BLUE_CM,
            skill::BOILING_AETHER_GREEN_NM,
            skill::BOILING_AETHER_GREEN_CM,
        ]),
    },
    Rule {
        logic: "CosmicObservatory",
        id: 8,
        challenge: true,
        inverted: false,
        loss: Loss::Hit(&[
            skill::SPINNING_NEBULA_CENTRAL,
            skill::SPINNING_NEBULA_WITH_TELEPORT,
        ]),
    },
    Rule {
        logic: "SpiritRace",
        id: 10,
        challenge: false,
        inverted: false,
        loss: Loss::Buff(skill::CRIPPLED),
    },
    Rule {
        logic: "WhisperingShadow",
        id: 1,
        challenge: false,
        inverted: false,
        loss: Loss::Hit(&[skill::LOFTED_CRYOFLASH, skill::TERRESTRIAL_CRYOFLASH]),
    },
    Rule {
        logic: "KainengOverlook",
        id: 3,
        challenge: true,
        inverted: false,
        loss: Loss::Hit(&[
            skill::DRAGON_SLASH_WAVE_NM,
            skill::DRAGON_SLASH_WAVE_CM,
            skill::DRAGON_SLASH_RUSH_NM1,
            skill::DRAGON_SLASH_RUSH_NM2,
            skill::DRAGON_SLASH_RUSH1_CM,
            skill::DRAGON_SLASH_RUSH2_CM,
        ]),
    },
    Rule {
        logic: "GreerTheBlightbringer",
        id: 14,
        challenge: true,
        inverted: false,
        loss: Loss::Buff(skill::PLAGUE_ROT),
    },
    Rule {
        logic: "KainengOverlook",
        id: 4,
        challenge: true,
        inverted: true,
        loss: Loss::AnyHit(&[
            skill::ENHANCED_DESTRUCTIVE_AURA_SKILL1,
            skill::ENHANCED_DESTRUCTIVE_AURA_SKILL2,
        ]),
    },
    Rule {
        logic: "HarvestTemple",
        id: 2,
        challenge: false,
        inverted: false,
        loss: Loss::Hit(&[
            skill::MORDREMOTH_SHOCKWAVE,
            skill::TSUNAMI_SLAM_CLAW_ORB,
            skill::TSUNAMI_SLAM_TAIL_ORB,
        ]),
    },
    Rule {
        logic: "XunlaiJadeJunkyard",
        id: 6,
        challenge: false,
        inverted: false,
        loss: Loss::Hit(&[
            skill::WALL_OF_FEAR,
            skill::WAVE_OF_TORMENT_NM,
            skill::WAVE_OF_TORMENT_CM,
            skill::TERRIFYING_APPARITION,
        ]),
    },
    Rule {
        logic: "XunlaiJadeJunkyard",
        id: 7,
        challenge: true,
        inverted: false,
        loss: Loss::Buff(skill::DEVOURING_VOID),
    },
];

fn achievement_rule(trigger_id: i32, id: i64) -> Option<&'static Rule> {
    let logic = ei_data::targets::logic(trigger_id)?;
    RULES.iter().find(|r| r.logic == logic.name && r.id == id)
}

/// The achievement events of the players (time, player index, lost) per (trigger id, achievement
/// id). The fight is the one encounter of the log: a player takes part if the fight and the
/// time the player is known intersect.
fn achievement_events(
    kept: &Kept,
    players: &[Player],
    spans: &HashMap<u64, (i64, i64)>,
    active: &[bool],
    fight: Fight,
    trigger_id: i32,
    defs: &[&ei_data::mechanics::Mechanic],
) -> Achievements {
    let mut out = HashMap::new();
    let redirect = |a: u64| *kept.agent_redirection.get(&a).unwrap_or(&a);
    let special = SpecialData::new(kept, players);
    for id in defs
        .iter()
        .filter(|m| m.kind == ei_data::mechanics::Kind::Achievement)
        .map(|m| m.ids[0])
    {
        let Some(rule) = achievement_rule(trigger_id, id) else {
            continue;
        };
        let mut events = Vec::new();
        if !rule.challenge || fight.challenge {
            for (i, player) in players.iter().enumerate().filter(|(i, _)| active[*i]) {
                let Some(&(first, last)) = spans.get(&player.address) else {
                    continue;
                };
                let (first, last) = (first - fight.offset, last - fight.offset);
                if last.min(fight.duration) - first.max(0) <= 0 {
                    continue;
                }
                let loss = match rule.loss {
                    Loss::Febe | Loss::Decima => {
                        let status = special.status(kept, players, spans, i);
                        let ctx = Special {
                            kept,
                            player,
                            fight,
                            index: i,
                            owner: &*special.owner,
                            status: &status,
                        };
                        if matches!(rule.loss, Loss::Febe) {
                            ctx.febe()
                        } else {
                            ctx.decima()
                        }
                    }
                    _ => kept
                        .events
                        .iter()
                        .find(|e| {
                            let time = e.time - fight.offset;
                            e.dst_agent != 0
                                && redirect(e.dst_agent) == player.address
                                && (0..=fight.duration).contains(&time)
                                && (first..=last).contains(&time)
                                && standard_loss(rule, e)
                        })
                        .map(|e| e.time - fight.offset),
                };
                // the C# code of the inverted rule only takes a loss in a successful encounter
                let loss = loss.filter(|_| !rule.inverted || fight.success);
                events.push(match (loss, rule.inverted) {
                    (Some(time), false) => (time, i, true),
                    (Some(time), true) => (time, i, false),
                    (None, false) => (fight.duration, i, !fight.success),
                    (None, true) => (fight.duration, i, true),
                });
            }
        }
        out.insert((trigger_id, id), events);
    }
    out
}

/// Whether a record is the loss the rule names (`Hit`, `AnyHit`, `Buff`).
fn standard_loss(rule: &Rule, e: &evtc_ingest::CombatRecord) -> bool {
    match rule.loss {
        Loss::Hit(ids) => {
            e.state_change == sc::COMBAT
                && ids.contains(&i64::from(e.skill_id))
                && if e.is_buff == 0 {
                    DIRECT_HIT_RESULTS.contains(&e.result)
                } else {
                    BUFF_HIT_RESULTS.contains(&e.result)
                }
        }
        Loss::AnyHit(ids) => e.state_change == sc::COMBAT && ids.contains(&i64::from(e.skill_id)),
        Loss::Buff(buff) => {
            matches!(e.state_change, sc::BUFF_APPLY | sc::BUFF_INITIAL)
                && i64::from(e.skill_id) == buff
        }
        Loss::Febe | Loss::Decima => false,
    }
}

/// The player a status record belongs to (see `owners`).
type Owner<'a> = Box<dyn Fn(&evtc_ingest::CombatRecord) -> Option<usize> + 'a>;

/// What the achievements with death and disconnect conditions need besides the records.
struct SpecialData<'a> {
    owner: Owner<'a>,
}

impl<'a> SpecialData<'a> {
    fn new(kept: &'a Kept, players: &'a [Player]) -> Self {
        Self {
            owner: Box::new(owners(kept, players)),
        }
    }

    fn status(
        &self,
        kept: &Kept,
        players: &[Player],
        spans: &HashMap<u64, (i64, i64)>,
        i: usize,
    ) -> Status {
        let span = spans.get(&players[i].address).copied().unwrap_or((0, 0));
        Status::of(kept, players[i].address, i, span, &self.owner)
    }
}

/// One player in one fight for the achievements with death and disconnect conditions.
struct Special<'a> {
    kept: &'a Kept,
    player: &'a Player,
    index: usize,
    fight: Fight,
    owner: &'a dyn Fn(&evtc_ingest::CombatRecord) -> Option<usize>,
    status: &'a Status,
}

impl Special<'_> {
    fn inside(&self, time: i64) -> bool {
        (0..=self.fight.duration).contains(&time)
    }

    /// Times (offset applied) of the status records of the player.
    fn own(&self, state_change: u8) -> Vec<i64> {
        self.kept
            .events
            .iter()
            .filter(|e| e.state_change == state_change && (self.owner)(e) == Some(self.index))
            .map(|e| e.time - self.fight.offset)
            .collect()
    }

    /// Times of the records on the player of the skills: hits only, or any damage event.
    fn damage(&self, ids: &[i64], hits_only: bool) -> Vec<i64> {
        let redirect = |a: u64| *self.kept.agent_redirection.get(&a).unwrap_or(&a);
        self.kept
            .events
            .iter()
            .filter(|e| {
                e.state_change == sc::COMBAT
                    && e.dst_agent != 0
                    && redirect(e.dst_agent) == self.player.address
                    && ids.contains(&i64::from(e.skill_id))
                    && (!hits_only
                        || if e.is_buff == 0 {
                            DIRECT_HIT_RESULTS.contains(&e.result)
                        } else {
                            BUFF_HIT_RESULTS.contains(&e.result)
                        })
            })
            .map(|e| e.time - self.fight.offset)
            .collect()
    }

    /// Time of the first record in the fight, if any.
    fn first(&self, mut times: Vec<i64>) -> Option<i64> {
        times.sort_unstable();
        times.into_iter().find(|&t| self.inside(t))
    }

    /// Dead or disconnected at some point of the fight (`IsDead`, `IsDC` over the fight).
    fn away(&self) -> bool {
        let shift = |t: i64| t.saturating_sub(self.fight.offset);
        let meets = |list: &[(i64, i64)]| {
            list.iter()
                .any(|&(s, e)| !(self.fight.duration < shift(s) || shift(e) < 0))
        };
        meets(&self.status.dead) || meets(&self.status.dc)
    }

    /// Temple of Febe, Unbounded Optimism.
    fn febe(&self) -> Option<i64> {
        let mut lost = self.damage(
            &[
                skill::WAIL_OF_DESPAIR_CM,
                skill::WAIL_OF_DESPAIR_EMPOWERED_CM,
                skill::POOL_OF_DESPAIR_CM,
                skill::POOL_OF_DESPAIR_EMPOWERED_CM,
            ],
            true,
        );
        lost.extend(self.own(sc::CHANGE_DEAD));
        lost.extend(self.own(sc::DESPAWN));
        self.first(lost).or_else(|| self.away().then_some(0))
    }

    /// Decima, This Bug Can Dance.
    fn decima(&self) -> Option<i64> {
        if let Some(t) = self.first(self.damage(&[skill::FULGENT_FENCE_CM], true)) {
            return Some(t);
        }
        let hurt = self.damage(
            &[
                skill::FLUXLANCE_FUSILLADE_CM,
                skill::FLUXLANCE_SALVO_CM1,
                skill::FLUXLANCE_SALVO_CM2,
                skill::FLUXLANCE_SALVO_CM3,
                skill::FLUXLANCE_SALVO_CM4,
                skill::FLUXLANCE_SALVO_CM5,
                skill::CHORUS_OF_THUNDER_CM,
                skill::DISCORDANT_THUNDER_CM,
                skill::HARMONIOUS_THUNDER,
            ],
            false,
        );
        let redirect = |a: u64| *self.kept.agent_redirection.get(&a).unwrap_or(&a);
        let exposed: Vec<i64> = self
            .kept
            .events
            .iter()
            .filter(|e| {
                matches!(e.state_change, sc::BUFF_APPLY | sc::BUFF_INITIAL)
                    && i64::from(e.skill_id) == skill::EXPOSED_PLAYER
                    && e.dst_agent != 0
                    && redirect(e.dst_agent) == self.player.address
            })
            .map(|e| e.time - self.fight.offset)
            .filter(|&t| hurt.iter().any(|&h| (h - t).abs() < SERVER_DELAY))
            .collect();
        if let Some(t) = self.first(exposed) {
            return Some(t);
        }
        self.first(self.own(sc::CHANGE_DEAD))
            .or_else(|| self.first(self.own(sc::DESPAWN)))
            .or_else(|| self.away().then_some(0))
    }
}

fn player_events(
    kept: &Kept,
    m: &ei_data::mechanics::Mechanic,
    offset: i64,
    duration: i64,
    species: &HashMap<u64, i64>,
    index_of: &dyn Fn(u64) -> Option<usize>,
    has_buff: &mut dyn FnMut(usize, i64, i64, i64) -> bool,
) -> Vec<(i64, usize)> {
    use ei_data::mechanics::{Filter, Kind};
    let redirect = |a: u64| *kept.agent_redirection.get(&a).unwrap_or(&a);
    // the agent that lost the buffs is the source of a removal, the target of the other records
    let affected = |e: &evtc_ingest::CombatRecord| {
        if e.state_change == sc::BUFF_REMOVE_ALL {
            e.src_agent
        } else {
            e.dst_agent
        }
    };
    kept.events
        .iter()
        .filter(|e| affected(e) != 0 && m.ids.contains(&i64::from(e.skill_id)))
        .filter(|e| match m.kind {
            Kind::PlayerDstHit => {
                e.state_change == sc::COMBAT
                    && if e.is_buff == 0 {
                        DIRECT_HIT_RESULTS.contains(&e.result)
                    } else {
                        BUFF_HIT_RESULTS.contains(&e.result)
                    }
            }
            Kind::PlayerDstDamage => e.state_change == sc::COMBAT,
            Kind::PlayerDstBuffRemove => e.state_change == sc::BUFF_REMOVE_ALL,
            _ => matches!(e.state_change, sc::BUFF_APPLY | sc::BUFF_INITIAL),
        })
        .filter(|e| match m.filter {
            Filter::Damage => {
                if e.is_buff == 0 {
                    e.value > 0
                } else {
                    e.buff_dmg > 0
                }
            }
            Filter::AppliedDuration(n) => i64::from(e.value) == n,
            Filter::Killed => e.result == KILLING_BLOW,
            Filter::Downed => e.result == DOWNED,
            Filter::DownedOrKilled => e.result == KILLING_BLOW || e.result == DOWNED,
            _ => true,
        })
        .filter(|e| {
            // a removal reports the remover in the target of the record
            let other = if e.state_change == sc::BUFF_REMOVE_ALL {
                e.dst_agent
            } else {
                e.src_agent
            };
            species_ok(m.filter, species, |side| {
                redirect(if side == Side::To { affected(e) } else { other })
            })
        })
        .filter_map(|e| Some((e, index_of(affected(e))?)))
        .filter(|(e, i)| {
            let time = e.time - offset;
            let mut stable = || has_buff(*i, STABILITY, time - SERVER_DELAY, time - SERVER_DELAY);
            if m.filter == Filter::NoStability && stable() {
                return false;
            }
            if m.stability.is_some_and(|want| stable() != want) {
                return false;
            }
            if let Filter::TargetBuff {
                buff,
                present,
                delay,
                window,
            } = m.filter
            {
                let at = time - delay;
                return has_buff(*i, buff, at - window, at + window) == present;
            }
            true
        })
        .map(|(e, i)| (e.time - offset, i))
        .filter(|(time, _)| (0..=duration).contains(time))
        .collect()
}

/// The enemies of the encounter that mechanics report: address of each agent and its name.
/// Agents of a target are named like `NumericallyRenameSpecies` does (a number after the name
/// if the species has several targets), those of the trash mobs by their own name.
fn enemy_names(
    kept: &Kept,
    table: evtc_ingest::AgentTable<'_>,
    targets: &[crate::encounter::Target],
    offset: i64,
) -> HashMap<u64, String> {
    let name_of = |address: u64| -> String {
        table
            .iter()
            .find(|a| a.address() == address)
            .map(|a| a.name().split('\0').next().unwrap_or("").to_owned())
            .unwrap_or_default()
    };
    let rank = rank_prefixes(kept);
    let mut names = HashMap::new();
    // the embodiments that appear together with Cerus are of a species of their own (the
    // negative ids of the permanent embodiments)
    let permanent = |t: &crate::encounter::Target| (-58..=-53).contains(&t.species);
    let species_of = |t: &crate::encounter::Target| t.species;
    let mut counts: HashMap<i32, usize> = HashMap::new();
    for t in targets {
        *counts.entry(species_of(t)).or_default() += 1;
    }
    let mut seen: HashMap<i32, usize> = HashMap::new();
    for t in targets {
        let base = name_of(t.address);
        let name = if counts[&species_of(t)] > 1 {
            let n = seen.entry(species_of(t)).or_default();
            *n += 1;
            format!("{base} {n}")
        } else {
            base
        };
        let name = if permanent(t) {
            format!("{name} (Permanent)")
        } else {
            renamed(kept, table, t, offset, &name).unwrap_or(name)
        };
        let name = format!("{}{name}", rank.get(&t.address).copied().unwrap_or(""));
        for &a in &t.addresses {
            names.insert(a, name.clone());
        }
    }
    for address in crate::encounter::trash_addresses(kept, table) {
        names.entry(address).or_insert_with(|| {
            format!(
                "{}{}",
                rank.get(&address).copied().unwrap_or(""),
                name_of(address)
            )
        });
    }
    names
}

/// `ArcDPSBuilds.AgentInfoAdded`: arcdps writes the rank flags of NPCs from this build on.
const AGENT_INFO_ADDED: i32 = 20_260_929;

/// The names the encounter logics give targets (`OverrideName`) that follow from the log: the
/// empowered embodiments of Cerus, the heroes of Deimos's Sauls, Greer and Ura by their mode.
/// Not ported: the dark Ai, the Sunqua Peak instance and the targets of the other logics.
fn renamed(
    kept: &Kept,
    table: evtc_ingest::AgentTable<'_>,
    target: &crate::encounter::Target,
    offset: i64,
    name: &str,
) -> Option<String> {
    use ei_data::species_ids::target_id as id;
    let empowered = |buff: i64| {
        kept.events.iter().any(|e| {
            matches!(e.state_change, sc::BUFF_APPLY | sc::BUFF_INITIAL)
                && i64::from(e.skill_id) == buff
                && target.addresses.contains(
                    kept.agent_redirection
                        .get(&e.dst_agent)
                        .unwrap_or(&e.dst_agent),
                )
                && (e.time - offset - target.first).abs() < SERVER_DELAY
        })
    };
    let embodiment = match target.species {
        id::EMBODIMENT_OF_DESPAIR => skill::EMPOWERED_DESPAIR_EMBODIMENT,
        id::EMBODIMENT_OF_ENVY => skill::EMPOWERED_ENVY_EMBODIMENT,
        id::EMBODIMENT_OF_GLUTTONY => skill::EMPOWERED_GLUTTONY_EMBODIMENT,
        id::EMBODIMENT_OF_MALICE => skill::EMPOWERED_MALICE_EMBODIMENT,
        id::EMBODIMENT_OF_RAGE => skill::EMPOWERED_RAGE_EMBODIMENT,
        id::EMBODIMENT_OF_REGRET => skill::EMPOWERED_REGRET_EMBODIMENT,
        _ => 0,
    };
    match target.species {
        _ if embodiment != 0 => empowered(embodiment).then(|| format!("Empowered {name}")),
        // before arcdps knew the ranks Ura's logic names its adds itself
        id::ELITE_FUMAROLLER if kept.version.build < AGENT_INFO_ADDED => {
            Some(format!("Elite {name}"))
        }
        id::CHAMPION_FUMAROLLER if kept.version.build < AGENT_INFO_ADDED => {
            Some(format!("Champion {name}"))
        }
        id::LEGENDARY_VENTSHOT if kept.version.build < AGENT_INFO_ADDED => {
            Some(format!("Legendary {name}"))
        }
        // the elemental Ai of the Ai logic (the dark Ai is no agent; the Sunqua Peak instance names its own)
        id::AI_KEEPER_OF_THE_PEAK if i32::from(kept.trigger_id) == id::AI_KEEPER_OF_THE_PEAK => {
            Some("Elemental Ai, Keeper of the Peak".to_owned())
        }
        id::THIEF => Some("Thief".to_owned()),
        id::DRUNKARD => Some("Drunkard".to_owned()),
        id::GAMBLER => Some("Gambler".to_owned()),
        id::GREER if table.iter().any(|a| a.profession() == id::EREG as u32) => {
            Some("Godspoil Greer".to_owned())
        }
        id::URA => Some(
            if crate::encounter::target_health(kept, table, target.address)
                .is_some_and(|hp| hp > 70_000_000)
            {
                "Godscream Ura"
            } else {
                "Ura, the Steamshrieker"
            }
            .to_owned(),
        ),
        _ => None,
    }
}

/// The prefix `GetNPCRankPrefix` puts before the name of an NPC, by address: from the first agent
/// info record of the agent (arcdps from 2026-09-29 on).
fn rank_prefixes(kept: &Kept) -> HashMap<u64, &'static str> {
    const CHAMPION: u32 = 1 << 1;
    const ELITE: u32 = 1 << 5;
    const LEGENDARY: u32 = 1 << 11;
    const NO_DECORATE_NAME: u32 = 1 << 19;
    const VETERAN: u32 = 1 << 29;
    let mut out = HashMap::new();
    if kept.version.build < AGENT_INFO_ADDED {
        return out;
    }
    let mut seen = std::collections::HashSet::new();
    for e in kept
        .events
        .iter()
        .filter(|e| e.state_change == sc::AGENT_INFO)
    {
        let address = *kept
            .agent_redirection
            .get(&e.src_agent)
            .unwrap_or(&e.src_agent);
        if !seen.insert(address) {
            continue;
        }
        let flags = e.buff_dmg.cast_unsigned();
        if flags == 0 || flags & NO_DECORATE_NAME != 0 {
            continue;
        }
        let prefix = if flags & VETERAN != 0 {
            "Veteran "
        } else if flags & ELITE != 0 {
            "Elite "
        } else if flags & CHAMPION != 0 {
            "Champion "
        } else if flags & LEGENDARY != 0 {
            "Legendary "
        } else {
            continue;
        };
        out.insert(address, prefix);
    }
    out
}

/// An animated cast by an enemy (`AnimatedCastEvent`).
struct EnemyCast {
    address: u64,
    caster: String,
    skill: i64,
    start: i64,
    duration: i64,
    interrupted: bool,
}

/// The animated casts of the enemies (`CombatEventFactory.CreateCastEvents`): a start pairs with
/// the next end of the same skill; a start that never ends lasts as long as expected, but at most
/// until the next cast of the caster starts; an end without a start belongs to a cast that began
/// before the log.
fn enemy_casts(
    kept: &Kept,
    enemies: &HashMap<u64, String>,
    squad: &std::collections::HashSet<u64>,
) -> Vec<EnemyCast> {
    const CANCEL: u8 = 4;
    const ANIM_WITH_SCALED_DURATIONS: i32 = 20_191_107;
    type Open<'a> = HashMap<(u64, u32), &'a evtc_ingest::CombatRecord>;
    let redirect = |a: u64| *kept.agent_redirection.get(&a).unwrap_or(&a);
    // (caster address, skill, start, duration, unknown end, interrupted)
    let mut casts: Vec<(u64, u32, i64, i64, bool, bool)> = Vec::new();
    let expected = |start: &evtc_ingest::CombatRecord| {
        i64::from(if kept.version.build >= ANIM_WITH_SCALED_DURATIONS {
            start.buff_dmg
        } else {
            start.value
        })
    };
    let mut open: Open<'_> = HashMap::new();
    for e in &kept.events {
        if e.src_agent == 0 || !matches!(e.state_change, sc::ANIMATION_START | sc::ANIMATION_STOP) {
            continue;
        }
        let caster = redirect(e.src_agent);
        if !enemies.contains_key(&caster) {
            continue;
        }
        let key = (caster, e.skill_id);
        if e.state_change == sc::ANIMATION_START {
            if let Some(previous) = open.insert(key, e) {
                casts.push((
                    caster,
                    e.skill_id,
                    previous.time,
                    expected(previous),
                    true,
                    false,
                ));
            }
        } else if let Some(start) = open.remove(&key) {
            let mut duration = i64::from(e.value);
            let real = e.time - start.time;
            if (duration - real).abs() > SERVER_DELAY {
                duration = real;
            }
            casts.push((
                caster,
                e.skill_id,
                start.time,
                duration,
                false,
                e.is_activation == CANCEL,
            ));
        } else {
            let duration = i64::from(e.value);
            if e.time - duration < 0 {
                casts.push((
                    caster,
                    e.skill_id,
                    e.time - duration,
                    duration,
                    false,
                    e.is_activation == CANCEL,
                ));
            }
        }
    }
    for ((caster, skill), start) in open {
        casts.push((caster, skill, start.time, expected(start), true, false));
    }
    casts.sort_by_key(|c| (c.0, c.2));
    // a cast of unknown end stops when the next one of the caster starts
    for i in 0..casts.len().saturating_sub(1) {
        if casts[i].0 == casts[i + 1].0 && casts[i].4 {
            let max_end = casts[i + 1].2 + SERVER_DELAY;
            if casts[i].2 + casts[i].3 > max_end {
                casts[i].3 = max_end - casts[i].2;
            }
        }
    }
    casts
        .into_iter()
        // casts of players that last at most 1 ms are dropped
        .filter(|c| !(squad.contains(&c.0) && c.3 <= 1))
        .map(
            |(caster, skill, start, duration, _, interrupted)| EnemyCast {
                address: caster,
                caster: enemies[&caster].clone(),
                skill: i64::from(skill),
                start,
                duration,
                interrupted,
            },
        )
        .collect()
}

/// Whether a cast passes the check of a mechanic.
fn cast_filter(filter: ei_data::mechanics::Filter, cast: &EnemyCast) -> bool {
    use ei_data::mechanics::Filter;
    match filter {
        Filter::DurationAtLeast(n) => cast.duration >= n,
        Filter::DurationAbove(n) => cast.duration > n,
        Filter::DurationBelow(n) => cast.duration < n,
        Filter::DurationAtMost(n) => cast.duration <= n,
        Filter::Interrupted => cast.interrupted,
        Filter::NotInterrupted => !cast.interrupted,
        _ => true,
    }
}
