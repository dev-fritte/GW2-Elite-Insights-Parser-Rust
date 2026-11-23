//! The whole path from the bytes of a log to the JSON the core can currently write.

use std::fmt;

use evtc_ingest::Evtc;
use serde_json::{Map, Value, json};

use crate::api::MapList;
use crate::keep::{KeepError, KeepSettings, keep_events};
use crate::meta::{Metadata, extract};

/// What the core knows about a log.
#[derive(Debug, Clone)]
pub struct Analysis {
    /// Log level metadata.
    pub metadata: Metadata,
}

/// Why a log could not be analysed.
#[derive(Debug)]
pub enum AnalysisError {
    /// The file is not a readable EVTC log.
    Ingest(evtc_ingest::Error),
    /// The record filter rejected the log.
    Keep(KeepError),
}

impl fmt::Display for AnalysisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ingest(e) => write!(f, "{e}"),
            Self::Keep(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for AnalysisError {}

impl From<evtc_ingest::Error> for AnalysisError {
    fn from(e: evtc_ingest::Error) -> Self {
        Self::Ingest(e)
    }
}

impl From<KeepError> for AnalysisError {
    fn from(e: KeepError) -> Self {
        Self::Keep(e)
    }
}

/// Analyses the bytes of an (already decompressed) EVTC log.
///
/// # Errors
/// [`AnalysisError`] if the log is malformed, too short, too long or has no combat records.
pub fn analyze(data: &[u8], maps: Option<&MapList>) -> Result<Analysis, AnalysisError> {
    let evtc = Evtc::parse(data)?;
    let header = evtc.header();
    let records: Vec<_> = evtc.events().collect();
    let kept = keep_events(
        &records,
        header.trigger_id(),
        header.build(),
        KeepSettings::default(),
    )?;
    Ok(Analysis {
        metadata: extract(&kept, maps),
    })
}

/// The JSON object of an analysis. Keys that exist in the C# JSON (`JsonLog`) carry the same name
/// and meaning there, so they can be compared field by field. Keys that the C# JSON does not
/// have start with `core`.
#[must_use]
pub fn to_json(analysis: &Analysis) -> Value {
    let m = &analysis.metadata;
    let mut o = Map::new();
    o.insert("arcVersion".into(), json!(m.arc_version));
    o.insert("arcRevision".into(), json!(m.arc_revision));
    o.insert("triggerID".into(), json!(m.trigger_id));
    o.insert("isInstanceLog".into(), json!(m.is_instance_log));
    if let Some(map_id) = m.map_id {
        o.insert("mapID".into(), json!(map_id));
    }
    o.insert("gW2Build".into(), json!(m.gw2_build));
    o.insert("language".into(), json!(m.language.name()));
    o.insert("languageID".into(), json!(m.language as u8));
    o.insert("fractalScale".into(), json!(m.fractal_scale));
    if let Some(region) = m.region {
        o.insert("region".into(), json!(region.name()));
    }
    if let Some(start) = m.log_start_unix {
        o.insert("coreLogStartUnix".into(), json!(start));
    }
    if let Some(end) = m.log_end_unix {
        o.insert("coreLogEndUnix".into(), json!(end));
    }
    Value::Object(o)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_data_that_is_not_a_log() {
        assert!(matches!(
            analyze(b"hello", None),
            Err(AnalysisError::Ingest(_))
        ));
    }

    #[test]
    fn a_log_without_combat_events_is_rejected_by_the_filter() {
        let mut data = b"EVTC20260920".to_vec();
        data.extend_from_slice(&[1, 0x45, 0x3C, 0]);
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        assert!(matches!(
            analyze(&data, None),
            Err(AnalysisError::Keep(KeepError::NoEvents))
        ));
    }
}
