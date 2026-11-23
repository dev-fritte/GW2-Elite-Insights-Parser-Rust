//! Property tests: the reader must never panic and must agree with the structure it is fed.

use evtc_ingest::{AGENT_RECORD_LEN, COMBAT_RECORD_LEN, CombatRecord, Evtc, SKILL_RECORD_LEN};
use proptest::prelude::*;

fn build_log(revision: u8, agents: &[Vec<u8>], skills: &[Vec<u8>], events: &[u8]) -> Vec<u8> {
    let mut v = b"EVTC20260920".to_vec();
    v.extend_from_slice(&[revision, 1, 0, 0]);
    v.extend_from_slice(&u32::try_from(agents.len()).unwrap().to_le_bytes());
    for a in agents {
        v.extend_from_slice(a);
    }
    v.extend_from_slice(&u32::try_from(skills.len()).unwrap().to_le_bytes());
    for s in skills {
        v.extend_from_slice(s);
    }
    v.extend_from_slice(events);
    v
}

proptest! {
    #[test]
    fn arbitrary_bytes_never_panic(data in proptest::collection::vec(any::<u8>(), 0..2048)) {
        if let Ok(evtc) = Evtc::parse(&data) {
            // touching everything must be safe as well
            for a in evtc.agents().iter() {
                let _ = (a.name(), a.address());
            }
            for s in evtc.skills().unique() {
                let _ = (s.name(), s.id());
            }
            let _ = evtc.events().count();
        }
    }

    #[test]
    fn every_prefix_of_a_valid_log_is_handled(
        agents in proptest::collection::vec(proptest::collection::vec(any::<u8>(), AGENT_RECORD_LEN), 0..4),
        skills in proptest::collection::vec(proptest::collection::vec(any::<u8>(), SKILL_RECORD_LEN), 0..4),
        events in proptest::collection::vec(any::<u8>(), 0..300),
        cut in 0usize..1000,
    ) {
        let log = build_log(1, &agents, &skills, &events);
        let cut = cut.min(log.len());
        let result = Evtc::parse(&log[..cut]);
        if cut == log.len() || cut >= log.len() - events.len() {
            // everything up to the event area is present
            let evtc = result.expect("complete tables parse");
            prop_assert_eq!(evtc.agents().len(), agents.len());
            prop_assert_eq!(evtc.skills().len(), skills.len());
        }
    }

    #[test]
    fn counts_follow_the_event_bytes(
        records in 0usize..20,
        trailing in 0usize..COMBAT_RECORD_LEN,
        revision in 0u8..3,
    ) {
        let events = vec![0x5Au8; records * COMBAT_RECORD_LEN + trailing];
        let log = build_log(revision, &[], &[], &events);
        let evtc = Evtc::parse(&log).unwrap();
        prop_assert_eq!(evtc.event_count(), records);
        prop_assert_eq!(evtc.trailing_bytes(), trailing);
        prop_assert_eq!(evtc.events().len(), records);
    }

    #[test]
    fn canonical_form_is_stable_for_revision_1(raw in proptest::collection::vec(any::<u8>(), COMBAT_RECORD_LEN)) {
        let raw: [u8; COMBAT_RECORD_LEN] = raw.try_into().unwrap();
        let record = CombatRecord::decode(1, &raw);
        let canonical = record.to_canonical();
        // decoding the canonical form again changes nothing
        prop_assert_eq!(CombatRecord::decode(1, &canonical), record);
        // only the state change byte may differ from the raw bytes
        for i in 0..COMBAT_RECORD_LEN {
            if i != 56 {
                prop_assert_eq!(canonical[i], raw[i]);
            }
        }
        prop_assert!(canonical[56] <= 93);
    }
}
