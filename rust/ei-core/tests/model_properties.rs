//! Property tests: every index query equals the naive scan over all events.

use ei_core::generated::state_change as sc;
use ei_core::rules::{dst_is_agent, has_time, src_is_agent};
use ei_core::{AgentId, Model};
use evtc_ingest::CombatRecord;
use proptest::prelude::*;

fn record(time: i64, src: u64, dst: u64, state_change: u8) -> CombatRecord {
    CombatRecord {
        time,
        src_agent: src,
        dst_agent: dst,
        value: 0,
        buff_dmg: 0,
        overstack_value: 0,
        skill_id: 0,
        src_instid: 0,
        dst_instid: 0,
        src_master_instid: 0,
        dst_master_instid: 0,
        iff: 0,
        is_buff: 0,
        result: 0,
        is_activation: 0,
        is_buff_remove: 0,
        is_ninety: 0,
        is_fifty: 0,
        is_moving: 0,
        state_change,
        is_flanking: 0,
        is_shields: 0,
        is_offcycle: 0,
        pad: 0,
    }
}

fn arb_event() -> impl Strategy<Value = CombatRecord> {
    (
        -5i64..40,
        0u64..8,
        0u64..8,
        prop::sample::select(vec![
            sc::COMBAT,
            sc::SPAWN,
            sc::BUFF_APPLY,
            sc::LANGUAGE,
            sc::MAP_ID,
            sc::GW_BUILD,
        ]),
    )
        .prop_map(|(time, src, dst, s)| record(time, src, dst, s))
}

/// Naive reference: scan all events, keep the ones that qualify, sort by (time, position).
fn naive(events: &[CombatRecord], agent: u64, start: i64, end: i64, as_src: bool) -> Vec<u32> {
    let mut hits: Vec<(i64, u32)> = Vec::new();
    for (i, e) in events.iter().enumerate() {
        let (address, is_agent) = if as_src {
            (e.src_agent, src_is_agent(e.state_change))
        } else {
            (e.dst_agent, dst_is_agent(e.state_change))
        };
        if is_agent
            && address == agent
            && address != 0
            && has_time(e.state_change)
            && e.time >= start
            && e.time <= end
        {
            hits.push((e.time, u32::try_from(i).unwrap()));
        }
    }
    hits.sort_unstable();
    hits.into_iter().map(|(_, i)| i).collect()
}

proptest! {
    #[test]
    fn time_range_queries_equal_the_naive_scan(
        events in proptest::collection::vec(arb_event(), 0..120),
        start in -10i64..45,
        len in 0i64..60,
    ) {
        let end = start + len;
        let table: Vec<u64> = (1..4).collect();
        let model = Model::from_events(events.clone(), &table);
        for address in 1u64..8 {
            let Some(id) = model.agent_id(address) else {
                prop_assert!(naive(&events, address, start, end, true).is_empty());
                prop_assert!(naive(&events, address, start, end, false).is_empty());
                continue;
            };
            prop_assert_eq!(model.src_events_in(id, start, end), &naive(&events, address, start, end, true)[..]);
            prop_assert_eq!(model.dst_events_in(id, start, end), &naive(&events, address, start, end, false)[..]);
        }
    }

    #[test]
    fn state_change_index_lists_every_event_once_in_file_order(
        events in proptest::collection::vec(arb_event(), 0..120),
    ) {
        let model = Model::from_events(events.clone(), &[]);
        let mut total = 0;
        for s in 0..=255u8 {
            let list = model.with_state_change(s);
            total += list.len();
            prop_assert!(list.windows(2).all(|w| w[0] < w[1]));
            prop_assert!(list.iter().all(|&i| events[i as usize].state_change == s));
        }
        prop_assert_eq!(total, events.len());
    }
}

#[test]
fn table_agents_come_first_and_unknown_agents_follow() {
    let events = vec![record(1, 7, 0, sc::COMBAT), record(2, 3, 9, sc::BUFF_APPLY)];
    let model = Model::from_events(events, &[3, 5, 3]);
    assert_eq!(model.table_agent_count(), 2);
    assert_eq!(model.agent_id(3), Some(AgentId(0)));
    assert_eq!(model.agent_id(5), Some(AgentId(1)));
    assert_eq!(model.agent_id(7), Some(AgentId(2)));
    assert_eq!(model.agent_id(9), Some(AgentId(3)));
    assert_eq!(model.agent_count(), 4);
    assert_eq!(model.address(AgentId(3)), Some(9));
    assert_eq!(model.agent_id(0), None);
}
