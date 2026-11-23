//! The squad players of a log.
//!
//! Port of the player related part of `EvtcParser.CompleteAgentsAndLogData` and
//! `CompletePlayers`, reduced to what a log without split or merged players needs: which
//! agents of the agent table are players, their name, account, group and profession.
//! Not ported: regrouping of duplicate agents and the per specialisation split of instance
//! logs (`AgentManipulationHelper`). The parity tool shows where that matters.

use std::collections::HashMap;

use evtc_ingest::{Agent, AgentTable, parse_dotnet_int};

use crate::keep::Kept;
use crate::rules::{dst_is_agent, src_is_agent};
use crate::spec::SpecList;

/// A squad player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Player {
    /// Address of the agent in the combat records.
    pub address: u64,
    /// Character name.
    pub character: String,
    /// Account name without the leading colon.
    pub account: String,
    /// Squad subgroup, 1 if the log has no squad.
    pub group: i32,
    /// Profession or elite specialisation name (`GetSpec`).
    pub profession: String,
}

/// What kind of agent an entry of the agent table is, by its profession fields and name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentKind {
    /// A non player character.
    Npc,
    /// A gadget.
    Gadget,
    /// A player of the squad.
    Player,
    /// A player outside the squad (name without account, or without a group).
    NonSquadPlayer,
}

fn name_parts(agent: Agent<'_>) -> Vec<String> {
    agent.name().split('\0').map(str::to_owned).collect()
}

/// Classifies an agent like `ParseAgentData` and the `AgentItem` constructor do.
#[must_use]
pub fn agent_kind(agent: Agent<'_>) -> AgentKind {
    if agent.is_elite() == 0xFFFF_FFFF {
        return if agent.profession() & 0xFFFF_0000 == 0xFFFF_0000 {
            AgentKind::Gadget
        } else {
            AgentKind::Npc
        };
    }
    let parts = name_parts(agent);
    // `AgentItem` ctor: `splitStr.Length < 2 || splitStr[1].Length == 0 || splitStr[2].Length == 0
    // || splitStr[0].Contains('-')`; an index error inside the `try` leaves the type unchanged.
    if parts.len() < 2 || parts[1].is_empty() {
        return AgentKind::NonSquadPlayer;
    }
    match parts.get(2) {
        Some(group) if group.is_empty() || parts[0].contains('-') => AgentKind::NonSquadPlayer,
        _ => AgentKind::Player,
    }
}

/// For each agent address: whether it has been seen in a kept record with a non-zero instance
/// id (`UpdateAgentData` gives an agent its `InstID` and aware times from such records).
fn seen_with_instance_id(kept: &Kept) -> HashMap<u64, bool> {
    let mut seen: HashMap<u64, bool> = HashMap::new();
    let redirect = |address: u64| *kept.agent_redirection.get(&address).unwrap_or(&address);
    for e in &kept.events {
        if src_is_agent(e.state_change) && e.src_agent != 0 {
            let flag = seen.entry(redirect(e.src_agent)).or_insert(false);
            *flag |= e.src_instid != 0;
        }
        if dst_is_agent(e.state_change) && e.dst_agent != 0 {
            let flag = seen.entry(redirect(e.dst_agent)).or_insert(false);
            *flag |= e.dst_instid != 0;
        }
    }
    seen
}

/// Finds the squad players.
///
/// `squadless` is `true` for log types without a squad (5 player instances, sPvP); the group of
/// every player is then 1, as it is when any player has group 0. Unparsable groups count as 0.
#[must_use]
pub fn find_players(
    agents: AgentTable<'_>,
    kept: &Kept,
    specs: &SpecList,
    squadless: bool,
) -> Vec<Player> {
    let seen = seen_with_instance_id(kept);
    let mut players: Vec<Player> = agents
        .iter()
        .filter(|a| agent_kind(*a) == AgentKind::Player)
        .filter(|a| seen.get(&a.address()).copied().unwrap_or(false))
        .map(|a| {
            let parts = name_parts(a);
            let group = parts
                .get(2)
                .and_then(|g| parse_dotnet_int(g.as_bytes()))
                .unwrap_or(0);
            Player {
                address: a.address(),
                character: parts[0].clone(),
                account: parts[1].trim_start_matches(':').to_owned(),
                group: if squadless { 1 } else { group },
                profession: specs.spec_name(a.profession(), a.is_elite()).to_owned(),
            }
        })
        .collect();
    if players.iter().any(|p| p.group == 0) {
        for p in &mut players {
            p.group = 1;
        }
    }
    // `RegroupSameAgentsAndDetermineTeams`: players with the same account become one agent that
    // copies the first of them
    let mut accounts = std::collections::HashSet::new();
    players.retain(|p| accounts.insert(p.account.clone()));
    players.sort_by(|a, b| a.character.cmp(&b.character));
    players
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent_record(name: &[u8], prof: u32, elite: u32) -> Vec<u8> {
        let mut v = 7u64.to_le_bytes().to_vec();
        v.extend_from_slice(&prof.to_le_bytes());
        v.extend_from_slice(&elite.to_le_bytes());
        v.extend_from_slice(&[0u8; 12]);
        let mut n = name.to_vec();
        n.resize(68, 0);
        v.extend_from_slice(&n);
        v
    }

    fn kind_of(name: &[u8], prof: u32, elite: u32) -> AgentKind {
        let mut data = 1u32.to_le_bytes().to_vec();
        data.extend(agent_record(name, prof, elite));
        let (table, _) = AgentTable::parse(&data).unwrap();
        agent_kind(table.get(0).unwrap())
    }

    #[test]
    fn classifies_agents() {
        assert_eq!(kind_of(b"Boss\0", 15429, 0xFFFF_FFFF), AgentKind::Npc);
        assert_eq!(
            kind_of(b"Gadget\0", 0xFFFF_1234, 0xFFFF_FFFF),
            AgentKind::Gadget
        );
        assert_eq!(kind_of(b"Name\0:Acc.1234\x003\0", 8, 7), AgentKind::Player);
        assert_eq!(
            kind_of(b"Name\0:Acc.1234\0\0", 8, 7),
            AgentKind::NonSquadPlayer
        );
        assert_eq!(kind_of(b"Name\0\x003\0", 8, 7), AgentKind::NonSquadPlayer);
        assert_eq!(
            kind_of(b"ch1234-5\0:Acc.1\x003\0", 8, 7),
            AgentKind::NonSquadPlayer
        );
        assert_eq!(kind_of(b"OnlyName", 8, 7), AgentKind::NonSquadPlayer);
    }
}
