//! Compares the encounter offset and duration of the Rust core with the C# JSON.

use std::collections::BTreeMap;

use ei_core::pipeline::analyze_offset;
use serde_json::Value;

/// Counts per encounter (log folder name).
#[derive(Default)]
pub struct OffsetStats {
    pub logs: usize,
    pub offset_ok: usize,
    pub duration_ok: usize,
    pub targets_ok: usize,
    pub target_examples: Vec<String>,
    pub examples: Vec<String>,
}

/// Compares one log.
///
/// # Errors
/// The text of the analysis error.
pub fn compare_log(
    data: &[u8],
    reference: &Value,
    folder: &str,
    label: &str,
    stats: &mut BTreeMap<String, OffsetStats>,
) -> Result<(), String> {
    let ours = analyze_offset(data).map_err(|e| e.to_string())?;
    let expected_offset = reference
        .get("logStartOffset")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let expected_duration = reference.get("durationMS").and_then(Value::as_i64);
    let s = stats.entry(folder.to_string()).or_default();
    s.logs += 1;
    let offset = ours.offset;
    let offset_ok = offset == Some(expected_offset);
    if offset_ok {
        s.offset_ok += 1;
    }
    let duration_ok = offset.zip(ours.fight_end).map(|(o, end)| end - o) == expected_duration;
    if duration_ok {
        s.duration_ok += 1;
    }
    // the target list: species ids of the real targets, in order (negative ids are volatile species)
    let expected_targets: Vec<i64> = reference
        .get("targets")
        .and_then(Value::as_array)
        .map(|t| {
            t.iter()
                .filter(|x| x.get("isFake").and_then(Value::as_bool) != Some(true))
                .filter_map(|x| x.get("id").and_then(Value::as_i64))
                .filter(|id| *id > 0)
                .collect()
        })
        .unwrap_or_default();
    let our_targets: Vec<i64> = ours.targets.iter().map(|t| i64::from(t.species)).collect();
    if our_targets == expected_targets {
        s.targets_ok += 1;
    } else if s.target_examples.len() < 3 {
        s.target_examples.push(format!(
            "{label}: rust {our_targets:?} reference {expected_targets:?}"
        ));
    }
    if (!offset_ok || !duration_ok) && s.examples.len() < 3 {
        s.examples.push(format!(
            "{label}: offset rust {offset:?} reference {expected_offset}; duration rust {:?} reference {expected_duration:?}",
            offset.zip(ours.fight_end).map(|(o, end)| end - o)
        ));
    }
    Ok(())
}
