//! Combat records.

use crate::COMBAT_RECORD_LEN;

/// Raw `is_statechange` values from this one up are mapped to this value, like the C# reader
/// does (`ArcDPSEnums.GetStateChange` maps everything unknown to `StateChange.Unknown`).
pub const STATE_CHANGE_UNKNOWN: u8 = 93;

/// One decoded combat record, in the layout the C# `CombatItem` exposes.
///
/// Revision 1 records (header revision byte not 0) map 1:1 to the 64 raw bytes. Revision 0
/// records are widened (`overstack_value` and `skill_id` to 32 bit) and have no
/// `dst_master_instid` and no `pad`. In both cases `state_change` is clamped to
/// [`STATE_CHANGE_UNKNOWN`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)] // the fields carry the names of the arcdps combat record fields
pub struct CombatRecord {
    pub time: i64,
    pub src_agent: u64,
    pub dst_agent: u64,
    pub value: i32,
    pub buff_dmg: i32,
    pub overstack_value: u32,
    pub skill_id: u32,
    pub src_instid: u16,
    pub dst_instid: u16,
    pub src_master_instid: u16,
    pub dst_master_instid: u16,
    pub iff: u8,
    pub is_buff: u8,
    pub result: u8,
    pub is_activation: u8,
    pub is_buff_remove: u8,
    pub is_ninety: u8,
    pub is_fifty: u8,
    pub is_moving: u8,
    pub state_change: u8,
    pub is_flanking: u8,
    pub is_shields: u8,
    pub is_offcycle: u8,
    pub pad: u32,
}

fn le_u16(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn le_u32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn le_u64(b: &[u8], at: usize) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[at..at + 8]);
    u64::from_le_bytes(a)
}

#[allow(clippy::cast_possible_wrap)] // reinterpreting the bits as signed is the point
fn le_i32(b: &[u8], at: usize) -> i32 {
    le_u32(b, at) as i32
}

impl CombatRecord {
    /// Decodes one raw record. `revision` is the header revision byte.
    #[must_use]
    #[allow(clippy::cast_possible_wrap)]
    pub fn decode(revision: u8, raw: &[u8; COMBAT_RECORD_LEN]) -> Self {
        let clamp = |b: u8| b.min(STATE_CHANGE_UNKNOWN);
        if revision > 0 {
            // time, src, dst, value, buff_dmg, overstack u32, skill u32, 4 instids, 12 flags, pad
            Self {
                time: le_u64(raw, 0) as i64,
                src_agent: le_u64(raw, 8),
                dst_agent: le_u64(raw, 16),
                value: le_i32(raw, 24),
                buff_dmg: le_i32(raw, 28),
                overstack_value: le_u32(raw, 32),
                skill_id: le_u32(raw, 36),
                src_instid: le_u16(raw, 40),
                dst_instid: le_u16(raw, 42),
                src_master_instid: le_u16(raw, 44),
                dst_master_instid: le_u16(raw, 46),
                iff: raw[48],
                is_buff: raw[49],
                result: raw[50],
                is_activation: raw[51],
                is_buff_remove: raw[52],
                is_ninety: raw[53],
                is_fifty: raw[54],
                is_moving: raw[55],
                state_change: clamp(raw[56]),
                is_flanking: raw[57],
                is_shields: raw[58],
                is_offcycle: raw[59],
                pad: le_u32(raw, 60),
            }
        } else {
            // time, src, dst, value, buff_dmg, overstack u16, skill u16, 3 instids, 9 garbage
            // bytes, 12 flags, 1 garbage byte
            Self {
                time: le_u64(raw, 0) as i64,
                src_agent: le_u64(raw, 8),
                dst_agent: le_u64(raw, 16),
                value: le_i32(raw, 24),
                buff_dmg: le_i32(raw, 28),
                overstack_value: u32::from(le_u16(raw, 32)),
                skill_id: u32::from(le_u16(raw, 34)),
                src_instid: le_u16(raw, 36),
                dst_instid: le_u16(raw, 38),
                src_master_instid: le_u16(raw, 40),
                dst_master_instid: 0,
                iff: raw[51],
                is_buff: raw[52],
                result: raw[53],
                is_activation: raw[54],
                is_buff_remove: raw[55],
                is_ninety: raw[56],
                is_fifty: raw[57],
                is_moving: raw[58],
                state_change: clamp(raw[59]),
                is_flanking: raw[60],
                is_shields: raw[61],
                is_offcycle: raw[62],
                pad: 0,
            }
        }
    }

    /// The canonical 64 byte form used by the oracle hashes (`docs/phase1/ORACLE.md`).
    #[must_use]
    #[allow(clippy::cast_sign_loss)] // bit reinterpretation
    pub fn to_canonical(&self) -> [u8; COMBAT_RECORD_LEN] {
        let mut out = [0u8; COMBAT_RECORD_LEN];
        out[0..8].copy_from_slice(&(self.time as u64).to_le_bytes());
        out[8..16].copy_from_slice(&self.src_agent.to_le_bytes());
        out[16..24].copy_from_slice(&self.dst_agent.to_le_bytes());
        out[24..28].copy_from_slice(&self.value.to_le_bytes());
        out[28..32].copy_from_slice(&self.buff_dmg.to_le_bytes());
        out[32..36].copy_from_slice(&self.overstack_value.to_le_bytes());
        out[36..40].copy_from_slice(&self.skill_id.to_le_bytes());
        out[40..42].copy_from_slice(&self.src_instid.to_le_bytes());
        out[42..44].copy_from_slice(&self.dst_instid.to_le_bytes());
        out[44..46].copy_from_slice(&self.src_master_instid.to_le_bytes());
        out[46..48].copy_from_slice(&self.dst_master_instid.to_le_bytes());
        out[48..60].copy_from_slice(&[
            self.iff,
            self.is_buff,
            self.result,
            self.is_activation,
            self.is_buff_remove,
            self.is_ninety,
            self.is_fifty,
            self.is_moving,
            self.state_change,
            self.is_flanking,
            self.is_shields,
            self.is_offcycle,
        ]);
        out[60..64].copy_from_slice(&self.pad.to_le_bytes());
        out
    }
}

/// Iterator over the decoded combat records of a log, in file order.
#[derive(Debug, Clone)]
pub struct EventIter<'a> {
    revision: u8,
    chunks: std::slice::Iter<'a, [u8; COMBAT_RECORD_LEN]>,
}

impl<'a> EventIter<'a> {
    pub(crate) fn new(revision: u8, event_bytes: &'a [u8]) -> Self {
        let (records, _) = event_bytes.as_chunks::<COMBAT_RECORD_LEN>();
        Self {
            revision,
            chunks: records.iter(),
        }
    }
}

impl Iterator for EventIter<'_> {
    type Item = CombatRecord;

    fn next(&mut self) -> Option<CombatRecord> {
        let raw = self.chunks.next()?;
        Some(CombatRecord::decode(self.revision, raw))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.chunks.size_hint()
    }
}

impl ExactSizeIterator for EventIter<'_> {}

#[cfg(test)]
mod tests {
    use super::*;

    fn rev1_record() -> [u8; 64] {
        let mut r = [0u8; 64];
        r[0..8].copy_from_slice(&(-5i64).to_le_bytes());
        r[8..16].copy_from_slice(&0x0102_0304_0506_0708u64.to_le_bytes());
        r[16..24].copy_from_slice(&9u64.to_le_bytes());
        r[24..28].copy_from_slice(&(-100i32).to_le_bytes());
        r[28..32].copy_from_slice(&7i32.to_le_bytes());
        r[32..36].copy_from_slice(&0x0001_0002u32.to_le_bytes());
        r[36..40].copy_from_slice(&0x000A_BCDEu32.to_le_bytes());
        r[40..48].copy_from_slice(&[1, 0, 2, 0, 3, 0, 4, 0]);
        for (i, b) in (0..12u8).enumerate() {
            r[48 + i] = 100 + b;
        }
        r[56] = 200; // state change beyond the known range
        r[60..64].copy_from_slice(&0xAABB_CCDDu32.to_le_bytes());
        r
    }

    #[test]
    fn decodes_revision_1_and_clamps_the_state_change() {
        let raw = rev1_record();
        let c = CombatRecord::decode(1, &raw);
        assert_eq!(c.time, -5);
        assert_eq!(c.src_agent, 0x0102_0304_0506_0708);
        assert_eq!((c.value, c.buff_dmg), (-100, 7));
        assert_eq!((c.overstack_value, c.skill_id), (0x0001_0002, 0x000A_BCDE));
        assert_eq!(
            (
                c.src_instid,
                c.dst_instid,
                c.src_master_instid,
                c.dst_master_instid
            ),
            (1, 2, 3, 4)
        );
        assert_eq!((c.iff, c.is_buff, c.is_moving), (100, 101, 107));
        assert_eq!(c.state_change, STATE_CHANGE_UNKNOWN);
        assert_eq!(
            (c.is_flanking, c.is_offcycle, c.pad),
            (109, 111, 0xAABB_CCDD)
        );
        // canonical form equals the raw bytes except for the clamped state change
        let mut expected = raw;
        expected[56] = STATE_CHANGE_UNKNOWN;
        assert_eq!(c.to_canonical(), expected);
    }

    #[test]
    fn known_state_changes_pass_through() {
        let mut raw = rev1_record();
        raw[56] = 92;
        assert_eq!(CombatRecord::decode(1, &raw).state_change, 92);
        raw[56] = 93;
        assert_eq!(CombatRecord::decode(1, &raw).state_change, 93);
    }

    #[test]
    fn decodes_revision_0_with_widened_fields() {
        let mut r = [0u8; 64];
        r[0..8].copy_from_slice(&42i64.to_le_bytes());
        r[24..28].copy_from_slice(&1i32.to_le_bytes());
        r[32..34].copy_from_slice(&0x1234u16.to_le_bytes());
        r[34..36].copy_from_slice(&0x5678u16.to_le_bytes());
        r[36..42].copy_from_slice(&[1, 0, 2, 0, 3, 0]);
        r[42..51].fill(0xEE); // garbage
        for i in 0..12u8 {
            r[51 + usize::from(i)] = 10 + i;
        }
        r[63] = 0xFF; // garbage
        let c = CombatRecord::decode(0, &r);
        assert_eq!(c.time, 42);
        assert_eq!((c.overstack_value, c.skill_id), (0x1234, 0x5678));
        assert_eq!((c.src_instid, c.dst_instid, c.src_master_instid), (1, 2, 3));
        assert_eq!((c.dst_master_instid, c.pad), (0, 0));
        assert_eq!((c.iff, c.state_change, c.is_offcycle), (10, 18, 21));
    }

    #[test]
    fn iterator_yields_complete_records_only() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&rev1_record());
        bytes.extend_from_slice(&rev1_record());
        bytes.extend_from_slice(&[0u8; 10]);
        let it = EventIter::new(1, &bytes);
        assert_eq!(it.len(), 2);
        assert_eq!(it.count(), 2);
    }
}
