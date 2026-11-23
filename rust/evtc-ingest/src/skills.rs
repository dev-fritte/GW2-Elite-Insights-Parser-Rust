//! The skill table.

use std::borrow::Cow;
use std::collections::HashSet;

use crate::SKILL_RECORD_LEN;
use crate::agents::read_count;
use crate::error::{Error, Section};

/// Size of the name field of a skill record.
pub const SKILL_NAME_LEN: usize = 64;

/// View over the skill table. Nothing is copied: records are decoded on access.
///
/// Layout (`EvtcParser.ParseSkillData`): `u32` count, then per skill 68 bytes: `i32` id and a
/// 64 byte name. The C# reader keeps a dictionary, so a skill id that appears more than once
/// keeps only its first entry; see [`SkillTable::unique`].
#[derive(Debug, Clone, Copy)]
pub struct SkillTable<'a> {
    records: &'a [[u8; SKILL_RECORD_LEN]],
}

impl<'a> SkillTable<'a> {
    /// Reads the table at the start of `data` and returns it together with the bytes after it.
    ///
    /// # Errors
    /// [`Error::Truncated`] if the count or the records do not fit into `data`.
    pub fn parse(data: &'a [u8]) -> Result<(Self, &'a [u8]), Error> {
        let (count, body) = read_count(data, Section::Skills)?;
        let needed = u64::from(count) * SKILL_RECORD_LEN as u64;
        let Some((records, rest)) = usize::try_from(needed)
            .ok()
            .and_then(|len| body.split_at_checked(len))
        else {
            return Err(Error::Truncated {
                section: Section::Skills,
                needed: 4 + needed,
                available: data.len() as u64,
            });
        };
        let (records, _) = records.as_chunks::<SKILL_RECORD_LEN>();
        Ok((Self { records }, rest))
    }

    /// Number of skill records as stored in the file, duplicates included.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// `true` if the table has no skills.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Iterates over all records in file order, duplicates included.
    #[must_use]
    pub fn iter(&self) -> impl ExactSizeIterator<Item = Skill<'a>> + use<'a> {
        self.records.iter().map(|record| Skill { record })
    }

    /// Iterates over the skills the C# reader keeps: the first record of every skill id, in file
    /// order. Allocates a set of the ids seen so far.
    pub fn unique(&self) -> impl Iterator<Item = Skill<'a>> + use<'a> {
        let mut seen = HashSet::with_capacity(self.records.len());
        self.iter().filter(move |skill| seen.insert(skill.id()))
    }
}

/// One skill record.
#[derive(Debug, Clone, Copy)]
pub struct Skill<'a> {
    record: &'a [u8; SKILL_RECORD_LEN],
}

impl<'a> Skill<'a> {
    /// The skill id.
    #[must_use]
    pub fn id(self) -> i32 {
        i32::from_le_bytes([
            self.record[0],
            self.record[1],
            self.record[2],
            self.record[3],
        ])
    }

    /// The 64 byte name field, including everything after the first NUL byte.
    #[must_use]
    pub fn name_field(self) -> &'a [u8] {
        &self.record[4..]
    }

    /// The name bytes up to (not including) the first NUL byte, like `GetString(reader, 64)`.
    #[must_use]
    pub fn name_bytes(self) -> &'a [u8] {
        let field = self.name_field();
        let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
        &field[..end]
    }

    /// The name decoded like .NET `Encoding.UTF8.GetString`: invalid sequences become U+FFFD.
    #[must_use]
    pub fn name(self) -> Cow<'a, str> {
        String::from_utf8_lossy(self.name_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: i32, name: &[u8]) -> Vec<u8> {
        let mut v = id.to_le_bytes().to_vec();
        let mut n = name.to_vec();
        n.resize(SKILL_NAME_LEN, 0);
        v.extend_from_slice(&n);
        assert_eq!(v.len(), SKILL_RECORD_LEN);
        v
    }

    fn table(records: &[Vec<u8>]) -> Vec<u8> {
        let mut data = u32::try_from(records.len()).unwrap().to_le_bytes().to_vec();
        for r in records {
            data.extend_from_slice(r);
        }
        data
    }

    #[test]
    fn reads_id_and_nul_terminated_name() {
        let data = table(&[
            record(-5, b"Might\0garbage"),
            record(740, &[b'A', 0xFF, b'B']),
        ]);
        let (skills, rest) = SkillTable::parse(&data).unwrap();
        assert!(rest.is_empty());
        let all: Vec<_> = skills.iter().collect();
        assert_eq!(all[0].id(), -5);
        assert_eq!(all[0].name(), "Might");
        assert_eq!(all[0].name_field().len(), 64);
        assert_eq!(all[1].name(), "A\u{FFFD}B");
    }

    #[test]
    fn full_width_name_without_nul() {
        let data = table(&[record(1, &[b'x'; 64])]);
        let (skills, _) = SkillTable::parse(&data).unwrap();
        assert_eq!(skills.iter().next().unwrap().name_bytes().len(), 64);
    }

    #[test]
    fn unique_keeps_the_first_record_of_an_id() {
        let data = table(&[record(1, b"first"), record(2, b"two"), record(1, b"second")]);
        let (skills, _) = SkillTable::parse(&data).unwrap();
        assert_eq!(skills.len(), 3);
        let unique: Vec<_> = skills
            .unique()
            .map(|s| (s.id(), s.name().into_owned()))
            .collect();
        assert_eq!(unique, [(1, "first".to_string()), (2, "two".to_string())]);
    }

    #[test]
    fn rejects_truncated_tables() {
        assert!(matches!(
            SkillTable::parse(&[1, 0]),
            Err(Error::Truncated {
                section: Section::Skills,
                needed: 4,
                available: 2
            })
        ));
        let mut data = table(&[record(1, b"a")]);
        data.truncate(data.len() - 1);
        assert!(SkillTable::parse(&data).is_err());
        assert!(SkillTable::parse(&u32::MAX.to_le_bytes()).is_err());
    }
}
