//! A whole log: header, agent table, skill table and the combat record area.

use crate::COMBAT_RECORD_LEN;
use crate::agents::AgentTable;
use crate::error::Error;
use crate::events::EventIter;
use crate::header::Header;
use crate::skills::SkillTable;

/// A parsed log. All parts borrow from the input bytes, nothing is copied.
#[derive(Debug, Clone, Copy)]
pub struct Evtc<'a> {
    header: Header<'a>,
    agents: AgentTable<'a>,
    skills: SkillTable<'a>,
    events: &'a [u8],
}

impl<'a> Evtc<'a> {
    /// Reads header, agent table and skill table. The combat records after them are only
    /// located, not decoded (see [`Evtc::event_bytes`]).
    ///
    /// # Errors
    /// Any error of [`Header::parse`], [`AgentTable::parse`] or [`SkillTable::parse`].
    pub fn parse(data: &'a [u8]) -> Result<Self, Error> {
        let header = Header::parse(data)?;
        let (agents, rest) = AgentTable::parse(&data[crate::HEADER_LEN..])?;
        let (skills, events) = SkillTable::parse(rest)?;
        Ok(Self {
            header,
            agents,
            skills,
            events,
        })
    }

    /// The header.
    #[must_use]
    pub fn header(&self) -> Header<'a> {
        self.header
    }

    /// The agent table.
    #[must_use]
    pub fn agents(&self) -> AgentTable<'a> {
        self.agents
    }

    /// The skill table.
    #[must_use]
    pub fn skills(&self) -> SkillTable<'a> {
        self.skills
    }

    /// Everything after the skill table: the combat records and possibly a partial last record.
    #[must_use]
    pub fn event_bytes(&self) -> &'a [u8] {
        self.events
    }

    /// Iterates over the decoded combat records in file order (revision taken from the header).
    #[must_use]
    pub fn events(&self) -> EventIter<'a> {
        EventIter::new(self.header.revision(), self.events)
    }

    /// Number of complete 64 byte combat records.
    #[must_use]
    pub fn event_count(&self) -> usize {
        self.events.len() / COMBAT_RECORD_LEN
    }

    /// Bytes after the last complete combat record. The C# reader ignores them.
    #[must_use]
    pub fn trailing_bytes(&self) -> usize {
        self.events.len() % COMBAT_RECORD_LEN
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AGENT_RECORD_LEN, HEADER_LEN, SKILL_RECORD_LEN};

    fn log(agents: u32, skills: u32, event_bytes: usize) -> Vec<u8> {
        let mut v = b"EVTC20260920".to_vec();
        v.extend_from_slice(&[1, 0x45, 0x3C, 0]);
        v.extend_from_slice(&agents.to_le_bytes());
        v.extend(std::iter::repeat_n(7u8, agents as usize * AGENT_RECORD_LEN));
        v.extend_from_slice(&skills.to_le_bytes());
        v.extend(std::iter::repeat_n(9u8, skills as usize * SKILL_RECORD_LEN));
        v.extend(std::iter::repeat_n(3u8, event_bytes));
        v
    }

    #[test]
    fn locates_all_sections() {
        let data = log(2, 3, 64 * 5 + 10);
        let evtc = Evtc::parse(&data).unwrap();
        assert_eq!(evtc.header().trigger_id(), 0x3C45);
        assert_eq!(evtc.agents().len(), 2);
        assert_eq!(evtc.skills().len(), 3);
        assert_eq!(evtc.event_count(), 5);
        assert_eq!(evtc.trailing_bytes(), 10);
        assert_eq!(evtc.event_bytes().len(), 330);
    }

    #[test]
    fn accepts_empty_tables_and_no_events() {
        let data = log(0, 0, 0);
        assert_eq!(data.len(), HEADER_LEN + 8);
        let evtc = Evtc::parse(&data).unwrap();
        assert!(evtc.agents().is_empty() && evtc.skills().is_empty());
        assert_eq!(evtc.event_count(), 0);
    }

    #[test]
    fn reports_truncation_in_the_right_section() {
        let data = log(2, 3, 0);
        let cut_in_skills = &data[..data.len() - 1];
        assert!(matches!(
            Evtc::parse(cut_in_skills),
            Err(Error::Truncated {
                section: crate::Section::Skills,
                ..
            })
        ));
        let cut_in_agents = &data[..HEADER_LEN + 4 + 10];
        assert!(matches!(
            Evtc::parse(cut_in_agents),
            Err(Error::Truncated {
                section: crate::Section::Agents,
                ..
            })
        ));
    }
}
