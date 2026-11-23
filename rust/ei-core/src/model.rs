//! The event model: all combat records of a log, dense agent ids, and indices for per agent
//! and per state change lookups.
//!
//! Every event exists exactly once, in `events`. Indices only store `u32` positions into it.
//! The per agent lists are ordered by time (ties keep file order), so a time range is found with
//! two binary searches instead of a scan.

use std::collections::HashMap;

use evtc_ingest::{CombatRecord, Evtc};

use crate::csr::{Csr, NO_KEY};
use crate::rules::{dst_is_agent, has_time, src_is_agent};

/// Dense id of an agent: its position in the agent table, followed by addresses that only occur
/// in events (the C# parser creates an `UNKNOWN` agent for each of those).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AgentId(pub u32);

/// Events and indices of one log.
#[derive(Debug, Clone)]
pub struct Model {
    events: Vec<CombatRecord>,
    addresses: Vec<u64>,
    ids: HashMap<u64, AgentId>,
    table_agents: usize,
    by_src: Csr,
    by_dst: Csr,
    by_state_change: Csr,
}

impl Model {
    /// Builds the model of a parsed log.
    #[must_use]
    pub fn build(evtc: &Evtc<'_>) -> Self {
        let table: Vec<u64> = evtc.agents().iter().map(|a| a.address()).collect();
        Self::from_events(evtc.events().collect(), &table)
    }

    /// Builds a model from decoded events and the addresses of the agent table.
    ///
    /// # Panics
    /// If there are more than `u32::MAX` events or agents.
    #[must_use]
    pub fn from_events(events: Vec<CombatRecord>, table_addresses: &[u64]) -> Self {
        let mut addresses = Vec::with_capacity(table_addresses.len());
        let mut ids = HashMap::with_capacity(table_addresses.len());
        for &address in table_addresses {
            ids.entry(address).or_insert_with(|| {
                addresses.push(address);
                AgentId(u32::try_from(addresses.len() - 1).expect("agent count fits u32"))
            });
        }
        let table_agents = addresses.len();

        let mut intern = |address: u64| -> u32 {
            let id = *ids.entry(address).or_insert_with(|| {
                addresses.push(address);
                AgentId(u32::try_from(addresses.len() - 1).expect("agent count fits u32"))
            });
            id.0
        };
        let mut src_key = vec![NO_KEY; events.len()];
        let mut dst_key = vec![NO_KEY; events.len()];
        for (i, e) in events.iter().enumerate() {
            if src_is_agent(e.state_change) && e.src_agent != 0 {
                src_key[i] = intern(e.src_agent);
            }
            if dst_is_agent(e.state_change) && e.dst_agent != 0 {
                dst_key[i] = intern(e.dst_agent);
            }
        }

        // events with an agent, in time order; the sort is stable so ties keep the file order
        let mut order: Vec<u32> = (0u32..)
            .zip(src_key.iter().zip(&dst_key))
            .filter(|&(i, (&s, &d))| {
                (s != NO_KEY || d != NO_KEY) && has_time(events[i as usize].state_change)
            })
            .map(|(i, _)| i)
            .collect();
        order.sort_by_key(|&i| events[i as usize].time);

        let sorted_src: Vec<u32> = order.iter().map(|&i| src_key[i as usize]).collect();
        let sorted_dst: Vec<u32> = order.iter().map(|&i| dst_key[i as usize]).collect();
        let mut by_src = Csr::build(&sorted_src, addresses.len());
        let mut by_dst = Csr::build(&sorted_dst, addresses.len());
        by_src.map_items(&order);
        by_dst.map_items(&order);

        let state_keys: Vec<u32> = events.iter().map(|e| u32::from(e.state_change)).collect();
        let by_state_change = Csr::build(&state_keys, 256);

        Self {
            events,
            addresses,
            ids,
            table_agents,
            by_src,
            by_dst,
            by_state_change,
        }
    }

    /// All events in file order.
    #[must_use]
    pub fn events(&self) -> &[CombatRecord] {
        &self.events
    }

    /// Number of agents, table agents first.
    #[must_use]
    pub fn agent_count(&self) -> usize {
        self.addresses.len()
    }

    /// Number of agents that come from the agent table (the rest only occur in events).
    #[must_use]
    pub fn table_agent_count(&self) -> usize {
        self.table_agents
    }

    /// The address of an agent.
    #[must_use]
    pub fn address(&self, id: AgentId) -> Option<u64> {
        self.addresses.get(id.0 as usize).copied()
    }

    /// The dense id of an address. For addresses that occur more than once in the agent table
    /// this is the first one.
    #[must_use]
    pub fn agent_id(&self, address: u64) -> Option<AgentId> {
        self.ids.get(&address).copied()
    }

    /// Positions of the events whose source is `agent`, ordered by time.
    #[must_use]
    pub fn src_events(&self, agent: AgentId) -> &[u32] {
        self.by_src.get(agent.0)
    }

    /// Positions of the events whose destination is `agent`, ordered by time.
    #[must_use]
    pub fn dst_events(&self, agent: AgentId) -> &[u32] {
        self.by_dst.get(agent.0)
    }

    /// The part of [`Model::src_events`] with `start <= time <= end`.
    #[must_use]
    pub fn src_events_in(&self, agent: AgentId, start: i64, end: i64) -> &[u32] {
        self.time_range(self.src_events(agent), start, end)
    }

    /// The part of [`Model::dst_events`] with `start <= time <= end`.
    #[must_use]
    pub fn dst_events_in(&self, agent: AgentId, start: i64, end: i64) -> &[u32] {
        self.time_range(self.dst_events(agent), start, end)
    }

    fn time_range<'a>(&self, list: &'a [u32], start: i64, end: i64) -> &'a [u32] {
        let time = |i: u32| self.events[i as usize].time;
        let from = list.partition_point(|&i| time(i) < start);
        let to = list.partition_point(|&i| time(i) <= end);
        list.get(from..to.max(from)).unwrap_or_default()
    }

    /// Positions of all events with the given state change, in file order.
    #[must_use]
    pub fn with_state_change(&self, state_change: u8) -> &[u32] {
        self.by_state_change.get(u32::from(state_change))
    }

    /// The first event with the given state change, in file order.
    #[must_use]
    pub fn first_with(&self, state_change: u8) -> Option<&CombatRecord> {
        self.with_state_change(state_change)
            .first()
            .map(|&i| &self.events[i as usize])
    }
}
