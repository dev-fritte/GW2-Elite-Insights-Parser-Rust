//! The 16 byte log header.

use crate::HEADER_LEN;
use crate::error::{Error, Section};

/// Parsed header of an EVTC log.
///
/// Layout (`EvtcParser.ParseLogData`): 12 bytes `EVTC<yyyymmdd>`, 1 byte combat record revision,
/// 2 bytes trigger id (little endian), 1 byte unused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header<'a> {
    version: &'a [u8; 12],
    build: i32,
    revision: u8,
    trigger_id: u16,
}

impl<'a> Header<'a> {
    /// Reads the header from the start of `data`.
    ///
    /// Mirrors the C# check: the first four bytes are `EVTC` and the remaining eight must parse
    /// as an `int` (`int.TryParse`: optional surrounding whitespace, optional sign, digits).
    ///
    /// # Errors
    /// [`Error::Truncated`] if `data` is shorter than 16 bytes, [`Error::NotEvtc`] if the magic
    /// or the build number is invalid.
    pub fn parse(data: &'a [u8]) -> Result<Self, Error> {
        let Some(head) = data.first_chunk::<HEADER_LEN>() else {
            return Err(Error::Truncated {
                section: Section::Header,
                needed: HEADER_LEN as u64,
                available: data.len() as u64,
            });
        };
        let version = <&[u8; 12]>::try_from(&head[..12]).map_err(|_| Error::NotEvtc)?;
        if !version.starts_with(b"EVTC") {
            return Err(Error::NotEvtc);
        }
        let build = parse_dotnet_int(&version[4..]).ok_or(Error::NotEvtc)?;
        Ok(Self {
            version,
            build,
            revision: head[12],
            trigger_id: u16::from_le_bytes([head[13], head[14]]),
        })
    }

    /// The 12 raw version bytes, e.g. `EVTC20260920`.
    #[must_use]
    pub fn version_bytes(&self) -> &'a [u8; 12] {
        self.version
    }

    /// The version as text. The bytes are guaranteed to be ASCII by [`Header::parse`].
    #[must_use]
    pub fn build_string(&self) -> &'a str {
        std::str::from_utf8(self.version).unwrap_or_default()
    }

    /// The arcdps build number (the digits after `EVTC`).
    #[must_use]
    pub fn build(&self) -> i32 {
        self.build
    }

    /// Revision of the combat record layout: 0 is the old layout, 1 the current one.
    #[must_use]
    pub fn revision(&self) -> u8 {
        self.revision
    }

    /// Trigger id of the log (boss species id, or the instance marker).
    #[must_use]
    pub fn trigger_id(&self) -> u16 {
        self.trigger_id
    }
}

/// Subset of .NET `int.TryParse(string)` (and `int.Parse`, which accepts the same text) with `NumberStyles.Integer`: leading and trailing
/// white space (and trailing NUL characters), one optional sign, at least one digit.
#[must_use]
pub fn parse_dotnet_int(text: &[u8]) -> Option<i32> {
    let is_space = |b: u8| matches!(b, b' ' | b'\t' | b'\n' | 0x0B | 0x0C | b'\r');
    let mut end = text.len();
    while end > 0 && (text[end - 1] == 0 || is_space(text[end - 1])) {
        end -= 1;
    }
    let mut start = 0;
    while start < end && is_space(text[start]) {
        start += 1;
    }
    let mut digits = &text[start..end];
    let negative = match digits.first() {
        Some(b'-') => {
            digits = &digits[1..];
            true
        }
        Some(b'+') => {
            digits = &digits[1..];
            false
        }
        _ => false,
    };
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let mut value: i64 = 0;
    for &d in digits {
        value = value * 10 + i64::from(d - b'0');
        if value > i64::from(i32::MAX) + 1 {
            return None;
        }
    }
    i32::try_from(if negative { -value } else { value }).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header_bytes(version: &[u8; 12], revision: u8, id: u16) -> Vec<u8> {
        let mut v = version.to_vec();
        v.push(revision);
        v.extend_from_slice(&id.to_le_bytes());
        v.push(0);
        v
    }

    #[test]
    fn parses_a_regular_header() {
        let data = header_bytes(b"EVTC20260920", 1, 15429);
        let h = Header::parse(&data).unwrap();
        assert_eq!(h.build_string(), "EVTC20260920");
        assert_eq!(h.build(), 20_260_920);
        assert_eq!(h.revision(), 1);
        assert_eq!(h.trigger_id(), 15429);
    }

    #[test]
    fn rejects_short_data() {
        let err = Header::parse(&[0u8; 15]).unwrap_err();
        assert!(matches!(
            err,
            Error::Truncated {
                section: Section::Header,
                needed: 16,
                available: 15
            }
        ));
    }

    #[test]
    fn rejects_wrong_magic_and_non_numeric_build() {
        assert!(matches!(
            Header::parse(&header_bytes(b"ABCD20260920", 1, 1)),
            Err(Error::NotEvtc)
        ));
        assert!(matches!(
            Header::parse(&header_bytes(b"EVTC2026092x", 1, 1)),
            Err(Error::NotEvtc)
        ));
        assert!(matches!(
            Header::parse(&header_bytes(b"EVTC        ", 1, 1)),
            Err(Error::NotEvtc)
        ));
    }

    #[test]
    fn follows_dotnet_int_parsing() {
        assert_eq!(parse_dotnet_int(b"20260920"), Some(20_260_920));
        assert_eq!(parse_dotnet_int(b" 12 "), Some(12));
        assert_eq!(parse_dotnet_int(b"-7"), Some(-7));
        assert_eq!(parse_dotnet_int(b"+7\0\0"), Some(7));
        assert_eq!(parse_dotnet_int(b"2147483647"), Some(i32::MAX));
        assert_eq!(parse_dotnet_int(b"2147483648"), None);
        assert_eq!(parse_dotnet_int(b"-2147483648"), Some(i32::MIN));
        assert_eq!(parse_dotnet_int(b"1 2"), None);
        assert_eq!(parse_dotnet_int(b"-"), None);
    }
}
