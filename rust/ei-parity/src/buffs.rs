//! Compares the buff stack graphs of the Rust simulation with the `states` of the C# JSON.

use std::collections::BTreeMap;

use ei_core::api::Api;
use ei_core::pipeline::analyze_buffs;
use serde_json::Value;

/// Counts per buff id.
#[derive(Default)]
pub struct BuffStats {
    pub compared: usize,
    pub mismatches: usize,
    pub examples: Vec<String>,
}

/// Merges equal consecutive values and drops zero length segments.
fn normalize(states: &[(i64, i64)]) -> Vec<(i64, i64)> {
    let mut out: Vec<(i64, i64)> = Vec::new();
    for &(time, value) in states {
        while out.last().is_some_and(|l| l.0 == time) {
            out.pop();
        }
        if out.last().is_some_and(|l| l.1 == value) {
            continue;
        }
        out.push((time, value));
    }
    // a graph that is zero from the start is the same as an empty graph
    if out.len() == 1 && out[0].1 == 0 {
        out.clear();
    }
    out
}

fn reference_states(buff: &Value) -> Vec<(i64, i64)> {
    let list = buff
        .get("states")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|p| {
                    let p = p.as_array()?;
                    Some((p.first()?.as_i64()?, p.get(1)?.as_i64()?))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    normalize(&list)
}

/// Compares all players of one log; returns the number of buff graphs compared and the mismatches
/// are recorded per buff id in `stats`.
#[allow(clippy::too_many_lines)]
pub fn compare_log(
    data: &[u8],
    api: &Api,
    reference: &Value,
    label: &str,
    stats: &mut BTreeMap<i64, BuffStats>,
    extra: &mut BTreeMap<i64, BuffStats>,
) -> Result<(), String> {
    let offset = reference
        .get("logStartOffset")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let duration = reference.get("durationMS").and_then(Value::as_i64);
    let analysis = analyze_buffs(data, api, offset, duration).map_err(|e| e.to_string())?;
    let players = reference
        .get("players")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for ours in &analysis.players {
        let Some(theirs) = players.iter().find(|p| {
            p.get("account").and_then(Value::as_str) == Some(ours.player.account.as_str())
                || p.get("account").and_then(Value::as_str)
                    == Some(format!(":{}", ours.player.account).as_str())
        }) else {
            continue;
        };
        let ref_buffs = theirs
            .get("buffUptimes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let before: usize = stats.values().map(|s| s.mismatches).sum();
        for rb in &ref_buffs {
            let Some(id) = rb
                .get("id")
                .and_then(Value::as_i64)
                .filter(|&id| id >= 0 || ours.buffs.iter().any(|b| b.buff.id == id))
            else {
                // negative ids are graphs the C# parser derives (number of boons and so on)
                continue;
            };
            let expected = reference_states(rb);
            let got = ours
                .buffs
                .iter()
                .find(|b| b.buff.id == id)
                .map(|b| {
                    let graph: Vec<(i64, i64)> = b
                        .states(analysis.log_start, analysis.log_end)
                        .into_iter()
                        .map(|(t, v)| (t, i64::from(v)))
                        .collect();
                    normalize(&graph)
                })
                .unwrap_or_default();
            let s = stats.entry(id).or_default();
            s.compared += 1;
            if got != expected {
                s.mismatches += 1;
                if s.examples.len() < 2 {
                    let first = got
                        .iter()
                        .zip(&expected)
                        .position(|(a, b)| a != b)
                        .unwrap_or(got.len().min(expected.len()));
                    let from = first.saturating_sub(1);
                    s.examples.push(format!(
                        "{label} {}: from entry {first}: rust {:?} reference {:?}",
                        ours.player.character,
                        &got[from..got.len().min(from + 4)],
                        &expected[from..expected.len().min(from + 4)]
                    ));
                }
            }
        }
        // buffs the Rust simulation has that the reference does not list
        for ours_buff in &ours.buffs {
            let id = ours_buff.buff.id;
            if ref_buffs
                .iter()
                .any(|rb| rb.get("id").and_then(Value::as_i64) == Some(id))
            {
                continue;
            }
            // the C# JSON skips hidden buffs and buffs without any time inside the log
            let present = ours_buff.buff.classification != ei_data::buffs::Classification::Hidden
                && ours_buff.items.iter().any(|i| {
                    i.active_stacks > 0
                        && i.clamped_duration(analysis.log_start, analysis.log_end) > 0
                });
            if !present {
                continue;
            }
            if std::env::var_os("PARITY_EXTRAS").is_some() {
                println!("EXTRA {label} | {} | {id}", ours.player.character);
            }
            let s = extra.entry(id).or_default();
            s.compared += 1;
            s.mismatches += 1;
            if s.examples.len() < 2 {
                s.examples.push(format!(
                    "{label} {}: rust has the buff, the reference has not",
                    ours.player.character
                ));
            }
        }
        if std::env::var_os("PARITY_PLAYERS").is_some() {
            let after: usize = stats.values().map(|s| s.mismatches).sum();
            if after > before {
                println!(
                    "PLAYER {label} {}: {} differ",
                    ours.player.character,
                    after - before
                );
            }
        }
    }
    Ok(())
}
