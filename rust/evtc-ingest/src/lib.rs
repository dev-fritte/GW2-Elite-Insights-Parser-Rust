//! Zero-copy reader for arcdps EVTC logs.
//!
//! The reader is checked against the C# reference through the oracle described in
//! `docs/phase1/ORACLE.md`: for every log the counts and canonical hashes of the header,
//! agent table, skill table and combat records must match.
//!
//! Ported from `GW2EIEvtcParser/EvtcParser.cs` of the GW2 Elite Insights parser (MIT License,
//! Copyright (c) 2018 baaron4), see the `LICENSE` file in the repository root.

mod agents;
mod error;
mod events;
mod evtc;
mod header;
mod load;
mod skills;

pub use agents::{AGENT_NAME_LEN, Agent, AgentTable};
pub use error::{Error, Section};
pub use events::{CombatRecord, EventIter, STATE_CHANGE_UNKNOWN};
pub use evtc::Evtc;
pub use header::Header;
pub use load::{is_compressed_name, is_supported_name, read_archive, read_log};
pub use skills::{SKILL_NAME_LEN, Skill, SkillTable};

/// Size of the fixed header in bytes: 12 byte `EVTC<date>`, 1 byte revision, 2 byte trigger id,
/// 1 byte unused (`EvtcParser.ParseLogData`).
pub const HEADER_LEN: usize = 16;

/// Size of one agent record in bytes (`EvtcParser.ParseAgentData`).
pub const AGENT_RECORD_LEN: usize = 96;

/// Size of one skill record in bytes: 4 byte id and 64 byte name (`EvtcParser.ParseSkillData`).
pub const SKILL_RECORD_LEN: usize = 68;

/// Size of one combat record in bytes (`EvtcParser.ReadCombatItem` / `ReadCombatItemRev1`).
pub const COMBAT_RECORD_LEN: usize = 64;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_sizes_match_the_evtc_format() {
        assert_eq!(HEADER_LEN, 12 + 1 + 2 + 1);
        assert_eq!(AGENT_RECORD_LEN, 8 + 4 + 4 + 2 + 2 + 2 + 2 + 2 + 2 + 68);
        assert_eq!(SKILL_RECORD_LEN, 4 + 64);
        assert_eq!(COMBAT_RECORD_LEN, 8 + 8 + 8 + 4 + 4 + 4 + 4 + 8 + 12 + 4);
    }
}
