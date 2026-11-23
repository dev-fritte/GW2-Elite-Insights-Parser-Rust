//! The agent table.

use std::borrow::Cow;

use crate::AGENT_RECORD_LEN;
use crate::error::{Error, Section};

/// Size of the name field of an agent record.
pub const AGENT_NAME_LEN: usize = 68;

/// View over the agent table. Nothing is copied: records are decoded on access.
///
/// Layout (`EvtcParser.ParseAgentData`): `u32` count, then per agent 96 bytes:
/// address `u64`, profession `u32`, `is_elite` `u32`, toughness, concentration, healing `u16`,
/// hitbox width `u16`, condition `u16`, hitbox height `u16`, and a 68 byte name.
#[derive(Debug, Clone, Copy)]
pub struct AgentTable<'a> {
    records: &'a [[u8; AGENT_RECORD_LEN]],
}

impl<'a> AgentTable<'a> {
    /// Reads the table at the start of `data` and returns it together with the bytes after it.
    ///
    /// # Errors
    /// [`Error::Truncated`] if the count or the records do not fit into `data`.
    pub fn parse(data: &'a [u8]) -> Result<(Self, &'a [u8]), Error> {
        let (count, body) = read_count(data, Section::Agents)?;
        let needed = u64::from(count) * AGENT_RECORD_LEN as u64;
        let Some((records, rest)) = usize::try_from(needed)
            .ok()
            .and_then(|len| body.split_at_checked(len))
        else {
            return Err(Error::Truncated {
                section: Section::Agents,
                needed: 4 + needed,
                available: data.len() as u64,
            });
        };
        let (records, _) = records.as_chunks::<AGENT_RECORD_LEN>();
        Ok((Self { records }, rest))
    }

    /// Number of agents as stored in the file.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// `true` if the table has no agents.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// The agent at `index`, if there is one.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<Agent<'a>> {
        self.records.get(index).map(|record| Agent { record })
    }

    /// Iterates over all agents in file order.
    #[must_use]
    pub fn iter(&self) -> impl ExactSizeIterator<Item = Agent<'a>> + use<'a> {
        self.records.iter().map(|record| Agent { record })
    }
}

/// One agent record.
#[derive(Debug, Clone, Copy)]
pub struct Agent<'a> {
    record: &'a [u8; AGENT_RECORD_LEN],
}

impl<'a> Agent<'a> {
    fn u16_at(self, offset: usize) -> u16 {
        u16::from_le_bytes([self.record[offset], self.record[offset + 1]])
    }

    fn u32_at(self, offset: usize) -> u32 {
        u32::from_le_bytes([
            self.record[offset],
            self.record[offset + 1],
            self.record[offset + 2],
            self.record[offset + 3],
        ])
    }

    /// The agent address used by combat records to refer to this agent.
    #[must_use]
    pub fn address(&self) -> u64 {
        let mut b = [0u8; 8];
        b.copy_from_slice(&self.record[0..8]);
        u64::from_le_bytes(b)
    }

    /// Raw profession value (player profession id, or species/gadget id with high bits set).
    #[must_use]
    pub fn profession(&self) -> u32 {
        self.u32_at(8)
    }

    /// Raw `is_elite` value (elite specialisation id, or a marker for NPCs and gadgets).
    #[must_use]
    pub fn is_elite(&self) -> u32 {
        self.u32_at(12)
    }

    /// Toughness attribute.
    #[must_use]
    pub fn toughness(&self) -> u16 {
        self.u16_at(16)
    }

    /// Concentration attribute (named `healing` in a comment of the C# reader, but read second).
    #[must_use]
    pub fn concentration(&self) -> u16 {
        self.u16_at(18)
    }

    /// Healing power attribute.
    #[must_use]
    pub fn healing(&self) -> u16 {
        self.u16_at(20)
    }

    /// Hitbox width. The C# reader doubles the stored value.
    #[must_use]
    pub fn hitbox_width(&self) -> u32 {
        2 * u32::from(self.u16_at(22))
    }

    /// Condition damage attribute.
    #[must_use]
    pub fn condition(&self) -> u16 {
        self.u16_at(24)
    }

    /// Hitbox height. The C# reader doubles the stored value.
    #[must_use]
    pub fn hitbox_height(&self) -> u32 {
        2 * u32::from(self.u16_at(26))
    }

    /// The full 68 byte name field, including trailing NUL bytes.
    /// For players this is `character\0account\0subgroup\0`.
    #[must_use]
    pub fn name_bytes(&self) -> &'a [u8] {
        &self.record[28..]
    }

    /// The name field decoded like .NET `Encoding.UTF8.GetString`: invalid sequences become
    /// U+FFFD, NUL bytes are kept.
    #[must_use]
    pub fn name(&self) -> Cow<'a, str> {
        String::from_utf8_lossy(self.name_bytes())
    }
}

/// Reads the leading `u32` count of a table.
pub(crate) fn read_count(data: &[u8], section: Section) -> Result<(u32, &[u8]), Error> {
    match data.split_first_chunk::<4>() {
        Some((count, rest)) => Ok((u32::from_le_bytes(*count), rest)),
        None => Err(Error::Truncated {
            section,
            needed: 4,
            available: data.len() as u64,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(address: u64, prof: u32, elite: u32, attrs: [u16; 6], name: &[u8]) -> Vec<u8> {
        let mut v = address.to_le_bytes().to_vec();
        v.extend_from_slice(&prof.to_le_bytes());
        v.extend_from_slice(&elite.to_le_bytes());
        for a in attrs {
            v.extend_from_slice(&a.to_le_bytes());
        }
        let mut n = name.to_vec();
        n.resize(AGENT_NAME_LEN, 0);
        v.extend_from_slice(&n);
        assert_eq!(v.len(), AGENT_RECORD_LEN);
        v
    }

    #[test]
    fn reads_fields_in_file_order() {
        // attrs in file order: toughness, concentration, healing, hitbox width, condition, hitbox height
        let mut data = 1u32.to_le_bytes().to_vec();
        data.extend(record(
            0x1122_3344_5566_7788,
            5,
            27,
            [10, 20, 30, 40, 50, 60],
            b"Name\0Acc.1234\0",
        ));
        data.extend_from_slice(&[0xAA, 0xBB]);
        let (table, rest) = AgentTable::parse(&data).unwrap();
        assert_eq!(rest, &[0xAA, 0xBB]);
        assert_eq!(table.len(), 1);
        let a = table.get(0).unwrap();
        assert_eq!(a.address(), 0x1122_3344_5566_7788);
        assert_eq!((a.profession(), a.is_elite()), (5, 27));
        assert_eq!(
            (a.toughness(), a.concentration(), a.healing()),
            (10, 20, 30)
        );
        assert_eq!(
            (a.hitbox_width(), a.condition(), a.hitbox_height()),
            (80, 50, 120)
        );
        assert_eq!(a.name_bytes().len(), 68);
        assert!(a.name().starts_with("Name\0Acc.1234\0"));
        assert_eq!(a.name().len(), 68);
        assert!(table.get(1).is_none());
    }

    #[test]
    fn decodes_invalid_utf8_lossily() {
        let mut data = 1u32.to_le_bytes().to_vec();
        data.extend(record(1, 0, 0, [0; 6], &[0x41, 0xFF, 0x42]));
        let (table, _) = AgentTable::parse(&data).unwrap();
        assert!(table.get(0).unwrap().name().starts_with("A\u{FFFD}B"));
    }

    #[test]
    fn rejects_truncated_tables() {
        assert!(matches!(
            AgentTable::parse(&[1, 0, 0]),
            Err(Error::Truncated {
                section: Section::Agents,
                needed: 4,
                available: 3
            })
        ));
        let mut data = 2u32.to_le_bytes().to_vec();
        data.extend(record(1, 0, 0, [0; 6], b"x"));
        assert!(matches!(
            AgentTable::parse(&data),
            Err(Error::Truncated {
                section: Section::Agents,
                ..
            })
        ));
        // an absurd count must not overflow or allocate
        assert!(AgentTable::parse(&u32::MAX.to_le_bytes()).is_err());
    }

    #[test]
    fn iterates_all_agents() {
        let mut data = 3u32.to_le_bytes().to_vec();
        for i in 0..3u64 {
            data.extend(record(i, 0, 0, [0; 6], b"a"));
        }
        let (table, rest) = AgentTable::parse(&data).unwrap();
        assert!(rest.is_empty());
        let addrs: Vec<u64> = table.iter().map(|a| a.address()).collect();
        assert_eq!(addrs, [0, 1, 2]);
    }
}
