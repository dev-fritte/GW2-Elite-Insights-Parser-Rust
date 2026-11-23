//! Error type of the ingest crate.

use std::fmt;

/// The part of the file a read failed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    /// The 16 byte header.
    Header,
    /// The agent table (count and records).
    Agents,
    /// The skill table (count and records).
    Skills,
}

impl fmt::Display for Section {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Header => "header",
            Self::Agents => "agent table",
            Self::Skills => "skill table",
        })
    }
}

/// Everything that can go wrong while reading an EVTC log.
#[derive(Debug)]
pub enum Error {
    /// The data does not start with `EVTC` followed by a number
    /// (`EvtcFileException("Not EVTC")` in the C# reader).
    NotEvtc,
    /// The data ends before the named section is complete.
    Truncated {
        /// Section that was being read.
        section: Section,
        /// Bytes the section needs, counted from the start of the section.
        needed: u64,
        /// Bytes left in the data at the start of the section.
        available: u64,
    },
    /// A compressed log is not a ZIP archive with exactly one entry.
    InvalidArchive(String),
    /// Reading the file failed.
    Io(std::io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotEvtc => f.write_str("not an EVTC log"),
            Self::Truncated {
                section,
                needed,
                available,
            } => {
                write!(
                    f,
                    "truncated {section}: needs {needed} bytes, {available} available"
                )
            }
            Self::InvalidArchive(why) => write!(f, "invalid archive: {why}"),
            Self::Io(e) => write!(f, "i/o error: {e}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
